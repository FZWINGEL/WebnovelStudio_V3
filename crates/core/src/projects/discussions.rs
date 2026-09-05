//! Durable author-room chapter discussions.
//!
//! A discussion start is one actor-owned transaction: the target is checked,
//! its immutable context revision is frozen, the exact C2 packet is compiled,
//! and the user message plus queued run are inserted before the transaction
//! commits. Provider execution is intentionally outside this module. The
//! output methods below are the small durable boundary a later supervisor can
//! drive with deterministic or live events.

use super::*;
use crate::context::packet::{
    CompiledPacket, MockContextBudget, PacketError, PacketRequest, compile_packet,
};
use crate::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use crate::documents::{
    Endpoint, ScopeGrant, ScopeKind, ScopeValidationRequest, capture_scope, validate_scope,
};
use crate::projects::context_packets::PrepareContext;
use crate::projects::story_context::{FreezeStory, FrozenContext};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_INSTRUCTION_BYTES: usize = 64 * 1024;
const MAX_SCOPE_QUOTE_BYTES: usize = 256 * 1024;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 128 * 1024;
const MAX_PINNED_DOCUMENTS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionScopeInput {
    pub kind: ScopeKind,
    pub start: Option<Endpoint>,
    pub end: Option<Endpoint>,
    pub quote: String,
    pub source_body_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartDiscussion {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub instruction: String,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    pub budget: MockContextBudget,
    pub previous_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunOwner {
    pub project_id: String,
    pub operation_namespace: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscussionRunStatus {
    Queued,
    Running,
    Stopping,
    Completed,
    Stopped,
    Failed,
    Interrupted,
}

impl DiscussionRunStatus {
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
                "The discussion job has an unknown status.",
            )),
        }
    }
    fn active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Stopping)
    }
    fn terminal(self) -> bool {
        !self.active()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscussionMessageRole {
    User,
    Assistant,
}

impl DiscussionMessageRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
    fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The discussion message has an unknown role.",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionMessage {
    pub id: String,
    pub thread_id: String,
    pub run_id: Option<String>,
    pub role: DiscussionMessageRole,
    pub content: String,
    pub scope: Option<ScopeGrant>,
    pub packet_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionRun {
    pub id: String,
    pub thread_id: String,
    pub owner: RunOwner,
    pub operation_id: String,
    pub payload_hash: String,
    pub target: Head,
    pub packet_id: String,
    pub previous_run_id: Option<String>,
    pub status: DiscussionRunStatus,
    pub dispatch_state: String,
    pub sequence: String,
    pub output_text: String,
    pub stop_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionStart {
    pub thread_id: String,
    pub run: DiscussionRun,
    pub user_message: DiscussionMessage,
    pub packet: CompiledPacket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionDispatch {
    pub run: DiscussionRun,
    pub packet: CompiledPacket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionFail {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionDraft {
    pub document_id: String,
    pub version: String,
    pub text: String,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveDiscussionDraft {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub document_id: String,
    pub expected_version: String,
    pub text: String,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionView {
    pub document_id: String,
    pub thread_id: Option<String>,
    pub messages: Vec<DiscussionMessage>,
    pub runs: Vec<DiscussionRun>,
    pub draft: Option<DiscussionDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionOutputAppend {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub chunk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionFinish {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub assistant_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionStop {
    pub run: DiscussionRun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionBegin {
    pub owner: RunOwner,
}

pub(super) enum DiscussionCommand {
    Start(StartDiscussion, Reply<DiscussionStart>),
    Begin(DiscussionBegin, Reply<DiscussionDispatch>),
    MarkDelivered(RunOwner, Reply<DiscussionRun>),
    Append(DiscussionOutputAppend, Reply<DiscussionRun>),
    Finish(DiscussionFinish, Reply<DiscussionRun>),
    Fail(DiscussionFail, Reply<DiscussionRun>),
    Stop(ProjectAccess, String, Reply<DiscussionStop>),
    Read(ProjectAccess, String, Reply<DiscussionView>),
    SaveDraft(SaveDiscussionDraft, Reply<DiscussionDraft>),
}

impl ProjectSession {
    pub fn start_discussion(&self, request: StartDiscussion) -> CoreResult<DiscussionStart> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Start(request, reply)))
        })
    }

    pub fn begin_discussion_run(&self, request: DiscussionBegin) -> CoreResult<DiscussionDispatch> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Begin(request, reply)))
        })
    }

    pub fn mark_discussion_delivered(&self, owner: RunOwner) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::MarkDelivered(owner, reply)))
        })
    }

    pub fn append_discussion_output(
        &self,
        request: DiscussionOutputAppend,
    ) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Append(request, reply)))
        })
    }

    pub fn finish_discussion(&self, request: DiscussionFinish) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Finish(request, reply)))
        })
    }

    pub fn fail_discussion_run(&self, request: DiscussionFail) -> CoreResult<DiscussionRun> {
        self.request(|reply| Command::Discussion(Box::new(DiscussionCommand::Fail(request, reply))))
    }

    pub fn stop_discussion(
        &self,
        access: ProjectAccess,
        run_id: String,
    ) -> CoreResult<DiscussionStop> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Stop(access, run_id, reply)))
        })
    }

    pub fn read_discussion(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<DiscussionView> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Read(
                access,
                document_id,
                reply,
            )))
        })
    }

    pub fn save_discussion_draft(
        &self,
        request: SaveDiscussionDraft,
    ) -> CoreResult<DiscussionDraft> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::SaveDraft(request, reply)))
        })
    }
}

