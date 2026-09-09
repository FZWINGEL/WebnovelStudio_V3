//! Validation for the project-chat assistant response contract.
//!
//! Project chat is allowed to produce useful prose without producing a
//! document. When it does produce a document, the provider supplies only a
//! typed, ID-free block sequence. Rust owns the response bounds and target
//! references; the caller owns persistence and adoption.

use super::{CoreError, CoreResult, Head};
use crate::documents::{
    Endpoint, ScopeGrant, ScopeKind, TypedReplacementBlock, capture_scope,
    typed_replacement_snapshot, validate_typed_replacement_blocks,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;

/// Versioned response contract used by project-level conversations.
pub const PROJECT_CHAT_RESPONSE_CONTRACT: &str = "project-assistant-output.v1";

/// Versioned response contract for an unscoped chapter discussion.  It is
/// intentionally separate from the project-chat envelope: a chapter answer
/// may suggest a passage, but it cannot grant itself edit authority.
pub const CHAPTER_DISCUSSION_RESPONSE_CONTRACT: &str = "chapter-discussion-output.v1";
pub const CHAPTER_TARGET_HEAD_MARKER: &str =
    "Frozen chapter target head (copy this exact JSON in sourceHead):";

/// The first-slice response limits. A later grouped-adoption slice may accept
/// all three draft slots, while its transaction still validates each target.
pub const MAX_PROJECT_CHAT_QUESTIONS: usize = 2;
pub const MAX_PROJECT_CHAT_ASSUMPTIONS: usize = 3;
pub const MAX_PROJECT_CHAT_DRAFTS: usize = 3;
pub const MAX_PROJECT_CHAT_RESPONSE_BYTES: usize = 64 * 1024;
pub const MAX_PROJECT_CHAT_KEY_BYTES: usize = 128;
pub const MAX_PROJECT_CHAT_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_PROJECT_CHAT_ANSWER_BYTES: usize = 48 * 1024;
pub const MAX_PROJECT_CHAT_TITLE_BYTES: usize = 512;
pub const MAX_PROJECT_CHAT_CHANGE_SUMMARY_BYTES: usize = 8 * 1024;
pub const MAX_PROJECT_CHAT_HANDOFF_INSTRUCTION_BYTES: usize = 8 * 1024;
pub const MAX_PROJECT_CHAT_HANDOFF_BRIEF_BYTES: usize = 8 * 1024;
pub const MAX_CHAPTER_DISCUSSION_RESPONSE_BYTES: usize = 64 * 1024;
pub const MAX_CHAPTER_DISCUSSION_ANSWER_BYTES: usize = 48 * 1024;
pub const MAX_CHAPTER_RANGE_ID_BYTES: usize = 256;
pub const MAX_CHAPTER_RANGE_QUOTE_BYTES: usize = 32 * 1024;

/// Prompt recipe used by new project-chat packets. The response envelope
/// remains v1; this version only selects the system guidance that the packet
/// compiler freezes alongside the story context.
pub const PROJECT_CHAT_PROMPT_RECIPE_V2: &str = "project-chat-prompt.v2";

/// The exact project-chat guidance used before chapter handoff support. Keep
/// this immutable so packets whose frozen chat metadata predates prompt recipe
/// versioning can still be reproduced byte-for-byte.
pub const PROJECT_CHAT_RESPONSE_INSTRUCTION_LEGACY: &str = r#"You are the author's story-development partner for English novels. Help turn conversation into useful worldbuilding, characters, themes, hooks, and plans. Work in the order useful to the author; never require forms, a document checklist, or answers to every question before drafting. Wuxia, xianxia, or translated-webnovel register is optional when the author wants it. Keep existing infrastructure details out of the author-facing answer.

Ask zero to two focused questions only when their answers materially affect this task. Explain the creative choice briefly. When safe, offer a clearly provisional assumption and make progress. If the author asks you to draft now, do so without another setup interview. Respect exact question dispositions in projectChat: notNow defers, notRelevant suppresses unsolicited reopening, keepMysterious preserves uncertainty, and reconsider explicitly reopens it. A new author request can reopen any topic. Rejected assumptions must not quietly return as facts. None of these decisions establishes canon.

Discuss contradictions and consequences instead of silently changing established material. Drafts are unadopted proposals. Existing ordinary story documents and explicitly supplied assistant drafts have different authority. Assistant drafts are task material only; accepted story sources and adopted guidance remain distinct. Do not claim to have read omitted context. The blank conversation anchor is infrastructure and must never become draft content.

Response contract: project-assistant-output.v1. Return only one JSON object with this exact top-level shape:
{"schemaVersion":"project-assistant-output.v1","answer":"Useful prose for the author.","questions":[],"assumptions":[],"drafts":[]}
An answer alone is valid. Return at most two questions, three task-local assumptions, and three nonchapter drafts. Questions and assumptions have exactly {"key":"unique-short-key","text":"..."}. Each draft has exactly {"key":"unique-short-key","title":"...","kind":"world","changeSummary":"...","targetHandle":null,"predecessorHandle":null,"blocks":[{"type":"paragraph","content":[{"type":"text","text":"English prose."}]}]}. Keys must be unique across all three arrays and contain only ASCII letters, digits, underscores, and hyphens. Use concise titles. Do not include extra fields.

Draft kinds are note, world, character, theme, hook, or scene. Chapters use a separate scoped writing request. Omit targetHandle or use null for a new draft. To propose an update, copy an exact ordinary nonchapter source handle supplied in the frozen request; never invent a target or use a control-anchor or assistant-draft handle as a Working target. Explain what changes in changeSummary. When revising an unadopted draft, produce a fresh candidate; never pretend its old version was replaced or adopted. If the frozen request explicitly includes an unadopted assistant draft, set predecessorHandle to that exact assistant-draft handle; never use an ordinary, chapter, or control handle as predecessorHandle.

Blocks use only paragraph, heading with attrs.level from 1 to 3, or sceneBreak. Inline content uses nonempty text nodes, optional bold/italic/link marks with absolute http/https/mailto URLs, and hardBreak. Supply no document IDs, editor block IDs, editor steps, HTML, Markdown fences, or canon decisions. The application allocates identities, validates the response against its frozen sources, retains isolated drafts, and waits for the author to review and adopt them."#;

/// Resolve the exact project-chat system recipe recorded by the frozen chat
/// metadata. Historical snapshots omit the version and intentionally use the
/// legacy bytes; unknown explicit versions fail closed.
pub fn project_chat_response_instruction(
    prompt_recipe_version: Option<&str>,
) -> Result<&'static str, String> {
    match prompt_recipe_version {
        None => Ok(PROJECT_CHAT_RESPONSE_INSTRUCTION_LEGACY),
        Some(PROJECT_CHAT_PROMPT_RECIPE_V2) => Ok(PROJECT_CHAT_RESPONSE_INSTRUCTION),
        Some(version) => Err(format!(
            "unknown project-chat prompt recipe version {version:?}"
        )),
    }
}

