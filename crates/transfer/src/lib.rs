//! L6 — portability: backup, recovery, export and migration.
//!
//! # Why this boundary
//!
//! Transfer validates backups and exports and preserves retained immutable
//! history. Recovery deliberately creates a fresh project identity and rebinds
//! its active namespace/ownership. V2 import translates supported material and
//! retains legacy evidence without granting it current story authority.
//!
//! # Why L6 and not L5
//!
//! The charter this crate was declared with said "L5, may depend on L0–L3".
//! That was aspirational. A transfer validator calls *every* concern's own
//! storage validator — `reviewed_story`, `story_context`, `context_packets`,
//! `memory`, `history`, `discussions`, `workshop`, `project_chat`,
//! `discussion_lookup`, `guidance`, `proposals` — so it is a pure CONSUMER of
//! the layers below it and belongs above all of them, not beside them. Nothing
//! below it depends on `wns-transfer`. The library sits above it and uses its
//! installation workflow. See `docs/ARCHITECTURE.md` for the current layer map.
//!
//! # Modules
//!
//! * `transfer.rs` — backup, recover, duplicate, recovery copy, draft export
//! * `exports.rs` — the export record's durable metadata
//! * `host.rs` — the seam: [`TransferSource`] reads a live project,
//!   [`TransferFactory`] creates the recovered one
//!
//! * `import.rs` — V2 installation through the project factory seam
//! * `v2_import.rs` — V2 archive inspection and preview contracts

pub mod exports;
pub mod host;
pub mod import;
pub mod transfer;
pub mod v2_import;

pub use host::{TransferFactory, TransferSource};
