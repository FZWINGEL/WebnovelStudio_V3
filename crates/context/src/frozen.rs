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
use crate::{ContextPurpose, InformationPolicy, SourceDescriptor, SourceRef, StorySnapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use wns_kernel::{CoreError, CoreResult, ProjectAccess};

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