/// Provider instruction for the project-chat response envelope.
///
/// This is deliberately an application response contract rather than a story
/// tool API. The provider cannot allocate document IDs, editor block IDs, or
/// adoption decisions.
pub const PROJECT_CHAT_RESPONSE_INSTRUCTION: &str = r#"You are the author's story-development partner for English novels. Help turn conversation into useful worldbuilding, characters, themes, hooks, and plans. Work in the order useful to the author; never require forms, a document checklist, or answers to every question before drafting. Wuxia, xianxia, or translated-webnovel register is optional when the author wants it. Keep existing infrastructure details out of the author-facing answer.

Ask zero to two focused questions only when their answers materially affect this task. Explain the creative choice briefly. When safe, offer a clearly provisional assumption and make progress. If the author asks you to draft now, do so without another setup interview. Respect exact question dispositions in projectChat: notNow defers, notRelevant suppresses unsolicited reopening, keepMysterious preserves uncertainty, and reconsider explicitly reopens it. A new author request can reopen any topic. Rejected assumptions must not quietly return as facts. None of these decisions establishes canon.

Discuss contradictions and consequences instead of silently changing established material. Drafts are unadopted proposals. Existing ordinary story documents and explicitly supplied assistant drafts have different authority. Assistant drafts are task material only; accepted story sources and adopted guidance remain distinct. Do not claim to have read omitted context. The blank conversation anchor is infrastructure and must never become draft content.

Response contract: project-assistant-output.v1. Return only one JSON object with this exact top-level shape:
{"schemaVersion":"project-assistant-output.v1","answer":"Useful prose for the author.","questions":[],"assumptions":[],"drafts":[],"chapterHandoff":null}
An answer alone is valid. Return at most two questions, three task-local assumptions, and three nonchapter drafts. Questions and assumptions have exactly {"key":"unique-short-key","text":"..."}. Each draft has exactly {"key":"unique-short-key","title":"...","kind":"world","changeSummary":"...","targetHandle":null,"predecessorHandle":null,"blocks":[{"type":"paragraph","content":[{"type":"text","text":"English prose."}]}]}. Keys must be unique across all three arrays and contain only ASCII letters, digits, underscores, and hyphens. Use concise titles. Do not include extra fields.

Draft kinds are note, world, character, theme, hook, or scene. Chapters use a separate scoped writing request. Omit targetHandle or use null for a new draft. To propose an update, copy an exact ordinary nonchapter source handle supplied in the frozen request; never invent a target or use a control-anchor or assistant-draft handle as a Working target. Explain what changes in changeSummary. When revising an unadopted draft, produce a fresh candidate; never pretend its old version was replaced or adopted. If the frozen request explicitly includes an unadopted assistant draft, set predecessorHandle to that exact assistant-draft handle; never use an ordinary, chapter, or control handle as predecessorHandle.

When the author asks to start writing a chapter, you may optionally include chapterHandoff with exactly {"targetHandle":null,"proposedTitle":"Chapter title","instruction":"What the author wants written.","brief":"Optional author-room guidance to review before dispatch."}. Use null targetHandle for a proposed blank chapter, or copy an exact ordinary chapter handle from the frozen request. This is a proposal only: the application must show the boundary and obtain author confirmation before creating or dispatching a chapter task. It does not create canon, grant edit authority, trigger a second invocation, or contain provider IDs or editor steps. Omit chapterHandoff when the request is not about beginning chapter writing.

