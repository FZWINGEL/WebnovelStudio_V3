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
