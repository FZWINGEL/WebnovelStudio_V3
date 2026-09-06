//! Durable, source-bound chapter navigation memory.
//!
//! Memory refresh is deliberately a small actor-owned lifecycle.  Starting a
//! job freezes one saved chapter and its exact packet in one transaction;
//! dispatch and terminal completion are separate transactions; installation
//! is a short, independently stale-checked transaction.  Generated text is
//! retained as an unreviewed aid and never becomes canon or manuscript text.

use super::*;
use crate::context::memory::{DigestCandidate, MAX_RAW_BYTES, validate_navigation_digest};
use crate::context::packet::{
    CompiledPacket, MEMORY_RESPONSE_CONTRACT, MockContextBudget, PacketError, PacketRequest,
    ProviderBinding, compile_packet, serialized_input,
};
use crate::context::{Audience, BasisKind, ContextPurpose, InformationPolicy, SourceRef};
use crate::projects::context_packets::{PrepareContext, validated_packet_record};
use crate::projects::discussions::{ProviderCleanup, ProviderOutcomeStatus, ProviderUsage};
use crate::projects::story_context::{
    FreezeStory, FrozenContext, SourceRead, read_source, validated_snapshot_record,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

const MEMORY_INSTRUCTION: &str = "Create the bounded navigation-digest.v1 JSON object for the one supplied saved chapter. Use only exact evidence from that chapter; do not make edits or establish canon.";
const MAX_ERROR_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartMemory {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub budget: MockContextBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryOwner {
    pub project_id: String,
    pub operation_namespace: String,
    pub job_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    fn as_str(self) -> &'static str {
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

    fn parse(value: &str) -> CoreResult<Self> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryDispatchState {
    Pending,
    Dispatched,
}

impl MemoryDispatchState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Dispatched => "dispatched",
        }
    }

    fn parse(value: &str) -> CoreResult<Self> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub context_source_epoch: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompleteMemory {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryResult {
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
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryCompletion {
    pub job: MemoryJob,
    pub result: MemoryResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub context_source_epoch: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryRead {
    pub document_id: String,
    pub jobs: Vec<MemoryJob>,
    pub views: Vec<MemoryView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryList {
    pub jobs: Vec<MemoryJob>,
    pub views: Vec<MemoryView>,
}

/// Parent actor registration forwards these commands through the one owned
/// SQLite connection.  No command starts a provider call itself; Begin merely
/// claims a queued packet and returns the exact frozen dispatch payload.
#[allow(clippy::large_enum_variant)]
pub(super) enum MemoryCommand {
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
struct MemoryRequestIdentity<'a> {
    project_id: &'a str,
    operation_namespace: &'a str,
    operation_id: &'a str,
    expected: &'a Head,
    budget: &'a MockContextBudget,
    provider_binding: &'a Option<ProviderBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredMemoryRequest {
    project_id: String,
    operation_namespace: String,
    operation_id: String,
    expected: Head,
    budget: MockContextBudget,
    provider_binding: Option<ProviderBinding>,
}

impl StartMemory {
    fn identity(&self) -> MemoryRequestIdentity<'_> {
        MemoryRequestIdentity {
            project_id: &self.access.project_id,
            operation_namespace: &self.access.operation_namespace,
            operation_id: &self.operation_id,
            expected: &self.expected,
            budget: &self.budget,
            provider_binding: &self.provider_binding,
        }
    }

    fn stored_request(&self) -> StoredMemoryRequest {
        StoredMemoryRequest {
            project_id: self.access.project_id.clone(),
            operation_namespace: self.access.operation_namespace.clone(),
            operation_id: self.operation_id.clone(),
            expected: self.expected.clone(),
            budget: self.budget.clone(),
            provider_binding: self.provider_binding.clone(),
        }
    }
}

impl ProjectSession {
    pub fn start_memory(&self, request: StartMemory) -> CoreResult<MemoryJob> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Start(request, reply))))
    }

    pub fn begin_memory(&self, owner: MemoryOwner) -> CoreResult<MemoryDispatch> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Begin(owner, reply))))
    }

    pub fn stop_memory(&self, access: ProjectAccess, job_id: String) -> CoreResult<MemoryJob> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Stop(access, job_id, reply))))
    }

    pub fn complete_memory(&self, request: CompleteMemory) -> CoreResult<MemoryCompletion> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Complete(request, reply))))
    }

    pub fn install_memory(&self, owner: MemoryOwner) -> CoreResult<MemoryView> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::Install(owner, reply))))
    }

    pub fn read_memory(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<MemoryRead> {
        self.request(|reply| {
            Command::Memory(Box::new(MemoryCommand::Read(access, document_id, reply)))
        })
    }

    pub fn list_memory(&self, access: ProjectAccess) -> CoreResult<MemoryList> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::List(access, reply))))
    }

    pub fn read_memory_job(&self, owner: MemoryOwner) -> CoreResult<MemoryJob> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::ReadJob(owner, reply))))
    }

    pub fn read_memory_source(
        &self,
        access: ProjectAccess,
        view_id: String,
    ) -> CoreResult<SourceRead> {
        self.request(|reply| {
            Command::Memory(Box::new(MemoryCommand::ReadViewSource(
                access, view_id, reply,
            )))
        })
    }

    pub fn interrupt_memory_claim(&self, owner: MemoryOwner) -> CoreResult<MemoryJob> {
        self.request(|reply| Command::Memory(Box::new(MemoryCommand::InterruptClaim(owner, reply))))
    }
}

impl OwnedProject {
    pub(super) fn handle_memory(&mut self, command: MemoryCommand) {
        macro_rules! mutate {
            ($reply:expr, $operation:expr) => {{
                let result = $operation;
                self.fence_uncertain(&result);
                let _ = $reply.send(result);
            }};
        }
        match command {
            MemoryCommand::Start(request, reply) => mutate!(reply, self.start_memory(request)),
            MemoryCommand::Begin(owner, reply) => mutate!(reply, self.begin_memory(owner)),
            MemoryCommand::Stop(access, job_id, reply) => {
                mutate!(reply, self.stop_memory(access, job_id))
            }
            MemoryCommand::Complete(request, reply) => {
                mutate!(reply, self.complete_memory(request))
            }
            MemoryCommand::Install(owner, reply) => {
                mutate!(reply, self.install_memory(owner))
            }
            MemoryCommand::Read(access, document_id, reply) => {
                let _ = reply.send(self.read_memory(access, &document_id));
            }
            MemoryCommand::List(access, reply) => {
                let _ = reply.send(self.list_memory(access));
            }
            MemoryCommand::ReadJob(owner, reply) => {
                let result = validate_runtime_owner(self, &owner)
                    .and_then(|()| {
                        let policy = current_policy_version(self.db()?)?;
                        read_memory_job_with_policy(self.db()?, &owner.job_id, &policy)
                    })
                    .and_then(|job| {
                        if job.owner == owner {
                            Ok(job)
                        } else {
                            Err(CoreError::new(
                                "MemoryProjectMismatch",
                                "The memory owner does not match the persisted job.",
                            ))
                        }
                    });
                let _ = reply.send(result);
            }
            MemoryCommand::ReadViewSource(access, view_id, reply) => {
                let _ = reply.send(self.read_memory_source(access, &view_id));
            }
            MemoryCommand::InterruptClaim(owner, reply) => {
                mutate!(reply, self.interrupt_memory_claim(owner))
            }
        }
    }

