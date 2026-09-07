//! Story Workshop generation contracts.
//!
//! The project actor owns the workshop session and resolves its current CAS
//! state.  This module owns the small, provider-neutral request and response
//! contract used at that boundary.  It deliberately does not persist a
//! second candidate table: raw output remains in the existing discussion run
//! and is interpreted only for completed workshop runs.

use super::discussions::{FeedbackIntent, StartDiscussion};
use super::workshop::{
    CandidateChoiceStatus, Lens, WorkshopDepth, WorkshopPreference, WorkshopQuestion,
    WorkshopRelationship, WorkshopSession, WorkshopState,
};
pub use super::workshop::{WorkshopCandidate, WorkshopOutput};
use super::{CoreError, CoreResult, Head, ProjectAccess};
use crate::context::packet::{MockContextBudget, ProviderBinding};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const WORKSHOP_RESPONSE_CONTRACT: &str = "story-workshop-output.v1";
pub const WORKSHOP_SCHEMA_VERSION: &str = "story-workshop-output.v1";
const MAX_TEXT_BYTES: usize = 32 * 1024;
const MAX_DETAIL_BYTES: usize = 8 * 1024;
const MAX_CANDIDATE_BYTES: usize = 128 * 1024;
const MAX_CANDIDATES: usize = 3;
const VOICE_GUIDANCE_ACTION: &str = "voiceGuidance";
// Situation packets created before the three-choice contract used the
// generic refinement cardinality. Their immutable packet metadata is still
// readable, so keep that exact instruction prefix as a compatibility marker
// when projecting historical stored results.
const LEGACY_SITUATION_INSTRUCTION_PREFIX: &str = "Use one concrete situation to offer three contrasting tentative choices by this person or relationship. Show commitments and pressure through behavior, not a required trauma or biography.";
const VOICE_GUIDANCE_DIMENSIONS: [&str; 5] = [
    "Sentence density",
    "Viewpoint distance",
    "Humor",
    "Exposition",
    "Dialogue rhythm",
];
const WORKSHOP_ACTIONS: &[&str] = &[
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

/// The renderer sends only this exploration intent. The project actor fills
/// the remaining request fields from its current workshop state and anchor
/// document while holding the actor's CAS boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopWorkingSelection {
    pub from: u32,
    pub to: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A literal protected by the actor. The core checks exact string presence
/// only when the literal falls inside the editable response scope; semantic
/// preservation remains reviewable author work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopLiteral {
    pub text: String,
    pub fixed: bool,
}

/// A trusted snapshot assembled by the workshop actor. The IDs are resolved
/// to current project documents before this value reaches the packet builder;
/// renderer supplied IDs are never used as source authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopContext {
    pub expected: Head,
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
    pub included_document_ids: Vec<String>,
    pub included_alternatives: Vec<String>,
    pub rejected_rationales: Vec<String>,
    pub questions: Vec<WorkshopQuestion>,
    pub original_notes: String,
    pub outside_direction: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<WorkshopRelationship>,
}

/// Metadata retained inside the immutable discussion request and packet.
/// Optional fields keep historical discussion request and packet bytes
/// unchanged when this value is absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub original_notes: String,
    pub outside_direction: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_guidance: Option<WorkshopVoiceGuidance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<WorkshopRelationship>,
}

/// Frozen author material for a voice-guidance request. This is evidence for
/// reviewable style instructions only; it is never a canon or writing record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopVoiceGuidance {
    pub sample: String,
    pub author_instruction: String,
    pub dimensions: Vec<String>,
    pub adopt_events: bool,
}

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
            validate_text(text, MAX_TEXT_BYTES, "workshop context")?;
        }
        validate_text(&context.original_notes, MAX_TEXT_BYTES, "original notes")?;
        if context.questions.len() > 256 {
            return Err(invalid("The workshop question list is too large."));
        }
        for question in &context.questions {
            validate_id(&question.id, "workshop question ID")?;
            validate_text(&question.text, MAX_DETAIL_BYTES, "workshop question")?;
            validate_text(
                &question.reason,
                MAX_DETAIL_BYTES,
                "workshop question reason",
            )?;
        }
        validate_string_list(&context.chosen_details, MAX_TEXT_BYTES, "chosen details")?;
        validate_string_list(&context.fixed_details, MAX_TEXT_BYTES, "fixed details")?;
        validate_string_list(
            &context.fixed_source_refs,
            MAX_DETAIL_BYTES,
            "fixed source references",
        )?;
        validate_string_list(&context.preferences, MAX_TEXT_BYTES, "preferences")?;
        validate_string_list(
            &context.hard_constraints,
            MAX_TEXT_BYTES,
            "hard constraints",
        )?;
        validate_string_list(
            &context.included_alternatives,
            MAX_TEXT_BYTES,
            "included alternatives",
        )?;
        validate_string_list(
            &context.rejected_rationales,
            MAX_TEXT_BYTES,
            "rejection rationales",
        )?;
        for detail in &context.selected_details {
            validate_text(&detail.text, MAX_DETAIL_BYTES, "selected detail")?;
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
            original_notes: context.original_notes.clone(),
            outside_direction: context.outside_direction,
            relationship: context.relationship.clone(),
            voice_guidance,
        })
    }
}

/// The internal request assembled by the actor after resolving workshop CAS
/// state. It can be converted into the existing discussion lifecycle without
/// changing provider ownership, stop, recovery, or terminal persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopGenerationRequest {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub exploration: WorkshopExploration,
    pub context: WorkshopContext,
    pub budget: MockContextBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
}

/// Renderer-facing input consumed by the project actor. The actor resolves
/// the matching session and anchor, then expands this into
/// [`WorkshopGenerationRequest`] before calling the normal discussion start
/// lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartWorkshop {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub exploration: WorkshopExploration,
    pub budget: MockContextBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
}

/// Exact actor-resolved material that may be included in the immutable
/// request packet. Chosen decisions and saved alternatives remain separate so
/// an alternative cannot be presented as adopted merely because it is useful
/// for comparison.
#[derive(Debug, Clone, Default)]
pub struct WorkshopResolvedMaterial {
    pub chosen_details: Vec<String>,
    pub included_alternatives: Vec<String>,
    /// Exact content for a fully protected chosen decision whose author did
    /// not select narrower protected literals. It remains a context
    /// constraint and is only an output literal when it falls inside scope.
    pub fixed_details: Vec<String>,
    /// Exact directional relationship material resolved by the actor.
    pub relationship: Option<WorkshopRelationship>,
}

