//! Local materialization of a completed project-chat response.
//!
//! This boundary deliberately does not know how to invoke a provider.  A
//! worker may call it for every terminal run, including after reopening a
//! project.  Only a durable terminal that still has an exact output event is
//! eligible for a draft; malformed, stopped, and failed outputs remain
//! readable in the originating run but never become story documents.

use super::*;
use wns_story::context_packets;
use wns_context::project_chat_output::{
    ChatDraftOutput, ChatGroupEffectsOutput, materialize_draft_body,
    parse_project_assistant_output_with_predecessors_and_chapters,
};
use wns_story::story_context;
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::json;
use std::collections::BTreeSet;

const MATERIALIZE_RECEIPT_KIND: &str = "materializeChatResult";
const MATERIALIZE_ITEM_KIND: &str = "materializeChatResult";

struct RawTerminalRun {
    status: String,
    sequence: i64,
    output_text: String,
    packet_id: String,
    terminal_event_id: String,
}

fn materialize_operation_id(run_id: &str) -> String {
    format!("chat-materialize-{run_id}")
}

fn materialize_payload_hash(owner: &RunOwner) -> String {
    sha256_hex(
        format!(
            "project-chat-materialize-v1:{}:{}:{}",
            owner.project_id, owner.operation_namespace, owner.run_id
        )
        .as_bytes(),
    )
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MaterializationEvent {
    run_id: String,
    sequence: i64,
    output_hash: String,
    output_valid: bool,
    #[serde(default)]
    draft_refs: Vec<MaterializationDraftRef>,
    detail: Option<String>,
    #[serde(default)]
    group_effects: Option<ChatGroupEffectsOutput>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MaterializationDraftRef {
    ordinal: usize,
    document_id: String,
}

fn replay_materialization(
    tx: &Transaction<'_>,
    access: &ProjectAccess,
    conversation_id: &str,
    operation_id: &str,
    payload_hash: &str,
) -> CoreResult<Option<ChatMaterialization>> {
    if existing_receipt(
        tx,
        &access.operation_namespace,
        operation_id,
        MATERIALIZE_RECEIPT_KIND,
        payload_hash,
    )?
    .is_none()
    {
        return Ok(None);
    }
    let (item_id, payload_json, payload_hash_stored): (String, String, String) = tx.query_row(
        "SELECT id,payload_json,payload_hash FROM conversation_items
             WHERE conversation_id=? AND operation_id=? AND kind=?",
        params![conversation_id, operation_id, MATERIALIZE_ITEM_KIND],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if sha256_hex(payload_json.as_bytes()) != payload_hash_stored {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The materialization event fingerprint changed.",
        ));
    }
    let event: MaterializationEvent = serde_json::from_str(&payload_json)?;
    let (sequence, output): (i64, String) = tx.query_row(
        "SELECT sequence,output_text FROM discussion_runs WHERE id=? AND project_id=? AND operation_namespace=?",
        params![&event.run_id, access.project_id, access.operation_namespace],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if event.sequence != sequence || event.output_hash != sha256_hex(output.as_bytes()) {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The materialization event does not match the retained run output.",
        ));
    }
    Ok(Some(ChatMaterialization {
        run_id: event.run_id,
        item_id,
        draft_ids: event
            .draft_refs
            .into_iter()
            .map(|draft| {
                let _ = draft.ordinal;
                draft.document_id
            })
            .collect(),
        output_valid: event.output_valid,
        detail: event.detail,
        group_effects: event.group_effects,
    }))
}