    pub(super) fn start_memory(&mut self, request: StartMemory) -> CoreResult<MemoryJob> {
        self.check_access(&request.access)?;
        validate_start_memory(&request)?;
        let payload_hash = sha256_hex(
            serde_json::to_string(&request.identity())
                .map_err(CoreError::from)?
                .as_bytes(),
        );
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT id,payload_hash FROM memory_jobs WHERE operation_namespace=? AND operation_id=?",
                params![request.access.operation_namespace, request.operation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((job_id, previous_hash)) = existing {
            if previous_hash != payload_hash {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This memory refresh operation was already used for a different request.",
                ));
            }
            let job = read_memory_job_with_policy(&tx, &job_id, &current_policy_version(&tx)?)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(job);
        }

        let policy = current_memory_policy(&tx)?;
        let freeze = FreezeStory {
            access: request.access.clone(),
            operation_id: request.operation_id.clone(),
            expected: request.expected.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::MemoryAnalysis,
            policy,
        };
        let frozen = story_context::freeze_memory_story_at(&tx, &freeze, &payload_hash)?;
        let target_handle = frozen.snapshot.target.revision_id.clone();
        let source = story_context::read_source(&tx, &frozen, &target_handle)?;
        let prepare = PrepareContext {
            access: request.access.clone(),
            operation_id: request.operation_id.clone(),
            snapshot_id: frozen.snapshot.snapshot_id.clone(),
            instruction: MEMORY_INSTRUCTION.to_owned(),
            mandatory_handles: Vec::new(),
            transient_mandatory_handles: None,
            safe_brief: None,
            scope: None,
            budget: request.budget.clone(),
            provider_binding: request.provider_binding.clone(),
            response_contract: Some(MEMORY_RESPONSE_CONTRACT.to_owned()),
        };
        let packet_request = PacketRequest {
            packet_id: new_id(),
            session_id: new_id(),
            invocation_ordinal: "0".to_owned(),
            frozen: frozen.clone(),
            instruction: MEMORY_INSTRUCTION.to_owned(),
            sources: vec![source.clone()],
            mandatory_handles: Vec::new(),
            scope: None,
            safe_brief: None,
            budget: request.budget.clone(),
            provider_binding: request.provider_binding.clone(),
            response_contract: Some(MEMORY_RESPONSE_CONTRACT.to_owned()),
        };
        let packet = compile_packet(&packet_request).map_err(packet_error)?;
        context_packets::persist_compiled_packet_at(&tx, &prepare, &packet)?;
        let job_id = new_id();
        let stored_request = request.stored_request();
        tx.execute(
            "INSERT INTO memory_jobs(id,project_id,operation_namespace,operation_id,payload_hash,request_json,target_document_id,target_version,target_body_hash,source_document_id,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,status,dispatch_state) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                job_id,
                request.access.project_id,
                request.access.operation_namespace,
                request.operation_id,
                payload_hash,
                serde_json::to_string(&stored_request)?,
                request.expected.document_id,
                parse_version(&request.expected.version)?,
                request.expected.body_hash,
                source.descriptor.source.document_id,
                source.descriptor.source.revision_id,
                source.descriptor.source.body_hash,
                frozen.snapshot.snapshot_id,
                packet.receipt.packet_id,
                parse_version(&frozen.snapshot.context_source_epoch)?,
                parse_version(&frozen.policy.version)?,
                MemoryJobStatus::Queued.as_str(),
                MemoryDispatchState::Pending.as_str(),
            ],
        )?;
        let job = read_memory_job_with_policy(&tx, &job_id, &current_policy_version(&tx)?)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(job)
    }

    pub(super) fn begin_memory(&mut self, owner: MemoryOwner) -> CoreResult<MemoryDispatch> {
        validate_runtime_owner(self, &owner)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_memory_job_row(&tx, &owner.job_id)?;
        validate_job_owner(&current, &owner)?;
        let status = MemoryJobStatus::parse(&current.status)?;
        if matches!(status, MemoryJobStatus::Queued) {
            if MemoryDispatchState::parse(&current.dispatch_state)? != MemoryDispatchState::Pending
            {
                return Err(CoreError::new(
                    "MemoryDispatchConflict",
                    "The queued memory job has already been claimed.",
                ));
            }
            let (frozen, namespace) =
                story_context::validated_snapshot_record(&tx, &current.snapshot_id)?;
            if namespace != owner.operation_namespace
                || frozen.snapshot.project_id != owner.project_id
            {
                return Err(CoreError::new(
                    "MemoryProjectMismatch",
                    "The memory snapshot belongs to another project namespace.",
                ));
            }
            ensure_current_policy(&tx, &frozen)?;
            ensure_memory_basis_current(&tx, &current)?;
            let source = story_context::read_source(&tx, &frozen, &current.source_revision_id)?;
            let packet = context_packets::validated_packet_record(&tx, &current.packet_id)?;
            tx.execute(
                "UPDATE memory_jobs SET status='running',dispatch_state='dispatched',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status='queued' AND dispatch_state='pending'",
                [&owner.job_id],
            )?;
            let job = read_memory_job(&tx, &owner.job_id, true)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(MemoryDispatch {
                job,
                packet,
                source,
                newly_dispatched: true,
            });
        }
        if matches!(status, MemoryJobStatus::Running) {
            let (frozen, namespace) =
                story_context::validated_snapshot_record(&tx, &current.snapshot_id)?;
            if namespace != owner.operation_namespace
                || frozen.snapshot.project_id != owner.project_id
            {
                return Err(CoreError::new(
                    "MemoryProjectMismatch",
                    "The memory snapshot belongs to another project namespace.",
                ));
            }
            ensure_current_policy(&tx, &frozen)?;
            let source = story_context::read_source(&tx, &frozen, &current.source_revision_id)?;
            let packet = context_packets::validated_packet_record(&tx, &current.packet_id)?;
            let job = read_memory_job(&tx, &owner.job_id, true)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(MemoryDispatch {
                job,
                packet,
                source,
                newly_dispatched: false,
            });
        }
        Err(CoreError::new(
            "MemoryJobSealed",
            "This memory job cannot be dispatched in its current state.",
        ))
    }

    /// Resolve an uncertain durable dispatch claim without submitting a
    /// provider request.  This is intentionally owner based so recovery can
    /// run without a renderer lease.
    pub(super) fn interrupt_memory_claim(&mut self, owner: MemoryOwner) -> CoreResult<MemoryJob> {
        validate_runtime_owner(self, &owner)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_memory_job_row(&tx, &owner.job_id)?;
        validate_job_owner(&current, &owner)?;
        let status = MemoryJobStatus::parse(&current.status)?;
        let result = read_memory_result(&tx, &owner.job_id, true)?;
        if !matches!(
            status,
            MemoryJobStatus::Queued | MemoryJobStatus::Running | MemoryJobStatus::Stopping
        ) {
            // Terminal and already-interrupted claims are idempotent reads.
            let job =
                read_memory_job_with_policy(&tx, &owner.job_id, &current_policy_version(&tx)?)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(job);
        }
        if result.is_some() {
            return Err(CoreError::new(
                "InvalidMemoryLifecycle",
                "An active memory claim already has a terminal result.",
            ));
        }
        validate_memory_lifecycle(&current, None, false)?;
        tx.execute(
            "UPDATE memory_jobs SET status='interrupted',stop_reason='dispatch_outcome_unknown',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status IN ('queued','running','stopping')",
            [&owner.job_id],
        )?;
        let job = read_memory_job_with_policy(&tx, &owner.job_id, &current_policy_version(&tx)?)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(job)
    }

    pub(super) fn stop_memory(
        &mut self,
        access: ProjectAccess,
        job_id: String,
    ) -> CoreResult<MemoryJob> {
        self.check_access(&access)?;
        check_id(&job_id)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_memory_job_row(&tx, &job_id)?;
        if current.project_id != access.project_id
            || current.operation_namespace != access.operation_namespace
        {
            return Err(CoreError::new(
                "MemoryProjectMismatch",
                "The memory job belongs to another project namespace.",
            ));
        }
        let status = MemoryJobStatus::parse(&current.status)?;
        let next = match status {
            MemoryJobStatus::Queued => Some(MemoryJobStatus::Stopped),
            MemoryJobStatus::Running => Some(MemoryJobStatus::Stopping),
            _ => None,
        };
        if let Some(next) = next {
            tx.execute(
                "UPDATE memory_jobs SET status=?,stop_reason='author_stopped',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status=?",
                params![next.as_str(), job_id, status.as_str()],
            )?;
        }
        let job = read_memory_job_with_policy(&tx, &job_id, &current_policy_version(&tx)?)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(job)
    }

    pub(super) fn complete_memory(
        &mut self,
        request: CompleteMemory,
    ) -> CoreResult<MemoryCompletion> {
        validate_runtime_owner(self, &request.owner)?;
        validate_complete_memory(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_memory_job_row(&tx, &request.owner.job_id)?;
        validate_job_owner(&current, &request.owner)?;
        if let Some(saved) = read_memory_result(&tx, &request.owner.job_id, true)? {
            if result_matches(&saved, &request) {
                let policy = current_policy_version(&tx)?;
                let reveal = current.disclosure_policy_epoch.to_string() == policy;
                let result = if reveal {
                    saved
                } else {
                    read_memory_result(&tx, &request.owner.job_id, false)?.ok_or_else(|| {
                        CoreError::new(
                            "PersistenceUnavailable",
                            "The memory result could not be read.",
                        )
                    })?
                };
                let job = read_memory_job_with_policy(&tx, &request.owner.job_id, &policy)?;
                tx.commit().map_err(CoreError::uncertain)?;
                return Ok(MemoryCompletion { job, result });
            }
            return Err(CoreError::new(
                "MemoryResultConflict",
                "A terminal memory result is already durably recorded for this job.",
            ));
        }
        let status = MemoryJobStatus::parse(&current.status)?;
        let recovered_dispatch_claim = status == MemoryJobStatus::Interrupted
            && current.dispatch_state == MemoryDispatchState::Dispatched.as_str()
            && current.stop_reason.as_deref() == Some("recovered_unknown_external_outcome");
        if !matches!(status, MemoryJobStatus::Running | MemoryJobStatus::Stopping)
            && !recovered_dispatch_claim
        {
            return Err(CoreError::new(
                "MemoryJobNotRunning",
                "Claim the queued memory job before recording its terminal result.",
            ));
        }
        let packet = context_packets::validated_packet_record(&tx, &current.packet_id)?;
        validate_delivery(&request, &packet)?;
        let (frozen, namespace) =
            story_context::validated_snapshot_record(&tx, &current.snapshot_id)?;
        if namespace != request.owner.operation_namespace
            || frozen.snapshot.project_id != request.owner.project_id
        {
            return Err(CoreError::new(
                "MemoryProjectMismatch",
                "The memory snapshot belongs to another project namespace.",
            ));
        }
        let source = story_context::read_source(&tx, &frozen, &current.source_revision_id)?;
        if source.descriptor.source.project_id != current.project_id
            || source.descriptor.source.document_id != current.source_document_id
            || source.descriptor.source.revision_id != current.source_revision_id
            || source.descriptor.source.body_hash != current.source_body_hash
        {
            return Err(CoreError::new(
                "InvalidMemoryJob",
                "The memory job source binding does not match its frozen snapshot.",
            ));
        }
        let (candidate, validation_error) =
            match validate_navigation_digest(request.raw_output.as_bytes(), &source) {
                Ok(candidate) => (Some(candidate), None),
                Err(error) => (None, Some(format!("{}: {}", error.code, error.detail))),
            };
        let (lifecycle, stop_reason) = if recovered_dispatch_claim {
            (
                MemoryJobStatus::Interrupted,
                Some("recovered_unknown_external_outcome"),
            )
        } else {
            terminal_state_with_cleanup(
                status,
                request.outcome,
                candidate.is_some(),
                request.cleanup,
            )
        };
        let candidate_json = candidate.as_ref().map(serde_json::to_string).transpose()?;
        let usage_json = request
            .usage
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        tx.execute(
            "INSERT INTO memory_results(job_id,event_id,raw_output,raw_output_hash,candidate_json,outcome,confirmed_stdin_bytes,usage_json,cleanup,error,validation_error,effective_identity) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                request.owner.job_id,
                request.event_id,
                request.raw_output,
                sha256_hex(request.raw_output.as_bytes()),
                candidate_json,
                provider_outcome_as_str(request.outcome),
                request.confirmed_stdin_bytes.as_deref().map(parse_decimal_u64).transpose()?.map(i64::try_from).transpose().map_err(|_| CoreError::new("InvalidRequest", "The provider stdin byte count is too large."))?,
                usage_json,
                request.cleanup.map(provider_cleanup_as_str),
                request.error,
                validation_error,
                request.effective_identity,
            ],
        )?;
        tx.execute(
            "UPDATE memory_jobs SET status=?,stop_reason=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND (status IN ('running','stopping') OR (status='interrupted' AND dispatch_state='dispatched' AND stop_reason='recovered_unknown_external_outcome'))",
            params![lifecycle.as_str(), stop_reason, request.owner.job_id,],
        )?;
        let policy = current_policy_version(&tx)?;
        let reveal = current.disclosure_policy_epoch.to_string() == policy;
        let result = read_memory_result(&tx, &request.owner.job_id, reveal)?.ok_or_else(|| {
            CoreError::new(
                "PersistenceUnavailable",
                "The memory result could not be read.",
            )
        })?;
        let job = read_memory_job_with_policy(&tx, &request.owner.job_id, &policy)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(MemoryCompletion { job, result })
    }

    pub(super) fn install_memory(&mut self, owner: MemoryOwner) -> CoreResult<MemoryView> {
        validate_runtime_owner(self, &owner)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_memory_job_row(&tx, &owner.job_id)?;
        validate_job_owner(&current, &owner)?;
        if let Some(view) = read_memory_view(&tx, &owner.job_id, true)? {
            let resolved = resolve_view_current(&tx, view)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(resolved);
        }
        if MemoryJobStatus::parse(&current.status)? != MemoryJobStatus::Completed {
            return Err(CoreError::new(
                "MemoryInstallBlocked",
                "Only a completed memory candidate can be installed.",
            ));
        }
        let result = read_memory_result(&tx, &owner.job_id, true)?.ok_or_else(|| {
            CoreError::new(
                "MemoryInstallBlocked",
                "The completed memory job has no result.",
            )
        })?;
        let candidate_json = result
            .candidate
            .as_ref()
            .ok_or_else(|| {
                CoreError::new(
                    "MemoryCandidateInvalid",
                    "The terminal memory output did not validate.",
                )
            })
            .and_then(|candidate| serde_json::to_string(candidate).map_err(CoreError::from))?;
        let (frozen, namespace) =
            story_context::validated_snapshot_record(&tx, &current.snapshot_id)?;
        if namespace != owner.operation_namespace || frozen.snapshot.project_id != owner.project_id
        {
            return Err(CoreError::new(
                "MemoryProjectMismatch",
                "The memory snapshot belongs to another project namespace.",
            ));
        }
        ensure_current_policy(&tx, &frozen)?;
        let current_policy = current_policy_version(&tx)?;
        let current_epoch = current_source_epoch(&tx)?;
        let document = read_document(&tx, &current.target_document_id)?;
        let current_basis = document.head.version == current.target_version.to_string()
            && document.head.body_hash == current.target_body_hash
            && current_epoch == current.context_source_epoch.to_string()
            && current_policy == current.disclosure_policy_epoch.to_string();
        let view_id = new_id();
        tx.execute(
            "INSERT INTO memory_views(id,job_id,project_id,operation_namespace,document_id,target_version,target_body_hash,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,candidate_json,installed_current) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                view_id,
                owner.job_id,
                current.project_id,
                current.operation_namespace,
                current.target_document_id,
                current.target_version,
                current.target_body_hash,
                current.source_revision_id,
                current.source_body_hash,
                current.snapshot_id,
                current.packet_id,
                current.context_source_epoch,
                current.disclosure_policy_epoch,
                candidate_json,
                current_basis,
            ],
        )?;
        tx.execute(
            "INSERT INTO memory_view_sources(view_id,document_id,revision_id,body_hash) VALUES(?,?,?,?)",
            params![
                view_id,
                current.source_document_id,
                current.source_revision_id,
                current.source_body_hash
            ],
        )?;
        let view = read_memory_view(&tx, &owner.job_id, true)?.ok_or_else(|| {
            CoreError::new(
                "PersistenceUnavailable",
                "The generated memory view could not be read.",
            )
        })?;
        let view = resolve_view_current(&tx, view)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(view)
    }

    pub(super) fn read_memory(
        &self,
        access: ProjectAccess,
        document_id: &str,
    ) -> CoreResult<MemoryRead> {
        self.check_access(&access)?;
        check_id(document_id)?;
        let policy = current_policy_version(self.db()?)?;
        // A recovered project rotates its live identity while retaining the
        // old memory rows as read-only history.  Read/list are therefore
        // document scoped; every mutating path still validates the live
        // project/namespace owner before it can act on a job.
        let mut jobs = self.db()?.prepare(
            "SELECT id FROM memory_jobs WHERE target_document_id=? ORDER BY created_at,id",
        )?;
        let ids = jobs
            .query_map([document_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let jobs = ids
            .iter()
            .map(|id| read_memory_job_with_policy(self.db()?, id, policy.as_str()))
            .collect::<CoreResult<Vec<_>>>()?;
        let views = read_memory_views_for_document(self.db()?, document_id, policy.as_str())?;
        Ok(MemoryRead {
            document_id: document_id.to_owned(),
            jobs,
            views,
        })
    }

    fn read_memory_source(&self, access: ProjectAccess, view_id: &str) -> CoreResult<SourceRead> {
        self.check_access(&access)?;
        check_id(view_id)?;
        let db = self.db()?;
        // A view must be present in this opened database. Its original owner
        // is retained on recovery; this read grants no operation authority.
        let job_id: String = db
            .query_row(
                "SELECT job_id FROM memory_views WHERE id=?",
                [view_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                CoreError::new(
                    "MemoryViewNotFound",
                    "The memory view is not available in this project.",
                )
            })?;
        let job = read_memory_job_row(db, &job_id)?;
        validate_memory_job_record(db, &job)?;
        let (frozen, _) = validated_snapshot_record(db, &job.snapshot_id)?;
        ensure_current_policy(db, &frozen)?;
        read_source(db, &frozen, &job.source_revision_id)
    }

    pub(super) fn list_memory(&self, access: ProjectAccess) -> CoreResult<MemoryList> {
        self.check_access(&access)?;
        let policy = current_policy_version(self.db()?)?;
        let mut jobs = self
            .db()?
            .prepare("SELECT id FROM memory_jobs ORDER BY created_at,id")?;
        let ids = jobs
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let jobs = ids
            .iter()
            .map(|id| read_memory_job_with_policy(self.db()?, id, policy.as_str()))
            .collect::<CoreResult<Vec<_>>>()?;
        let views = read_memory_views(self.db()?, policy.as_str())?;
        Ok(MemoryList { jobs, views })
    }

    /// Reopening never submits a provider request.  Active jobs become
    /// interrupted with unknown external outcome and remain historical.
    pub(super) fn recover_interrupted_memory(&mut self) -> CoreResult<u32> {
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE memory_jobs SET status='interrupted',stop_reason='recovered_unknown_external_outcome',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status IN ('queued','running','stopping')",
            [],
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(changed as u32)
    }
}

#[derive(Debug, Clone)]
struct MemoryJobRow {
    id: String,
    project_id: String,
    operation_namespace: String,
    operation_id: String,
    payload_hash: String,
    request_json: String,
    target_document_id: String,
    target_version: i64,
    target_body_hash: String,
    source_document_id: String,
    source_revision_id: String,
    source_body_hash: String,
    snapshot_id: String,
    packet_id: String,
    context_source_epoch: i64,
    disclosure_policy_epoch: i64,
    status: String,
    dispatch_state: String,
    stop_reason: Option<String>,
    created_at: String,
    updated_at: String,
}

fn validate_start_memory(request: &StartMemory) -> CoreResult<()> {
    check_id(&request.operation_id)?;
    check_id(&request.expected.document_id)?;
    parse_version(&request.expected.version)?;
    if request.expected.body_hash.len() != 64
        || !request
            .expected
            .body_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The expected chapter head has an invalid body hash.",
        ));
    }
    if request.budget.model_id != crate::context::packet::MOCK_MODEL_ID {
        return Err(CoreError::new(
            "InvalidBudget",
            "The chapter memory recipe uses the deterministic mock budget profile.",
        ));
    }
    if let Some(binding) = &request.provider_binding {
        binding
            .validate()
            .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
    }
    Ok(())
}

