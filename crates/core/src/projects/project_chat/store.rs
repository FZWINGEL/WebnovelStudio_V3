use super::super::discussions::{self, FeedbackIntent, StartDiscussion};
use super::super::project_chat_context::ProjectChatFreeze;
use super::*;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::collections::HashSet;

pub(super) fn epochs(db: &Connection) -> CoreResult<(String, String)> {
    db.query_row(
        "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |r| {
            Ok((
                r.get::<_, i64>(0)?.to_string(),
                r.get::<_, i64>(1)?.to_string(),
            ))
        },
    )
    .map_err(CoreError::from)
}

pub(super) fn require_conversation(
    db: &Connection,
    access: &ProjectAccess,
    id: &str,
) -> CoreResult<DocumentRecord> {
    let anchor: String = db.query_row(
        "SELECT anchor_document_id FROM project_conversations WHERE id=? AND project_id=? AND operation_namespace=?",
        params![id,access.project_id,access.operation_namespace], |r| r.get(0)).optional()?
        .ok_or_else(|| CoreError::new("WrongProjectConversation", "This conversation belongs to another project identity."))?;
    let record = read_document_with_role(db, &anchor, DocumentRole::ConversationAnchor)?;
    let blocks = record.body["body"]["content"].as_array();
    if record.kind != "note"
        || blocks.is_none_or(|blocks| {
            blocks.len() != 1
                || blocks[0]["type"] != "paragraph"
                || blocks[0]
                    .get("content")
                    .and_then(Value::as_array)
                    .is_some_and(|c| !c.is_empty())
        })
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The conversation control anchor must remain blank.",
        ));
    }
    Ok(record)
}

fn ensure_conversation(db: &Connection, access: &ProjectAccess) -> CoreResult<String> {
    if let Some(id) = db
        .query_row(
            "SELECT id FROM project_conversations WHERE project_id=? AND operation_namespace=?",
            params![access.project_id, access.operation_namespace],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        require_conversation(db, access, &id)?;
        return Ok(id);
    }
    let id = new_id();
    let anchor = new_id();
    let body = json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":new_id()}}]}});
    let validated = validate_snapshot_json(&serde_json::to_string(&body)?)
        .map_err(|m| CoreError::new("InvalidDocument", &m))?;
    db.execute("INSERT INTO documents(id,kind,title,position,schema_version,body_json,body_hash,role) VALUES(?,'note','Project conversation',-1,1,?,?,'conversationAnchor')",
        params![anchor,validated.canonical_json,validated.hash])?;
    db.execute("INSERT INTO project_conversations(id,project_id,operation_namespace,anchor_document_id,composer_json) VALUES(?,?,?,?,?)",
        params![id,access.project_id,access.operation_namespace,anchor,serde_json::to_string(&ProjectComposer::default())?])?;
    let document = require_conversation(db, access, &id)?;
    checkpoint_at(db, &document, "conversationAnchor")?;
    Ok(id)
}

pub(super) fn read_composer(db: &Connection, id: &str) -> CoreResult<ProjectComposerSnapshot> {
    let (version, body): (i64, String) = db.query_row(
        "SELECT composer_version,composer_json FROM project_conversations WHERE id=?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(ProjectComposerSnapshot {
        conversation_id: id.into(),
        version: version.to_string(),
        body: serde_json::from_str(&body)?,
    })
}

pub(super) fn append_item(
    db: &Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    operation_id: Option<&str>,
    kind: &str,
    reference_id: Option<&str>,
    payload: &Value,
) -> CoreResult<ConversationItem> {
    let id = new_id();
    let payload_json = serde_json::to_string(&crate::canonicalize_value(payload.clone()))?;
    db.execute("INSERT INTO conversation_items(id,conversation_id,project_id,operation_namespace,sequence,operation_id,kind,reference_id,payload_json,payload_hash) VALUES(?,?,?,?,(SELECT COALESCE(MAX(sequence),0)+1 FROM conversation_items WHERE conversation_id=?),?,?,?,?,?)",
        params![id,conversation_id,access.project_id,access.operation_namespace,conversation_id,operation_id,kind,reference_id,payload_json,sha256_hex(payload_json.as_bytes())])?;
    read_item(db, &id)
}

