//! The frozen context a packet is compiled from, and bounded search over it.
//!
//! Moved down from `projects/story_context.rs` as the core of the
//! packet-compiler inversion (`docs/V3_ARCHITECTURE_MODULAR.md` §3.4): these are
//! the shapes the compiler *consumes*, so leaving them above it is exactly what
//! made it reach upward for its own inputs.
//!
//! `search_saved_passages` and its `literal_spans` helper came with them because
//! both are pure — they take the frozen context plus a passage-reader closure
//! and never open a database. The operations that DO read SQLite stayed in
//! `projects`, and that split — vocabulary down, operations up — is what the
//! inversion turns on.

use crate::chat_vocabulary::{
    ChatDispositionScope, ChatDispositionScopeKind, ChatUnknownTo, FrozenProjectChat,
    FrozenProjectChatDisposition, ProjectChatFreeze,
};
use crate::conversation::FrozenConversation;
use crate::guidance::FrozenGuidance;
use crate::navigation::FrozenNavigationView;
use crate::reviewed_evidence::ReviewedEvidenceSet;
use crate::reviewed_knowledge::ReviewedKnowledgeSet;
use crate::reviewed_promises::ReviewedPromiseSet;
use crate::reviewed_summaries::ReviewedSummarySet;
use crate::contracts::{BasisKind, SourceKind};
use crate::conversation::validate_conversation;
use crate::eligibility::{EligibilityError, EligibilityReceipt, evaluate_sources};
use crate::guidance::validate_frozen_guidance;
use crate::navigation::validate_frozen_navigation_views;
use crate::reviewed_knowledge::validate_frozen_knowledge_set;
use crate::reviewed_promises::validate_frozen_promise_set;
use crate::reviewed_summaries::validate_frozen_set as validate_frozen_summary;
use crate::{
    Audience, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy, SourceDescriptor,
    SourceRef, StorySnapshot,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use wns_kernel::{
    CoreError, CoreResult, DocumentRecord, DocumentRole, Head, ProjectAccess, check_id,
    parse_version,
    sha256_hex,
};
use wns_storage::{checkpoint_at, read_document, read_document_with_role};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenContext {
    pub snapshot: StorySnapshot,
    pub policy: InformationPolicy,
    pub purpose: ContextPurpose,
    pub aliases: BTreeMap<String, Vec<String>>,
    /// No titles or text from excluded material are exposed to a writing packet.
    pub excluded_source_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guidance: Vec<FrozenGuidance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<FrozenConversation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub navigation_views: Vec<FrozenNavigationView>,
    /// Complete author-reviewed record sets selected from immutable bundles.
    /// Empty legacy snapshots omit this field and retain their original JSON.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewed_evidence: Vec<ReviewedEvidenceSet>,
    /// Complete author-reviewed promise sets selected from immutable bundles.
    /// Empty legacy snapshots omit this field and retain their original JSON.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewed_promises: Vec<ReviewedPromiseSet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewed_knowledge: Vec<ReviewedKnowledgeSet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewed_summaries: Vec<ReviewedSummarySet>,
    /// Present only for a project-level author-room discussion. Ordinary
    /// snapshots omit this field so their historical manifest bytes remain
    /// stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_chat: Option<FrozenProjectChat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePassage {
    pub handle: String,
    pub source: SourceRef,
    pub block_id: String,
    pub block_order: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRead {
    pub descriptor: SourceDescriptor,
    pub passages: Vec<SourcePassage>,
    pub body: Value,
    pub used_validated_projection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    Literal,
    Lexical,
    ExactAlias,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchStory {
    pub access: ProjectAccess,
    pub snapshot_id: String,
    pub query: String,
    pub mode: SearchMode,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchHit {
    pub passage: SourcePassage,
    pub start_utf16: u32,
    pub end_utf16: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchResult {
    pub snapshot_id: String,
    pub hits: Vec<SearchHit>,
    /// Alias/title matches identify a source, not an occurrence in its prose.
    pub source_matches: Vec<SourceDescriptor>,
    pub searched_sources: u32,
    pub has_more: bool,
    /// Retrieval reports where it looked, never that an event did not happen.
    pub coverage: String,
}
/// validation uses the same matcher without opening a database or granting
/// access to additional sources.
pub fn search_saved_passages(
    frozen: &FrozenContext,
    query: &str,
    mode: SearchMode,
    limit: u32,
    mut passages: impl FnMut(&str) -> CoreResult<Vec<SourcePassage>>,
) -> CoreResult<SearchResult> {
    if query.trim().is_empty() || !(1..=100).contains(&limit) {
        return Err(CoreError::new(
            "InvalidSearch",
            "A bounded nonempty search is required.",
        ));
    }
    let mut hits = Vec::new();
    let mut source_matches = Vec::new();
    let normalized = query.to_lowercase();
    let mut has_more = false;
    for source in &frozen.snapshot.sources {
        let alias = source.display_name.to_lowercase() == normalized
            || frozen
                .aliases
                .get(&source.handle)
                .is_some_and(|names| names.iter().any(|name| name.to_lowercase() == normalized));
        if matches!(mode, SearchMode::ExactAlias) {
            if alias {
                if source_matches.len() == limit as usize {
                    has_more = true;
                } else {
                    source_matches.push(source.clone());
                }
            }
            continue;
        }
        for passage in passages(&source.handle)? {
            let spans = match mode {
                SearchMode::ExactAlias => {
                    unreachable!("alias matching returns source descriptors")
                }
                SearchMode::Literal => literal_spans(&passage.text, &normalized),
                SearchMode::Lexical => {
                    let terms: Vec<_> = normalized.split_whitespace().collect();
                    if terms
                        .iter()
                        .all(|term| passage.text.to_lowercase().contains(term))
                    {
                        literal_spans(&passage.text, terms[0])
                    } else {
                        Vec::new()
                    }
                }
            };
            for (start_utf16, end_utf16) in spans {
                if hits.len() == limit as usize {
                    has_more = true;
                    break;
                }
                hits.push(SearchHit {
                    passage: passage.clone(),
                    start_utf16,
                    end_utf16,
                });
            }
        }
    }
    Ok(SearchResult { snapshot_id: frozen.snapshot.snapshot_id.clone(), hits, source_matches, searched_sources: frozen.snapshot.sources.len() as u32, has_more, coverage: "Exact eligible saved sources; a missing match does not establish that an event never happened.".into() })
}

/// Unicode lowercase can expand a character, so retain an original UTF-16 map
/// instead of treating normalized UTF-8 byte positions as editor offsets.
fn literal_spans(text: &str, query: &str) -> Vec<(u32, u32)> {
    let mut normalized = String::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut offset = 0;
    for character in text.chars() {
        let next = offset + character.len_utf16() as u32;
        let lowered: String = character.to_lowercase().collect();
        starts.extend(std::iter::repeat_n(offset, lowered.len()));
        ends.extend(std::iter::repeat_n(next, lowered.len()));
        normalized.push_str(&lowered);
        offset = next;
    }
    normalized
        .match_indices(query)
        .map(|(at, matched)| (starts[at], ends[at + matched.len() - 1]))
        .collect()
}

// ---------------------------------------------------------------------------
// Decoding and eligibility.
//
// Moved down from `projects/story_context.rs`. `decode_snapshot` is the only
// constructor of a [`FrozenContext`] from stored bytes, and every invariant the
// frozen context must satisfy is enforced inside it — so it belongs beside the
// type rather than beside the module that writes the row.
//
// It had to move for the same reason the vocabulary did. `conversation_context`
// rebuilds a turn by decoding a manifest, and `story_context` freezes a
// conversation by selecting one, so whichever crate held the decoder sat above
// the other. At L3 it sits below both.
// ---------------------------------------------------------------------------

pub fn eligibility_error(error: EligibilityError) -> CoreError {
    CoreError::new("ContextSourceDisallowed", &error.to_string())
}

pub fn eligibility(
    snapshot: &StorySnapshot,
    policy: &InformationPolicy,
    purpose: ContextPurpose,
    handles: &[String],
) -> Result<EligibilityReceipt, EligibilityError> {
    evaluate_sources(snapshot, policy, purpose, handles)
}

pub fn decode_snapshot(json: &str, hash: &str) -> CoreResult<FrozenContext> {
    if sha256_hex(json.as_bytes()) != hash {
        return Err(CoreError::new(
            "InvalidContext",
            "The context manifest failed its fingerprint check.",
        ));
    }
    let frozen: FrozenContext =
        serde_json::from_str(json).map_err(|e| CoreError::new("InvalidContext", &e.to_string()))?;
    if let Some(chat) = &frozen.project_chat
        && (frozen.snapshot.basis != BasisKind::Working
            || frozen.purpose != ContextPurpose::Discuss
            || frozen.policy.audience != Audience::AuthorRoom
            || chat.conversation_id.is_empty()
            || chat.anchor_document_id.is_empty()
            || chat.operation_namespace.is_empty())
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "Project-chat metadata is only valid for a Working author-room discussion.",
        ));
    }
    let control_sources: Vec<_> = frozen
        .snapshot
        .sources
        .iter()
        .filter(|source| source.kind == SourceKind::ConversationControl)
        .collect();
    if !control_sources.is_empty()
        && (frozen.project_chat.is_none()
            || control_sources.len() != 1
            || control_sources[0].source != frozen.snapshot.target)
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "A conversation control anchor may appear only as the project-chat target.",
        ));
    }
    if frozen.project_chat.is_none()
        && frozen
            .snapshot
            .sources
            .iter()
            .any(|source| source.kind == SourceKind::AssistantDraft)
    {
        return Err(CoreError::new(
            "InvalidProjectChatContext",
            "An assistant draft source requires explicit project-chat metadata.",
        ));
    }
    if frozen.purpose == ContextPurpose::MemoryAnalysis
        && (!frozen.aliases.is_empty()
            || !frozen.guidance.is_empty()
            || frozen.conversation.is_some())
    {
        return Err(CoreError::new(
            "InvalidContext",
            "Chapter memory cannot include aliases, guidance, or discussion.",
        ));
    }
    validate_conversation(
        frozen.conversation.as_ref(),
        &frozen.snapshot.project_id,
        &frozen.snapshot.target.document_id,
        &frozen.policy.version,
        frozen.policy.audience,
        frozen.purpose,
    )
    .map_err(|message| CoreError::new("InvalidConversationContext", &message))?;
    if frozen.snapshot.ordering_epoch != frozen.snapshot.context_source_epoch
        || frozen.snapshot.disclosure_policy_version != frozen.policy.version
    {
        return Err(CoreError::new(
            "InvalidContext",
            "The frozen ordering or disclosure epoch is inconsistent.",
        ));
    }
    if frozen.policy.audience == Audience::RestrictedWriting && !frozen.aliases.is_empty() {
        return Err(CoreError::new(
            "InvalidContext",
            "Unclassified aliases cannot enter restricted writing context.",
        ));
    }
    validate_frozen_navigation_views(
        &frozen.navigation_views,
        &frozen.snapshot,
        &frozen.policy,
        frozen.purpose,
    )?;
    for promises in &frozen.reviewed_promises {
        validate_frozen_promise_set(promises, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for knowledge in &frozen.reviewed_knowledge {
        validate_frozen_knowledge_set(knowledge, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    for summary in &frozen.reviewed_summaries {
        validate_frozen_summary(summary, &frozen.snapshot, &frozen.policy, frozen.purpose)?;
    }
    validate_frozen_guidance(
        &frozen.guidance,
        &frozen.snapshot.project_id,
        &frozen.snapshot.target.document_id,
        frozen.policy.audience,
    )
    .map_err(|message| CoreError::new("InvalidContext", &message))?;
    let selected: Vec<_> = frozen
        .snapshot
        .sources
        .iter()
        .map(|source| source.handle.clone())
        .collect();
    eligibility(&frozen.snapshot, &frozen.policy, frozen.purpose, &selected)
        .map_err(eligibility_error)?;
    if frozen
        .aliases
        .keys()
        .any(|handle| !selected.contains(handle))
    {
        return Err(CoreError::new(
            "InvalidContext",
            "An alias refers to a source outside the manifest.",
        ));
    }
    Ok(frozen)
}

// ---------------------------------------------------------------------------
// Frozen project-chat validation.
//
// Moved down from `webnovel-core::projects::project_chat_context` (1,310
// lines, bound for `wns-conversation` at L5). `story_context` calls
// `validate_frozen_project_chat` from `validate_pins`, which is what makes a
// snapshot record *validated* — so at L5 it was an upward call that blocked
// `story_context` from ever reaching `wns-story`.
//
// It is not one function. Measured by transitive closure it is eight, 696
// lines: the validators, the disposition projection they read, and the
// reference parsers both use. An attempt to move only the three validators
// failed to compile, which is how the missing five were found.
//
// They belong here on their own merits: this is the same family as
// `validate_frozen_guidance`, `validate_frozen_navigation_views`,
// `validate_frozen_promise_set` and `validate_frozen_summary` above, and they
// take the same `&FrozenContext` plus a connection.
//
// What stays in core is the freeze side — `freeze_project_chat_at`,
// `augment_frozen_chat` and their dispatch — which names `FreezeStory` from
// `story_context` and belongs with the conversation that produces a snapshot
// rather than with the snapshot that validates one.
// ---------------------------------------------------------------------------

const MAX_PROJECT_CHAT_DISPOSITIONS: usize = 64;
const MAX_DISPOSITION_RATIONALE_BYTES: usize = 8 * 1024;
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
pub fn collect_project_chat_dispositions(
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
        let output = crate::project_chat_output::parse_project_assistant_output_with_predecessors_and_chapters(
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
pub fn validate_frozen_project_chat(
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
pub fn project_chat_basis_is_current(
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
pub fn require_blank_anchor(db: &Connection, document_id: &str) -> CoreResult<DocumentRecord> {
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

// ---------------------------------------------------------------------------
// The project-chat freeze.
//
// Moved down from `projects/story_context.rs` and
// `projects/project_chat_context.rs`. This is the last half of a cycle that ran
// three deep:
//
//   project_chat_context::freeze_project_chat_at
//     -> story_context::freeze_project_chat_at
//          -> project_chat_context::augment_frozen_chat
//
// `FreezeStory` moved first because `augment_frozen_chat` takes it, and it is
// pure vocabulary: ProjectAccess and Head from the kernel, BasisKind,
// ContextPurpose and InformationPolicy already here. No field of it needed
// anything above L3, which is the whole reason the cycle can be discharged.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FreezeStory {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub basis: BasisKind,
    pub purpose: ContextPurpose,
    pub policy: InformationPolicy,
}

pub fn augment_frozen_chat(
    tx: &Connection,
    request: &FreezeStory,
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
    frozen.conversation = crate::conversation::select_project_conversation_at(
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
