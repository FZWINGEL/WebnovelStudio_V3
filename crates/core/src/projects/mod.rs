//! One locked project, one owned SQLite connection, and explicit renderer leases.
use crate::documents::Endpoint;
use crate::{storage, validate_snapshot_json};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub mod background_work;
pub mod context_packets;
mod conversation_context;
pub mod discussion_lookup;
pub mod discussions;
pub mod evidence_queries;
pub mod exports;
pub mod guidance;
pub mod history;
pub mod import;
pub mod memory;
pub mod project_chat;
pub mod project_chat_output;
pub mod proposals;
pub mod reviewed_story;
// Moved to wns-context (L2): a summary is vocabulary a compiled packet carries.
pub use wns_context::reviewed_summary;
pub mod source_pins;
pub mod story_context;
pub mod workshop;
pub mod workshop_generation;

// Split out of this file so that a crate can eventually be extracted from it.
// Every item is re-exported below, so the 28 `use super::*` globs under
// `projects/` and every `crate::projects::{…}` import resolve as before. This
// file was a namespace rather than a module: it held the record types, the
// actor, the session façade and the persistence helpers together, and none of
// them could move until they were separated.
mod context_api;
mod document_api;
mod install;
mod owned;
mod project_api;
mod records;
mod session;
#[cfg(test)]
mod tests;
mod work_api;
mod workshop_api;
pub(crate) use install::*;
pub(crate) use owned::*;

pub use context_api::*;
pub use document_api::*;
pub use project_api::*;
pub use records::*;
pub use session::*;
pub use work_api::*;
pub use workshop_api::*;

// Moved to wns-kernel (L0) so that wns-storage can be extracted without
// depending on this module — `storage` importing `CoreError` from here is what
// makes today's storage↔projects cycle. Re-exported at this path so every
// existing `webnovel_core::projects::{CoreError, CoreResult, Head}` import,
// including the `use super::*` globs in this crate's own submodules, keeps
// resolving unchanged.
pub use wns_kernel::{
    CoreError, CoreResult, Head, ProjectAccess, Revision, check_id, logical_hash,
    parse_stored_version, parse_version,
};

// The shared primitive layer, moved down so the remaining step-7 modules can
// follow it. Eight functions and three record types had 240 call sites across
// the twelve files under `projects/`, and every module's actor side reached for
// the same ones. While they lived here, a module could not travel to another
// crate: the helpers its bodies call would have to stay behind.
//
// Vocabulary went to L0, row access to L1, and both are re-exported at their
// historical paths so not one of the 240 call sites changed.
pub use wns_kernel::{
    AppliedDecision, DocumentRecord, DocumentRole, ProjectInfo, RestoredDecision, SourceEpoch,
    StoredResult,
};
// Document vocabulary, moved to L2 beside the model it describes. Re-exported
// here so `webnovel_core::projects::blank_document` and the integration tests
// that build fixtures with it are unchanged.
pub use wns_documents::blank_document;
pub(crate) use wns_kernel::{new_id, require_head, validate_title};
pub(crate) use wns_storage::{
    checkpoint_at, existing_receipt, insert_receipt, read_document, read_revision,
};

// L2 vocabulary that the packet compiler consumes. `story_records` is the set of
// shapes a compiled packet carries, so it lives at or below the compiler rather
// than above it — a layer-2 crate may not reach up into the crate under
// decomposition for its own input types. Re-exported here so the 28 `use
// super::*` globs and every `crate::projects::story_records::{…}` import keep
// resolving.
pub use wns_context::story_records;

#[cfg(test)]
fn hold_context_after_commit_before_ack(operation_id: &str) {
    tests::hold_after_commit_before_ack(operation_id);
}

// The view state and its read/validate helpers moved to `wns-documents` (L2).
// `transfer` validates a stored view state before accepting a backup, and
// cannot reach it through the crate being decomposed.
pub(crate) use wns_documents::{read_view_state, validate_endpoint};