fn read_item(db: &Connection, id: &str) -> CoreResult<ConversationItem> {
    let row:(i64,String,Option<String>,String,String,String) = db.query_row("SELECT sequence,kind,reference_id,payload_json,payload_hash,created_at FROM conversation_items WHERE id=?", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    if sha256_hex(row.3.as_bytes()) != row.4 {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "Conversation event fingerprint changed.",
        ));
    }
    Ok(ConversationItem {
        id: id.into(),
        sequence: row.0.to_string(),
        kind: row.1,
        reference_id: row.2,
        payload: serde_json::from_str(&row.3)?,
        created_at: row.5,
    })
}

pub(super) fn read_item_by_reference(
    db: &Connection,
    conversation_id: &str,
    kind: &str,
    reference: &str,
) -> CoreResult<ConversationItem> {
    let id:String = db.query_row("SELECT id FROM conversation_items WHERE conversation_id=? AND kind=? AND reference_id=? ORDER BY sequence DESC LIMIT 1", params![conversation_id,kind,reference], |r| r.get(0)).optional()?
        .ok_or_else(|| CoreError::new("ChatItemNotFound", "The conversation reference was not found."))?;
    read_item(db, &id)
}

pub(super) fn replay_local<T: DeserializeOwned>(
    db: &Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    operation_id: &str,
    kind: &str,
    payload_hash: &str,
) -> CoreResult<Option<T>> {
    if existing_receipt(
        db,
        &access.operation_namespace,
        operation_id,
        kind,
        payload_hash,
    )?
    .is_none()
    {
        return Ok(None);
    }
    let id: String = db.query_row(
        "SELECT id FROM conversation_items WHERE conversation_id=? AND operation_id=? AND kind=?",
        params![conversation_id, operation_id, kind],
        |r| r.get(0),
    )?;
    Ok(Some(serde_json::from_value(read_item(db, &id)?.payload)?))
}

// Each argument is one field of the existing command receipt, kept explicit here.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_local<T: Serialize>(
    db: &Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    operation_id: &str,
    kind: &str,
    payload_hash: &str,
    anchor: &DocumentRecord,
    result: &T,
) -> CoreResult<()> {
    reject_run_operation(db, access, operation_id)?;
    append_item(
        db,
        access,
        conversation_id,
        Some(operation_id),
        kind,
        None,
        &serde_json::to_value(result)?,
    )?;
    insert_receipt(
        db,
        &access.operation_namespace,
        operation_id,
        kind,
        payload_hash,
        &StoredResult {
            head: anchor.head.clone(),
            saved_generation: "0".into(),
            applied: None,
            restored: None,
        },
    )
}

fn reject_run_operation(db: &Connection, access: &ProjectAccess, id: &str) -> CoreResult<()> {
    let used:bool = db.query_row("SELECT EXISTS(SELECT 1 FROM discussion_runs WHERE operation_namespace=? AND operation_id=?)", params![access.operation_namespace,id], |r| r.get(0))?;
    if used {
        return Err(CoreError::new(
            "OperationIdReusedWithDifferentPayload",
            "This operation ID already identifies a generation request.",
        ));
    }
    Ok(())
}

pub(super) fn find_run(
    db: &Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    operation_id: &str,
) -> CoreResult<Option<DiscussionRun>> {
    require_conversation(db, access, conversation_id)?;
    let run:Option<String> = db.query_row("SELECT r.id FROM discussion_runs r JOIN conversation_items i ON i.reference_id=r.id AND i.kind='request' WHERE i.conversation_id=? AND r.project_id=? AND r.operation_namespace=? AND r.operation_id=?",
        params![conversation_id,access.project_id,access.operation_namespace,operation_id], |r|r.get(0)).optional()?;
    run.map(|id| discussions::read_run(db, &id)).transpose()
}

