//! Project-chat context ownership and draft/source fences.
//!
//! A project conversation is a durable application projection rooted at a
//! blank control anchor.  The anchor is useful for grouping runs, but it is
//! never a story source.  This module adds the small amount of typed metadata
//! needed to keep an ordinary working snapshot, an explicitly selected
//! unadopted draft, and the conversation projection distinct.

use super::conversation_context;
use super::project_chat::{ChatDispositionScope, ChatDispositionScopeKind, ChatUnknownTo};
use super::story_context::FrozenContext;
// Moved to wns-context (L2); see project_chat.rs for why. Re-exported at the
// historical path so `crate::projects::project_chat_context::{…}` resolves.
pub use wns_context::chat_vocabulary::{
    FrozenProjectChat, FrozenProjectChatDisposition, ProjectChatDraftRef,
};
use super::*;
use crate::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, SourceDescriptor, SourceKind,
    SourceRef, evaluate_sources,
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};

const MAX_PROJECT_CHAT_DISPOSITIONS: usize = 64;
const MAX_DISPOSITION_RATIONALE_BYTES: usize = 8 * 1024;

/// Exact ordinary heads explicitly attached to a project-chat request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectChatFreeze {
    pub conversation_id: String,
    #[serde(default)]
    pub source_refs: Vec<Head>,
    #[serde(default)]
    pub task_draft_refs: Vec<ProjectChatDraftRef>,
    /// New project-chat accepts freeze the response-prompt recipe explicitly.
    /// Historical snapshots omit this field and reproduce the legacy recipe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_recipe_version: Option<String>,
}

/// Freeze a project-level discussion on top of the ordinary working context.
/// The blank conversation anchor is the structural target; ordinary source
/// eligibility remains owned by the shared compiler. This function only
/// authenticates the project projection and appends explicitly requested
/// pending drafts as author-room task material.
pub(crate) fn freeze_project_chat_at(
    tx: &Connection,
    request: &crate::projects::story_context::FreezeStory,
    payload_hash: &str,
    chat: &ProjectChatFreeze,
) -> CoreResult<FrozenContext> {
    if request.basis != BasisKind::Working
        || request.purpose != ContextPurpose::Discuss
        || request.policy.audience != Audience::AuthorRoom
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "Project chat requires a Working author-room discussion context.",
        ));
    }
    super::story_context::freeze_project_chat_at(tx, request, payload_hash, chat)
}

