//! Backup and recovery validation for the project-chat projection.
//!
//! Project chat deliberately stores references to the existing discussion,
//! packet, revision, and receipt authorities.  SQLite foreign keys catch
//! missing rows, but they do not prove that the rows have the same project
//! owner, that a reference still has the claimed fingerprint, or that an
//! isolated draft has not been smuggled into the ordinary story namespace.
//! This pass is therefore run on the same read snapshot as the rest of backup
//! validation and rejects malformed chat state before it can be recovered.

use super::ProjectComposer;
use super::{ChatDispositionScope, ChatDispositionScopeKind, ChatUnknownTo};
use wns_context::project_chat_output::ChatGroupEffectsOutput;
use wns_kernel::{CoreError, CoreResult, DocumentRole};
use wns_storage::read_document_with_role;
use wns_kernel::validate_snapshot_json;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const CHAT_KINDS: &[&str] = &[
    "request",
    "chapterRequest",
    "saveProjectComposer",
    "saveAssistantDraft",
    "materializeChatResult",
    "chatDisposition",
    "adoptionPreview",
    "adoptionDecision",
];

#[derive(Debug, Clone)]
struct ConversationOwner {
    conversation_id: String,
    project_id: String,
    operation_namespace: String,
    anchor_document_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MaterializationEvent {
    run_id: String,
    sequence: i64,
    output_hash: String,
    output_valid: bool,
    #[serde(default)]
    draft_refs: Vec<MaterializationDraftRef>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    group_effects: Option<ChatGroupEffectsOutput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MaterializationDraftRef {
    ordinal: usize,
    document_id: String,
}

fn invalid(detail: impl Into<String>) -> CoreError {
    CoreError::new("InvalidProjectChat", &detail.into())
}

fn id(value: &str, label: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid(format!("{label} has an invalid identifier.")));
    }
    Ok(())
}

fn version(value: i64, label: &str) -> CoreResult<()> {
    if value < 0 {
        return Err(invalid(format!("{label} is negative.")));
    }
    Ok(())
}

fn version_text(value: &str, label: &str) -> CoreResult<()> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || value.parse::<i64>().is_err()
    {
        return Err(invalid(format!("{label} has an invalid version.")));
    }
    Ok(())
}

fn hash(value: &str, label: &str) -> CoreResult<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!("{label} has an invalid hash.")));
    }
    Ok(())
}

fn sha256(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn canonical_json(value: &str, label: &str) -> CoreResult<Value> {
    let parsed: Value = serde_json::from_str(value)
        .map_err(|error| invalid(format!("{label} is not valid JSON: {error}")))?;
    let canonical = serde_json::to_string(&wns_kernel::canonicalize_value(parsed.clone()))
        .map_err(|error| invalid(format!("{label} cannot be canonicalized: {error}")))?;
    if canonical != value {
        return Err(invalid(format!("{label} is not in canonical form.")));
    }
    Ok(parsed)
}

fn json_value(value: &str, label: &str) -> CoreResult<Value> {
    serde_json::from_str(value)
        .map_err(|error| invalid(format!("{label} is not valid JSON: {error}")))
}

fn validate_disposition_payload(
    db: &Connection,
    owner: &ConversationOwner,
    conversation_id: &str,
    reference_id: &str,
    payload: &Value,
) -> CoreResult<()> {
    let disposition = payload
        .get("disposition")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("A disposition payload has no status."))?;
    let scope = match payload.get("scope") {
        Some(value) if !value.is_null() => {
            serde_json::from_value::<ChatDispositionScope>(value.clone())
                .map_err(|_| invalid("A disposition scope is malformed."))?
        }
        _ => ChatDispositionScope::default(),
    };
    scope
        .validate_shape()
        .map_err(|_| invalid("A disposition scope is malformed."))?;
    let unknown_to = match payload.get("unknownTo") {
        Some(value) if !value.is_null() => Some(
            serde_json::from_value::<ChatUnknownTo>(value.clone())
                .map_err(|_| invalid("A disposition unknownTo value is invalid."))?,
        ),
        _ => None,
    };
    if unknown_to.is_some() && disposition != "keepMysterious" {
        return Err(invalid(
            "unknownTo is only valid for keepMysterious questions.",
        ));
    }
    let is_draft = !reference_id.contains(':');
    if is_draft {
        if !matches!(scope.kind, ChatDispositionScopeKind::Project) || unknown_to.is_some() {
            return Err(invalid(
                "Draft dispositions must use project scope without unknownTo.",
            ));
        }
        return Ok(());
    }
    let (run_id, key) = reference_id
        .split_once(':')
        .ok_or_else(|| invalid("A response disposition reference is malformed."))?;
    id(run_id, "disposition producer")?;
    let linked: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversation_items WHERE conversation_id=? AND project_id=? AND operation_namespace=? AND kind='request' AND reference_id=?)",
        params![conversation_id, owner.project_id, owner.operation_namespace, run_id],
        |row| row.get(0),
    )?;
    if !linked {
        return Err(invalid(
            "A response disposition producer is outside its conversation.",
        ));
    }
    if !key.is_empty()
        && let Some(output_text) = db
            .query_row(
                "SELECT output_text FROM discussion_runs WHERE id=? AND project_id=? AND operation_namespace=?",
                params![run_id, owner.project_id, owner.operation_namespace],
                |row| row.get::<_, String>(0),
            )
            .optional()?
    {
        let output: Value = serde_json::from_str(&output_text)
            .map_err(|_| invalid("A response disposition producer output is invalid."))?;
        let assumption = output
            .get("assumptions")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| item.get("key").and_then(Value::as_str) == Some(key))
            });
        if assumption && (!matches!(scope.kind, ChatDispositionScopeKind::Project) || unknown_to.is_some()) {
            return Err(invalid(
                "Assumption dispositions must use project scope without unknownTo.",
            ));
        }
    }
    match scope.kind {
        ChatDispositionScopeKind::Project => Ok(()),
        ChatDispositionScopeKind::Task => {
            if scope.reference_id.as_deref() != Some(run_id) {
                return Err(invalid(
                    "A task disposition scope must reference its producing run.",
                ));
            }
            Ok(())
        }
        ChatDispositionScopeKind::Chapter | ChatDispositionScopeKind::Document => {
            let document_id = scope.reference_id.as_deref().expect("validated scope");
            let document = read_document_with_role(db, document_id, DocumentRole::Ordinary)
                .map_err(|_| {
                    invalid("A disposition scope references a missing or ineligible document.")
                })?;
            if scope.kind == ChatDispositionScopeKind::Chapter && document.kind != "chapter" {
                return Err(invalid(
                    "A disposition scope references an ineligible document.",
                ));
            }
            Ok(())
        }
    }
}

