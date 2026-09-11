//! L1 — the document model and the editor contract.
//!
//! **Skeleton.** Nothing has been ported yet; this crate declares the boundary
//! and the dependency edge so the layering rule is checked from the first commit.
//!
//! # Why this boundary
//!
//! Document semantics and schema evolution change for unrelated reasons. Today
//! both live in `webnovel-core`, so a migration added for chat recompiles the
//! document model and vice versa.
//!
//! # What lands here
//!
//! From `crates/core/src/`:
//! * `documents/mod.rs` — document records and roles
//! * `documents/scope.rs` (~1,614 lines) — structural scope validation
//! * `documents/structured.rs` (~376 lines) — typed rich blocks
//!
//! # What deliberately does NOT land here
//!
//! The W0 snapshot validator lives in `wns-kernel`. It is a pure function from a
//! JSON string to a canonical receipt — it never touches a document record, a
//! connection or a project — so it belongs with the canonicalization primitives
//! rather than with the model that consumes them.
//!
//! # Dependency rule
//!
//! L1. May depend on `wns-kernel` only. No sibling edge to `wns-storage` or
//! `wns-providers`.