fn validate_complete_memory(request: &CompleteMemory) -> CoreResult<()> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.job_id)?;
    check_id(&request.event_id)?;
    if request.raw_output.len() > MAX_RAW_BYTES {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The memory terminal output exceeds the 64 KiB application cap.",
        ));
    }
    if let Some(value) = &request.confirmed_stdin_bytes {
        parse_decimal_u64(value)?;
    }
    validate_optional_text(request.error.as_deref(), "provider error")?;
    validate_optional_text(request.effective_identity.as_deref(), "provider identity")?;
    Ok(())
}

fn parse_decimal_u64(value: &str) -> CoreResult<u64> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "Provider counters must be canonical nonnegative decimal strings.",
        ));
    }
    value.parse::<u64>().map_err(|_| {
        CoreError::new(
            "InvalidRequest",
            "Provider counters exceed the supported range.",
        )
    })
}

fn validate_optional_text(value: Option<&str>, label: &str) -> CoreResult<()> {
    if value.is_some_and(|value| {
        value.is_empty() || value.len() > MAX_ERROR_BYTES || value.chars().any(char::is_control)
    }) {
        return Err(CoreError::new(
            "InvalidRequest",
            &format!("The {label} is empty, too large, or contains control characters."),
        ));
    }
    Ok(())
}