fn head(document_id: &str, version_value: i64, body_hash: &str, label: &str) -> CoreResult<()> {
    id(document_id, &format!("{label} document"))?;
    version(version_value, &format!("{label} version"))?;
    hash(body_hash, &format!("{label} body"))?;
    Ok(())
}

fn validate_document_body(body: &str, body_hash: &str, label: &str) -> CoreResult<Value> {
    hash(body_hash, &format!("{label} hash"))?;
    let receipt = validate_snapshot_json(body)
        .map_err(|error| invalid(format!("{label} is invalid: {error}")))?;
    if receipt.hash != body_hash || receipt.canonical_json != body {
        return Err(invalid(format!(
            "{label} fingerprint does not match its body."
        )));
    }
    Ok(receipt.snapshot)
}

fn blank_anchor(body: &Value) -> bool {
    let Some(blocks) = body
        .get("body")
        .and_then(Value::as_object)
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    blocks.len() == 1
        && blocks[0].get("type").and_then(Value::as_str) == Some("paragraph")
        && blocks[0]
            .get("content")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
}

type StoredDocumentRow = (String, String, i64, String, String, Option<String>, i64);
type ValidatedDocumentRow = (
    DocumentRole,
    String,
    i64,
    String,
    String,
    Option<String>,
    i64,
);
type StoredRunRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
);

fn document_role(db: &Connection, document_id: &str) -> CoreResult<ValidatedDocumentRow> {
    let row: Option<StoredDocumentRow> = db
        .query_row(
            "SELECT role,kind,working_version,body_hash,body_json,last_checkpoint_id,trashed
             FROM documents WHERE id=?",
            [document_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some((role, kind, working_version, body_hash, body_json, checkpoint, trashed)) = row else {
        return Err(invalid(format!("Document {document_id:?} is missing.")));
    };
    let role =
        DocumentRole::from_storage(&role).map_err(|_| invalid("Document role is invalid."))?;
    id(document_id, "document")?;
    version(working_version, "document working version")?;
    validate_document_body(&body_json, &body_hash, "document body")?;
    if let Some(checkpoint) = &checkpoint {
        id(checkpoint, "document checkpoint")?;
    }
    if trashed != 0 && trashed != 1 {
        return Err(invalid("Document trash state is invalid."));
    }
    Ok((
        role,
        kind,
        working_version,
        body_hash,
        body_json,
        checkpoint,
        trashed,
    ))
}

fn validate_anchor(db: &Connection, owner: &ConversationOwner) -> CoreResult<()> {
    let (role, kind, _version, body_hash, body_json, checkpoint, trashed) =
        document_role(db, &owner.anchor_document_id)?;
    if role != DocumentRole::ConversationAnchor || kind != "note" || trashed != 0 {
        return Err(invalid(
            "A project conversation anchor has the wrong role or kind.",
        ));
    }
    let body = validate_document_body(&body_json, &body_hash, "conversation anchor")?;
    if !blank_anchor(&body) {
        return Err(invalid("A project conversation anchor must remain blank."));
    }
    let Some(checkpoint) = checkpoint else {
        return Err(invalid(
            "A project conversation anchor has no immutable revision.",
        ));
    };
    validate_revision(db, &checkpoint, &owner.anchor_document_id, Some(&body_hash))?;
    Ok(())
}

fn validate_head_json(value: &Value, label: &str) -> CoreResult<(String, String, String)> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(format!("{label} is not an object.")))?;
    let document_id = object
        .get("documentId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{label} has no document ID.")))?;
    let version = object
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{label} has no version.")))?;
    let body_hash = object
        .get("bodyHash")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{label} has no body hash.")))?;
    id(document_id, &format!("{label} document"))?;
    version_text(version, &format!("{label} version"))?;
    hash(body_hash, &format!("{label} body"))?;
    Ok((
        document_id.to_owned(),
        version.to_owned(),
        body_hash.to_owned(),
    ))
}

