//! L4 — the six-lens Story Workshop.
//!
//! **Skeleton.** Nothing has been ported yet; this crate declares the boundary
//! and the dependency edge so the layering rule is checked from the first commit.
//!
//! # Why this boundary
//!
//! The Workshop is a self-contained product surface with its own vocabulary
//! (lenses, candidates, alternatives, scoped preferences, decisions, what-if and
//! noncanon moments) and its own generation orchestration. It shares the
//! discussion run machinery but not the discussion domain.
//!
//! # What lands here
//!
//! From `crates/core/src/` (~6,030 lines):
//! * `projects/workshop.rs` (~4,135 lines) — six-lens domain logic, persistence
//!   and generation orchestration in one file; the three separate on extraction
//! * `projects/workshop_generation.rs` (~1,895 lines)
//!
//! # The edge that must be inverted first
//!
//! `projects/workshop.rs:9` imports `crate::projects::discussions::DiscussionRun`.
//! Two 4,000-line modules coupled by one type. Until the shared run/settlement
//! type moves down to `wns-story` (or a small shared type at a lower layer),
//! `workshop` and `conversation` cannot be siblings — this crate would need a
//! sibling edge, and the layering rule forbids that deliberately: a boundary that
//! needs a sibling edge is the wrong boundary, and the rule is meant to say so.
//!
//! # Dependency rule
//!
//! L4. May depend on L0–L3.

/// What the Workshop needs from the actor.
pub mod host;
