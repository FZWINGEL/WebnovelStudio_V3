//! L2 — deterministic context packet compilation.
//!
//! **Skeleton.** Nothing has been ported yet; this crate declares the boundary
//! and the dependency edge so the layering rule is checked from the first commit.
//!
//! # Why this boundary
//!
//! Packet compilation is deterministic, bounded and byte-exact — it either
//! produces the same packet bytes for the same inputs or it errors. Story
//! semantics fail for entirely different reasons and are asserted by different
//! suites. Separating them is what makes the byte-compatibility guarantee (C2's
//! "actual-packet receipts") testable in isolation.
//!
//! # What lands here
//!
//! From `crates/core/src/`:
//! * `context/` (~9,835 lines across 18 files) — eligibility, lookup, receipts
//! * `context/packet.rs` (~4,149 lines) — **the hardest file in the tree**
//! * `projects/project_chat_context.rs`, `projects/project_chat_output.rs`
//!
//! # The inversion this extraction depends on
//!
//! `context/packet.rs:48-55` reaches into `crate::projects::{project_chat_output,
//! story_context, workshop_generation}`. Until that is inverted — the compiler
//! receiving already-materialised inputs instead of assembling them itself —
//! this layer cannot exist. That inversion is the single highest-value change in
//! the Rust tree, and it is why `packet.rs` must be split before it is moved.
//!
//! # Dependency rule
//!
//! L2. May depend on L0–L1 (`wns-kernel`, `wns-storage`, `wns-documents`,
//! `wns-providers`). Must not depend on `wns-story` or above.