fn validate_composer(
    db: &Connection,
    owner: &ConversationOwner,
    composer_json: &str,
) -> CoreResult<()> {
    let composer: ProjectComposer = serde_json::from_str(composer_json)
        .map_err(|error| invalid(format!("The composer is invalid: {error}")))?;
    if composer.text.len() > 64 * 1024
        || composer.source_refs.len() > 64
        || composer.task_draft_refs.len() > 3
    {
        return Err(invalid("The composer exceeds its storage bounds."));
    }
    let mut source_ids = HashSet::new();
    for source in &composer.source_refs {
        id(&source.document_id, "composer source")?;
        version_text(&source.version, "composer source version")?;
        hash(&source.body_hash, "composer source hash")?;
        if !source_ids.insert(source.document_id.clone()) {
            return Err(invalid("The composer repeats a source document."));
        }
        let (role, _kind, ..) = document_role(db, &source.document_id)?;
        if role != DocumentRole::Ordinary {
            return Err(invalid("The composer source is not an ordinary document."));
        }
    }
    if let Some(focused) = &composer.focused_document_ref {
        id(&focused.document_id, "composer focus")?;
        version_text(&focused.version, "composer focus version")?;
        hash(&focused.body_hash, "composer focus hash")?;
        if !source_ids.insert(focused.document_id.clone()) {
            return Err(invalid("The composer repeats its focused document."));
        }
        let (role, _kind, ..) = document_role(db, &focused.document_id)?;
        if role != DocumentRole::Ordinary {
            return Err(invalid("The composer focus is not an ordinary document."));
        }
    }
    let mut draft_ids = HashSet::new();
    for draft in &composer.task_draft_refs {
        id(&draft.head.document_id, "composer draft")?;
        version_text(&draft.head.version, "composer draft version")?;
        hash(&draft.head.body_hash, "composer draft hash")?;
        version_text(&draft.disposition_version, "composer draft disposition")?;
        if !draft_ids.insert(draft.head.document_id.clone()) {
            return Err(invalid("The composer repeats a task draft."));
        }
        let valid: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM assistant_drafts
             WHERE document_id=? AND conversation_id=? AND project_id=? AND operation_namespace=?)",
            params![
                draft.head.document_id,
                owner.conversation_id,
                owner.project_id,
                owner.operation_namespace
            ],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(invalid(
                "The composer task draft is outside its conversation.",
            ));
        }
    }
    if let Some(chapter) = composer.chapter.as_ref() {
        head(
            &chapter.target.document_id,
            chapter
                .target
                .version
                .parse::<i64>()
                .map_err(|_| invalid("Chapter target version is invalid."))?,
            &chapter.target.body_hash,
            "chapter target",
        )?;
        let (role, kind, working_version, body_hash, ..) =
            document_role(db, &chapter.target.document_id)?;
        if role != DocumentRole::Ordinary || kind != "chapter" {
            return Err(invalid(
                "A chapter composer target must be an ordinary chapter document.",
            ));
        }
        if working_version.to_string() != chapter.target.version
            || body_hash != chapter.target.body_hash
        {
            return Err(invalid(
                "A chapter composer target does not match its document head.",
            ));
        }
        if let Some(scope) = chapter.scope.as_ref()
            && (scope.quote.len() > 256 * 1024
                || scope.source_body_hash.len() != 64
                || !scope
                    .source_body_hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || scope.source_body_hash != chapter.target.body_hash)
        {
            return Err(invalid("A chapter composer scope is malformed."));
        }
        if let Some(brief) = chapter.safe_brief.as_ref() {
            if brief.text.trim().is_empty() || brief.text.len() > 16 * 1024 || !brief.confirmed {
                return Err(invalid("A chapter composer brief is malformed."));
            }
            if let Some(origin) = brief.project_origin.as_ref() {
                if origin.version != "project-conversation-brief.v1"
                    || origin.project_id != owner.project_id
                    || origin.operation_namespace != owner.operation_namespace
                    || origin.conversation_id != owner.conversation_id
                    || origin.target != chapter.target
                {
                    return Err(invalid(
                        "A chapter composer brief has the wrong project provenance.",
                    ));
                }
                id(&origin.message_id, "chapter brief message")?;
                hash(&origin.scope_hash, "chapter brief scope")?;
                hash(&origin.text_hash, "chapter brief text")?;
                let scope_hash = sha256(
                    &serde_json::to_vec(&chapter.scope)
                        .map_err(|_| invalid("A chapter brief scope cannot be hashed."))?,
                );
                let text_hash = sha256(brief.text.as_bytes());
                if origin.scope_hash != scope_hash || origin.text_hash != text_hash {
                    return Err(invalid(
                        "A chapter composer brief does not match its selected content.",
                    ));
                }
                let message_exists: bool = db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversation_items
                     WHERE conversation_id=? AND project_id=? AND operation_namespace=?
                       AND json_extract(payload_json,'$.userMessageId')=?)",
                    params![
                        owner.conversation_id,
                        owner.project_id,
                        owner.operation_namespace,
                        origin.message_id
                    ],
                    |row| row.get(0),
                )?;
                if !message_exists {
                    return Err(invalid(
                        "A chapter composer brief references a missing conversation message.",
                    ));
                }
            }
            if let Some(origin_id) = brief.origin_message_id.as_deref() {
                id(origin_id, "chapter brief message")?;
                if brief
                    .project_origin
                    .as_ref()
                    .is_some_and(|origin| origin.message_id != origin_id)
                {
                    return Err(invalid(
                        "A chapter composer brief has conflicting message provenance.",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_revision(
    db: &Connection,
    revision_id: &str,
    expected_document_id: &str,
    expected_hash: Option<&str>,
) -> CoreResult<(String, i64, String)> {
    let mut seen = HashSet::new();
    validate_revision_inner(
        db,
        revision_id,
        expected_document_id,
        expected_hash,
        &mut seen,
    )
}

fn validate_revision_inner(
    db: &Connection,
    revision_id: &str,
    expected_document_id: &str,
    expected_hash: Option<&str>,
    seen: &mut HashSet<String>,
) -> CoreResult<(String, i64, String)> {
    id(revision_id, "revision")?;
    if !seen.insert(revision_id.to_owned()) {
        return Err(invalid("Revision ancestry contains a cycle."));
    }
    let row: (String, i64, String, String, Option<String>, i64) = db.query_row(
        "SELECT document_id,source_working_version,body_hash,body_json,parent_id,schema_version
         FROM revisions WHERE id=?",
        [revision_id],
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
    )?;
    if row.0 != expected_document_id || row.5 != 1 {
        return Err(invalid(
            "A revision points at the wrong document or schema.",
        ));
    }
    head(&row.0, row.1, &row.2, "revision")?;
    if let Some(expected_hash) = expected_hash
        && expected_hash != row.2
    {
        return Err(invalid("A revision body hash does not match its document."));
    }
    validate_document_body(&row.3, &row.2, "revision body")?;
    if let Some(parent) = row.4 {
        validate_revision_inner(db, &parent, expected_document_id, None, seen)?;
    }
    Ok((row.0, row.1, row.2))
}

fn validate_run(
    db: &Connection,
    owner: &ConversationOwner,
    run_id: &str,
    chapter_request: bool,
) -> CoreResult<(i64, String, String, String, String, String)> {
    id(run_id, "discussion run")?;
    let row: Option<StoredRunRow> = db
        .query_row(
            "SELECT thread_id,project_id,operation_namespace,target_document_id,target_version,
                    target_body_hash,packet_id,status,dispatch_state,sequence,output_text
             FROM discussion_runs WHERE id=?",
            [run_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get::<_, i64>(4)?.to_string(),
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                ))
            },
        )
        .optional()?;
    let Some((
        thread_id,
        project_id,
        namespace,
        target_id,
        target_version,
        target_hash,
        packet_id,
        status,
        dispatch,
        sequence,
        output,
    )) = row
    else {
        return Err(invalid("A project-chat item refers to a missing run."));
    };
    if project_id != owner.project_id || namespace != owner.operation_namespace {
        return Err(invalid("A project-chat run has the wrong owner."));
    }
    id(&thread_id, "discussion thread")?;
    id(&packet_id, "context packet")?;
    version_text(&target_version, "run target version")?;
    hash(&target_hash, "run target hash")?;
    version(sequence, "run sequence")?;
    if !matches!(
        status.as_str(),
        "queued" | "running" | "stopping" | "completed" | "stopped" | "failed" | "interrupted"
    ) || !matches!(dispatch.as_str(), "pending" | "claimed" | "delivered")
    {
        return Err(invalid(
            "A project-chat run has an invalid lifecycle state.",
        ));
    }
    let thread: Option<(String, String, String)> = db
        .query_row(
            "SELECT project_id,operation_namespace,document_id FROM discussion_threads WHERE id=?",
            [&thread_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((thread_project, thread_namespace, thread_document)) = thread else {
        return Err(invalid("A project-chat run has no discussion thread."));
    };
    if thread_project != owner.project_id || thread_namespace != owner.operation_namespace {
        return Err(invalid("A project-chat run has the wrong thread owner."));
    }
    let (role, kind, ..) = document_role(db, &target_id)?;
    if chapter_request {
        if thread_document != target_id
            || target_id == owner.anchor_document_id
            || role != DocumentRole::Ordinary
            || kind != "chapter"
        {
            return Err(invalid(
                "A chapter request run must target its ordinary chapter document.",
            ));
        }
    } else if thread_document != owner.anchor_document_id
        || target_id != owner.anchor_document_id
        || role != DocumentRole::ConversationAnchor
    {
        return Err(invalid(
            "A project-chat run is not rooted in its conversation anchor.",
        ));
    }
    let packet_owner: Option<(String, String)> = db
        .query_row(
            "SELECT project_id,operation_namespace FROM context_packets WHERE id=?",
            [&packet_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if packet_owner.as_ref() != Some(&(owner.project_id.clone(), owner.operation_namespace.clone()))
    {
        return Err(invalid("A project-chat run packet has the wrong owner."));
    }
    let user_messages: i64 = db.query_row(
        "SELECT COUNT(*) FROM discussion_messages WHERE run_id=? AND thread_id=? AND role='user'",
        params![run_id, thread_id],
        |r| r.get(0),
    )?;
    if user_messages != 1 {
        return Err(invalid(
            "A project-chat run must have exactly one user message.",
        ));
    }
    Ok((sequence, output, packet_id, status, dispatch, target_hash))
}

fn validate_materialization(
    db: &Connection,
    owner: &ConversationOwner,
    reference_id: &str,
    payload: &Value,
) -> CoreResult<()> {
    let event: MaterializationEvent = serde_json::from_value(payload.clone())
        .map_err(|error| invalid(format!("The materialization event is invalid: {error}")))?;
    if event.run_id != reference_id {
        return Err(invalid(
            "A materialization event reference is inconsistent.",
        ));
    }
    let (run_sequence, output, _packet, status, dispatch, _target_hash) =
        validate_run(db, owner, &event.run_id, false)?;
    if event.sequence != run_sequence || event.output_hash != sha256(output.as_bytes()) {
        return Err(invalid(
            "A materialization event does not match its run output.",
        ));
    }
    hash(&event.output_hash, "materialization output")?;
    if event.output_valid
        && (status != "completed" || dispatch != "delivered" || event.detail.is_some())
    {
        return Err(invalid(
            "Only a delivered completed run can materialize a draft.",
        ));
    }
    if let Some(expected_effects) = &event.group_effects {
        let output_json: Value = serde_json::from_str(&output)
            .map_err(|_| invalid("A grouped materialization has non-JSON run output."))?;
        let actual_effects = output_json
            .get("groupEffects")
            .filter(|value| !value.is_null())
            .cloned()
            .ok_or_else(|| invalid("A grouped materialization has no matching output effects."))?;
        let actual_effects: ChatGroupEffectsOutput = serde_json::from_value(actual_effects)
            .map_err(|error| {
                invalid(format!(
                    "A grouped materialization has invalid effects: {error}"
                ))
            })?;
        if &actual_effects != expected_effects {
            return Err(invalid(
                "A materialization effect projection does not match its run output.",
            ));
        }
    }
    let mut ordinals = HashSet::new();
    let mut refs = HashSet::new();
    for draft in &event.draft_refs {
        if draft.ordinal >= 3
            || !ordinals.insert(draft.ordinal)
            || !refs.insert(draft.document_id.clone())
        {
            return Err(invalid(
                "A materialization event contains duplicate or invalid draft ordinals.",
            ));
        }
        id(&draft.document_id, "materialized draft")?;
        let row: Option<(String, String, i64)> = db
            .query_row(
                "SELECT conversation_id,operation_namespace,output_ordinal FROM assistant_drafts
                 WHERE document_id=? AND project_id=? AND origin_run_id=?",
                params![draft.document_id, owner.project_id, event.run_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((conversation_id, namespace, ordinal)) = row else {
            return Err(invalid(
                "A materialization event refers to an unrelated draft.",
            ));
        };
        if namespace != owner.operation_namespace || ordinal != draft.ordinal as i64 {
            return Err(invalid("A materialized draft has inconsistent provenance."));
        }
        let conversation_matches: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM project_conversations WHERE id=? AND project_id=? AND operation_namespace=?)",
            params![conversation_id, owner.project_id, owner.operation_namespace],
            |r| r.get(0),
        )?;
        if !conversation_matches {
            return Err(invalid(
                "A materialized draft belongs to another conversation.",
            ));
        }
    }
    let actual: i64 = db.query_row(
        "SELECT COUNT(*) FROM assistant_drafts WHERE project_id=? AND operation_namespace=? AND origin_run_id=?",
        params![owner.project_id, owner.operation_namespace, event.run_id],
        |r| r.get(0),
    )?;
    if actual != event.draft_refs.len() as i64 {
        return Err(invalid(
            "A materialization event does not enumerate every draft.",
        ));
    }
    Ok(())
}

fn validate_conversation_items(
    db: &Connection,
    owners: &HashMap<String, ConversationOwner>,
) -> CoreResult<()> {
    let mut query = db.prepare(
        "SELECT id,conversation_id,project_id,operation_namespace,sequence,operation_id,
                kind,reference_id,payload_json,payload_hash
         FROM conversation_items ORDER BY conversation_id,sequence,id",
    )?;
    let rows = query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
        ))
    })?;
    let mut previous: HashMap<String, i64> = HashMap::new();
    for row in rows {
        let (
            item_id,
            conversation_id,
            project_id,
            namespace,
            sequence,
            operation_id,
            kind,
            reference_id,
            payload_json,
            payload_hash,
        ) = row?;
        id(&item_id, "conversation item")?;
        let owner = owners
            .get(&conversation_id)
            .ok_or_else(|| invalid("A conversation item has no conversation root."))?;
        if owner.project_id != project_id || owner.operation_namespace != namespace {
            return Err(invalid("A conversation item has the wrong owner."));
        }
        let expected_sequence = previous.get(&conversation_id).copied().unwrap_or(0) + 1;
        if sequence != expected_sequence {
            return Err(invalid("Conversation item sequences are not contiguous."));
        }
        previous.insert(conversation_id.clone(), sequence);
        if !CHAT_KINDS.contains(&kind.as_str()) {
            return Err(invalid(format!("Unknown project-chat item kind {kind:?}.")));
        }
        let payload = canonical_json(&payload_json, "conversation item payload")?;
        hash(&payload_hash, "conversation item payload")?;
        if sha256(payload_json.as_bytes()) != payload_hash {
            return Err(invalid("A conversation item payload fingerprint changed."));
        }
        if let Some(operation_id) = operation_id.as_deref() {
            id(operation_id, "conversation item operation")?;
        }
        match kind.as_str() {
            "request" | "chapterRequest" => {
                let reference_id = reference_id
                    .as_deref()
                    .ok_or_else(|| invalid("A request item has no run reference."))?;
                let chapter_request = kind == "chapterRequest";
                let _ = validate_run(db, owner, reference_id, chapter_request)?;
                if operation_id.is_none() {
                    return Err(invalid("A request item has no operation identity."));
                }
                if chapter_request {
                    let target = payload
                        .get("target")
                        .ok_or_else(|| invalid("A chapter request has no target head."))?;
                    let (target_id, target_version, target_hash) =
                        validate_head_json(target, "chapter request target")?;
                    let (run_target_id, run_target_version, run_target_hash): (String, i64, String) =
                        db.query_row(
                            "SELECT target_document_id,target_version,target_body_hash FROM discussion_runs WHERE id=?",
                            [reference_id],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        )?;
                    if target_id != run_target_id
                        || target_version != run_target_version.to_string()
                        || target_hash != run_target_hash
                    {
                        return Err(invalid(
                            "A chapter request target does not match its discussion run.",
                        ));
                    }
                }
            }
            "materializeChatResult" => {
                let reference_id = reference_id
                    .as_deref()
                    .ok_or_else(|| invalid("A materialization item has no run reference."))?;
                validate_materialization(db, owner, reference_id, &payload)?;
            }
            "saveAssistantDraft" => {
                let document_id = reference_id
                    .as_deref()
                    .ok_or_else(|| invalid("A draft-save item has no draft reference."))?;
                let role: Option<String> = db
                    .query_row(
                        "SELECT role FROM documents WHERE id=?",
                        [document_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if role.as_deref() != Some(DocumentRole::AssistantDraft.storage_name()) {
                    return Err(invalid(
                        "A draft-save item does not reference an assistant draft.",
                    ));
                }
            }
            "chatDisposition" => {
                let reference_id = reference_id
                    .as_deref()
                    .ok_or_else(|| invalid("A disposition item has no response reference."))?;
                if reference_id.is_empty() || reference_id.len() > 256 {
                    return Err(invalid("A disposition reference is invalid."));
                }
                if !payload.is_object() {
                    return Err(invalid("A disposition payload is not an object."));
                }
                validate_disposition_payload(db, owner, &conversation_id, reference_id, &payload)?;
            }
            "adoptionPreview" => {
                let reference_id = reference_id
                    .as_deref()
                    .ok_or_else(|| invalid("An adoption preview has no identity."))?;
                crate::project_chat::adoption::validate_backup_preview(
                    db,
                    &owner.project_id,
                    &owner.operation_namespace,
                    &owner.conversation_id,
                    reference_id,
                    &payload,
                )?;
            }
            "adoptionDecision" => {
                let reference_id = reference_id
                    .as_deref()
                    .ok_or_else(|| invalid("An adoption decision has no identity."))?;
                id(reference_id, "adoption decision")?;
                if payload.get("schemaVersion").and_then(Value::as_str)
                    != Some("chat-adoption-decision.v1")
                {
                    return Err(invalid("An adoption decision has an invalid schema."));
                }
                let preview_id = payload
                    .get("previewId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("An adoption decision has no preview."))?;
                let decision_id = payload
                    .get("decisionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("An adoption decision has no decision ID."))?;
                if preview_id.is_empty() || decision_id != reference_id {
                    return Err(invalid("An adoption decision identity is inconsistent."));
                }
                let preview_exists: bool = db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversation_items WHERE conversation_id=? AND kind='adoptionPreview' AND reference_id=?)",
                    params![conversation_id, preview_id], |r| r.get(0))?;
                if !preview_exists {
                    return Err(invalid("An adoption decision has no immutable preview."));
                }
            }
            "saveProjectComposer" => {
                if operation_id.is_none() || reference_id.is_some() {
                    return Err(invalid("A composer-save item has invalid references."));
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn validate_assistant_drafts(
    db: &Connection,
    owners: &HashMap<String, ConversationOwner>,
) -> CoreResult<()> {
    let mut query = db.prepare(
        "SELECT document_id,conversation_id,project_id,operation_namespace,origin_run_id,
                output_ordinal,packet_id,source_epoch,policy_epoch,target_json,initial_revision_id,
                predecessor_document_id,disposition,disposition_version
         FROM assistant_drafts ORDER BY document_id",
    )?;
    let rows = query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, String>(12)?,
            row.get::<_, i64>(13)?,
        ))
    })?;
    for row in rows {
        let (
            document_id,
            conversation_id,
            project_id,
            namespace,
            run_id,
            ordinal,
            packet_id,
            source_epoch,
            policy_epoch,
            target_json,
            initial_revision_id,
            predecessor,
            disposition,
            disposition_version,
        ) = row?;
        id(&document_id, "assistant draft")?;
        let owner = owners
            .get(&conversation_id)
            .ok_or_else(|| invalid("An assistant draft has no conversation root."))?;
        if owner.project_id != project_id || owner.operation_namespace != namespace {
            return Err(invalid("An assistant draft has the wrong owner."));
        }
        if !(0..3).contains(&ordinal) {
            return Err(invalid("An assistant draft output ordinal is invalid."));
        }
        version(source_epoch, "assistant draft source epoch")?;
        version(policy_epoch, "assistant draft policy epoch")?;
        version(disposition_version, "assistant draft disposition version")?;
        if !matches!(
            disposition.as_str(),
            "pending" | "rejected" | "adopted" | "superseded"
        ) {
            return Err(invalid("An assistant draft disposition is invalid."));
        }
        let (role, _kind, working_version, body_hash, _body, checkpoint, trashed) =
            document_role(db, &document_id)?;
        if role != DocumentRole::AssistantDraft || trashed != 0 {
            return Err(invalid("An assistant draft document has the wrong role."));
        }
        head(&document_id, working_version, &body_hash, "assistant draft")?;
        let initial = validate_revision(db, &initial_revision_id, &document_id, None)?;
        if checkpoint.is_none() || initial.0 != document_id {
            return Err(invalid(
                "An assistant draft has an invalid initial revision.",
            ));
        }
        let run_owner: Option<(String, String, String)> = db
            .query_row(
                "SELECT project_id,operation_namespace,packet_id FROM discussion_runs WHERE id=?",
                [&run_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if run_owner.as_ref() != Some(&(project_id.clone(), namespace.clone(), packet_id.clone())) {
            return Err(invalid("An assistant draft has invalid run provenance."));
        }
        let packet_matches: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM context_packets WHERE id=? AND project_id=? AND operation_namespace=?)",
            params![packet_id, project_id, namespace], |r| r.get(0))?;
        if !packet_matches {
            return Err(invalid(
                "An assistant draft packet is missing or belongs elsewhere.",
            ));
        }
        let materialized: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversation_items
             WHERE conversation_id=? AND project_id=? AND operation_namespace=?
               AND kind='materializeChatResult' AND reference_id=?)",
            params![&conversation_id, &project_id, &namespace, &run_id],
            |r| r.get(0),
        )?;
        if !materialized {
            return Err(invalid("An assistant draft has no materialization event."));
        }
        if let Some(target_json) = target_json {
            let target: Value = json_value(&target_json, "assistant draft target")?;
            let (target_id, target_version, target_hash) =
                validate_head_json(&target, "assistant draft target")?;
            let (target_role, _kind, _working, _hash, _body, _checkpoint, _trashed) =
                document_role(db, &target_id)?;
            if target_role != DocumentRole::Ordinary {
                return Err(invalid(
                    "An assistant draft target is not an ordinary document.",
                ));
            }
            let target_revision: Option<(String, i64, String)> = db.query_row(
                "SELECT document_id,source_working_version,body_hash FROM revisions WHERE document_id=? AND source_working_version=?",
                params![target_id, target_version.parse::<i64>().map_err(|_| invalid("Target version is invalid."))?],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
            if target_revision
                .as_ref()
                .is_none_or(|(_, _, stored_hash)| stored_hash != &target_hash)
            {
                return Err(invalid(
                    "An assistant draft target has no matching historical revision.",
                ));
            }
        }
        if let Some(predecessor) = predecessor {
            id(&predecessor, "assistant draft predecessor")?;
            let predecessor_ok: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM assistant_drafts WHERE document_id=? AND conversation_id=? AND project_id=? AND operation_namespace=?)",
                params![predecessor, conversation_id, project_id, namespace], |r| r.get(0))?;
            if !predecessor_ok || predecessor == document_id {
                return Err(invalid("An assistant draft predecessor is invalid."));
            }
        }
    }
    Ok(())
}

fn validate_command_receipts(
    db: &Connection,
    owners: &HashMap<String, ConversationOwner>,
) -> CoreResult<()> {
    let mut query = db.prepare(
        "SELECT operation_namespace,operation_id,document_id,payload_hash,operation_kind,result_json
         FROM command_receipts WHERE operation_kind IN ('materializeChatResult','saveProjectComposer',
         'saveAssistantDraft','chatDisposition','adoptChatPreview')",
    )?;
    let rows = query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (namespace, operation_id, document_id, payload_hash, kind, result_json) = row?;
        id(&namespace, "receipt namespace")?;
        id(&operation_id, "receipt operation")?;
        hash(&payload_hash, "receipt payload")?;
        let result = json_value(&result_json, "chat receipt result")?;
        let linked_item: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversation_items
             WHERE operation_namespace=? AND operation_id=? AND kind IN
             ('materializeChatResult','saveProjectComposer','saveAssistantDraft',
              'chatDisposition','adoptionDecision'))",
            params![&namespace, &operation_id],
            |row| row.get(0),
        )?;
        if !linked_item {
            return Err(invalid(
                "A chat receipt has no immutable conversation event.",
            ));
        }
        if kind == "adoptChatPreview" {
            let receipt = result
                .as_object()
                .ok_or_else(|| invalid("An adoption receipt is not an object."))?;
            let preview_id = receipt
                .get("previewId")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("An adoption receipt has no preview."))?;
            let decision_id = receipt
                .get("decisionId")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("An adoption receipt has no decision."))?;
            id(preview_id, "adoption receipt preview")?;
            id(decision_id, "adoption receipt decision")?;
            let documents = receipt
                .get("documents")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("An adoption receipt has no documents."))?;
            if documents.is_empty() || documents.len() > 3 {
                return Err(invalid(
                    "An adoption receipt has an invalid document count.",
                ));
            }
            for document in documents {
                let object = document
                    .as_object()
                    .ok_or_else(|| invalid("An adoption receipt document is invalid."))?;
                let (document_id_ref, version_ref, hash_ref) = validate_head_json(
                    object
                        .get("head")
                        .ok_or_else(|| invalid("An adoption receipt document has no head."))?,
                    "adoption receipt head",
                )?;
                if object.get("role").and_then(Value::as_str)
                    != Some(DocumentRole::Ordinary.storage_name())
                {
                    return Err(invalid(
                        "An adoption receipt can only rehydrate ordinary documents.",
                    ));
                }
                let checkpoint = object
                    .get("lastCheckpointId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("An adoption receipt has no checkpoint."))?;
                validate_revision(db, checkpoint, &document_id_ref, Some(&hash_ref))?;
                let _ = version_ref;
            }
            if document_id.as_deref()
                != documents
                    .first()
                    .and_then(|document| document.get("head"))
                    .and_then(|head| head.get("documentId"))
                    .and_then(Value::as_str)
            {
                return Err(invalid(
                    "An adoption receipt head does not match its command receipt.",
                ));
            }
            continue;
        }
        let stored: wns_kernel::StoredResult = serde_json::from_value(result)
            .map_err(|error| invalid(format!("A chat receipt result is invalid: {error}")))?;
        if let Some(document_id) = document_id
            && stored.head.document_id != document_id
        {
            return Err(invalid("A chat receipt result has the wrong document."));
        }
        validate_head_json(&serde_json::to_value(&stored.head)?, "chat receipt head")?;
        version_text(&stored.saved_generation, "chat receipt generation")?;
        if !owners
            .values()
            .any(|owner| owner.operation_namespace == namespace)
        {
            // Historical receipt namespaces are retained after recovery.  A
            // chat owner relation is checked by its conversation item; this
            // branch only rejects an orphan namespace when no project-chat
            // projection can explain it.
            return Err(invalid("A chat receipt has no project-chat owner."));
        }
    }
    Ok(())
}

