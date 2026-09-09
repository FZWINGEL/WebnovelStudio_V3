//! Review and adoption of project-chat material.
//!
//! A chat response is never a story write.  The response is first materialised
//! as an isolated assistant draft; this module turns an explicitly reviewed,
//! exact draft into an immutable preview and, later, into one atomic Working
//! adoption.  The preview item stores references only.  Bodies are recovered
//! from immutable revisions so the conversation ledger cannot become a second
//! document store.

use super::*;
use crate::projects::material_adoption::{self, MaterialTarget};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

const PREVIEW_KIND: &str = "adoptionPreview";
const DECISION_KIND: &str = "adoptionDecision";
const RECEIPT_KIND: &str = "adoptChatPreview";
const MAX_TARGETS: usize = 3;

pub(super) fn read_preview(
    project: &OwnedProject,
    access: &ProjectAccess,
    conversation_id: &str,
    preview_id: &str,
) -> CoreResult<ChatAdoptionPreview> {
    project.check_access(access)?;
    check_id(preview_id)?;
    let db = project.db()?;
    store::require_conversation(db, access, conversation_id)?;
    let item = store::read_item_by_reference(db, conversation_id, PREVIEW_KIND, preview_id)?;
    let stored: StoredPreview = serde_json::from_value(item.payload)?;
    if stored.preview.id != preview_id || stored.preview.conversation_id != conversation_id {
        return Err(CoreError::new(
            "PreviewMismatch",
            "The saved preview belongs to another review.",
        ));
    }
    // Reading an old review never refreshes its source proof or authorizes Apply.
    expand_preview(db, access, stored.preview)
}