Blocks use only paragraph, heading with attrs.level from 1 to 3, or sceneBreak. Inline content uses nonempty text nodes, optional bold/italic/link marks with absolute http/https/mailto URLs, and hardBreak. Supply no document IDs, editor block IDs, editor steps, HTML, Markdown fences, or canon decisions. The application allocates identities, validates the response against its frozen sources, retains isolated drafts, and waits for the author to review and adopt them."#;

/// Provider guidance for a chapter Discuss request with no preselected scope.
/// The optional range is a review hint only. It must never be treated as an
/// editor selection or a write grant by the provider or the application.
pub const CHAPTER_DISCUSSION_RESPONSE_INSTRUCTION: &str = r#"You are giving feedback on an English novel chapter. Return only one JSON object with this exact top-level shape:
{"schemaVersion":"chapter-discussion-output.v1","answer":"Readable feedback for the author.","rangeProposal":null}
The answer is required and may explain the feedback, priorities, and uncertainty. You may include at most one rangeProposal when the request concerns one or more contiguous paragraphs that were not preselected. A range proposal is only a review hint and does not authorize editing. If no specific contiguous range is useful, use null.
When present, rangeProposal must have exactly {"sourceHead":{"documentId":"...","version":"...","bodyHash":"..."},"firstBlockId":"...","lastBlockId":"...","quote":"..."}. Copy sourceHead exactly from the frozen target chapter metadata. Copy the exact first and last editor block IDs from that chapter. quote must be the exact source text for all blocks in that inclusive range. Do not invent IDs, offsets, document revisions, or replacement prose. Do not include provider IDs, editor steps, HTML, Markdown fences, or extra fields."#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChapterDiscussionOutput {
    pub schema_version: String,
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range_proposal: Option<ChapterRangeProposalInput>,
}

/// Provider-owned range input. This is deliberately not a ScopeGrant: it is
/// converted into a read-only projection only after Rust verifies it against
/// the frozen chapter source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChapterRangeProposalInput {
    pub source_head: Head,
    pub first_block_id: String,
    pub last_block_id: String,
    pub quote: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChapterRangeProposal {
    pub source_head: Head,
    pub first_block_id: String,
    pub last_block_id: String,
    pub quote: String,
}

/// Read-only projection returned to the renderer. `range_error` preserves the
/// answer when a provider supplied a malformed or stale hint, while making the
/// hint unavailable for staging. It never grants a scope or changes a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChapterDiscussionProjection {
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range_proposal: Option<ChapterRangeProposal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range_error: Option<String>,
}

pub fn parse_chapter_discussion_output(text: &str) -> CoreResult<ChapterDiscussionOutput> {
    if text.len() > MAX_CHAPTER_DISCUSSION_RESPONSE_BYTES {
        return Err(output_error(format!(
            "chapter discussion response exceeds {MAX_CHAPTER_DISCUSSION_RESPONSE_BYTES} bytes"
        )));
    }
    let output: ChapterDiscussionOutput = serde_json::from_str(text).map_err(|error| {
        output_error(format!(
            "response is not valid chapter-discussion JSON: {error}"
        ))
    })?;
    if output.schema_version != CHAPTER_DISCUSSION_RESPONSE_CONTRACT {
        return Err(output_error(format!(
            "schemaVersion must be {CHAPTER_DISCUSSION_RESPONSE_CONTRACT:?}"
        )));
    }
    if output.answer.trim().is_empty() || output.answer.len() > MAX_CHAPTER_DISCUSSION_ANSWER_BYTES
    {
        return Err(output_error(
            "chapter discussion answer must be nonempty and within its response limit".to_owned(),
        ));
    }
    if let Some(range) = &output.range_proposal {
        validate_range_input_shape(range)?;
    }
    Ok(output)
}

/// Parse a provider response and retain readable feedback even when its
/// optional range hint is stale or malformed. A valid hint is checked against
/// the exact frozen target head and canonical block structure.
pub fn project_chapter_discussion_output(
    text: &str,
    target: &Head,
    source_snapshot: &Value,
) -> CoreResult<ChapterDiscussionProjection> {
    let output = match parse_chapter_discussion_output(text) {
        Ok(output) => output,
        Err(error) => {
            // Keep a readable answer when only the optional range object is
            // malformed. Unknown top-level fields and malformed envelopes
            // still fail closed with no structured projection.
            let raw: Value = serde_json::from_str(text).map_err(|_| error.clone())?;
            let object = raw.as_object().ok_or_else(|| error.clone())?;
            if object.get("schemaVersion").and_then(Value::as_str)
                != Some(CHAPTER_DISCUSSION_RESPONSE_CONTRACT)
            {
                return Err(error);
            }
            let answer = object
                .get("answer")
                .and_then(Value::as_str)
                .filter(|answer| {
                    !answer.trim().is_empty() && answer.len() <= MAX_CHAPTER_DISCUSSION_ANSWER_BYTES
                })
                .ok_or_else(|| error.clone())?
                .to_owned();
            return Ok(ChapterDiscussionProjection {
                answer,
                range_proposal: None,
                range_error: Some(error.detail),
            });
        }
    };
    let Some(range) = output.range_proposal else {
        return Ok(ChapterDiscussionProjection {
            answer: output.answer,
            range_proposal: None,
            range_error: None,
        });
    };
    match validate_range_against_source(&range, target, source_snapshot) {
        Ok(validated) => Ok(ChapterDiscussionProjection {
            answer: output.answer,
            range_proposal: Some(validated),
            range_error: None,
        }),
        Err(error) => Ok(ChapterDiscussionProjection {
            answer: output.answer,
            range_proposal: None,
            range_error: Some(error.detail),
        }),
    }
}

