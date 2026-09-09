//! Role-scoped persistence for isolated project-chat drafts.
//!
//! Generic editor save/checkpoint/reconciliation paths intentionally remain
//! ordinary-document-only.  These commands are the narrow bridge used by the
//! draft editor and retain the same immutable revision and receipt rules.

use super::*;
use crate::projects::context_packets;
use crate::projects::project_chat::materialize::{
    allowed_chapter_target_handles, allowed_predecessor_handles, allowed_target_handles,
};
use crate::projects::project_chat_output::{
    MAX_PROJECT_CHAT_KEY_BYTES, parse_project_assistant_output_with_predecessors_and_chapters,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

const SAVE_DRAFT_KIND: &str = "saveAssistantDraft";

fn validate_rationale(value: &str) -> CoreResult<()> {
    if value.len() > 8 * 1024 || value.chars().any(char::is_control) {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "A disposition rationale must be at most 8 KiB without control characters.",
        ));
    }
    Ok(())
}

fn validate_response_key(value: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > MAX_PROJECT_CHAT_KEY_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(CoreError::new(
            "InvalidDisposition",
            "A response key is invalid.",
        ));
    }
    Ok(())
}

fn validate_reference_id(value: &str) -> CoreResult<()> {
    if let Some((run_id, key)) = value.split_once(':') {
        check_id(run_id)?;
        validate_response_key(key)?;
    } else {
        check_id(value)?;
    }
    Ok(())
}

// Explicit scope/producer identity fields are validated together at acceptance.
#[allow(clippy::too_many_arguments)]
fn validate_response_scope(
    tx: &rusqlite::Transaction<'_>,
    access: &ProjectAccess,
    conversation_id: &str,
    producer_run_id: &str,
    item_kind: &str,
    disposition: &str,
    scope: &crate::projects::project_chat::ChatDispositionScope,
    unknown_to: Option<crate::projects::project_chat::ChatUnknownTo>,
) -> CoreResult<()> {
    scope.validate_shape()?;
    match scope.kind {
        crate::projects::project_chat::ChatDispositionScopeKind::Project => {}
        crate::projects::project_chat::ChatDispositionScopeKind::Task => {
            if scope.reference_id.as_deref() != Some(producer_run_id) {
                return Err(CoreError::new(
                    "InvalidDisposition",
                    "A task disposition scope must reference its producing run.",
                ));
            }
        }
        crate::projects::project_chat::ChatDispositionScopeKind::Chapter
        | crate::projects::project_chat::ChatDispositionScopeKind::Document => {
            let document_id = scope.reference_id.as_deref().expect("validated scope");
            let document = read_document_with_role(tx, document_id, DocumentRole::Ordinary)
                .map_err(|_| {
                    CoreError::new(
                        "InvalidDisposition",
                        "A disposition scope references a missing or ineligible document.",
                    )
                })?;
            if scope.kind == crate::projects::project_chat::ChatDispositionScopeKind::Chapter
                && document.kind != "chapter"
            {
                return Err(CoreError::new(
                    "InvalidDisposition",
                    "A disposition scope references an ineligible document.",
                ));
            }
        }
    }
    if item_kind == "assumption"
        && (!matches!(
            scope.kind,
            crate::projects::project_chat::ChatDispositionScopeKind::Project
        ) || unknown_to.is_some())
    {
        return Err(CoreError::new(
            "InvalidDisposition",
            "Assumption dispositions are project-scoped and cannot carry unknownTo.",
        ));
    }
    if unknown_to.is_some() && disposition != "keepMysterious" {
        return Err(CoreError::new(
            "InvalidDisposition",
            "unknownTo is only valid for keepMysterious questions.",
        ));
    }
    let linked: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversation_items WHERE conversation_id=? AND project_id=? AND operation_namespace=? AND kind='request' AND reference_id=?)",
        params![conversation_id, access.project_id, access.operation_namespace, producer_run_id],
        |row| row.get(0),
    )?;
    if !linked {
        return Err(CoreError::new(
            "InvalidDisposition",
            "A disposition scope producer is outside this conversation.",
        ));
    }
    Ok(())
}

fn draft_row(
    tx: &rusqlite::Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    document_id: &str,
) -> CoreResult<(String, i64)> {
    let row: Option<(String, i64)> = tx
        .query_row(
            "SELECT disposition,disposition_version FROM assistant_drafts
             WHERE document_id=? AND conversation_id=? AND project_id=? AND operation_namespace=?",
            params![
                document_id,
                conversation_id,
                access.project_id,
                access.operation_namespace
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.ok_or_else(|| {
        CoreError::new(
            "AssistantDraftNotFound",
            "This draft is outside the current project conversation.",
        )
    })
}

fn operation_collision(
    tx: &rusqlite::Transaction<'_>,
    access: &ProjectAccess,
    operation_id: &str,
) -> CoreResult<()> {
    let used: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM discussion_runs WHERE operation_namespace=? AND operation_id=?)",
        params![access.operation_namespace, operation_id],
        |r| r.get(0),
    )?;
    if used {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This operation ID already identifies a generation request.",
        ));
    }
    Ok(())
}

