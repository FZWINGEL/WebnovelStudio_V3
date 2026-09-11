//! L4 — story memory and the reviewed-story boundary.
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
//! Note that `reviewed_summary.rs` and `story_records.rs` arrived early, in
//! `wns-context`: they are the record shapes a compiled packet *carries*, so the
//! compiler could not stop reaching upward for its own input vocabulary until
//! they sat below it. The two crates are siblings in spirit — reviewed story is
//! the L4 concern, the record shapes it exchanges are L3 vocabulary.
//!
//! # Dependency rule
//!
//! L4. May depend on L0–L3. Must not depend on `wns-conversation`,
//! `wns-workshop` or above.

/// Durable AuthorRoom source preferences: vocabulary and actor-side logic,
/// behind a host trait the actor implements. See the module.
pub mod source_pins;

/// Author-only reviewed prose basis: the reviewed-story boundary itself.
///
/// The third module to move in step 7 and the first that other modules call
/// *into* — `evidence_queries`, `exports`, `story_context` and `transfer` all
/// reach for its functions, which is why it precedes them in the step-7 order.
pub mod context_packets;
pub mod run_vocabulary;
pub mod discussion_vocabulary;
pub mod evidence_queries;
pub mod host;
pub mod memory;
pub mod story_context;
pub mod reviewed_story;
pub mod workshop_metadata;
pub mod workshop_state;
pub mod workshop_vocabulary;