fn validate_range_input_shape(range: &ChapterRangeProposalInput) -> CoreResult<()> {
    if range.first_block_id.trim().is_empty()
        || range.last_block_id.trim().is_empty()
        || range.first_block_id.len() > MAX_CHAPTER_RANGE_ID_BYTES
        || range.last_block_id.len() > MAX_CHAPTER_RANGE_ID_BYTES
        || range.quote.trim().is_empty()
        || range.quote.len() > MAX_CHAPTER_RANGE_QUOTE_BYTES
        || range.first_block_id.chars().any(char::is_control)
        || range.last_block_id.chars().any(char::is_control)
    {
        return Err(output_error(
            "rangeProposal must contain bounded nonempty block IDs and an exact quote".to_owned(),
        ));
    }
    Ok(())
}

fn validate_range_against_source(
    range: &ChapterRangeProposalInput,
    target: &Head,
    source_snapshot: &Value,
) -> CoreResult<ChapterRangeProposal> {
    validate_range_input_shape(range)?;
    if &range.source_head != target {
        return Err(output_error(
            "rangeProposal sourceHead does not match the frozen chapter target".to_owned(),
        ));
    }
    let end_offset = block_utf16_length(source_snapshot, &range.last_block_id)?;
    let captured = capture_scope(
        source_snapshot,
        ScopeGrant {
            kind: ScopeKind::Blocks,
            start: Some(Endpoint {
                block_id: range.first_block_id.clone(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: range.last_block_id.clone(),
                utf16_offset: end_offset,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .map_err(|message| {
        output_error(format!(
            "rangeProposal is not a valid block range: {message}"
        ))
    })?;
    if captured.source_hash != target.body_hash || captured.quote != range.quote {
        return Err(output_error(
            "rangeProposal quote or source revision does not match the frozen chapter".to_owned(),
        ));
    }
    Ok(ChapterRangeProposal {
        source_head: range.source_head.clone(),
        first_block_id: range.first_block_id.clone(),
        last_block_id: range.last_block_id.clone(),
        quote: captured.quote,
    })
}

fn block_utf16_length(source_snapshot: &Value, block_id: &str) -> CoreResult<u32> {
    let blocks = source_snapshot
        .get("body")
        .and_then(|body| body.get("content"))
        .and_then(Value::as_array)
        .ok_or_else(|| output_error("chapter source has no block content".to_owned()))?;
    let block = blocks
        .iter()
        .find(|block| {
            block
                .get("attrs")
                .and_then(|attrs| attrs.get("id"))
                .and_then(Value::as_str)
                == Some(block_id)
        })
        .ok_or_else(|| {
            output_error(format!(
                "rangeProposal block {block_id:?} is not in the chapter"
            ))
        })?;
    let content = block
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut units = 0_u32;
    for inline in content {
        match inline.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = inline
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| output_error("chapter text node is invalid".to_owned()))?;
                units = units
                    .checked_add(text.encode_utf16().count() as u32)
                    .ok_or_else(|| output_error("chapter block length overflowed".to_owned()))?;
            }
            Some("hardBreak") => units = units.saturating_add(1),
            Some(other) => {
                return Err(output_error(format!(
                    "chapter inline node type {other:?} is unsupported"
                )));
            }
            None => {
                return Err(output_error(
                    "chapter inline node type is missing".to_owned(),
                ));
            }
        }
    }
    Ok(units)
}

/// A validated project-level assistant response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAssistantOutput {
    pub schema_version: String,
    pub answer: String,
    pub questions: Vec<ChatQuestion>,
    pub assumptions: Vec<ChatAssumption>,
    pub drafts: Vec<ChatDraftOutput>,
    /// Optional author-room proposal for crossing into a chapter-writing task.
    /// This is never a chapter request or a write authority by itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter_handoff: Option<ChatChapterHandoff>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatQuestion {
    pub key: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAssumption {
    pub key: String,
    pub text: String,
}

/// A bounded proposal to move from project conversation into chapter writing.
/// `target_handle` is an exact frozen ordinary-chapter source handle when
/// present; `None` asks the author to confirm a new blank chapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatChapterHandoff {
    pub target_handle: Option<String>,
    pub proposed_title: String,
    pub instruction: String,
    pub brief: String,
}

/// A document proposal from project chat. `target_handle` refers only to a
/// frozen, caller-supplied handle; it is never an application document ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatDraftOutput {
    pub key: String,
    pub title: String,
    pub kind: String,
    pub target_handle: Option<String>,
    /// The exact frozen assistant-draft source being revised, when any. This
    /// is provenance only: it never replaces or supersedes that draft.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_handle: Option<String>,
    pub change_summary: String,
    pub blocks: Vec<TypedReplacementBlock>,
}

