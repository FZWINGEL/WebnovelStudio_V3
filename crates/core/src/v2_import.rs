//! Read-only preview of a verified WebnovelStudio V2 schema-8 database.
//!
//! Moved to `wns-transfer` (L6): the V2 installer consumes the preview inside
//! its staging boundary, and the library drives the workflow above it. This
//! is the re-export at
//! the historical path, so `webnovel_core::v2_import::{…}`, the benchmark
//! example, `v2_import_commands`, and the integration tests are unchanged.

pub use wns_transfer::v2_import::*;
