//! The workshop packet metadata a compiled packet carries, and its parser.
//!
//! Moved down from `webnovel-core`'s `projects/workshop_generation.rs` and
//! `projects/workshop.rs`, both bound for `wns-workshop` at L5. `context_packets`
//! (L4) validates an instruction's workshop metadata while preparing a packet,
//! so the parser and every shape it reads has to sit below both. Both source
//! modules re-export all of it at the historical paths.
//!
//! Two files' worth of code arriving in one is what surfaced the collision this
//! move was blocked on: `workshop.rs` and `workshop_generation.rs` each declared
//! a `validate_text`, with the last two parameters swapped. The parser's copy
//! was renamed to `validate_workshop_text` in the commit before this one, so the
//! extraction is the only thing that can fail here.

use crate::workshop_vocabulary::{
    Lens, StoryPossibility, StoryPossibilityStatus,
    WorkshopDepth, WorkshopQuestion,
    WorkshopRelationship,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use wns_kernel::{CoreError, CoreResult, Head, check_id};

pub const MAX_DETAIL_BYTES: usize = 8 * 1024;
pub const VOICE_GUIDANCE_ACTION: &str = "voiceGuidance";
pub const VOICE_GUIDANCE_DIMENSIONS: [&str; 5] = [
    "Sentence density",
    "Viewpoint distance",
    "Humor",
    "Exposition",
    "Dialogue rhythm",
];
/// document while holding the actor's CAS boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopWorkingSelection {
    pub from: u32,
    pub to: u32,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopExploration {
    pub session_id: String,
    pub expected_version: String,
    pub working_generation: String,
    pub action: String,
    pub instruction: String,
    pub selected_scope: String,
    pub selected_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_selection: Option<WorkshopWorkingSelection>,
}
/// preservation remains reviewable author work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopLiteral {
    pub text: String,
    pub fixed: bool,
}
/// renderer supplied IDs are never used as source authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopContext {
    pub expected: Head,
    pub lens: Lens,
    pub depth: WorkshopDepth,
    pub current_element: String,
    /// Author-corrected interpretation, separate from the exact editable prose.
    /// This travels in the extensible final instruction, not the strict
    /// metadata projection used to validate historical candidate scopes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_brief: Option<String>,
    pub direction: String,
    pub still_open: String,
    pub focus_question: String,
    pub focus_reason: String,
    pub selected_details: Vec<WorkshopLiteral>,
    pub chosen_details: Vec<String>,
    pub fixed_details: Vec<String>,
    pub fixed_source_refs: Vec<String>,
    pub preferences: Vec<String>,
    pub hard_constraints: Vec<String>,
    pub included_document_ids: Vec<String>,
    pub included_alternatives: Vec<String>,
    pub rejected_rationales: Vec<String>,
    pub questions: Vec<WorkshopQuestion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub story_possibilities: Vec<StoryPossibility>,
    pub original_notes: String,
    pub outside_direction: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<WorkshopRelationship>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopPacketMetadata {
    pub exploration: WorkshopExploration,
    pub lens: Lens,
    pub depth: WorkshopDepth,
    pub current_element: String,
    pub direction: String,
    pub still_open: String,
    pub focus_question: String,
    pub focus_reason: String,
    pub selected_details: Vec<WorkshopLiteral>,
    pub chosen_details: Vec<String>,
    pub fixed_details: Vec<String>,
    pub fixed_source_refs: Vec<String>,
    pub preferences: Vec<String>,
    pub hard_constraints: Vec<String>,
    pub included_alternatives: Vec<String>,
    pub rejected_rationales: Vec<String>,
    pub questions: Vec<WorkshopQuestion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub story_possibilities: Vec<StoryPossibility>,
    pub original_notes: String,
    pub outside_direction: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_guidance: Option<WorkshopVoiceGuidance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<WorkshopRelationship>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopVoiceGuidance {
    pub sample: String,
    pub author_instruction: String,
    pub dimensions: Vec<String>,
    pub adopt_events: bool,
}
pub fn metadata_from_instruction(instruction: &str) -> CoreResult<WorkshopPacketMetadata> {
    let value: Value = serde_json::from_str(instruction)
        .map_err(|error| invalid(&format!("The workshop instruction is not JSON: {error}")))?;
    if value.get("schemaVersion").and_then(Value::as_str) != Some("story-workshop-request.v1") {
        return Err(invalid("The workshop instruction has an unknown schema."));
    }
    if let Some(brief) = value.get("authorBrief") {
        let brief = brief
            .as_str()
            .ok_or_else(|| invalid("The workshop author brief must be text."))?;
        validate_workshop_text(brief, MAX_WORKSHOP_TEXT_BYTES, "author brief")?;
    }
    let metadata = value
        .get("workshop")
        .ok_or_else(|| invalid("The workshop instruction has no frozen metadata."))?;
    let metadata: WorkshopPacketMetadata = serde_json::from_value(metadata.clone())
        .map_err(|error| invalid(&format!("The workshop metadata is invalid: {error}")))?;
    validate_exploration(&metadata.exploration)?;
    validate_story_possibilities(&metadata.story_possibilities)?;
    if metadata.story_possibilities.iter().any(|possibility| {
        possibility.status != StoryPossibilityStatus::Open || possibility.text.trim().is_empty()
    }) {
        return Err(invalid(
            "Frozen workshop story possibilities must be open and nonempty.",
        ));
    }
    validate_voice_guidance_metadata(&metadata)?;
    for (field, outer, embedded) in [
        (
            "action",
            value.get("action").and_then(Value::as_str),
            Some(metadata.exploration.action.as_str()),
        ),
        (
            "instruction",
            value.get("instruction").and_then(Value::as_str),
            Some(metadata.exploration.instruction.as_str()),
        ),
        (
            "selectedScope",
            value.get("selectedScope").and_then(Value::as_str),
            Some(metadata.exploration.selected_scope.as_str()),
        ),
        (
            "selectedText",
            value.get("selectedText").and_then(Value::as_str),
            Some(metadata.exploration.selected_text.as_str()),
        ),
        (
            "currentElement",
            value.get("currentElement").and_then(Value::as_str),
            Some(metadata.current_element.as_str()),
        ),
        (
            "direction",
            value.get("direction").and_then(Value::as_str),
            Some(metadata.direction.as_str()),
        ),
        (
            "stillOpen",
            value.get("stillOpen").and_then(Value::as_str),
            Some(metadata.still_open.as_str()),
        ),
        (
            "focusQuestion",
            value.get("focusQuestion").and_then(Value::as_str),
            Some(metadata.focus_question.as_str()),
        ),
        (
            "focusReason",
            value.get("focusReason").and_then(Value::as_str),
            Some(metadata.focus_reason.as_str()),
        ),
    ] {
        if outer != embedded {
            return Err(invalid(&format!(
                "The workshop instruction has mismatched {field} metadata."
            )));
        }
    }
    if value.get("outsideDirection").and_then(Value::as_bool) != Some(metadata.outside_direction) {
        return Err(invalid(
            "The workshop instruction has mismatched outsideDirection metadata.",
        ));
    }
    Ok(metadata)
}
pub fn is_voice_guidance_action(action: &str) -> bool {
    action == VOICE_GUIDANCE_ACTION
}
pub fn is_supported_action(action: &str) -> bool {
    WORKSHOP_ACTIONS.contains(&action)
}
pub fn validate_voice_guidance_metadata(metadata: &WorkshopPacketMetadata) -> CoreResult<()> {
    if is_voice_guidance_action(&metadata.exploration.action) {
        let guidance = metadata
            .voice_guidance
            .as_ref()
            .ok_or_else(|| invalid("Voice-guidance metadata is missing from the request."))?;
        if guidance.sample != metadata.exploration.selected_text
            || guidance.author_instruction != metadata.exploration.instruction
            || guidance.dimensions
                != VOICE_GUIDANCE_DIMENSIONS
                    .iter()
                    .map(|dimension| (*dimension).to_owned())
                    .collect::<Vec<_>>()
            || guidance.adopt_events
        {
            return Err(invalid(
                "Voice-guidance metadata does not match the frozen author request.",
            ));
        }
    } else if metadata.voice_guidance.is_some() {
        return Err(invalid(
            "Voice-guidance metadata is not allowed for another workshop action.",
        ));
    }
    Ok(())
}
pub fn validate_exploration(exploration: &WorkshopExploration) -> CoreResult<()> {
    validate_id(&exploration.session_id, "workshop session ID")?;
    validate_id(&exploration.expected_version, "workshop expected version")?;
    validate_id(
        &exploration.working_generation,
        "workshop working generation",
    )?;
    if exploration.action.trim().is_empty() {
        return Err(invalid("The workshop action is empty."));
    }
    validate_workshop_text(&exploration.action, 128, "workshop action")?;
    if !is_supported_action(&exploration.action) {
        return Err(invalid("The workshop action is unsupported."));
    }
    validate_workshop_text(
        &exploration.instruction,
        MAX_WORKSHOP_TEXT_BYTES,
        "workshop instruction",
    )?;
    if exploration.instruction.trim().is_empty() {
        return Err(invalid("The workshop instruction is empty."));
    }
    validate_workshop_text(&exploration.selected_scope, 256, "selected scope")?;
    validate_workshop_text(
        &exploration.selected_text,
        MAX_DETAIL_BYTES,
        "selected text",
    )?;
    if exploration.selected_scope.trim().is_empty() {
        return Err(invalid("The workshop selected scope is empty."));
    }
    if is_voice_guidance_action(&exploration.action) && exploration.selected_text.trim().is_empty()
    {
        return Err(invalid(
            "Voice guidance requires an author-selected or current sample.",
        ));
    }
    if let Some(selection) = &exploration.working_selection {
        validate_workshop_text(&selection.text, MAX_DETAIL_BYTES, "working selection")?;
        if selection.from > selection.to {
            return Err(invalid("The workshop working selection range is inverted."));
        }
    }
    Ok(())
}
pub fn validate_workshop_text(value: &str, limit: usize, label: &str) -> CoreResult<()> {
    if value.len() > limit
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(invalid(&format!(
            "The {label} is too large or contains control characters."
        )));
    }
    Ok(())
}
pub fn validate_id(value: &str, label: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(invalid(&format!("The {label} is invalid.")));
    }
    Ok(())
}
pub fn invalid(detail: &str) -> CoreError {
    CoreError::new("InvalidWorkshop", detail)
}
pub fn metadata_value(metadata: &WorkshopPacketMetadata) -> CoreResult<Value> {
    Ok(serde_json::to_value(metadata)?)
}