/// Parse and validate one complete provider response.
///
/// `allowed_target_handles` is the exact set captured in the request's frozen
/// context. An absent target handle describes a new isolated draft; a present
/// handle must be a member of this set. No target or block ID is inferred from
/// provider prose.
pub fn parse_project_assistant_output(
    text: &str,
    allowed_target_handles: &BTreeSet<String>,
) -> CoreResult<ProjectAssistantOutput> {
    parse_project_assistant_output_with_predecessors(text, allowed_target_handles, &BTreeSet::new())
}

/// Parse a response with separately authenticated ordinary targets and
/// explicitly attached assistant-draft predecessors.
pub fn parse_project_assistant_output_with_predecessors(
    text: &str,
    allowed_target_handles: &BTreeSet<String>,
    allowed_predecessor_handles: &BTreeSet<String>,
) -> CoreResult<ProjectAssistantOutput> {
    parse_project_assistant_output_with_predecessors_and_chapters(
        text,
        allowed_target_handles,
        allowed_predecessor_handles,
        &BTreeSet::new(),
    )
}

/// Parse a response while separately authenticating ordinary material targets,
/// attached assistant-draft predecessors, and ordinary chapter targets for a
/// proposed author-room handoff. Keeping the chapter set separate prevents a
/// chapter handle from becoming a nonchapter material target by accident.
pub fn parse_project_assistant_output_with_predecessors_and_chapters(
    text: &str,
    allowed_target_handles: &BTreeSet<String>,
    allowed_predecessor_handles: &BTreeSet<String>,
    allowed_chapter_handles: &BTreeSet<String>,
) -> CoreResult<ProjectAssistantOutput> {
    if allowed_target_handles
        .iter()
        .any(|handle| allowed_predecessor_handles.contains(handle))
    {
        return Err(output_error(
            "target and predecessor handle sets overlap".to_owned(),
        ));
    }
    if allowed_target_handles
        .iter()
        .any(|handle| allowed_chapter_handles.contains(handle))
        || allowed_predecessor_handles
            .iter()
            .any(|handle| allowed_chapter_handles.contains(handle))
    {
        return Err(output_error(
            "material target, predecessor, and chapter handle sets must be disjoint".to_owned(),
        ));
    }
    if text.len() > MAX_PROJECT_CHAT_RESPONSE_BYTES {
        return Err(output_error(format!(
            "response exceeds {MAX_PROJECT_CHAT_RESPONSE_BYTES} bytes"
        )));
    }

    let output: ProjectAssistantOutput = serde_json::from_str(text).map_err(|error| {
        output_error(format!(
            "response is not valid project-assistant JSON: {error}"
        ))
    })?;
    validate_output(
        &output,
        allowed_target_handles,
        allowed_predecessor_handles,
        allowed_chapter_handles,
    )?;
    Ok(output)
}

/// Materialize provider-owned blocks into a canonical editor document.
///
/// Every invocation allocates fresh application block IDs. The input grammar
/// has no ID-bearing fields, and serde's `deny_unknown_fields` rejects an ID
/// if a provider tries to inject one into a block or inline node.
pub fn materialize_draft_body(blocks: &[TypedReplacementBlock]) -> CoreResult<Value> {
    validate_typed_replacement_blocks(blocks).map_err(output_error)?;
    if blocks.is_empty() {
        return Err(output_error(
            "a project-chat draft must contain at least one block".to_owned(),
        ));
    }

    let ids: Vec<String> = blocks
        .iter()
        .enumerate()
        .map(|(index, _)| format!("chat-draft-{}-{index}", Uuid::new_v4()))
        .collect();
    typed_replacement_snapshot(blocks, &ids).map_err(output_error)
}