impl OwnedProject {
    pub(super) fn handle_discussion(&mut self, command: DiscussionCommand) {
        macro_rules! mutate {
            ($reply:expr, $operation:expr) => {{
                let result = $operation;
                self.fence_uncertain(&result);
                let _ = $reply.send(result);
            }};
        }
        match command {
            DiscussionCommand::Start(request, reply) => {
                mutate!(reply, self.start_discussion(request));
            }
            DiscussionCommand::Begin(request, reply) => {
                mutate!(reply, self.begin_discussion_run(request));
            }
            DiscussionCommand::MarkDelivered(owner, reply) => {
                mutate!(reply, self.mark_discussion_delivered(owner));
            }
            DiscussionCommand::Append(request, reply) => {
                mutate!(reply, self.append_discussion_output(request));
            }
            DiscussionCommand::Finish(request, reply) => {
                mutate!(reply, self.finish_discussion(request));
            }
            DiscussionCommand::Fail(request, reply) => {
                mutate!(reply, self.fail_discussion_run(request));
            }
            DiscussionCommand::Stop(access, run_id, reply) => {
                mutate!(reply, self.stop_discussion(access, run_id));
            }
            DiscussionCommand::Read(access, document_id, reply) => {
                let _ = reply.send(self.read_discussion(access, document_id));
            }
            DiscussionCommand::SaveDraft(request, reply) => {
                mutate!(reply, self.save_discussion_draft(request));
            }
        }
    }

    /// Start is intentionally an actor method. The parent `Command` enum can
    /// wire this method after the C2 persistence checkpoint without nesting a
    /// freeze transaction or a packet transaction.
    pub(super) fn start_discussion(
        &mut self,
        request: StartDiscussion,
    ) -> CoreResult<DiscussionStart> {
        self.check_access(&request.access)?;
        validate_start(&request)?;
        let payload_hash = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT id,payload_hash FROM discussion_runs WHERE project_id=? AND operation_namespace=? AND operation_id=?",
                params![request.access.project_id, request.access.operation_namespace, request.operation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((run_id, previous_payload)) = existing {
            if previous_payload != payload_hash {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This discussion operation was already used for a different request.",
                ));
            }
            let result = read_start(&tx, &run_id)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(result);
        }

        let context_request = FreezeStory {
            access: request.access.clone(),
            operation_id: new_id(),
            expected: request.expected.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::Discuss,
            policy: current_discussion_policy(&tx)?,
        };
        let context_payload = logical_hash(&context_request)?;
        let frozen_context =
            story_context::freeze_discussion_story_at(&tx, &context_request, &context_payload)?;
        let target = read_revision(&tx, &frozen_context.snapshot.target.revision_id)?;
        let mandatory_handles =
            resolve_pinned_handles(&frozen_context, &request.pinned_document_ids)?;
        let source_reads = frozen_context
            .snapshot
            .sources
            .iter()
            .map(|source| story_context::read_source(&tx, &frozen_context, &source.handle))
            .collect::<CoreResult<Vec<_>>>()?;
        let scope = capture_discussion_scope(request.scope.as_ref(), &target.body)?;
        let packet = compile_packet(&PacketRequest {
            packet_id: new_id(),
            session_id: new_id(),
            invocation_ordinal: "0".into(),
            frozen: frozen_context.clone(),
            instruction: request.instruction.clone(),
            sources: source_reads,
            mandatory_handles: mandatory_handles.clone(),
            scope: scope.clone(),
            budget: request.budget.clone(),
        })
        .map_err(packet_error)?;
        insert_packet(&tx, &packet, &request, &mandatory_handles, scope.as_ref())?;
        guidance::consume_request_guidance_at(
            &tx,
            &frozen_context.snapshot.snapshot_id,
            &frozen_context.guidance,
        )?;

        let thread_id = ensure_thread(&tx, &request.access, &request.expected.document_id)?;
        validate_previous_run(
            &tx,
            request.previous_run_id.as_deref(),
            &request.access,
            &request.expected.document_id,
        )?;
        let run_id = new_id();
        tx.execute(
            "INSERT INTO discussion_runs(id,thread_id,project_id,operation_namespace,operation_id,payload_hash,target_document_id,target_version,target_body_hash,packet_id,previous_run_id,status,sequence,output_text) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,0,'')",
            params![
                run_id,
                thread_id,
                request.access.project_id,
                request.access.operation_namespace,
                request.operation_id,
                payload_hash,
                request.expected.document_id,
                parse_version(&request.expected.version)?,
                request.expected.body_hash,
                packet.receipt.packet_id,
                request.previous_run_id,
                DiscussionRunStatus::Queued.as_str(),
            ],
        )?;
        let user_message_id = new_id();
        tx.execute(
            "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,scope_json,packet_id) VALUES(?,?,?,?,?,?,?)",
            params![
                user_message_id,
                thread_id,
                run_id,
                DiscussionMessageRole::User.as_str(),
                request.instruction,
                scope.as_ref().map(serde_json::to_string).transpose()?,
                packet.receipt.packet_id,
            ],
        )?;
        let run = read_run(&tx, &run_id)?;
        let user_message = read_message(&tx, &user_message_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(DiscussionStart {
            thread_id,
            run,
            user_message,
            packet,
        })
    }