/// Decorate an already-built working context before its immutable snapshot is
/// persisted.  Kept separate so the normal freeze path remains byte-stable.
pub(crate) fn augment_frozen_chat(
    tx: &Connection,
    request: &crate::projects::story_context::FreezeStory,
    frozen: &mut FrozenContext,
    chat: &ProjectChatFreeze,
) -> CoreResult<()> {
    check_id(&chat.conversation_id)?;
    let (project, namespace, anchor): (String, String, String) = tx
        .query_row(
            "SELECT project_id,operation_namespace,anchor_document_id FROM project_conversations WHERE id=?",
            [&chat.conversation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::new(
                "ProjectConversationNotFound",
                "The project conversation is not available in this project.",
            )
        })?;
    if project != request.access.project_id || namespace != request.access.operation_namespace {
        return Err(CoreError::new(
            "ProjectConversationMismatch",
            "The project conversation belongs to another project or operation namespace.",
        ));
    }
    let _anchor_document = require_blank_anchor(tx, &anchor)?;

    let mut seen_sources = HashSet::new();
    for head in &chat.source_refs {
        let document = read_document_with_role(tx, &head.document_id, DocumentRole::Ordinary)?;
        if document.head != *head {
            return Err(CoreError::new(
                "SourceChanged",
                "A project-chat source head is no longer current.",
            ));
        }
        if !seen_sources.insert(head.document_id.clone()) {
            return Err(CoreError::new(
                "DuplicateSource",
                "A project-chat source was attached more than once.",
            ));
        }
        let exists = frozen.snapshot.sources.iter().any(|source| {
            source.source.document_id == head.document_id
                && source.source.body_hash == head.body_hash
        });
        if !exists {
            return Err(CoreError::new(
                "SourceOutsideFrozenContext",
                "A project-chat source is not present in the frozen working context.",
            ));
        }
    }

    let mut seen_drafts = HashSet::new();
    for draft_ref in &chat.task_draft_refs {
        check_id(&draft_ref.head.document_id)?;
        let requested_version = parse_version(&draft_ref.disposition_version)?;
        if !seen_drafts.insert(draft_ref.head.document_id.clone()) {
            return Err(CoreError::new(
                "DuplicateDraft",
                "A project-chat draft was attached more than once.",
            ));
        }
        let row: Option<(String, String, String, i64)> = tx
            .query_row(
                "SELECT project_id,operation_namespace,disposition,disposition_version FROM assistant_drafts WHERE document_id=?",
                [&draft_ref.head.document_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((draft_project, draft_namespace, disposition, stored_version)) = row else {
            return Err(CoreError::new(
                "DraftNotFound",
                "The selected project-chat draft is not available.",
            ));
        };
        if draft_project != request.access.project_id
            || draft_namespace != request.access.operation_namespace
            || disposition != "pending"
            || stored_version != requested_version
        {
            return Err(CoreError::new(
                "DraftChanged",
                "The selected draft is no longer pending at the requested disposition version.",
            ));
        }
        let draft = read_document_with_role(
            tx,
            &draft_ref.head.document_id,
            DocumentRole::AssistantDraft,
        )?;
        if draft.head != draft_ref.head {
            return Err(CoreError::new(
                "DraftChanged",
                "The selected project-chat draft head is no longer current.",
            ));
        }
        let revision = checkpoint_at(tx, &draft, "projectChatContext")?;
        let source = SourceRef {
            project_id: request.access.project_id.clone(),
            document_id: revision.head.document_id.clone(),
            revision_id: revision.id.clone(),
            body_hash: revision.head.body_hash.clone(),
        };
        let descriptor = SourceDescriptor {
            handle: revision.id,
            source,
            display_name: draft.title,
            kind: SourceKind::AssistantDraft,
            current: true,
            coverage: CoverageLabel::Verbatim,
            disclosure: Disclosure {
                reader_position: None,
                visible_to_characters: Vec::new(),
                author_only: true,
                future_private: false,
            },
            story_time: None,
            dependencies: Vec::new(),
        };
        if frozen
            .snapshot
            .sources
            .iter()
            .any(|source| source.source == descriptor.source)
        {
            return Err(CoreError::new(
                "DuplicateSource",
                "A project-chat draft duplicates an existing frozen source.",
            ));
        }
        frozen.snapshot.sources.push(descriptor);
    }

    frozen.project_chat = Some(FrozenProjectChat {
        conversation_id: chat.conversation_id.clone(),
        anchor_document_id: anchor.clone(),
        operation_namespace: namespace.clone(),
        source_refs: chat.source_refs.clone(),
        task_draft_refs: chat.task_draft_refs.clone(),
        prompt_recipe_version: chat.prompt_recipe_version.clone(),
        dispositions: Vec::new(),
    });
    if frozen.snapshot.target.document_id != anchor
        || !frozen.snapshot.sources.iter().any(|source| {
            source.source == frozen.snapshot.target
                && source.kind == SourceKind::ConversationControl
        })
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "Project chat must freeze its blank conversation anchor as the structural target.",
        ));
    }
    frozen.conversation = conversation_context::select_project_conversation_at(
        tx,
        &request.access,
        &chat.conversation_id,
        &anchor,
        &request.policy.version,
    )?;
    let dispositions = collect_project_chat_dispositions(tx, frozen)?;
    frozen
        .project_chat
        .as_mut()
        .expect("project-chat metadata was just installed")
        .dispositions = dispositions;

    let handles: Vec<String> = frozen
        .snapshot
        .sources
        .iter()
        .map(|source| source.handle.clone())
        .collect();
    evaluate_sources(&frozen.snapshot, &frozen.policy, frozen.purpose, &handles)
        .map_err(|error| CoreError::new("ContextSourceDisallowed", &error.to_string()))?;
    Ok(())
}

fn parse_disposition_scope(payload: &Value) -> CoreResult<ChatDispositionScope> {
    let scope: ChatDispositionScope = payload
        .get("scope")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| {
            CoreError::new(
                "InvalidProjectChatContext",
                "A disposition scope is not valid JSON.",
            )
        })?
        .unwrap_or_default();
    scope.validate_shape().map_err(|_| {
        CoreError::new(
            "InvalidProjectChatContext",
            "A disposition scope is malformed.",
        )
    })?;
    Ok(scope)
}

fn parse_unknown_to(payload: &Value) -> CoreResult<Option<ChatUnknownTo>> {
    if payload.get("unknownTo").is_some_and(Value::is_null) {
        return Ok(None);
    }
    payload
        .get("unknownTo")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| {
            CoreError::new(
                "InvalidProjectChatContext",
                "A disposition unknownTo value is invalid.",
            )
        })
}

fn validate_disposition_scope_reference(
    db: &Connection,
    scope: &ChatDispositionScope,
    producer_run_id: &str,
) -> CoreResult<()> {
    scope.validate_shape().map_err(|_| {
        CoreError::new(
            "InvalidProjectChatContext",
            "A disposition scope is malformed.",
        )
    })?;
    match scope.kind {
        ChatDispositionScopeKind::Project => Ok(()),
        ChatDispositionScopeKind::Task => {
            if scope.reference_id.as_deref() != Some(producer_run_id) {
                return Err(CoreError::new(
                    "InvalidProjectChatContext",
                    "A task disposition scope must reference its producing run.",
                ));
            }
            Ok(())
        }
        ChatDispositionScopeKind::Chapter | ChatDispositionScopeKind::Document => {
            let document_id = scope.reference_id.as_deref().expect("validated scope");
            let document = read_document_with_role(db, document_id, DocumentRole::Ordinary)
                .map_err(|_| {
                    CoreError::new(
                        "InvalidProjectChatContext",
                        "A disposition scope references a missing or ineligible document.",
                    )
                })?;
            if scope.kind == ChatDispositionScopeKind::Chapter && document.kind != "chapter" {
                return Err(CoreError::new(
                    "InvalidProjectChatContext",
                    "A disposition scope references an ineligible document.",
                ));
            }
            Ok(())
        }
    }
}

