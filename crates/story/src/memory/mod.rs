//! Durable, source-bound chapter navigation memory.
//!
//! Memory refresh is deliberately a small actor-owned lifecycle.  Starting a
//! job freezes one saved chapter and its exact packet in one transaction;
//! dispatch and terminal completion are separate transactions; installation
//! is a short, independently stale-checked transaction.  Generated text is
//! retained as an unreviewed aid and never becomes canon or manuscript text.
//!
//! # Layout
//!
//! The module was one file; it is now split by responsibility:
//! `types` holds the command vocabulary and stored row shapes, `operations`
//! the actor-facing commands and readers, `rows` the SQL row/parsing helpers,
//! and `validation` the view reads and storage validation entry points.

use crate::context_packets;
use crate::context_packets::{PrepareContext, validated_packet_record};
use crate::host::StoryHost;
use crate::story_context;
use wns_context::memory::{DigestCandidate, MAX_RAW_BYTES, validate_navigation_digest};
use wns_context::navigation::navigation_content_hash;
use wns_context::packet::{
    CompiledPacket, MEMORY_RESPONSE_CONTRACT, MockContextBudget, PacketError, PacketRequest,
    ProviderBinding, compile_packet, serialized_input,
};
use wns_context::{Audience, BasisKind, ContextPurpose, InformationPolicy, SourceRef};
use wns_kernel::{
    CoreError, CoreResult, Head, ProjectAccess, ProjectInfo, Reply, SourceEpoch, check_id, new_id,
    parse_stored_version, parse_version, sha256_hex,
};
use wns_storage::read_document;
// Named at the crate that owns them, not through `discussions`' re-export:
// this import is the whole of the edge that made `memory` (L4) depend on
// `discussions` (L5), and it stops being an edge only when it points below.
use wns_providers::vocabulary::{
    HttpDeliverySubmission, ProviderCleanup, ProviderDeliveryReceipt, ProviderOutcomeStatus,
    ProviderUsage,
};

use crate::story_context::{
    FreezeStory, FrozenContext, SourceRead, read_source, validated_snapshot_record,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use wns_providers::codex_app_server::{AppServerDelivery, AppServerDispatch};
use wns_providers::http_request::prepare_request as prepare_http_request;

pub mod app_server;

mod operations;
mod rows;
mod types;
mod validation;

pub use operations::*;
pub(crate) use rows::*;
pub use types::*;
pub use validation::*;
