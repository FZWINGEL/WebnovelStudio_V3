//! L2 — source-bound story-context contracts and the deterministic packet compiler.
//!
//! This crate deliberately stops at source eligibility and packet compilation.
//! It does not read SQLite, call a provider, install memory, or authorize a
//! manuscript edit. The project owner supplies a frozen [`StorySnapshot`] whose
//! source descriptors have already been resolved against the authoritative
//! project.
//!
//! # Why this crate exists
//!
//! Packet compilation is deterministic, bounded and byte-exact: it either
//! produces the same packet bytes for the same inputs or it errors. Story
//! semantics — reviewed prefixes, evidence, promises — fail for entirely
//! different reasons. Separating them is what makes the byte-compatibility
//! guarantee testable in isolation.
//!
//! # The inversion this crate is the product of
//!
//! The compiler used to reach *upward* for its own inputs: `packet.rs` imported
//! response contracts, workshop packet metadata, frozen context types and story
//! records from the crate under decomposition, while that crate imported
//! provider constants back. The fix was never to move files — it was to correct
//! which layer owns the vocabulary. Everything a packet *carries* or *embeds*
//! lives here; the operations that produce it stay above.
//!
//! # Dependency rule
//!
//! L2. May depend on L0–L1 (`wns-kernel`, `wns-storage`, `wns-documents`,
//! `wns-providers`). Must not depend on `wns-story` or above.

pub mod contracts;
pub mod eligibility;

pub mod continuation;
pub mod conversation;
pub mod evidence_history;
pub mod guidance;
pub mod knowledge_history;
pub mod lookup;
pub mod memory;
pub mod memory_lookup;
pub mod navigation;
pub mod packet;
pub mod promise_history;
pub mod reviewed_evidence;
pub mod reviewed_knowledge;
pub mod reviewed_promises;
pub mod reviewed_summaries;

// Vocabulary moved down by the inversion.
pub mod chat_vocabulary;
pub mod frozen;
pub mod response_contracts;
pub mod reviewed_prefix;
pub mod reviewed_summary;
pub mod story_records;

pub use contracts::{
    Audience, BasisKind, BudgetError, BudgetErrorCode, CharacterGrant, ContextPurpose,
    CoverageEntry, CoverageLabel, Disclosure, EligibilityRequest, InformationPolicy,
    MAX_SAFE_BRIEF_BYTES, PacketReceipt, ProjectBriefOrigin, ReviewedBasisManifest,
    ReviewedBasisMember, SafeBriefInput, SafeBriefReceipt, SourceDescriptor, SourceKind, SourceRef,
    StorySnapshot, StoryTime,
};
pub use eligibility::author_room_structured_revision_allowed;
pub use eligibility::{
    EligibilityError, EligibilityErrorCode, EligibilityReceipt, EligibleSource,
    evaluate_eligibility, evaluate_sources,
};
pub use evidence_history::{
    EvidenceHistory, EvidenceHistoryObservation, EvidenceHistoryUncertainty, query_evidence_history,
};
pub use frozen::{
    FrozenContext, SearchHit, SearchMode, SearchResult, SearchStory, SourcePassage, SourceRead,
    search_saved_passages,
};
pub use promise_history::{
    PromiseHistory, PromiseHistoryObservation, PromiseHistoryUncertainty, query_promise_history,
};
pub use reviewed_evidence::{
    ReviewedEvidenceCoverage, ReviewedEvidenceOmission, ReviewedEvidenceOmissionReason,
    ReviewedEvidenceSet, eligible_records,
};
pub use reviewed_promises::{
    ReviewedPromiseCoverage, ReviewedPromiseOmission, ReviewedPromiseOmissionReason,
    ReviewedPromiseSet,
};