fn disposition_scope_applies(
    scope: &ChatDispositionScope,
    frozen: &FrozenContext,
    _producer_run_id: &str,
) -> bool {
    let Some(chat) = frozen.project_chat.as_ref() else {
        return false;
    };
    match scope.kind {
        ChatDispositionScopeKind::Project => true,
        // The current project-chat freeze has no explicit task identity. A
        // prior run appearing in bounded history is not proof that the new
        // request is the same task, so task decisions stay out of the packet
        // until a future request carries an explicit task reference.
        ChatDispositionScopeKind::Task => false,
        ChatDispositionScopeKind::Chapter | ChatDispositionScopeKind::Document => {
            let Some(document_id) = scope.reference_id.as_deref() else {
                return false;
            };
            chat.source_refs
                .iter()
                .any(|head| head.document_id == document_id)
                || chat
                    .task_draft_refs
                    .iter()
                    .any(|draft| draft.head.document_id == document_id)
        }
    }
}

/// Resolve the latest question/assumption decisions against their original
/// completed run. The stored event contains only a reference and decision;
/// this projection adds the exact source output text and fingerprints needed
/// to make the next request inspectable without duplicating the transcript.
fn collect_project_chat_dispositions(
    db: &Connection,
    frozen: &FrozenContext,
) -> CoreResult<Vec<FrozenProjectChatDisposition>> {
    let chat = frozen.project_chat.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidProjectChatContext",
            "Project-chat metadata is missing.",
        )
    })?;
    let mut query = db.prepare(
        "SELECT id,reference_id,payload_json,payload_hash,project_id,operation_namespace
         FROM conversation_items
         WHERE conversation_id=? AND project_id=? AND operation_namespace=?
           AND kind='chatDisposition'
         ORDER BY sequence ASC",
    )?;
    let rows = query
        .query_map(
            rusqlite::params![
                chat.conversation_id,
                frozen.snapshot.project_id,
                chat.operation_namespace
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut latest = BTreeMap::<String, FrozenProjectChatDisposition>::new();
    for (item_id, reference_id, payload_json, payload_hash, project, namespace) in rows {
        if project != frozen.snapshot.project_id || namespace != chat.operation_namespace {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A disposition event has another project or operation namespace.",
            ));
        }
        if sha256_hex(payload_json.as_bytes()) != payload_hash {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A disposition event fingerprint changed.",
            ));
        }
        // Draft disposition events are already represented by the exact draft
        // refs above. Only response question/assumption decisions belong in
        // this compact context projection.
        let Some((producer_run_id, key)) = reference_id.split_once(':') else {
            continue;
        };
        check_id(producer_run_id)?;
        if key.is_empty() || key.len() > 128 || key.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A response disposition has an invalid response key.",
            ));
        }
        let payload: Value = serde_json::from_str(&payload_json).map_err(|_| {
            CoreError::new(
                "InvalidProjectChatContext",
                "A disposition event payload is not valid JSON.",
            )
        })?;
        if payload["referenceId"].as_str() != Some(reference_id.as_str()) {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A disposition event reference does not match its row.",
            ));
        }
        let disposition = payload["disposition"].as_str().ok_or_else(|| {
            CoreError::new(
                "InvalidProjectChatContext",
                "A disposition event has no decision value.",
            )
        })?;
        let version = payload["version"].as_str().ok_or_else(|| {
            CoreError::new(
                "InvalidProjectChatContext",
                "A disposition event has no version.",
            )
        })?;
        let parsed_version = parse_version(version)?;
        let rationale = payload["rationale"].as_str().unwrap_or_default();
        if rationale.len() > MAX_DISPOSITION_RATIONALE_BYTES {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A disposition rationale is too large.",
            ));
        }
        let scope = parse_disposition_scope(&payload)?;
        let unknown_to = parse_unknown_to(&payload)?;
        let (run_project, run_namespace, status, output_text): (String, String, String, String) = db
            .query_row(
                "SELECT project_id,operation_namespace,status,output_text FROM discussion_runs WHERE id=?",
                [producer_run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| {
                CoreError::new(
                    "InvalidProjectChatContext",
                    "A disposition producer run is missing.",
                )
            })?;
        if run_project != frozen.snapshot.project_id
            || run_namespace != chat.operation_namespace
            || status != "completed"
        {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A disposition producer run is not a completed owned response.",
            ));
        }
        let linked: bool = db.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM conversation_items
                WHERE conversation_id=? AND project_id=? AND operation_namespace=?
                  AND kind='request' AND reference_id=?
            )",
            rusqlite::params![
                chat.conversation_id,
                frozen.snapshot.project_id,
                chat.operation_namespace,
                producer_run_id
            ],
            |row| row.get(0),
        )?;
        if !linked {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A disposition producer is not linked to this project conversation.",
            ));
        }
        let mut allowed_handles: BTreeSet<String> = frozen
            .snapshot
            .sources
            .iter()
            .filter(|source| {
                source.kind != SourceKind::ConversationControl
                    && source.kind != SourceKind::AssistantDraft
            })
            .map(|source| source.handle.clone())
            .collect();
        let mut chapter_handles = BTreeSet::new();
        for source in &frozen.snapshot.sources {
            let (role, kind): (String, String) = db.query_row(
                "SELECT role,kind FROM documents WHERE id=? AND trashed=0",
                [&source.source.document_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if role == DocumentRole::Ordinary.storage_name() && kind == "chapter" {
                chapter_handles.insert(source.handle.clone());
            }
        }
        allowed_handles.retain(|handle| !chapter_handles.contains(handle));
        let predecessor_handles: BTreeSet<String> = chat
            .task_draft_refs
            .iter()
            .filter_map(|draft| {
                frozen.snapshot.sources.iter().find_map(|source| {
                    (source.kind == SourceKind::AssistantDraft
                        && source.source.document_id == draft.head.document_id
                        && source.source.body_hash == draft.head.body_hash)
                        .then(|| source.handle.clone())
                })
            })
            .collect();
        let output = crate::projects::project_chat_output::parse_project_assistant_output_with_predecessors_and_chapters(
            &output_text,
            &allowed_handles,
            &predecessor_handles,
            &chapter_handles,
        )?;
        let (item_kind, text) = if let Some(question) =
            output.questions.iter().find(|item| item.key == key)
        {
            if !matches!(
                disposition,
                "notNow" | "notRelevant" | "keepMysterious" | "reconsider"
            ) {
                return Err(CoreError::new(
                    "InvalidProjectChatContext",
                    "The question disposition is not valid.",
                ));
            }
            ("question", question.text.clone())
        } else if let Some(assumption) = output.assumptions.iter().find(|item| item.key == key) {
            if !matches!(disposition, "assumptionReject" | "reconsider") {
                return Err(CoreError::new(
                    "InvalidProjectChatContext",
                    "The assumption disposition is not valid.",
                ));
            }
            ("assumption", assumption.text.clone())
        } else {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A disposition key is absent from its producing output.",
            ));
        };
        validate_disposition_scope_reference(db, &scope, producer_run_id)?;
        if item_kind == "assumption"
            && (!matches!(scope.kind, ChatDispositionScopeKind::Project) || unknown_to.is_some())
        {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "Assumption dispositions are project-scoped and cannot carry unknownTo.",
            ));
        }
        if unknown_to.is_some() && disposition != "keepMysterious" {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "unknownTo is only valid for keepMysterious questions.",
            ));
        }
        if let Some(previous) = latest.get(&reference_id) {
            let previous_version = parse_version(&previous.version)?;
            if parsed_version != previous_version + 1 {
                return Err(CoreError::new(
                    "InvalidProjectChatContext",
                    "Disposition versions must advance exactly once.",
                ));
            }
        } else if parsed_version != 1 {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "The first disposition version must be one.",
            ));
        }
        latest.insert(
            reference_id.clone(),
            FrozenProjectChatDisposition {
                item_id,
                payload_hash,
                reference_id: reference_id.clone(),
                producer_run_id: producer_run_id.into(),
                key: key.into(),
                item_kind: item_kind.into(),
                text,
                disposition: disposition.into(),
                version: version.into(),
                rationale: rationale.into(),
                scope,
                unknown_to,
            },
        );
        if latest.len() > MAX_PROJECT_CHAT_DISPOSITIONS {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A project conversation has too many active disposition references.",
            ));
        }
    }
    Ok(latest
        .into_values()
        .filter(|decision| {
            disposition_scope_applies(&decision.scope, frozen, &decision.producer_run_id)
        })
        .collect())
}

