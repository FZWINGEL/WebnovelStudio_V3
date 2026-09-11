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
pub mod records;
pub mod material_adoption;
pub mod scope;
pub mod structured;

/// A new, empty W0 document.
///
/// Moved down from `webnovel-core::projects`, where it sat beside the actor
/// despite being nothing but document vocabulary: one paragraph with a fresh
/// id. `webnovel-core` re-exports it, so `webnovel_core::projects::blank_document`
/// and the integration tests that call it are unchanged.
pub fn blank_document() -> serde_json::Value {
    serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":wns_kernel::new_id()}}]}})
}

pub use records::{
    CheckpointReason, CheckpointRequest, OperationReceipt, ReconcileRequest, ReconciledDocument,
    SaveAck, SaveCause, SaveSnapshot,
};

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
