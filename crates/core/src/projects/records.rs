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

use super::check_id;
use crate::documents::Endpoint;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use wns_kernel::{CoreError, CoreResult, DocumentRecord, Head, ProjectAccess, StoredResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectInfo {
    pub project_id: String,
    pub operation_namespace: String,
    pub title: String,
    pub format_version: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMetadata {
    pub project: ProjectInfo,
    pub metadata_version: String,
}
/// Identity of the library operation that installed this independent folder.
/// Kept beside the database so registry recovery does not require a schema upgrade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreationOrigin {
    pub operation_namespace: String,
    pub operation_id: String,
}
pub fn write_creation_origin(path: &Path, origin: &CreationOrigin) -> CoreResult<()> {
    check_id(&origin.operation_namespace)?;
    check_id(&origin.operation_id)?;
    let mut file = File::create_new(path.join("creation.json"))?;
    file.write_all(&serde_json::to_vec(origin)?)?;
    file.sync_all()?;
    Ok(())
}
pub fn read_creation_origin(path: &Path) -> CoreResult<CreationOrigin> {
    use std::io::Read;
    let mut bytes = Vec::new();
    File::open(path.join("creation.json"))?
        .take(4097)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(CoreError::new(
            "InvalidProject",
            "Invalid project creation record.",
        ));
    }
    let origin: CreationOrigin = serde_json::from_slice(&bytes)?;
    check_id(&origin.operation_namespace)?;
    check_id(&origin.operation_id)?;
    Ok(origin)
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveSnapshot {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub local_generation: String,
    pub body: Value,
    pub cause: SaveCause,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SaveCause {
    Typing,
    Undo,
    Redo,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveAck {
    pub project_id: String,
    pub document_id: String,
    pub session: String,
    pub operation_namespace: String,
    pub operation_id: String,
    pub head: Head,
    pub saved_generation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReconcileRequest {
    pub project_id: String,
    pub operation_namespace: String,
    pub session: String,
    pub document_id: String,
    pub pending_operation_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationReceipt {
    pub operation_id: String,
    pub operation_kind: String,
    pub payload_hash: String,
    pub result: StoredResult,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReconciledDocument {
    pub access: ProjectAccess,
    pub document: DocumentRecord,
    pub receipts: Vec<OperationReceipt>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckpointRequest {
    pub access: ProjectAccess,
    pub expected: Head,
    pub reason: CheckpointReason,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckpointReason {
    Manual,
    Switch,
    Close,
    Source,
    Export,
    Interval,
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