fn validate_output(
    output: &ProjectAssistantOutput,
    allowed_target_handles: &BTreeSet<String>,
    allowed_predecessor_handles: &BTreeSet<String>,
    allowed_chapter_handles: &BTreeSet<String>,
) -> CoreResult<()> {
    if output.schema_version != PROJECT_CHAT_RESPONSE_CONTRACT {
        return Err(output_error(format!(
            "schemaVersion must be {PROJECT_CHAT_RESPONSE_CONTRACT:?}"
        )));
    }
    validate_text(&output.answer, MAX_PROJECT_CHAT_ANSWER_BYTES, "answer")?;
    if output.questions.len() > MAX_PROJECT_CHAT_QUESTIONS {
        return Err(output_error(format!(
            "questions may contain at most {MAX_PROJECT_CHAT_QUESTIONS} items"
        )));
    }
    if output.assumptions.len() > MAX_PROJECT_CHAT_ASSUMPTIONS {
        return Err(output_error(format!(
            "assumptions may contain at most {MAX_PROJECT_CHAT_ASSUMPTIONS} items"
        )));
    }
    if output.drafts.len() > MAX_PROJECT_CHAT_DRAFTS {
        return Err(output_error(format!(
            "drafts may contain at most {MAX_PROJECT_CHAT_DRAFTS} items"
        )));
    }

    if let Some(handoff) = &output.chapter_handoff {
        validate_text(
            &handoff.proposed_title,
            MAX_PROJECT_CHAT_TITLE_BYTES,
            "chapterHandoff proposedTitle",
        )?;
        validate_text(
            &handoff.instruction,
            MAX_PROJECT_CHAT_HANDOFF_INSTRUCTION_BYTES,
            "chapterHandoff instruction",
        )?;
        validate_text(
            &handoff.brief,
            MAX_PROJECT_CHAT_HANDOFF_BRIEF_BYTES,
            "chapterHandoff brief",
        )?;
        if let Some(handle) = &handoff.target_handle
            && (handle.is_empty() || !allowed_chapter_handles.contains(handle))
        {
            return Err(output_error(format!(
                "chapterHandoff references chapter handle {:?} outside the frozen ordinary chapters",
                handle
            )));
        }
    }

    let mut keys = BTreeSet::new();
    for question in &output.questions {
        validate_key(&question.key, "question key")?;
        if !keys.insert(question.key.clone()) {
            return Err(output_error(format!(
                "response key {:?} is duplicated",
                question.key
            )));
        }
        validate_text(&question.text, MAX_PROJECT_CHAT_TEXT_BYTES, "question text")?;
    }
    for assumption in &output.assumptions {
        validate_key(&assumption.key, "assumption key")?;
        if !keys.insert(assumption.key.clone()) {
            return Err(output_error(format!(
                "response key {:?} is duplicated",
                assumption.key
            )));
        }
        validate_text(
            &assumption.text,
            MAX_PROJECT_CHAT_TEXT_BYTES,
            "assumption text",
        )?;
    }
    for draft in &output.drafts {
        validate_key(&draft.key, "draft key")?;
        if !keys.insert(draft.key.clone()) {
            return Err(output_error(format!(
                "response key {:?} is duplicated",
                draft.key
            )));
        }
        validate_text(&draft.title, MAX_PROJECT_CHAT_TITLE_BYTES, "draft title")?;
        validate_text(
            &draft.change_summary,
            MAX_PROJECT_CHAT_CHANGE_SUMMARY_BYTES,
            "draft changeSummary",
        )?;
        if !matches!(
            draft.kind.as_str(),
            "note" | "world" | "character" | "theme" | "hook" | "scene"
        ) {
            return Err(output_error(format!(
                "draft {:?} has unsupported kind {:?}; chapter and arbitrary kinds are not allowed",
                draft.key, draft.kind
            )));
        }
        if let Some(handle) = &draft.target_handle
            && (handle.is_empty() || !allowed_target_handles.contains(handle))
        {
            return Err(output_error(format!(
                "draft {:?} references target handle {:?} outside the frozen request",
                draft.key, handle
            )));
        }
        if let Some(handle) = &draft.predecessor_handle
            && (handle.is_empty() || !allowed_predecessor_handles.contains(handle))
        {
            return Err(output_error(format!(
                "draft {:?} references predecessor handle {:?} outside the explicitly attached assistant drafts",
                draft.key, handle
            )));
        }
        if draft.blocks.is_empty() {
            return Err(output_error(format!(
                "draft {:?} must contain at least one block",
                draft.key
            )));
        }
        validate_typed_replacement_blocks(&draft.blocks).map_err(output_error)?;
    }
    Ok(())
}

fn validate_key(value: &str, label: &str) -> CoreResult<()> {
    if value.is_empty() || value.len() > MAX_PROJECT_CHAT_KEY_BYTES {
        return Err(output_error(format!(
            "{label} must be 1..{MAX_PROJECT_CHAT_KEY_BYTES} bytes"
        )));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(output_error(format!(
            "{label} must use only ASCII letters, digits, '_' or '-'"
        )));
    }
    Ok(())
}

fn validate_text(value: &str, max_bytes: usize, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(output_error(format!("{label} must not be blank")));
    }
    if value.len() > max_bytes {
        return Err(output_error(format!("{label} exceeds {max_bytes} bytes")));
    }
    Ok(())
}

