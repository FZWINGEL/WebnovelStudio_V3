//! L2 — canonical document and proposal-scope contracts, and document lifecycle.
//!
//! W1's scope validator lives here so it can be used by the durable core without
//! importing Tauri, ProseMirror, or a provider runtime. The W0 snapshot validator
//! it builds on lives one layer down, in `wns-kernel`.
//!
//! Extracted from `webnovel-core`, where document semantics and database schema
//! evolution changed for unrelated reasons in the same crate.
//!
//! Raised from L1 to L2 when [`history`] moved here. Revision history reads and
//! writes document rows, so it depends on `wns-storage`, which owns the schema;
//! the edge points strictly downward, so the layering rule is unchanged.

pub mod history;
pub mod scope;
pub mod structured;

pub use scope::{
    Endpoint, ScopeGrant, ScopeKind, ScopeReceipt, ScopeValidationError, ScopeValidationRequest,
    StructuralToken, capture_append_scope, capture_scope, structural_token_iter, structural_tokens,
    validate_append, validate_scope, validate_scope_json, validate_text_replacement,
};
pub use structured::{
    MAX_STRUCTURED_BLOCKS, MAX_STRUCTURED_EXPLANATION_BYTES, MAX_STRUCTURED_UTF16_UNITS,
    STRUCTURED_PROPOSAL_RESPONSE_CONTRACT, TypedReplacementBlock, TypedReplacementHeadingAttrs,
    TypedReplacementInline, TypedReplacementLinkAttrs, TypedReplacementMark,
    typed_replacement_snapshot, validate_structured_replacement, validate_typed_replacement_blocks,
};
