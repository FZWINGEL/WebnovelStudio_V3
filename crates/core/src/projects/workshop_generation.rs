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
    WorkshopSession, WorkshopState,
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
    validate_working_selection(&exploration, &session.working_text)?;
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
        .filter(|preference| preference.confirmed && preference_applies(preference, session))
        .map(format_preference)
        .collect::<Vec<_>>();
    let hard_constraints = state
        .preferences
        .iter()
        .filter(|preference| {
            preference.confirmed
                && preference.strength == super::workshop::PreferenceStrength::Hard
                && preference.polarity != super::workshop::PreferencePolarity::Neutral
                && preference_applies(preference, session)
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
    // A Keep fixed decision is a project-level protection. It can have been
    // recorded from another exploration session and still constrain this
    // request; status/access are checked here, while the actor has already
    // resolved the exact source revision and protected literals.
    for decision in state.decisions.iter().filter(|decision| {
        decision.fixed && decision.status == super::workshop::WorkshopDecisionStatus::Chosen
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
        included_document_ids: session.included_document_ids.clone(),
        included_alternatives: resolved_material.included_alternatives,
        rejected_rationales,
        questions: session.questions.clone(),
        original_notes: session.original_notes.clone(),
        outside_direction: session.outside_direction,
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
    let expected = if is_direction_action(&metadata.exploration.action) {
        3..=3
    } else {
        1..=MAX_CANDIDATES
    };
    if !expected.contains(&output.candidates.len()) {
        return Err(invalid(
            if is_direction_action(&metadata.exploration.action) {
                "A direction workshop response must contain exactly three candidates."
            } else {
                "A workshop refinement response must contain one to three candidates."
            },
        ));
    }
    if output.candidates.len() > MAX_CANDIDATES {
        return Err(invalid(
            "The workshop response contains too many candidates.",
        ));
    }
    let mut dimensions = BTreeSet::new();
    for (index, candidate) in output.candidates.iter_mut().enumerate() {
        validate_candidate(candidate, metadata, run_id, index)?;
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
    let editable_scope = editable_scope_text(metadata);
    let whole_or_synthesis = is_whole_or_synthesis_scope(&metadata.exploration.selected_scope)
        && metadata.exploration.working_selection.is_none();
    let mut required = metadata
        .fixed_details
        .iter()
        .filter(|literal| editable_scope.contains(literal.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    // A whole-work synthesis can include selected tray material even when the
    // current element does not repeat it verbatim. Scoped replacements must
    // leave fixed facts outside their captured range to the surrounding
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

fn preference_applies(preference: &WorkshopPreference, session: &WorkshopSession) -> bool {
    matches!(preference.scope, super::workshop::PreferenceScope::Project)
        || (preference.scope == super::workshop::PreferenceScope::Element
            && preference.target_id.as_deref() == session.focus_document_id.as_deref())
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
    fn fixed_project_decisions_from_another_session_stay_protected() {
        let session = WorkshopSession {
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
                status: super::super::workshop::WorkshopDecisionStatus::Chosen,
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
