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

use crate::chat_vocabulary::FrozenProjectChat;
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
    Audience, ContextPurpose, InformationPolicy, SourceDescriptor, SourceRef, StorySnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use wns_kernel::{CoreError, CoreResult, ProjectAccess, sha256_hex};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePassage {
    pub handle: String,
    pub source: SourceRef,
    pub block_id: String,
    pub block_order: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRead {
    pub descriptor: SourceDescriptor,
    pub passages: Vec<SourcePassage>,
    pub body: Value,
    pub used_validated_projection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    Literal,
    Lexical,
    ExactAlias,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchStory {
    pub access: ProjectAccess,
    pub snapshot_id: String,
    pub query: String,
    pub mode: SearchMode,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchHit {
    pub passage: SourcePassage,
    pub start_utf16: u32,
    pub end_utf16: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