pub(super) fn find_chapter_run(
    db: &Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    operation_id: &str,
) -> CoreResult<Option<DiscussionRun>> {
    require_conversation(db, access, conversation_id)?;
    let run: Option<String> = db
        .query_row(
            "SELECT r.id FROM discussion_runs r
             JOIN conversation_items i ON i.reference_id=r.id AND i.kind='chapterRequest'
             WHERE i.conversation_id=? AND r.project_id=? AND r.operation_namespace=?
               AND r.operation_id=?",
            params![
                conversation_id,
                access.project_id,
                access.operation_namespace,
                operation_id
            ],
            |r| r.get(0),
        )
        .optional()?;
    run.map(|id| discussions::read_run(db, &id)).transpose()
}

/// Read the validated, non-authoritative range hint from a chapter Discuss
/// run. The run must be an authenticated chapterRequest in the current project
/// namespace. A legacy/plain response simply has no structured projection.
pub(super) fn read_chapter_feedback(
    db: &Connection,
    access: &ProjectAccess,
    run_id: &str,
) -> CoreResult<Option<ChapterDiscussionFeedback>> {
    check_id(run_id)?;
    let belongs: bool = db.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM conversation_items
              WHERE reference_id=? AND kind='chapterRequest'
                AND project_id=? AND operation_namespace=?
         )",
        params![run_id, access.project_id, access.operation_namespace],
        |row| row.get(0),
    )?;
    if !belongs {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "The chapter discussion does not belong to this project identity.",
        ));
    }
    let run = discussions::read_run(db, run_id)?;
    if run.owner.project_id != access.project_id
        || run.owner.operation_namespace != access.operation_namespace
    {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "The chapter discussion does not belong to this project identity.",
        ));
    }
    let request_json: String = db.query_row(
        "SELECT request_json FROM context_packets WHERE id=?",
        [&run.packet_id],
        |row| row.get(0),
    )?;
    let request: crate::projects::context_packets::PrepareContext =
        serde_json::from_str(&request_json).map_err(|error| {
            CoreError::new(
                "InvalidContextPacket",
                &format!("The saved chapter packet request is invalid: {error}"),
            )
        })?;
    if request.response_contract.as_deref()
        != Some(crate::projects::project_chat_output::CHAPTER_DISCUSSION_RESPONSE_CONTRACT)
    {
        return Ok(None);
    }
    let packet = crate::projects::context_packets::validated_packet_record(db, &run.packet_id)?;
    let (frozen, owner_namespace) =
        crate::projects::story_context::validated_snapshot_record(db, &packet.receipt.snapshot_id)?;
    if owner_namespace != access.operation_namespace
        || frozen.snapshot.project_id != access.project_id
        || frozen.snapshot.target.document_id != run.target.document_id
        || frozen.snapshot.target.body_hash != run.target.body_hash
    {
        return Err(CoreError::new(
            "InvalidContextPacket",
            "The chapter packet target does not match its retained run.",
        ));
    }
    let target_handle = frozen
        .snapshot
        .sources
        .iter()
        .find(|source| source.source == frozen.snapshot.target)
        .map(|source| source.handle.clone())
        .ok_or_else(|| {
            CoreError::new(
                "InvalidContextPacket",
                "The chapter packet target is missing from its frozen source manifest.",
            )
        })?;
    let target = crate::projects::story_context::read_source(db, &frozen, &target_handle)?;
    let mut projection =
        match crate::projects::project_chat_output::project_chapter_discussion_output(
            &run.output_text,
            &run.target,
            &target.body,
        ) {
            Ok(projection) => projection,
            Err(_) => return Ok(None),
        };
    if run.status != discussions::DiscussionRunStatus::Completed
        || run.dispatch_state != "delivered"
    {
        projection.range_proposal = None;
        projection.range_error.get_or_insert_with(|| {
            "The chapter response is not a completed, delivered run.".to_owned()
        });
    }
    let (current_source_epoch, current_policy_epoch): (i64, i64) = db.query_row(
        "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if frozen.snapshot.context_source_epoch != current_source_epoch.to_string()
        || frozen.policy.version != current_policy_epoch.to_string()
    {
        projection.range_proposal = None;
        projection.range_error.get_or_insert_with(|| {
            "The story context changed; refresh this chapter feedback before selecting a range."
                .to_owned()
        });
    }
    Ok(Some(ChapterDiscussionFeedback {
        run_id: run.id,
        target: run.target,
        answer: projection.answer,
        range_proposal: projection.range_proposal,
        range_error: projection.range_error,
    }))
}