/// The durable payload for a preview is intentionally ref-only.  `before`
/// contains metadata and its checkpoint identity, while `body_revision_id`
/// points at the immutable draft revision that supplies the proposed body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredPreview {
    schema_version: String,
    preview: StoredPreviewMetadata,
    request_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredPreviewMetadata {
    id: String,
    version: String,
    digest: String,
    project_id: String,
    operation_namespace: String,
    conversation_id: String,
    source_epoch: String,
    policy_epoch: String,
    workshop_version: String,
    targets: Vec<StoredPreviewTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredPreviewTarget {
    draft: Value,
    draft_revision_id: String,
    draft_document_id: String,
    disposition_version: String,
    document_id: String,
    title: String,
    kind: String,
    before: Option<StoredDocumentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredDocumentRef {
    head: Head,
    title: String,
    kind: String,
    metadata_version: String,
    last_checkpoint_id: Option<String>,
    role: DocumentRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredAdoptionReceipt {
    preview_id: String,
    documents: Vec<StoredDocumentRef>,
    decision_id: String,
}

pub(super) fn prepare(
    project: &mut OwnedProject,
    request: PrepareChatAdoption,
) -> CoreResult<ChatAdoptionPreview> {
    project.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    check_id(&request.conversation_id)?;
    if request.drafts.is_empty() || request.drafts.len() > MAX_TARGETS {
        return Err(CoreError::new(
            "InvalidRequest",
            "Chat adoption requires one to three nonchapter drafts.",
        ));
    }

    let payload_hash = logical_hash(&request)?;
    let connection = project.db_mut()?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // A request operation is idempotent.  Keep the original immutable preview
    // and reconstruct its bodies from revisions rather than replaying writes.
    if let Some(existing) = operation_item(
        &tx,
        &request.access,
        &request.conversation_id,
        &request.operation_id,
    )? {
        if existing.kind != PREVIEW_KIND {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID already belongs to another project-chat command.",
            ));
        }
        let stored: StoredPreview = serde_json::from_value(existing.payload)?;
        if stored.request_hash != payload_hash {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID was already used for another chat adoption preview.",
            ));
        }
        let preview = expand_preview(&tx, &request.access, stored.preview)?;
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(preview);
    }

    let anchor = store::require_conversation(&tx, &request.access, &request.conversation_id)?;
    if anchor.role != DocumentRole::ConversationAnchor {
        return Err(CoreError::new(
            "DocumentRoleMismatch",
            "The project conversation anchor is not available.",
        ));
    }
    let (source_epoch, policy_epoch) = store::epochs(&tx)?;
    let workshop_version = current_workshop_version(&tx)?;
    let mut targets = Vec::with_capacity(request.drafts.len());
    let mut target_ids = HashSet::new();
    let mut draft_ids = HashSet::new();

    for (ordinal, draft_ref) in request.drafts.iter().enumerate() {
        let fields = draft_ref_fields(draft_ref)?;
        let draft_document_id = fields.document_id.clone();
        if !draft_ids.insert(draft_document_id.clone()) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Chat adoption drafts must be unique.",
            ));
        }
        let mut draft = store::read_draft(
            &tx,
            &request.access,
            &request.conversation_id,
            &draft_document_id,
        )?;
        if draft.stale {
            return Err(CoreError::new(
                "ContextChanged",
                "The assistant draft was produced against an older story or policy epoch.",
            ));
        }
        if draft.disposition != "pending" {
            return Err(CoreError::new(
                "DraftNotAdoptable",
                "Only a pending assistant draft can be prepared for adoption.",
            ));
        }
        require_exact_draft_ref(&fields, &draft)?;
        if draft.document.role != DocumentRole::AssistantDraft {
            return Err(CoreError::new(
                "DocumentRoleMismatch",
                "Chat adoption requires an isolated assistant draft.",
            ));
        }
        if draft.document.kind == "chapter"
            || !["note", "world", "character", "theme", "hook", "scene"]
                .contains(&draft.document.kind.as_str())
        {
            return Err(CoreError::new(
                "InvalidDocument",
                "Chat adoption can target only nonchapter material documents.",
            ));
        }
        // Autosave advances the working draft without forcing a checkpoint
        // on every keystroke. Preparing review captures that exact saved head.
        let revision = checkpoint_at(&tx, &draft.document, "chatAdoptionPreviewDraft")?;
        let draft_revision_id = revision.id.clone();
        draft.document.last_checkpoint_id = Some(draft_revision_id.clone());
        // Ensure the checkpoint is valid before exposing a review preview.
        let revision = read_revision(&tx, &draft_revision_id)?;
        if revision.head.document_id != draft_document_id || revision.head != draft.document.head {
            return Err(CoreError::new(
                "DraftChanged",
                "The assistant draft revision no longer matches its current head.",
            ));
        }

        let (document_id, title, kind, before) = if let Some(expected) = draft.target.clone() {
            if !target_ids.insert(expected.document_id.clone()) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Chat adoption targets must be unique.",
                ));
            }
            let current = read_document(&tx, &expected.document_id)?;
            require_head(&current.head, &expected)?;
            if current.role != DocumentRole::Ordinary {
                return Err(CoreError::new(
                    "DocumentRoleMismatch",
                    "An adoption target must be an ordinary story document.",
                ));
            }
            // A pre-chat document may have no checkpoint yet.  Creating this
            // immutable before reference does not alter its head or source
            // epoch and avoids duplicating its body in the preview ledger.
            checkpoint_at(&tx, &current, "chatAdoptionPreviewBefore")?;
            let current = read_document(&tx, &expected.document_id)?;
            (
                current.head.document_id.clone(),
                current.title.clone(),
                current.kind.clone(),
                Some(current),
            )
        } else {
            let reserved = new_id();
            if !target_ids.insert(reserved.clone()) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Chat adoption targets must be unique.",
                ));
            }
            (
                reserved,
                draft.document.title.clone(),
                draft.document.kind.clone(),
                None,
            )
        };

        targets.push((
            draft_ref,
            fields,
            draft,
            draft_revision_id,
            document_id,
            title,
            kind,
            before,
            ordinal,
        ));
    }

    let material_targets = targets
        .iter()
        .map(
            |(_, _, draft, _, document_id, title, kind, _, _)| MaterialTarget {
                document_id: document_id.clone(),
                title: title.clone(),
                kind: kind.clone(),
                body: draft.document.body.clone(),
                expected: draft.target.clone(),
            },
        )
        .collect::<Vec<_>>();
    workshop::validate_chat_material_targets(&tx, &material_targets)?;

    let preview_id = new_id();
    let mut expanded_targets = Vec::with_capacity(targets.len());
    let mut stored_targets = Vec::with_capacity(targets.len());
    for (draft_ref, fields, draft, draft_revision_id, document_id, title, kind, before, _) in
        targets
    {
        expanded_targets.push(ChatAdoptionTarget {
            draft: draft_ref.clone(),
            draft_revision_id: draft_revision_id.clone(),
            document_id: document_id.clone(),
            title: title.clone(),
            kind: kind.clone(),
            before: before.clone(),
            body: draft.document.body.clone(),
        });
        stored_targets.push(StoredPreviewTarget {
            draft: serde_json::to_value(draft_ref)?,
            draft_revision_id,
            draft_document_id: fields.document_id,
            disposition_version: fields.disposition_version,
            document_id,
            title,
            kind,
            before: before.map(stored_document_ref),
        });
    }
    let metadata_without_digest = ChatAdoptionPreview {
        id: preview_id,
        version: "1".into(),
        digest: String::new(),
        project_id: request.access.project_id.clone(),
        operation_namespace: request.access.operation_namespace.clone(),
        conversation_id: request.conversation_id.clone(),
        source_epoch,
        policy_epoch,
        workshop_version,
        targets: expanded_targets,
    };
    let digest = preview_digest(&metadata_without_digest)?;
    let mut preview = metadata_without_digest;
    preview.digest = digest.clone();
    let stored = StoredPreview {
        schema_version: "chat-adoption-preview.v1".into(),
        preview: StoredPreviewMetadata {
            id: preview.id.clone(),
            version: preview.version.clone(),
            digest,
            project_id: preview.project_id.clone(),
            operation_namespace: preview.operation_namespace.clone(),
            conversation_id: preview.conversation_id.clone(),
            source_epoch: preview.source_epoch.clone(),
            policy_epoch: preview.policy_epoch.clone(),
            workshop_version: preview.workshop_version.clone(),
            targets: stored_targets,
        },
        request_hash: payload_hash,
    };
    let payload = serde_json::to_value(&stored)?;
    store::append_item(
        &tx,
        &request.access,
        &request.conversation_id,
        Some(&request.operation_id),
        PREVIEW_KIND,
        Some(&preview.id),
        &payload,
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(preview)
}

