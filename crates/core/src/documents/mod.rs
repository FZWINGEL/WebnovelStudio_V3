//! Canonical document and proposal-scope contracts.
//!
//! The W0 snapshot validator remains in the crate root for compatibility with
//! the native spike. W1's scope validator lives here so it can be used by the
//! durable core without importing Tauri, ProseMirror, or a provider runtime.

pub mod scope;

pub use scope::{
    Endpoint, ScopeGrant, ScopeKind, ScopeReceipt, ScopeValidationError, ScopeValidationRequest,
    StructuralToken, capture_scope, structural_token_iter, structural_tokens, validate_scope,
    validate_scope_json,
};
