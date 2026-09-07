//! Durable Story Workshop state and reviewed adoption boundary.
//!
//! The workshop is deliberately a small author-room projection.  Documents
//! and their immutable revisions remain the only story authority.  Workshop
//! rows retain exploration state, exact previews, and receipts so a lost IPC
//! acknowledgment can be reconciled without replaying a mutation.

use super::*;
use crate::projects::discussions::DiscussionRun;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const MAX_SESSIONS: usize = 256;
const MAX_PREFERENCES: usize = 512;
const MAX_DECISIONS: usize = 512;
const MAX_RELATIONSHIPS: usize = 512;
const MAX_IMPACTS: usize = 1024;
const MAX_PRESETS: usize = 128;
const MAX_LIST: usize = 512;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_DETAIL_BYTES: usize = 32 * 1024;
type WorkshopCandidateRecord = (String, String, Option<WorkshopRelationship>);
type WorkshopCandidateOutput = (String, WorkshopCandidate, Option<WorkshopRelationship>);

fn storage_valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lens {
    Overview,
    World,
    People,
    Themes,
    Possibilities,
    Notebook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkshopBranchKind {
    Working,
    WhatIf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkshopDepth {
    Sketch,
    Develop,
    Document,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreferencePolarity {
    Neutral,
    Want,
    Avoid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreferenceStrength {
    Soft,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreferenceScope {
    Project,
    Element,
    Exploration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CandidateChoiceStatus {
    Saved,
    Rejected,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkshopQuestionStatus {
    Open,
    NotNow,
    NotRelevant,
    KeepMysterious,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnknownTo {
    Author,
    Reader,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkshopDecisionStatus {
    Chosen,
    Archived,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkshopRelationshipStatus {
    Tentative,
    Chosen,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkshopImpactKind {
    Contradiction,
    PossibleTension,
    DependentAssumption,
    StyleSuggestion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkshopImpactStatus {
    NeedsReview,
    Acknowledged,
    Intentional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopPreference {
    pub id: String,
    pub label: String,
    pub family: String,
    pub meaning: String,
    pub examples: String,
    pub timing: String,
    pub polarity: PreferencePolarity,
    pub strength: PreferenceStrength,
    pub scope: PreferenceScope,
    pub target_id: Option<String>,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedDetail {
    pub id: String,
    pub candidate_id: Option<String>,
    pub text: String,
    pub fixed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateChoice {
    pub candidate_id: String,
    pub status: CandidateChoiceStatus,
    pub rationale: String,
    pub include_in_context: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopQuestion {
    pub id: String,
    pub text: String,
    pub reason: String,
    pub status: WorkshopQuestionStatus,
    pub unknown_to: UnknownTo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopSession {
    pub id: String,
    pub title: String,
    pub lens: Lens,
    pub parent_session_id: Option<String>,
    pub branch_kind: WorkshopBranchKind,
    pub brief: String,
    pub direction: String,
    pub still_open: String,
    pub focus_question: String,
    pub focus_reason: String,
    pub focus_document_id: Option<String>,
    pub anchor_document_id: Option<String>,
    pub depth: WorkshopDepth,
    pub outside_direction: bool,
    pub included_document_ids: Vec<String>,
    pub working_text: String,
    pub working_title: String,
    pub working_generation: String,
    pub selected_details: Vec<SelectedDetail>,
    pub choices: Vec<CandidateChoice>,
    pub questions: Vec<WorkshopQuestion>,
    pub composer: String,
    pub selected_scope: String,
    pub original_notes: String,
    pub active_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopDecision {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub document_id: String,
    pub revision_id: String,
    pub head: Head,
    pub candidate_ids: Vec<String>,
    pub rationale: String,
    pub status: WorkshopDecisionStatus,
    pub fixed: bool,
    pub protected_text: Vec<String>,
    pub access: String,
    pub supersedes_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopRelationship {
    pub id: String,
    pub from_document_id: String,
    pub to_document_id: String,
    #[serde(rename = "type")]
    pub relationship_type: String,
    pub description: String,
    pub uncertainty: String,
    pub status: WorkshopRelationshipStatus,
    pub source_heads: Vec<Head>,
}

/// A relationship proposed as part of an atomic adoption.  This remains a
/// request shape until the adoption commits the exact endpoint heads into a
/// `WorkshopRelationship` record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopRelationshipDraft {
    pub id: String,
    pub from_document_id: String,
    pub to_document_id: String,
    #[serde(rename = "type")]
    pub relationship_type: String,
    pub description: String,
    pub uncertainty: String,
    pub from_expected: Option<Head>,
    pub to_expected: Option<Head>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopImpact {
    pub id: String,
    pub decision_id: String,
    pub document_id: String,
    pub kind: WorkshopImpactKind,
    pub reason: String,
    pub status: WorkshopImpactStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<String>,
}

/// An author classification supplied with an adoption preview.  It is tied
/// to a candidate affected target and becomes an immutable impact provenance
/// record when the adoption commits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopImpactDraft {
    pub document_id: String,
    pub kind: WorkshopImpactKind,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopAdoptionImpact {
    pub candidate_id: String,
    pub document_id: String,
    pub kind: WorkshopImpactKind,
    pub reason: String,
    pub status: WorkshopImpactStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopPreset {
    pub id: String,
    pub name: String,
    pub preferences: Vec<WorkshopPreference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopState {
    pub schema_version: u32,
    pub current_session_id: Option<String>,
    pub sessions: Vec<WorkshopSession>,
    pub preferences: Vec<WorkshopPreference>,
    pub decisions: Vec<WorkshopDecision>,
    pub relationships: Vec<WorkshopRelationship>,
    pub impacts: Vec<WorkshopImpact>,
    pub presets: Vec<WorkshopPreset>,
}

impl Default for WorkshopState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            current_session_id: None,
            sessions: Vec::new(),
            preferences: Vec::new(),
            decisions: Vec::new(),
            relationships: Vec::new(),
            impacts: Vec::new(),
            presets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopSnapshot {
    pub version: String,
    pub state: WorkshopState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopCandidateImplication {
    pub text: String,
    pub basis: String,
    pub assumption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopCandidateAffectedTarget {
    pub document_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopCandidate {
    pub id: String,
    pub title: String,
    pub content: String,
    pub dimension_value: String,
    pub implications: Vec<WorkshopCandidateImplication>,
    pub assumptions: Vec<String>,
    pub affected_targets: Vec<WorkshopCandidateAffectedTarget>,
    pub preserved_details: Vec<String>,
    pub changed_details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopOutputInterpretation {
    pub you_said: String,
    pub possible_direction: String,
    pub still_open: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopOutput {
    pub schema_version: String,
    pub request_kind: String,
    pub question: String,
    pub question_reason: String,
    pub dimension: String,
    pub interpretation: WorkshopOutputInterpretation,
    pub candidates: Vec<WorkshopCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopResult {
    pub run: DiscussionRun,
    pub session_id: String,
    pub working_generation: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_selection: Option<crate::projects::workshop_generation::WorkshopWorkingSelection>,
    pub output: Option<WorkshopOutput>,
    pub validation_error: Option<String>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopView {
    pub version: String,
    pub state: WorkshopState,
    pub results: Vec<WorkshopResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveWorkshop {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected_version: String,
    pub state: WorkshopState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopAdoptionTarget {
    pub document_id: String,
    pub expected: Option<Head>,
    pub title: String,
    pub kind: String,
    pub body: Value,
    pub mode: AdoptionMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AdoptionMode {
    Add,
    Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewWorkshopAdoption {
    pub access: ProjectAccess,
    pub session_id: String,
    pub expected_version: String,
    pub candidate_ids: Vec<String>,
    pub targets: Vec<WorkshopAdoptionTarget>,
    pub rationale: String,
    pub protected_text: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<WorkshopRelationshipDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub impact_drafts: Vec<WorkshopImpactDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopAdoptionPreview {
    pub id: String,
    pub session_id: String,
    pub expected_version: String,
    pub targets: Vec<WorkshopAdoptionTarget>,
    pub before: Vec<DocumentRecord>,
    pub rationale: String,
    pub protected_text: Vec<String>,
    pub candidate_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<WorkshopRelationship>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoint_sources: Vec<DocumentRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub impacts: Vec<WorkshopAdoptionImpact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopAdoptionAck {
    pub snapshot: WorkshopSnapshot,
    pub documents: Vec<DocumentRecord>,
    pub decision_ids: Vec<String>,
}

fn workshop_snapshot(version: i64, state: WorkshopState) -> CoreResult<WorkshopSnapshot> {
    Ok(WorkshopSnapshot {
        version: parse_stored_version(version)?,
        state,
    })
}

fn state_json(state: &WorkshopState) -> CoreResult<(String, String)> {
    let value = crate::canonicalize_value(serde_json::to_value(state)?);
    let json = serde_json::to_string(&value)?;
    Ok((json.clone(), sha256_hex(json.as_bytes())))
}

fn parse_state(json: &str, hash: &str) -> CoreResult<WorkshopState> {
    let state: WorkshopState = serde_json::from_str(json).map_err(|error| {
        CoreError::new(
            "InvalidProject",
            &format!("The workshop state is invalid: {error}"),
        )
    })?;
    let (canonical, actual_hash) = state_json(&state)?;
    if canonical != json || actual_hash != hash {
        return Err(CoreError::new(
            "InvalidProject",
            "The workshop state failed its fingerprint check.",
        ));
    }
    validate_state_shape(&state)?;
    Ok(state)
}

fn read_state(connection: &Connection) -> CoreResult<(i64, WorkshopState)> {
    let row: Option<(i64, String, String)> = connection
        .query_row(
            "SELECT version,state_json,state_hash FROM workshop_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    match row {
        Some((version, state, hash)) if version >= 0 => Ok((version, parse_state(&state, &hash)?)),
        Some(_) => Err(CoreError::new(
            "InvalidProject",
            "The workshop state has a negative version.",
        )),
        None => Ok((0, WorkshopState::default())),
    }
}

fn validate_text(value: &str, label: &str, max: usize) -> CoreResult<()> {
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

fn validate_id_list(values: &[String], label: &str) -> CoreResult<()> {
    if values.len() > MAX_LIST {
        return Err(CoreError::new(
            "InvalidRequest",
            &format!("{label} contains too many entries."),
        ));
    }
    let mut seen = HashSet::new();
    for value in values {
        check_id(value)?;
        if !seen.insert(value) {
            return Err(CoreError::new(
                "InvalidRequest",
                &format!("{label} contains a duplicate identifier."),
            ));
        }
    }
    Ok(())
}

fn validate_kind(kind: &str) -> CoreResult<()> {
    if !["note", "character", "world", "theme", "hook", "scene"].contains(&kind) {
        return Err(CoreError::new(
            "InvalidDocument",
            "Workshop adoption can target only nonchapter documents.",
        ));
    }
    Ok(())
}

fn existing_document_ids(connection: &Connection) -> CoreResult<HashSet<String>> {
    let mut statement = connection.prepare("SELECT id FROM documents WHERE trashed=0")?;
    Ok(statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .collect())
}

fn validate_preferences(preferences: &[WorkshopPreference], label: &str) -> CoreResult<()> {
    if preferences.len() > MAX_PREFERENCES {
        return Err(CoreError::new(
            "InvalidRequest",
            "Too many workshop preferences.",
        ));
    }
    let mut ids = HashSet::new();
    for preference in preferences {
        check_id(&preference.id)?;
        if !ids.insert(&preference.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop preference IDs must be unique.",
            ));
        }
        if preference.label.trim().is_empty() {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop preference labels must not be blank.",
            ));
        }
        for (value, name) in [
            (&preference.label, "preference label"),
            (&preference.family, "preference family"),
            (&preference.meaning, "preference meaning"),
            (&preference.examples, "preference examples"),
            (&preference.timing, "preference timing"),
        ] {
            validate_text(value, name, MAX_TEXT_BYTES)?;
        }
        if preference.scope == PreferenceScope::Project && preference.target_id.is_some() {
            return Err(CoreError::new(
                "InvalidRequest",
                "Project preferences cannot have a target.",
            ));
        }
        if preference.scope != PreferenceScope::Project
            && let Some(target) = &preference.target_id
        {
            check_id(target)?;
        }
    }
    let _ = label;
    Ok(())
}

/// Validate a preset at a file or IPC boundary without applying it to a
/// project. Presets contain project-level preference vocabulary only; the
/// caller still needs to merge the returned value through saveWorkshop.
pub fn validate_workshop_preset(preset: &WorkshopPreset) -> CoreResult<()> {
    check_id(&preset.id)?;
    validate_text(&preset.name, "preset name", MAX_DETAIL_BYTES)?;
    if preset.name.trim().is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop preset name must not be blank.",
        ));
    }
    validate_preferences(&preset.preferences, "preset preferences")?;
    if preset
        .preferences
        .iter()
        .any(|preference| preference.scope != PreferenceScope::Project)
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop preset may contain only project preferences.",
        ));
    }
    Ok(())
}

fn validate_hard_preference_conflicts(state: &WorkshopState) -> CoreResult<()> {
    for project in state.preferences.iter().filter(|preference| {
        preference.scope == PreferenceScope::Project
            && preference.strength == PreferenceStrength::Hard
            && preference.confirmed
    }) {
        for other in state
            .preferences
            .iter()
            .filter(|preference| preference.confirmed && preference.id != project.id)
        {
            if project.label.trim().to_lowercase() == other.label.trim().to_lowercase()
                && project.polarity != PreferencePolarity::Neutral
                && other.polarity != PreferencePolarity::Neutral
                && project.polarity != other.polarity
            {
                return Err(CoreError::new(
                    "PreferenceConflict",
                    "A workshop preference conflicts with a hard project preference.",
                ));
            }
        }
    }
    Ok(())
}

fn validate_session_branch_graph(sessions: &[WorkshopSession]) -> CoreResult<()> {
    let by_id: HashMap<&str, &WorkshopSession> = sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect();

    for session in sessions {
        match (session.branch_kind, session.parent_session_id.as_deref()) {
            (WorkshopBranchKind::Working, None) => {}
            (WorkshopBranchKind::Working, Some(_)) => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A working workshop session cannot have a parent.",
                ));
            }
            (WorkshopBranchKind::WhatIf, None) => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A what-if workshop session must have a parent.",
                ));
            }
            (WorkshopBranchKind::WhatIf, Some(parent)) if parent == session.id => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A what-if workshop session cannot parent itself.",
                ));
            }
            (WorkshopBranchKind::WhatIf, Some(parent)) if !by_id.contains_key(parent) => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A what-if workshop session has an unknown parent.",
                ));
            }
            (WorkshopBranchKind::WhatIf, Some(_)) => {}
        }
    }

    for session in sessions {
        let mut seen = HashSet::new();
        let mut current = Some(session.id.as_str());
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Workshop session parents cannot contain a cycle.",
                ));
            }
            current = by_id
                .get(id)
                .and_then(|parent| parent.parent_session_id.as_deref());
        }
    }
    Ok(())
}

fn validate_state_shape(state: &WorkshopState) -> CoreResult<()> {
    if state.schema_version != 1 {
        return Err(CoreError::new(
            "UnsupportedSchema",
            "Unsupported workshop state schema.",
        ));
    }
    if state.sessions.len() > MAX_SESSIONS
        || state.decisions.len() > MAX_DECISIONS
        || state.relationships.len() > MAX_RELATIONSHIPS
        || state.impacts.len() > MAX_IMPACTS
        || state.presets.len() > MAX_PRESETS
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "Workshop state exceeds its bounded size.",
        ));
    }
    validate_session_branch_graph(&state.sessions)?;
    let mut sessions = HashSet::new();
    for session in &state.sessions {
        check_id(&session.id)?;
        if !sessions.insert(&session.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop session IDs must be unique.",
            ));
        }
        for (value, name, max) in [
            (&session.title, "session title", MAX_DETAIL_BYTES),
            (&session.brief, "session brief", MAX_TEXT_BYTES),
            (&session.direction, "session direction", MAX_TEXT_BYTES),
            (&session.still_open, "session stillOpen", MAX_TEXT_BYTES),
            (
                &session.focus_question,
                "session focusQuestion",
                MAX_TEXT_BYTES,
            ),
            (&session.focus_reason, "session focusReason", MAX_TEXT_BYTES),
            (&session.working_text, "session workingText", MAX_TEXT_BYTES),
            (
                &session.working_title,
                "session workingTitle",
                MAX_DETAIL_BYTES,
            ),
            (
                &session.working_generation,
                "session workingGeneration",
                128,
            ),
            (&session.composer, "session composer", MAX_TEXT_BYTES),
            (
                &session.selected_scope,
                "session selectedScope",
                MAX_DETAIL_BYTES,
            ),
            (
                &session.original_notes,
                "session originalNotes",
                MAX_TEXT_BYTES,
            ),
        ] {
            validate_text(value, name, max)?;
        }
        if let Some(parent) = &session.parent_session_id {
            check_id(parent)?;
        }
        for id in [
            session.focus_document_id.as_ref(),
            session.anchor_document_id.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            check_id(id)?;
        }
        validate_id_list(&session.included_document_ids, "session included documents")?;
        if session.selected_details.len() > MAX_LIST
            || session.choices.len() > MAX_LIST
            || session.questions.len() > MAX_LIST
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop session contains too many entries.",
            ));
        }
        let mut details = HashSet::new();
        for detail in &session.selected_details {
            check_id(&detail.id)?;
            if !details.insert(&detail.id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Selected detail IDs must be unique.",
                ));
            }
            validate_text(&detail.text, "selected detail", MAX_DETAIL_BYTES)?;
            if let Some(candidate) = &detail.candidate_id {
                check_id(candidate)?;
            }
        }
        let mut choices = HashSet::new();
        for choice in &session.choices {
            check_id(&choice.candidate_id)?;
            if !choices.insert(&choice.candidate_id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Candidate choice IDs must be unique.",
                ));
            }
            validate_text(&choice.rationale, "candidate rationale", MAX_DETAIL_BYTES)?;
        }
        let mut questions = HashSet::new();
        for question in &session.questions {
            check_id(&question.id)?;
            if !questions.insert(&question.id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Workshop question IDs must be unique.",
                ));
            }
            validate_text(&question.text, "workshop question", MAX_DETAIL_BYTES)?;
            validate_text(
                &question.reason,
                "workshop question reason",
                MAX_DETAIL_BYTES,
            )?;
        }
        if let Some(run) = &session.active_run_id {
            check_id(run)?;
        }
    }
    if let Some(current) = &state.current_session_id {
        check_id(current)?;
        if !sessions.contains(current) {
            return Err(CoreError::new(
                "InvalidRequest",
                "The current workshop session does not exist.",
            ));
        }
    }
    validate_preferences(&state.preferences, "preferences")?;
    validate_hard_preference_conflicts(state)?;
    validate_preferences(
        &state
            .presets
            .iter()
            .flat_map(|preset| preset.preferences.clone())
            .collect::<Vec<_>>(),
        "preset preferences",
    )?;
    let mut decision_ids = HashSet::new();
    let mut chosen_documents = HashSet::new();
    for decision in &state.decisions {
        check_id(&decision.id)?;
        if !decision_ids.insert(&decision.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop decision IDs must be unique.",
            ));
        }
        if decision.status == WorkshopDecisionStatus::Chosen
            && !chosen_documents.insert(decision.document_id.as_str())
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A document cannot have more than one chosen workshop decision.",
            ));
        }
        check_id(&decision.session_id)?;
        check_id(&decision.document_id)?;
        check_id(&decision.revision_id)?;
        check_id(&decision.head.document_id)?;
        if decision.head.document_id != decision.document_id {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop decision head targets another document.",
            ));
        }
        if decision.candidate_ids.len() > MAX_LIST {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop decision has too many candidates.",
            ));
        }
        validate_id_list(&decision.candidate_ids, "decision candidates")?;
        validate_text(&decision.title, "decision title", MAX_DETAIL_BYTES)?;
        validate_text(&decision.rationale, "decision rationale", MAX_TEXT_BYTES)?;
        if decision.access != "authorRoom" {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop decisions must use authorRoom access.",
            ));
        }
        for text in &decision.protected_text {
            validate_text(text, "protected decision text", MAX_DETAIL_BYTES)?;
        }
        if let Some(id) = &decision.supersedes_id {
            check_id(id)?;
        }
    }
    let mut relationship_ids = HashSet::new();
    for relationship in &state.relationships {
        check_id(&relationship.id)?;
        if !relationship_ids.insert(&relationship.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop relationship IDs must be unique.",
            ));
        }
        check_id(&relationship.from_document_id)?;
        check_id(&relationship.to_document_id)?;
        for (value, name) in [
            (&relationship.relationship_type, "relationship type"),
            (&relationship.description, "relationship description"),
            (&relationship.uncertainty, "relationship uncertainty"),
        ] {
            validate_text(value, name, MAX_DETAIL_BYTES)?;
        }
        if relationship.source_heads.len() != 2 {
            return Err(CoreError::new(
                "InvalidRequest",
                "A relationship must retain both source heads.",
            ));
        }
        for head in &relationship.source_heads {
            check_id(&head.document_id)?;
            parse_version(&head.version)?;
            if !valid_hash(&head.body_hash) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship has an invalid source hash.",
                ));
            }
        }
    }
    let mut impact_ids = HashSet::new();
    for impact in &state.impacts {
        check_id(&impact.id)?;
        if !impact_ids.insert(&impact.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop impact IDs must be unique.",
            ));
        }
        check_id(&impact.decision_id)?;
        check_id(&impact.document_id)?;
        validate_text(&impact.reason, "impact reason", MAX_DETAIL_BYTES)?;
        if let Some(candidate_id) = &impact.candidate_id {
            check_id(candidate_id)?;
        }
        if let Some(relationship_id) = &impact.relationship_id {
            check_id(relationship_id)?;
        }
        if impact.candidate_id.is_some() && impact.relationship_id.is_some() {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop impact cannot mix candidate and relationship provenance.",
            ));
        }
    }
    for preset in &state.presets {
        check_id(&preset.id)?;
        validate_text(&preset.name, "preset name", MAX_DETAIL_BYTES)?;
    }
    Ok(())
}

