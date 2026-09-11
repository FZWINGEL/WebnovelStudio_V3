//! L5 — portability: backup, recovery, export and migration.
//!
//! **Skeleton.** Nothing has been ported yet; this crate declares the boundary
//! and the dependency edge so the layering rule is checked from the first commit.
//!
//! # Why this boundary
//!
//! Every path here moves bytes that must stay byte-identical: a backup manifest,
//! a recovered project's fresh identity, an immutable export record, a V2 import's
//! inert legacy evidence, a click-time recovery copy. `transfer.rs` is the second
//! hub in the tree (2,334 lines, 21 references into `projects`), and grouping the
//! whole portability surface is what stops those guarantees being re-derived per
//! entry point.
//!
//! # What lands here
//!
//! From `crates/core/src/`:
//! * `transfer.rs` (~2,334 lines) — backup, recover, duplicate, recovery copy
//! * `v2_import.rs` (~998 lines)
//! * `projects/exports.rs`, `projects/import.rs`, `projects/source_pins.rs`,
//!   `projects/material_adoption.rs`
//!
//! # Dependency rule
//!
//! L5. May depend on L0–L3. Deliberately does **not** depend on L4: exporting a
//! project has no business knowing about a conversation.
