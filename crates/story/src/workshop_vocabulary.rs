//! Workshop vocabulary the packet metadata graph reaches for.
//!
//! Moved down from `webnovel-core::projects::workshop.rs` (4,135 lines, bound
//! for `wns-workshop` at L5). `context_packets` (L4) validates a workshop
//! instruction's metadata while preparing a packet, so `WorkshopPacketMetadata`
//! and everything it holds has to sit below both.
//!
//! Fifteen types, and the count is the point. Each was found by following a
//! field: `WorkshopPacketMetadata` holds `Vec<StoryPossibility>`, which holds
//! `StoryPossibilityKind`; `WorkshopQuestion` holds `WorkshopQuestionStatus` and
//! `UnknownTo`; `WorkshopPreference` holds three more enums. Every one of them
//! is five or six variants, so every one looked like nothing on its own. The
//! union is sixteen of the forty-three types `workshop.rs` declares.
//!
//! `workshop.rs` re-exports all fifteen at their historical paths.

use serde::{Deserialize, Serialize};
use wns_kernel::Head;

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
pub enum WorkshopRelationshipStatus {
    Tentative,
    Chosen,
    Archived,
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
pub struct WorkshopQuestion {
    pub id: String,
    pub text: String,
    pub reason: String,
    pub status: WorkshopQuestionStatus,
    pub unknown_to: UnknownTo,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoryPossibilityKind {
    UnresolvedQuestion,
    IntendedPayoff,
    PossibleArc,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoryPossibilityStatus {
    Open,
    Archived,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryPossibility {
    pub id: String,
    pub kind: StoryPossibilityKind,
    pub text: String,
    pub status: StoryPossibilityStatus,
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

// ---- The workshop state record set -------------------------------------
//
// These describe the shape of a saved Workshop snapshot, which two crates
// need to name and neither owns alone: `wns-workshop` (L5) persists them, and
// `wns-conversation` (L5) validates a chat-origin snapshot against them. A
// sibling may not reach sideways, so the vocabulary sits below both. The
// relationship and preference types that were already here are the precedent;
// this completes the extraction it started.
//
// Re-exported from `wns_workshop::workshop`, which is where every existing
// path — core, the desktop app, and six integration-test files — names them.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkshopBranchKind {
    Working,
    WhatIf,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub story_possibilities: Vec<StoryPossibility>,
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

pub struct WorkshopSnapshotOrigin<'a> {
    pub project_id: &'a str,
    pub namespace: &'a str,
    pub operation: &'a str,
    pub version: i64,
    pub payload_hash: &'a str,
}