fn current_memory_policy(tx: &Connection) -> CoreResult<InformationPolicy> {
    Ok(InformationPolicy {
        version: current_policy_version(tx)?,
        audience: Audience::AuthorRoom,
        reader_frontier: None,
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    })
}

fn current_policy_version(db: &Connection) -> CoreResult<String> {
    let value: i64 = db.query_row(
        "SELECT disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    parse_stored_version(value)
}

fn current_source_epoch(db: &Connection) -> CoreResult<String> {
    let value: i64 = db.query_row(
        "SELECT context_source_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    parse_stored_version(value)
}

fn ensure_current_policy(db: &Connection, frozen: &FrozenContext) -> CoreResult<()> {
    if frozen.policy.version != current_policy_version(db)? {
        return Err(CoreError::new(
            "ContextPolicyChanged",
            "Source permissions changed. The memory job cannot expose its frozen source.",
        ));
    }
    Ok(())
}

fn ensure_memory_basis_current(db: &Connection, row: &MemoryJobRow) -> CoreResult<()> {
    let document = read_document(db, &row.target_document_id)?;
    if document.head.version != row.target_version.to_string()
        || document.head.body_hash != row.target_body_hash
        || current_source_epoch(db)? != row.context_source_epoch.to_string()
        || current_policy_version(db)? != row.disclosure_policy_epoch.to_string()
    {
        Err(CoreError::new(
            "MemoryBasisChanged",
            "The saved chapter or story-context basis changed before memory dispatch.",
        ))
    } else {
        Ok(())
    }
}

fn read_memory_job_row(db: &Connection, id: &str) -> CoreResult<MemoryJobRow> {
    check_id(id)?;
    db.query_row(
        "SELECT id,project_id,operation_namespace,operation_id,payload_hash,request_json,target_document_id,target_version,target_body_hash,source_document_id,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,status,dispatch_state,stop_reason,created_at,updated_at FROM memory_jobs WHERE id=?",
        [id],
        |row| {
            Ok(MemoryJobRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                operation_namespace: row.get(2)?,
                operation_id: row.get(3)?,
                payload_hash: row.get(4)?,
                request_json: row.get(5)?,
                target_document_id: row.get(6)?,
                target_version: row.get(7)?,
                target_body_hash: row.get(8)?,
                source_document_id: row.get(9)?,
                source_revision_id: row.get(10)?,
                source_body_hash: row.get(11)?,
                snapshot_id: row.get(12)?,
                packet_id: row.get(13)?,
                context_source_epoch: row.get(14)?,
                disclosure_policy_epoch: row.get(15)?,
                status: row.get(16)?,
                dispatch_state: row.get(17)?,
                stop_reason: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::new("MemoryJobNotFound", "The memory job is not available."))
}

fn validate_job_owner(row: &MemoryJobRow, owner: &MemoryOwner) -> CoreResult<()> {
    if row.project_id != owner.project_id
        || row.operation_namespace != owner.operation_namespace
        || row.id != owner.job_id
    {
        return Err(CoreError::new(
            "MemoryProjectMismatch",
            "The memory owner does not match the persisted project namespace and job.",
        ));
    }
    Ok(())
}

fn invalid_memory_lifecycle() -> CoreError {
    CoreError::new(
        "InvalidMemoryLifecycle",
        "The memory job status, dispatch state, stop reason, and result do not form a legal lifecycle.",
    )
}

/// Validate lifecycle-only invariants shared by request reads and transfer
/// validation.  Source/candidate integrity is checked separately by the
/// historical validator; policy-redacted reads still need these state checks.
fn validate_memory_lifecycle(
    row: &MemoryJobRow,
    result: Option<&MemoryResult>,
    provider_bound: bool,
) -> CoreResult<()> {
    let status = MemoryJobStatus::parse(&row.status)?;
    let dispatch = MemoryDispatchState::parse(&row.dispatch_state)?;
    let reason = row.stop_reason.as_deref();
    let valid = match status {
        MemoryJobStatus::Queued => {
            dispatch == MemoryDispatchState::Pending && result.is_none() && reason.is_none()
        }
        MemoryJobStatus::Running => {
            dispatch == MemoryDispatchState::Dispatched && result.is_none() && reason.is_none()
        }
        MemoryJobStatus::Stopping => {
            dispatch == MemoryDispatchState::Dispatched
                && result.is_none()
                && reason == Some("author_stopped")
        }
        MemoryJobStatus::Stopped => {
            matches!(reason, Some("author_stopped") | Some("provider_stopped"))
                && match (dispatch, result) {
                    (MemoryDispatchState::Pending, None) => reason == Some("author_stopped"),
                    (MemoryDispatchState::Dispatched, Some(result)) => {
                        result.cleanup != Some(ProviderCleanup::Unresolved)
                            && (!provider_bound
                                || result.outcome != ProviderOutcomeStatus::Completed
                                || result.cleanup == Some(ProviderCleanup::Settled))
                    }
                    _ => false,
                }
        }
        MemoryJobStatus::Completed => {
            dispatch == MemoryDispatchState::Dispatched
                && reason.is_none()
                && result.is_some_and(|result| {
                    result.outcome == ProviderOutcomeStatus::Completed
                        && result.cleanup != Some(ProviderCleanup::Unresolved)
                        && (!provider_bound || result.cleanup == Some(ProviderCleanup::Settled))
                })
        }
        MemoryJobStatus::Failed => {
            dispatch == MemoryDispatchState::Dispatched
                && reason.is_none()
                && result.is_some_and(|result| {
                    result.cleanup != Some(ProviderCleanup::Unresolved)
                        && (!provider_bound
                            || result.outcome != ProviderOutcomeStatus::Completed
                            || result.cleanup == Some(ProviderCleanup::Settled))
                })
        }
        MemoryJobStatus::Interrupted => match result {
            None => {
                matches!(
                    dispatch,
                    MemoryDispatchState::Pending | MemoryDispatchState::Dispatched
                ) && matches!(
                    reason,
                    Some("recovered_unknown_external_outcome") | Some("dispatch_outcome_unknown")
                )
            }
            Some(result) => {
                dispatch == MemoryDispatchState::Dispatched
                    && (!provider_bound
                        || result.outcome != ProviderOutcomeStatus::Completed
                        || result.cleanup == Some(ProviderCleanup::Settled))
                    && ((result.cleanup == Some(ProviderCleanup::Unresolved)
                        && matches!(reason, Some("author_stopped") | Some("cleanup_unresolved")))
                        || reason == Some("recovered_unknown_external_outcome"))
            }
        },
    };
    if valid {
        Ok(())
    } else {
        Err(invalid_memory_lifecycle())
    }
}

fn validate_runtime_owner(project: &OwnedProject, owner: &MemoryOwner) -> CoreResult<()> {
    if owner.project_id != project.info.project_id
        || owner.operation_namespace != project.info.operation_namespace
    {
        return Err(CoreError::new(
            "MemoryProjectMismatch",
            "The memory job belongs to another project or recovered project identity.",
        ));
    }
    Ok(())
}

fn read_memory_job(db: &Connection, id: &str, reveal: bool) -> CoreResult<MemoryJob> {
    let row = read_memory_job_row(db, id)?;
    let historical: bool = db.query_row(
        "SELECT id<>? OR operation_namespace<>? FROM project WHERE singleton=1",
        params![row.project_id, row.operation_namespace],
        |record| record.get(0),
    )?;
    let packet = validated_packet_record(db, &row.packet_id)?;
    let source = SourceRef {
        project_id: row.project_id.clone(),
        document_id: row.source_document_id.clone(),
        revision_id: row.source_revision_id.clone(),
        body_hash: row.source_body_hash.clone(),
    };
    let result = read_memory_result(db, id, reveal)?;
    validate_memory_lifecycle(
        &row,
        result.as_ref(),
        packet.options.provider_binding.is_some(),
    )?;
    let view = read_memory_view(db, id, reveal)?;
    Ok(MemoryJob {
        id: row.id.clone(),
        owner: MemoryOwner {
            project_id: row.project_id,
            operation_namespace: row.operation_namespace,
            job_id: row.id,
        },
        operation_id: row.operation_id,
        payload_hash: row.payload_hash,
        target: Head {
            document_id: row.target_document_id,
            version: row.target_version.to_string(),
            body_hash: row.target_body_hash,
        },
        source,
        snapshot_id: row.snapshot_id,
        packet_id: row.packet_id,
        context_source_epoch: row.context_source_epoch.to_string(),
        disclosure_policy_version: row.disclosure_policy_epoch.to_string(),
        provider_binding: packet.options.provider_binding,
        status: MemoryJobStatus::parse(&row.status)?,
        dispatch_state: MemoryDispatchState::parse(&row.dispatch_state)?,
        historical,
        stop_reason: row.stop_reason,
        result,
        view,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn read_memory_job_with_policy(
    db: &Connection,
    id: &str,
    policy_version: &str,
) -> CoreResult<MemoryJob> {
    let row = read_memory_job_row(db, id)?;
    let mut job = read_memory_job(
        db,
        id,
        row.disclosure_policy_epoch.to_string() == policy_version,
    )?;
    if let Some(view) = job.view.take() {
        job.view = Some(resolve_view_current(db, view)?);
    }
    Ok(job)
}

fn validate_delivery(request: &CompleteMemory, packet: &CompiledPacket) -> CoreResult<()> {
    if packet.options.provider_binding.is_some()
        && request.outcome == ProviderOutcomeStatus::Completed
        && request.cleanup != Some(ProviderCleanup::Settled)
    {
        return Err(CoreError::new(
            "ProviderCleanupUnknown",
            "A completed live memory result needs confirmed settled provider cleanup.",
        ));
    }
    let serialized = serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
    let delivered = request
        .confirmed_stdin_bytes
        .as_deref()
        .map(parse_decimal_u64)
        .transpose()?;
    if packet.options.provider_binding.is_some() {
        match (request.outcome, delivered) {
            (ProviderOutcomeStatus::Completed, Some(delivered))
                if delivered == serialized.len() as u64 => {}
            (ProviderOutcomeStatus::Completed, _) => {
                return Err(CoreError::new(
                    "ProviderInputUnknown",
                    "A completed live memory result needs exact local stdin delivery proof.",
                ));
            }
            (_, Some(delivered)) if delivered > serialized.len() as u64 => {
                return Err(CoreError::new(
                    "ProviderInputMismatch",
                    "The provider delivery count exceeds the frozen memory packet.",
                ));
            }
            _ => {}
        }
    } else if let Some(delivered) = delivered
        && delivered > serialized.len() as u64
    {
        return Err(CoreError::new(
            "ProviderInputMismatch",
            "The reported local delivery exceeds the frozen memory packet.",
        ));
    }
    Ok(())
}

fn terminal_state_with_cleanup(
    prior: MemoryJobStatus,
    outcome: ProviderOutcomeStatus,
    candidate_valid: bool,
    cleanup: Option<ProviderCleanup>,
) -> (MemoryJobStatus, Option<&'static str>) {
    if cleanup == Some(ProviderCleanup::Unresolved) {
        return (
            MemoryJobStatus::Interrupted,
            Some(if prior == MemoryJobStatus::Stopping {
                "author_stopped"
            } else {
                "cleanup_unresolved"
            }),
        );
    }
    if prior == MemoryJobStatus::Stopping {
        return (MemoryJobStatus::Stopped, Some("author_stopped"));
    }
    if outcome == ProviderOutcomeStatus::Stopped {
        return (MemoryJobStatus::Stopped, Some("provider_stopped"));
    }
    (
        match outcome {
            ProviderOutcomeStatus::Completed if candidate_valid => MemoryJobStatus::Completed,
            ProviderOutcomeStatus::Completed
            | ProviderOutcomeStatus::TimedOut
            | ProviderOutcomeStatus::OutputLimit
            | ProviderOutcomeStatus::Failed
            | ProviderOutcomeStatus::Stopped => MemoryJobStatus::Failed,
        },
        None,
    )
}

fn provider_outcome_as_str(outcome: ProviderOutcomeStatus) -> &'static str {
    match outcome {
        ProviderOutcomeStatus::Completed => "completed",
        ProviderOutcomeStatus::Stopped => "stopped",
        ProviderOutcomeStatus::TimedOut => "timed_out",
        ProviderOutcomeStatus::OutputLimit => "output_limit",
        ProviderOutcomeStatus::Failed => "failed",
    }
}

fn parse_provider_outcome(value: &str) -> CoreResult<ProviderOutcomeStatus> {
    match value {
        "completed" => Ok(ProviderOutcomeStatus::Completed),
        "stopped" => Ok(ProviderOutcomeStatus::Stopped),
        "timed_out" => Ok(ProviderOutcomeStatus::TimedOut),
        "output_limit" => Ok(ProviderOutcomeStatus::OutputLimit),
        "failed" => Ok(ProviderOutcomeStatus::Failed),
        _ => Err(CoreError::new(
            "InvalidProject",
            "The saved memory result has an unknown provider outcome.",
        )),
    }
}

fn provider_cleanup_as_str(cleanup: ProviderCleanup) -> &'static str {
    match cleanup {
        ProviderCleanup::Settled => "settled",
        ProviderCleanup::Unresolved => "unresolved",
    }
}

fn parse_provider_cleanup(value: &str) -> CoreResult<ProviderCleanup> {
    match value {
        "settled" => Ok(ProviderCleanup::Settled),
        "unresolved" => Ok(ProviderCleanup::Unresolved),
        _ => Err(CoreError::new(
            "InvalidProject",
            "The saved memory result has an unknown cleanup state.",
        )),
    }
}

fn result_matches(saved: &MemoryResult, request: &CompleteMemory) -> bool {
    saved.event_id == request.event_id
        && saved.raw_output.as_deref() == Some(request.raw_output.as_str())
        && saved.outcome == request.outcome
        && saved.confirmed_stdin_bytes == request.confirmed_stdin_bytes
        && saved.usage == request.usage
        && saved.cleanup == request.cleanup
        && saved.error == request.error
        && saved.effective_identity == request.effective_identity
}

type MemoryResultColumns = (
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

fn read_memory_result(
    db: &Connection,
    job_id: &str,
    reveal: bool,
) -> CoreResult<Option<MemoryResult>> {
    let row: Option<MemoryResultColumns> = db
        .query_row(
            "SELECT job_id,event_id,raw_output,raw_output_hash,candidate_json,outcome,confirmed_stdin_bytes,usage_json,cleanup,error,validation_error,effective_identity,created_at FROM memory_results WHERE job_id=?",
            [job_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))
            },
        )
        .optional()?;
    let Some((
        saved_job_id,
        event_id,
        raw_output,
        raw_output_hash,
        candidate_json,
        outcome,
        confirmed_stdin_bytes,
        usage_json,
        cleanup,
        error,
        validation_error,
        effective_identity,
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    if saved_job_id != job_id || sha256_hex(raw_output.as_bytes()) != raw_output_hash {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved memory result failed its raw-output fingerprint check.",
        ));
    }
    if raw_output.len() > MAX_RAW_BYTES {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved memory result exceeds the bounded output contract.",
        ));
    }
    check_id(&saved_job_id)?;
    check_id(&event_id)?;
    if reveal {
        validate_optional_text(error.as_deref(), "provider error")?;
        validate_optional_text(validation_error.as_deref(), "memory validation error")?;
        validate_optional_text(effective_identity.as_deref(), "provider identity")?;
    }
    let candidate = if reveal {
        candidate_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
    } else {
        None
    };
    let usage = usage_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?;
    let confirmed_stdin_bytes = confirmed_stdin_bytes
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| {
                    CoreError::new("InvalidProject", "The saved delivery count is negative.")
                })
                .map(|value| value.to_string())
        })
        .transpose()?;
    let (error, validation_error, effective_identity) = if reveal {
        (error, validation_error, effective_identity)
    } else {
        // Diagnostics are provider supplied text and may echo the frozen
        // chapter.  A policy-revoked read must redact every such field, not
        // only the raw output and candidate JSON.
        (None, None, None)
    };
    let result = MemoryResult {
        job_id: saved_job_id,
        event_id,
        raw_output: reveal.then_some(raw_output),
        outcome: parse_provider_outcome(&outcome)?,
        confirmed_stdin_bytes,
        usage,
        cleanup: cleanup.as_deref().map(parse_provider_cleanup).transpose()?,
        error,
        validation_error,
        candidate: if reveal { candidate } else { None },
        effective_identity,
        created_at,
    };
    Ok(Some(result))
}