    pub(super) fn append_discussion_output(
        &mut self,
        request: DiscussionOutputAppend,
    ) -> CoreResult<DiscussionRun> {
        validate_output_event(&request.owner, &request.event_id, &request.chunk)?;
        validate_runtime_owner(self, &request.owner)?;
        let expected = parse_version(&request.expected_sequence)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        if let Some((kind, chunk, event_sequence)) =
            existing_event(&tx, &request.owner.run_id, &request.event_id)?
        {
            if kind == "chunk"
                && chunk == request.chunk
                && expected.checked_add(1) == Some(event_sequence)
            {
                tx.commit().map_err(CoreError::uncertain)?;
                return Ok(current);
            }
            return Err(CoreError::new(
                "EventIdReused",
                "An output event ID was reused with different content or sequence.",
            ));
        }
        ensure_run_started(current.status)?;
        let sequence = parse_version(&current.sequence)?;
        if expected != sequence {
            return Err(CoreError::new(
                "SequenceConflict",
                "The output sequence is stale; reconcile the run before retrying.",
            ));
        }
        let next_output = append_text(&current.output_text, &request.chunk, MAX_OUTPUT_BYTES)?;
        let next = sequence
            .checked_add(1)
            .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
        tx.execute(
            "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
            params![request.owner.run_id, next, request.event_id, "chunk", request.chunk],
        )?;
        let changed = tx.execute(
            "UPDATE discussion_runs SET status='running',sequence=?,output_text=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running' AND sequence=?",
            params![next, next_output, request.owner.run_id, request.owner.project_id, request.owner.operation_namespace, sequence],
        )?;
        if changed != 1 {
            return Err(CoreError::new(
                "SequenceConflict",
                "The run changed before the output event was committed.",
            ));
        }
        let result = read_run(&tx, &request.owner.run_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }

    /// Claim a queued run for one dispatcher. The claim is separate from the
    /// persisted run state, so a later provider supervisor can mark delivery
    /// without confusing a durable queued job with model execution.
    pub(super) fn begin_discussion_run(
        &mut self,
        request: DiscussionBegin,
    ) -> CoreResult<DiscussionDispatch> {
        check_id(&request.owner.project_id)?;
        check_id(&request.owner.operation_namespace)?;
        check_id(&request.owner.run_id)?;
        validate_runtime_owner(self, &request.owner)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        match current.status {
            DiscussionRunStatus::Queued => {
                let (snapshot_source_epoch, snapshot_policy_epoch): (i64, i64) = tx.query_row(
                    "SELECT cp.snapshot_id,ss.context_source_epoch,ss.disclosure_policy_epoch FROM discussion_runs dr JOIN context_packets cp ON cp.id=dr.packet_id JOIN story_snapshots ss ON ss.id=cp.snapshot_id WHERE dr.id=?",
                    [&request.owner.run_id],
                    |row| Ok((row.get(1)?, row.get(2)?)),
                )?;
                let (source_epoch, policy_epoch): (i64, i64) = tx.query_row(
                    "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                if snapshot_source_epoch != source_epoch || snapshot_policy_epoch != policy_epoch {
                    let stale_message = "The story changed before this discussion started; the saved response was not dispatched.";
                    let _ = seal_run(
                        &tx,
                        &current,
                        DiscussionRunStatus::Failed,
                        "context_stale",
                        &format!("system-stale-{}", current.id),
                        stale_message,
                    )?;
                    tx.commit().map_err(CoreError::uncertain)?;
                    return Err(CoreError::new(
                        "ContextChanged",
                        "The queued discussion is historical because its frozen context is no longer current.",
                    ));
                }
                let changed = tx.execute(
                    "UPDATE discussion_runs SET status='running',dispatch_state='claimed',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='queued' AND dispatch_state='pending'",
                    params![request.owner.run_id, request.owner.project_id, request.owner.operation_namespace],
                )?;
                if changed != 1 {
                    return Err(CoreError::new(
                        "RunAlreadyStarted",
                        "Another dispatcher already claimed this discussion run.",
                    ));
                }
            }
            DiscussionRunStatus::Running => {
                return Err(CoreError::new(
                    "RunAlreadyStarted",
                    "This discussion run is already claimed by a dispatcher.",
                ));
            }
            _ => {
                return Err(CoreError::new(
                    "RunSealed",
                    "This discussion run is no longer dispatchable.",
                ));
            }
        }
        let run = read_run(&tx, &request.owner.run_id)?;
        let packet_access = ProjectAccess {
            project_id: request.owner.project_id.clone(),
            operation_namespace: request.owner.operation_namespace.clone(),
            session: String::new(),
            writer_lease: String::new(),
        };
        let packet = context_packets::read_context_packet_at(&tx, &packet_access, &run.packet_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(DiscussionDispatch { run, packet })
    }

    pub(super) fn mark_discussion_delivered(
        &mut self,
        owner: RunOwner,
    ) -> CoreResult<DiscussionRun> {
        check_id(&owner.project_id)?;
        check_id(&owner.operation_namespace)?;
        check_id(&owner.run_id)?;
        validate_runtime_owner(self, &owner)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &owner.run_id)?;
        validate_owner(&current, &owner)?;
        if current.status != DiscussionRunStatus::Running {
            return Err(CoreError::new(
                "RunSealed",
                "Only a running discussion can be marked delivered.",
            ));
        }
        tx.execute("UPDATE discussion_runs SET dispatch_state='delivered',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running'", params![owner.run_id,owner.project_id,owner.operation_namespace])?;
        let run = read_run(&tx, &owner.run_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(run)
    }

    pub(super) fn finish_discussion(
        &mut self,
        request: DiscussionFinish,
    ) -> CoreResult<DiscussionRun> {
        validate_output_event(&request.owner, &request.event_id, &request.assistant_text)?;
        validate_runtime_owner(self, &request.owner)?;
        let expected = parse_version(&request.expected_sequence)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        if let Some((kind, text, event_sequence)) =
            existing_event(&tx, &request.owner.run_id, &request.event_id)?
        {
            if kind == "terminal"
                && text == request.assistant_text
                && expected.checked_add(1) == Some(event_sequence)
                && current.status == DiscussionRunStatus::Completed
            {
                tx.commit().map_err(CoreError::uncertain)?;
                return Ok(current);
            }
            return Err(CoreError::new(
                "EventIdReused",
                "A terminal event ID was reused with a different outcome, content, or sequence.",
            ));
        }
        ensure_run_started(current.status)?;
        let sequence = parse_version(&current.sequence)?;
        if expected != sequence {
            return Err(CoreError::new(
                "SequenceConflict",
                "The output sequence is stale; reconcile the run before retrying.",
            ));
        }
        let next = sequence
            .checked_add(1)
            .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
        tx.execute(
            "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
            params![request.owner.run_id, next, request.event_id, "terminal", request.assistant_text],
        )?;
        let changed = tx.execute(
            "UPDATE discussion_runs SET status='completed',sequence=?,output_text=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running' AND sequence=?",
            params![next, request.assistant_text, request.owner.run_id, request.owner.project_id, request.owner.operation_namespace, sequence],
        )?;
        if changed != 1 {
            return Err(CoreError::new(
                "SequenceConflict",
                "The run changed before completion was committed.",
            ));
        }
        let message_id = new_id();
        tx.execute(
            "INSERT INTO discussion_messages(id,thread_id,run_id,role,content) VALUES(?,?,?,?,?)",
            params![
                message_id,
                current.thread_id,
                request.owner.run_id,
                DiscussionMessageRole::Assistant.as_str(),
                request.assistant_text
            ],
        )?;
        let result = read_run(&tx, &request.owner.run_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }

    /// Seal a provider or supervisor failure while preserving every chunk
    /// already accepted. The failure explanation is an immutable assistant
    /// message in the same transaction as the terminal run event.
    pub(super) fn fail_discussion_run(
        &mut self,
        request: DiscussionFail,
    ) -> CoreResult<DiscussionRun> {
        check_id(&request.owner.project_id)?;
        check_id(&request.owner.operation_namespace)?;
        validate_runtime_owner(self, &request.owner)?;
        check_id(&request.event_id)?;
        check_id(&request.owner.run_id)?;
        let expected = parse_version(&request.expected_sequence)?;
        let reason = request.reason.trim();
        if reason.is_empty() || reason.len() > MAX_EVENT_BYTES {
            return Err(CoreError::new(
                "InvalidRequest",
                "A discussion failure reason must be nonempty and at most 128 KiB.",
            ));
        }
        let terminal_text = format!("Discussion failed: {reason}");
        if terminal_text.len() > MAX_EVENT_BYTES {
            return Err(CoreError::new(
                "InvalidRequest",
                "The discussion failure message exceeds the durable event limit.",
            ));
        }
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        if let Some((kind, text, event_sequence)) =
            existing_event(&tx, &request.owner.run_id, &request.event_id)?
        {
            if kind == "terminal"
                && text == terminal_text
                && expected.checked_add(1) == Some(event_sequence)
                && current.status == DiscussionRunStatus::Failed
            {
                tx.commit().map_err(CoreError::uncertain)?;
                return Ok(current);
            }
            return Err(CoreError::new(
                "EventIdReused",
                "A terminal event ID was reused with a different outcome, content, or sequence.",
            ));
        }
        let sequence = parse_version(&current.sequence)?;
        if expected != sequence {
            return Err(CoreError::new(
                "SequenceConflict",
                "The output sequence changed before failure was recorded.",
            ));
        }
        let result = seal_run(
            &tx,
            &current,
            DiscussionRunStatus::Failed,
            reason,
            &request.event_id,
            &terminal_text,
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }

    pub(super) fn stop_discussion(
        &mut self,
        access: ProjectAccess,
        run_id: String,
    ) -> CoreResult<DiscussionStop> {
        self.check_access(&access)?;
        check_id(&run_id)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &run_id)?;
        if current.owner.project_id != access.project_id
            || current.owner.operation_namespace != access.operation_namespace
        {
            return Err(CoreError::new(
                "DiscussionProjectMismatch",
                "This run belongs to another project session.",
            ));
        }
        let run = if current.status.active() {
            seal_run(
                &tx,
                &current,
                DiscussionRunStatus::Stopped,
                "author_stopped",
                &format!("system-stop-{}", current.id),
                "The author stopped this discussion before a complete response was received.",
            )?
        } else {
            current
        };
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(DiscussionStop { run })
    }

    /// The open path calls this once after migration. Active jobs are not
    /// replayed; they become inspectable interrupted history.
    pub(super) fn recover_interrupted_discussions(&mut self) -> CoreResult<u32> {
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = tx.prepare(
            "SELECT id FROM discussion_runs WHERE status IN ('queued','running','stopping') ORDER BY created_at,id",
        )?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for id in &ids {
            let current = read_run(&tx, id)?;
            seal_run(
                &tx,
                &current,
                DiscussionRunStatus::Interrupted,
                "project_reopened",
                &format!("system-interrupted-{}", current.id),
                "The project was reopened before this discussion produced a complete response.",
            )?;
        }
        tx.commit().map_err(CoreError::uncertain)?;
        u32::try_from(ids.len())
            .map_err(|_| CoreError::new("InvalidProject", "Too many discussion jobs."))
    }

    pub(super) fn read_discussion(
        &self,
        access: ProjectAccess,
        document_id: String,
    ) -> CoreResult<DiscussionView> {
        self.check_access(&access)?;
        check_id(&document_id)?;
        let db = self.db()?;
        let thread_id: Option<String> = db
            .query_row(
                "SELECT id FROM discussion_threads WHERE project_id=? AND operation_namespace=? AND document_id=?",
                params![access.project_id, access.operation_namespace, document_id],
                |row| row.get(0),
            )
            .optional()?;
        // Copies retain readable history. Run mutations still require the
        // current project and operation namespace, never a historical owner.
        read_document(db, &document_id)?;
        let mut runs_statement = db.prepare("SELECT dr.id FROM discussion_runs dr JOIN discussion_threads dt ON dt.id=dr.thread_id WHERE dt.document_id=? ORDER BY dr.rowid")?;
        let run_ids = runs_statement
            .query_map([&document_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let runs = run_ids
            .iter()
            .map(|id| read_run(db, id))
            .collect::<CoreResult<Vec<_>>>()?;
        let mut message_statement = db.prepare(
            "SELECT dm.id FROM discussion_messages dm JOIN discussion_threads dt ON dt.id=dm.thread_id WHERE dt.document_id=? ORDER BY dm.rowid",
        )?;
        let message_ids = message_statement
            .query_map([&document_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let messages = message_ids
            .iter()
            .map(|id| read_message(db, id))
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(DiscussionView {
            document_id: document_id.clone(),
            thread_id,
            messages,
            runs,
            draft: read_draft(db, &access, &document_id)?,
        })
    }

    pub(super) fn save_discussion_draft(
        &mut self,
        request: SaveDiscussionDraft,
    ) -> CoreResult<DiscussionDraft> {
        self.check_access(&request.access)?;
        check_id(&request.document_id)?;
        check_id(&request.operation_id)?;
        let expected = parse_version(&request.expected_version)?;
        if request.text.len() > MAX_OUTPUT_BYTES
            || request.pinned_document_ids.len() > MAX_PINNED_DOCUMENTS
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "The discussion draft is too large or has too many pinned documents.",
            ));
        }
        for id in &request.pinned_document_ids {
            check_id(id)?;
        }
        if let Some(scope) = &request.scope
            && scope.quote.len() > MAX_SCOPE_QUOTE_BYTES
        {
            return Err(CoreError::new(
                "InvalidScope",
                "The discussion draft scope quote is too large.",
            ));
        }
        let payload_hash = logical_hash(&request)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String,String)> = tx.query_row("SELECT payload_hash,result_json FROM discussion_draft_receipts WHERE project_id=? AND operation_namespace=? AND operation_id=?", params![request.access.project_id,request.access.operation_namespace,request.operation_id], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
        if let Some((previous, result)) = existing {
            if previous != payload_hash {
                return Err(CoreError::new(
                    "OperationIdReusedWithDifferentPayload",
                    "This draft operation was already used for different content.",
                ));
            }
            let draft: DiscussionDraft = serde_json::from_str(&result)?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(draft);
        }
        read_document(&tx, &request.document_id)?;
        let current: Option<i64> = tx.query_row("SELECT version FROM discussion_drafts WHERE project_id=? AND operation_namespace=? AND document_id=?", params![request.access.project_id,request.access.operation_namespace,request.document_id], |row| row.get(0)).optional()?;
        let current = current.unwrap_or(0);
        if current != expected {
            return Err(CoreError::new(
                "DraftVersionConflict",
                "The composer draft changed; reload it before saving.",
            ));
        }
        let next = current
            .checked_add(1)
            .ok_or_else(|| CoreError::new("InvalidRequest", "The draft version is exhausted."))?;
        tx.execute("INSERT INTO discussion_drafts(project_id,operation_namespace,document_id,version,text,scope_json,pinned_document_ids_json) VALUES(?,?,?,?,?,?,?) ON CONFLICT(project_id,operation_namespace,document_id) DO UPDATE SET version=excluded.version,text=excluded.text,scope_json=excluded.scope_json,pinned_document_ids_json=excluded.pinned_document_ids_json,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![request.access.project_id,request.access.operation_namespace,request.document_id,next,request.text,request.scope.as_ref().map(serde_json::to_string).transpose()?,serde_json::to_string(&request.pinned_document_ids)?])?;
        let draft = read_draft(&tx, &request.access, &request.document_id)?.ok_or_else(|| {
            CoreError::new(
                "PersistenceUnavailable",
                "The saved composer draft could not be read.",
            )
        })?;
        tx.execute("INSERT INTO discussion_draft_receipts(project_id,operation_namespace,operation_id,document_id,expected_version,payload_hash,result_json) VALUES(?,?,?,?,?,?,?)", params![request.access.project_id,request.access.operation_namespace,request.operation_id,request.document_id,expected,payload_hash,serde_json::to_string(&draft)?])?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(draft)
    }
}

fn validate_start(request: &StartDiscussion) -> CoreResult<()> {
    check_id(&request.operation_id)?;
    check_id(&request.expected.document_id)?;
    if request.instruction.trim().is_empty() || request.instruction.len() > MAX_INSTRUCTION_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "A discussion instruction must be nonempty and at most 64 KiB.",
        ));
    }
    if request.pinned_document_ids.len() > MAX_PINNED_DOCUMENTS {
        return Err(CoreError::new(
            "InvalidRequest",
            "A discussion may pin at most 64 source documents.",
        ));
    }
    for id in &request.pinned_document_ids {
        check_id(id)?;
    }
    if let Some(run_id) = &request.previous_run_id {
        check_id(run_id)?;
    }
    if let Some(scope) = &request.scope
        && (scope.quote.len() > MAX_SCOPE_QUOTE_BYTES
            || scope.source_body_hash.len() != 64
            || !scope
                .source_body_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(CoreError::new(
            "InvalidScope",
            "A discussion scope quote or source hash is invalid.",
        ));
    }
    Ok(())
}

fn current_discussion_policy(tx: &Connection) -> CoreResult<InformationPolicy> {
    let policy_epoch: i64 = tx.query_row(
        "SELECT disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    Ok(InformationPolicy {
        version: policy_epoch.to_string(),
        audience: Audience::AuthorRoom,
        reader_frontier: None,
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    })
}

fn resolve_pinned_handles(
    frozen: &FrozenContext,
    pinned_document_ids: &[String],
) -> CoreResult<Vec<String>> {
    let mut handles = Vec::with_capacity(pinned_document_ids.len());
    for document_id in pinned_document_ids {
        let handle = frozen
            .snapshot
            .sources
            .iter()
            .find(|source| source.source.document_id == *document_id)
            .map(|source| source.handle.clone())
            .ok_or_else(|| {
                CoreError::new(
                    "SourceNotFound",
                    "A pinned document is not in the frozen context.",
                )
            })?;
        if handles.iter().any(|existing| existing == &handle) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A document was pinned more than once.",
            ));
        }
        handles.push(handle);
    }
    Ok(handles)
}