/// Validate the durable project-chat identity when reopening or transferring
/// a frozen snapshot.  Historical copies keep their original namespace; only
/// a new dispatch may be checked against the caller's current access.
pub(crate) fn validate_frozen_project_chat(
    db: &Connection,
    frozen: &FrozenContext,
    snapshot_namespace: &str,
) -> CoreResult<()> {
    let Some(chat) = frozen.project_chat.as_ref() else {
        return Ok(());
    };
    if chat.operation_namespace != snapshot_namespace
        || chat.operation_namespace.is_empty()
        || chat.conversation_id.is_empty()
        || chat.anchor_document_id.is_empty()
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "The frozen project-chat identity is incomplete or has the wrong namespace.",
        ));
    }
    let row: Option<(String, String, String)> = db
        .query_row(
            "SELECT project_id,operation_namespace,anchor_document_id FROM project_conversations WHERE id=?",
            [&chat.conversation_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((project, namespace, anchor)) = row else {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "The frozen project conversation is missing.",
        ));
    };
    if project != frozen.snapshot.project_id
        || namespace != snapshot_namespace
        || anchor != chat.anchor_document_id
        || frozen.snapshot.target.document_id != chat.anchor_document_id
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "The frozen project conversation does not match its project or anchor.",
        ));
    }
    let _anchor_document = require_blank_anchor(db, &chat.anchor_document_id).map_err(|_| {
        CoreError::new(
            "InvalidProjectChatContext",
            "The project-chat anchor is not a blank control document.",
        )
    })?;
    let target = frozen
        .snapshot
        .sources
        .iter()
        .find(|source| source.source == frozen.snapshot.target)
        .ok_or_else(|| {
            CoreError::new(
                "InvalidProjectChatContext",
                "The frozen control target is missing.",
            )
        })?;
    if target.kind != SourceKind::ConversationControl {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "The frozen project-chat target is not a control anchor.",
        ));
    }
    let draft_descriptors: Vec<_> = frozen
        .snapshot
        .sources
        .iter()
        .filter(|source| source.kind == SourceKind::AssistantDraft)
        .collect();
    if draft_descriptors.len() != chat.task_draft_refs.len()
        || draft_descriptors.iter().any(|source| {
            !chat.task_draft_refs.iter().any(|draft| {
                draft.head.document_id == source.source.document_id
                    && draft.head.body_hash == source.source.body_hash
            })
        })
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "The frozen assistant-draft source set does not match its explicit task references.",
        ));
    }
    for draft in &chat.task_draft_refs {
        let (project, namespace, _disposition, version): (String, String, String, i64) = db
            .query_row(
                "SELECT project_id,operation_namespace,disposition,disposition_version FROM assistant_drafts WHERE document_id=?",
                [&draft.head.document_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        // A retained historical snapshot remains readable after an author
        // rejects or edits the draft. New dispatches use `augment_frozen_chat`
        // above, which still requires the exact pending disposition/head.
        if project != frozen.snapshot.project_id
            || namespace != snapshot_namespace
            || version < parse_version(&draft.disposition_version)?
        {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen project-chat draft no longer matches its disposition.",
            ));
        }
        let stored_role: String = db.query_row(
            "SELECT role FROM documents WHERE id=?",
            [&draft.head.document_id],
            |r| r.get(0),
        )?;
        if stored_role != DocumentRole::AssistantDraft.storage_name() {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen project-chat draft no longer has its isolated authority role.",
            ));
        }
    }
    validate_frozen_dispositions(db, frozen, chat)?;
    Ok(())
}

