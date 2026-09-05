use serde::{Deserialize, Serialize};

/// The editorial basis selected for a frozen request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum BasisKind {
    Working,
    Reviewed,
    ExplicitHistory,
}

/// Whether the packet serves an author-room discussion or a restricted prose
/// request. Author-room knowledge is never an Apply authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Audience {
    AuthorRoom,
    RestrictedWriting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum ContextPurpose {
    Discuss,
    Revise,
    Continue,
    Plan,
    StoryQuestion,
}

/// The authority/provenance class resolved by the project owner. These are
/// intentionally distinct: an observation or digest can assist retrieval but
/// cannot become reviewed story authority merely by entering a packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum SourceKind {
    CurrentDraft,
    ReviewedAuthority,
    ExplicitRule,
    AdoptedGuidance,
    GeneratedObservation,
    GeneratedDigest,
    PlanAlternative,
    Historical,
    PrivateFuture,
    AuthorRoomDiscussion,
}

/// Coverage describes how a source may be represented in a later packet. A
/// directory entry is navigable metadata, not semantic story evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum CoverageLabel {
    Verbatim,
    Digest,
    DirectoryOnly,
}

/// A policy boundary for reader and character disclosure. All positions are
/// decimal strings because they cross the JavaScript boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InformationPolicy {
    /// Canonical decimal project disclosure-policy epoch, not a display label.
    pub version: String,
    pub audience: Audience,
    pub reader_frontier: Option<String>,
    pub character_id: Option<String>,
    pub character_grants: Vec<CharacterGrant>,
    pub allow_alternatives: bool,
    pub allow_historical: bool,
}

/// An explicit grant that a limited-POV character may use a source already
/// disclosed to the reader at or before the stated frontier. It never bypasses
/// the reader frontier and never uses story time as a disclosure shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterGrant {
    pub character_id: String,
    pub source_handle: String,
    pub reader_frontier: String,
}

/// Exact identity of one source revision. Display names and labels live on the
/// resolved descriptor, so Unicode source names remain lossless here.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRef {
    pub project_id: String,
    pub document_id: String,
    pub revision_id: String,
    pub body_hash: String,
}

/// Reader-disclosure metadata is separate from optional fictional story time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Disclosure {
    pub reader_position: Option<String>,
    pub visible_to_characters: Vec<String>,
    pub author_only: bool,
    pub future_private: bool,
}

/// Optional fictional chronology. Eligibility never uses it to override the
/// reader disclosure frontier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryTime {
    pub label: String,
    pub position: Option<String>,
}

/// A source descriptor is produced by the Rust project/source resolver. The
/// eligibility kernel does not accept a client-side "safe" or "reviewed"
/// claim; it checks this descriptor against the frozen snapshot and all of its
/// exact source dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceDescriptor {
    pub handle: String,
    pub source: SourceRef,
    pub display_name: String,
    pub kind: SourceKind,
    pub current: bool,
    pub coverage: CoverageLabel,
    pub disclosure: Disclosure,
    pub story_time: Option<StoryTime>,
    /// Every influential input belongs here, including sources not shown as a
    /// citation in the final prose. The kernel walks all of them.
    pub dependencies: Vec<SourceRef>,
}

/// Immutable source and policy basis for one context request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorySnapshot {
    pub snapshot_id: String,
    pub project_id: String,
    pub basis: BasisKind,
    pub target: SourceRef,
    pub context_source_epoch: String,
    pub ordering_epoch: String,
    pub disclosure_policy_version: String,
    /// These descriptors are Rust-resolved and frozen with the snapshot.
    pub sources: Vec<SourceDescriptor>,
}

/// Inputs to the pure eligibility kernel. The snapshot and policy are
/// resolved by the Rust project owner before this request is constructed;
/// callers cannot use this type to assert that an arbitrary client source is
/// current, reviewed, or disclosed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EligibilityRequest {
    pub snapshot: StorySnapshot,
    pub policy: InformationPolicy,
    pub purpose: ContextPurpose,
    pub requested_handles: Vec<String>,
}

/// A packet coverage item records what representation was actually delivered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoverageEntry {
    pub handle: String,
    pub label: String,
    pub detail: CoverageLabel,
}

/// Exact durable receipt contract for a compiled packet. C2 owns packet
/// construction; C0 defines the fields that must remain auditable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PacketReceipt {
    pub packet_id: String,
    pub session_id: String,
    pub snapshot_id: String,
    pub invocation_ordinal: String,
    pub source_handles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guidance_handles: Vec<String>,
    pub coverage: Vec<CoverageEntry>,
    pub omissions: Vec<String>,
    pub input_hash: String,
    pub input_tokens: String,
    pub token_accounting_method: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum BudgetErrorCode {
    MandatoryContextTooLarge,
    BudgetExhausted,
    InvalidBudget,
}

/// Structured budget failures preserve decimal counters and explicit gaps;
/// callers must not silently shorten a mandatory target into a different task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BudgetError {
    pub code: BudgetErrorCode,
    pub message: String,
    pub required_input_tokens: String,
    pub available_input_tokens: String,
    pub mandatory_handles: Vec<String>,
}
