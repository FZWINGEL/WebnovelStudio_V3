//! webnovel-core — project actor ownership and compatibility facades.
//!
//! Domain implementation lives in the layered workspace. Core owns the project
//! actor, command ordering, live access and host implementations that connect
//! sibling domains without a sibling dependency. Re-exports preserve existing
//! caller paths. `docs/ARCHITECTURE.md` describes the current boundaries;
//! `docs/V3_ARCHITECTURE_MODULAR.md` retains the extraction history.
//!
//! The actor and its per-concern facades share the same command queue. Splitting
//! a source package does not create another writer or transaction coordinator.

// L0 — extracted to `wns-kernel`. Re-exported at the crate root so that
// `crate::CoreError`, `crate::sha256_hex` and `crate::validate_snapshot_json`
// keep resolving for every existing caller.
pub use wns_kernel::{
    CoreError, CoreResult, Head, SnapshotReceipt, canonicalize_value, sha256_hex,
    validate_snapshot_json,
};

// L3 — extracted to `wns-context`. Re-exported at this path so every existing
// `crate::context::*` reference resolves unchanged.
pub use wns_context as context;
// L2 — extracted to `wns-documents`. Aliased at this path so every existing
// `crate::documents::*` reference resolves unchanged.
pub use wns_documents as documents;
pub mod library;
pub mod projects;
// L1 — extracted to `wns-providers`. Aliased at this path so every existing
// `crate::providers::*` reference resolves unchanged.
pub use wns_providers as providers;
// L1 — extracted to `wns-storage`, which now depends only on `wns-kernel`.
// Aliased at this path so every existing `crate::storage::{configure, migrate,
// LATEST_SCHEMA_VERSION}` reference resolves unchanged.
pub use wns_storage as storage;
pub mod transfer;
pub mod v2_import;
