//! Review and adoption of project-chat material.
//!
//! A chat response is never a story write.  The response is first materialised
//! as an isolated assistant draft; this module turns an explicitly reviewed,
//! exact draft into an immutable preview and, later, into one atomic Working
//! adoption.  The preview item stores references only.  Bodies are recovered
//! from immutable revisions so the conversation ledger cannot become a second
//! document store.
//!
//! # Layout
//!
//! The module was one file; it is now split by responsibility:
//! `types` holds the group-origin record, `stored` the persisted preview and
//! decision row shapes, `operations` the preview/adopt commands, `expand` the
//! ref-expansion helpers, and `validation` the storage validation entry
//! points.

use wns_kernel::SourceEpoch;

use super::*;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use wns_context::project_chat_output::{
    ChatGroupEffectsOutput, parse_project_assistant_output_with_predecessors_and_chapters,
};
use wns_documents::material_adoption::{self, MaterialAdoptionFlow, MaterialTarget};
use wns_story::workshop_state;
use wns_story::{context_packets, story_context, workshop_vocabulary as workshop};

mod expand;
mod operations;
mod stored;
#[cfg(test)]
mod tests;
mod types;
mod validation;

pub(super) use expand::*;
pub(super) use operations::*;
pub(super) use stored::*;
pub(super) use types::*;
pub(super) use validation::*;
// Single bindings wins over the `use super::*` glob, which would otherwise make
// `adoption::validate_chat_workshop_snapshot` ambiguous with the parent
// module's same-named wrapper.
pub(crate) use validation::existing_receipt;
pub(crate) use validation::validate_chat_workshop_snapshot;
