//! Source-bound story-context contracts and the pure C0 eligibility kernel.
//!
//! This module deliberately stops at source eligibility. It does not read
//! SQLite, call a provider, install memory, or authorize a manuscript edit.
//! The project owner supplies a frozen [`StorySnapshot`] whose source
//! descriptors have already been resolved against the authoritative project.

mod contracts;
mod eligibility;
pub mod memory;
pub mod navigation;
pub mod packet;

pub use contracts::{
    Audience, BasisKind, BudgetError, BudgetErrorCode, CharacterGrant, ContextPurpose,
    CoverageEntry, CoverageLabel, Disclosure, EligibilityRequest, InformationPolicy,
    MAX_SAFE_BRIEF_BYTES, PacketReceipt, ReviewedBasisManifest, ReviewedBasisMember,
    SafeBriefInput, SafeBriefReceipt, SourceDescriptor, SourceKind, SourceRef, StorySnapshot,
    StoryTime,
};
pub use eligibility::{
    EligibilityError, EligibilityErrorCode, EligibilityReceipt, EligibleSource,
    evaluate_eligibility, evaluate_sources,
};
pub mod conversation;
pub mod guidance;