fn validate_state_references(
    connection: &Connection,
    state: &WorkshopState,
    current: Option<&WorkshopState>,
    extra_document_ids: &HashSet<String>,
) -> CoreResult<()> {
    validate_state_shape(state)?;
    let document_ids = existing_document_ids(connection)?;
    let session_ids: HashSet<&str> = state
        .sessions
        .iter()
        .map(|session| session.id.as_str())
        .collect();
    for session in &state.sessions {
        for id in session
            .focus_document_id
            .as_ref()
            .into_iter()
            .chain(session.included_document_ids.iter())
        {
            if !document_ids.contains(id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A workshop session references an unknown document.",
                ));
            }
        }
        // A new manual exploration starts with a local anchor placeholder.
        // It becomes a real document only when the author explicitly adopts
        // it; every other anchor must already belong to this project.
        if let Some(anchor) = &session.anchor_document_id
            && !document_ids.contains(anchor)
            && !anchor.starts_with("workshop-")
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop session references an unknown anchor document.",
            ));
        }
    }
    for preference in &state.preferences {
        match preference.scope {
            PreferenceScope::Project => {
                if preference.target_id.is_some() {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "A project preference cannot target a local record.",
                    ));
                }
            }
            PreferenceScope::Element => {
                let Some(target) = preference.target_id.as_ref() else {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An element preference needs a document target.",
                    ));
                };
                if !document_ids.contains(target) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An element preference references an unknown document.",
                    ));
                }
            }
            PreferenceScope::Exploration => {
                let Some(target) = preference.target_id.as_ref() else {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An exploration preference needs a session target.",
                    ));
                };
                if !session_ids.contains(target.as_str()) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An exploration preference references an unknown session.",
                    ));
                }
            }
        }
    }
    for preset in &state.presets {
        validate_workshop_preset(preset)?;
    }
    let decision_map: HashMap<&str, &WorkshopDecision> = state
        .decisions
        .iter()
        .map(|decision| (decision.id.as_str(), decision))
        .collect();
    for decision in &state.decisions {
        if !session_ids.contains(decision.session_id.as_str())
            || !document_ids.contains(&decision.document_id)
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop decision references an unknown target.",
            ));
        }
        let revision = read_revision(connection, &decision.revision_id)?;
        if revision.head != decision.head || revision.head.document_id != decision.document_id {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop decision provenance is invalid.",
            ));
        }
    }
    for impact in &state.impacts {
        let Some(decision) = decision_map.get(impact.decision_id.as_str()) else {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop impact references an unknown decision.",
            ));
        };
        if !document_ids.contains(&impact.document_id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop impact references an unknown document.",
            ));
        }
        if let Some(candidate_id) = &impact.candidate_id
            && !decision.candidate_ids.contains(candidate_id)
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop impact references a candidate outside its decision.",
            ));
        }
        if let Some(relationship_id) = &impact.relationship_id {
            let Some(relationship) = state
                .relationships
                .iter()
                .find(|relationship| relationship.id == *relationship_id)
            else {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A workshop impact references an unknown relationship.",
                ));
            };
            if relationship.from_document_id != impact.document_id
                && relationship.to_document_id != impact.document_id
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship impact targets a document outside the relationship.",
                ));
            }
            if decision.document_id != impact.document_id {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship impact is bound to the wrong adoption decision.",
                ));
            }
        }
    }
    for relationship in &state.relationships {
        if (!document_ids.contains(&relationship.from_document_id)
            && !extra_document_ids.contains(&relationship.from_document_id))
            || (!document_ids.contains(&relationship.to_document_id)
                && !extra_document_ids.contains(&relationship.to_document_id))
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop relationship has a dangling endpoint.",
            ));
        }
        let expected = [
            relationship.from_document_id.as_str(),
            relationship.to_document_id.as_str(),
        ];
        for (head, document_id) in relationship.source_heads.iter().zip(expected) {
            if head.document_id != document_id {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship source head targets the wrong endpoint.",
                ));
            }
        }
    }
    if let Some(previous) = current {
        let previous_sessions: HashMap<&str, &WorkshopSession> = previous
            .sessions
            .iter()
            .map(|session| (session.id.as_str(), session))
            .collect();
        for session in &state.sessions {
            let Some(old) = previous_sessions.get(session.id.as_str()) else {
                continue;
            };
            if old.parent_session_id != session.parent_session_id
                || old.branch_kind != session.branch_kind
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A saved workshop session's branch identity is immutable; create a new fork instead.",
                ));
            }
        }
        let previous_decisions: HashMap<&str, &WorkshopDecision> = previous
            .decisions
            .iter()
            .map(|decision| (decision.id.as_str(), decision))
            .collect();
        for decision in &state.decisions {
            if !previous_decisions.contains_key(decision.id.as_str()) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "New workshop decisions can be created only by adoption.",
                ));
            }
            let old = previous_decisions[decision.id.as_str()];
            if old.session_id != decision.session_id
                || old.title != decision.title
                || old.document_id != decision.document_id
                || old.revision_id != decision.revision_id
                || old.head != decision.head
                || old.candidate_ids != decision.candidate_ids
                || old.access != decision.access
                || old.supersedes_id != decision.supersedes_id
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Workshop decision provenance is immutable; edit rationale, protection, or status instead.",
                ));
            }
        }
        if state.decisions.len() != previous.decisions.len() {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop decisions cannot be removed or added by save.",
            ));
        }
        let previous_impacts: HashMap<&str, &WorkshopImpact> = previous
            .impacts
            .iter()
            .map(|impact| (impact.id.as_str(), impact))
            .collect();
        for impact in &state.impacts {
            let Some(old) = previous_impacts.get(impact.id.as_str()) else {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "New workshop impacts can be created only by adoption.",
                ));
            };
            if old.decision_id != impact.decision_id
                || old.document_id != impact.document_id
                || old.kind != impact.kind
                || old.reason != impact.reason
                || old.candidate_id != impact.candidate_id
                || old.relationship_id != impact.relationship_id
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Workshop impact provenance is immutable; edit its review status instead.",
                ));
            }
        }
        if state.impacts.len() != previous.impacts.len() {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop impacts cannot be removed or added by save.",
            ));
        }
        let old_details: HashMap<&str, &SelectedDetail> = previous
            .sessions
            .iter()
            .flat_map(|session| session.selected_details.iter())
            .map(|detail| (detail.id.as_str(), detail))
            .collect();
        for detail in state
            .sessions
            .iter()
            .flat_map(|session| session.selected_details.iter())
        {
            if let Some(old) = old_details.get(detail.id.as_str())
                && old.fixed
                && detail.fixed
                && old.text != detail.text
            {
                return Err(CoreError::new(
                    "ProtectedContentChanged",
                    "A fixed selected detail must be explicitly unprotected before editing.",
                ));
            }
        }
    }
    Ok(())
}