fn read_terminal_run(tx: &Transaction<'_>, owner: &RunOwner) -> CoreResult<Option<RawTerminalRun>> {
    let row: Option<(String, String, i64, String, String)> = tx
        .query_row(
            "SELECT status,dispatch_state,sequence,output_text,packet_id
             FROM discussion_runs
             WHERE id=? AND project_id=? AND operation_namespace=?",
            params![owner.run_id, owner.project_id, owner.operation_namespace],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((status, dispatch_state, sequence, output_text, packet_id)) = row else {
        return Err(CoreError::new(
            "DiscussionRunNotFound",
            "The project-chat run is not available in this project.",
        ));
    };
    if matches!(status.as_str(), "queued" | "running" | "stopping") || dispatch_state != "delivered"
    {
        return Ok(None);
    }
    let terminal: Option<(String, String)> = tx
        .query_row(
            "SELECT event_id,chunk FROM discussion_output_events
             WHERE run_id=? AND sequence=? AND kind='terminal'",
            params![owner.run_id, sequence],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((terminal_event_id, terminal_text)) = terminal else {
        // The durable run is terminal but its matching output event is not
        // present.  Keep the run inspectable; it cannot be materialized.
        return Ok(None);
    };
    if terminal_text != output_text {
        return Ok(None);
    }
    Ok(Some(RawTerminalRun {
        status,
        sequence,
        output_text,
        packet_id,
        terminal_event_id,
    }))
}

fn read_provider_terminal(
    tx: &Transaction<'_>,
    owner: &RunOwner,
    run: &RawTerminalRun,
    packet: &wns_context::packet::CompiledPacket,
) -> CoreResult<bool> {
    let Some(binding) = packet.options.provider_binding.as_ref() else {
        return Ok(true);
    };
    let row: Option<(String, String, String, i64, String)> = tx
        .query_row(
            "SELECT assistant_text,outcome,cleanup,expected_sequence,terminal_event_id
             FROM provider_results WHERE run_id=? AND packet_id=?",
            params![owner.run_id, run.packet_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((assistant_text, outcome, cleanup, expected_sequence, event_id)) = row else {
        return Ok(false);
    };
    // This is an integrity check rather than a provider capability check.  A
    // provider binding without its terminal receipt is not durable enough for
    // local draft creation.
    let _ = binding;
    Ok(assistant_text == run.output_text
        && outcome == "completed"
        && cleanup == "settled"
        && expected_sequence + 1 == run.sequence
        && event_id == run.terminal_event_id)
}

fn parse_packet_and_context(
    tx: &Transaction<'_>,
    run: &RawTerminalRun,
) -> CoreResult<(
    wns_context::packet::CompiledPacket,
    wns_story::story_context::FrozenContext,
)> {
    let packet = context_packets::validated_packet_record(tx, &run.packet_id)?;
    let (frozen, _) = story_context::validated_snapshot_record(tx, &packet.receipt.snapshot_id)?;
    Ok((packet, frozen))
}

pub(crate) fn allowed_target_handles(
    tx: &Transaction<'_>,
    frozen: &wns_story::story_context::FrozenContext,
) -> CoreResult<BTreeSet<String>> {
    let mut handles = BTreeSet::new();
    for source in &frozen.snapshot.sources {
        let (role, kind): (String, String) = tx.query_row(
            "SELECT role,kind FROM documents WHERE id=? AND trashed=0",
            [&source.source.document_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if role == DocumentRole::Ordinary.storage_name() && kind != "chapter" {
            handles.insert(source.handle.clone());
        }
    }
    Ok(handles)
}

/// Return exact frozen handles for ordinary chapter documents. Chapter
/// handoffs use a separate set so a chapter revision can never become a
/// nonchapter material target by sharing the generic target validator.
pub(crate) fn allowed_chapter_target_handles(
    tx: &Transaction<'_>,
    frozen: &wns_story::story_context::FrozenContext,
) -> CoreResult<BTreeSet<String>> {
    let mut handles = BTreeSet::new();
    for source in &frozen.snapshot.sources {
        let (role, kind): (String, String) = tx.query_row(
            "SELECT role,kind FROM documents WHERE id=? AND trashed=0",
            [&source.source.document_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if role == DocumentRole::Ordinary.storage_name() && kind == "chapter" {
            handles.insert(source.handle.clone());
        }
    }
    Ok(handles)
}

/// Return only handles for assistant drafts that the author explicitly
/// attached to this frozen request. Draft sources are never inferred from the
/// full snapshot and are never valid ordinary working targets.
pub(crate) fn allowed_predecessor_handles(
    tx: &Transaction<'_>,
    frozen: &wns_story::story_context::FrozenContext,
) -> CoreResult<BTreeSet<String>> {
    let Some(chat) = frozen.project_chat.as_ref() else {
        return Ok(BTreeSet::new());
    };
    let mut handles = BTreeSet::new();
    for source in &frozen.snapshot.sources {
        if source.kind != wns_context::SourceKind::AssistantDraft {
            continue;
        }
        let explicit = chat.task_draft_refs.iter().any(|draft| {
            draft.head.document_id == source.source.document_id
                && draft.head.body_hash == source.source.body_hash
        });
        if !explicit {
            continue;
        }
        let role: String = tx.query_row(
            "SELECT role FROM documents WHERE id=? AND trashed=0",
            [&source.source.document_id],
            |row| row.get(0),
        )?;
        if role != DocumentRole::AssistantDraft.storage_name() {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "An explicitly attached predecessor is not an assistant draft.",
            ));
        }
        handles.insert(source.handle.clone());
    }
    Ok(handles)
}

fn predecessor_document_id(
    frozen: &wns_story::story_context::FrozenContext,
    handle: Option<&str>,
) -> CoreResult<Option<String>> {
    let Some(handle) = handle else {
        return Ok(None);
    };
    let chat = frozen.project_chat.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidProjectChatOutput",
            "A draft predecessor requires project-chat context.",
        )
    })?;
    let source = frozen
        .snapshot
        .sources
        .iter()
        .find(|source| source.handle == handle)
        .ok_or_else(|| {
            CoreError::new(
                "InvalidProjectChatOutput",
                "The draft predecessor is not frozen.",
            )
        })?;
    if source.kind != wns_context::SourceKind::AssistantDraft
        || !chat.task_draft_refs.iter().any(|draft| {
            draft.head.document_id == source.source.document_id
                && draft.head.body_hash == source.source.body_hash
        })
    {
        return Err(CoreError::new(
            "InvalidProjectChatOutput",
            "A draft predecessor must be an explicitly attached assistant draft.",
        ));
    }
    Ok(Some(source.source.document_id.clone()))
}

fn source_head(
    tx: &Transaction<'_>,
    frozen: &wns_story::story_context::FrozenContext,
    handle: &str,
) -> CoreResult<Head> {
    let source = frozen
        .snapshot
        .sources
        .iter()
        .find(|source| source.handle == handle)
        .ok_or_else(|| {
            CoreError::new(
                "InvalidProjectChatOutput",
                "The draft target is not frozen.",
            )
        })?;
    let (document_id, version, hash): (String, i64, String) = tx.query_row(
        "SELECT document_id,source_working_version,body_hash FROM revisions WHERE id=?",
        [&source.source.revision_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if document_id != source.source.document_id || hash != source.source.body_hash {
        return Err(CoreError::new(
            "InvalidProjectChatOutput",
            "The draft target revision no longer matches its frozen source.",
        ));
    }
    Ok(Head {
        document_id,
        version: version.to_string(),
        body_hash: hash,
    })
}

fn insert_draft(
    tx: &Transaction<'_>,
    owner: &RunOwner,
    conversation_id: &str,
    run: &RawTerminalRun,
    frozen: &wns_story::story_context::FrozenContext,
    ordinal: usize,
    draft: &ChatDraftOutput,
) -> CoreResult<String> {
    let body = materialize_draft_body(&draft.blocks)?;
    let validated = validate_snapshot_json(&serde_json::to_string(&body)?)
        .map_err(|message| CoreError::new("InvalidDocument", &message))?;
    let document_id = new_id();
    let position: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position),-1)+1 FROM documents",
        [],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash,projection_dirty,trashed,role)
         VALUES(?,?,?, ?,0,1,?,?,0,0,'assistantDraft')",
        params![
            document_id,
            draft.kind,
            draft.title,
            position,
            validated.canonical_json,
            validated.hash
        ],
    )?;
    let document = read_document_with_role(tx, &document_id, DocumentRole::AssistantDraft)?;
    let revision = checkpoint_at(tx, &document, "projectChatDraft")?;
    let target = draft
        .target_handle
        .as_deref()
        .map(|handle| source_head(tx, frozen, handle))
        .transpose()?;
    let predecessor = predecessor_document_id(frozen, draft.predecessor_handle.as_deref())?;
    let source_epoch = parse_version(&frozen.snapshot.context_source_epoch)?;
    let policy_epoch = parse_version(&frozen.policy.version)?;
    tx.execute(
        "INSERT INTO assistant_drafts(document_id,conversation_id,project_id,operation_namespace,origin_run_id,output_ordinal,packet_id,source_epoch,policy_epoch,target_json,initial_revision_id,predecessor_document_id)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            document_id,
            conversation_id,
            owner.project_id,
            owner.operation_namespace,
            owner.run_id,
            ordinal as i64,
            run.packet_id,
            source_epoch,
            policy_epoch,
            target.as_ref().map(serde_json::to_string).transpose()?,
            revision.id,
            predecessor,
        ],
    )?;
    Ok(document_id)
}

