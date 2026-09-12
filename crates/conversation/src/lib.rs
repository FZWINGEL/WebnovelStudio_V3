//! L5 — discussions and the project conversation.
//!
//! Owns discussion lifecycle and queries, project-chat operations, proposal
//! decisions, author guidance and the background-work census. Core supplies
//! actor access through host traits; it retains the single project queue.
//!
//! # Why this boundary
//!
//! Conversation and Workshop have separate author-approved adoption workflows.
//! Both retain their own atomic receipts and projections, using shared document
//! mutation helpers where appropriate. There is no universal adoption capability
//! that makes every unauthorized write unrepresentable. Runtime validation and
//! integration tests enforce these workflows; see `docs/ARCHITECTURE.md`.
//!
//! # Modules
//!
//! * `discussions.rs` — run
//!   lifecycle and settlement, recovery/lost-acknowledgment, lookup invocation,
//!   and packet-assembly glue over the deterministic context compiler
//! * `discussion_lookup.rs`, `discussions/`
//! * `project_chat.rs` and `project_chat/`
//! * `proposals.rs`, `guidance.rs`, `background_work.rs`
//!
//! # Dependency rule
//!
//! L5. May depend on L0–L4. Must not depend on `wns-workshop` — these are
//! siblings. Their shared vocabulary sits below both; core connects their hosts.

// `project_chat_output` lived here briefly and moved on to `wns-context` (L3).
// Its lowest consumer decides its layer: `collect_project_chat_dispositions`,
// which had to reach L3 for the frozen-snapshot split, parses assistant output
// through it. `wns-conversation` reaches down for it, and so does every other
// caller — `discussions` and the seven files under `project_chat/` — through
// `webnovel-core`'s shim, which now points at `wns-context`.

pub mod host;

/// Durable author guidance: the authoring half, behind `StoryHost`.
///
/// The frozen half it used to share a file with sits at L3 in `wns-context`,
/// which is what discharged the cycle this module had with `story_context`.
pub mod guidance;

/// Project-chat context ownership and the freeze dispatch it wraps.
pub mod project_chat_context;

/// Durable state for the bounded, request-scoped discussion lookup loop.
pub mod discussion_lookup;

/// Explicit review and atomic author decisions over immutable source passages.
pub mod proposals;

/// Discussion run lifecycle and settlement, recovery, and lookup invocation.
///
/// The four-concern file the architecture document names: run lifecycle and
/// settlement, recovery/lost-acknowledgment, lookup invocation, and
/// packet-assembly glue. `app_server` and `retry` are its submodules.
pub mod discussions;

/// The project conversation projection: references, composer CAS and decisions.
pub mod project_chat;

/// The active-work census, and the stop/interrupt intents over it.
pub mod background_work;