fn current_context_epoch(connection: &Connection) -> CoreResult<String> {
    let epoch: i64 = connection.query_row(
        "SELECT context_source_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    parse_stored_version(epoch)
}

/// Return candidate IDs only from completed, integrity-checked workshop runs.
/// A candidate remains usable for a child what-if session through its parent
/// chain; its frozen source epoch must still be current at the mutation
/// boundary.
fn workshop_candidate_sessions(
    connection: &Connection,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: Option<&str>,
) -> CoreResult<HashMap<String, (String, Option<WorkshopRelationship>)>> {
    Ok(
        workshop_candidate_records(connection, project_id, operation_namespace, source_epoch)?
            .into_iter()
            .map(|(candidate_id, (session_id, _content, relationship))| {
                (candidate_id, (session_id, relationship))
            })
            .collect(),
    )
}

fn workshop_candidate_records(
    connection: &Connection,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: Option<&str>,
) -> CoreResult<HashMap<String, WorkshopCandidateRecord>> {
    workshop_candidate_records_with_filter(
        connection,
        Some(project_id),
        Some(operation_namespace),
        source_epoch,
    )
}

fn historical_workshop_candidate_records(
    connection: &Connection,
) -> CoreResult<HashMap<String, (String, String)>> {
    Ok(
        workshop_candidate_records_with_filter(connection, None, None, None)?
            .into_iter()
            .map(|(candidate_id, (session_id, content, _relationship))| {
                (candidate_id, (session_id, content))
            })
            .collect(),
    )
}

fn workshop_candidate_records_with_filter(
    connection: &Connection,
    project_id: Option<&str>,
    operation_namespace: Option<&str>,
    source_epoch: Option<&str>,
) -> CoreResult<HashMap<String, WorkshopCandidateRecord>> {
    Ok(workshop_candidate_outputs_with_filter(
        connection,
        project_id,
        operation_namespace,
        source_epoch,
    )?
    .into_iter()
    .map(|(candidate_id, (session_id, candidate, relationship))| {
        (candidate_id, (session_id, candidate.content, relationship))
    })
    .collect())
}

fn workshop_candidate_outputs_with_filter(
    connection: &Connection,
    project_id: Option<&str>,
    operation_namespace: Option<&str>,
    source_epoch: Option<&str>,
) -> CoreResult<HashMap<String, WorkshopCandidateOutput>> {
    let mut statement = connection.prepare(
        "SELECT id,packet_id,output_text FROM discussion_runs WHERE status='completed' AND dispatch_state='delivered' ORDER BY rowid",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut candidates = HashMap::new();
    for (run_id, packet_id, output_text) in rows {
        let packet =
            match crate::projects::context_packets::validated_packet_record(connection, &packet_id)
            {
                Ok(packet) => packet,
                Err(_) => continue,
            };
        let Some(instruction) = packet
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content.as_str())
        else {
            continue;
        };
        let metadata =
            match crate::projects::workshop_generation::metadata_from_instruction(instruction) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
        let (frozen, namespace) = match crate::projects::story_context::validated_snapshot_record(
            connection,
            &packet.receipt.snapshot_id,
        ) {
            Ok(snapshot) => snapshot,
            Err(_) => continue,
        };
        if project_id.is_some_and(|project| frozen.snapshot.project_id != project)
            || operation_namespace.is_some_and(|expected| namespace != expected)
            || source_epoch.is_some_and(|epoch| frozen.snapshot.context_source_epoch != epoch)
        {
            continue;
        }
        let output = match crate::projects::workshop_generation::validate_workshop_output(
            &output_text,
            &metadata,
            &run_id,
        ) {
            Ok(output) => output,
            Err(_) => continue,
        };
        for candidate in output.candidates {
            let candidate_id = candidate.id.clone();
            if candidates
                .insert(
                    candidate_id,
                    (
                        metadata.exploration.session_id.clone(),
                        candidate,
                        metadata.relationship.clone(),
                    ),
                )
                .is_some()
            {
                return Err(CoreError::new(
                    "InvalidProject",
                    "Two workshop results contain the same candidate identity.",
                ));
            }
        }
    }
    Ok(candidates)
}

fn build_adoption_impacts(
    connection: &Connection,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: &str,
    candidate_ids: &[String],
    targets: &[WorkshopAdoptionTarget],
    impact_drafts: &[WorkshopImpactDraft],
) -> CoreResult<Vec<WorkshopAdoptionImpact>> {
    if impact_drafts.len() > MAX_LIST {
        return Err(CoreError::new(
            "InvalidRequest",
            "The adoption contains too many impact classifications.",
        ));
    }
    let mut overrides = HashMap::new();
    for draft in impact_drafts {
        // Workshop anchors are blank author-room implementation notes. Older
        // renderers may still send their model annotations as stale drafts;
        // discard them before validating override provenance.
        if is_workshop_anchor_id(&draft.document_id) {
            continue;
        }
        check_id(&draft.document_id)?;
        validate_text(&draft.reason, "impact reason", MAX_DETAIL_BYTES)?;
        if draft.reason.trim().is_empty() {
            return Err(CoreError::new(
                "InvalidRequest",
                "An impact classification requires a reason.",
            ));
        }
        if overrides
            .insert(draft.document_id.as_str(), draft)
            .is_some()
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "An impact classification may appear only once per target.",
            ));
        }
    }
    let mut available = existing_document_ids(connection)?;
    available.extend(targets.iter().map(|target| target.document_id.clone()));
    let candidates = workshop_candidate_outputs_with_filter(
        connection,
        Some(project_id),
        Some(operation_namespace),
        Some(source_epoch),
    )?;
    let mut actual_targets = HashSet::new();
    let mut impacts = Vec::new();
    for candidate_id in candidate_ids {
        let Some((_, candidate, _relationship)) = candidates.get(candidate_id) else {
            return Err(CoreError::new(
                "InvalidWorkshopCandidate",
                "A workshop candidate is not a completed validated result from this project.",
            ));
        };
        for affected in &candidate.affected_targets {
            // A model can see the run's internal anchor in its frozen packet,
            // but it is never story material eligible for an adoption review
            // flag. Keep the raw candidate/packet history unchanged.
            if is_workshop_anchor_id(&affected.document_id) {
                continue;
            }
            if !available.contains(&affected.document_id) {
                return Err(CoreError::new(
                    "InvalidWorkshopImpactTarget",
                    "A candidate affected target does not belong to this project or adoption.",
                ));
            }
            actual_targets.insert(affected.document_id.as_str());
            let (kind, reason) =
                if let Some(override_draft) = overrides.get(affected.document_id.as_str()) {
                    (override_draft.kind, override_draft.reason.clone())
                } else {
                    // Model output can identify affected material, but only the
                    // author may classify a contradiction or another stronger
                    // impact category.
                    (WorkshopImpactKind::PossibleTension, affected.reason.clone())
                };
            impacts.push(WorkshopAdoptionImpact {
                candidate_id: candidate_id.clone(),
                document_id: affected.document_id.clone(),
                kind,
                reason,
                status: WorkshopImpactStatus::NeedsReview,
            });
        }
    }
    if overrides
        .keys()
        .any(|document_id| !actual_targets.contains(document_id))
    {
        return Err(CoreError::new(
            "InvalidWorkshopImpactTarget",
            "An impact classification must refer to an actual candidate affected target.",
        ));
    }
    Ok(impacts)
}

fn is_workshop_anchor_id(document_id: &str) -> bool {
    document_id.starts_with("workshop-")
}

fn session_is_ancestor(
    state: &WorkshopState,
    session_id: &str,
    candidate_session_id: &str,
) -> bool {
    let sessions: HashMap<&str, &WorkshopSession> = state
        .sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect();
    let mut current = Some(session_id);
    for _ in 0..=MAX_SESSIONS {
        let Some(id) = current else { return false };
        if id == candidate_session_id {
            return true;
        }
        current = sessions
            .get(id)
            .and_then(|session| session.parent_session_id.as_deref());
    }
    false
}

/// A fixed decision constrains a request when the request names its source,
/// explicitly includes its source, explores a validated relationship endpoint,
/// or descends from the decision's exploration. Protection remains durable
/// across decision status changes; this predicate only controls packet context.
pub(crate) fn fixed_decision_is_relevant(
    state: &WorkshopState,
    session: &WorkshopSession,
    decision: &WorkshopDecision,
    relationship: Option<&WorkshopRelationship>,
) -> bool {
    session.focus_document_id.as_deref() == Some(decision.document_id.as_str())
        || session
            .included_document_ids
            .iter()
            .any(|document_id| document_id == &decision.document_id)
        || relationship.is_some_and(|relationship| {
            relationship.from_document_id == decision.document_id
                || relationship.to_document_id == decision.document_id
        })
        || session_is_ancestor(state, &session.id, &decision.session_id)
}