/// Freshness is separate from historical integrity: edits and dispositions
/// never make old evidence unreadable, but they do revoke a queued dispatch or
/// an adoption based on the prior task material.
pub(crate) fn project_chat_basis_is_current(
    db: &Connection,
    frozen: &FrozenContext,
) -> CoreResult<bool> {
    let Some(chat) = &frozen.project_chat else {
        return Ok(true);
    };
    for source in &chat.source_refs {
        let current = read_document(db, &source.document_id)?;
        if current.head != *source {
            return Ok(false);
        }
    }
    for reference in &chat.task_draft_refs {
        let current = read_document_with_role(
            db,
            &reference.head.document_id,
            DocumentRole::AssistantDraft,
        )?;
        let (status, version): (String,i64) = db.query_row("SELECT disposition,disposition_version FROM assistant_drafts WHERE document_id=? AND conversation_id=?", params![reference.head.document_id,chat.conversation_id], |r| Ok((r.get(0)?,r.get(1)?)))?;
        if current.head != reference.head
            || status != "pending"
            || version.to_string() != reference.disposition_version
        {
            return Ok(false);
        }
    }
    Ok(collect_project_chat_dispositions(db, frozen)? == chat.dispositions)
}

fn validate_frozen_dispositions(
    db: &Connection,
    frozen: &FrozenContext,
    chat: &FrozenProjectChat,
) -> CoreResult<()> {
    if chat.dispositions.len() > MAX_PROJECT_CHAT_DISPOSITIONS {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "The frozen project-chat disposition projection is too large.",
        ));
    }
    let mut references = HashSet::new();
    for decision in &chat.dispositions {
        check_id(&decision.item_id)?;
        if !references.insert(&decision.reference_id) {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "The frozen project-chat disposition references a duplicate item.",
            ));
        }
        let row: Option<(String, String, String, String, String, String)> = db
            .query_row(
                "SELECT conversation_id,project_id,operation_namespace,kind,reference_id,payload_json
                 FROM conversation_items WHERE id=?",
                [&decision.item_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((conversation, project, namespace, kind, reference, payload_json)) = row else {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen disposition event is missing.",
            ));
        };
        if conversation != chat.conversation_id
            || project != frozen.snapshot.project_id
            || namespace != chat.operation_namespace
            || kind != "chatDisposition"
            || reference != decision.reference_id
            || sha256_hex(payload_json.as_bytes()) != decision.payload_hash
        {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen disposition event no longer matches its provenance.",
            ));
        }
        let payload: Value = serde_json::from_str(&payload_json).map_err(|_| {
            CoreError::new(
                "InvalidProjectChatContext",
                "A frozen disposition event payload is invalid.",
            )
        })?;
        let payload_scope = parse_disposition_scope(&payload)?;
        let payload_unknown_to = parse_unknown_to(&payload)?;
        if payload["referenceId"].as_str() != Some(decision.reference_id.as_str())
            || payload["disposition"].as_str() != Some(decision.disposition.as_str())
            || payload["version"].as_str() != Some(decision.version.as_str())
            || payload["rationale"].as_str() != Some(decision.rationale.as_str())
            || payload_scope != decision.scope
            || payload_unknown_to != decision.unknown_to
        {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen disposition event payload changed.",
            ));
        }
        parse_version(&decision.version)?;
        let Some((producer_run_id, key)) = decision.reference_id.split_once(':') else {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen response disposition has no producing run.",
            ));
        };
        if producer_run_id != decision.producer_run_id || key != decision.key {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen disposition producer reference changed.",
            ));
        }
        let (run_project, run_namespace): (String, String) = db.query_row(
            "SELECT project_id,operation_namespace FROM discussion_runs WHERE id=?",
            [producer_run_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if run_project != frozen.snapshot.project_id || run_namespace != chat.operation_namespace {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen disposition producer has another project identity.",
            ));
        }
        if !matches!(decision.item_kind.as_str(), "question" | "assumption")
            || (decision.item_kind == "question"
                && !matches!(
                    decision.disposition.as_str(),
                    "notNow" | "notRelevant" | "keepMysterious" | "reconsider"
                ))
            || (decision.item_kind == "assumption"
                && !matches!(
                    decision.disposition.as_str(),
                    "assumptionReject" | "reconsider"
                ))
        {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen disposition has an invalid item or status.",
            ));
        }
        validate_disposition_scope_reference(db, &decision.scope, &decision.producer_run_id)?;
        if (decision.item_kind == "assumption"
            && (!matches!(decision.scope.kind, ChatDispositionScopeKind::Project)
                || decision.unknown_to.is_some()))
            || (decision.unknown_to.is_some() && decision.disposition != "keepMysterious")
        {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen disposition has an invalid scope or unknownTo value.",
            ));
        }
        if !disposition_scope_applies(&decision.scope, frozen, &decision.producer_run_id) {
            return Err(CoreError::new(
                "InvalidProjectChatContext",
                "A frozen disposition is outside the current project-chat scope.",
            ));
        }
    }
    Ok(())
}

