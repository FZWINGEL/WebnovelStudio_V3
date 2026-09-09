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
use crate::projects::project_chat_output::{
    ChatGroupEffectsOutput, parse_project_assistant_output_with_predecessors_and_chapters,
};
use crate::projects::{context_packets, story_context, workshop};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

const PREVIEW_KIND: &str = "adoptionPreview";
const DECISION_KIND: &str = "adoptionDecision";
const RECEIPT_KIND: &str = "adoptChatPreview";
const MAX_TARGETS: usize = 3;

struct GroupOrigin {
    output_hash: String,
    effects: Option<ChatGroupEffectsOutput>,
    handles: HashMap<String, (Head, String)>,
    draft_keys: HashMap<String, (String, Head, String)>,
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effects: Option<ChatAdoptionEffects>,
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

    let draft_document_ids = targets
        .iter()
        .map(|(_, fields, _, _, _, _, _, _, _)| fields.document_id.clone())
        .collect::<Vec<_>>();
    let origin = read_group_origin(
        &tx,
        &request.access,
        &request.conversation_id,
        &draft_document_ids,
    )?;
    let effects = build_adoption_effects(&tx, &targets, &origin, request.group_effects.as_ref())?;

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
        effects: Some(effects.clone()),
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
            effects: Some(effects),
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
    let committed_relationships = preview
        .effects
        .as_ref()
        .map(|effects| materialize_chat_relationships(effects, &documents))
        .transpose()?
        .unwrap_or_default();
    let workshop_payload_hash = logical_hash(&(&request, &preview.effects))?;
    workshop::append_chat_relationships(
        &tx,
        &request.access.project_id,
        &request.access.operation_namespace,
        &request.operation_id,
        &workshop_payload_hash,
        parse_version(&preview.workshop_version)?,
        &committed_relationships,
    )?;
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
        "effects": preview.effects,
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

type PreparedChatTarget<'a> = (
    &'a ProjectChatDraftRef,
    DraftRefFields,
    AssistantDraft,
    String,
    String,
    String,
    String,
    Option<DocumentRecord>,
    usize,
);

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

fn read_group_origin(
    connection: &Transaction<'_>,
    access: &ProjectAccess,
    conversation_id: &str,
    draft_document_ids: &[String],
) -> CoreResult<GroupOrigin> {
    let mut run_id = None;
    for document_id in draft_document_ids {
        let row: (String, String, String, String) = connection.query_row(
            "SELECT origin_run_id,packet_id,project_id,operation_namespace FROM assistant_drafts
             WHERE document_id=? AND conversation_id=?",
            params![document_id, conversation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if row.2 != access.project_id || row.3 != access.operation_namespace {
            return Err(CoreError::new(
                "WrongProjectSession",
                "A chat adoption draft belongs to another project identity.",
            ));
        }
        if let Some(existing) = run_id.as_deref()
            && existing != row.0
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "Grouped chat adoption drafts must come from one assistant response.",
            ));
        }
        run_id = Some(row.0);
    }
    let run_id = run_id.ok_or_else(|| {
        CoreError::new(
            "InvalidRequest",
            "Grouped chat adoption requires at least one originating run.",
        )
    })?;
    let (output_text, packet_id): (String, String) = connection.query_row(
        "SELECT output_text,packet_id FROM discussion_runs WHERE id=? AND project_id=? AND operation_namespace=?",
        params![&run_id, access.project_id, access.operation_namespace],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let output_hash = sha256_hex(output_text.as_bytes());
    let packet = context_packets::validated_packet_record(connection, &packet_id)?;
    let (frozen, _) =
        story_context::validated_snapshot_record(connection, &packet.receipt.snapshot_id)?;
    let handles = super::materialize::allowed_target_handles(connection, &frozen)?;
    let chapter_handles = super::materialize::allowed_chapter_target_handles(connection, &frozen)?;
    let predecessor_handles = super::materialize::allowed_predecessor_handles(connection, &frozen)?;
    let output = parse_project_assistant_output_with_predecessors_and_chapters(
        &output_text,
        &handles,
        &predecessor_handles,
        &chapter_handles,
    )?;
    let materialization_json: Option<String> = connection
        .query_row(
            "SELECT payload_json FROM conversation_items
         WHERE conversation_id=? AND project_id=? AND operation_namespace=?
           AND kind='materializeChatResult' AND reference_id=?
         ORDER BY sequence DESC LIMIT 1",
            params![
                conversation_id,
                access.project_id,
                access.operation_namespace,
                &run_id
            ],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(materialization_json) = materialization_json {
        let materialization: Value = serde_json::from_str(&materialization_json)?;
        let persisted_effects = materialization
            .get("groupEffects")
            .filter(|value| !value.is_null())
            .cloned()
            .map(serde_json::from_value)
            .transpose()?;
        if persisted_effects != output.group_effects {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "The retained group effects do not match the assistant response.",
            ));
        }
    } else if output.group_effects.is_some() {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "Grouped effects have no retained materialization record.",
        ));
    }

    let mut handle_heads = HashMap::new();
    for source in &frozen.snapshot.sources {
        let revision = read_revision(connection, &source.source.revision_id)?;
        if revision.head.document_id != source.source.document_id
            || revision.head.body_hash != source.source.body_hash
        {
            return Err(CoreError::new(
                "ContextChanged",
                "A grouped chat source revision changed before adoption.",
            ));
        }
        let kind: String = connection.query_row(
            "SELECT kind FROM documents WHERE id=? AND trashed=0",
            [&source.source.document_id],
            |row| row.get(0),
        )?;
        handle_heads.insert(source.handle.clone(), (revision.head, kind));
    }
    let mut draft_keys = HashMap::new();
    for (ordinal, draft) in output.drafts.iter().enumerate() {
        if let Some(document_id) = connection
            .query_row(
                "SELECT document_id FROM assistant_drafts
                 WHERE origin_run_id=? AND output_ordinal=? AND conversation_id=?",
                params![&run_id, ordinal as i64, conversation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            let document =
                read_document_with_role(connection, &document_id, DocumentRole::AssistantDraft)?;
            let kind = document.kind.clone();
            let head = document.head;
            draft_keys.insert(draft.key.clone(), (document_id, head, kind));
        }
    }
    Ok(GroupOrigin {
        output_hash,
        effects: output.group_effects,
        handles: handle_heads,
        draft_keys,
    })
}