fn candidate_relationship_matches_session(
    state: &WorkshopState,
    session_id: &str,
    candidate_relationship: Option<&WorkshopRelationship>,
) -> bool {
    let Some(session) = state
        .sessions
        .iter()
        .find(|session| session.id == session_id)
    else {
        return false;
    };
    let candidate_id = candidate_relationship.map(|relationship| relationship.id.as_str());
    if session.relationship_id.as_deref() != candidate_id {
        return false;
    }
    let Some(expected) = candidate_relationship else {
        return true;
    };
    state
        .relationships
        .iter()
        .find(|relationship| relationship.id == expected.id)
        .is_some_and(|current| current == expected)
}

fn validate_candidate_provenance(
    connection: &Connection,
    state: &WorkshopState,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: &str,
    requested_session: Option<&str>,
    candidate_ids: &[String],
) -> CoreResult<()> {
    let candidates = workshop_candidate_sessions(
        connection,
        project_id,
        operation_namespace,
        Some(source_epoch),
    )?;
    let historical_candidates = historical_workshop_candidate_records(connection)?
        .into_iter()
        .map(|(candidate_id, (session_id, _content))| (candidate_id, session_id))
        .collect::<HashMap<_, _>>();
    let validate_for = |session_id: &str, ids: &[String]| -> CoreResult<()> {
        for candidate_id in ids {
            let Some((candidate_session_id, candidate_relationship)) = candidates.get(candidate_id)
            else {
                let detail = if historical_candidates.contains_key(candidate_id) {
                    "A workshop candidate was generated against an older source epoch; generate a fresh result before adoption."
                } else {
                    "A workshop candidate is not a completed validated result from this project."
                };
                return Err(CoreError::new("InvalidWorkshopCandidate", detail));
            };
            if !session_is_ancestor(state, session_id, candidate_session_id) {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A workshop candidate belongs to another exploration branch.",
                ));
            }
            if !candidate_relationship_matches_session(
                state,
                session_id,
                candidate_relationship.as_ref(),
            ) {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A workshop candidate was generated for a different relationship scope; generate a fresh result before adoption.",
                ));
            }
        }
        Ok(())
    };
    let validate_historical = |session_id: &str, ids: &[String]| -> CoreResult<()> {
        for candidate_id in ids {
            let Some(candidate_session_id) = historical_candidates.get(candidate_id) else {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A workshop candidate is not a completed validated result from this project.",
                ));
            };
            if !session_is_ancestor(state, session_id, candidate_session_id) {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A workshop candidate belongs to another exploration branch.",
                ));
            }
        }
        Ok(())
    };
    if let Some(session_id) = requested_session {
        validate_for(session_id, candidate_ids)?;
    }
    for session in &state.sessions {
        for detail in &session.selected_details {
            if let Some(candidate_id) = &detail.candidate_id {
                validate_historical(&session.id, std::slice::from_ref(candidate_id))?;
            }
        }
        validate_historical(
            &session.id,
            &session
                .choices
                .iter()
                .map(|choice| choice.candidate_id.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}

fn validate_relationship_freshness(
    connection: &Connection,
    state: &WorkshopState,
    previous: Option<&WorkshopState>,
) -> CoreResult<()> {
    let previous_relationships: HashMap<&str, &WorkshopRelationship> = previous
        .into_iter()
        .flat_map(|state| state.relationships.iter())
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect();
    for relationship in &state.relationships {
        if previous_relationships
            .get(relationship.id.as_str())
            .is_some_and(|old| *old == relationship)
        {
            // A stale facet remains visible for author review until the
            // author edits or replaces the relationship explicitly.
            continue;
        }
        for head in &relationship.source_heads {
            let current = read_document(connection, &head.document_id)?;
            if current.head != *head {
                return Err(CoreError::new(
                    "StaleRelationship",
                    "A relationship source changed; review the relationship before saving it.",
                ));
            }
        }
    }
    Ok(())
}

fn body_text(value: &Value) -> String {
    // Match the renderer's restricted-snapshot projection, including block
    // boundaries and hard breaks. Concatenating text nodes would change the
    // literal the author protected when a selection spans paragraphs.
    value["body"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|block| {
            block["content"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|node| {
                    if node["type"] == "hardBreak" {
                        "\n"
                    } else {
                        node["text"].as_str().unwrap_or_default()
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn body_blocks(value: &Value) -> CoreResult<&Vec<Value>> {
    value["body"]["content"].as_array().ok_or_else(|| {
        CoreError::new(
            "InvalidDocument",
            "The document body has no content blocks.",
        )
    })
}

fn canonical_body(body: &Value) -> CoreResult<(Value, String)> {
    let receipt = validate_snapshot_json(&serde_json::to_string(body)?)
        .map_err(|error| CoreError::new("InvalidDocument", &error))?;
    Ok((receipt.snapshot, receipt.hash))
}

fn validate_protected_text(
    old: Option<&DocumentRecord>,
    new_body: &Value,
    protected: &[String],
    must_exist_in_old: &[String],
) -> CoreResult<()> {
    let new_text = body_text(new_body);
    for text in protected {
        validate_text(text, "protected text", MAX_DETAIL_BYTES)?;
        if text.is_empty() || !new_text.contains(text) {
            return Err(CoreError::new(
                "ProtectedContentChanged",
                "The adoption result removed protected text.",
            ));
        }
        if old.is_some_and(|record| {
            must_exist_in_old.contains(text) && !body_text(&record.body).contains(text)
        }) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Protected text is not present in the source document.",
            ));
        }
    }
    Ok(())
}

fn target_fixed_text(
    connection: &Connection,
    state: &WorkshopState,
    session_id: &str,
    document: &DocumentRecord,
) -> CoreResult<Vec<String>> {
    let session = state
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The adoption session does not exist."))?;
    let mut protected = session
        .selected_details
        .iter()
        .filter(|detail| {
            detail.fixed
                && !detail.text.is_empty()
                && body_text(&document.body).contains(&detail.text)
        })
        .map(|detail| detail.text.clone())
        .collect::<Vec<_>>();
    for decision in state.decisions.iter().filter(|decision| {
        // Protection is independent from decision status. Archiving or
        // superseding a decision must not release its protected source; the
        // author must explicitly clear Keep fixed first.
        decision.fixed && decision.document_id == document.head.document_id
    }) {
        if decision.protected_text.is_empty() {
            protected.push(body_text(
                &read_revision(connection, &decision.revision_id)?.body,
            ));
        } else {
            protected.extend(decision.protected_text.iter().cloned());
        }
    }
    protected.sort();
    protected.dedup();
    Ok(protected)
}

fn validate_adoption_targets(
    connection: &Connection,
    state: &WorkshopState,
    session_id: &str,
    targets: &mut [WorkshopAdoptionTarget],
) -> CoreResult<Vec<DocumentRecord>> {
    if targets.is_empty() || targets.len() > 64 {
        return Err(CoreError::new(
            "InvalidRequest",
            "Adoption requires 1..64 targets.",
        ));
    }
    let mut ids = HashSet::new();
    let mut before = Vec::new();
    for target in targets {
        check_id(&target.document_id)?;
        if !ids.insert(&target.document_id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Adoption target IDs must be unique.",
            ));
        }
        validate_title(&target.title)?;
        validate_kind(&target.kind)?;
        let (canonical, _) = canonical_body(&target.body)?;
        target.body = canonical;
        let current = match read_document(connection, &target.document_id) {
            Ok(document) => Some(document),
            Err(error) if error.code == "DocumentNotFound" => None,
            Err(error) => return Err(error),
        };
        match (&target.expected, current) {
            (None, Some(_)) => {
                return Err(CoreError::new(
                    "VersionConflict",
                    "A new adoption target already exists.",
                ));
            }
            (None, None) if target.mode != AdoptionMode::Add => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "New adoption targets must use add mode.",
                ));
            }
            (None, None) => {}
            (Some(expected), Some(current)) => {
                if expected != &current.head {
                    let mut error = CoreError::new(
                        "VersionConflict",
                        "An adoption target has changed since preview.",
                    );
                    error.current_head = Some(current.head.clone());
                    return Err(error);
                }
                if current.kind != target.kind || current.head.document_id != target.document_id {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An adoption target kind or identity changed.",
                    ));
                }
                if current.title != target.title {
                    return Err(CoreError::new(
                        "InvalidAdoption",
                        "An existing document title cannot change during workshop adoption.",
                    ));
                }
                let fixed = target_fixed_text(connection, state, session_id, &current)?;
                validate_protected_text(Some(&current), &target.body, &fixed, &fixed)?;
                if target.mode == AdoptionMode::Add {
                    let source = body_blocks(&current.body)?;
                    let result = body_blocks(&target.body)?;
                    if result.len() <= source.len() || result[..source.len()] != source[..] {
                        return Err(CoreError::new(
                            "InvalidAdoption",
                            "Append adoption must preserve every existing block exactly.",
                        ));
                    }
                }
                before.push(current);
            }
            (Some(_), None) => {
                return Err(CoreError::new(
                    "DocumentNotFound",
                    "An adoption target no longer exists.",
                ));
            }
        }
    }
    Ok(before)
}

fn validate_relationship_draft_fields(draft: &WorkshopRelationshipDraft) -> CoreResult<()> {
    check_id(&draft.id)?;
    for (document_id, expected) in [
        (&draft.from_document_id, &draft.from_expected),
        (&draft.to_document_id, &draft.to_expected),
    ] {
        check_id(document_id)?;
        if let Some(expected) = expected {
            if expected.document_id != *document_id || !valid_hash(&expected.body_hash) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship endpoint has an invalid expected head.",
                ));
            }
            parse_version(&expected.version)?;
        }
    }
    if draft.from_document_id == draft.to_document_id {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop relationship cannot connect a document to itself.",
        ));
    }
    for (value, label) in [
        (&draft.relationship_type, "relationship type"),
        (&draft.description, "relationship description"),
        (&draft.uncertainty, "relationship uncertainty"),
    ] {
        validate_text(value, label, MAX_DETAIL_BYTES)?;
    }
    if draft.relationship_type.trim().is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop relationship requires a type.",
        ));
    }
    if draft.description.trim().is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop relationship requires a description.",
        ));
    }
    Ok(())
}

fn projected_target_heads(
    connection: &Connection,
    targets: &[WorkshopAdoptionTarget],
) -> CoreResult<HashMap<String, Head>> {
    let mut heads = HashMap::new();
    for target in targets {
        let (_, body_hash) = canonical_body(&target.body)?;
        let version = match read_document(connection, &target.document_id) {
            Ok(current) => parse_version(&current.head.version)?
                .checked_add(1)
                .ok_or_else(|| {
                    CoreError::new("VersionLimit", "The document version limit was reached.")
                })?,
            Err(error) if error.code == "DocumentNotFound" => 0,
            Err(error) => return Err(error),
        };
        heads.insert(
            target.document_id.clone(),
            Head {
                document_id: target.document_id.clone(),
                version: version.to_string(),
                body_hash,
            },
        );
    }
    Ok(heads)
}