pub(super) fn adopt(
    project: &mut OwnedProject,
    request: AdoptChatPreview,
) -> CoreResult<ChatAdoptionAck> {
    project.check_access(&request.access)?;
    check_id(&request.operation_id)?;
    check_id(&request.conversation_id)?;
    check_id(&request.preview_id)?;
    parse_version(&request.preview_version)?;
    if !valid_hash(&request.preview_digest) {
        return Err(CoreError::new(
            "InvalidRequest",
            "The preview digest is invalid.",
        ));
    }
    let payload_hash = logical_hash(&request)?;
    let connection = project.db_mut()?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

    if let Some(existing) =
        existing_receipt(&tx, &request.access, &request.operation_id, &payload_hash)?
    {
        let stored: StoredAdoptionReceipt = serde_json::from_str(&existing)?;
        let documents = stored
            .documents
            .iter()
            .cloned()
            .map(|reference| read_historical_document_ref(&tx, reference))
            .collect::<CoreResult<Vec<_>>>()?;
        let ack = ChatAdoptionAck {
            preview_id: stored.preview_id,
            documents,
            decision_id: stored.decision_id,
        };
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(ack);
    }

    let item = store::read_item_by_reference(
        &tx,
        &request.conversation_id,
        PREVIEW_KIND,
        &request.preview_id,
    )?;
    let stored: StoredPreview = serde_json::from_value(item.payload)?;
    if stored.preview.id != request.preview_id
        || stored.preview.version != request.preview_version
        || stored.preview.digest != request.preview_digest
    {
        return Err(CoreError::new(
            "PreviewMismatch",
            "The selected chat adoption preview identity does not match.",
        ));
    }
    let preview = expand_preview(&tx, &request.access, stored.preview)?;
    validate_preview_current(&tx, &request, &preview)?;
    let targets = preview
        .targets
        .iter()
        .map(|target| MaterialTarget {
            document_id: target.document_id.clone(),
            title: target.title.clone(),
            kind: target.kind.clone(),
            body: target.body.clone(),
            expected: target.before.as_ref().map(|before| before.head.clone()),
        })
        .collect::<Vec<_>>();
    workshop::validate_chat_material_targets(&tx, &targets)?;

    let mut documents = Vec::with_capacity(targets.len());
    for target in &targets {
        documents.push(material_adoption::write_material_target_at(
            &tx,
            target,
            "beforeChatAdoption",
            "chatAdoption",
        )?);
    }
    if !documents.is_empty() {
        tx.execute(
            "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
            [],
        )?;
    }
    let decision_id = new_id();
    for target in &preview.targets {
        let changed = tx.execute(
            "UPDATE assistant_drafts SET disposition='adopted',disposition_version=disposition_version+1 WHERE document_id=? AND conversation_id=? AND disposition='pending' AND disposition_version=?",
            params![
                target.draft_document_id(),
                request.conversation_id,
                draft_disposition_version(&target.draft)?,
            ],
        )?;
        if changed != 1 {
            return Err(CoreError::new(
                "DraftChanged",
                "An assistant draft changed while its preview was being adopted.",
            ));
        }
    }
    let decision_payload = json!({
        "schemaVersion": "chat-adoption-decision.v1",
        "previewId": preview.id,
        "previewVersion": preview.version,
        "previewDigest": preview.digest,
        "documentIds": documents.iter().map(|document| document.head.document_id.clone()).collect::<Vec<_>>(),
        "decisionId": decision_id,
    });
    store::append_item(
        &tx,
        &request.access,
        &request.conversation_id,
        Some(&request.operation_id),
        DECISION_KIND,
        Some(&decision_id),
        &decision_payload,
    )?;
    let receipt = StoredAdoptionReceipt {
        preview_id: preview.id.clone(),
        documents: documents.iter().cloned().map(stored_document_ref).collect(),
        decision_id: decision_id.clone(),
    };
    tx.execute(
        "INSERT INTO command_receipts(operation_namespace,operation_id,document_id,payload_hash,operation_kind,result_json) VALUES(?,?,?,?,?,?)",
        params![
            request.access.operation_namespace,
            request.operation_id,
            documents.first().map(|document| document.head.document_id.as_str()),
            payload_hash,
            RECEIPT_KIND,
            serde_json::to_string(&receipt)?,
        ],
    )?;
    let ack = ChatAdoptionAck {
        preview_id: preview.id,
        documents,
        decision_id,
    };
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(ack)
}