impl WorkshopGenerationRequest {
    pub fn into_discussion(self) -> CoreResult<(StartDiscussion, WorkshopPacketMetadata)> {
        validate_exploration(&self.exploration)?;
        validate_id(&self.operation_id, "operation ID")?;
        let metadata =
            WorkshopPacketMetadata::from_context(self.exploration.clone(), &self.context)?;
        let instruction = workshop_instruction(&self.exploration, &self.context)?;
        let pinned_document_ids = self.context.included_document_ids.clone();
        for document_id in &pinned_document_ids {
            validate_id(document_id, "included document ID")?;
        }
        let start = StartDiscussion {
            access: self.access,
            operation_id: self.operation_id,
            expected: self.context.expected,
            instruction,
            intent: FeedbackIntent::WorkshopExplore,
            basis: None,
            scope: None,
            pinned_document_ids,
            safe_brief: None,
            budget: self.budget,
            provider_binding: self.provider_binding,
            previous_run_id: None,
            lookup: None,
        };
        Ok((start, metadata))
    }
}

/// Build the generation request from the actor-owned workshop state. The
/// caller must resolve `expected` from the session's current anchor document
/// while holding the same actor turn that checks the session/version CAS.
pub fn from_session(
    request: StartWorkshop,
    session: &WorkshopSession,
    state: &WorkshopState,
    expected: Head,
) -> CoreResult<WorkshopGenerationRequest> {
    from_session_with_material(
        request,
        session,
        state,
        expected,
        WorkshopResolvedMaterial::default(),
    )
}

/// Variant used by the actor after resolving durable chosen decisions and
/// selected alternatives. The renderer can name included documents, but only
/// these actor-owned lists of exact content are allowed into the immutable
/// packet. Keep the lists separate: a saved alternative is not an adopted
/// decision merely because it is deliberately included in context.
pub fn from_session_with_material(
    request: StartWorkshop,
    session: &WorkshopSession,
    state: &WorkshopState,
    expected: Head,
    resolved_material: WorkshopResolvedMaterial,
) -> CoreResult<WorkshopGenerationRequest> {
    let StartWorkshop {
        access,
        operation_id,
        exploration,
        budget,
        provider_binding,
    } = request;
    if session.id != exploration.session_id {
        return Err(invalid(
            "The workshop session does not match the exploration.",
        ));
    }
    if session.working_generation != exploration.working_generation {
        return Err(invalid(
            "The workshop working version changed before generation.",
        ));
    }
    if is_legacy_situation_instruction(&exploration) {
        return Err(invalid(
            "This situation prompt is outdated. Refresh the situation prompt before starting a new request.",
        ));
    }
    validate_working_selection(&exploration, &session.working_text)?;
    let relationship = resolved_material.relationship;
    match (session.relationship_id.as_deref(), relationship.as_ref()) {
        (None, None) => {}
        (Some(session_id), Some(relationship)) if session_id == relationship.id => {
            validate_relationship_metadata(relationship)?;
        }
        (Some(_), None) => {
            return Err(invalid(
                "The relationship exploration is missing its validated relationship.",
            ));
        }
        (None, Some(_)) | (Some(_), Some(_)) => {
            return Err(invalid(
                "The relationship does not belong to this workshop session.",
            ));
        }
    }
    let selected_details = session
        .selected_details
        .iter()
        .map(|detail| WorkshopLiteral {
            text: detail.text.clone(),
            fixed: detail.fixed,
        })
        .collect::<Vec<_>>();
    let rejected_rationales = session
        .choices
        .iter()
        .filter(|choice| choice.status == CandidateChoiceStatus::Rejected)
        .map(|choice| format!("{}: {}", choice.candidate_id, choice.rationale))
        .collect::<Vec<_>>();
    let preferences = state
        .preferences
        .iter()
        .filter(|preference| {
            preference.confirmed && preference_applies(preference, session, relationship.as_ref())
        })
        .map(format_preference)
        .collect::<Vec<_>>();
    let hard_constraints = state
        .preferences
        .iter()
        .filter(|preference| {
            preference.confirmed
                && preference.strength == super::workshop::PreferenceStrength::Hard
                && preference.polarity != super::workshop::PreferencePolarity::Neutral
                && preference_applies(preference, session, relationship.as_ref())
        })
        .map(|preference| format!("hard constraint: {}", format_preference(preference)))
        .collect::<Vec<_>>();
    let mut fixed_details = session
        .selected_details
        .iter()
        .filter(|detail| detail.fixed)
        .map(|detail| detail.text.clone())
        .collect::<Vec<_>>();
    fixed_details.extend(resolved_material.fixed_details);
    let mut fixed_source_refs = Vec::new();
    // Keep fixed protection remains durable across decision status changes,
    // but only relevant source decisions belong in this request packet. The
    // actor resolves exact source revisions before calling this builder.
    for decision in state.decisions.iter().filter(|decision| {
        decision.fixed
            && super::workshop::fixed_decision_is_relevant(
                state,
                session,
                decision,
                relationship.as_ref(),
            )
    }) {
        fixed_details.extend(decision.protected_text.iter().cloned());
        fixed_source_refs.push(format!(
            "{}@{}",
            decision.document_id, decision.head.version
        ));
    }
    fixed_details.sort();
    fixed_details.dedup();
    fixed_source_refs.sort();
    fixed_source_refs.dedup();
    let current_element = [
        session.working_text.as_str(),
        session.brief.as_str(),
        session.composer.as_str(),
    ]
    .into_iter()
    .find(|value| !value.trim().is_empty())
    .unwrap_or("No working story element has been chosen yet.")
    .to_owned();
    let mut included_document_ids = session.included_document_ids.clone();
    if let Some(relationship) = relationship.as_ref() {
        for document_id in [&relationship.from_document_id, &relationship.to_document_id] {
            if !included_document_ids
                .iter()
                .any(|existing| existing == document_id)
            {
                included_document_ids.push(document_id.clone());
            }
        }
    }
    let context = WorkshopContext {
        expected,
        lens: session.lens,
        depth: session.depth,
        current_element,
        direction: session.direction.clone(),
        still_open: session.still_open.clone(),
        focus_question: session.focus_question.clone(),
        focus_reason: session.focus_reason.clone(),
        selected_details,
        chosen_details: resolved_material.chosen_details,
        fixed_details,
        fixed_source_refs,
        preferences,
        hard_constraints,
        included_document_ids,
        included_alternatives: resolved_material.included_alternatives,
        rejected_rationales,
        questions: session.questions.clone(),
        original_notes: session.original_notes.clone(),
        outside_direction: session.outside_direction,
        relationship,
    };
    Ok(WorkshopGenerationRequest {
        access,
        operation_id,
        exploration,
        context,
        budget,
        provider_binding,
    })
}