fn validate_relationship_drafts(
    connection: &Connection,
    state: &WorkshopState,
    targets: &[WorkshopAdoptionTarget],
    drafts: &[WorkshopRelationshipDraft],
) -> CoreResult<Vec<WorkshopRelationship>> {
    if drafts.len() > MAX_RELATIONSHIPS {
        return Err(CoreError::new(
            "InvalidRequest",
            "The adoption contains too many relationship drafts.",
        ));
    }
    let target_map: HashMap<&str, &WorkshopAdoptionTarget> = targets
        .iter()
        .map(|target| (target.document_id.as_str(), target))
        .collect();
    let mut ids: HashSet<&str> = state
        .relationships
        .iter()
        .map(|relationship| relationship.id.as_str())
        .collect();
    let projected = projected_target_heads(connection, targets)?;
    let mut resolved = Vec::with_capacity(drafts.len());
    for draft in drafts {
        validate_relationship_draft_fields(draft)?;
        if !ids.insert(draft.id.as_str()) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A relationship draft ID is already in use.",
            ));
        }
        let mut endpoint_heads = Vec::with_capacity(2);
        for (document_id, expected) in [
            (&draft.from_document_id, &draft.from_expected),
            (&draft.to_document_id, &draft.to_expected),
        ] {
            let current = match read_document(connection, document_id) {
                Ok(current) => Some(current),
                Err(error) if error.code == "DocumentNotFound" => None,
                Err(error) => return Err(error),
            };
            let head = if let Some(current) = current {
                if !["character", "world"].contains(&current.kind.as_str()) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "Workshop relationships may only connect character or world documents.",
                    ));
                }
                let Some(expected) = expected else {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An existing relationship endpoint requires its current head.",
                    ));
                };
                if expected != &current.head {
                    let mut error = CoreError::new(
                        "VersionConflict",
                        "A relationship endpoint changed since this adoption was prepared.",
                    );
                    error.current_head = Some(current.head);
                    return Err(error);
                }
                if let Some(target) = target_map.get(document_id.as_str())
                    && target.kind != current.kind
                {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "A relationship endpoint kind does not match its adoption target.",
                    ));
                }
                projected
                    .get(document_id.as_str())
                    .cloned()
                    .unwrap_or(current.head)
            } else {
                if expected.is_some() {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "A new relationship endpoint must not include an expected head.",
                    ));
                }
                let Some(target) = target_map.get(document_id.as_str()) else {
                    return Err(CoreError::new(
                        "DocumentNotFound",
                        "A relationship endpoint does not belong to this project or adoption.",
                    ));
                };
                if target.expected.is_some() || target.mode != AdoptionMode::Add {
                    return Err(CoreError::new(
                        "VersionConflict",
                        "A new relationship endpoint must be an added adoption target.",
                    ));
                }
                if !["character", "world"].contains(&target.kind.as_str()) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "Workshop relationships may only connect character or world documents.",
                    ));
                }
                projected
                    .get(document_id.as_str())
                    .cloned()
                    .ok_or_else(|| {
                        CoreError::new(
                            "InvalidProject",
                            "A relationship endpoint has no projected adoption head.",
                        )
                    })?
            };
            endpoint_heads.push(head);
        }
        resolved.push(WorkshopRelationship {
            id: draft.id.clone(),
            from_document_id: draft.from_document_id.clone(),
            to_document_id: draft.to_document_id.clone(),
            relationship_type: draft.relationship_type.clone(),
            description: draft.description.clone(),
            uncertainty: draft.uncertainty.clone(),
            status: WorkshopRelationshipStatus::Chosen,
            source_heads: endpoint_heads,
        });
    }
    Ok(resolved)
}

fn materialize_relationship_drafts(
    drafts: &[WorkshopRelationshipDraft],
    heads: &HashMap<String, Head>,
) -> CoreResult<Vec<WorkshopRelationship>> {
    drafts
        .iter()
        .map(|draft| {
            let from = heads.get(&draft.from_document_id).cloned().ok_or_else(|| {
                CoreError::new(
                    "InvalidProject",
                    "A committed relationship endpoint head is missing.",
                )
            })?;
            let to = heads.get(&draft.to_document_id).cloned().ok_or_else(|| {
                CoreError::new(
                    "InvalidProject",
                    "A committed relationship endpoint head is missing.",
                )
            })?;
            Ok(WorkshopRelationship {
                id: draft.id.clone(),
                from_document_id: draft.from_document_id.clone(),
                to_document_id: draft.to_document_id.clone(),
                relationship_type: draft.relationship_type.clone(),
                description: draft.description.clone(),
                uncertainty: draft.uncertainty.clone(),
                status: WorkshopRelationshipStatus::Chosen,
                source_heads: vec![from, to],
            })
        })
        .collect()
}

fn relationship_endpoint_sources(
    connection: &Connection,
    drafts: &[WorkshopRelationshipDraft],
) -> CoreResult<Vec<DocumentRecord>> {
    let mut seen = HashSet::new();
    let mut sources = Vec::new();
    for document_id in drafts
        .iter()
        .flat_map(|draft| [&draft.from_document_id, &draft.to_document_id])
    {
        if !seen.insert(document_id.clone()) {
            continue;
        }
        match read_document(connection, document_id) {
            Ok(document) => sources.push(document),
            Err(error) if error.code == "DocumentNotFound" => {}
            Err(error) => return Err(error),
        }
    }
    Ok(sources)
}

fn validate_preview_relationships(
    request: &PreviewWorkshopAdoption,
    preview: &WorkshopAdoptionPreview,
) -> CoreResult<()> {
    if request.relationships.len() != preview.relationships.len() {
        return Err(CoreError::new(
            "InvalidProject",
            "An adoption preview has mismatched relationship provenance.",
        ));
    }
    for (draft, relationship) in request.relationships.iter().zip(&preview.relationships) {
        validate_relationship_draft_fields(draft)?;
        if relationship.id != draft.id
            || relationship.from_document_id != draft.from_document_id
            || relationship.to_document_id != draft.to_document_id
            || relationship.relationship_type != draft.relationship_type
            || relationship.description != draft.description
            || relationship.uncertainty != draft.uncertainty
            || relationship.status != WorkshopRelationshipStatus::Chosen
            || relationship.source_heads.len() != 2
            || relationship.source_heads[0].document_id != draft.from_document_id
            || relationship.source_heads[1].document_id != draft.to_document_id
        {
            return Err(CoreError::new(
                "InvalidProject",
                "An adoption preview relationship is not bound to its request.",
            ));
        }
        for head in &relationship.source_heads {
            parse_version(&head.version)?;
            if !valid_hash(&head.body_hash) {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An adoption preview relationship has an invalid source hash.",
                ));
            }
        }
    }
    for impact in &preview.impacts {
        check_id(&impact.candidate_id)?;
        check_id(&impact.document_id)?;
        validate_text(&impact.reason, "impact reason", MAX_DETAIL_BYTES)?;
        if impact.status != WorkshopImpactStatus::NeedsReview
            || !request.candidate_ids.contains(&impact.candidate_id)
        {
            return Err(CoreError::new(
                "InvalidProject",
                "An adoption preview impact has invalid provenance.",
            ));
        }
    }
    let mut relationship_heads = preview
        .relationships
        .iter()
        .flat_map(|relationship| relationship.source_heads.iter())
        .map(|head| {
            (
                head.document_id.as_str(),
                head.version.as_str(),
                head.body_hash.as_str(),
            )
        })
        .collect::<HashSet<_>>();
    relationship_heads.extend(preview.before.iter().map(|record| {
        (
            record.head.document_id.as_str(),
            record.head.version.as_str(),
            record.head.body_hash.as_str(),
        )
    }));
    for source in &preview.endpoint_sources {
        if !relationship_heads.contains(&(
            source.head.document_id.as_str(),
            source.head.version.as_str(),
            source.head.body_hash.as_str(),
        )) {
            return Err(CoreError::new(
                "InvalidProject",
                "An adoption preview endpoint source is not bound to a relationship head.",
            ));
        }
    }
    Ok(())
}

fn validate_target_dependencies(
    state: &WorkshopState,
    targets: &[WorkshopAdoptionTarget],
    connection: &Connection,
) -> CoreResult<()> {
    let mut available = existing_document_ids(connection)?;
    available.extend(targets.iter().map(|target| target.document_id.clone()));
    for relationship in &state.relationships {
        if !available.contains(&relationship.from_document_id)
            || !available.contains(&relationship.to_document_id)
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "Adoption would leave a dangling relationship endpoint.",
            ));
        }
    }
    Ok(())
}

fn operation_payload<T: Serialize>(value: &T) -> CoreResult<String> {
    logical_hash(value)
}

fn existing_workshop_receipt(
    connection: &Connection,
    namespace: &str,
    operation_id: &str,
    kind: &str,
    payload_hash: &str,
) -> CoreResult<Option<Value>> {
    if let Some((stored_kind, stored_hash, result)) = connection
        .query_row(
            "SELECT operation_kind,payload_hash,result_json FROM workshop_receipts WHERE operation_namespace=? AND operation_id=?",
            params![namespace, operation_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        )
        .optional()?
    {
        if stored_kind != kind || stored_hash != payload_hash {
            return Err(CoreError::new("OperationIdReusedWithDifferentPayload", "This operation ID was already used for another workshop request."));
        }
        return Ok(Some(serde_json::from_str(&result)?));
    }
    for table in ["command_receipts", "proposal_receipts"] {
        let kind_column = if table == "command_receipts" {
            "operation_kind"
        } else {
            "kind"
        };
        let found: Option<(String, String)> = connection
            .query_row(
                &format!("SELECT {kind_column},payload_hash FROM {table} WHERE operation_namespace=? AND operation_id=?"),
                params![namespace, operation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if found.is_some() {
            return Err(CoreError::new(
                "OperationIdReusedWithDifferentPayload",
                "This operation ID belongs to another command family.",
            ));
        }
    }
    Ok(None)
}

fn read_workshop_results(
    connection: &Connection,
    state: &WorkshopState,
    project_id: &str,
    operation_namespace: &str,
    source_epoch: &str,
) -> CoreResult<Vec<WorkshopResult>> {
    let mut statement = connection.prepare("SELECT id FROM discussion_runs ORDER BY rowid")?;
    let run_ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut results = Vec::new();
    for run_id in run_ids {
        let run = crate::projects::discussions::read_run(connection, &run_id)?;
        if run.intent != crate::projects::discussions::FeedbackIntent::WorkshopExplore {
            continue;
        }
        let packet =
            crate::projects::context_packets::validated_packet_record(connection, &run.packet_id)?;
        let instruction = packet
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content.as_str())
            .ok_or_else(|| {
                CoreError::new(
                    "InvalidProject",
                    "A workshop discussion packet has no user instruction.",
                )
            })?;
        let metadata =
            crate::projects::workshop_generation::metadata_from_instruction(instruction)?;
        let (frozen, namespace) = crate::projects::story_context::validated_snapshot_record(
            connection,
            &packet.receipt.snapshot_id,
        )?;
        let historical_identity =
            frozen.snapshot.project_id != project_id || namespace != operation_namespace;
        if historical_identity {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND trashed=0)",
                [&run.target.document_id],
                |row| row.get(0),
            )?;
            if !exists {
                continue;
            }
        }
        let current_session = state
            .sessions
            .iter()
            .find(|session| session.id == metadata.exploration.session_id);
        let relationship_stale = current_session.is_none_or(|session| {
            !candidate_relationship_matches_session(
                state,
                &session.id,
                metadata.relationship.as_ref(),
            )
        });
        let stale = historical_identity
            || frozen.snapshot.context_source_epoch != source_epoch
            || current_session.is_none_or(|session| {
                session.working_generation != metadata.exploration.working_generation
            })
            || relationship_stale;
        let (output, validation_error) = if run.status
            == crate::projects::discussions::DiscussionRunStatus::Completed
            && run.dispatch_state == "delivered"
        {
            match crate::projects::workshop_generation::validate_workshop_output(
                &run.output_text,
                &metadata,
                &run.id,
            ) {
                Ok(output) => (Some(output), None),
                Err(error) => (None, Some(error.detail)),
            }
        } else {
            (None, None)
        };
        results.push(WorkshopResult {
            run,
            session_id: metadata.exploration.session_id,
            working_generation: metadata.exploration.working_generation,
            action: metadata.exploration.action,
            working_selection: metadata.exploration.working_selection,
            output,
            validation_error,
            stale,
        });
    }
    Ok(results)
}

fn insert_snapshot(
    connection: &Connection,
    project_id: &str,
    namespace: &str,
    operation_id: &str,
    version: i64,
    payload_hash: &str,
    state: &WorkshopState,
) -> CoreResult<()> {
    let (json, hash) = state_json(state)?;
    connection.execute(
        "INSERT INTO workshop_snapshots(id,project_id,operation_namespace,operation_id,version,payload_hash,state_json,state_hash) VALUES(?,?,?,?,?,?,?,?)",
        params![new_id(), project_id, namespace, operation_id, version, payload_hash, json, hash],
    )?;
    Ok(())
}

fn store_state(connection: &Connection, version: i64, state: &WorkshopState) -> CoreResult<()> {
    let (json, hash) = state_json(state)?;
    let updated = connection.execute(
        "UPDATE workshop_state SET version=?,state_json=?,state_hash=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE singleton=1",
        params![version, json, hash],
    )?;
    if updated == 0 {
        connection.execute(
            "INSERT INTO workshop_state(singleton,version,state_json,state_hash) VALUES(1,?,?,?)",
            params![version, json, hash],
        )?;
    }
    Ok(())
}