pub(super) fn is_root_project_chat_run(
    db: &Connection,
    access: &ProjectAccess,
    run_id: &str,
) -> CoreResult<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM conversation_items
             WHERE project_id=? AND operation_namespace=?
               AND reference_id=? AND kind='request'
         )",
        params![access.project_id, access.operation_namespace, run_id],
        |row| row.get(0),
    )?)
}

pub(super) fn read_draft(
    db: &Connection,
    access: &ProjectAccess,
    conversation_id: &str,
    document_id: &str,
) -> CoreResult<AssistantDraft> {
    require_conversation(db, access, conversation_id)?;
    let row:(String,String,String,Option<String>,Option<String>,String,i64,i64,i64) = db.query_row(
        "SELECT origin_run_id,packet_id,initial_revision_id,target_json,predecessor_document_id,disposition,disposition_version,source_epoch,policy_epoch FROM assistant_drafts WHERE document_id=? AND conversation_id=? AND project_id=? AND operation_namespace=?",
        params![document_id,conversation_id,access.project_id,access.operation_namespace], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).optional()?
        .ok_or_else(|| CoreError::new("AssistantDraftNotFound", "This assistant draft is outside the current conversation."))?;
    let document = read_document_with_role(db, document_id, DocumentRole::AssistantDraft)?;
    let current = epochs(db)?;
    let (manifest, hash): (String,String) = db.query_row("SELECT s.manifest_json,s.manifest_hash FROM context_packets p JOIN story_snapshots s ON s.id=p.snapshot_id WHERE p.id=?", [&row.1], |r|Ok((r.get(0)?,r.get(1)?)))?;
    let frozen = super::super::story_context::decode_snapshot(&manifest, &hash)?;
    let task_current =
        super::super::project_chat_context::project_chat_basis_is_current(db, &frozen)?;
    Ok(AssistantDraft {
        document,
        conversation_id: conversation_id.into(),
        origin_run_id: row.0,
        packet_id: row.1,
        initial_revision_id: row.2,
        target: row.3.map(|s| serde_json::from_str(&s)).transpose()?,
        predecessor_document_id: row.4,
        disposition: row.5,
        disposition_version: row.6.to_string(),
        stale: row.7.to_string() != current.0 || row.8.to_string() != current.1 || !task_current,
    })
}

pub(super) fn validate_composer(composer: &ProjectComposer) -> CoreResult<()> {
    if composer.text.len() > 64 * 1024
        || composer.source_refs.len() > 64
        || composer.task_draft_refs.len() > 3
    {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The composer exceeds the request limits.",
        ));
    }
    let mut ids = HashSet::new();
    for head in composer
        .source_refs
        .iter()
        .chain(composer.task_draft_refs.iter().map(|d| &d.head))
    {
        check_id(&head.document_id)?;
        parse_version(&head.version)?;
        if !ids.insert(&head.document_id)
            || head.body_hash.len() != 64
            || !head.body_hash.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A source reference is duplicated or malformed.",
            ));
        }
    }
    Ok(())
}