fn resolve_effect_ref(
    origin: &GroupOrigin,
    target_ids: &HashSet<String>,
    draft_targets: &HashMap<String, (String, Head, String)>,
    reference: &str,
) -> CoreResult<(String, Head, String)> {
    if origin.draft_keys.contains_key(reference) {
        let Some((draft_document_id, draft_head, kind)) = draft_targets.get(reference) else {
            return Err(CoreError::new(
                "InvalidRequest",
                "A grouped effect references a draft the author did not include.",
            ));
        };
        if !target_ids.contains(draft_document_id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A grouped effect references a draft the author did not include.",
            ));
        }
        return Ok((draft_document_id.clone(), draft_head.clone(), kind.clone()));
    }
    let Some((head, kind)) = origin.handles.get(reference) else {
        return Err(CoreError::new(
            "InvalidRequest",
            "A grouped effect references a source outside the frozen request.",
        ));
    };
    Ok((head.document_id.clone(), head.clone(), kind.clone()))
}

fn build_adoption_effects(
    connection: &Connection,
    targets: &[PreparedChatTarget<'_>],
    origin: &GroupOrigin,
    requested: Option<&ChatGroupEffectsOutput>,
) -> CoreResult<ChatAdoptionEffects> {
    let retained = &origin.effects;
    if let Some(requested) = requested
        && retained.as_ref() != Some(requested)
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The submitted grouped effects do not match the retained assistant response.",
        ));
    }
    if retained.as_ref().is_some_and(|effects| {
        !effects.impacts.is_empty()
            || !effects.supersessions.is_empty()
            || !effects.placements.is_empty()
    }) {
        return Err(CoreError::new(
            "InvalidRequest",
            "Grouped impacts, supersessions, and placements are review-only until their atomic Workshop materialization is available.",
        ));
    }
    let target_ids = targets
        .iter()
        .map(|(_, _, _, _, document_id, _, _, _, _)| document_id.clone())
        .collect::<HashSet<_>>();
    let mut draft_targets = HashMap::new();
    for (_, fields, draft, _, document_id, _, kind, before, _) in targets {
        for (key, (assistant_document_id, _, _)) in &origin.draft_keys {
            if assistant_document_id == &fields.document_id {
                let head = before
                    .as_ref()
                    .map(|record| record.head.clone())
                    .unwrap_or_else(|| Head {
                        document_id: document_id.clone(),
                        version: "0".into(),
                        body_hash: draft.document.head.body_hash.clone(),
                    });
                draft_targets.insert(key.clone(), (document_id.clone(), head, kind.clone()));
            }
        }
    }
    let relationship_dependencies =
        workshop::chat_relationship_dependencies(connection, &target_ids)?
            .into_iter()
            .map(|relationship| ChatRelationshipDependency {
                relationship_id: relationship.id,
                from_document_id: relationship.from_document_id,
                to_document_id: relationship.to_document_id,
                relationship_type: relationship.relationship_type,
                from_head: relationship.source_heads[0].clone(),
                to_head: relationship.source_heads[1].clone(),
            })
            .collect::<Vec<_>>();
    let mut protected_content = Vec::new();
    for (_, _, _, _, document_id, _, _, before, _) in targets {
        let Some(before) = before else { continue };
        for text in workshop::chat_protected_text(connection, document_id)? {
            protected_content.push(ChatProtectedContent {
                target_document_id: document_id.clone(),
                source_head: before.head.clone(),
                text_hash: sha256_hex(text.as_bytes()),
                text,
            });
        }
    }
    let mut proposed_relationships = Vec::new();
    let mut relationship_ids = HashMap::new();
    if let Some(effects) = retained {
        for relationship in &effects.relationships {
            let (from_document_id, from_head, from_kind) =
                resolve_effect_ref(origin, &target_ids, &draft_targets, &relationship.from_ref)?;
            let (to_document_id, to_head, to_kind) =
                resolve_effect_ref(origin, &target_ids, &draft_targets, &relationship.to_ref)?;
            if !["character", "world"].contains(&from_kind.as_str())
                || !["character", "world"].contains(&to_kind.as_str())
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Chat relationships may only connect character or world material.",
                ));
            }
            let relationship_id = new_id();
            relationship_ids.insert(relationship.key.clone(), relationship_id.clone());
            proposed_relationships.push(ChatAdoptionRelationship {
                key: relationship.key.clone(),
                relationship_id,
                from_document_id,
                to_document_id,
                relationship_type: relationship.relationship_type.clone(),
                description: relationship.description.clone(),
                uncertainty: relationship.uncertainty.clone(),
                from_head,
                to_head,
            });
        }
        let resolve_target =
            |reference: &str| resolve_effect_ref(origin, &target_ids, &draft_targets, reference);
        let mut impacts = Vec::new();
        for impact in &effects.impacts {
            let (target_document_id, _, _) = resolve_target(&impact.target_ref)?;
            let relationship_id = impact
                .relationship_key
                .as_ref()
                .map(|key| {
                    relationship_ids.get(key).cloned().ok_or_else(|| {
                        CoreError::new(
                            "InvalidRequest",
                            "An impact references an unknown proposed relationship.",
                        )
                    })
                })
                .transpose()?;
            impacts.push(ChatAdoptionImpact {
                target_document_id,
                kind: impact.kind.clone(),
                reason: impact.reason.clone(),
                relationship_id,
                relationship_key: impact.relationship_key.clone(),
            });
        }
        let supersessions = effects
            .supersessions
            .iter()
            .map(|supersession| {
                let (target_document_id, _, _) = resolve_target(&supersession.target_ref)?;
                let (superseded_document_id, _, _) = resolve_target(&supersession.superseded_ref)?;
                Ok(ChatAdoptionSupersession {
                    target_document_id,
                    superseded_document_id,
                    reason: supersession.reason.clone(),
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        let placements = effects
            .placements
            .iter()
            .map(|placement| {
                let (target_document_id, _, _) = resolve_target(&placement.target_ref)?;
                let before_document_id = placement
                    .before_ref
                    .as_deref()
                    .map(|reference| resolve_target(reference).map(|(id, _, _)| id))
                    .transpose()?;
                let after_document_id = placement
                    .after_ref
                    .as_deref()
                    .map(|reference| resolve_target(reference).map(|(id, _, _)| id))
                    .transpose()?;
                Ok(ChatAdoptionPlacement {
                    target_document_id,
                    before_document_id,
                    after_document_id,
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(ChatAdoptionEffects {
            version: CHAT_ADOPTION_EFFECTS_VERSION.into(),
            source_output_hash: origin.output_hash.clone(),
            relationship_dependencies,
            protected_content,
            proposed_relationships,
            impacts,
            supersessions,
            placements,
        })
    } else {
        Ok(ChatAdoptionEffects {
            version: CHAT_ADOPTION_EFFECTS_VERSION.into(),
            source_output_hash: origin.output_hash.clone(),
            relationship_dependencies,
            protected_content,
            proposed_relationships,
            impacts: Vec::new(),
            supersessions: Vec::new(),
            placements: Vec::new(),
        })
    }
}

fn materialize_chat_relationships(
    effects: &ChatAdoptionEffects,
    documents: &[DocumentRecord],
) -> CoreResult<Vec<crate::projects::workshop::WorkshopRelationship>> {
    let heads = documents
        .iter()
        .map(|document| (document.head.document_id.clone(), document.head.clone()))
        .collect::<HashMap<_, _>>();
    effects
        .proposed_relationships
        .iter()
        .map(|relationship| {
            let from_head = heads
                .get(&relationship.from_document_id)
                .cloned()
                .unwrap_or_else(|| relationship.from_head.clone());
            let to_head = heads
                .get(&relationship.to_document_id)
                .cloned()
                .unwrap_or_else(|| relationship.to_head.clone());
            Ok(crate::projects::workshop::WorkshopRelationship {
                id: relationship.relationship_id.clone(),
                from_document_id: relationship.from_document_id.clone(),
                to_document_id: relationship.to_document_id.clone(),
                relationship_type: relationship.relationship_type.clone(),
                description: relationship.description.clone(),
                uncertainty: relationship.uncertainty.clone(),
                status: crate::projects::workshop::WorkshopRelationshipStatus::Chosen,
                source_heads: vec![from_head, to_head],
            })
        })
        .collect()
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
        effects: metadata.effects,
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
        effects: stored.preview.effects,
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
    validate_effects_shape(connection, &preview)?;
    Ok(())
}

fn preview_digest(preview: &ChatAdoptionPreview) -> CoreResult<String> {
    let mut value = serde_json::to_value(preview)?;
    value["digest"] = Value::String(String::new());
    logical_hash(&value)
}

/// Validate the immutable shape of a grouped manifest while reading a
/// backup. This intentionally does not apply current-head drift rules: a
/// historical preview remains readable after later author edits, but malformed
/// or silently unbound effect references must never enter the recovered view.
pub(super) fn validate_effects_shape(
    connection: &Connection,
    preview: &ChatAdoptionPreview,
) -> CoreResult<()> {
    let Some(effects) = preview.effects.as_ref() else {
        return Ok(());
    };
    if effects.version != CHAT_ADOPTION_EFFECTS_VERSION
        || !valid_hash(&effects.source_output_hash)
        || !effects.impacts.is_empty()
        || !effects.supersessions.is_empty()
        || !effects.placements.is_empty()
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A chat adoption effect manifest has an unsupported or incomplete effect set.",
        ));
    }
    let target_ids = preview
        .targets
        .iter()
        .map(|target| target.document_id.as_str())
        .collect::<HashSet<_>>();
    let mut relationship_ids = HashSet::new();
    for dependency in &effects.relationship_dependencies {
        check_id(&dependency.relationship_id)?;
        check_id(&dependency.from_document_id)?;
        check_id(&dependency.to_document_id)?;
        if dependency.from_document_id == dependency.to_document_id
            || dependency.from_head.document_id != dependency.from_document_id
            || dependency.to_head.document_id != dependency.to_document_id
            || !valid_hash(&dependency.from_head.body_hash)
            || !valid_hash(&dependency.to_head.body_hash)
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A relationship dependency has invalid endpoint provenance.",
            ));
        }
        parse_version(&dependency.from_head.version)?;
        parse_version(&dependency.to_head.version)?;
    }
    for protected in &effects.protected_content {
        if !target_ids.contains(protected.target_document_id.as_str())
            || protected.source_head.document_id != protected.target_document_id
            || !valid_hash(&protected.source_head.body_hash)
            || sha256_hex(protected.text.as_bytes()) != protected.text_hash
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "Protected chat content is not bound to an adoption target.",
            ));
        }
        parse_version(&protected.source_head.version)?;
    }
    for relationship in &effects.proposed_relationships {
        check_id(&relationship.relationship_id)?;
        if !relationship_ids.insert(relationship.relationship_id.as_str())
            || relationship.from_document_id == relationship.to_document_id
            || relationship.from_head.document_id != relationship.from_document_id
            || relationship.to_head.document_id != relationship.to_document_id
            || !valid_hash(&relationship.from_head.body_hash)
            || !valid_hash(&relationship.to_head.body_hash)
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A proposed relationship has invalid identity or provenance.",
            ));
        }
        parse_version(&relationship.from_head.version)?;
        parse_version(&relationship.to_head.version)?;
        for (document_id, head) in [
            (&relationship.from_document_id, &relationship.from_head),
            (&relationship.to_document_id, &relationship.to_head),
        ] {
            if target_ids.contains(document_id.as_str()) {
                continue;
            }
            let document = read_document(connection, document_id)?;
            if document.role != DocumentRole::Ordinary
                || !["character", "world"].contains(&document.kind.as_str())
                || document.head != *head
            {
                return Err(CoreError::new(
                    "InvalidProjectChat",
                    "A proposed relationship endpoint is outside the frozen ordinary material.",
                ));
            }
        }
    }
    Ok(())
}