fn resolve_workshop_relationship(
    connection: &Connection,
    state: &WorkshopState,
    session: &WorkshopSession,
) -> CoreResult<Option<WorkshopRelationship>> {
    let Some(relationship_id) = session.relationship_id.as_deref() else {
        return Ok(None);
    };
    check_id(relationship_id)?;
    let relationship = state
        .relationships
        .iter()
        .find(|relationship| relationship.id == relationship_id)
        .cloned()
        .ok_or_else(|| {
            CoreError::new(
                "InvalidWorkshopRelationship",
                "The relationship exploration target does not exist.",
            )
        })?;
    if relationship.status == WorkshopRelationshipStatus::Archived {
        return Err(CoreError::new(
            "InvalidWorkshopRelationship",
            "Archived relationships cannot be explored.",
        ));
    }
    if relationship.from_document_id == relationship.to_document_id
        || relationship.source_heads.len() != 2
    {
        return Err(CoreError::new(
            "InvalidWorkshopRelationship",
            "The relationship endpoints are incomplete.",
        ));
    }
    let mut source_heads = HashMap::new();
    for head in &relationship.source_heads {
        if head.document_id != relationship.from_document_id
            && head.document_id != relationship.to_document_id
        {
            return Err(CoreError::new(
                "StaleRelationship",
                "A relationship source does not match its endpoint.",
            ));
        }
        if source_heads
            .insert(head.document_id.clone(), head.clone())
            .is_some()
        {
            return Err(CoreError::new(
                "StaleRelationship",
                "A relationship contains duplicate endpoint sources.",
            ));
        }
    }
    if source_heads.len() != 2 {
        return Err(CoreError::new(
            "StaleRelationship",
            "A relationship is missing an exact endpoint source.",
        ));
    }
    for document_id in [&relationship.from_document_id, &relationship.to_document_id] {
        let document = read_document(connection, document_id).map_err(|error| {
            CoreError::new(
                "StaleRelationship",
                &format!("A relationship endpoint is unavailable: {}", error.detail),
            )
        })?;
        if !matches!(document.kind.as_str(), "character" | "world") {
            return Err(CoreError::new(
                "InvalidWorkshopRelationship",
                "Relationship exploration requires character or world endpoints.",
            ));
        }
        if source_heads.get(document_id.as_str()) != Some(&document.head) {
            return Err(CoreError::new(
                "StaleRelationship",
                "A relationship endpoint changed; review the relationship before exploring it.",
            ));
        }
    }
    Ok(Some(relationship))
}

impl OwnedProject {
    /// Start is kept on the project actor so the workshop CAS, first-anchor
    /// creation, frozen packet, and discussion run all use one serialized
    /// project boundary. A durable workshop receipt is checked before reading
    /// the current workshop version: retrying a lost acknowledgment must
    /// replay the original run even when the editor has since advanced.
    pub(super) fn start_workshop(
        &mut self,
        request: crate::projects::workshop_generation::StartWorkshop,
    ) -> CoreResult<crate::projects::discussions::DiscussionStart> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        let payload_hash = operation_payload(&request)?;
        if let Some(value) = existing_workshop_receipt(
            self.db()?,
            &request.access.operation_namespace,
            &request.operation_id,
            "startWorkshop",
            &payload_hash,
        )? {
            let started = serde_json::from_value(value).map_err(|error| {
                CoreError::new(
                    "InvalidProject",
                    &format!("The workshop start receipt is invalid: {error}"),
                )
            })?;
            return Ok(started);
        }