impl OwnedProject {
    pub(super) fn read_project_conversation(
        &mut self,
        request: ReadProjectConversation,
    ) -> CoreResult<ProjectConversation> {
        self.check_access(&request.access)?;
        if request.limit == 0 || request.limit > 100 {
            return Err(CoreError::new(
                "InvalidRequest",
                "Choose a conversation page size from 1 to 100.",
            ));
        }
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = ensure_conversation(&tx, &request.access)?;
        let before = request
            .before
            .as_deref()
            .map(parse_version)
            .transpose()?
            .unwrap_or(i64::MAX);
        let rows = {
            let mut q=tx.prepare("SELECT id FROM conversation_items WHERE conversation_id=? AND sequence<? AND kind NOT IN ('saveProjectComposer','saveAssistantDraft','adoptChatPreviewReceipt','prepareChatAdoptionReceipt') ORDER BY sequence DESC LIMIT ?")?;
            q.query_map(params![id, before, request.limit as i64 + 1], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        let has_older = rows.len() > request.limit as usize;
        let mut items = rows
            .into_iter()
            .take(request.limit as usize)
            .map(|id| read_item(&tx, &id))
            .collect::<CoreResult<Vec<_>>>()?;
        let older_before = has_older.then(|| {
            items
                .last()
                .expect("nonempty bounded page")
                .sequence
                .clone()
        });
        items.reverse();
        for item in &mut items {
            if (item.kind == "request" || item.kind == "chapterRequest")
                && let Some(run_id) = &item.reference_id
            {
                let run = discussions::read_run(&tx, run_id)?;
                if run.owner.project_id != request.access.project_id
                    || run.owner.operation_namespace != request.access.operation_namespace
                {
                    return Err(CoreError::new(
                        "InvalidProjectChat",
                        "A conversation run has another owner.",
                    ));
                }
                let user: String = tx.query_row(
                    "SELECT content FROM discussion_messages WHERE run_id=? AND role='user'",
                    [run_id],
                    |r| r.get(0),
                )?;
                item.payload["run"] = serde_json::to_value(run)?;
                item.payload["instruction"] = Value::String(user);
                // Keep the authoritative assistant message identity beside
                // the run so an author can explicitly adapt that exact
                // project-chat answer into a restricted chapter brief.
                // The message is optional while a run is still queued.
                let assistant_message: Option<String> = tx
                        .query_row(
                            "SELECT id FROM discussion_messages WHERE run_id=? AND role='assistant' ORDER BY rowid DESC LIMIT 1",
                            [run_id],
                            |r| r.get(0),
                        )
                        .optional()?;
                if let Some(message_id) = assistant_message {
                    item.payload["assistantMessageId"] = Value::String(message_id);
                }
            }
        }
        let draft_ids = {
            // Do not silently hide old unresolved drafts. Timeline messages
            // are paged separately; the review inventory must remain complete.
            let mut q=tx.prepare("SELECT document_id FROM assistant_drafts WHERE conversation_id=? ORDER BY rowid DESC")?;
            q.query_map([&id], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let drafts = draft_ids
            .iter()
            .map(|d| read_draft(&tx, &request.access, &id, d))
            .collect::<CoreResult<Vec<_>>>()?;
        let active:Option<String>=tx.query_row("SELECT r.id FROM discussion_runs r JOIN conversation_items i ON i.reference_id=r.id AND i.kind IN ('request','chapterRequest') WHERE i.conversation_id=? AND r.status IN ('queued','running','stopping') ORDER BY r.rowid DESC LIMIT 1", [&id], |r|r.get(0)).optional()?;
        let active_run = active.map(|r| discussions::read_run(&tx, &r)).transpose()?;
        let (source_epoch, policy_epoch) = epochs(&tx)?;
        let earlier_workshop: bool =
            tx.query_row("SELECT EXISTS(SELECT 1 FROM workshop_state)", [], |r| {
                r.get(0)
            })?;
        let composer = read_composer(&tx, &id)?;
        let document_saves = super::save_recap::read_document_saves(&tx, &request.access, &id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(ProjectConversation {
            id,
            composer,
            items,
            older_before,
            active_run,
            drafts,
            source_epoch,
            policy_epoch,
            earlier_workshop,
            document_saves,
        })
    }

    pub(super) fn save_project_composer(
        &mut self,
        request: SaveProjectComposer,
    ) -> CoreResult<ProjectComposerSnapshot> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        validate_composer(&request.body)?;
        let payload = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let anchor = require_conversation(&tx, &request.access, &request.conversation_id)?;
        if let Some(version) = replay_local::<String>(
            &tx,
            &request.access,
            &request.conversation_id,
            &request.operation_id,
            "saveProjectComposer",
            &payload,
        )? {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(ProjectComposerSnapshot {
                conversation_id: request.conversation_id,
                version,
                body: request.body,
            });
        }
        let current = read_composer(&tx, &request.conversation_id)?;
        if current.version != request.expected_version {
            return Err(CoreError::new(
                "VersionConflict",
                "The project composer changed in another editor.",
            ));
        }
        let next = parse_version(&current.version)?
            .checked_add(1)
            .ok_or_else(|| CoreError::new("VersionLimit", "Composer version exhausted."))?;
        tx.execute("UPDATE project_conversations SET composer_version=?,composer_json=? WHERE id=? AND composer_version=?",params![next,serde_json::to_string(&request.body)?,request.conversation_id,parse_version(&current.version)?])?;
        record_local(
            &tx,
            &request.access,
            &request.conversation_id,
            &request.operation_id,
            "saveProjectComposer",
            &payload,
            &anchor,
            &next.to_string(),
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(ProjectComposerSnapshot {
            conversation_id: request.conversation_id,
            version: next.to_string(),
            body: request.body,
        })
    }

    pub(super) fn start_project_chat(
        &mut self,
        request: StartProjectChat,
    ) -> CoreResult<DiscussionStart> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        validate_composer(&request.composer)?;
        if request.composer.chapter.is_some() {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "A chapter task must be submitted through the chapter request command.",
            ));
        }
        if request.composer.text.trim().is_empty() {
            return Err(CoreError::new(
                "InvalidProjectChat",
                "Write an idea or a question first.",
            ));
        }
        let payload = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let anchor = require_conversation(&tx, &request.access, &request.conversation_id)?;
        let existing:Option<(String,String)>=tx.query_row("SELECT id,payload_hash FROM discussion_runs WHERE project_id=? AND operation_namespace=? AND operation_id=?",params![request.access.project_id,request.access.operation_namespace,request.operation_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((run_id, hash)) = existing {
            if hash != payload {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This request ID already has another instruction or model.",
                ));
            }
            read_item_by_reference(&tx, &request.conversation_id, "request", &run_id)?;
            let result = discussions::read_start(&tx, &run_id)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(result);
        }
        if existing_receipt(
            &tx,
            &request.access.operation_namespace,
            &request.operation_id,
            "projectChat",
            &payload,
        )?
        .is_some()
        {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "A local operation already uses this request ID.",
            ));
        }
        let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM discussion_runs r JOIN conversation_items i ON i.reference_id=r.id AND i.kind IN ('request','chapterRequest') WHERE i.conversation_id=? AND r.status IN ('queued','running','stopping'))",[&request.conversation_id],|r|r.get(0))?;
        if active {
            return Err(CoreError::new(
                "ProjectChatBusy",
                "Wait for this project's request or stop it before sending another.",
            ));
        }
        let current = read_composer(&tx, &request.conversation_id)?;
        if current.version != request.expected_composer_version || current.body != request.composer
        {
            return Err(CoreError::new(
                "VersionConflict",
                "Save the exact composer and references before sending.",
            ));
        }
        let mut sources = request.composer.source_refs.clone();
        if let Some(focused) = &request.composer.focused_document_ref
            && !sources.iter().any(|h| h.document_id == focused.document_id)
        {
            sources.push(focused.clone());
        }
        let chat = ProjectChatFreeze {
            conversation_id: request.conversation_id.clone(),
            source_refs: sources.clone(),
            task_draft_refs: request.composer.task_draft_refs.clone(),
            prompt_recipe_version: Some(
                crate::projects::project_chat_output::PROJECT_CHAT_PROMPT_RECIPE_V3.to_owned(),
            ),
        };
        let discussion = StartDiscussion {
            access: request.access.clone(),
            operation_id: request.operation_id.clone(),
            expected: anchor.head,
            instruction: request.composer.text.clone(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: request.budget,
            provider_binding: request.provider_binding,
            previous_run_id: None,
            lookup: None,
        };
        let result =
            discussions::start_discussion_at(&tx, &discussion, &payload, Some(&chat), false)?;
        append_item(
            &tx,
            &request.access,
            &request.conversation_id,
            Some(&request.operation_id),
            "request",
            Some(&result.run.id),
            &json!({"composerVersion":request.expected_composer_version,"sourceRefs":sources,"taskDraftRefs":request.composer.task_draft_refs,"userMessageId":result.user_message.id}),
        )?;
        // The accepted buffer is cleared atomically. Renderer keeps any newer
        // typing and saves that next buffer against this advanced watermark.
        tx.execute("UPDATE project_conversations SET composer_version=composer_version+1,composer_json=? WHERE id=?",params![serde_json::to_string(&ProjectComposer::default())?,request.conversation_id])?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }
}
