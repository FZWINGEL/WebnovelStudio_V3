//! L7 — the project library index and app preferences.
//!
//!
//! # Why this boundary
//!
//! This is a second database with its own schema version and its own concurrency
//! discipline — `library.sqlite3`, schema 4, with a `(key, schema_version,
//! revision, value_json, updated_at)` CAS on `revision`. It has nothing in common
//! with a project database except that both are SQLite, and it changes for
//! different reasons (a settings key, not a manuscript).
//!
//! # Modules
//!
//! * `library.rs` — index, project installation workflows and preference CAS
//! * `codex_transport.rs` — the Codex transport preference, own
//!   `schema_version = 1`
//!
//! Credentials deliberately do **not** live here — they are in the native
//! credential store (`wns-providers::credentials`).
//!
//! # Dependency rule
//!
//! L7 — see below. May depend on L0–L6.

/// The index, its preference CAS, and the V2 import workflow.
///
/// Generic over the factory it creates projects with; `webnovel-core` fixes
/// that parameter, because the project type lives there.
pub mod library;

/// The Codex transport preference, schema-versioned separately from the rest.
pub mod codex_transport;
