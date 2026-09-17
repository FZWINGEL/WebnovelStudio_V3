use super::*;

pub(crate) const MEMORY_INSTRUCTION: &str = "Create the bounded navigation-digest.v1 JSON object for the one supplied saved chapter. Use only exact evidence from that chapter; do not make edits or establish canon.";
pub(crate) const MAX_ERROR_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartMemory {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub budget: MockContextBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryOwner {
    pub project_id: String,
    pub operation_namespace: String,
    pub job_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MemoryJobStatus {
    Queued,
    Running,
    Stopping,
    Completed,
    Stopped,
    Failed,
    Interrupted,
}

impl MemoryJobStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    pub(crate) fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "stopping" => Ok(Self::Stopping),
            "completed" => Ok(Self::Completed),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved memory job has an unknown status.",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MemoryDispatchState {
    Pending,
    Dispatched,
}

impl MemoryDispatchState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Dispatched => "dispatched",
        }
    }

    pub(crate) fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "dispatched" => Ok(Self::Dispatched),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved memory job has an unknown dispatch state.",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryJob {
    pub id: String,
    pub owner: MemoryOwner,
    pub operation_id: String,
    pub payload_hash: String,
    pub target: Head,
    pub source: SourceRef,
    pub snapshot_id: String,
    pub packet_id: String,
    pub context_source_epoch: SourceEpoch,
    pub disclosure_policy_version: String,
    pub provider_binding: Option<ProviderBinding>,
    pub status: MemoryJobStatus,
    pub dispatch_state: MemoryDispatchState,
    pub historical: bool,
    pub stop_reason: Option<String>,
    pub result: Option<MemoryResult>,
    pub view: Option<MemoryView>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryDispatch {
    pub job: MemoryJob,
    pub packet: CompiledPacket,
    pub source: SourceRead,
    /// True only for the queue claim that first changes pending to dispatched.
    /// A replay of a running job is always false and never authorizes another
    /// external provider submission.
    pub newly_dispatched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompleteMemory {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_server: Option<AppServerDelivery>,
    pub owner: MemoryOwner,
    pub event_id: String,
    pub raw_output: String,
    pub outcome: ProviderOutcomeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_stdin_bytes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ProviderUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<ProviderCleanup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The adapter may report an effective identity later.  It is retained as
    /// provenance only and never used to authorize a different binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<ProviderDeliveryReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_server: Option<AppServerDelivery>,
    pub job_id: String,
    pub event_id: String,
    /// None on policy-revoked reads.  The durable terminal record always keeps
    /// the exact raw text in SQLite for historical validation and backup.
    pub raw_output: Option<String>,
    pub outcome: ProviderOutcomeStatus,
    pub confirmed_stdin_bytes: Option<String>,
    pub usage: Option<ProviderUsage>,
    pub cleanup: Option<ProviderCleanup>,
    pub error: Option<String>,
    pub validation_error: Option<String>,
    pub candidate: Option<DigestCandidate>,
    pub effective_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<ProviderDeliveryReceipt>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryCompletion {
    pub job: MemoryJob,
    pub result: MemoryResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryView {
    pub id: String,
    pub job_id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub document_id: String,
    pub target: Head,
    pub source: SourceRef,
    pub snapshot_id: String,
    pub packet_id: String,
    pub context_source_epoch: SourceEpoch,
    pub disclosure_policy_version: String,
    pub candidate: Option<DigestCandidate>,
    pub current: bool,
    pub source_changed: bool,
    pub policy_available: bool,
    /// Copied immutable history has an original owner and cannot authorize a
    /// job in the currently opened independent project.
    pub historical: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryRead {
    pub document_id: String,
    pub jobs: Vec<MemoryJob>,
    pub views: Vec<MemoryView>,
    /// A terminal candidate whose result has not been persisted as a view yet.
    ///
    /// The frontend has always read these two — `ChapterMemory` uses them to
    /// offer a local check and to retry the save — and nothing produced them,
    /// so the path was unreachable. The condition is the one the frontend
    /// already derives for itself when the field is absent: a completed job
    /// carrying a candidate and no view.
    pub pending_save: bool,
    pub pending_job_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryList {
    pub jobs: Vec<MemoryJob>,
    pub views: Vec<MemoryView>,
}

/// Parent actor registration forwards these commands through the one owned
/// SQLite connection.  No command starts a provider call itself; Begin merely
/// claims a queued packet and returns the exact frozen dispatch payload.
#[allow(clippy::large_enum_variant)]
pub enum MemoryCommand {
    ClaimAppServer(MemoryOwner, AppServerDispatch, Reply<()>),
    AckAppServer(MemoryOwner, AppServerDispatch, String, Reply<()>),
    Start(StartMemory, Reply<MemoryJob>),
    Begin(MemoryOwner, Reply<MemoryDispatch>),
    Stop(ProjectAccess, String, Reply<MemoryJob>),
    Complete(CompleteMemory, Reply<MemoryCompletion>),
    Install(MemoryOwner, Reply<MemoryView>),
    Read(ProjectAccess, String, Reply<MemoryRead>),
    List(ProjectAccess, Reply<MemoryList>),
    ReadJob(MemoryOwner, Reply<MemoryJob>),
    ReadViewSource(ProjectAccess, String, Reply<SourceRead>),
    InterruptClaim(MemoryOwner, Reply<MemoryJob>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryRequestIdentity<'a> {
    pub(crate) project_id: &'a str,
    pub(crate) operation_namespace: &'a str,
    pub(crate) operation_id: &'a str,
    pub(crate) expected: &'a Head,
    pub(crate) budget: &'a MockContextBudget,
    pub(crate) provider_binding: &'a Option<ProviderBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredMemoryRequest {
    pub(crate) project_id: String,
    pub(crate) operation_namespace: String,
    pub(crate) operation_id: String,
    pub(crate) expected: Head,
    pub(crate) budget: MockContextBudget,
    pub(crate) provider_binding: Option<ProviderBinding>,
}
