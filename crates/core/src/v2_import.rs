//! Read-only preview of a verified WebnovelStudio V2 schema-8 database.
//!
//! Moved to `wns-library` (L5): the library drives the V2 import workflow and
//! reads the preview itself, and `projects::import` — which stays in the
//! legacy crate for now — reaches it the same way. This is the re-export at
//! the historical path, so `webnovel_core::v2_import::{…}`, the benchmark
//! example, `v2_import_commands`, and the integration tests are unchanged.

pub use wns_library::v2_import::*;
