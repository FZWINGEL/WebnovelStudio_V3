//! L1 — the provider port and its adapters.
//!
//! **Skeleton.** Nothing has been ported yet; this crate declares the boundary
//! and the dependency edge so the layering rule is checked from the first commit.
//!
//! # Why this boundary
//!
//! This is the cheapest large extraction in the tree. Recon measured 16,116
//! lines across 26 files with only 7 references into `context` and 2 into
//! `projects` — and `providers/mod.rs:1-5` plus `providers/adapter.rs:1-5`
//! already document independence as an intent that nothing enforces. Extracting
//! it is what proves the extraction pattern at scale before the hard files.
//!
//! # What lands here
//!
//! From `crates/core/src/providers/`:
//! * `adapter.rs` — the port trait
//! * `codex_exec.rs`, `codex_runtime.rs`, `codex_discovery.rs`,
//!   `codex_catalog.rs`, `codex_profile.rs`, `codex_runner.rs` — the Exec route
//! * `codex_app_server/` (~4,737 lines) — the optional persistent transport
//! * `claude_*.rs` (~2,555 lines) — the Claude author integration
//! * `openai_compatible.rs`, `credentials.rs`, `endpoints.rs`,
//!   `preferences.rs`, `catalog.rs` (~2,915 lines) — OpenAI-compatible HTTP
//! * `cli/windows_process.rs` (~2,376 lines) — owned process plumbing
//! * the mock/local test model
//!
//! Nine references must be inverted to make this crate depend on nothing above
//! L0. That inversion is the work of the extraction, not a prerequisite for it.
//!
//! # Dependency rule
//!
//! L1. May depend on `wns-kernel` only.