        // A process loss can occur after the discussion transaction commits but
        // before the workshop receipt is written. In that narrow recovery
        // window, the immutable discussion row is still the authoritative
        // operation result. Match the frozen Workshop envelope and binding
        // before replaying it; a different request remains an operation-ID
        // collision.
        if let Some(run_id) = self.existing_workshop_run(&request.access, &request.operation_id)? {
            let started = crate::projects::discussions::read_start(self.db()?, &run_id)?;
            let metadata = started
                .packet
                .messages
                .iter()
                .rev()
                .find(|message| message.role == "user")
                .map(|message| {
                    crate::projects::workshop_generation::metadata_from_instruction(
                        &message.content,
                    )
                })
                .transpose()?;
            let prepared_budget_json: String = self.db()?.query_row(
                "SELECT request_json FROM context_packets WHERE id=?",
                [&started.run.packet_id],
                |row| row.get(0),
            )?;
            let prepared: crate::projects::context_packets::PrepareContext =
                serde_json::from_str(&prepared_budget_json)?;
            let budget_matches = prepared.budget.model_id == request.budget.model_id
                && prepared.budget.context_window_tokens == request.budget.context_window_tokens
                && prepared.budget.reserved_output_tokens == request.budget.reserved_output_tokens
                && prepared.budget.reserved_protocol_tokens
                    == request.budget.reserved_protocol_tokens;
            let access_matches = prepared.access.project_id == request.access.project_id
                && prepared.access.operation_namespace == request.access.operation_namespace
                && prepared.operation_id == request.operation_id;
            let matches = started.run.intent
                == crate::projects::discussions::FeedbackIntent::WorkshopExplore
                && metadata.is_some_and(|metadata| metadata.exploration == request.exploration)
                && started.packet.options.provider_binding == request.provider_binding
                && budget_matches
                && access_matches;
            if !matches {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This operation ID was already used for a different workshop request.",
                ));
            }
            return Ok(started);
        }

        let (version, state) = read_state(self.db()?)?;
        let expected_version = parse_version(&request.exploration.expected_version)?;
        if version != expected_version {
            return Err(CoreError::new(
                "VersionConflict",
                "The workshop changed; reload it before generating.",
            ));
        }
        let session = state
            .sessions
            .iter()
            .find(|session| session.id == request.exploration.session_id)
            .cloned()
            .ok_or_else(|| {
                CoreError::new("InvalidWorkshop", "The workshop session does not exist.")
            })?;
        if session.working_generation != request.exploration.working_generation {
            return Err(CoreError::new(
                "InvalidWorkshop",
                "The workshop working version changed before generation.",
            ));
        }
        let relationship = resolve_workshop_relationship(self.db()?, &state, &session)?;
        let anchor = self.ensure_workshop_anchor(&session)?;
        let source_epoch = current_context_epoch(self.db()?)?;
        let candidates = workshop_candidate_records(
            self.db()?,
            &self.info.project_id,
            &request.access.operation_namespace,
            Some(&source_epoch),
        )?;
        let mut included_alternatives = Vec::new();
        for choice in session.choices.iter().filter(|choice| {
            choice.status == CandidateChoiceStatus::Saved && choice.include_in_context
        }) {
            let Some((candidate_session_id, content, candidate_relationship)) =
                candidates.get(&choice.candidate_id)
            else {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A selected workshop candidate is stale; generate a fresh result before continuing.",
                ));
            };
            if !session_is_ancestor(&state, &session.id, candidate_session_id) {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A selected workshop candidate belongs to another exploration branch.",
                ));
            }
            if !candidate_relationship_matches_session(
                &state,
                &session.id,
                candidate_relationship.as_ref(),
            ) {
                return Err(CoreError::new(
                    "InvalidWorkshopCandidate",
                    "A selected workshop candidate belongs to a different relationship scope.",
                ));
            }
            included_alternatives.push(content.clone());
        }
        let mut chosen_details = Vec::new();
        let mut fixed_details = Vec::new();
        for decision in &state.decisions {
            let in_lineage = decision.status == WorkshopDecisionStatus::Chosen
                && session_is_ancestor(&state, &session.id, &decision.session_id);
            let relevant_fixed = decision.fixed
                && fixed_decision_is_relevant(&state, &session, decision, relationship.as_ref());
            let needs_full_protected_body = relevant_fixed && decision.protected_text.is_empty();
            if !in_lineage && !needs_full_protected_body {
                continue;
            }
            // Resolve the exact saved revision only for chosen lineage
            // material or a relevant fixed decision whose protection is a
            // whole source body. Unrelated fixed decisions stay out of the
            // packet entirely.
            let revision = read_revision(self.db()?, &decision.revision_id)?;
            let body = body_text(&revision.body);
            if in_lineage {
                chosen_details.push(format!(
                    "{}: {}\nAuthor rationale: {}",
                    decision.title, body, decision.rationale
                ));
            }
            if needs_full_protected_body {
                fixed_details.push(body);
            }
        }
        let generation = crate::projects::workshop_generation::from_session_with_material(
            request.clone(),
            &session,
            &state,
            anchor.head,
            crate::projects::workshop_generation::WorkshopResolvedMaterial {
                chosen_details,
                included_alternatives,
                fixed_details,
                relationship,
            },
        )?;
        let (discussion, _metadata) = generation.into_discussion()?;
        let started = self.start_discussion(discussion)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO workshop_receipts(operation_namespace,operation_id,operation_kind,payload_hash,result_json) VALUES(?,?,?,?,?)",
            params![
                request.access.operation_namespace,
                request.operation_id,
                "startWorkshop",
                payload_hash,
                serde_json::to_string(&started)?
            ],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(started)
    }

    fn existing_workshop_run(
        &self,
        access: &ProjectAccess,
        operation_id: &str,
    ) -> CoreResult<Option<String>> {
        self.db()?
            .query_row(
                "SELECT id FROM discussion_runs WHERE project_id=? AND operation_namespace=? AND operation_id=?",
                params![access.project_id, access.operation_namespace, operation_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(CoreError::from)
    }

    fn ensure_workshop_anchor(&mut self, session: &WorkshopSession) -> CoreResult<DocumentRecord> {
        let document_id = session.anchor_document_id.as_deref().ok_or_else(|| {
            CoreError::new(
                "InvalidWorkshop",
                "The workshop session has no anchor document for generation.",
            )
        })?;
        match read_document(self.db()?, document_id) {
            Ok(document) => return Ok(document),
            Err(error)
                if error.code == "DocumentNotFound" && document_id.starts_with("workshop-") => {}
            Err(error) => return Err(error),
        }
        let body = json!({
            "schemaVersion": 1,
            "body": {
                "type": "doc",
                "content": [{"type":"paragraph","attrs":{"id":document_id},"content":[]}]
            }
        });
        let (canonical, hash) = canonical_body(&body)?;
        let title = if session.working_title.trim().is_empty() {
            if session.title.trim().is_empty() {
                "Workshop anchor"
            } else {
                &session.title
            }
        } else {
            &session.working_title
        };
        validate_title(title)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let inserted = tx.execute(
            "INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash) VALUES(?,?,?,(SELECT COUNT(*) FROM documents),0,1,?,?) ON CONFLICT(id) DO NOTHING",
            params![document_id, "note", title, serde_json::to_string(&canonical)?, hash],
        )?;
        if inserted == 1 {
            tx.execute(
                "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
                [],
            )?;
        }
        tx.commit().map_err(CoreError::uncertain)?;
        read_document(self.db()?, document_id)
    }

    pub(super) fn read_workshop(&self, access: ProjectAccess) -> CoreResult<WorkshopView> {
        self.check_access(&access)?;
        let (version, state) = read_state(self.db()?)?;
        let source_epoch = current_context_epoch(self.db()?)?;
        Ok(WorkshopView {
            version: parse_stored_version(version)?,
            results: read_workshop_results(
                self.db()?,
                &state,
                &self.info.project_id,
                &access.operation_namespace,
                &source_epoch,
            )?,
            state,
        })
    }

    pub(super) fn save_workshop(&mut self, request: SaveWorkshop) -> CoreResult<WorkshopSnapshot> {
        self.check_access(&request.access)?;
        check_id(&request.operation_id)?;
        let expected = parse_version(&request.expected_version)?;
        let payload_hash = operation_payload(&request)?;
        let current_project_id = self.info.project_id.clone();
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(result) = existing_workshop_receipt(
            &tx,
            &request.access.operation_namespace,
            &request.operation_id,
            "saveWorkshop",
            &payload_hash,
        )? {
            let snapshot: WorkshopSnapshot = serde_json::from_value(result)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(snapshot);
        }
        let (current_version, current_state) = read_state(&tx)?;
        validate_state_references(&tx, &request.state, Some(&current_state), &HashSet::new())?;
        validate_relationship_freshness(&tx, &request.state, Some(&current_state))?;
        let source_epoch = current_context_epoch(&tx)?;
        validate_candidate_provenance(
            &tx,
            &request.state,
            &current_project_id,
            &request.access.operation_namespace,
            &source_epoch,
            None,
            &[],
        )?;
        if current_version != expected {
            return Err(CoreError::new(
                "VersionConflict",
                "The workshop changed; reload it before saving.",
            ));
        }
        let changed = request.state != current_state;
        let version = if changed {
            expected.checked_add(1).ok_or_else(|| {
                CoreError::new("VersionLimit", "The workshop version limit was reached.")
            })?
        } else {
            expected
        };
        if changed {
            store_state(&tx, version, &request.state)?;
            insert_snapshot(
                &tx,
                &current_project_id,
                &request.access.operation_namespace,
                &request.operation_id,
                version,
                &payload_hash,
                &request.state,
            )?;
        }
        let snapshot = workshop_snapshot(version, request.state.clone())?;
        tx.execute(
            "INSERT INTO workshop_receipts(operation_namespace,operation_id,operation_kind,payload_hash,result_json) VALUES(?,?,?,?,?)",
            params![request.access.operation_namespace, request.operation_id, "saveWorkshop", payload_hash, serde_json::to_string(&snapshot)?],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(snapshot)
    }

    pub(super) fn workshop_history(
        &self,
        access: ProjectAccess,
    ) -> CoreResult<Vec<WorkshopSnapshot>> {
        self.check_access(&access)?;
        let mut statement = self.db()?.prepare(
            "SELECT operation_namespace,operation_id,version,payload_hash,state_json,state_hash FROM workshop_snapshots ORDER BY version DESC,id DESC",
        )?;
        let mut output = Vec::new();
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })? {
            let (namespace, operation, version, payload, state, hash) = row?;
            if !storage_valid_id(&namespace) || !valid_hash(&payload) {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A workshop history snapshot has invalid identity or payload hash.",
                ));
            }
            let receipt_payload: Option<String> = self
                .db()?
                .query_row(
                    "SELECT payload_hash FROM workshop_receipts WHERE operation_namespace=? AND operation_id=?",
                    params![namespace, operation],
                    |row| row.get(0),
                )
                .optional()?;
            if receipt_payload.as_deref() != Some(payload.as_str()) {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A workshop history snapshot has no matching immutable receipt.",
                ));
            }
            output.push(workshop_snapshot(version, parse_state(&state, &hash)?)?);
        }
        Ok(output)
    }

    pub(super) fn preview_workshop_adoption(
        &mut self,
        request: PreviewWorkshopAdoption,
    ) -> CoreResult<WorkshopAdoptionPreview> {
        self.check_access(&request.access)?;
        check_id(&request.session_id)?;
        let expected = parse_version(&request.expected_version)?;
        validate_id_list(&request.candidate_ids, "adoption candidates")?;
        validate_text(&request.rationale, "adoption rationale", MAX_TEXT_BYTES)?;
        for text in &request.protected_text {
            validate_text(text, "protected text", MAX_DETAIL_BYTES)?;
        }
        let (version, state) = read_state(self.db()?)?;
        if version != expected {
            return Err(CoreError::new(
                "VersionConflict",
                "The workshop changed; reload before previewing adoption.",
            ));
        }
        if !state
            .sessions
            .iter()
            .any(|session| session.id == request.session_id)
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "The adoption session does not exist.",
            ));
        }
        let mut targets = request.targets.clone();
        let before =
            validate_adoption_targets(self.db()?, &state, &request.session_id, &mut targets)?;
        let target_ids = targets
            .iter()
            .map(|target| target.document_id.clone())
            .collect::<HashSet<_>>();
        validate_state_references(self.db()?, &state, None, &target_ids)?;
        let source_epoch = current_context_epoch(self.db()?)?;
        validate_candidate_provenance(
            self.db()?,
            &state,
            &self.info.project_id,
            &request.access.operation_namespace,
            &source_epoch,
            Some(&request.session_id),
            &request.candidate_ids,
        )?;
        let relationships =
            validate_relationship_drafts(self.db()?, &state, &targets, &request.relationships)?;
        let impacts = build_adoption_impacts(
            self.db()?,
            &self.info.project_id,
            &request.access.operation_namespace,
            &source_epoch,
            &request.candidate_ids,
            &targets,
            &request.impact_drafts,
        )?;
        let endpoint_sources = relationship_endpoint_sources(self.db()?, &request.relationships)?;
        validate_protected_texts_for_targets(
            self.db()?,
            &state,
            &request.session_id,
            &before,
            &targets,
            &request.protected_text,
        )?;
        validate_target_dependencies(&state, &targets, self.db()?)?;
        let request_json =
            serde_json::to_string(&crate::canonicalize_value(serde_json::to_value(&request)?))?;
        let preview = WorkshopAdoptionPreview {
            id: new_id(),
            session_id: request.session_id,
            expected_version: request.expected_version,
            targets,
            before,
            rationale: request.rationale,
            protected_text: request.protected_text,
            candidate_ids: request.candidate_ids.clone(),
            relationships,
            endpoint_sources,
            impacts,
        };
        let preview_json =
            serde_json::to_string(&crate::canonicalize_value(serde_json::to_value(&preview)?))?;
        let preview_hash = sha256_hex(preview_json.as_bytes());
        let current_project_id = self.info.project_id.clone();
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO workshop_adoption_previews(id,project_id,operation_namespace,session_id,expected_version,payload_hash,request_json,preview_json,preview_hash) VALUES(?,?,?,?,?,?,?,?,?)",
            params![preview.id, current_project_id, request.access.operation_namespace, preview.session_id, expected, sha256_hex(request_json.as_bytes()), request_json, preview_json, preview_hash],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(preview)
    }

    pub(super) fn adopt_workshop(
        &mut self,
        access: ProjectAccess,
        operation_id: String,
        preview_id: String,
    ) -> CoreResult<WorkshopAdoptionAck> {
        self.check_access(&access)?;
        check_id(&operation_id)?;
        check_id(&preview_id)?;
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct AdoptRequest<'a> {
            access: &'a ProjectAccess,
            operation_id: &'a str,
            preview_id: &'a str,
        }
        let payload_hash = operation_payload(&AdoptRequest {
            access: &access,
            operation_id: &operation_id,
            preview_id: &preview_id,
        })?;
        let current_project_id = self.info.project_id.clone();
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(result) = existing_workshop_receipt(
            &tx,
            &access.operation_namespace,
            &operation_id,
            "adoptWorkshop",
            &payload_hash,
        )? {
            let ack: WorkshopAdoptionAck = serde_json::from_value(result)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(ack);
        }
        let preview_row: Option<(String, String, String, i64, String, String, String)> = tx
            .query_row(
                "SELECT project_id,operation_namespace,session_id,expected_version,request_json,preview_json,preview_hash FROM workshop_adoption_previews WHERE id=?",
                [preview_id.as_str()],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
            )
            .optional()?;
        let (
            preview_project_id,
            namespace,
            session_id,
            expected,
            request_json,
            preview_json,
            preview_hash,
        ) = preview_row.ok_or_else(|| {
            CoreError::new("PreviewNotFound", "The adoption preview no longer exists.")
        })?;
        if preview_project_id != current_project_id || namespace != access.operation_namespace {
            return Err(CoreError::new(
                "WrongProjectSession",
                "This adoption preview belongs to another project identity.",
            ));
        }
        let preview: WorkshopAdoptionPreview = serde_json::from_str(&preview_json)?;
        let canonical_preview =
            serde_json::to_string(&crate::canonicalize_value(serde_json::to_value(&preview)?))?;
        if preview.id != preview_id || sha256_hex(canonical_preview.as_bytes()) != preview_hash {
            return Err(CoreError::new(
                "InvalidProject",
                "The adoption preview failed its fingerprint check.",
            ));
        }
        let stored_request: PreviewWorkshopAdoption = serde_json::from_str(&request_json)?;
        if stored_request.session_id != session_id
            || stored_request.access.operation_namespace != namespace
        {
            return Err(CoreError::new(
                "InvalidProject",
                "The adoption preview provenance is invalid.",
            ));
        }
        let (current_version, mut state) = read_state(&tx)?;
        if current_version != expected {
            return Err(CoreError::new(
                "VersionConflict",
                "The workshop changed after this preview was created.",
            ));
        }
        if preview.expected_version != expected.to_string() || preview.session_id != session_id {
            return Err(CoreError::new(
                "InvalidProject",
                "The adoption preview version is invalid.",
            ));
        }
        let mut targets = preview.targets.clone();
        let target_ids = targets
            .iter()
            .map(|target| target.document_id.clone())
            .collect::<HashSet<_>>();
        let before = validate_adoption_targets(&tx, &state, &preview.session_id, &mut targets)?;
        validate_state_references(&tx, &state, None, &target_ids)?;
        let source_epoch = current_context_epoch(&tx)?;
        validate_candidate_provenance(
            &tx,
            &state,
            &current_project_id,
            &access.operation_namespace,
            &source_epoch,
            Some(&preview.session_id),
            &preview.candidate_ids,
        )?;
        let expected_relationships =
            validate_relationship_drafts(&tx, &state, &targets, &stored_request.relationships)?;
        let expected_impacts = build_adoption_impacts(
            &tx,
            &current_project_id,
            &access.operation_namespace,
            &source_epoch,
            &preview.candidate_ids,
            &targets,
            &stored_request.impact_drafts,
        )?;
        if expected_relationships != preview.relationships || expected_impacts != preview.impacts {
            return Err(CoreError::new(
                "InvalidProject",
                "The adoption preview relationship or impact provenance changed.",
            ));
        }
        if !same_document_records(&before, &preview.before) {
            return Err(CoreError::new(
                "VersionConflict",
                "A preview source changed before adoption.",
            ));
        }
        validate_protected_texts_for_targets(
            &tx,
            &state,
            &preview.session_id,
            &before,
            &targets,
            &preview.protected_text,
        )?;
        validate_target_dependencies(&state, &targets, &tx)?;

        let mut resulting_documents = Vec::new();
        let mut decisions = Vec::new();
        let mut changed_heads: HashMap<String, Head> = HashMap::new();
        for target in &targets {
            let current = match read_document(&tx, &target.document_id) {
                Ok(document) => Some(document),
                Err(error) if error.code == "DocumentNotFound" => None,
                Err(error) => return Err(error),
            };
            let (canonical, hash) = canonical_body(&target.body)?;
            let record = if let Some(current) = current {
                checkpoint_at(&tx, &current, "beforeWorkshopAdoption")?;
                let next = parse_version(&current.head.version)?
                    .checked_add(1)
                    .ok_or_else(|| {
                        CoreError::new("VersionLimit", "The document version limit was reached.")
                    })?;
                tx.execute(
                    "UPDATE documents SET working_version=?,body_json=?,body_hash=?,projection_dirty=1 WHERE id=? AND working_version=? AND body_hash=?",
                    params![next, serde_json::to_string(&canonical)?, hash, target.document_id, parse_version(&current.head.version)?, current.head.body_hash],
                )?;
                let updated = read_document(&tx, &target.document_id)?;
                checkpoint_at(&tx, &updated, "workshopAdoption")?;
                read_document(&tx, &target.document_id)?
            } else {
                tx.execute(
                    "INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash) VALUES(?,?,?,(SELECT COUNT(*) FROM documents),0,1,?,?)",
                    params![target.document_id, target.kind, target.title, serde_json::to_string(&canonical)?, hash],
                )?;
                let inserted = read_document(&tx, &target.document_id)?;
                checkpoint_at(&tx, &inserted, "workshopAdoption")?;
                read_document(&tx, &target.document_id)?
            };
            changed_heads.insert(target.document_id.clone(), record.head.clone());
            let supersedes = state
                .decisions
                .iter()
                .rev()
                .find(|decision| {
                    decision.document_id == target.document_id
                        && decision.status == WorkshopDecisionStatus::Chosen
                })
                .map(|decision| decision.id.clone());
            let decision = WorkshopDecision {
                id: new_id(),
                session_id: preview.session_id.clone(),
                title: target.title.clone(),
                document_id: target.document_id.clone(),
                revision_id: record.last_checkpoint_id.clone().ok_or_else(|| {
                    CoreError::new(
                        "PersistenceUnavailable",
                        "Adoption revision was not created.",
                    )
                })?,
                head: record.head.clone(),
                candidate_ids: preview.candidate_ids.clone(),
                rationale: preview.rationale.clone(),
                status: WorkshopDecisionStatus::Chosen,
                fixed: !preview.protected_text.is_empty(),
                protected_text: preview.protected_text.clone(),
                access: "authorRoom".into(),
                supersedes_id: supersedes,
            };
            decisions.push(decision);
            resulting_documents.push(record);
        }
        let mut committed_heads = changed_heads.clone();
        for relationship in &expected_relationships {
            for document_id in [&relationship.from_document_id, &relationship.to_document_id] {
                if !committed_heads.contains_key(document_id) {
                    committed_heads
                        .insert(document_id.clone(), read_document(&tx, document_id)?.head);
                }
            }
        }
        let committed_relationships =
            materialize_relationship_drafts(&stored_request.relationships, &committed_heads)?;
        if committed_relationships != expected_relationships {
            return Err(CoreError::new(
                "VersionConflict",
                "A relationship endpoint changed while the adoption was being committed.",
            ));
        }
        if !changed_heads.is_empty() {
            tx.execute(
                "UPDATE project SET context_source_epoch=context_source_epoch+1 WHERE singleton=1",
                [],
            )?;
        }
        let decision_ids: Vec<String> = decisions
            .iter()
            .map(|decision| decision.id.clone())
            .collect();
        // A document has one current chosen workshop decision. Preserve the
        // immutable history, but retire the prior projection before adding
        // the new adoption so Story Bible consumers cannot see two competing
        // chosen revisions for the same target.
        for previous in &mut state.decisions {
            if previous.status == WorkshopDecisionStatus::Chosen
                && decisions
                    .iter()
                    .any(|next| next.document_id == previous.document_id)
            {
                previous.status = WorkshopDecisionStatus::Superseded;
            }
        }
        state.current_session_id = Some(preview.session_id.clone());
        state.decisions.extend(decisions.clone());
        let existing_relationships = state.relationships.clone();
        state.relationships.extend(committed_relationships);
        for relationship in &existing_relationships {
            for source in relationship
                .source_heads
                .iter()
                .filter(|head| changed_heads.contains_key(&head.document_id))
            {
                let decision_id = decisions
                    .iter()
                    .find(|decision| decision.document_id == source.document_id)
                    .map(|decision| decision.id.clone())
                    .ok_or_else(|| {
                        CoreError::new(
                            "InvalidRequest",
                            "A relationship impact has no adoption decision for its changed source.",
                        )
                    })?;
                state.impacts.push(WorkshopImpact {
                    id: new_id(),
                    decision_id,
                    document_id: source.document_id.clone(),
                    kind: WorkshopImpactKind::PossibleTension,
                    reason: format!(
                        "Relationship {} uses changed source {}; review whether it still holds.",
                        relationship.id, source.document_id
                    ),
                    status: WorkshopImpactStatus::NeedsReview,
                    candidate_id: None,
                    relationship_id: Some(relationship.id.clone()),
                });
            }
        }
        for impact in expected_impacts {
            let decision_id = decisions
                .iter()
                .find(|decision| decision.document_id == impact.document_id)
                .or_else(|| decisions.first())
                .map(|decision| decision.id.clone())
                .ok_or_else(|| {
                    CoreError::new(
                        "InvalidRequest",
                        "A candidate impact has no adoption decision.",
                    )
                })?;
            state.impacts.push(WorkshopImpact {
                id: new_id(),
                decision_id,
                document_id: impact.document_id,
                kind: impact.kind,
                reason: impact.reason,
                status: WorkshopImpactStatus::NeedsReview,
                candidate_id: Some(impact.candidate_id),
                relationship_id: None,
            });
        }
        let next_version = expected.checked_add(1).ok_or_else(|| {
            CoreError::new("VersionLimit", "The workshop version limit was reached.")
        })?;
        validate_state_shape(&state)?;
        store_state(&tx, next_version, &state)?;
        insert_snapshot(
            &tx,
            &current_project_id,
            &access.operation_namespace,
            &operation_id,
            next_version,
            &payload_hash,
            &state,
        )?;
        let snapshot = workshop_snapshot(next_version, state)?;
        let ack = WorkshopAdoptionAck {
            snapshot,
            documents: resulting_documents,
            decision_ids,
        };
        tx.execute(
            "INSERT INTO workshop_receipts(operation_namespace,operation_id,operation_kind,payload_hash,result_json) VALUES(?,?,?,?,?)",
            params![access.operation_namespace, operation_id, "adoptWorkshop", payload_hash, serde_json::to_string(&ack)?],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(ack)
    }
}