// ---- ref-only persistence and validation ---------------------------------

#[derive(Debug, Clone)]
struct DraftRefFields {
    document_id: String,
    head: Head,
    disposition_version: String,
}

fn draft_ref_fields(reference: &impl Serialize) -> CoreResult<DraftRefFields> {
    let value = serde_json::to_value(reference)?;
    let head: Head = value
        .get("head")
        .cloned()
        .ok_or_else(|| {
            CoreError::new(
                "InvalidRequest",
                "A chat draft reference has no exact head.",
            )
        })
        .and_then(|head| serde_json::from_value(head).map_err(CoreError::from))?;
    let document_id = head.document_id.clone();
    let disposition_version = value
        .get("dispositionVersion")
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .or_else(|| value.as_i64().map(|number| number.to_string()))
        })
        .ok_or_else(|| {
            CoreError::new(
                "InvalidRequest",
                "A chat draft reference has no disposition version.",
            )
        })?;
    check_id(&document_id)?;
    parse_version(&head.version)?;
    if !valid_hash(&head.body_hash) {
        return Err(CoreError::new(
            "InvalidRequest",
            "A chat draft reference has an invalid body hash.",
        ));
    }
    parse_version(&disposition_version)?;
    Ok(DraftRefFields {
        document_id: document_id.to_owned(),
        head,
        disposition_version,
    })
}

