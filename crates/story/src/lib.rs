//! L3 — story memory and the reviewed-story boundary.
//!
//! **Skeleton.** Nothing has been ported yet; this crate declares the boundary
//! and the dependency edge so the layering rule is checked from the first commit.
//!
//! # Why this boundary
//!
//! This is where the product's spine lives. Documents and immutable revisions are
//! story *authority*; digests, summaries, lookup results, guidance and chat
//! output are explicitly not. Today that separation is a convention spread across
//! ADR 0012–0021, 0030 and 0031 and upheld by review discipline. Making it a
//! crate boundary is what lets the type system hold it instead.
//!
//! # What lands here
//!
//! From `crates/core/src/` (~13,000 lines):
//! * `projects/memory.rs`, `projects/memory/`
//! * `projects/reviewed_story.rs`, `projects/reviewed_summary.rs`
//! * `projects/story_context.rs`, `projects/story_records.rs`
//! * `projects/evidence_queries.rs`
//! * `context/reviewed_*.rs`, `context/*_history.rs`, `context/continuation.rs`,
//!   `context/navigation.rs`
//!
//! # Dependency rule
//!
//! L3. May depend on L0–L2. Must not depend on `wns-conversation`,
//! `wns-workshop` or above.

/// Durable AuthorRoom source preferences: vocabulary and actor-side logic,
/// behind a host trait the actor implements. See the module.
pub mod source_pins;