fn save_ack(access: &ProjectAccess, request: &SaveAssistantDraft, result: StoredResult) -> SaveAck {
    SaveAck {
        project_id: access.project_id.clone(),
        document_id: result.head.document_id.clone(),
        session: access.session.clone(),
        operation_namespace: access.operation_namespace.clone(),
        operation_id: request.snapshot.operation_id.clone(),
        head: result.head,
        saved_generation: result.saved_generation,
    }
}

fn draft_save_payload_hash(request: &SaveAssistantDraft) -> CoreResult<String> {
    let mut value = serde_json::to_value(request)?;
    if let Some(access) = value
        .get_mut("snapshot")
        .and_then(Value::as_object_mut)
        .and_then(|snapshot| snapshot.get_mut("access"))
        .and_then(Value::as_object_mut)
    {
        access.remove("session");
        access.remove("writerLease");
    }
    Ok(sha256_hex(
        serde_json::to_string(&crate::canonicalize_value(value))?.as_bytes(),
    ))
}

impl OwnedProject {
    pub(super) fn save_assistant_draft(
        &mut self,
        request: SaveAssistantDraft,
    ) -> CoreResult<SaveAck> {
        let access = request.snapshot.access.clone();
        self.check_access(&access)?;
        check_id(&request.snapshot.operation_id)?;
        check_id(&request.snapshot.expected.document_id)?;
        parse_version(&request.snapshot.expected.version)?;
        parse_version(&request.snapshot.local_generation)?;
        parse_version(&request.disposition_version)?;
        let payload = draft_save_payload_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let _anchor = store::require_conversation(&tx, &access, &request.conversation_id)?;
        let (disposition, disposition_version) = draft_row(
            &tx,
            &access,
            &request.conversation_id,
            &request.snapshot.expected.document_id,
        )?;
        if disposition != "pending"
            || disposition_version.to_string() != request.disposition_version
        {
            return Err(CoreError::new(
                "DraftChanged",
                "The draft is no longer pending at the requested disposition version.",
            ));
        }
        if let Some(result) = existing_receipt(
            &tx,
            &access.operation_namespace,
            &request.snapshot.operation_id,
            SAVE_DRAFT_KIND,
            &payload,
        )? {
            let ack = save_ack(&access, &request, result);
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(ack);
        }
        operation_collision(&tx, &access, &request.snapshot.operation_id)?;
        let before = read_document_with_role(
            &tx,
            &request.snapshot.expected.document_id,
            DocumentRole::AssistantDraft,
        )?;
        require_head(&before.head, &request.snapshot.expected)?;
        let validated = validate_snapshot_json(&serde_json::to_string(&request.snapshot.body)?)
            .map_err(|message| CoreError::new("InvalidDocument", &message))?;
        let mut head = before.head.clone();
        if validated.hash != head.body_hash {
            if request.snapshot.cause != SaveCause::Typing {
                checkpoint_at(&tx, &before, "beforeAssistantDraftUndoRedo")?;
            }
            let next = parse_version(&head.version)?
                .checked_add(1)
                .ok_or_else(|| {
                    CoreError::new("VersionLimit", "The draft version limit was reached.")
                })?;
            let changed = tx.execute(
                "UPDATE documents SET working_version=?,body_json=?,body_hash=?,projection_dirty=1
                 WHERE id=? AND role='assistantDraft' AND working_version=? AND body_hash=?",
                params![
                    next,
                    validated.canonical_json,
                    validated.hash,
                    head.document_id,
                    parse_version(&head.version)?,
                    head.body_hash
                ],
            )?;
            if changed != 1 {
                return Err(CoreError::new(
                    "VersionConflict",
                    "The draft changed before saving.",
                ));
            }
            head.version = next.to_string();
            head.body_hash = validated.hash;
            if request.snapshot.cause != SaveCause::Typing {
                checkpoint_at(
                    &tx,
                    &read_document_with_role(&tx, &head.document_id, DocumentRole::AssistantDraft)?,
                    "afterAssistantDraftUndoRedo",
                )?;
            }
        }
        let stored = StoredResult {
            head,
            saved_generation: request.snapshot.local_generation.clone(),
            applied: None,
            restored: None,
        };
        let ack = save_ack(&access, &request, stored.clone());
        store::append_item(
            &tx,
            &access,
            &request.conversation_id,
            Some(&request.snapshot.operation_id),
            SAVE_DRAFT_KIND,
            Some(&request.snapshot.expected.document_id),
            &serde_json::to_value(&ack)?,
        )?;
        insert_receipt(
            &tx,
            &access.operation_namespace,
            &request.snapshot.operation_id,
            SAVE_DRAFT_KIND,
            &payload,
            &stored,
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(ack)
    }

    pub(super) fn checkpoint_assistant_draft(
        &mut self,
        conversation_id: String,
        request: CheckpointRequest,
    ) -> CoreResult<Revision> {
        self.check_access(&request.access)?;
        check_id(&conversation_id)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        store::require_conversation(&tx, &request.access, &conversation_id)?;
        let _ = draft_row(
            &tx,
            &request.access,
            &conversation_id,
            &request.expected.document_id,
        )?;
        let document = read_document_with_role(
            &tx,
            &request.expected.document_id,
            DocumentRole::AssistantDraft,
        )?;
        require_head(&document.head, &request.expected)?;
        let reason = serde_json::to_value(request.reason)?;
        let revision = checkpoint_at(&tx, &document, reason.as_str().unwrap_or("manual"))?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(revision)
    }

    pub(super) fn reconcile_assistant_draft(
        &mut self,
        conversation_id: String,
        request: ReconcileRequest,
    ) -> CoreResult<ReconciledDocument> {
        if request.project_id != self.info.project_id
            || request.operation_namespace != self.info.operation_namespace
        {
            return Err(CoreError::new(
                "WrongProjectSession",
                "Reconciliation belongs to another project.",
            ));
        }
        check_id(&conversation_id)?;
        check_id(&request.session)?;
        check_id(&request.document_id)?;
        if request.pending_operation_ids.len() > 64 {
            return Err(CoreError::new(
                "InvalidRequest",
                "Too many pending operations.",
            ));
        }
        for id in &request.pending_operation_ids {
            check_id(id)?;
        }
        self.recover_connection()?;
        let access = self.attach(request.session)?;
        let tx = self.db()?;
        store::require_conversation(tx, &access, &conversation_id)?;
        let _ = draft_row(tx, &access, &conversation_id, &request.document_id)?;
        let document =
            read_document_with_role(tx, &request.document_id, DocumentRole::AssistantDraft)?;
        let mut receipts = Vec::new();
        for id in request.pending_operation_ids {
            let row: Option<(String, String, String)> = tx
                .query_row(
                    "SELECT operation_kind,payload_hash,result_json FROM command_receipts
                     WHERE operation_namespace=? AND operation_id=? AND document_id=?",
                    params![self.info.operation_namespace, id, request.document_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            if let Some((operation_kind, payload_hash, result)) = row {
                receipts.push(OperationReceipt {
                    operation_id: id,
                    operation_kind,
                    payload_hash,
                    result: serde_json::from_str(&result)?,
                });
            }
        }
        Ok(ReconciledDocument {
            access,
            document,
            receipts,
        })
    }

    pub(super) fn set_chat_disposition(
        &mut self,
        request: SetChatDisposition,
    ) -> CoreResult<ConversationItem> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        check_id(&request.conversation_id)?;
        validate_reference_id(&request.reference_id)?;
        validate_rationale(&request.rationale)?;
        let payload = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let anchor = store::require_conversation(&tx, &request.access, &request.conversation_id)?;
        if let Some(existing) = store::replay_local::<ConversationItem>(
            &tx,
            &request.access,
            &request.conversation_id,
            &request.operation_id,
            "chatDisposition",
            &payload,
        )? {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(existing);
        }
        operation_collision(&tx, &request.access, &request.operation_id)?;
        let scope = request.scope.clone().unwrap_or_default();
        let next_version = if !request.reference_id.contains(':') {
            let (_current, version) = draft_row(
                &tx,
                &request.access,
                &request.conversation_id,
                &request.reference_id,
            )?;
            if version.to_string() != request.expected_version {
                return Err(CoreError::new(
                    "VersionConflict",
                    "The draft disposition changed.",
                ));
            }
            if !matches!(request.disposition.as_str(), "rejected" | "reconsider") {
                return Err(CoreError::new(
                    "InvalidDisposition",
                    "Drafts support rejected or reconsider.",
                ));
            }
            if !matches!(
                scope.kind,
                crate::projects::project_chat::ChatDispositionScopeKind::Project
            ) || request.unknown_to.is_some()
            {
                return Err(CoreError::new(
                    "InvalidDisposition",
                    "Draft dispositions are project-scoped and cannot carry unknownTo.",
                ));
            }
            scope.validate_shape()?;
            let next = version
                .checked_add(1)
                .ok_or_else(|| CoreError::new("VersionLimit", "Disposition version exhausted."))?;
            let status = if request.disposition == "reconsider" {
                "superseded"
            } else {
                "rejected"
            };
            tx.execute(
                "UPDATE assistant_drafts SET disposition=?,disposition_version=? WHERE document_id=? AND disposition_version=?",
                params![status, next, request.reference_id, version],
            )?;
            next
        } else {
            let (run_id, key) = request.reference_id.split_once(':').ok_or_else(|| {
                CoreError::new(
                    "InvalidDisposition",
                    "A response reference must include a run and key.",
                )
            })?;
            check_id(run_id)?;
            validate_response_key(key)?;
            let owner_ok: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversation_items WHERE conversation_id=? AND kind='request' AND reference_id=? AND project_id=? AND operation_namespace=?)",
                params![request.conversation_id, run_id, request.access.project_id, request.access.operation_namespace],
                |r| r.get(0),
            )?;
            if !owner_ok {
                return Err(CoreError::new(
                    "ChatReferenceNotFound",
                    "The response reference is outside this conversation.",
                ));
            }
            let output: String = tx.query_row("SELECT output_text FROM discussion_runs WHERE id=? AND project_id=? AND operation_namespace=?", params![run_id,request.access.project_id,request.access.operation_namespace], |r|r.get(0))?;
            let packet_id: String = tx.query_row(
                "SELECT packet_id FROM discussion_runs WHERE id=?",
                [run_id],
                |r| r.get(0),
            )?;
            let packet = context_packets::validated_packet_record(&tx, &packet_id)?;
            let (frozen, _) = crate::projects::story_context::validated_snapshot_record(
                &tx,
                &packet.receipt.snapshot_id,
            )?;
            let handles = allowed_target_handles(&tx, &frozen)?;
            let predecessor_handles = allowed_predecessor_handles(&tx, &frozen)?;
            let chapter_handles = allowed_chapter_target_handles(&tx, &frozen)?;
            let parsed = parse_project_assistant_output_with_predecessors_and_chapters(
                &output,
                &handles,
                &predecessor_handles,
                &chapter_handles,
            )?;
            let is_question = parsed.questions.iter().any(|value| value.key == key);
            let is_assumption = parsed.assumptions.iter().any(|value| value.key == key);
            if !is_question && !is_assumption {
                return Err(CoreError::new(
                    "ChatReferenceNotFound",
                    "The response key is not present in the originating output.",
                ));
            }
            let valid = if is_assumption {
                matches!(
                    request.disposition.as_str(),
                    "assumptionReject" | "reconsider"
                )
            } else {
                matches!(
                    request.disposition.as_str(),
                    "notNow" | "notRelevant" | "keepMysterious" | "reconsider"
                )
            };
            if !valid {
                return Err(CoreError::new(
                    "InvalidDisposition",
                    "The disposition is not valid for this response item.",
                ));
            }
            validate_response_scope(
                &tx,
                &request.access,
                &request.conversation_id,
                run_id,
                if is_question {
                    "question"
                } else {
                    "assumption"
                },
                &request.disposition,
                &scope,
                request.unknown_to,
            )?;
            let current: i64 = tx.query_row("SELECT COUNT(*) FROM conversation_items WHERE conversation_id=? AND kind='chatDisposition' AND reference_id=?", params![request.conversation_id,request.reference_id], |r|r.get(0))?;
            if current.to_string() != request.expected_version {
                return Err(CoreError::new(
                    "VersionConflict",
                    "The response disposition changed.",
                ));
            }
            current
                .checked_add(1)
                .ok_or_else(|| CoreError::new("VersionLimit", "Disposition version exhausted."))?
        };
        let event_payload = json!({
            "referenceId": request.reference_id,
            "disposition": request.disposition,
            "expectedVersion": request.expected_version,
            "version": next_version.to_string(),
            "rationale": request.rationale,
            "scope": scope,
            "unknownTo": request.unknown_to,
        });
        let item = store::append_item(
            &tx,
            &request.access,
            &request.conversation_id,
            Some(&request.operation_id),
            "chatDisposition",
            Some(&request.reference_id),
            &event_payload,
        )?;
        insert_receipt(
            &tx,
            &request.access.operation_namespace,
            &request.operation_id,
            "chatDisposition",
            &payload,
            &StoredResult {
                head: anchor.head,
                saved_generation: next_version.to_string(),
                applied: None,
                restored: None,
            },
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(item)
    }
}