fn require_exact_draft_ref(fields: &DraftRefFields, draft: &AssistantDraft) -> CoreResult<()> {
    if draft.document.head != fields.head || draft.disposition_version != fields.disposition_version
    {
        return Err(CoreError::new(
            "DraftChanged",
            "The chat draft changed after the request reference was captured.",
        ));
    }
    Ok(())
}

fn stored_document_ref(document: DocumentRecord) -> StoredDocumentRef {
    StoredDocumentRef {
        head: document.head,
        title: document.title,
        kind: document.kind,
        metadata_version: document.metadata_version,
        last_checkpoint_id: document.last_checkpoint_id,
        role: document.role,
    }
}

fn operation_item(
    connection: &Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    operation_id: &str,
) -> CoreResult<Option<ConversationItem>> {
    let row: Option<(String, i64, String, Option<String>, String, String)> = connection
        .query_row(
            "SELECT id,sequence,kind,reference_id,payload_json,created_at FROM conversation_items WHERE conversation_id=? AND project_id=? AND operation_namespace=? AND operation_id=?",
            params![conversation_id, access.project_id, access.operation_namespace, operation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?;
    row.map(|(id, sequence, kind, reference_id, payload, created_at)| {
        Ok(ConversationItem {
            id,
            sequence: sequence.to_string(),
            kind,
            reference_id,
            payload: serde_json::from_str(&payload)?,
            created_at,
        })
    })
    .transpose()
}

fn current_workshop_version(connection: &Connection) -> CoreResult<String> {
    let version: Option<i64> = connection
        .query_row(
            "SELECT version FROM workshop_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    parse_stored_version(version.unwrap_or(0))
}

fn expand_preview(
    connection: &Connection,
    access: &ProjectAccess,
    metadata: StoredPreviewMetadata,
) -> CoreResult<ChatAdoptionPreview> {
    if metadata.project_id != access.project_id
        || metadata.operation_namespace != access.operation_namespace
    {
        return Err(CoreError::new(
            "WrongProjectSession",
            "This chat adoption preview belongs to another project identity.",
        ));
    }
    let mut targets = Vec::with_capacity(metadata.targets.len());
    for target in metadata.targets {
        let draft: ProjectChatDraftRef = serde_json::from_value(target.draft.clone())?;
        read_document_with_role(
            connection,
            &target.draft_document_id,
            DocumentRole::AssistantDraft,
        )?;
        let revision = read_revision(connection, &target.draft_revision_id)?;
        if revision.head != draft.head || revision.head.document_id != target.draft_document_id {
            return Err(CoreError::new(
                "InvalidProject",
                "A preview references another assistant draft revision.",
            ));
        }
        let body = revision.body;
        let before = target
            .before
            .map(|stored| read_historical_document_ref(connection, stored))
            .transpose()?;
        targets.push(ChatAdoptionTarget {
            draft,
            draft_revision_id: target.draft_revision_id,
            document_id: target.document_id,
            title: target.title,
            kind: target.kind,
            before,
            body,
        });
    }
    let preview = ChatAdoptionPreview {
        id: metadata.id,
        version: metadata.version,
        digest: metadata.digest,
        project_id: metadata.project_id,
        operation_namespace: metadata.operation_namespace,
        conversation_id: metadata.conversation_id,
        source_epoch: metadata.source_epoch,
        policy_epoch: metadata.policy_epoch,
        workshop_version: metadata.workshop_version,
        targets,
    };
    let expected = preview_digest(&ChatAdoptionPreview {
        digest: String::new(),
        ..preview.clone()
    })?;
    if expected != preview.digest {
        return Err(CoreError::new(
            "InvalidProject",
            "The chat adoption preview failed its digest check.",
        ));
    }
    Ok(preview)
}

fn expand_document_ref(
    connection: &Connection,
    reference: StoredDocumentRef,
) -> CoreResult<DocumentRecord> {
    let current = read_document(connection, &reference.head.document_id)?;
    if current.head != reference.head
        || current.title != reference.title
        || current.kind != reference.kind
        || current.metadata_version != reference.metadata_version
        || current.last_checkpoint_id != reference.last_checkpoint_id
        || current.role != reference.role
    {
        return Err(CoreError::new(
            "VersionConflict",
            "A chat adoption preview target changed before adoption.",
        ));
    }
    Ok(current)
}

fn read_historical_document_ref(
    connection: &Connection,
    reference: StoredDocumentRef,
) -> CoreResult<DocumentRecord> {
    let checkpoint = reference.last_checkpoint_id.clone().ok_or_else(|| {
        CoreError::new(
            "InvalidProject",
            "A chat adoption receipt has no committed document revision.",
        )
    })?;
    let revision = read_revision(connection, &checkpoint)?;
    if revision.head != reference.head || revision.head.document_id != reference.head.document_id {
        return Err(CoreError::new(
            "InvalidProject",
            "A chat adoption receipt has an invalid document revision reference.",
        ));
    }
    Ok(DocumentRecord {
        head: reference.head,
        title: reference.title,
        kind: reference.kind,
        metadata_version: reference.metadata_version,
        body: revision.body,
        last_checkpoint_id: Some(checkpoint),
        role: reference.role,
    })
}

/// Validate a stored ref-only preview while transferring a database.
///
/// This deliberately rehydrates bodies from the immutable revision named by
/// the preview.  It does not compare the stored `before` reference with the
/// document's current working head: a later author edit makes a preview stale
/// for adoption, but it must not make an otherwise valid historical backup
/// unreadable.
pub(super) fn validate_backup_preview(
    connection: &Connection,
    project_id: &str,
    operation_namespace: &str,
    conversation_id: &str,
    reference_id: &str,
    payload: &Value,
) -> CoreResult<()> {
    check_id(project_id)?;
    check_id(operation_namespace)?;
    check_id(conversation_id)?;
    check_id(reference_id)?;
    let stored: StoredPreview = serde_json::from_value(payload.clone()).map_err(|error| {
        CoreError::new(
            "InvalidProjectChat",
            &format!("The stored adoption preview is invalid: {error}"),
        )
    })?;
    if stored.schema_version != "chat-adoption-preview.v1"
        || stored.preview.id != reference_id
        || stored.preview.project_id != project_id
        || stored.preview.operation_namespace != operation_namespace
        || stored.preview.conversation_id != conversation_id
        || stored.preview.targets.is_empty()
        || stored.preview.targets.len() > MAX_TARGETS
        || !valid_hash(&stored.request_hash)
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The stored adoption preview has invalid identity or bounds.",
        ));
    }
    parse_version(&stored.preview.version)?;
    if !valid_hash(&stored.preview.digest) {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The stored adoption preview has invalid version metadata.",
        ));
    }
    parse_version(&stored.preview.source_epoch)?;
    parse_version(&stored.preview.policy_epoch)?;
    parse_version(&stored.preview.workshop_version)?;
    let conversation_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM project_conversations
         WHERE id=? AND project_id=? AND operation_namespace=?)",
        rusqlite::params![conversation_id, project_id, operation_namespace],
        |row| row.get(0),
    )?;
    if !conversation_exists {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The adoption preview has no matching project conversation.",
        ));
    }

    let mut target_ids = HashSet::new();
    let mut targets = Vec::with_capacity(stored.preview.targets.len());
    for target in stored.preview.targets {
        let draft: ProjectChatDraftRef = serde_json::from_value(target.draft.clone())?;
        let draft_fields = draft_ref_fields(&draft)?;
        if draft_fields.document_id != target.draft_document_id
            || draft_fields.disposition_version != target.disposition_version
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "The adoption preview draft reference is inconsistent.",
            ));
        }
        let role: String = connection.query_row(
            "SELECT role FROM documents WHERE id=?",
            [&target.draft_document_id],
            |row| row.get(0),
        )?;
        if role != DocumentRole::AssistantDraft.storage_name() {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "An adoption preview does not point to an assistant draft.",
            ));
        }
        let draft_revision = read_revision(connection, &target.draft_revision_id)?;
        if draft_revision.head.document_id != target.draft_document_id
            || draft_revision.head != draft_fields.head
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "An adoption preview draft revision does not match its frozen head.",
            ));
        }
        if !target_ids.insert(target.document_id.clone()) {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "An adoption preview contains duplicate targets.",
            ));
        }
        check_id(&target.document_id)?;
        let before = if let Some(before) = target.before {
            if before.role != DocumentRole::Ordinary
                || before.head.document_id != target.document_id
            {
                return Err(CoreError::new(
                    "InvalidProjectChat",
                    "An adoption preview before reference is not ordinary story material.",
                ));
            }
            Some(read_historical_document_ref(connection, before)?)
        } else {
            let existing_role: Option<String> = connection
                .query_row(
                    "SELECT role FROM documents WHERE id=?",
                    [&target.document_id],
                    |row| row.get(0),
                )
                .optional()?;
            if existing_role.is_some_and(|role| role != DocumentRole::Ordinary.storage_name()) {
                return Err(CoreError::new(
                    "InvalidProjectChat",
                    "An adoption preview reserves an isolated document identity.",
                ));
            }
            None
        };
        targets.push(ChatAdoptionTarget {
            draft,
            draft_revision_id: target.draft_revision_id,
            document_id: target.document_id,
            title: target.title,
            kind: target.kind,
            before,
            body: draft_revision.body,
        });
    }
    let preview = ChatAdoptionPreview {
        id: stored.preview.id,
        version: stored.preview.version,
        digest: stored.preview.digest,
        project_id: stored.preview.project_id,
        operation_namespace: stored.preview.operation_namespace,
        conversation_id: stored.preview.conversation_id,
        source_epoch: stored.preview.source_epoch,
        policy_epoch: stored.preview.policy_epoch,
        workshop_version: stored.preview.workshop_version,
        targets,
    };
    let expected = preview_digest(&ChatAdoptionPreview {
        digest: String::new(),
        ..preview.clone()
    })?;
    if expected != preview.digest {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The adoption preview digest does not match its immutable references.",
        ));
    }
    Ok(())
}