fn body_text(value: &Value) -> String {
    value["body"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|block| {
            block["content"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|node| {
                    if node["type"] == "hardBreak" {
                        "\n"
                    } else {
                        node["text"].as_str().unwrap_or_default()
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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
    validate_effects_current(connection, preview)?;
    Ok(())
}

fn validate_effects_current(
    connection: &Connection,
    preview: &ChatAdoptionPreview,
) -> CoreResult<()> {
    let Some(effects) = preview.effects.as_ref() else {
        // Historical previews predate the grouped manifest. Their original
        // digest and body-only adoption semantics remain unchanged.
        return Ok(());
    };
    if effects.version != CHAT_ADOPTION_EFFECTS_VERSION || !valid_hash(&effects.source_output_hash)
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The chat adoption effect manifest has invalid version or provenance.",
        ));
    }
    let target_ids = preview
        .targets
        .iter()
        .map(|target| target.document_id.clone())
        .collect::<HashSet<_>>();
    let dependencies = workshop::chat_relationship_dependencies(connection, &target_ids)?;
    for expected in &effects.relationship_dependencies {
        let Some(actual) = dependencies
            .iter()
            .find(|relationship| relationship.id == expected.relationship_id)
        else {
            return Err(CoreError::new(
                "WorkshopChanged",
                "A relationship in this chat preview no longer exists.",
            ));
        };
        if actual.from_document_id != expected.from_document_id
            || actual.to_document_id != expected.to_document_id
            || actual.relationship_type != expected.relationship_type
            || actual.source_heads.first() != Some(&expected.from_head)
            || actual.source_heads.get(1) != Some(&expected.to_head)
        {
            return Err(CoreError::new(
                "WorkshopChanged",
                "A relationship dependency changed after this chat preview.",
            ));
        }
        for head in [&expected.from_head, &expected.to_head] {
            let current = read_document(connection, &head.document_id)?;
            let target_is_changed = target_ids.contains(&head.document_id);
            if !target_is_changed && current.head != *head {
                return Err(CoreError::new(
                    "VersionConflict",
                    "A relationship endpoint changed after this chat preview.",
                ));
            }
        }
    }
    for protected in &effects.protected_content {
        let current = read_document(connection, &protected.target_document_id)?;
        if current.head != protected.source_head
            || sha256_hex(protected.text.as_bytes()) != protected.text_hash
            || !workshop::chat_protected_text(connection, &protected.target_document_id)?
                .contains(&protected.text)
        {
            return Err(CoreError::new(
                "ProtectedContentChanged",
                "Protected chat content changed after this preview.",
            ));
        }
        if !body_text(&current.body).contains(&protected.text) {
            return Err(CoreError::new(
                "ProtectedContentChanged",
                "The adoption result removed protected chat content.",
            ));
        }
    }
    for relationship in &effects.proposed_relationships {
        for head in [&relationship.from_head, &relationship.to_head] {
            if target_ids.contains(&head.document_id) {
                let target = preview
                    .targets
                    .iter()
                    .find(|target| target.document_id == head.document_id)
                    .and_then(|target| target.before.as_ref())
                    .map(|before| &before.head);
                if target.is_some_and(|current| current != head) {
                    return Err(CoreError::new(
                        "DraftChanged",
                        "A proposed relationship target changed after preview preparation.",
                    ));
                }
            } else if read_document(connection, &head.document_id)?.head != *head {
                return Err(CoreError::new(
                    "VersionConflict",
                    "A proposed relationship endpoint changed after preview preparation.",
                ));
            }
        }
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
    let mut conflicting_receipt = false;
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
            conflicting_receipt = true;
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
        if conflicting_receipt {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID was already used for another command.",
            ));
        }
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
            effects: None,
        };
        let digest = preview_digest(&preview).expect("digest");
        let mut with_digest = preview.clone();
        with_digest.digest = "other".into();
        assert_eq!(digest, preview_digest(&with_digest).expect("digest"));
    }
}