/// Validate all project-chat rows in the caller's consistent read snapshot.
/// Historical roots from a recovered project are accepted when their owner
/// relation is internally consistent; current access paths still require the
/// new project identity, so copied pending drafts cannot be auto-adopted.
pub(super) fn validate_storage(db: &Connection) -> CoreResult<()> {
    let orphan_role: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents d WHERE
         (d.role='assistantDraft' AND NOT EXISTS(SELECT 1 FROM assistant_drafts a WHERE a.document_id=d.id))
         OR (d.role='conversationAnchor' AND NOT EXISTS(SELECT 1 FROM project_conversations c WHERE c.anchor_document_id=d.id)))",
        [], |row| row.get(0),
    )?;
    if orphan_role {
        return Err(invalid(
            "An isolated draft or conversation anchor has no project-chat provenance.",
        ));
    }
    let current: (String, String) = db.query_row(
        "SELECT id,operation_namespace FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    id(&current.0, "project")?;
    id(&current.1, "project namespace")?;

    let mut owners = HashMap::new();
    let mut query = db.prepare(
        "SELECT id,project_id,operation_namespace,anchor_document_id,composer_version,composer_json,view_state_json
         FROM project_conversations ORDER BY id",
    )?;
    let rows = query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    for row in rows {
        let (
            conversation_id,
            project_id,
            namespace,
            anchor_id,
            composer_version,
            composer_json,
            view_state,
        ) = row?;
        id(&conversation_id, "conversation")?;
        id(&project_id, "conversation project")?;
        id(&namespace, "conversation namespace")?;
        id(&anchor_id, "conversation anchor")?;
        version(composer_version, "composer version")?;
        let owner = ConversationOwner {
            conversation_id: conversation_id.clone(),
            project_id,
            operation_namespace: namespace,
            anchor_document_id: anchor_id,
        };
        validate_anchor(db, &owner)?;
        validate_composer(db, &owner, &composer_json)?;
        if let Some(view_state) = view_state {
            if view_state.len() > 64 * 1024 {
                return Err(invalid("Project-chat view state is too large."));
            }
            let _ = json_value(&view_state, "project-chat view state")?;
        }
        if owners.insert(conversation_id, owner).is_some() {
            return Err(invalid("A project conversation identity is duplicated."));
        }
    }
    validate_conversation_items(db, &owners)?;
    validate_assistant_drafts(db, &owners)?;
    validate_command_receipts(db, &owners)?;
    for owner in owners.values() {
        let mut runs = db.prepare(
            "SELECT id FROM discussion_runs
             WHERE project_id=? AND operation_namespace=? AND target_document_id=?",
        )?;
        let rows = runs.query_map(
            params![
                &owner.project_id,
                &owner.operation_namespace,
                &owner.anchor_document_id
            ],
            |row| row.get::<_, String>(0),
        )?;
        for run in rows {
            let run = run?;
            let linked: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversation_items
                 WHERE conversation_id=? AND project_id=? AND operation_namespace=?
                   AND kind IN ('request','chapterRequest') AND reference_id=?)",
                params![
                    &owner.conversation_id,
                    &owner.project_id,
                    &owner.operation_namespace,
                    &run
                ],
                |row| row.get(0),
            )?;
            if !linked {
                return Err(invalid(
                    "A project-chat anchor run has no request history item.",
                ));
            }
        }
    }
    Ok(())
}