fn preview_digest(preview: &ChatAdoptionPreview) -> CoreResult<String> {
    let mut value = serde_json::to_value(preview)?;
    value["digest"] = Value::String(String::new());
    logical_hash(&value)
}

fn validate_preview_current(
    connection: &Connection,
    request: &AdoptChatPreview,
    preview: &ChatAdoptionPreview,
) -> CoreResult<()> {
    if preview.project_id != request.access.project_id
        || preview.operation_namespace != request.access.operation_namespace
        || preview.conversation_id != request.conversation_id
    {
        return Err(CoreError::new(
            "WrongProjectSession",
            "The chat adoption preview belongs to another project identity.",
        ));
    }
    let (source_epoch, policy_epoch) = store::epochs(connection)?;
    if source_epoch != preview.source_epoch || policy_epoch != preview.policy_epoch {
        return Err(CoreError::new(
            "ContextChanged",
            "The story or permissions changed after this chat preview.",
        ));
    }
    if current_workshop_version(connection)? != preview.workshop_version {
        return Err(CoreError::new(
            "WorkshopChanged",
            "Workshop protection or relationships changed after this chat preview.",
        ));
    }
    let mut target_ids = HashSet::new();
    for target in &preview.targets {
        if let Some(before) = &target.before {
            expand_document_ref(connection, stored_document_ref(before.clone()))?;
        }
        if !target_ids.insert(&target.document_id) {
            return Err(CoreError::new(
                "InvalidProject",
                "A chat adoption preview contains duplicate targets.",
            ));
        }
        let draft = store::read_draft(
            connection,
            &request.access,
            &request.conversation_id,
            &target.draft_document_id(),
        )?;
        if draft.stale {
            return Err(CoreError::new(
                "ContextChanged",
                "The assistant draft was produced against an older story or policy epoch.",
            ));
        }
        if draft.disposition != "pending" {
            return Err(CoreError::new(
                "DraftNotAdoptable",
                "A chat draft is no longer pending adoption.",
            ));
        }
        if draft.document.last_checkpoint_id.as_deref() != Some(target.draft_revision_id.as_str()) {
            return Err(CoreError::new(
                "DraftChanged",
                "A chat draft revision changed after preview preparation.",
            ));
        }
        let revision = read_revision(connection, &target.draft_revision_id)?;
        if revision.body != target.body {
            return Err(CoreError::new(
                "DraftChanged",
                "A chat draft body changed after preview preparation.",
            ));
        }
        let fields = draft_ref_fields(&target.draft)?;
        require_exact_draft_ref(&fields, &draft)?;
    }
    Ok(())
}