fn output_error(detail: String) -> CoreError {
    CoreError::new("InvalidProjectChatOutput", &detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn parse(json: Value) -> CoreResult<ProjectAssistantOutput> {
        parse_project_assistant_output(&serde_json::to_string(&json).unwrap(), &BTreeSet::new())
    }

    fn parse_with_chapters(
        json: Value,
        targets: &BTreeSet<String>,
        chapters: &BTreeSet<String>,
    ) -> CoreResult<ProjectAssistantOutput> {
        parse_project_assistant_output_with_predecessors_and_chapters(
            &serde_json::to_string(&json).unwrap(),
            targets,
            &BTreeSet::new(),
            chapters,
        )
    }

    fn empty_output() -> Value {
        json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT,
            "answer": "I can start with the central conflict.",
            "questions": [],
            "assumptions": [],
            "drafts": []
        })
    }

    #[test]
    fn accepts_answer_only_response() {
        let output = parse(empty_output()).expect("answer-only output should be valid");
        assert_eq!(output.questions.len(), 0);
        assert_eq!(output.drafts.len(), 0);
        assert!(output.chapter_handoff.is_none());
    }

    #[test]
    fn prompt_recipe_falls_back_to_exact_legacy_bytes_and_rejects_unknown_versions() {
        let legacy_hash: String =
            Sha256::digest(PROJECT_CHAT_RESPONSE_INSTRUCTION_LEGACY.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
        assert_eq!(
            legacy_hash,
            "0f2bc52aab308e809ab07fc77e505d57bd076deffd758f4f75f9e8f547ad2f22"
        );
        assert_eq!(
            project_chat_response_instruction(None).expect("legacy recipe"),
            PROJECT_CHAT_RESPONSE_INSTRUCTION_LEGACY
        );
        assert_eq!(
            project_chat_response_instruction(Some(PROJECT_CHAT_PROMPT_RECIPE_V2))
                .expect("current recipe"),
            PROJECT_CHAT_RESPONSE_INSTRUCTION
        );
        assert!(project_chat_response_instruction(Some("project-chat-prompt.v999")).is_err());
    }

    #[test]
    fn accepts_blank_and_exact_existing_chapter_handoffs_without_creating_authority() {
        let blank = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT,
            "answer": "We can begin with the arrival scene.",
            "questions": [], "assumptions": [], "drafts": [],
            "chapterHandoff": {
                "targetHandle": null,
                "proposedTitle": "Arrival at the river gate",
                "instruction": "Draft the opening arrival and end before the letter is opened.",
                "brief": "Keep the missing ship unexplained and preserve the courier's uncertainty."
            }
        });
        let targets = BTreeSet::new();
        let mut chapters = BTreeSet::new();
        chapters.insert("chapter-revision-7".to_owned());
        let output = parse_with_chapters(blank, &targets, &chapters).expect("blank handoff");
        assert_eq!(output.chapter_handoff.as_ref().unwrap().target_handle, None);

        let existing = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT,
            "answer": "The current chapter can continue from its saved head.",
            "questions": [], "assumptions": [], "drafts": [],
            "chapterHandoff": {
                "targetHandle": "chapter-revision-7",
                "proposedTitle": "The river gate",
                "instruction": "Continue the chapter through the first public interruption.",
                "brief": "Keep the ending outside the proposed continuation."
            }
        });
        let output = parse_with_chapters(existing, &targets, &chapters).expect("existing handoff");
        assert_eq!(
            output.chapter_handoff.unwrap().target_handle.as_deref(),
            Some("chapter-revision-7")
        );
    }

    #[test]
    fn rejects_nonchapter_or_unknown_handoff_handles() {
        let value = |handle: &str| {
            json!({
                "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT,
                "answer": "A chapter transition proposal.",
                "questions": [], "assumptions": [], "drafts": [],
                "chapterHandoff": {
                    "targetHandle": handle,
                    "proposedTitle": "Chapter",
                    "instruction": "Write the next scene.",
                    "brief": "Keep the unresolved question unresolved."
                }
            })
        };
        let mut targets = BTreeSet::new();
        targets.insert("world-revision-2".to_owned());
        let mut chapters = BTreeSet::new();
        chapters.insert("chapter-revision-7".to_owned());
        assert!(parse_with_chapters(value("world-revision-2"), &targets, &chapters).is_err());
        assert!(parse_with_chapters(value("missing"), &targets, &chapters).is_err());
    }

    #[test]
    fn bounds_and_rejects_unknown_chapter_handoff_fields() {
        let mut oversize = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT,
            "answer": "A chapter transition proposal.",
            "questions": [], "assumptions": [], "drafts": [],
            "chapterHandoff": {
                "targetHandle": null,
                "proposedTitle": "Chapter",
                "instruction": "x",
                "brief": "x"
            }
        });
        oversize["chapterHandoff"]["instruction"] =
            Value::String("x".repeat(MAX_PROJECT_CHAT_HANDOFF_INSTRUCTION_BYTES + 1));
        let targets = BTreeSet::new();
        let chapters = BTreeSet::new();
        assert!(parse_with_chapters(oversize, &targets, &chapters).is_err());

        let unknown = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT,
            "answer": "A chapter transition proposal.",
            "questions": [], "assumptions": [], "drafts": [],
            "chapterHandoff": {
                "targetHandle": null,
                "proposedTitle": "Chapter",
                "instruction": "Write the next scene.",
                "brief": "Keep the unresolved question unresolved.",
                "editorSteps": []
            }
        });
        assert!(parse_with_chapters(unknown, &targets, &chapters).is_err());
    }

    #[test]
    fn accepts_literal_english_narrative_and_exact_target_handle() {
        let mut handles = BTreeSet::new();
        handles.insert("world-sketch".to_owned());
        let value = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT,
            "answer": "The river city can anchor the opening tension.",
            "questions": [{"key":"q-rival","text":"Should the rival know about the missing seal?"}],
            "assumptions": [{"key":"a-register","text":"I am assuming a restrained translated-webnovel register."}],
            "drafts": [{
                "key":"d-world",
                "title":"River City sketch",
                "kind":"world",
                "targetHandle":"world-sketch",
                "changeSummary":"Adds the city and its immediate pressure point.",
                "blocks":[{"type":"paragraph","content":[{"type":"text","text":"Mei's lantern burned blue beside the old river gate."}]}]
            }]
        });
        let output =
            parse_project_assistant_output(&serde_json::to_string(&value).unwrap(), &handles)
                .expect("narrative draft should be valid");
        assert_eq!(
            output.drafts[0].target_handle.as_deref(),
            Some("world-sketch")
        );
        assert_eq!(output.drafts[0].blocks.len(), 1);
    }

    #[test]
    fn accepts_only_an_explicit_assistant_predecessor_handle() {
        let value = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT,
            "answer": "A revised candidate.",
            "questions": [],
            "assumptions": [],
            "drafts": [{
                "key":"d-revised", "title":"Revised note", "kind":"note",
                "targetHandle":null, "predecessorHandle":"draft-revision-v1",
                "changeSummary":"Clarifies the earlier candidate.",
                "blocks":[{"type":"paragraph","content":[{"type":"text","text":"A clearer note."}]}]
            }]
        });
        let targets = BTreeSet::new();
        let mut predecessors = BTreeSet::new();
        predecessors.insert("draft-revision-v1".to_owned());
        let output = parse_project_assistant_output_with_predecessors(
            &serde_json::to_string(&value).unwrap(),
            &targets,
            &predecessors,
        )
        .expect("explicit predecessor should be accepted");
        assert_eq!(
            output.drafts[0].predecessor_handle.as_deref(),
            Some("draft-revision-v1")
        );
        assert!(
            parse_project_assistant_output(&serde_json::to_string(&value).unwrap(), &targets)
                .is_err()
        );
    }

    #[test]
    fn rejects_extra_fields_and_block_ids() {
        let mut value = empty_output();
        value["unexpected"] = json!(true);
        assert!(
            parse(value).is_err(),
            "extra top-level fields must be rejected"
        );

        let value = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT,
            "answer": "A draft.",
            "questions": [],
            "assumptions": [],
            "drafts": [{
                "key":"d-one", "title":"Note", "kind":"note", "targetHandle":null,
                "changeSummary":"A note.",
                "blocks":[{"type":"paragraph","attrs":{"id":"provider-id"},"content":[{"type":"text","text":"No IDs."}]}]
            }]
        });
        assert!(parse(value).is_err(), "provider block IDs must be rejected");
    }

    #[test]
    fn rejects_unsupported_kind_unknown_target_and_duplicate_keys() {
        let unsupported = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT, "answer":"x", "questions":[], "assumptions":[],
            "drafts":[{"key":"d","title":"Chapter","kind":"chapter","targetHandle":null,"changeSummary":"x","blocks":[{"type":"paragraph","content":[{"type":"text","text":"x"}]}]}]
        });
        assert!(parse(unsupported).is_err());

        let unknown_target = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT, "answer":"x", "questions":[], "assumptions":[],
            "drafts":[{"key":"d","title":"Note","kind":"note","targetHandle":"missing","changeSummary":"x","blocks":[{"type":"paragraph","content":[{"type":"text","text":"x"}]}]}]
        });
        assert!(parse(unknown_target).is_err());

        let duplicate = json!({
            "schemaVersion": PROJECT_CHAT_RESPONSE_CONTRACT, "answer":"x",
            "questions":[{"key":"same","text":"one"}], "assumptions":[{"key":"same","text":"two"}], "drafts":[]
        });
        assert!(parse(duplicate).is_err());
    }

    #[test]
    fn enforces_question_assumption_and_draft_limits() {
        let mut value = empty_output();
        value["questions"] = json!([
            {"key":"q1","text":"one"}, {"key":"q2","text":"two"}, {"key":"q3","text":"three"}
        ]);
        assert!(parse(value).is_err());

        let mut value = empty_output();
        value["drafts"] = json!([
            {"key":"d1","title":"one","kind":"note","targetHandle":null,"changeSummary":"x","blocks":[{"type":"paragraph","content":[{"type":"text","text":"x"}]}]},
            {"key":"d2","title":"two","kind":"note","targetHandle":null,"changeSummary":"x","blocks":[{"type":"paragraph","content":[{"type":"text","text":"x"}]}]},
            {"key":"d3","title":"three","kind":"note","targetHandle":null,"changeSummary":"x","blocks":[{"type":"paragraph","content":[{"type":"text","text":"x"}]}]},
            {"key":"d4","title":"four","kind":"note","targetHandle":null,"changeSummary":"x","blocks":[{"type":"paragraph","content":[{"type":"text","text":"x"}]}]}
        ]);
        assert!(parse(value).is_err());
    }

    #[test]
    fn materializes_fresh_canonical_block_ids() {
        let blocks = vec![TypedReplacementBlock::Paragraph {
            content: vec![crate::documents::TypedReplacementInline::Text {
                text: "A literal sentence.".to_owned(),
                marks: Vec::new(),
            }],
        }];
        let first = materialize_draft_body(&blocks).expect("first body");
        let second = materialize_draft_body(&blocks).expect("second body");
        let first_id = first["body"]["content"][0]["attrs"]["id"]
            .as_str()
            .expect("first ID");
        let second_id = second["body"]["content"][0]["attrs"]["id"]
            .as_str()
            .expect("second ID");
        assert_ne!(first_id, second_id);
        assert_eq!(first["body"]["content"][0]["type"], "paragraph");
    }
}
