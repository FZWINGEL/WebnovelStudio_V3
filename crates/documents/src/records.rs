//! The document write contract.
//!
//! Save, checkpoint and reconcile-after-a-lost-acknowledgment, as records. They
//! are pure vocabulary over `wns-kernel`'s `Head`, `ProjectAccess`,
//! `DocumentRecord` and `StoredResult`, which is why they can sit at L2 beside
//! the documents they describe — and why `wns-conversation` (L5), whose
//! assistant-draft lifecycle signs the same acks, can name them without an
//! edge into `webnovel-core`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use wns_kernel::{DocumentRecord, Head, ProjectAccess, StoredResult};

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
