//! Author-only reviewed prose basis.
//!
//! This slice records which exact chapter revisions an author has marked as
//! ready, with optional author-accepted summaries and passage-backed records.
//! Generated memory is never accepted implicitly. A ready head selects immutable bundles; the
//! bundles and stages remain historical evidence after the selection changes.
//!
//! # Why this file is here and not in `webnovel-core`
//!
//! Step 7 moves each `projects/` module's vocabulary and actor-side logic out of
//! `webnovel-core`, leaving the command plumbing behind, because an inherent
//! `impl` must live in the crate that owns the type. Reviewed story is this
//! crate's namesake concern, so it belongs here.
//!
//! It is the third module to move and the first of the modules that other
//! modules call *into*: `evidence_queries`, `exports`, `story_context` and
//! `transfer` all reach for its functions, which is why it precedes them in the
//! step-7 order.
//!
//! What made it movable is nothing this file changed. Its actor side reaches
//! eight crate-private helpers — `read_document`, `read_revision`,
//! `checkpoint_at`, `existing_receipt`, `insert_receipt`, `valid_hash`,
//! `require_head`, `new_id` — and every one of those had to reach L0 or L1
//! before a single line here could travel. It also has no test hook and no
//! sibling `projects/` call, which makes it the cleanest move so far.
//!
//! # Layout
//!
//! The module was one file; it is now split by responsibility:
//! `types` holds the public vocabulary and stored row shapes, `context` the
//! `ReviewValidationContext`, `operations` the author-facing commands and
//! readers, `rows` the SQL row/parsing helpers, `prefix` the reviewed-prefix
//! selection and batching, and `validators` the storage validation entry points.

use crate::host::StoryHost;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use wns_context::reviewed_summary::{
    SummaryChange, SummaryRevision, canonical_summary_json, summary_hash, validate_summary_binding,
};
use wns_context::story_records::{
    KnowledgeRecord, PossessionRecord, PromiseRecord, canonical_knowledge_json,
    canonical_promises_json, canonical_records_json, validate_knowledge, validate_promises,
    validate_records,
};
use wns_context::{
    ReviewedBasisManifest, ReviewedBasisMember, SourceDescriptor, SourceKind, SourceRef,
};
use wns_kernel::{
    CoreError, CoreResult, DocumentRecord, DocumentRole, Head, ProjectAccess, Reply, Revision,
    SourceEpoch, check_id, logical_hash, new_id, parse_stored_version, parse_version, require_head,
    sha256_hex, valid_hash, validate_title,
};
use wns_storage::{checkpoint_at, read_document, read_revision};

mod context;
mod operations;
mod prefix;
mod rows;
mod types;
mod validators;

pub use context::*;
pub use operations::*;
pub use prefix::*;
pub(crate) use rows::*;
pub use types::*;
pub use validators::*;