fn capture_discussion_scope(
    input: Option<&DiscussionScopeInput>,
    target: &Value,
) -> CoreResult<Option<ScopeGrant>> {
    let Some(input) = input else {
        return Ok(None);
    };
    let captured = capture_scope(
        target,
        ScopeGrant {
            kind: input.kind,
            start: input.start.clone(),
            end: input.end.clone(),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .map_err(|message| CoreError::new("InvalidScope", &message))?;
    if captured.source_hash != input.source_body_hash || captured.quote != input.quote {
        return Err(CoreError::new(
            "InvalidScope",
            "The scope quote or source hash does not match the exact target revision.",
        ));
    }
    validate_scope(&ScopeValidationRequest {
        source_snapshot: target.clone(),
        result_snapshot: target.clone(),
        scope: captured.clone(),
    })
    .map_err(|message| CoreError::new("InvalidScope", &message))?;
    Ok(Some(captured))
}

fn insert_packet(
    tx: &Connection,
    packet: &CompiledPacket,
    request: &StartDiscussion,
    mandatory_handles: &[String],
    scope: Option<&ScopeGrant>,
) -> CoreResult<()> {
    // context_packets is validated on packet reads and transfers. Persist its
    // canonical PrepareContext envelope rather than the larger discussion
    // request so the packet remains readable through the shared C2 contract.
    let prepared = PrepareContext {
        access: request.access.clone(),
        operation_id: request.operation_id.clone(),
        snapshot_id: packet.receipt.snapshot_id.clone(),
        instruction: request.instruction.clone(),
        mandatory_handles: mandatory_handles.to_vec(),
        scope: scope.cloned(),
        budget: request.budget.clone(),
    };
    let payload_hash = logical_hash(&prepared)?;
    let request_json = serde_json::to_string(&prepared)?;
    let packet_json = serde_json::to_string(packet)?;
    tx.execute("INSERT INTO context_packets(id,project_id,operation_namespace,operation_id,payload_hash,request_json,snapshot_id,session_id,invocation_ordinal,packet_json,packet_hash,input_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)", params![packet.receipt.packet_id, request.access.project_id, request.access.operation_namespace, request.operation_id, payload_hash, request_json, packet.receipt.snapshot_id, packet.receipt.session_id, parse_version(&packet.receipt.invocation_ordinal)?, packet_json, sha256_hex(packet_json.as_bytes()), packet.receipt.input_hash])?;
    Ok(())
}

fn ensure_thread(tx: &Connection, access: &ProjectAccess, document_id: &str) -> CoreResult<String> {
    tx.execute("INSERT INTO discussion_threads(id,project_id,operation_namespace,document_id) VALUES(?,?,?,?) ON CONFLICT(project_id,operation_namespace,document_id) DO NOTHING", params![new_id(), access.project_id, access.operation_namespace, document_id])?;
    tx.query_row("SELECT id FROM discussion_threads WHERE project_id=? AND operation_namespace=? AND document_id=?", params![access.project_id,access.operation_namespace,document_id], |row| row.get(0)).map_err(CoreError::from)
}

fn validate_previous_run(
    tx: &Connection,
    previous: Option<&str>,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let row: Option<(String,String,String,String)> = tx.query_row("SELECT project_id,operation_namespace,target_document_id,status FROM discussion_runs WHERE id=?", [previous], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
    let Some((project, namespace, target, status)) = row else {
        return Err(CoreError::new(
            "PreviousRunNotFound",
            "The previous discussion run is not available.",
        ));
    };
    if project != access.project_id
        || namespace != access.operation_namespace
        || target != document_id
    {
        return Err(CoreError::new(
            "PreviousRunMismatch",
            "A previous discussion must belong to the same project and target document.",
        ));
    }
    if !DiscussionRunStatus::parse(&status)?.terminal() {
        return Err(CoreError::new(
            "PreviousRunActive",
            "The previous discussion must be terminal before it can be used as context.",
        ));
    }
    Ok(())
}

fn read_start(db: &Connection, run_id: &str) -> CoreResult<DiscussionStart> {
    let run = read_run(db, run_id)?;
    let user_message = db.query_row("SELECT id FROM discussion_messages WHERE run_id=? AND role='user' ORDER BY created_at,id LIMIT 1", [run_id], |row| row.get::<_,String>(0)).map_err(CoreError::from).and_then(|id| read_message(db, &id))?;
    let packet = context_packets::read_context_packet_at(
        db,
        &ProjectAccess {
            project_id: run.owner.project_id.clone(),
            operation_namespace: run.owner.operation_namespace.clone(),
            session: String::new(),
            writer_lease: String::new(),
        },
        &run.packet_id,
    )?;
    Ok(DiscussionStart {
        thread_id: run.thread_id.clone(),
        run,
        user_message,
        packet,
    })
}

fn read_run(db: &Connection, run_id: &str) -> CoreResult<DiscussionRun> {
    check_id(run_id)?;
    type RunRow = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        Option<String>,
        String,
        String,
        i64,
        String,
        Option<String>,
        String,
        String,
    );
    let row: RunRow = db.query_row("SELECT id,thread_id,project_id,operation_namespace,operation_id,payload_hash,target_document_id,target_version,target_body_hash,packet_id,previous_run_id,status,dispatch_state,sequence,output_text,stop_reason,created_at,updated_at FROM discussion_runs WHERE id=?", [run_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?,row.get(10)?,row.get(11)?,row.get(12)?,row.get(13)?,row.get(14)?,row.get(15)?,row.get(16)?,row.get(17)?))).optional()?.ok_or_else(|| CoreError::new("DiscussionRunNotFound", "The discussion run is not available."))?;
    Ok(DiscussionRun {
        id: row.0.clone(),
        thread_id: row.1,
        owner: RunOwner {
            project_id: row.2,
            operation_namespace: row.3,
            run_id: row.0,
        },
        operation_id: row.4,
        payload_hash: row.5,
        target: Head {
            document_id: row.6,
            version: row.7.to_string(),
            body_hash: row.8,
        },
        packet_id: row.9,
        previous_run_id: row.10,
        status: DiscussionRunStatus::parse(&row.11)?,
        dispatch_state: row.12,
        sequence: row.13.to_string(),
        output_text: row.14,
        stop_reason: row.15,
        created_at: row.16,
        updated_at: row.17,
    })
}

fn read_message(db: &Connection, message_id: &str) -> CoreResult<DiscussionMessage> {
    type MessageRow = (
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    );
    let row: MessageRow = db.query_row("SELECT id,thread_id,run_id,role,content,scope_json,packet_id,created_at FROM discussion_messages WHERE id=?", [message_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?))).optional()?.ok_or_else(|| CoreError::new("DiscussionMessageNotFound", "The discussion message is not available."))?;
    Ok(DiscussionMessage {
        id: row.0,
        thread_id: row.1,
        run_id: row.2,
        role: DiscussionMessageRole::parse(&row.3)?,
        content: row.4,
        scope: row.5.map(|json| serde_json::from_str(&json)).transpose()?,
        packet_id: row.6,
        created_at: row.7,
    })
}