fn validate_protected_texts_for_targets(
    connection: &Connection,
    state: &WorkshopState,
    session_id: &str,
    before: &[DocumentRecord],
    targets: &[WorkshopAdoptionTarget],
    requested: &[String],
) -> CoreResult<()> {
    let mut before_by_id = before
        .iter()
        .map(|record| (record.head.document_id.as_str(), record))
        .collect::<HashMap<_, _>>();
    for target in targets {
        let mut protected = requested.to_vec();
        if let Some(old) = before_by_id.remove(target.document_id.as_str()) {
            let previous_fixed = target_fixed_text(connection, state, session_id, old)?;
            protected.extend(previous_fixed.clone());
            validate_protected_text(Some(old), &target.body, &protected, &previous_fixed)?;
        } else {
            validate_protected_text(None, &target.body, &protected, &[])?;
        }
    }
    Ok(())
}

fn same_document_records(left: &[DocumentRecord], right: &[DocumentRecord]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.head == right.head
                && left.title == right.title
                && left.kind == right.kind
                && left.metadata_version == right.metadata_version
                && left.body == right.body
                && left.last_checkpoint_id == right.last_checkpoint_id
        })
}

/// Validate workshop rows at backup/recovery boundaries. Historical previews
/// and receipts may retain an older identity after recovery, but all hashes,
/// JSON contracts, and bounded metadata must remain valid.
pub(crate) fn validate_storage(connection: &Connection) -> CoreResult<()> {
    if let Some((version, state, hash)) = connection
        .query_row(
            "SELECT version,state_json,state_hash FROM workshop_state WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
    {
        if version < 0 {
            return Err(CoreError::new(
                "InvalidBackup",
                "The workshop state version is negative.",
            ));
        }
        let parsed = parse_state(&state, &hash)
            .map_err(|error| CoreError::new("InvalidBackup", &error.detail))?;
        validate_state_shape(&parsed)
            .map_err(|error| CoreError::new("InvalidBackup", &error.detail))?;
    }
    let mut snapshots = connection.prepare("SELECT id,project_id,operation_namespace,operation_id,version,payload_hash,state_json,state_hash FROM workshop_snapshots")?;
    for row in snapshots.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })? {
        let (id, project, namespace, operation, version, payload, state, hash) = row?;
        if !storage_valid_id(&id)
            || !storage_valid_id(&project)
            || !storage_valid_id(&namespace)
            || !storage_valid_id(&operation)
            || version < 0
            || !valid_hash(&payload)
        {
            return Err(CoreError::new(
                "InvalidBackup",
                "A workshop snapshot has invalid identity or hash metadata.",
            ));
        }
        let receipt_payload: Option<String> = connection
            .query_row(
                "SELECT payload_hash FROM workshop_receipts WHERE operation_namespace=? AND operation_id=?",
                params![namespace, operation],
                |row| row.get(0),
            )
            .optional()?;
        if receipt_payload.as_deref() != Some(payload.as_str()) {
            return Err(CoreError::new(
                "InvalidBackup",
                "A workshop snapshot has no matching immutable receipt.",
            ));
        }
        let parsed = parse_state(&state, &hash)
            .map_err(|error| CoreError::new("InvalidBackup", &error.detail))?;
        validate_state_shape(&parsed)
            .map_err(|error| CoreError::new("InvalidBackup", &error.detail))?;
    }
    let mut previews = connection.prepare("SELECT id,project_id,operation_namespace,session_id,expected_version,payload_hash,request_json,preview_json,preview_hash FROM workshop_adoption_previews")?;
    for row in previews.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })? {
        let (id, project, namespace, session, version, payload, raw_request_json, preview, hash) =
            row?;
        if !storage_valid_id(&id)
            || !storage_valid_id(&project)
            || !storage_valid_id(&namespace)
            || !storage_valid_id(&session)
            || version < 0
            || !valid_hash(&payload)
            || !valid_hash(&hash)
        {
            return Err(CoreError::new(
                "InvalidBackup",
                "A workshop preview has invalid identity or hash metadata.",
            ));
        }
        let request: PreviewWorkshopAdoption =
            serde_json::from_str(&raw_request_json).map_err(|_| {
                CoreError::new("InvalidBackup", "A workshop preview request is invalid.")
            })?;
        let parsed: WorkshopAdoptionPreview = serde_json::from_str(&preview)
            .map_err(|_| CoreError::new("InvalidBackup", "A workshop preview is invalid."))?;
        validate_preview_relationships(&request, &parsed)
            .map_err(|error| CoreError::new("InvalidBackup", &error.detail))?;
        let canonical_request =
            serde_json::to_string(&crate::canonicalize_value(serde_json::to_value(&request)?))?;
        let mut canonical_targets = request.targets.clone();
        for target in &mut canonical_targets {
            target.body = canonical_body(&target.body)
                .map_err(|_| CoreError::new("InvalidBackup", "A workshop target body is invalid."))?
                .0;
        }
        let canonical =
            serde_json::to_string(&crate::canonicalize_value(serde_json::to_value(&parsed)?))?;
        if parsed.id != id
            || parsed.session_id != session
            || parsed.expected_version != version.to_string()
            || request.session_id != session
            || request.access.project_id != project
            || request.access.operation_namespace != namespace
            || request.expected_version != version.to_string()
            || request.candidate_ids != parsed.candidate_ids
            || request.rationale != parsed.rationale
            || request.protected_text != parsed.protected_text
            || canonical_targets != parsed.targets
            || canonical_request != raw_request_json
            || sha256_hex(raw_request_json.as_bytes()) != payload
            || sha256_hex(canonical.as_bytes()) != hash
        {
            return Err(CoreError::new(
                "InvalidBackup",
                "A workshop preview provenance or hash is invalid.",
            ));
        }
    }
    let mut receipts = connection.prepare("SELECT operation_namespace,operation_id,operation_kind,payload_hash,result_json FROM workshop_receipts")?;
    for row in receipts.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })? {
        let (namespace, operation, kind, payload, result) = row?;
        if !storage_valid_id(&namespace)
            || !storage_valid_id(&operation)
            || !valid_hash(&payload)
            || !["startWorkshop", "saveWorkshop", "adoptWorkshop"].contains(&kind.as_str())
        {
            return Err(CoreError::new(
                "InvalidBackup",
                "A workshop receipt has invalid operation metadata.",
            ));
        }
        if kind == "startWorkshop" {
            let started: crate::projects::discussions::DiscussionStart =
                serde_json::from_str(&result).map_err(|_| {
                    CoreError::new("InvalidBackup", "A workshop start receipt is invalid.")
                })?;
            if started.run.owner.operation_namespace != namespace
                || started.run.operation_id != operation
                || started.run.intent
                    != crate::projects::discussions::FeedbackIntent::WorkshopExplore
            {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop start receipt has invalid discussion provenance.",
                ));
            }
            let packet = crate::projects::context_packets::validated_packet_record(
                connection,
                &started.run.packet_id,
            )?;
            if packet.receipt.packet_id != started.run.packet_id
                || packet.options.provider_binding != started.run.provider_binding
            {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop start receipt does not match its immutable packet.",
                ));
            }
        } else if kind == "saveWorkshop" {
            let receipt_snapshot: WorkshopSnapshot =
                serde_json::from_str(&result).map_err(|_| {
                    CoreError::new("InvalidBackup", "A workshop save receipt is invalid.")
                })?;
            let stored_snapshot: Option<(i64, String, String)> = connection
                .query_row(
                    "SELECT version,state_json,state_hash FROM workshop_snapshots WHERE operation_namespace=? AND operation_id=?",
                    params![namespace, operation],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            if let Some((version, state_json, state_hash)) = stored_snapshot {
                let persisted = workshop_snapshot(version, parse_state(&state_json, &state_hash)?)?;
                if persisted != receipt_snapshot {
                    return Err(CoreError::new(
                        "InvalidBackup",
                        "A workshop save receipt does not match its immutable snapshot.",
                    ));
                }
            }
        } else {
            let receipt_ack: WorkshopAdoptionAck = serde_json::from_str(&result).map_err(|_| {
                CoreError::new("InvalidBackup", "A workshop adoption receipt is invalid.")
            })?;
            let stored_snapshot: Option<(i64, String, String)> = connection
                .query_row(
                    "SELECT version,state_json,state_hash FROM workshop_snapshots WHERE operation_namespace=? AND operation_id=?",
                    params![namespace, operation],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((version, state_json, state_hash)) = stored_snapshot else {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop adoption receipt has no immutable snapshot.",
                ));
            };
            let persisted = workshop_snapshot(version, parse_state(&state_json, &state_hash)?)?;
            if persisted != receipt_ack.snapshot {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop adoption receipt does not match its immutable snapshot.",
                ));
            }
            if receipt_ack.decision_ids.iter().any(|id| {
                !persisted
                    .state
                    .decisions
                    .iter()
                    .any(|decision| &decision.id == id)
            }) {
                return Err(CoreError::new(
                    "InvalidBackup",
                    "A workshop adoption receipt names a decision absent from its snapshot.",
                ));
            }
            for document in &receipt_ack.documents {
                let Some(revision_id) = document.last_checkpoint_id.as_deref() else {
                    return Err(CoreError::new(
                        "InvalidBackup",
                        "A workshop adoption receipt contains a document without a revision.",
                    ));
                };
                let revision = read_revision(connection, revision_id).map_err(|_| {
                    CoreError::new("InvalidBackup", "An adoption document revision is invalid.")
                })?;
                if revision.head != document.head || revision.body != document.body {
                    return Err(CoreError::new(
                        "InvalidBackup",
                        "An adoption document does not match its immutable revision.",
                    ));
                }
            }
        }
    }
    Ok(())
}