fn draft_disposition_version(reference: &ProjectChatDraftRef) -> CoreResult<String> {
    draft_ref_fields(reference).map(|fields| fields.disposition_version)
}

fn existing_receipt(
    connection: &Connection,
    access: &ProjectAccess,
    operation_id: &str,
    payload_hash: &str,
) -> CoreResult<Option<String>> {
    for (table, kind_column) in [
        ("workshop_receipts", "operation_kind"),
        ("proposal_receipts", "kind"),
    ] {
        let found: Option<String> = connection
            .query_row(
                &format!("SELECT {kind_column} FROM {table} WHERE operation_namespace=? AND operation_id=?"),
                params![access.operation_namespace, operation_id],
                |row| row.get(0),
            )
            .optional()?;
        if found.is_some() {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID was already used for another command.",
            ));
        }
    }
    let found: Option<(String, String)> = connection
        .query_row(
            "SELECT operation_kind,payload_hash,result_json FROM command_receipts WHERE operation_namespace=? AND operation_id=?",
            params![access.operation_namespace, operation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((kind, hash)) = found else {
        return Ok(None);
    };
    if kind != RECEIPT_KIND || hash != payload_hash {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This operation ID was already used for another command.",
        ));
    }
    connection
        .query_row(
            "SELECT result_json FROM command_receipts WHERE operation_namespace=? AND operation_id=?",
            params![access.operation_namespace, operation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(CoreError::from)
}

// These methods intentionally keep field extraction at the boundary.  The
// parent module can evolve ProjectChatDraftRef without making persisted
// previews depend on a second copy of its body.
trait ChatTargetExt {
    fn draft_document_id(&self) -> String;
}
impl ChatTargetExt for ChatAdoptionTarget {
    fn draft_document_id(&self) -> String {
        draft_ref_fields(&self.draft)
            .map(|fields| fields.document_id)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_digest_excludes_digest_field() {
        let preview = ChatAdoptionPreview {
            id: "preview".into(),
            version: "1".into(),
            digest: String::new(),
            project_id: "project".into(),
            operation_namespace: "namespace".into(),
            conversation_id: "conversation".into(),
            source_epoch: "0".into(),
            policy_epoch: "0".into(),
            workshop_version: "0".into(),
            targets: vec![],
        };
        let digest = preview_digest(&preview).expect("digest");
        let mut with_digest = preview.clone();
        with_digest.digest = "other".into();
        assert_eq!(digest, preview_digest(&with_digest).expect("digest"));
    }
}
