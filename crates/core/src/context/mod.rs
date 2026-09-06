//! Source-bound story-context contracts and the pure C0 eligibility kernel.
//!
//! This module deliberately stops at source eligibility. It does not read
//! SQLite, call a provider, install memory, or authorize a manuscript edit.
//! The project owner supplies a frozen [`StorySnapshot`] whose source
//! descriptors have already been resolved against the authoritative project.

pub mod continuation;
mod contracts;
mod eligibility;
pub mod evidence_history;
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

pub use contracts::{
    Audience, BasisKind, BudgetError, BudgetErrorCode, CharacterGrant, ContextPurpose,
    CoverageEntry, CoverageLabel, Disclosure, EligibilityRequest, InformationPolicy,
    MAX_SAFE_BRIEF_BYTES, PacketReceipt, ReviewedBasisManifest, ReviewedBasisMember,
    SafeBriefInput, SafeBriefReceipt, SourceDescriptor, SourceKind, SourceRef, StorySnapshot,
    StoryTime,
};
pub(crate) use eligibility::author_room_structured_revision_allowed;
pub use eligibility::{
    EligibilityError, EligibilityErrorCode, EligibilityReceipt, EligibleSource,
    evaluate_eligibility, evaluate_sources,
};
pub use evidence_history::{
    EvidenceHistory, EvidenceHistoryObservation, EvidenceHistoryUncertainty, query_evidence_history,
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
pub mod conversation;
pub mod guidance;