type MemoryViewColumns = (
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    String,
    i64,
    String,
);

fn read_memory_view(db: &Connection, job_id: &str, reveal: bool) -> CoreResult<Option<MemoryView>> {
    let row: Option<MemoryViewColumns> = db
        .query_row(
            "SELECT id,job_id,project_id,operation_namespace,document_id,target_version,target_body_hash,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,candidate_json,installed_current,created_at FROM memory_views WHERE job_id=?",
            [job_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                    row.get(14)?,
                    row.get(15)?,
                ))
            },
        )
        .optional()?;
    let Some((
        id,
        saved_job_id,
        project_id,
        operation_namespace,
        document_id,
        target_version,
        target_body_hash,
        source_revision_id,
        source_body_hash,
        snapshot_id,
        packet_id,
        context_source_epoch,
        disclosure_policy_epoch,
        candidate_json,
        installed_current,
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    check_id(&id)?;
    let candidate = if reveal {
        Some(serde_json::from_str(&candidate_json).map_err(|error| {
            CoreError::new(
                "InvalidProject",
                &format!("The saved memory view is invalid: {error}"),
            )
        })?)
    } else {
        None
    };
    Ok(Some(MemoryView {
        id,
        job_id: saved_job_id,
        project_id: project_id.clone(),
        operation_namespace,
        document_id: document_id.clone(),
        target: Head {
            document_id: document_id.clone(),
            version: target_version.to_string(),
            body_hash: target_body_hash,
        },
        source: SourceRef {
            project_id,
            document_id: document_id.clone(),
            revision_id: source_revision_id,
            body_hash: source_body_hash,
        },
        snapshot_id,
        packet_id,
        context_source_epoch: context_source_epoch.to_string(),
        disclosure_policy_version: disclosure_policy_epoch.to_string(),
        candidate,
        current: installed_current != 0,
        source_changed: false,
        policy_available: reveal,
        historical: false,
        created_at,
    }))
}

fn resolve_view_current(db: &Connection, mut view: MemoryView) -> CoreResult<MemoryView> {
    let (current_project_id, current_namespace): (String, String) = db.query_row(
        "SELECT id,operation_namespace FROM project WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let owner_current =
        view.project_id == current_project_id && view.operation_namespace == current_namespace;
    view.historical = !owner_current;
    let current_policy = current_policy_version(db)?;
    if view.disclosure_policy_version != current_policy {
        view.current = false;
        view.source_changed = false;
        view.policy_available = false;
        view.candidate = None;
        return Ok(view);
    }
    let current_epoch = current_source_epoch(db)?;
    let current = read_document(db, &view.document_id)?;
    let basis_current = owner_current
        && current.head == view.target
        && current_epoch == view.context_source_epoch
        && view.disclosure_policy_version == current_policy;
    if !owner_current {
        // Rotated/recovered rows remain visible as bounded history, but can
        // never look current or authorize work in the new project identity.
        view.current = false;
        view.source_changed = false;
    } else {
        // "Current" describes the source/policy basis, not a singleton
        // winning candidate.  Several explicit refresh jobs may therefore
        // remain current for the same exact basis; their immutable job IDs
        // keep those candidates distinct for the caller to choose.
        view.current = view.current && basis_current;
        view.source_changed = !basis_current;
    }
    Ok(view)
}

fn read_memory_views_for_document(
    db: &Connection,
    document_id: &str,
    policy_version: &str,
) -> CoreResult<Vec<MemoryView>> {
    let mut statement = db.prepare(
        "SELECT v.job_id FROM memory_views v WHERE v.document_id=? ORDER BY v.created_at,v.id",
    )?;
    let ids = statement
        .query_map([document_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .map(|id| {
            read_memory_view(db, id, policy_version == current_policy_version(db)?)?
                .ok_or_else(|| {
                    CoreError::new("MemoryViewNotFound", "The memory view is not available.")
                })
                .and_then(|view| resolve_view_current(db, view))
        })
        .collect()
}

fn read_memory_views(db: &Connection, policy_version: &str) -> CoreResult<Vec<MemoryView>> {
    let mut statement =
        db.prepare("SELECT v.job_id FROM memory_views v ORDER BY v.created_at,v.id")?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .map(|id| {
            read_memory_view(db, id, policy_version == current_policy_version(db)?)?
                .ok_or_else(|| {
                    CoreError::new("MemoryViewNotFound", "The memory view is not available.")
                })
                .and_then(|view| resolve_view_current(db, view))
        })
        .collect()
}

/// Validate every retained memory row without consulting today's current
/// document head or disclosure policy.  Transfer/backup validation uses this
/// historical path; request-facing reads add the current project/lease/policy
/// checks above.
pub(crate) fn validate_memory_storage(db: &Connection) -> CoreResult<()> {
    for table in [
        "memory_jobs",
        "memory_results",
        "memory_views",
        "memory_view_sources",
    ] {
        let present: i64 = db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            [table],
            |row| row.get(0),
        )?;
        if present != 1 {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                &format!("The memory backup is missing its {table} table."),
            ));
        }
    }
    // Prepare each table up front so a schema that happens to contain no jobs
    // still proves that all immutable memory tables are present and readable.
    db.prepare("SELECT job_id FROM memory_results")?;
    db.prepare("SELECT job_id FROM memory_views")?;
    db.prepare("SELECT view_id FROM memory_view_sources")?;
    let mut statement = db.prepare(
        "SELECT id,project_id,operation_namespace,operation_id,payload_hash,request_json,target_document_id,target_version,target_body_hash,source_document_id,source_revision_id,source_body_hash,snapshot_id,packet_id,context_source_epoch,disclosure_policy_epoch,status,dispatch_state,stop_reason FROM memory_jobs ORDER BY id",
    )?;
    let jobs = statement
        .query_map([], |row| {
            Ok(MemoryJobRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                operation_namespace: row.get(2)?,
                operation_id: row.get(3)?,
                payload_hash: row.get(4)?,
                request_json: row.get(5)?,
                target_document_id: row.get(6)?,
                target_version: row.get(7)?,
                target_body_hash: row.get(8)?,
                source_document_id: row.get(9)?,
                source_revision_id: row.get(10)?,
                source_body_hash: row.get(11)?,
                snapshot_id: row.get(12)?,
                packet_id: row.get(13)?,
                context_source_epoch: row.get(14)?,
                disclosure_policy_epoch: row.get(15)?,
                status: row.get(16)?,
                dispatch_state: row.get(17)?,
                stop_reason: row.get(18)?,
                created_at: String::new(),
                updated_at: String::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for row in jobs {
        validate_memory_job_record(db, &row)?;
    }

    // Foreign-key enforcement is a connection setting.  Explicitly reject
    // orphan rows as well, so backup validation remains sound on a detached
    // connection that did not enable PRAGMA foreign_keys.
    let mut results = db.prepare("SELECT job_id FROM memory_results")?;
    let result_ids = results
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for job_id in result_ids {
        let known: Option<i64> = db
            .query_row("SELECT 1 FROM memory_jobs WHERE id=?", [&job_id], |row| {
                row.get(0)
            })
            .optional()?;
        if known.is_none() {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A memory result has no owning memory job.",
            ));
        }
    }
    let mut views = db.prepare("SELECT id,job_id FROM memory_views")?;
    let view_rows = views
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (view_id, job_id) in view_rows {
        let known: Option<i64> = db
            .query_row("SELECT 1 FROM memory_jobs WHERE id=?", [&job_id], |row| {
                row.get(0)
            })
            .optional()?;
        if known.is_none() {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view has no owning memory job.",
            ));
        }
        let source_rows: i64 = db.query_row(
            "SELECT COUNT(*) FROM memory_view_sources WHERE view_id=?",
            [&view_id],
            |row| row.get(0),
        )?;
        if source_rows != 1 {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view must have exactly one source dependency.",
            ));
        }
    }
    let mut source_rows = db.prepare("SELECT view_id FROM memory_view_sources")?;
    let source_view_ids = source_rows
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for view_id in source_view_ids {
        let known: Option<i64> = db
            .query_row("SELECT 1 FROM memory_views WHERE id=?", [&view_id], |row| {
                row.get(0)
            })
            .optional()?;
        if known.is_none() {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A memory view source has no owning generated view.",
            ));
        }
    }
    Ok(())
}