fn workshop_instruction(
    exploration: &WorkshopExploration,
    context: &WorkshopContext,
) -> CoreResult<String> {
    let value = serde_json::json!({
        "schemaVersion": "story-workshop-request.v1",
        "workshop": WorkshopPacketMetadata::from_context(exploration.clone(), context)?,
        "action": exploration.action,
        "instruction": exploration.instruction,
        "selectedScope": exploration.selected_scope,
        "selectedText": exploration.selected_text,
        "currentElement": context.current_element,
        "direction": context.direction,
        "stillOpen": context.still_open,
        "focusQuestion": context.focus_question,
        "focusReason": context.focus_reason,
        "outsideDirection": context.outside_direction,
    });
    let encoded = serde_json::to_string(&value)?;
    if encoded.len() > MAX_TEXT_BYTES {
        return Err(invalid("The workshop request instruction is too large."));
    }
    Ok(encoded)
}

/// Decode the immutable workshop envelope embedded in the trusted final
/// instruction. The actor is responsible for constructing this instruction
/// from the current workshop CAS state before calling `start_discussion`.
pub fn metadata_from_instruction(instruction: &str) -> CoreResult<WorkshopPacketMetadata> {
    let value: Value = serde_json::from_str(instruction)
        .map_err(|error| invalid(&format!("The workshop instruction is not JSON: {error}")))?;
    if value.get("schemaVersion").and_then(Value::as_str) != Some("story-workshop-request.v1") {
        return Err(invalid("The workshop instruction has an unknown schema."));
    }
    let metadata = value
        .get("workshop")
        .ok_or_else(|| invalid("The workshop instruction has no frozen metadata."))?;
    let metadata: WorkshopPacketMetadata = serde_json::from_value(metadata.clone())
        .map_err(|error| invalid(&format!("The workshop metadata is invalid: {error}")))?;
    validate_exploration(&metadata.exploration)?;
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

/// Validate a completed workshop response. `run_id` is used to assign stable
/// candidate IDs, so IDs stay durable and cannot collide across runs.
pub fn validate_workshop_output(
    raw: &str,
    metadata: &WorkshopPacketMetadata,
    run_id: &str,
) -> CoreResult<WorkshopOutput> {
    validate_id(run_id, "run ID")?;
    if raw.len() > MAX_CANDIDATE_BYTES {
        return Err(invalid("The workshop response is too large."));
    }
    let mut output: WorkshopOutput = serde_json::from_str(raw)
        .map_err(|error| invalid(&format!("The workshop response is not valid JSON: {error}")))?;
    if output.schema_version != WORKSHOP_SCHEMA_VERSION {
        return Err(invalid(
            "The workshop response schema version is unsupported.",
        ));
    }
    let expected_kind = if is_direction_action(&metadata.exploration.action) {
        "directions"
    } else {
        "refinement"
    };
    if output.request_kind != expected_kind {
        return Err(invalid(
            "The workshop response kind does not match the requested action.",
        ));
    }
    if output.request_kind.trim().is_empty()
        || output.question.trim().is_empty()
        || output.question_reason.trim().is_empty()
        || output.dimension.trim().is_empty()
    {
        return Err(invalid(
            "The workshop response is missing a required question field.",
        ));
    }
    validate_text(&output.request_kind, 256, "request kind")?;
    validate_text(&output.question, MAX_TEXT_BYTES, "question")?;
    validate_text(&output.question_reason, MAX_TEXT_BYTES, "question reason")?;
    validate_text(&output.dimension, 256, "dimension")?;
    for text in [
        &output.interpretation.you_said,
        &output.interpretation.possible_direction,
        &output.interpretation.still_open,
    ] {
        validate_text(text, MAX_TEXT_BYTES, "interpretation")?;
    }
    let (expected, cardinality_message) = if is_direction_action(&metadata.exploration.action)
        || is_voice_guidance_action(&metadata.exploration.action)
    {
        (
            3..=3,
            "A direction or voice-guidance workshop response must contain exactly three candidates.",
        )
    } else if metadata.exploration.action == "situation"
        && !is_legacy_situation_instruction(&metadata.exploration)
    {
        (
            3..=3,
            "A situation workshop response must contain exactly three tentative choices.",
        )
    } else if metadata.exploration.action == "moment" {
        (
            2..=MAX_CANDIDATES,
            "A moment workshop response must contain two or three treatments.",
        )
    } else {
        (
            1..=MAX_CANDIDATES,
            "A workshop refinement response must contain one to three candidates.",
        )
    };
    if !expected.contains(&output.candidates.len()) {
        return Err(invalid(cardinality_message));
    }
    if output.candidates.len() > MAX_CANDIDATES {
        return Err(invalid(
            "The workshop response contains too many candidates.",
        ));
    }
    let mut dimensions = BTreeSet::new();
    for (index, candidate) in output.candidates.iter_mut().enumerate() {
        validate_candidate(candidate, metadata, run_id, index)?;
        if is_voice_guidance_action(&metadata.exploration.action) {
            validate_voice_guidance_candidate(candidate, metadata)?;
        }
        dimensions.insert(candidate.dimension_value.to_ascii_lowercase());
        candidate.id = format!("{run_id}-{index}");
    }
    if dimensions.len() != output.candidates.len() {
        return Err(invalid(
            "Workshop candidates must differ on their declared dimension.",
        ));
    }
    Ok(output)
}

fn validate_candidate(
    candidate: &WorkshopCandidate,
    metadata: &WorkshopPacketMetadata,
    run_id: &str,
    index: usize,
) -> CoreResult<()> {
    for (value, label, limit) in [
        (&candidate.title, "candidate title", 256),
        (&candidate.content, "candidate content", MAX_TEXT_BYTES),
        (&candidate.dimension_value, "candidate dimension", 512),
    ] {
        validate_text(value, limit, label)?;
        if value.trim().is_empty() {
            return Err(invalid(&format!("The {label} is empty.")));
        }
    }
    if candidate.content.len() > MAX_CANDIDATE_BYTES {
        return Err(invalid("A workshop candidate is too large."));
    }
    validate_string_list(
        &candidate.assumptions,
        MAX_TEXT_BYTES,
        "candidate assumptions",
    )?;
    validate_string_list(
        &candidate.preserved_details,
        MAX_TEXT_BYTES,
        "preserved details",
    )?;
    validate_string_list(
        &candidate.changed_details,
        MAX_TEXT_BYTES,
        "changed details",
    )?;
    if candidate.changed_details.is_empty() {
        return Err(invalid(
            "A workshop candidate must describe at least one changed detail.",
        ));
    }
    for implication in &candidate.implications {
        for (value, label) in [
            (&implication.text, "implication"),
            (&implication.basis, "implication basis"),
            (&implication.assumption, "implication assumption"),
        ] {
            validate_text(value, MAX_DETAIL_BYTES, label)?;
            if value.trim().is_empty() {
                return Err(invalid(&format!("The {label} is empty.")));
            }
        }
    }
    for target in &candidate.affected_targets {
        validate_id(&target.document_id, "affected document ID")?;
        validate_text(&target.reason, MAX_DETAIL_BYTES, "affected target reason")?;
    }
    // Voice guidance describes style; it does not rewrite the selected sample
    // or working story, so protected story literals remain context constraints
    // and must not be forced into STYLE instructions.
    if !is_voice_guidance_action(&metadata.exploration.action) {
        let editable_scope = editable_scope_text(metadata);
        let whole_or_synthesis = is_whole_or_synthesis_scope(&metadata.exploration.selected_scope)
            && metadata.exploration.working_selection.is_none();
        let mut required = metadata
            .fixed_details
            .iter()
            .filter(|literal| editable_scope.contains(literal.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        // A whole-work synthesis can include selected tray material even when
        // the current element does not repeat it verbatim. Scoped replacements
        // must leave fixed facts outside their captured range to the surrounding
        // working text and must not be forced to duplicate them.
        if whole_or_synthesis {
            required.extend(
                metadata
                    .selected_details
                    .iter()
                    .filter(|detail| detail.fixed)
                    .map(|detail| detail.text.clone()),
            );
        }
        required.sort();
        required.dedup();
        for literal in required {
            if !literal.is_empty() && !candidate.content.contains(&literal) {
                return Err(invalid(&format!(
                    "Candidate {} does not preserve the fixed detail in its editable scope.",
                    index + 1
                )));
            }
        }
    }
    let _ = run_id;
    Ok(())
}

fn editable_scope_text(metadata: &WorkshopPacketMetadata) -> &str {
    if let Some(selection) = metadata.exploration.working_selection.as_ref() {
        return selection.text.as_str();
    }
    if !is_whole_or_synthesis_scope(&metadata.exploration.selected_scope)
        && !metadata.exploration.selected_text.trim().is_empty()
    {
        return metadata.exploration.selected_text.as_str();
    }
    metadata.current_element.as_str()
}

fn is_whole_or_synthesis_scope(scope: &str) -> bool {
    let scope = scope.trim().to_ascii_lowercase();
    scope.is_empty()
        || scope.contains("whole")
        || scope.contains("working version")
        || scope.contains("synthesis")
        || scope.contains("entire")
        || scope.contains("complete")
}

fn is_direction_action(action: &str) -> bool {
    matches!(
        action,
        "directions" | "explore" | "findDirection" | "findDirections"
    )
}

fn is_voice_guidance_action(action: &str) -> bool {
    action == VOICE_GUIDANCE_ACTION
}

fn is_legacy_situation_instruction(exploration: &WorkshopExploration) -> bool {
    exploration.action == "situation"
        && exploration
            .instruction
            .trim_start()
            .starts_with(LEGACY_SITUATION_INSTRUCTION_PREFIX)
}

fn is_supported_action(action: &str) -> bool {
    WORKSHOP_ACTIONS.contains(&action)
}

fn validate_voice_guidance_candidate(
    candidate: &WorkshopCandidate,
    metadata: &WorkshopPacketMetadata,
) -> CoreResult<()> {
    let content = candidate.content.to_ascii_lowercase();
    if !content.contains("style") {
        return Err(invalid(
            "A voice-guidance candidate must contain explicit STYLE instructions.",
        ));
    }
    for dimension in VOICE_GUIDANCE_DIMENSIONS {
        if !content.contains(&dimension.to_ascii_lowercase()) {
            return Err(invalid(&format!(
                "A voice-guidance candidate must address {dimension}."
            )));
        }
    }
    let sample = metadata.exploration.selected_text.trim();
    if !sample.is_empty() && candidate.content.contains(sample) {
        return Err(invalid(
            "A voice-guidance candidate must not copy the sample or its events into style instructions.",
        ));
    }
    Ok(())
}

fn validate_voice_guidance_metadata(metadata: &WorkshopPacketMetadata) -> CoreResult<()> {
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

fn preference_applies(
    preference: &WorkshopPreference,
    session: &WorkshopSession,
    relationship: Option<&WorkshopRelationship>,
) -> bool {
    let targets_element = preference.target_id.as_deref().is_some_and(|target| {
        session.focus_document_id.as_deref() == Some(target)
            || relationship.is_some_and(|relationship| {
                relationship.from_document_id == target || relationship.to_document_id == target
            })
    });
    matches!(preference.scope, super::workshop::PreferenceScope::Project)
        || (preference.scope == super::workshop::PreferenceScope::Element && targets_element)
        || (preference.scope == super::workshop::PreferenceScope::Exploration
            && preference.target_id.as_deref() == Some(session.id.as_str()))
}

fn format_preference(preference: &WorkshopPreference) -> String {
    let polarity = match preference.polarity {
        super::workshop::PreferencePolarity::Want => "want",
        super::workshop::PreferencePolarity::Avoid => "avoid",
        super::workshop::PreferencePolarity::Neutral => "neutral",
    };
    let scope = match preference.scope {
        super::workshop::PreferenceScope::Project => "project",
        super::workshop::PreferenceScope::Element => "element",
        super::workshop::PreferenceScope::Exploration => "exploration",
    };
    let strength = match preference.strength {
        super::workshop::PreferenceStrength::Soft => "soft",
        super::workshop::PreferenceStrength::Hard => "hard",
    };
    let mut value = format!(
        "{polarity} {} [{}; scope={scope}; strength={strength}]",
        preference.label, preference.meaning
    );
    if !preference.examples.trim().is_empty() {
        value.push_str(&format!("; examples={}", preference.examples));
    }
    if !preference.timing.trim().is_empty() {
        value.push_str(&format!("; timing={}", preference.timing));
    }
    value
}

fn validate_exploration(exploration: &WorkshopExploration) -> CoreResult<()> {
    validate_id(&exploration.session_id, "workshop session ID")?;
    validate_id(&exploration.expected_version, "workshop expected version")?;
    validate_id(
        &exploration.working_generation,
        "workshop working generation",
    )?;
    if exploration.action.trim().is_empty() {
        return Err(invalid("The workshop action is empty."));
    }
    validate_text(&exploration.action, 128, "workshop action")?;
    if !is_supported_action(&exploration.action) {
        return Err(invalid("The workshop action is unsupported."));
    }
    validate_text(
        &exploration.instruction,
        MAX_TEXT_BYTES,
        "workshop instruction",
    )?;
    if exploration.instruction.trim().is_empty() {
        return Err(invalid("The workshop instruction is empty."));
    }
    validate_text(&exploration.selected_scope, 256, "selected scope")?;
    validate_text(
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
        validate_text(&selection.text, MAX_DETAIL_BYTES, "working selection")?;
        if selection.from > selection.to {
            return Err(invalid("The workshop working selection range is inverted."));
        }
    }
    Ok(())
}

fn validate_working_selection(
    exploration: &WorkshopExploration,
    working_text: &str,
) -> CoreResult<()> {
    let Some(selection) = &exploration.working_selection else {
        return Ok(());
    };
    let captured = utf16_slice(working_text, selection.from, selection.to).ok_or_else(|| {
        invalid("The workshop working selection is outside the saved working text.")
    })?;
    if captured != selection.text || captured != exploration.selected_text {
        return Err(invalid(
            "The workshop working selection no longer matches the saved working text.",
        ));
    }
    Ok(())
}

fn utf16_slice(value: &str, from: u32, to: u32) -> Option<String> {
    if from > to {
        return None;
    }
    let mut start = None;
    let mut end = None;
    let mut offset = 0u32;
    for (byte, character) in value.char_indices() {
        if offset == from {
            start = Some(byte);
        }
        if offset == to {
            end = Some(byte);
        }
        offset = offset.checked_add(character.len_utf16() as u32)?;
    }
    if offset == from {
        start = Some(value.len());
    }
    if offset == to {
        end = Some(value.len());
    }
    Some(value.get(start?..end?)?.to_owned())
}

fn validate_string_list(values: &[String], limit: usize, label: &str) -> CoreResult<()> {
    if values.len() > 256 {
        return Err(invalid(&format!("The {label} list is too large.")));
    }
    for value in values {
        validate_text(value, limit, label)?;
    }
    Ok(())
}

fn validate_text(value: &str, limit: usize, label: &str) -> CoreResult<()> {
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

fn validate_id(value: &str, label: &str) -> CoreResult<()> {
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

fn invalid(detail: &str) -> CoreError {
    CoreError::new("InvalidWorkshop", detail)
}

fn validate_relationship_metadata(relationship: &WorkshopRelationship) -> CoreResult<()> {
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
    validate_text(
        &relationship.relationship_type,
        MAX_DETAIL_BYTES,
        "relationship type",
    )?;
    validate_text(
        &relationship.description,
        MAX_TEXT_BYTES,
        "relationship description",
    )?;
    validate_text(
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

/// Packet metadata is JSON by design so old packet rows can be read without
/// knowing this feature. This helper is useful to packet and transfer code.
pub fn metadata_value(metadata: &WorkshopPacketMetadata) -> CoreResult<Value> {
    Ok(serde_json::to_value(metadata)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::workshop::WorkshopSession;
    use crate::projects::workshop::{Lens, WorkshopBranchKind, WorkshopDepth};

    fn exploration(action: &str) -> WorkshopExploration {
        WorkshopExploration {
            session_id: "session-1".into(),
            expected_version: "7".into(),
            working_generation: "generation-1".into(),
            action: action.into(),
            instruction: "Keep the selected pressure visible.".into(),
            selected_scope: "Whole working version".into(),
            selected_text: "The editable selected passage".into(),
            working_selection: None,
        }
    }

    fn context() -> WorkshopContext {
        WorkshopContext {
            expected: Head {
                document_id: "workshop-session-1".into(),
                version: "7".into(),
                body_hash: "hash".into(),
            },
            lens: Lens::Possibilities,
            depth: WorkshopDepth::Develop,
            current_element: "The current story element".into(),
            direction: "An editable direction".into(),
            still_open: "Its consequences remain open".into(),
            focus_question: "What changes next?".into(),
            focus_reason: "The author is comparing mechanisms.".into(),
            selected_details: vec![WorkshopLiteral {
                text: "The protected detail".into(),
                fixed: true,
            }],
            chosen_details: vec!["saved-choice: Keep the quiet routine".into()],
            fixed_details: vec!["The fixed project constraint".into()],
            fixed_source_refs: vec!["world-1@3".into()],
            preferences: vec!["want ordinary life".into()],
            hard_constraints: vec!["must preserve the protected detail".into()],
            included_document_ids: vec!["world-1".into()],
            included_alternatives: vec!["saved-choice".into()],
            rejected_rationales: vec!["rejected-choice: too narrow".into()],
            questions: vec![WorkshopQuestion {
                id: "question-1".into(),
                text: "What should remain unknown?".into(),
                reason: "Keep the mystery alive while comparing directions.".into(),
                status: super::super::workshop::WorkshopQuestionStatus::KeepMysterious,
                unknown_to: super::super::workshop::UnknownTo::Both,
            }],
            original_notes: "A note the author intentionally brought into this exploration.".into(),
            outside_direction: false,
            relationship: None,
        }
    }

    fn start_request(action: &str) -> StartWorkshop {
        StartWorkshop {
            access: ProjectAccess {
                project_id: "project-1".into(),
                session: "renderer-1".into(),
                writer_lease: "lease-1".into(),
                operation_namespace: "namespace-1".into(),
            },
            operation_id: "operation-1".into(),
            exploration: exploration(action),
            budget: MockContextBudget::new("32000", "8000", "1000"),
            provider_binding: None,
        }
    }

    fn candidate(dimension: &str) -> Value {
        serde_json::json!({
            "id": "",
            "title": format!("Direction {dimension}"),
            "content": format!("The current story element keeps The protected detail and The fixed project constraint. {dimension} changes the mechanism."),
            "dimensionValue": dimension,
            "implications": [{
                "text": "A related choice may move.",
                "basis": "the selected pressure",
                "assumption": "the author wants this possibility explored"
            }],
            "assumptions": ["This remains a noncanon alternative."],
            "affectedTargets": [],
            "preservedDetails": ["The protected detail", "The fixed project constraint"],
            "changedDetails": [dimension]
        })
    }

    fn metadata(action: &str) -> WorkshopPacketMetadata {
        WorkshopPacketMetadata::from_context(exploration(action), &context()).unwrap()
    }

    #[test]
    fn generation_envelope_round_trips_and_keeps_outer_fields_bound() {
        let request = WorkshopGenerationRequest {
            access: ProjectAccess {
                project_id: "project-1".into(),
                session: "renderer-1".into(),
                writer_lease: "lease-1".into(),
                operation_namespace: "namespace-1".into(),
            },
            operation_id: "operation-1".into(),
            exploration: exploration("directions"),
            context: context(),
            budget: MockContextBudget::new("32000", "8000", "1000"),
            provider_binding: None,
        };
        let (start, expected) = request.into_discussion().unwrap();
        assert_eq!(start.intent, FeedbackIntent::WorkshopExplore);
        assert_eq!(
            metadata_from_instruction(&start.instruction).unwrap(),
            expected
        );
        assert_eq!(expected.lens, Lens::Possibilities);
        assert_eq!(expected.depth, WorkshopDepth::Develop);
        assert_eq!(expected.questions.len(), 1);
        assert_eq!(
            expected.questions[0].status,
            super::super::workshop::WorkshopQuestionStatus::KeepMysterious
        );
        assert_eq!(
            expected.questions[0].unknown_to,
            super::super::workshop::UnknownTo::Both
        );
        assert!(expected.original_notes.contains("intentionally brought"));

        let mut tampered: Value = serde_json::from_str(&start.instruction).unwrap();
        tampered["selectedText"] = Value::String("renderer tamper".into());
        let error = metadata_from_instruction(&tampered.to_string()).unwrap_err();
        assert_eq!(error.code, "InvalidWorkshop");
    }

    #[test]
    fn completed_direction_output_gets_stable_ids_and_requires_three_axes() {
        let metadata = metadata("directions");
        let raw = serde_json::json!({
            "schemaVersion": WORKSHOP_SCHEMA_VERSION,
            "requestKind": "directions",
            "question": "What changes next?",
            "questionReason": "The author is comparing mechanisms.",
            "dimension": "Mechanism",
            "interpretation": {
                "youSaid": "An editable direction",
                "possibleDirection": "The current story element",
                "stillOpen": "Its consequences remain open"
            },
            "candidates": [candidate("routine"), candidate("institution"), candidate("relationship")]
        })
        .to_string();
        let output = validate_workshop_output(&raw, &metadata, "run-1").unwrap();
        assert_eq!(
            output
                .candidates
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect::<Vec<_>>(),
            vec!["run-1-0", "run-1-1", "run-1-2"]
        );
        assert!(
            output
                .candidates
                .iter()
                .all(|candidate| !candidate.content.contains("The editable selected passage"))
        );

        let bad = serde_json::json!({
            "schemaVersion": WORKSHOP_SCHEMA_VERSION,
            "requestKind": "directions",
            "question": "What changes next?",
            "questionReason": "The author is comparing mechanisms.",
            "dimension": "Mechanism",
            "interpretation": {"youSaid":"", "possibleDirection":"", "stillOpen":""},
            "candidates": [candidate("same"), candidate("same"), candidate("same")]
        });
        assert!(validate_workshop_output(&bad.to_string(), &metadata, "run-2").is_err());
    }

    #[test]
    fn moment_output_requires_two_or_three_treatments_but_other_refinements_allow_one() {
        let response = |dimensions: &[&str]| {
            serde_json::json!({
                "schemaVersion": WORKSHOP_SCHEMA_VERSION,
                "requestKind": "refinement",
                "question": "Which treatment fits?",
                "questionReason": "Compare how the same situation feels.",
                "dimension": "Treatment",
                "interpretation": {
                    "youSaid": "A situation to test",
                    "possibleDirection": "A treatment to compare",
                    "stillOpen": "The author decides what to keep"
                },
                "candidates": dimensions.iter().map(|dimension| candidate(dimension)).collect::<Vec<_>>()
            })
            .to_string()
        };
        let moment_metadata = metadata("moment");
        let one_error =
            validate_workshop_output(&response(&["intimate"]), &moment_metadata, "run-moment-one")
                .unwrap_err();
        assert!(one_error.detail.contains("two or three"));
        assert_eq!(
            validate_workshop_output(
                &response(&["intimate", "wondrous"]),
                &moment_metadata,
                "run-moment-two"
            )
            .unwrap()
            .candidates
            .len(),
            2
        );
        assert_eq!(
            validate_workshop_output(
                &response(&["intimate", "wondrous", "brisk"]),
                &moment_metadata,
                "run-moment-three"
            )
            .unwrap()
            .candidates
            .len(),
            3
        );
        assert_eq!(
            validate_workshop_output(
                &response(&["concrete"]),
                &metadata("concrete"),
                "run-refinement-one"
            )
            .unwrap()
            .candidates
            .len(),
            1
        );
    }

    #[test]
    fn situation_output_requires_three_choices_without_rejecting_legacy_stored_results() {
        let response = |dimensions: &[&str]| {
            serde_json::json!({
                "schemaVersion": WORKSHOP_SCHEMA_VERSION,
                "requestKind": "refinement",
                "question": "What will they do under pressure?",
                "questionReason": "Compare how the same situation reveals commitments.",
                "dimension": "Choice",
                "interpretation": {
                    "youSaid": "A situation to test",
                    "possibleDirection": "A tentative response",
                    "stillOpen": "The author decides what fits"
                },
                "candidates": dimensions.iter().map(|dimension| candidate(dimension)).collect::<Vec<_>>()
            })
            .to_string()
        };
        let current = metadata("situation");
        assert_eq!(
            validate_workshop_output(
                &response(&["withdraw", "negotiate", "confront"]),
                &current,
                "run-situation-three"
            )
            .unwrap()
            .candidates
            .len(),
            3
        );
        for (dimensions, run_id) in [
            (&["withdraw"][..], "run-situation-one"),
            (&["withdraw", "negotiate"][..], "run-situation-two"),
            (
                &["withdraw", "negotiate", "confront", "leave"][..],
                "run-situation-four",
            ),
        ] {
            let error =
                validate_workshop_output(&response(dimensions), &current, run_id).unwrap_err();
            assert!(error.detail.contains("exactly three"));
        }

        let mut legacy = exploration("situation");
        legacy.instruction = LEGACY_SITUATION_INSTRUCTION_PREFIX.into();
        let legacy_context = context();
        let legacy_instruction = workshop_instruction(&legacy, &legacy_context).unwrap();
        let legacy_metadata = metadata_from_instruction(&legacy_instruction).unwrap();
        assert_eq!(
            validate_workshop_output(
                &response(&["withdraw"]),
                &legacy_metadata,
                "run-situation-legacy"
            )
            .unwrap()
            .candidates
            .len(),
            1
        );

        let session = WorkshopSession {
            id: "session-1".into(),
            title: "Situation test".into(),
            lens: Lens::Possibilities,
            parent_session_id: None,
            branch_kind: WorkshopBranchKind::Working,
            brief: "A bounded test exploration".into(),
            direction: String::new(),
            still_open: String::new(),
            focus_question: String::new(),
            focus_reason: String::new(),
            focus_document_id: None,
            anchor_document_id: Some("workshop-session-1".into()),
            depth: WorkshopDepth::Develop,
            outside_direction: false,
            included_document_ids: Vec::new(),
            working_text: String::new(),
            working_title: String::new(),
            working_generation: "generation-1".into(),
            selected_details: Vec::new(),
            choices: Vec::new(),
            questions: Vec::new(),
            composer: String::new(),
            selected_scope: "Whole working version".into(),
            original_notes: String::new(),
            active_run_id: None,
            relationship_id: None,
        };
        let state = WorkshopState {
            schema_version: 1,
            current_session_id: Some(session.id.clone()),
            sessions: vec![session.clone()],
            ..WorkshopState::default()
        };
        let mut new_request = start_request("situation");
        new_request.exploration.instruction = LEGACY_SITUATION_INSTRUCTION_PREFIX.into();
        new_request.exploration.selected_text.clear();
        let error = from_session_with_material(
            new_request,
            &session,
            &state,
            Head {
                document_id: "workshop-session-1".into(),
                version: "7".into(),
                body_hash: "hash".into(),
            },
            WorkshopResolvedMaterial::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, "InvalidWorkshop");
        assert!(error.detail.contains("Refresh the situation prompt"));
    }

    #[test]
    fn voice_guidance_freezes_sample_and_requires_three_style_sets() {
        let sample = "Rain ticked against the workshop glass while she counted each drop.";
        let mut request = start_request(VOICE_GUIDANCE_ACTION);
        request.exploration.selected_text = sample.into();
        request.exploration.instruction =
            "Use the sample as voice evidence. The author prefers restrained warmth.".into();
        let generation = WorkshopGenerationRequest {
            access: request.access,
            operation_id: request.operation_id,
            exploration: request.exploration,
            context: context(),
            budget: request.budget,
            provider_binding: request.provider_binding,
        };
        let (start, expected) = generation.into_discussion().unwrap();
        assert_eq!(start.intent, FeedbackIntent::WorkshopExplore);
        assert!(start.basis.is_none());
        assert!(start.scope.is_none());
        assert!(start.safe_brief.is_none());
        assert!(start.previous_run_id.is_none());
        let frozen = metadata_from_instruction(&start.instruction).unwrap();
        let guidance = frozen.voice_guidance.as_ref().unwrap();
        assert_eq!(guidance.sample, sample);
        assert_eq!(
            guidance.author_instruction,
            expected.exploration.instruction
        );
        assert_eq!(guidance.dimensions.len(), VOICE_GUIDANCE_DIMENSIONS.len());
        assert!(!guidance.adopt_events);

        let style_candidate = |dimension: &str| {
            let mut value = candidate(dimension);
            value["content"] = Value::String(format!(
                "STYLE guidance\nSentence density: use {dimension} sentence lengths.\nViewpoint distance: stay close to perception.\nHumor: use restrained warmth.\nExposition: reveal context through selected detail.\nDialogue rhythm: let turns breathe."
            ));
            value
        };
        let raw = serde_json::json!({
            "schemaVersion": WORKSHOP_SCHEMA_VERSION,
            "requestKind": "refinement",
            "question": "Which voice qualities should carry forward?",
            "questionReason": "The author is comparing style treatments.",
            "dimension": "Voice treatment",
            "interpretation": {"youSaid":"A sample and optional style explanation", "possibleDirection":"Review style guidance", "stillOpen":"The author decides what to keep"},
            "candidates": [style_candidate("restrained"), style_candidate("brisk"), style_candidate("lyrical")]
        })
        .to_string();
        let output = validate_workshop_output(&raw, &frozen, "run-voice").unwrap();
        assert_eq!(output.candidates.len(), 3);
        assert!(
            output
                .candidates
                .iter()
                .all(|candidate| candidate.content.contains("STYLE guidance"))
        );

        let mut copied = serde_json::from_str::<Value>(&raw).unwrap();
        copied["candidates"][0]["content"] = Value::String(sample.into());
        assert!(validate_workshop_output(&copied.to_string(), &frozen, "run-voice-copy").is_err());
    }

    #[test]
    fn scoped_output_checks_fixed_literals_only_inside_editable_scope() {
        let mut metadata = metadata("directions");
        metadata.exploration.selected_scope = "Selected passage in working version".into();
        metadata.exploration.selected_text = "A fixed inside fact".into();
        metadata.exploration.working_selection = Some(WorkshopWorkingSelection {
            from: 4,
            to: 22,
            text: "A fixed inside fact".into(),
        });
        metadata.fixed_details = vec!["fixed outside fact".into(), "fixed inside fact".into()];
        metadata.selected_details.clear();

        let mut candidates = vec![candidate("one"), candidate("two"), candidate("three")];
        for value in &mut candidates {
            value["content"] =
                Value::String("The replacement changes while fixed inside fact remains.".into());
        }
        let good = serde_json::json!({
            "schemaVersion": WORKSHOP_SCHEMA_VERSION,
            "requestKind": "directions",
            "question": "What changes next?",
            "questionReason": "Compare scoped mechanisms.",
            "dimension": "Mechanism",
            "interpretation": {"youSaid":"", "possibleDirection":"", "stillOpen":""},
            "candidates": candidates
        });
        assert!(validate_workshop_output(&good.to_string(), &metadata, "run-scoped").is_ok());

        let mut bad = good;
        for value in bad["candidates"].as_array_mut().unwrap() {
            value["content"] = Value::String("The replacement changes entirely.".into());
        }
        assert!(validate_workshop_output(&bad.to_string(), &metadata, "run-scoped").is_err());
    }

    #[test]
    fn renderer_scope_is_descriptive_and_core_keeps_actor_state() {
        let session = WorkshopSession {
            id: "session-1".into(),
            title: "Explore".into(),
            lens: Lens::Overview,
            parent_session_id: None,
            branch_kind: WorkshopBranchKind::Working,
            brief: String::new(),
            direction: String::new(),
            still_open: String::new(),
            focus_question: String::new(),
            focus_reason: String::new(),
            focus_document_id: None,
            anchor_document_id: Some("workshop-session-1".into()),
            depth: WorkshopDepth::Sketch,
            outside_direction: false,
            included_document_ids: Vec::new(),
            working_text: String::new(),
            working_title: String::new(),
            working_generation: "generation-1".into(),
            selected_details: Vec::new(),
            choices: Vec::new(),
            questions: vec![WorkshopQuestion {
                id: "question-1".into(),
                text: "What should remain unknown?".into(),
                reason: "The author deferred this question.".into(),
                status: super::super::workshop::WorkshopQuestionStatus::NotNow,
                unknown_to: super::super::workshop::UnknownTo::Author,
            }],
            composer: "Help me find a direction without assuming a genre or cast.".into(),
            selected_scope: "Whole working version".into(),
            original_notes: "An intentionally preserved note.".into(),
            active_run_id: None,
            relationship_id: None,
        };
        let mut state = WorkshopState::default();
        state.sessions.push(session.clone());
        let mut request = start_request("directions");
        request.exploration.selected_scope = "Selected passage".into();
        let generated = from_session(request, &session, &state, context().expected).unwrap();
        assert_eq!(generated.context.expected.version, "7");
        assert_eq!(
            generated.context.current_element,
            "Help me find a direction without assuming a genre or cast."
        );
        assert_eq!(generated.exploration.selected_scope, "Selected passage");
        assert_eq!(
            generated.context.questions[0].status,
            super::super::workshop::WorkshopQuestionStatus::NotNow
        );
        assert_eq!(
            generated.context.questions[0].unknown_to,
            super::super::workshop::UnknownTo::Author
        );
        assert_eq!(
            generated.context.original_notes,
            "An intentionally preserved note."
        );
    }

    #[test]
    fn fixed_decisions_are_scoped_but_protection_survives_status_changes() {
        let mut session = WorkshopSession {
            id: "session-1".into(),
            title: "Current exploration".into(),
            lens: Lens::Overview,
            parent_session_id: None,
            branch_kind: WorkshopBranchKind::Working,
            brief: "A current element".into(),
            direction: String::new(),
            still_open: String::new(),
            focus_question: String::new(),
            focus_reason: String::new(),
            focus_document_id: None,
            anchor_document_id: Some("workshop-session-1".into()),
            depth: WorkshopDepth::Sketch,
            outside_direction: false,
            included_document_ids: Vec::new(),
            working_text: String::new(),
            working_title: String::new(),
            working_generation: "generation-1".into(),
            selected_details: Vec::new(),
            choices: Vec::new(),
            questions: Vec::new(),
            composer: String::new(),
            selected_scope: "Whole working version".into(),
            original_notes: String::new(),
            active_run_id: None,
            relationship_id: None,
        };
        let mut state = WorkshopState::default();
        state.sessions.push(session.clone());
        state
            .decisions
            .push(super::super::workshop::WorkshopDecision {
                id: "decision-1".into(),
                session_id: "older-session".into(),
                title: "A project rule".into(),
                document_id: "world-1".into(),
                revision_id: "revision-1".into(),
                head: context().expected,
                candidate_ids: Vec::new(),
                rationale: "Keep this rule visible across explorations.".into(),
                status: super::super::workshop::WorkshopDecisionStatus::Archived,
                fixed: true,
                protected_text: vec!["The older session's protected rule".into()],
                access: "authorRoom".into(),
                supersedes_id: None,
            });
        let generated = from_session(
            start_request("directions"),
            &session,
            &state,
            context().expected,
        )
        .unwrap();
        assert!(
            !generated
                .context
                .fixed_details
                .contains(&"The older session's protected rule".to_owned())
        );
        assert!(
            !generated
                .context
                .fixed_source_refs
                .contains(&"world-1@7".to_owned())
        );

        session.included_document_ids.push("world-1".into());
        let generated = from_session(
            start_request("directions"),
            &session,
            &state,
            context().expected,
        )
        .unwrap();
        assert!(
            generated
                .context
                .fixed_details
                .contains(&"The older session's protected rule".to_owned())
        );
        assert!(
            generated
                .context
                .fixed_source_refs
                .contains(&"world-1@7".to_owned())
        );
        assert!(generated.context.chosen_details.is_empty());
    }

    #[test]
    fn working_selection_matches_saved_utf16_text() {
        let mut request = exploration("refine");
        request.selected_text = "Tea 🐉".into();
        request.working_selection = Some(WorkshopWorkingSelection {
            from: 2,
            to: 8,
            text: "Tea 🐉".into(),
        });
        assert!(validate_working_selection(&request, "A Tea 🐉 now").is_ok());
        request.working_selection.as_mut().unwrap().text = "Other".into();
        assert!(validate_working_selection(&request, "A Tea 🐉 now").is_err());
    }
}
