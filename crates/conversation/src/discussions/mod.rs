//! Durable author-room chapter discussions.
//!
//! A discussion start is one actor-owned transaction: the target is checked,
//! its immutable context revision is frozen, the exact C2 packet is compiled,
//! and the user message plus queued run are inserted before the transaction
//! commits. Provider execution is intentionally outside this module. The
//! output methods below are the small durable boundary a later supervisor can
//! drive with deterministic or live events.
//!
//! # Layout
//!
//! The module was one file; it is now split by responsibility:
//! `types` holds the command vocabulary and size limits, `operations` the
//! actor-facing commands and readers, and `rows` the SQL row/parsing
//! helpers and storage validation.
use crate::{guidance, proposals};
use wns_context::project_chat_output;
use wns_kernel::{
    CoreError, CoreResult, Head, ProjectAccess, ProjectInfo, Reply, check_id, logical_hash, new_id,
    parse_stored_version, parse_version, sha256_hex,
};
use wns_storage::{read_document, read_revision};
pub use wns_story::discussion_vocabulary::{
    DiscussionScopeInput, FeedbackIntent, StartDiscussion, skip_default_feedback_intent,
};
use wns_story::host::StoryHost;
pub use wns_story::run_vocabulary::*;
use wns_story::{context_packets, source_pins, story_context};
// The provider delivery vocabulary moved to `wns-providers::vocabulary` (L1),
// below both this module (bound for `wns-conversation`, L5) and `memory`
// (`wns-story`, L4). Re-exported at the historical path so
// `discussions::ProviderCleanup` and its siblings resolve unchanged for
// `discussion_lookup` and `memory`.
use crate::discussion_lookup;
use wns_context::continuation::CONTINUATION_RESPONSE_CONTRACT;
use wns_context::lookup::LookupAllowance;
use wns_context::packet::{
    CODEX_INPUT_LIMIT_BYTES, CODEX_OUTPUT_LIMIT_BYTES, CompiledPacket, LOOKUP_RESPONSE_CONTRACT,
    PROPOSAL_RESPONSE_CONTRACT, PacketError, PacketRequest, ProviderBinding,
    STRUCTURED_PROPOSAL_RESPONSE_CONTRACT, compile_packet, serialized_input,
};
use wns_context::{Audience, BasisKind, ContextPurpose, InformationPolicy, MAX_SAFE_BRIEF_BYTES};
use wns_documents::{
    ScopeGrant, ScopeKind, ScopeValidationRequest, capture_append_scope, capture_scope,
    validate_scope,
};
pub use wns_providers::vocabulary::{
    HttpDeliverySubmission, HttpProviderUsage, ProviderCleanup, ProviderDeliveryReceipt,
    ProviderOutcomeStatus, ProviderUsage,
};
use wns_story::context_packets::PrepareContext;
use wns_story::story_context::{FreezeReviewedContinuation, FreezeStory, FrozenContext};
// The workshop contract and its parser both live below this module now — the
// contract at L3, the parser in `wns-story` beside the metadata types. Naming
// them here rather than through `workshop_generation` is what removes the
// L5→L5 edge this module used to have with `wns-workshop`.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use wns_context::response_contracts::WORKSHOP_RESPONSE_CONTRACT;
use wns_providers::http_request::prepare_request as prepare_http_request;
use wns_story::workshop_metadata::{metadata_from_instruction, metadata_value};

pub use wns_context::SafeBriefInput;

mod app_server;
mod operations;
pub mod queries;
mod retry;
mod rows;
mod types;

mod lookup;
mod packet;
mod validation;
use lookup::*;
use packet::*;
use validation::*;

pub use operations::*;
pub use rows::*;
pub use types::*;

// These were reachable through `discussions` before the split; a glob import is
// private, so the ones that were `pub` are named here.
pub use lookup::{advance_lookup, claim_lookup_invocation, halt_lookup, settle_lookup_invocation};
pub use validation::validate_start;
