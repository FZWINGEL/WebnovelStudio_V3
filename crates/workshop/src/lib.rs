//! L5 — the six-lens Story Workshop.
//!
//! Owns Workshop state, generation contracts, candidate interpretation,
//! adoption previews and atomic adoption. Core supplies the project host and
//! conversation-owned run readers. No sibling dependency is required.
//!
//! # Why this boundary
//!
//! The Workshop is a self-contained product surface with its own vocabulary
//! (lenses, candidates, alternatives, scoped preferences, decisions, what-if and
//! noncanon moments) and its own generation orchestration. It shares the
//! discussion run machinery but not the discussion domain.
//!
//! # Modules
//!
//! * `workshop.rs` and `workshop/` — persistence, provenance and adoption
//! * `workshop_generation.rs` — request and output contracts
//!
//! # Conversation seam
//!
//! Shared run vocabulary lives in `wns-story`. Conversation owns run queries;
//! Workshop supplies the active transaction connection when reading candidate
//! outputs. Host access still includes raw SQLite for Workshop transactions:
//! see `docs/ARCHITECTURE.md` for the boundary and its enforcement limits.
//!
//! # Dependency rule
//!
//! L5. May depend on L0–L4, never the sibling `wns-conversation`.

/// What the Workshop needs from the actor.
pub mod host;

/// Durable Story Workshop state and reviewed adoption boundary.
pub mod workshop;

/// Story Workshop generation contracts.
pub mod workshop_generation;
