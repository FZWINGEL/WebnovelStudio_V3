//! L5 — discussions and the project conversation.
//!
//! **Skeleton.** Nothing has been ported yet; this crate declares the boundary
//! and the dependency edge so the layering rule is checked from the first commit.
//!
//! # Why this boundary
//!
//! Every AI-produces-a-change path converges here. "Explicit Apply only" — an
//! atomic commit carrying a receipt, before/after revisions and a source-epoch
//! advance — is currently upheld at each call site separately. Once this is a
//! crate, the adoption transaction becomes the *only* thing that can write a
//! document, and the other paths become unrepresentable rather than merely
//! discouraged.
//!
//! # What lands here
//!
//! From `crates/core/src/` (~18,000 lines):
//! * `projects/discussions.rs` (~4,605 lines) — the four-concern file: run
//!   lifecycle and settlement, recovery/lost-acknowledgment, lookup invocation,
//!   and packet-assembly glue. The glue moves *down* to `wns-context`.
//! * `projects/discussion_lookup.rs`, `projects/discussions/`
//! * `projects/project_chat.rs` and `projects/project_chat/` (8 files)
//! * `projects/proposals.rs` (~1,708 lines), `projects/guidance.rs`
//! * the single `Adoption` entry point (§3 of the architecture document)
//!
//! # Dependency rule
//!
//! L5. May depend on L0–L4. Must not depend on `wns-workshop` — these are
//! siblings, and the `workshop.rs:9` edge into `discussions` must be inverted
//! before either can move.

// `project_chat_output` lived here briefly and moved on to `wns-context` (L3).
// Its lowest consumer decides its layer: `collect_project_chat_dispositions`,
// which had to reach L3 for the frozen-snapshot split, parses assistant output
// through it. `wns-conversation` reaches down for it, and so does every other
// caller — `discussions` and the seven files under `project_chat/` — through
// `webnovel-core`'s shim, which now points at `wns-context`.

/// Durable author guidance: the authoring half, behind `StoryHost`.
///
/// The frozen half it used to share a file with sits at L3 in `wns-context`,
/// which is what discharged the cycle this module had with `story_context`.
pub mod guidance;