fn validate_memory_job_record(db: &Connection, row: &MemoryJobRow) -> CoreResult<()> {
    for value in [
        &row.id,
        &row.project_id,
        &row.operation_namespace,
        &row.operation_id,
        &row.target_document_id,
        &row.source_document_id,
        &row.source_revision_id,
        &row.snapshot_id,
        &row.packet_id,
    ] {
        check_id(value)?;
    }
    if !is_sha256(&row.payload_hash)
        || !is_sha256(&row.target_body_hash)
        || !is_sha256(&row.source_body_hash)
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory job has an invalid fingerprint.",
        ));
    }
    let stored: StoredMemoryRequest = serde_json::from_str(&row.request_json).map_err(|error| {
        CoreError::new(
            "InvalidMemoryStorage",
            &format!("A memory request is malformed: {error}"),
        )
    })?;
    if stored.project_id != row.project_id
        || stored.operation_namespace != row.operation_namespace
        || stored.operation_id != row.operation_id
        || stored.expected.document_id != row.target_document_id
        || stored.expected.version != row.target_version.to_string()
        || stored.expected.body_hash != row.target_body_hash
        || sha256_hex(serde_json::to_string(&stored)?.as_bytes()) != row.payload_hash
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory request does not match its immutable job identity.",
        ));
    }
    let (frozen, namespace) = validated_snapshot_record(db, &row.snapshot_id)?;
    if namespace != row.operation_namespace
        || frozen.snapshot.project_id != row.project_id
        || frozen.purpose != ContextPurpose::MemoryAnalysis
        || frozen.snapshot.target.document_id != row.target_document_id
        || frozen.snapshot.target.revision_id != row.source_revision_id
        || frozen.snapshot.target.body_hash != row.source_body_hash
        || frozen.snapshot.context_source_epoch != row.context_source_epoch.to_string()
        || frozen.policy.version != row.disclosure_policy_epoch.to_string()
        || frozen.snapshot.sources.len() != 1
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory job does not match its exact one-chapter snapshot.",
        ));
    }
    let source = read_source(db, &frozen, &row.source_revision_id)?;
    if source.descriptor.source.project_id != row.project_id
        || source.descriptor.source.document_id != row.source_document_id
        || source.descriptor.source.revision_id != row.source_revision_id
        || source.descriptor.source.body_hash != row.source_body_hash
        || row.target_body_hash != row.source_body_hash
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory job source is not the frozen chapter revision.",
        ));
    }
    let source_version: i64 = db.query_row(
        "SELECT source_working_version FROM revisions WHERE document_id=? AND id=?",
        params![row.source_document_id, row.source_revision_id],
        |query| query.get(0),
    )?;
    if source_version != row.target_version {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory source revision does not match the frozen target version.",
        ));
    }
    let packet = validated_packet_record(db, &row.packet_id)?;
    if packet.receipt.snapshot_id != row.snapshot_id
        || packet.receipt.source_handles.len() != 1
        || packet.receipt.source_handles[0] != row.source_revision_id
        || packet.options.provider_binding != stored.provider_binding
    {
        return Err(CoreError::new(
            "InvalidMemoryStorage",
            "A memory packet does not match its job, source, or producer binding.",
        ));
    }
    let status = MemoryJobStatus::parse(&row.status)?;
    let result = read_memory_result(db, &row.id, true)?;
    validate_memory_lifecycle(
        row,
        result.as_ref(),
        packet.options.provider_binding.is_some(),
    )
    .map_err(|_| {
        CoreError::new(
            "InvalidMemoryStorage",
            "A memory job has an invalid lifecycle, dispatch state, stop reason, or result.",
        )
    })?;
    if let Some(result) = &result {
        let delivery = CompleteMemory {
            owner: MemoryOwner {
                project_id: row.project_id.clone(),
                operation_namespace: row.operation_namespace.clone(),
                job_id: row.id.clone(),
            },
            event_id: result.event_id.clone(),
            raw_output: result.raw_output.clone().ok_or_else(|| {
                CoreError::new(
                    "InvalidMemoryStorage",
                    "A retained memory result has no raw output.",
                )
            })?,
            outcome: result.outcome,
            confirmed_stdin_bytes: result.confirmed_stdin_bytes.clone(),
            usage: result.usage.clone(),
            cleanup: result.cleanup,
            error: result.error.clone(),
            effective_identity: result.effective_identity.clone(),
        };
        validate_delivery(&delivery, &packet).map_err(|error| {
            CoreError::new(
                "InvalidMemoryStorage",
                &format!("A retained memory result has invalid delivery proof: {error}"),
            )
        })?;
        if let Some(raw) = result.raw_output.as_deref() {
            let validated = validate_navigation_digest(raw.as_bytes(), &source);
            match (&result.candidate, validated) {
                (Some(saved), Ok(actual)) if saved == &actual => {}
                (None, Err(_)) => {}
                _ => {
                    return Err(CoreError::new(
                        "InvalidMemoryStorage",
                        "A memory result candidate does not match its retained raw output.",
                    ));
                }
            }
        }
        if status == MemoryJobStatus::Completed
            && (result.outcome != ProviderOutcomeStatus::Completed || result.candidate.is_none())
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A completed memory job must retain a completed provider outcome and valid candidate.",
            ));
        }
        if status == MemoryJobStatus::Failed
            && result.outcome == ProviderOutcomeStatus::Completed
            && result.candidate.is_some()
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A failed memory job cannot retain a valid completed result as its terminal outcome.",
            ));
        }
    }
    let view = read_memory_view(db, &row.id, true)?;
    if let Some(view) = view {
        if status != MemoryJobStatus::Completed
            || view.project_id != row.project_id
            || view.operation_namespace != row.operation_namespace
            || view.document_id != row.target_document_id
            || view.target.version != row.target_version.to_string()
            || view.target.body_hash != row.target_body_hash
            || view.source.revision_id != row.source_revision_id
            || view.source.body_hash != row.source_body_hash
            || view.snapshot_id != row.snapshot_id
            || view.packet_id != row.packet_id
            || view.context_source_epoch != row.context_source_epoch.to_string()
            || view.disclosure_policy_version != row.disclosure_policy_epoch.to_string()
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view is not bound to its job basis.",
            ));
        }
        let candidate = view.candidate.ok_or_else(|| {
            CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view has no candidate.",
            )
        })?;
        if validate_navigation_digest(serde_json::to_string(&candidate)?.as_bytes(), &source)
            .is_err()
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view candidate is not valid for its source.",
            ));
        }
        if result.as_ref().and_then(|result| result.candidate.as_ref()) != Some(&candidate) {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view candidate differs from its terminal result.",
            ));
        }
        let source_count: i64 = db.query_row(
            "SELECT COUNT(*) FROM memory_view_sources WHERE view_id=?",
            [&view.id],
            |query| query.get(0),
        )?;
        if source_count != 1 {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view must retain its one exact source dependency.",
            ));
        }
        let linked: (String, String, String) = db.query_row(
            "SELECT document_id,revision_id,body_hash FROM memory_view_sources WHERE view_id=?",
            [&view.id],
            |query| Ok((query.get(0)?, query.get(1)?, query.get(2)?)),
        )?;
        if linked
            != (
                row.source_document_id.clone(),
                row.source_revision_id.clone(),
                row.source_body_hash.clone(),
            )
        {
            return Err(CoreError::new(
                "InvalidMemoryStorage",
                "A generated memory view dependency is not source-bound.",
            ));
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn packet_error(error: PacketError) -> CoreError {
    CoreError::new("InvalidContextPacket", &error.to_string())
}