fn read_draft(
    db: &Connection,
    access: &ProjectAccess,
    document_id: &str,
) -> CoreResult<Option<DiscussionDraft>> {
    let row: Option<(String, i64, String, Option<String>, String, String)> = db
        .query_row(
            "SELECT document_id,version,text,scope_json,pinned_document_ids_json,updated_at FROM discussion_drafts WHERE project_id=? AND operation_namespace=? AND document_id=?",
            params![access.project_id, access.operation_namespace, document_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?;
    let Some((document_id, version, text, scope_json, pins_json, updated_at)) = row else {
        return Ok(None);
    };
    Ok(Some(DiscussionDraft {
        document_id,
        version: parse_stored_version(version)?,
        text,
        scope: scope_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
        pinned_document_ids: serde_json::from_str(&pins_json)?,
        updated_at,
    }))
}

fn existing_event(
    db: &Connection,
    run_id: &str,
    event_id: &str,
) -> CoreResult<Option<(String, String, i64)>> {
    db.query_row(
        "SELECT kind,chunk,sequence FROM discussion_output_events WHERE run_id=? AND event_id=?",
        params![run_id, event_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
    .map_err(CoreError::from)
}

fn validate_owner(run: &DiscussionRun, owner: &RunOwner) -> CoreResult<()> {
    if &run.owner != owner {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "The output owner does not match the persisted run.",
        ));
    }
    Ok(())
}

fn validate_runtime_owner(project: &OwnedProject, owner: &RunOwner) -> CoreResult<()> {
    if owner.project_id != project.info.project_id
        || owner.operation_namespace != project.info.operation_namespace
    {
        return Err(CoreError::new(
            "DiscussionProjectMismatch",
            "The discussion run belongs to another project or recovered project identity.",
        ));
    }
    Ok(())
}

fn ensure_run_started(status: DiscussionRunStatus) -> CoreResult<()> {
    match status {
        DiscussionRunStatus::Running => Ok(()),
        DiscussionRunStatus::Queued => Err(CoreError::new(
            "RunNotStarted",
            "Claim the queued discussion before accepting provider output.",
        )),
        DiscussionRunStatus::Stopping => Err(CoreError::new(
            "RunStopping",
            "The discussion is stopping and no further output is accepted.",
        )),
        _ => Err(CoreError::new(
            "RunSealed",
            "This discussion run is already sealed.",
        )),
    }
}

fn seal_run(
    tx: &Connection,
    current: &DiscussionRun,
    status: DiscussionRunStatus,
    reason: &str,
    event_id: &str,
    message: &str,
) -> CoreResult<DiscussionRun> {
    if !current.status.active() || !status.terminal() {
        return Err(CoreError::new(
            "RunSealed",
            "This discussion run is already sealed.",
        ));
    }
    check_id(event_id)?;
    if reason.is_empty() || message.is_empty() || message.len() > MAX_EVENT_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "A terminal discussion event must include a bounded reason and message.",
        ));
    }
    let sequence = parse_version(&current.sequence)?;
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
    tx.execute(
        "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
        params![current.id, next, event_id, "terminal", message],
    )?;
    let changed = tx.execute(
        "UPDATE discussion_runs SET status=?,sequence=?,stop_reason=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status IN ('queued','running','stopping') AND sequence=?",
        params![status.as_str(), next, reason, current.id, current.owner.project_id, current.owner.operation_namespace, sequence],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "SequenceConflict",
            "The discussion changed before its terminal state was committed.",
        ));
    }
    let content = if current.output_text.is_empty() {
        message.to_owned()
    } else {
        format!("{}\n\n[{}]", current.output_text, message)
    };
    tx.execute(
        "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,packet_id) VALUES(?,?,?,?,?,?)",
        params![new_id(), current.thread_id, current.id, DiscussionMessageRole::Assistant.as_str(), content, current.packet_id],
    )?;
    read_run(tx, &current.id)
}

fn validate_output_event(owner: &RunOwner, event_id: &str, text: &str) -> CoreResult<()> {
    check_id(&owner.project_id)?;
    check_id(&owner.operation_namespace)?;
    check_id(&owner.run_id)?;
    check_id(event_id)?;
    if text.is_empty() || text.len() > MAX_EVENT_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "A discussion output event must be nonempty and at most 128 KiB.",
        ));
    }
    Ok(())
}

fn append_text(existing: &str, chunk: &str, limit: usize) -> CoreResult<String> {
    if existing
        .len()
        .checked_add(chunk.len())
        .is_none_or(|length| length > limit)
    {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The discussion output exceeds the durable limit.",
        ));
    }
    let mut result = String::with_capacity(existing.len() + chunk.len());
    result.push_str(existing);
    result.push_str(chunk);
    Ok(result)
}

fn packet_error(error: PacketError) -> CoreError {
    CoreError::new("ContextPreparationFailed", &error.to_string())
}
