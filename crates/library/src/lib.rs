//! L5 — the project library index and app preferences.
//!
//! **Skeleton.** Nothing has been ported yet; this crate declares the boundary
//! and the dependency edge so the layering rule is checked from the first commit.
//!
//! # Why this boundary
//!
//! This is a second database with its own schema version and its own concurrency
//! discipline — `library.sqlite3`, schema 4, with a `(key, schema_version,
//! revision, value_json, updated_at)` CAS on `revision`. It has nothing in common
//! with a project database except that both are SQLite, and it changes for
//! different reasons (a settings key, not a manuscript).
//!
//! # What lands here
//!
//! From `crates/core/src/`:
//! * `library.rs` (~1,105 lines) — index and preference CAS
//! * `library/codex_transport.rs` — the Codex transport preference, own
//!   `schema_version = 1`
//!
//! Credentials deliberately do **not** live here — they are in the native
//! credential store (`crates/core/src/providers/credentials.rs`).
//!
//! # Dependency rule
//!
//! L5. May depend on L0–L1. No dependency on `wns-story` or `wns-context`.

/// Read-only preview of a verified WebnovelStudio V2 schema-8 database.
///
/// It sits here rather than in `wns-transfer` because both consumers are
/// above it: the library drives the import workflow, and the project-side
/// installer consumes the preview inside its staging boundary.
pub mod v2_import;
