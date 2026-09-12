//! L4 — story memory and the reviewed-story boundary.
//!
//! This is where the product's spine lives. Documents and immutable revisions are
//! story *authority*; digests, summaries, lookup results, guidance and chat
//! output are explicitly not. Typed vocabulary, source identities, stored
//! fingerprints and domain validators enforce parts of that separation. The
//! crate boundary does not by itself prevent generated content being presented
//! as authority. See `docs/ARCHITECTURE.md` for the enforcement matrix.
//!
//! # Responsibilities
//!
//! * `memory.rs`, `memory/` — memory lifecycle and durable results
//! * `reviewed_story.rs`, `evidence_queries.rs` — reviewed authority and queries
//! * `story_context.rs`, `context_packets.rs` — source freeze and packet storage
//! * run, discussion and Workshop vocabulary shared by the L5 consumers
//!
//! Packet-carried reviewed records, navigation and history projections live in
//! `wns-context` at L3. Story orchestration consumes that vocabulary from L4.
//!
//! # Dependency rule
//!
//! L4. May depend on L0–L3. Must not depend on `wns-conversation`,
//! `wns-workshop` or above.

/// Durable AuthorRoom source preferences: vocabulary and actor-side logic,
/// behind a host trait the actor implements. See the module.
pub mod source_pins;

/// Author-only reviewed prose basis: the reviewed-story boundary itself.
///
/// Packet persistence validates its immutable source and receipt identities.
pub mod context_packets;
pub mod discussion_vocabulary;
pub mod evidence_queries;
pub mod host;
pub mod memory;
pub mod reviewed_story;
pub mod run_vocabulary;
pub mod story_context;
pub mod workshop_metadata;
pub mod workshop_state;
pub mod workshop_vocabulary;
