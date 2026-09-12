//! Project and document record types.
//!
//! Split out of `projects.rs`, which was a namespace rather than a module: it
//! held the record types, the actor, the session façade and the persistence
//! helpers in one 2,700-line file, and 28 files under `projects/` reached the
//! whole of it through `use super::*`.
//!
//! Nothing here is new. Every item is re-exported from `projects.rs`, so
//! `use super::*` and `crate::projects::{…}` resolve exactly as before. The
//! split exists because the actor and the records change for different reasons,
//! and a crate cannot be extracted from a namespace.

use crate::documents::Endpoint;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wns_kernel::{
    DocumentRecord, Head, ProjectAccess, ProjectInfo,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMetadata {
    pub project: ProjectInfo,
    pub metadata_version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewState {
    pub document_id: String,
    pub head: Head,
    pub anchor: Endpoint,
    pub focus: Endpoint,
}
#[derive(Debug)]
pub struct AttachedProject {
    pub metadata: ProjectMetadata,
    pub access: ProjectAccess,
    pub documents: Vec<DocumentRecord>,
    pub view_state: Option<ViewState>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateDocument {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub document_id: String,
    pub title: String,
    pub kind: String,
    pub body: Value,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub journal_mode: String,
    pub synchronous: i64,
    pub foreign_keys: i64,
    pub sqlite_version: String,
    pub sqlite_source_id: String,
    pub compile_options: Vec<String>,
}

// The document write contract moved to `wns-documents` (L2), beside the
// documents it describes and below `wns-conversation`, whose assistant-draft
// lifecycle signs the same acks. Re-exported at the historical path.
pub use wns_documents::records::{
    CheckpointReason, CheckpointRequest, OperationReceipt, ReconcileRequest, ReconciledDocument,
    SaveAck, SaveCause, SaveSnapshot,
};

// The creation record moved to `wns-storage` (L1), beside the database it
// describes. Re-exported at the historical path.
pub use wns_storage::creation::{CreationOrigin, read_creation_origin, write_creation_origin};
