//! webnovel-core — the domain crate.
//!
//! **Decomposition in progress.** This crate is being split into the layered
//! workspace described in [`docs/V3_ARCHITECTURE_MODULAR.md`]. The foundation
//! layer has been extracted as `wns-kernel`; the modules below are still here
//! and are scheduled for extraction in the order that document records.
//!
//! Nothing about this file's public surface is new: every item re-exported below
//! was reachable at this same path before the extraction, which is what keeps
//! the 77 registered integration suites compiling untouched.

// L0 — extracted to `wns-kernel`. Re-exported at the crate root so that
// `crate::CoreError`, `crate::sha256_hex` and `crate::validate_snapshot_json`
// keep resolving for every existing caller.
pub use wns_kernel::{
    CoreError, CoreResult, Head, SnapshotReceipt, canonicalize_value, sha256_hex,
    validate_snapshot_json,
};

pub mod context;
pub mod documents;
pub mod library;
pub mod projects;
pub mod providers;
// L1 — extracted to `wns-storage`, which now depends only on `wns-kernel`.
// Aliased at this path so every existing `crate::storage::{configure, migrate,
// LATEST_SCHEMA_VERSION}` reference resolves unchanged.
pub use wns_storage as storage;
pub mod transfer;
pub mod v2_import;