pub const MAX_STORY_POSSIBILITIES: usize = 64;
pub const MAX_STORY_POSSIBILITY_TEXT_CHARS: usize = 4000;
pub fn validate_text(value: &str, label: &str, max: usize) -> CoreResult<()> {
    if value.len() > max
        || value
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return Err(CoreError::new(
            "InvalidRequest",
            &format!("{label} is too long or contains a control character."),
        ));
    }
    Ok(())
}
pub fn validate_story_possibilities(values: &[StoryPossibility]) -> CoreResult<()> {
    if values.len() > MAX_STORY_POSSIBILITIES {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop session contains too many story possibilities.",
        ));
    }
    let mut ids = HashSet::new();
    for possibility in values {
        check_id(&possibility.id)?;
        if !ids.insert(&possibility.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Story possibility IDs must be unique within a workshop session.",
            ));
        }
        if possibility.text.chars().count() > MAX_STORY_POSSIBILITY_TEXT_CHARS {
            return Err(CoreError::new(
                "InvalidRequest",
                "Story possibility text must be at most 4000 characters.",
            ));
        }
        validate_text(&possibility.text, "story possibility", MAX_TEXT_BYTES)?;
    }
    Ok(())
}
pub const WORKSHOP_ACTIONS: &[&str] = &[
    "directions",
    "explore",
    "findDirection",
    "findDirections",
    "concrete",
    "consequences",
    "challenge",
    "ordinaryLife",
    "situation",
    "moment",
    VOICE_GUIDANCE_ACTION,
    "arc",
    "scale",
    "subvert",
    "synthesize",
];
pub const MAX_WORKSHOP_TEXT_BYTES: usize = 32 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024;
impl WorkshopPacketMetadata {
    pub fn from_context(
        exploration: WorkshopExploration,
        context: &WorkshopContext,
    ) -> CoreResult<Self> {
        validate_exploration(&exploration)?;
        if context.current_element.trim().is_empty() {
            return Err(invalid("The workshop current element is empty."));
        }
        for text in [
            &context.current_element,
            &context.direction,
            &context.still_open,
            &context.focus_question,
            &context.focus_reason,
        ] {
            validate_workshop_text(text, MAX_WORKSHOP_TEXT_BYTES, "workshop context")?;
        }
        validate_workshop_text(&context.original_notes, MAX_WORKSHOP_TEXT_BYTES, "original notes")?;
        if let Some(brief) = &context.author_brief {
            validate_workshop_text(brief, MAX_WORKSHOP_TEXT_BYTES, "author brief")?;
        }
        if context.questions.len() > 256 {
            return Err(invalid("The workshop question list is too large."));
        }
        for question in &context.questions {
            validate_id(&question.id, "workshop question ID")?;
            validate_workshop_text(&question.text, MAX_DETAIL_BYTES, "workshop question")?;
            validate_workshop_text(
                &question.reason,
                MAX_DETAIL_BYTES,
                "workshop question reason",
            )?;
        }
        validate_story_possibilities(&context.story_possibilities)?;
        validate_string_list(&context.chosen_details, MAX_WORKSHOP_TEXT_BYTES, "chosen details")?;
        validate_string_list(&context.fixed_details, MAX_WORKSHOP_TEXT_BYTES, "fixed details")?;
        validate_string_list(
            &context.fixed_source_refs,
            MAX_DETAIL_BYTES,
            "fixed source references",
        )?;
        validate_string_list(&context.preferences, MAX_WORKSHOP_TEXT_BYTES, "preferences")?;
        validate_string_list(
            &context.hard_constraints,
            MAX_WORKSHOP_TEXT_BYTES,
            "hard constraints",
        )?;
        validate_string_list(
            &context.included_alternatives,
            MAX_WORKSHOP_TEXT_BYTES,
            "included alternatives",
        )?;
        validate_string_list(
            &context.rejected_rationales,
            MAX_WORKSHOP_TEXT_BYTES,
            "rejection rationales",
        )?;
        for detail in &context.selected_details {
            validate_workshop_text(&detail.text, MAX_DETAIL_BYTES, "selected detail")?;
        }
        if let Some(relationship) = &context.relationship {
            validate_relationship_metadata(relationship)?;
        }
        let mut fixed = context.fixed_details.clone();
        fixed.extend(
            context
                .selected_details
                .iter()
                .filter(|detail| detail.fixed)
                .map(|detail| detail.text.clone()),
        );
        fixed.sort();
        fixed.dedup();
        let voice_guidance = if is_voice_guidance_action(&exploration.action) {
            Some(WorkshopVoiceGuidance {
                sample: exploration.selected_text.clone(),
                author_instruction: exploration.instruction.clone(),
                dimensions: VOICE_GUIDANCE_DIMENSIONS
                    .iter()
                    .map(|dimension| (*dimension).to_owned())
                    .collect(),
                adopt_events: false,
            })
        } else {
            None
        };
        Ok(Self {
            exploration,
            lens: context.lens,
            depth: context.depth,
            current_element: context.current_element.clone(),
            direction: context.direction.clone(),
            still_open: context.still_open.clone(),
            focus_question: context.focus_question.clone(),
            focus_reason: context.focus_reason.clone(),
            selected_details: context.selected_details.clone(),
            chosen_details: context.chosen_details.clone(),
            fixed_details: fixed,
            fixed_source_refs: context.fixed_source_refs.clone(),
            preferences: context.preferences.clone(),
            hard_constraints: context.hard_constraints.clone(),
            included_alternatives: context.included_alternatives.clone(),
            rejected_rationales: context.rejected_rationales.clone(),
            questions: context.questions.clone(),
            story_possibilities: context
                .story_possibilities
                .iter()
                .filter(|possibility| {
                    possibility.status == StoryPossibilityStatus::Open
                        && !possibility.text.trim().is_empty()
                })
                .cloned()
                .collect(),
            original_notes: context.original_notes.clone(),
            outside_direction: context.outside_direction,
            relationship: context.relationship.clone(),
            voice_guidance,
        })
    }
}
pub fn validate_string_list(values: &[String], limit: usize, label: &str) -> CoreResult<()> {
    if values.len() > 256 {
        return Err(invalid(&format!("The {label} list is too large.")));
    }
    for value in values {
        validate_workshop_text(value, limit, label)?;
    }
    Ok(())
}
pub fn validate_relationship_metadata(relationship: &WorkshopRelationship) -> CoreResult<()> {
    validate_id(&relationship.id, "relationship ID")?;
    validate_id(
        &relationship.from_document_id,
        "relationship source document ID",
    )?;
    validate_id(
        &relationship.to_document_id,
        "relationship target document ID",
    )?;
    if relationship.from_document_id == relationship.to_document_id {
        return Err(invalid("A relationship must have different endpoints."));
    }
    validate_workshop_text(
        &relationship.relationship_type,
        MAX_DETAIL_BYTES,
        "relationship type",
    )?;
    validate_workshop_text(
        &relationship.description,
        MAX_WORKSHOP_TEXT_BYTES,
        "relationship description",
    )?;
    validate_workshop_text(
        &relationship.uncertainty,
        MAX_DETAIL_BYTES,
        "relationship uncertainty",
    )?;
    if relationship.source_heads.len() != 2 {
        return Err(invalid("A relationship must retain both endpoint sources."));
    }
    let source_ids = relationship
        .source_heads
        .iter()
        .map(|head| head.document_id.as_str())
        .collect::<BTreeSet<_>>();
    if source_ids.len() != 2
        || !source_ids.contains(relationship.from_document_id.as_str())
        || !source_ids.contains(relationship.to_document_id.as_str())
    {
        return Err(invalid(
            "A relationship source must match both directional endpoints.",
        ));
    }
    Ok(())
}