// Actor-side logic, as free functions over `ProjectChatHost`.

pub fn materialize_chat_result(
host: &mut impl ProjectChatHost,
    owner: RunOwner,
) -> CoreResult<Option<ChatMaterialization>> {
    if owner.project_id != host.info().project_id
        || owner.operation_namespace != host.info().operation_namespace
    {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "The project-chat run belongs to another project identity.",
        ));
    }
    check_id(&owner.run_id)?;
    let tx = host
        .db_mut()?
        .transaction_with_behavior(TransactionBehavior::Immediate)?;

    let conversation_id: Option<String> = tx
        .query_row(
            "SELECT conversation_id FROM conversation_items
             WHERE kind='request' AND reference_id=? AND project_id=? AND operation_namespace=?
             ORDER BY sequence LIMIT 1",
            params![owner.run_id, owner.project_id, owner.operation_namespace],
            |r| r.get(0),
        )
        .optional()?;
    let Some(conversation_id) = conversation_id else {
        return Ok(None);
    };
    let access = ProjectAccess {
        project_id: owner.project_id.clone(),
        operation_namespace: owner.operation_namespace.clone(),
        session: String::new(),
        writer_lease: String::new(),
    };
    let operation_id = materialize_operation_id(&owner.run_id);
    let payload_hash = materialize_payload_hash(&owner);
    if let Some(saved) =
        replay_materialization(&tx, &access, &conversation_id, &operation_id, &payload_hash)?
    {
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(Some(saved));
    }
    let anchor = store::require_conversation(&tx, &access, &conversation_id)?;
    let Some(raw) = read_terminal_run(&tx, &owner)? else {
        tx.commit().map_err(CoreError::uncertain)?;
        return Ok(None);
    };

    // A packet or snapshot that fails its immutable validation produces a
    // retained non-adoptable result.  It is still recorded as a local
    // event so a later retry cannot accidentally call the provider again.
    let mut output_valid = false;
    let mut detail = None;
    let mut draft_ids = Vec::new();
    let mut group_effects = None;
    let parsed = if raw.status != "completed" {
        detail = Some(format!("terminal_status_{}", raw.status));
        None
    } else {
        match parse_packet_and_context(&tx, &raw) {
            Ok((packet, frozen)) if read_provider_terminal(&tx, &owner, &raw, &packet)? => {
                let handles = allowed_target_handles(&tx, &frozen)?;
                let predecessor_handles = allowed_predecessor_handles(&tx, &frozen)?;
                let chapter_handles = allowed_chapter_target_handles(&tx, &frozen)?;
                match parse_project_assistant_output_with_predecessors_and_chapters(
                    &raw.output_text,
                    &handles,
                    &predecessor_handles,
                    &chapter_handles,
                ) {
                    Ok(output) => {
                        group_effects = output.group_effects.clone();
                        if let Some(chat) = frozen.project_chat.as_ref() {
                            if chat.conversation_id != conversation_id
                                || chat.operation_namespace != owner.operation_namespace
                            {
                                detail = Some("project_chat_identity_mismatch".to_owned());
                                None
                            } else {
                                output_valid = true;
                                Some((output, frozen))
                            }
                        } else {
                            detail = Some("missing_project_chat_context".to_owned());
                            None
                        }
                    }
                    Err(error) => {
                        detail = Some(error.detail);
                        None
                    }
                }
            }
            Ok(_) => {
                detail = Some("provider_terminal_receipt_missing_or_mismatched".to_owned());
                None
            }
            Err(error) => {
                detail = Some(error.detail);
                None
            }
        }
    };
    if let Some((output, frozen)) = parsed {
        for (ordinal, draft) in output.drafts.iter().enumerate() {
            draft_ids.push(insert_draft(
                &tx,
                &owner,
                &conversation_id,
                &raw,
                &frozen,
                ordinal,
                draft,
            )?);
        }
        // A source or policy change while the provider was running does
        // not delete a reviewable draft.  The draft reader computes the
        // stale flag from these frozen epochs and adoption will refuse it.
        if detail.is_none() && raw.status != "completed" {
            output_valid = false;
            detail = Some("terminal_run_was_not_completed".to_owned());
        }
    }
    if raw.status != "completed" {
        output_valid = false;
        if detail.is_none() {
            detail = Some(format!("terminal_status_{}", raw.status));
        }
    }
    let item_payload = json!({
        "runId": owner.run_id,
        "sequence": raw.sequence,
        "outputHash": sha256_hex(raw.output_text.as_bytes()),
        "outputValid": output_valid,
        "draftRefs": draft_ids.iter().enumerate().map(|(ordinal, document_id)| json!({
            "ordinal": ordinal,
            "documentId": document_id,
        })).collect::<Vec<_>>(),
        "detail": detail.clone(),
        "groupEffects": group_effects.clone(),
    });
    let item = store::append_item(
        &tx,
        &access,
        &conversation_id,
        Some(&operation_id),
        MATERIALIZE_ITEM_KIND,
        Some(&owner.run_id),
        &item_payload,
    )?;
    let result = ChatMaterialization {
        run_id: owner.run_id,
        item_id: item.id.clone(),
        draft_ids,
        output_valid,
        detail,
        group_effects,
    };
    // Keep the event payload to references and the output fingerprint.
    // The assistant answer and draft bodies remain in their authoritative
    // run/message and document/revision rows respectively.
    insert_receipt(
        &tx,
        &access.operation_namespace,
        &operation_id,
        MATERIALIZE_RECEIPT_KIND,
        &payload_hash,
        &StoredResult {
            head: anchor.head,
            saved_generation: "0".into(),
            applied: None,
            restored: None,
        },
    )?;
    tx.commit().map_err(CoreError::uncertain)?;
    Ok(Some(result))
}