/// Read and structurally validate the control anchor. A role label alone is
/// insufficient: a forged or accidentally edited anchor must not become the
/// target of a project-chat request.
fn require_blank_anchor(db: &Connection, document_id: &str) -> CoreResult<DocumentRecord> {
    let document = read_document_with_role(db, document_id, DocumentRole::ConversationAnchor)?;
    let blocks = document.body["body"]["content"].as_array();
    let blank = document.kind == "note"
        && blocks.is_some_and(|blocks| {
            blocks.len() == 1
                && blocks[0]["type"] == "paragraph"
                && blocks[0]
                    .get("content")
                    .and_then(Value::as_array)
                    .is_none_or(|content| content.is_empty())
        });
    if !blank {
        return Err(CoreError::new(
            "InvalidProjectChat",
            "The project conversation control anchor must remain blank.",
        ));
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{
        Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, EligibilityErrorCode,
        InformationPolicy, StorySnapshot,
    };
    use rusqlite::Connection;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn source(document_id: &str, revision_id: &str, body_hash: &str) -> SourceRef {
        SourceRef {
            project_id: "project".into(),
            document_id: document_id.into(),
            revision_id: revision_id.into(),
            body_hash: body_hash.into(),
        }
    }

    fn descriptor(source: SourceRef, kind: SourceKind, author_only: bool) -> SourceDescriptor {
        SourceDescriptor {
            handle: source.revision_id.clone(),
            source,
            display_name: "fixture".into(),
            kind,
            current: true,
            coverage: CoverageLabel::Verbatim,
            disclosure: Disclosure {
                reader_position: Some("1".into()),
                visible_to_characters: Vec::new(),
                author_only,
                future_private: false,
            },
            story_time: None,
            dependencies: Vec::new(),
        }
    }

    fn policy(audience: Audience) -> InformationPolicy {
        InformationPolicy {
            version: "0".into(),
            audience,
            reader_frontier: (audience == Audience::RestrictedWriting).then(|| "1".into()),
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        }
    }

    fn snapshot(target: SourceRef, sources: Vec<SourceDescriptor>) -> StorySnapshot {
        StorySnapshot {
            snapshot_id: "snapshot".into(),
            project_id: "project".into(),
            basis: BasisKind::Working,
            target,
            context_source_epoch: "0".into(),
            ordering_epoch: "0".into(),
            disclosure_policy_version: "0".into(),
            sources,
            reviewed_basis: None,
        }
    }

    fn frozen(
        snapshot: StorySnapshot,
        purpose: ContextPurpose,
        policy: InformationPolicy,
    ) -> FrozenContext {
        FrozenContext {
            snapshot,
            policy,
            purpose,
            aliases: BTreeMap::new(),
            excluded_source_count: 0,
            guidance: Vec::new(),
            conversation: None,
            navigation_views: Vec::new(),
            reviewed_evidence: Vec::new(),
            reviewed_promises: Vec::new(),
            reviewed_knowledge: Vec::new(),
            reviewed_summaries: Vec::new(),
            project_chat: None,
        }
    }

    fn valid_body(text: Option<&str>) -> (String, String) {
        let body = match text {
            Some(text) => json!({
                "schemaVersion": 1,
                "body": {"type": "doc", "content": [{
                    "type": "paragraph", "attrs": {"id": "p"},
                    "content": [{"type": "text", "text": text}]
                }]}
            }),
            None => json!({
                "schemaVersion": 1,
                "body": {"type": "doc", "content": [{
                    "type": "paragraph", "attrs": {"id": "p"}
                }]}
            }),
        };
        let validated =
            crate::validate_snapshot_json(&body.to_string()).expect("valid fixture body");
        (validated.canonical_json, validated.hash)
    }

    fn role_fixture() -> Connection {
        let db = Connection::open_in_memory().expect("open fixture db");
        db.execute_batch(
            "CREATE TABLE documents(
                id TEXT PRIMARY KEY, title TEXT NOT NULL, kind TEXT NOT NULL,
                working_version INTEGER NOT NULL, metadata_version INTEGER NOT NULL,
                body_hash TEXT NOT NULL, body_json TEXT NOT NULL,
                last_checkpoint_id TEXT, role TEXT NOT NULL,
                trashed INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE project_conversations(
                id TEXT PRIMARY KEY, project_id TEXT NOT NULL,
                operation_namespace TEXT NOT NULL, anchor_document_id TEXT NOT NULL
            );
            CREATE TABLE assistant_drafts(
                document_id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL,
                project_id TEXT NOT NULL, operation_namespace TEXT NOT NULL,
                origin_run_id TEXT NOT NULL, output_ordinal INTEGER NOT NULL,
                packet_id TEXT NOT NULL, source_epoch INTEGER NOT NULL,
                policy_epoch INTEGER NOT NULL, target_json TEXT,
                initial_revision_id TEXT NOT NULL, predecessor_document_id TEXT,
                disposition TEXT NOT NULL, disposition_version INTEGER NOT NULL
            );",
        )
        .expect("create fixture tables");
        db
    }

    fn insert_document(
        db: &Connection,
        id: &str,
        kind: &str,
        role: DocumentRole,
        version: i64,
        body: &(String, String),
    ) {
        db.execute(
            "INSERT INTO documents(id,title,kind,working_version,metadata_version,body_hash,body_json,role)
             VALUES(?,?,?,?,?,?,?,?)",
            rusqlite::params![
                id,
                id,
                kind,
                version,
                "1",
                body.1,
                body.0,
                role.storage_name()
            ],
        )
        .expect("insert fixture document");
    }

    #[test]
    fn restricted_policy_rejects_explicit_assistant_draft_source() {
        let draft = source("draft", "draft-revision", &"a".repeat(64));
        let story = snapshot(
            draft.clone(),
            vec![descriptor(draft, SourceKind::AssistantDraft, true)],
        );
        let error = evaluate_sources(
            &story,
            &policy(Audience::RestrictedWriting),
            ContextPurpose::Revise,
            &["draft-revision".into()],
        )
        .expect_err("unadopted draft must not enter restricted writing");
        assert_eq!(error.code, EligibilityErrorCode::PrivateSource);
    }

    #[test]
    fn forged_control_source_without_project_chat_metadata_is_rejected() {
        let anchor = source("anchor", "anchor-revision", &"b".repeat(64));
        let context = frozen(
            snapshot(
                anchor.clone(),
                vec![descriptor(anchor, SourceKind::ConversationControl, true)],
            ),
            ContextPurpose::Discuss,
            policy(Audience::AuthorRoom),
        );
        let json = serde_json::to_string(&context).expect("serialize context");
        let error = super::super::story_context::decode_snapshot(
            &json,
            &crate::sha256_hex(json.as_bytes()),
        )
        .expect_err("control metadata cannot be forged by an ordinary packet");
        assert_eq!(error.code, "InvalidProjectChatContext");
    }

    #[test]
    fn conversation_anchor_must_remain_a_blank_note() {
        let db = role_fixture();
        let nonblank = valid_body(Some("edited anchor"));
        insert_document(
            &db,
            "anchor",
            "note",
            DocumentRole::ConversationAnchor,
            0,
            &nonblank,
        );
        let error = require_blank_anchor(&db, "anchor")
            .expect_err("a nonblank control anchor must be refused");
        assert_eq!(error.code, "InvalidProjectChat");
    }

    #[test]
    fn edited_ordinary_source_head_is_rejected_before_project_chat_freeze() {
        let db = role_fixture();
        let anchor_body = valid_body(None);
        let current_body = valid_body(Some("new source"));
        insert_document(
            &db,
            "anchor",
            "note",
            DocumentRole::ConversationAnchor,
            0,
            &anchor_body,
        );
        insert_document(
            &db,
            "source",
            "world",
            DocumentRole::Ordinary,
            2,
            &current_body,
        );
        db.execute(
            "INSERT INTO project_conversations(id,project_id,operation_namespace,anchor_document_id)
             VALUES('conversation','project','namespace','anchor')",
            [],
        )
        .expect("insert conversation");
        let old_head = Head {
            document_id: "source".into(),
            version: "1".into(),
            body_hash: "a".repeat(64),
        };
        let anchor_ref = source("anchor", "anchor-revision", &anchor_body.1);
        let source_ref = source("source", "source-revision-v1", &old_head.body_hash);
        let mut context = frozen(
            snapshot(
                anchor_ref,
                vec![
                    descriptor(source_ref, SourceKind::CurrentDraft, false),
                    descriptor(
                        source("anchor", "anchor-revision", &anchor_body.1),
                        SourceKind::ConversationControl,
                        true,
                    ),
                ],
            ),
            ContextPurpose::Discuss,
            policy(Audience::AuthorRoom),
        );
        let request = crate::projects::story_context::FreezeStory {
            access: ProjectAccess {
                project_id: "project".into(),
                session: "session".into(),
                writer_lease: "lease".into(),
                operation_namespace: "namespace".into(),
            },
            operation_id: "operation".into(),
            expected: Head {
                document_id: "anchor".into(),
                version: "0".into(),
                body_hash: anchor_body.1.clone(),
            },
            basis: BasisKind::Working,
            purpose: ContextPurpose::Discuss,
            policy: policy(Audience::AuthorRoom),
        };
        let chat = ProjectChatFreeze {
            conversation_id: "conversation".into(),
            source_refs: vec![old_head],
            task_draft_refs: Vec::new(),
            prompt_recipe_version: None,
        };
        let error = augment_frozen_chat(&db, &request, &mut context, &chat)
            .expect_err("an edited source must invalidate a new chat request");
        assert_eq!(error.code, "SourceChanged");
    }

    #[test]
    fn frozen_assistant_draft_requires_the_isolated_document_role() {
        let db = role_fixture();
        let anchor_body = valid_body(None);
        let draft_body = valid_body(Some("candidate"));
        insert_document(
            &db,
            "anchor",
            "note",
            DocumentRole::ConversationAnchor,
            0,
            &anchor_body,
        );
        // Deliberately install the candidate as an ordinary document. A
        // forged assistant_drafts row must not upgrade that row's authority.
        insert_document(
            &db,
            "draft",
            "world",
            DocumentRole::Ordinary,
            1,
            &draft_body,
        );
        db.execute(
            "INSERT INTO project_conversations(id,project_id,operation_namespace,anchor_document_id)
             VALUES('conversation','project','namespace','anchor')",
            [],
        )
        .expect("insert conversation");
        db.execute(
            "INSERT INTO assistant_drafts(
                document_id,conversation_id,project_id,operation_namespace,
                origin_run_id,output_ordinal,packet_id,source_epoch,policy_epoch,
                target_json,initial_revision_id,predecessor_document_id,disposition,disposition_version
             ) VALUES('draft','conversation','project','namespace','run',0,'packet',0,0,NULL,'revision',NULL,'pending',0)",
            [],
        )
        .expect("insert draft provenance");
        let anchor_ref = source("anchor", "anchor-revision", &anchor_body.1);
        let draft_ref = source("draft", "draft-revision", &draft_body.1);
        let mut context = frozen(
            snapshot(
                anchor_ref.clone(),
                vec![
                    descriptor(anchor_ref, SourceKind::ConversationControl, true),
                    descriptor(draft_ref.clone(), SourceKind::AssistantDraft, true),
                ],
            ),
            ContextPurpose::Discuss,
            policy(Audience::AuthorRoom),
        );
        context.project_chat = Some(FrozenProjectChat {
            conversation_id: "conversation".into(),
            anchor_document_id: "anchor".into(),
            operation_namespace: "namespace".into(),
            source_refs: Vec::new(),
            task_draft_refs: vec![ProjectChatDraftRef {
                head: Head {
                    document_id: "draft".into(),
                    version: "1".into(),
                    body_hash: draft_body.1,
                },
                disposition_version: "0".into(),
            }],
            prompt_recipe_version: None,
            dispositions: Vec::new(),
        });
        let error = validate_frozen_project_chat(&db, &context, "namespace")
            .expect_err("ordinary rows cannot masquerade as assistant drafts");
        assert_eq!(error.code, "InvalidProjectChatContext");
    }
}
