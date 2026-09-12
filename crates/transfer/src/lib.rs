//! L6 — portability: backup, recovery, export and migration.
//!
//! # Why this boundary
//!
//! Every path here moves bytes that must stay byte-identical: a backup manifest,
//! a recovered project's fresh identity, an immutable export record, a V2
//! import's inert legacy evidence, a click-time recovery copy.
//!
//! # Why L6 and not L5
//!
//! The charter this crate was declared with said "L5, may depend on L0–L3".
//! That was aspirational. A transfer validator calls *every* concern's own
//! storage validator — `reviewed_story`, `story_context`, `context_packets`,
//! `memory`, `history`, `discussions`, `workshop`, `project_chat`,
//! `discussion_lookup`, `guidance`, `proposals` — so it is a pure CONSUMER of
//! the layers below it and belongs above all of them, not beside them. Nothing
//! depends on `wns-transfer` except the app shell, which the layer table does
//! not register, so the raise costs no other crate a renumber.
//!
//! # What lands here
//!
//! * `transfer.rs` — backup, recover, duplicate, recovery copy, draft export
//! * `projects/exports.rs` — the export record's durable metadata
//! * `host.rs` — the seam: [`TransferSource`] reads a live project,
//!   [`TransferFactory`] creates the recovered one
//!
//! `projects/import.rs` (the V2 installer) follows once it has a seam for
//! constructing a project; `v2_import.rs` is already below this crate, in
//! `wns-library`.

pub mod exports;
pub mod host;
pub mod transfer;

pub use host::{TransferFactory, TransferSource};
