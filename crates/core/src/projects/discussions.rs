//! Durable author-room chapter discussions.
//!
//! A discussion start is one actor-owned transaction: the target is checked,
//! its immutable context revision is frozen, the exact C2 packet is compiled,
//! and the user message plus queued run are inserted before the transaction
//! commits. Provider execution is intentionally outside this module. The
//! output methods below are the small durable boundary a later supervisor can
//! drive with deterministic or live events.

use super::*;
use crate::context::continuation::CONTINUATION_RESPONSE_CONTRACT;
use crate::context::lookup::LookupAllowance;
use crate::context::packet::{
    CODEX_INPUT_LIMIT_BYTES, CODEX_OUTPUT_LIMIT_BYTES, CompiledPacket, LOOKUP_RESPONSE_CONTRACT,
    MockContextBudget, PROPOSAL_RESPONSE_CONTRACT, PacketError, PacketRequest, ProviderBinding,
    STRUCTURED_PROPOSAL_RESPONSE_CONTRACT, compile_packet, serialized_input,
};
use crate::context::{
    Audience, BasisKind, ContextPurpose, InformationPolicy, MAX_SAFE_BRIEF_BYTES,
};
use crate::documents::{
    Endpoint, ScopeGrant, ScopeKind, ScopeValidationRequest, capture_append_scope, capture_scope,
    validate_scope,
};
use crate::projects::context_packets::PrepareContext;
use crate::projects::discussion_lookup;
use crate::projects::story_context::{FreezeReviewedContinuation, FreezeStory, FrozenContext};
use crate::projects::workshop_generation::WORKSHOP_RESPONSE_CONTRACT;
use crate::providers::http_request::prepare_request as prepare_http_request;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub use crate::context::SafeBriefInput;

mod app_server;
mod retry;

const MAX_INSTRUCTION_BYTES: usize = 64 * 1024;
const MAX_SCOPE_QUOTE_BYTES: usize = 256 * 1024;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 128 * 1024;
const MAX_PINNED_DOCUMENTS: usize = 64;
const STOP_SETTLED_MESSAGE: &str =
    "You stopped this response. Any partial text shown here is saved.";
const STOP_UNRESOLVED_MESSAGE: &str =
    "This response was interrupted. Any partial text shown here is saved.";

/// The two author-room actions supported by a discussion request.  `Discuss`
/// is intentionally the wire default so older clients produce the same
/// request hash they did before intent was added to the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum FeedbackIntent {
    #[default]
    Discuss,
    ProposeEdits,
    Continue,
    WorkshopExplore,
}

impl FeedbackIntent {
    fn is_discuss(self) -> bool {
        self == Self::Discuss
    }

    fn purpose(self) -> ContextPurpose {
        match self {
            Self::Discuss => ContextPurpose::Discuss,
            Self::ProposeEdits => ContextPurpose::Revise,
            Self::Continue => ContextPurpose::Continue,
            Self::WorkshopExplore => ContextPurpose::StoryQuestion,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Discuss => "discuss",
            Self::ProposeEdits => "proposeEdits",
            Self::Continue => "continue",
            Self::WorkshopExplore => "workshopExplore",
        }
    }

    fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "discuss" => Ok(Self::Discuss),
            "proposeEdits" => Ok(Self::ProposeEdits),
            "continue" => Ok(Self::Continue),
            "workshopExplore" => Ok(Self::WorkshopExplore),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved discussion draft has an unknown intent.",
            )),
        }
    }

    fn from_purpose(purpose: ContextPurpose) -> CoreResult<Self> {
        match purpose {
            ContextPurpose::Discuss => Ok(Self::Discuss),
            ContextPurpose::Revise => Ok(Self::ProposeEdits),
            ContextPurpose::Continue => Ok(Self::Continue),
            ContextPurpose::StoryQuestion => Ok(Self::WorkshopExplore),
            ContextPurpose::Plan | ContextPurpose::MemoryAnalysis => Err(CoreError::new(
                "InvalidContext",
                "The discussion snapshot has an unsupported purpose.",
            )),
        }
    }
}

fn skip_default_feedback_intent(intent: &FeedbackIntent) -> bool {
    intent.is_discuss()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "skip_default_feedback_intent")]
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<SafeBriefInput>,
    pub budget: MockContextBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
    pub previous_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<LookupAllowance>,
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
    pub(super) fn as_str(self) -> &'static str {
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

/// The provider-side outcome is kept separate from the discussion lifecycle.
/// For example, a timed-out provider request with settled cleanup becomes a
/// durable failed discussion while retaining any validated prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderOutcomeStatus {
    Completed,
    Stopped,
    TimedOut,
    OutputLimit,
    Failed,
}

impl ProviderOutcomeStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Stopped => "stopped",
            Self::TimedOut => "timed_out",
            Self::OutputLimit => "output_limit",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "completed" => Ok(Self::Completed),
            "stopped" => Ok(Self::Stopped),
            "timed_out" => Ok(Self::TimedOut),
            "output_limit" => Ok(Self::OutputLimit),
            "failed" => Ok(Self::Failed),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved provider result has an unknown outcome.",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderCleanup {
    Settled,
    Unresolved,
}

impl ProviderCleanup {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Settled => "settled",
            Self::Unresolved => "unresolved",
        }
    }

    fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "settled" => Ok(Self::Settled),
            "unresolved" => Ok(Self::Unresolved),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved provider result has an unknown cleanup state.",
            )),
        }
    }
}

/// Evidence about the HTTP request itself.  This is intentionally separate
/// from Codex's local stdin count: an HTTP request can be accepted by a remote
/// server even when the local process loses the response before it is parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HttpDeliverySubmission {
    NotSent,
    Uncertain,
    ResponseReceived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpProviderUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDeliveryReceipt {
    pub body_hash: String,
    pub body_bytes: String,
    pub submission: HttpDeliverySubmission,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<HttpProviderUsage>,
}

/// Raw provider usage is optional. Missing usage is an explicit unknown value;
/// no estimate is substituted from the packet's byte accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderTerminalReport {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub assistant_text: String,
    pub binding: ProviderBinding,
    pub status: ProviderOutcomeStatus,
    pub confirmed_stdin_bytes: String,
    pub usage: Option<ProviderUsage>,
    pub cleanup: ProviderCleanup,
    pub error: Option<String>,
    /// The adapter cannot establish an effective identity in this slice. Keep
    /// this optional so a later qualified adapter can report one explicitly.
    pub effective_identity: Option<String>,
    /// The model identity claimed by a Claude terminal result.  It is kept
    /// separate from the requested binding model and is only accepted for
    /// the bounded Claude profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_model: Option<String>,
    /// Present only for OpenAI-compatible HTTP.  Historical Codex reports
    /// omit this field and retain their exact wire shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<ProviderDeliveryReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_server: Option<crate::providers::codex_app_server::AppServerDelivery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderResult {
    pub run_id: String,
    pub packet_id: String,
    pub event_id: String,
    pub expected_sequence: String,
    pub assistant_text: String,
    pub binding: ProviderBinding,
    pub status: ProviderOutcomeStatus,
    pub confirmed_stdin_bytes: String,
    pub usage: Option<ProviderUsage>,
    pub cleanup: ProviderCleanup,
    pub error: Option<String>,
    pub effective_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_model: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<ProviderDeliveryReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_server: Option<crate::providers::codex_app_server::AppServerDelivery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDiscussionSettlement {
    pub run: DiscussionRun,
    pub provider_result: ProviderResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionRun {
    pub id: String,
    pub thread_id: String,
    pub owner: RunOwner,
    pub operation_id: String,
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    pub payload_hash: String,
    pub target: Head,
    pub packet_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_result: Option<ProviderResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<crate::projects::discussion_lookup::LookupRunSummary>,
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
    #[serde(default, skip_serializing_if = "skip_default_feedback_intent")]
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<SafeBriefInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_run_id: Option<String>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<LookupAllowance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionRetry {
    pub text: String,
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<SafeBriefInput>,
    pub previous_run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<LookupAllowance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveDiscussionDraft {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub document_id: String,
    pub expected_version: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "skip_default_feedback_intent")]
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    pub scope: Option<DiscussionScopeInput>,
    pub pinned_document_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<SafeBriefInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<LookupAllowance>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscussionStopCleanup {
    Settled,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionStopSettled {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub assistant_text: String,
    pub cleanup: DiscussionStopCleanup,
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

#[allow(clippy::large_enum_variant)]
pub(crate) enum DiscussionCommand {
    ClaimAppServer(
        RunOwner,
        crate::providers::codex_app_server::AppServerDispatch,
        Reply<()>,
    ),
    AckAppServer(
        RunOwner,
        crate::providers::codex_app_server::AppServerDispatch,
        String,
        Reply<()>,
    ),
    Start(StartDiscussion, Reply<DiscussionStart>),
    Begin(DiscussionBegin, Reply<DiscussionDispatch>),
    MarkDelivered(RunOwner, Reply<DiscussionRun>),
    Append(DiscussionOutputAppend, Reply<DiscussionRun>),
    Finish(DiscussionFinish, Reply<DiscussionRun>),
    Fail(DiscussionFail, Reply<DiscussionRun>),
    Stop(ProjectAccess, String, Reply<DiscussionStop>),
    SettleStop(DiscussionStopSettled, Reply<DiscussionRun>),
    SettleProvider(ProviderTerminalReport, Reply<ProviderDiscussionSettlement>),
    ClaimLookup(
        RunOwner,
        String,
        Reply<crate::projects::discussion_lookup::LookupDispatch>,
    ),
    SettleLookup(
        crate::projects::discussion_lookup::LookupInvocationReport,
        Reply<DiscussionRun>,
    ),
    AdvanceLookup(
        crate::projects::discussion_lookup::LookupAdvanceRequest,
        Reply<crate::projects::discussion_lookup::LookupAdvance>,
    ),
    HaltLookup(
        crate::projects::discussion_lookup::LookupHaltRequest,
        Reply<DiscussionRun>,
    ),
    ReadRun(RunOwner, Reply<DiscussionRun>),
    Read(ProjectAccess, String, Reply<DiscussionView>),
    Retry(ProjectAccess, String, Reply<DiscussionRetry>),
    SaveDraft(SaveDiscussionDraft, Reply<DiscussionDraft>),
}

impl ProjectSession {
    pub fn discussion_retry(
        &self,
        access: ProjectAccess,
        run_id: String,
    ) -> CoreResult<DiscussionRetry> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::Retry(access, run_id, reply)))
        })
    }

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

    pub fn settle_discussion_stop(
        &self,
        request: DiscussionStopSettled,
    ) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::SettleStop(request, reply)))
        })
    }

    /// Atomically settle a bounded provider result and its discussion run.
    /// The report must carry the exact binding and sequence captured before
    /// dispatch; retries with the same event are idempotent.
    pub fn settle_provider_discussion(
        &self,
        request: ProviderTerminalReport,
    ) -> CoreResult<ProviderDiscussionSettlement> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::SettleProvider(request, reply)))
        })
    }

    pub fn claim_lookup_invocation(
        &self,
        owner: RunOwner,
        ordinal: String,
    ) -> CoreResult<crate::projects::discussion_lookup::LookupDispatch> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::ClaimLookup(
                owner, ordinal, reply,
            )))
        })
    }

    pub fn settle_lookup_invocation(
        &self,
        request: crate::projects::discussion_lookup::LookupInvocationReport,
    ) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::SettleLookup(request, reply)))
        })
    }

    pub fn advance_lookup(
        &self,
        request: crate::projects::discussion_lookup::LookupAdvanceRequest,
    ) -> CoreResult<crate::projects::discussion_lookup::LookupAdvance> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::AdvanceLookup(request, reply)))
        })
    }

    pub fn halt_lookup(
        &self,
        request: crate::projects::discussion_lookup::LookupHaltRequest,
    ) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::HaltLookup(request, reply)))
        })
    }

    /// Read an owned run for a worker or recovery actor. This deliberately
    /// does not require a renderer session or writer lease.
    pub fn read_discussion_run(&self, owner: RunOwner) -> CoreResult<DiscussionRun> {
        self.request(|reply| {
            Command::Discussion(Box::new(DiscussionCommand::ReadRun(owner, reply)))
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
            DiscussionCommand::ClaimAppServer(owner, dispatch, reply) => {
                mutate!(reply, self.claim_app_server_dispatch(owner, dispatch));
            }
            DiscussionCommand::AckAppServer(owner, dispatch, turn_id, reply) => {
                mutate!(
                    reply,
                    self.acknowledge_app_server_turn(owner, dispatch, turn_id)
                );
            }
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
            DiscussionCommand::SettleStop(request, reply) => {
                mutate!(reply, self.settle_discussion_stop(request));
            }
            DiscussionCommand::SettleProvider(request, reply) => {
                mutate!(reply, self.settle_provider_discussion(request));
            }
            DiscussionCommand::ClaimLookup(owner, ordinal, reply) => {
                mutate!(reply, self.claim_lookup_invocation(owner, &ordinal));
            }
            DiscussionCommand::SettleLookup(request, reply) => {
                mutate!(reply, self.settle_lookup_invocation(request));
            }
            DiscussionCommand::AdvanceLookup(request, reply) => {
                mutate!(reply, self.advance_lookup(request));
            }
            DiscussionCommand::HaltLookup(request, reply) => {
                mutate!(reply, self.halt_lookup(request));
            }
            DiscussionCommand::ReadRun(owner, reply) => {
                let result = validate_runtime_owner(self, &owner).and_then(|()| {
                    read_run(self.db()?, &owner.run_id).and_then(|run| {
                        validate_owner(&run, &owner)?;
                        Ok(run)
                    })
                });
                let _ = reply.send(result);
            }
            DiscussionCommand::Read(access, document_id, reply) => {
                let _ = reply.send(self.read_discussion(access, document_id));
            }
            DiscussionCommand::Retry(access, run_id, reply) => {
                let _ = reply.send(
                    self.check_access(&access)
                        .and_then(|()| retry::draft(self.db()?, &access, &run_id)),
                );
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
        let result = start_discussion_at(&tx, &request, &payload_hash, None, false)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }

    /// Start a discussion using an already-open actor transaction. Project
    /// chat uses this seam to persist its conversation reference atomically
    /// with the ordinary discussion run, packet, and message. It performs no
    /// access check or commit and therefore cannot nest actor transactions.
    pub(super) fn start_discussion_at(
        tx: &Connection,
        request: &StartDiscussion,
        payload_hash: &str,
        chat: Option<&crate::projects::project_chat_context::ProjectChatFreeze>,
        chapter_range: bool,
    ) -> CoreResult<DiscussionStart> {
        if let Some(chat) = chat {
            if request.intent != FeedbackIntent::Discuss
                || request.scope.is_some()
                || request.safe_brief.is_some()
                || request.lookup.is_some()
                || request.basis.is_some()
            {
                return Err(CoreError::new(
                    "InvalidProjectChatRequest",
                    "Project chat uses a plain author-room discussion without a writing scope, lookup, or brief.",
                ));
            }
            check_id(&chat.conversation_id)?;
        }
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
            let result = read_start(tx, &run_id)?;
            return Ok(result);
        }
        validate_safe_brief_origin(tx, request)?;

        let retry_guidance = retry::guidance(tx, request)?;
        let (purpose, policy) = discussion_context_policy(tx, request)?;
        let context_request = FreezeStory {
            access: request.access.clone(),
            operation_id: new_id(),
            expected: request.expected.clone(),
            basis: BasisKind::Working,
            purpose,
            policy,
        };
        let context_payload = logical_hash(&context_request)?;
        let frozen_context = if request.basis == Some(BasisKind::Reviewed) {
            let reviewed = FreezeReviewedContinuation {
                access: request.access.clone(),
                operation_id: context_request.operation_id.clone(),
                expected: request.expected.clone(),
                policy: context_request.policy.clone(),
            };
            let payload = logical_hash(&reviewed)?;
            story_context::freeze_reviewed_continuation_at(tx, &reviewed, &payload)?
        } else if let Some(chat) = chat {
            crate::projects::project_chat_context::freeze_project_chat_at(
                tx,
                &context_request,
                &context_payload,
                chat,
            )?
        } else {
            story_context::freeze_discussion_story_at(
                tx,
                &context_request,
                &context_payload,
                retry_guidance.as_deref(),
            )?
        };
        let target = read_revision(tx, &frozen_context.snapshot.target.revision_id)?;
        if chapter_range {
            let target_kind: Option<String> = tx
                .query_row(
                    "SELECT kind FROM documents WHERE id=? AND trashed=0",
                    [&request.expected.document_id],
                    |row| row.get(0),
                )
                .optional()?;
            if request.intent != FeedbackIntent::Discuss
                || request.scope.is_some()
                || target_kind.as_deref() != Some("chapter")
            {
                return Err(CoreError::new(
                    "InvalidChapterRequest",
                    "The chapter range response contract requires an unscoped Discuss request targeting an ordinary chapter.",
                ));
            }
        }
        // Project chat is rooted at its blank control anchor. Persistent
        // document pins are a legacy document-discussion feature and cannot
        // be read through that control identity; project-chat source refs are
        // authenticated by its dedicated freeze path instead.
        let persistent_ids = if chat.is_some() {
            Vec::new()
        } else {
            source_pins::persistent_for_discussion(
                tx,
                &request.access,
                &request.expected.document_id,
                request.intent.is_discuss(),
            )?
        };
        let merged_document_ids =
            merge_pinned_document_ids(&persistent_ids, &request.pinned_document_ids)?;
        let transient_handles =
            resolve_pinned_handles(&frozen_context, &request.pinned_document_ids)?;
        let mut all_mandatory_handles =
            resolve_pinned_handles(&frozen_context, &merged_document_ids)?;
        if let Some(chat) = chat {
            // Explicit project-chat source refs are author-selected evidence;
            // they are mandatory packet inputs and may not disappear under
            // layered budget packing. The blank control target remains
            // mandatory through the shared target rule.
            for head in chat
                .source_refs
                .iter()
                .chain(chat.task_draft_refs.iter().map(|draft| &draft.head))
            {
                let handle = frozen_context
                    .snapshot
                    .sources
                    .iter()
                    .find(|source| {
                        source.source.document_id == head.document_id
                            && source.source.body_hash == head.body_hash
                    })
                    .map(|source| source.handle.clone())
                    .ok_or_else(|| {
                        CoreError::new(
                            "SourceOutsideFrozenContext",
                            "A project-chat source ref has no frozen source handle.",
                        )
                    })?;
                if !all_mandatory_handles.contains(&handle) {
                    all_mandatory_handles.push(handle);
                }
            }
        }
        let target_handle = frozen_context
            .snapshot
            .sources
            .iter()
            .find(|source| source.source == frozen_context.snapshot.target)
            .map(|source| source.handle.clone())
            .ok_or_else(|| {
                CoreError::new(
                    "InvalidContext",
                    "The frozen discussion target is missing from its source manifest.",
                )
            })?;
        // A transient target selection keeps the existing compiler refusal.
        // A persistent target selection needs no additional source entry:
        // the compiler already reserves the complete target itself.
        let mandatory_handles = if transient_handles
            .iter()
            .any(|handle| handle == &target_handle)
        {
            all_mandatory_handles.clone()
        } else {
            all_mandatory_handles
                .iter()
                .filter(|handle| *handle != &target_handle)
                .cloned()
                .collect()
        };
        let source_reads = frozen_context
            .snapshot
            .sources
            .iter()
            .map(|source| story_context::read_source(tx, &frozen_context, &source.handle))
            .collect::<CoreResult<Vec<_>>>()?;
        let scope = if request.intent == FeedbackIntent::Continue {
            Some(
                capture_append_scope(&target.body)
                    .map_err(|message| CoreError::new("InvalidScope", &message))?,
            )
        } else {
            capture_discussion_scope(request.scope.as_ref(), &target.body)?
        };
        // The response contract is derived here from trusted intent and the
        // immutable live binding. It is never accepted from the renderer, so
        // old mock packets and old live packets remain contract-free.
        if request.intent == FeedbackIntent::WorkshopExplore {
            crate::projects::workshop_generation::metadata_from_instruction(&request.instruction)?;
        }
        let response_contract = if chat.is_some() {
            Some(crate::projects::project_chat_output::PROJECT_CHAT_RESPONSE_CONTRACT.to_owned())
        } else if chapter_range {
            Some(
                crate::projects::project_chat_output::CHAPTER_DISCUSSION_RESPONSE_CONTRACT
                    .to_owned(),
            )
        } else {
            match request.intent {
                FeedbackIntent::Discuss if request.lookup.is_some() => {
                    Some(LOOKUP_RESPONSE_CONTRACT.to_owned())
                }
                FeedbackIntent::Continue => Some(CONTINUATION_RESPONSE_CONTRACT.to_owned()),
                FeedbackIntent::ProposeEdits if request.provider_binding.is_some() => Some(
                    if scope.as_ref().is_some_and(|scope| {
                        matches!(scope.kind, ScopeKind::Blocks | ScopeKind::WholeDocument)
                    }) {
                        STRUCTURED_PROPOSAL_RESPONSE_CONTRACT.to_owned()
                    } else {
                        PROPOSAL_RESPONSE_CONTRACT.to_owned()
                    },
                ),
                FeedbackIntent::WorkshopExplore => Some(WORKSHOP_RESPONSE_CONTRACT.to_owned()),
                _ => None,
            }
        };
        let instruction = packet_instruction(request, response_contract.as_deref())?;
        // Parsed here, where the instruction is authored, and passed down. The
        // compiler used to parse it out of `instruction` itself, which made it
        // reach up into this crate for the workshop vocabulary and its
        // validation cluster.
        let workshop_metadata = if response_contract.as_deref() == Some(WORKSHOP_RESPONSE_CONTRACT)
        {
            let metadata = workshop_generation::metadata_from_instruction(&instruction)?;
            Some(workshop_generation::metadata_value(&metadata)?)
        } else {
            None
        };
        let packet = compile_packet(&PacketRequest {
            packet_id: new_id(),
            session_id: new_id(),
            invocation_ordinal: "0".into(),
            frozen: frozen_context.clone(),
            instruction,
            sources: source_reads,
            mandatory_handles: mandatory_handles.clone(),
            scope: scope.clone(),
            safe_brief: request.safe_brief.clone(),
            budget: request.budget.clone(),
            provider_binding: request.provider_binding.clone(),
            lookup: request.lookup.clone().map(|allowance| {
                crate::context::lookup::LookupPacketInput {
                    allowance,
                    completed_invocations: 0,
                    exchanges: Vec::new(),
                    source_projection: None,
                    reviewed_memory: Some(
                        crate::context::lookup::REVIEWED_MEMORY_CAPABILITY.to_owned(),
                    ),
                }
            }),
            response_contract: response_contract.clone(),
            workshop_metadata,
        })
        .map_err(packet_error)?;
        insert_packet(
            tx,
            &packet,
            request,
            &mandatory_handles,
            Some(transient_handles),
            scope.as_ref(),
            response_contract.as_deref(),
        )?;
        if retry_guidance.is_none() {
            guidance::consume_request_guidance_at(
                tx,
                &frozen_context.snapshot.snapshot_id,
                &frozen_context.guidance,
            )?;
        }

        let thread_id = ensure_thread(tx, &request.access, &request.expected.document_id)?;
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
        if let Some(lookup_allowance) = request.lookup.as_ref() {
            discussion_lookup::insert_initial(
                tx,
                &run_id,
                &packet,
                &frozen_context,
                &request.access.operation_namespace,
                lookup_allowance,
            )?;
        }
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
        let run = read_run(tx, &run_id)?;
        let user_message = read_message(tx, &user_message_id)?;
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
        reject_legacy_lookup_path(&current)?;
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
                let (snapshot_id, snapshot_source_epoch, snapshot_policy_epoch): (String, i64, i64) = tx.query_row(
                    "SELECT cp.snapshot_id,ss.context_source_epoch,ss.disclosure_policy_epoch FROM discussion_runs dr JOIN context_packets cp ON cp.id=dr.packet_id JOIN story_snapshots ss ON ss.id=cp.snapshot_id WHERE dr.id=?",
                    [&request.owner.run_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
                let (source_epoch, policy_epoch): (i64, i64) = tx.query_row(
                    "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                let project_chat_current = if snapshot_source_epoch == source_epoch
                    && snapshot_policy_epoch == policy_epoch
                {
                    let (frozen, _) = story_context::validated_snapshot_record(&tx, &snapshot_id)?;
                    !frozen.project_chat.is_some()
                        || crate::projects::project_chat_context::project_chat_basis_is_current(
                            &tx, &frozen,
                        )?
                } else {
                    false
                };
                if !project_chat_current {
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

    pub(super) fn claim_lookup_invocation(
        &mut self,
        owner: RunOwner,
        ordinal_text: &str,
    ) -> CoreResult<discussion_lookup::LookupDispatch> {
        check_id(&owner.project_id)?;
        check_id(&owner.operation_namespace)?;
        check_id(&owner.run_id)?;
        validate_runtime_owner(self, &owner)?;
        let ordinal = parse_lookup_ordinal(ordinal_text)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &owner.run_id)?;
        validate_owner(&current, &owner)?;
        if current.lookup.is_none() {
            return Err(CoreError::new(
                "LookupNotEnabled",
                "This discussion did not opt into bounded context lookup.",
            ));
        }
        ensure_run_started(current.status)?;
        let (snapshot_source_epoch, snapshot_policy_epoch): (i64, i64) = tx.query_row(
            "SELECT source_epoch,policy_epoch FROM discussion_lookup_invocations WHERE run_id=? AND ordinal=?",
            params![owner.run_id, i64::from(ordinal)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let (source_epoch, policy_epoch): (i64, i64) = tx.query_row(
            "SELECT context_source_epoch,disclosure_policy_epoch FROM project WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if snapshot_source_epoch != source_epoch || snapshot_policy_epoch != policy_epoch {
            let stale_message = "The story or disclosure policy changed before this lookup started; the saved response was not dispatched.";
            discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
            let _ = seal_run(
                &tx,
                &current,
                DiscussionRunStatus::Failed,
                "context_stale",
                &format!("lookup-stale-{}-{}", current.id, ordinal),
                stale_message,
            )?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Err(CoreError::new(
                "ContextChanged",
                "The queued lookup is historical because its frozen context is no longer current.",
            ));
        }
        let identity = discussion_lookup::claim(&tx, &owner, ordinal)?;
        let packet = read_lookup_packet(&tx, &owner, &identity.packet_id)?;
        let run = read_run(&tx, &owner.run_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(discussion_lookup::LookupDispatch {
            run,
            ordinal: ordinal.to_string(),
            packet,
        })
    }

    pub(super) fn settle_lookup_invocation(
        &mut self,
        request: discussion_lookup::LookupInvocationReport,
    ) -> CoreResult<DiscussionRun> {
        validate_lookup_report_shape(&request)?;
        validate_runtime_owner(self, &request.owner)?;
        let ordinal = parse_lookup_ordinal(&request.ordinal)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        if current.lookup.is_none() {
            return Err(CoreError::new(
                "LookupNotEnabled",
                "This discussion did not opt into bounded context lookup.",
            ));
        }
        if let Some(saved) = lookup_result_record(&tx, &request.owner.run_id, ordinal)? {
            if lookup_result_matches(&saved, &request) {
                tx.commit().map_err(CoreError::uncertain)?;
                return Ok(current);
            }
            return Err(CoreError::new(
                "LookupResultConflict",
                "A different immutable result is already recorded for this invocation.",
            ));
        }
        if !matches!(
            current.status,
            DiscussionRunStatus::Running | DiscussionRunStatus::Stopping
        ) {
            return Err(CoreError::new(
                "RunNotStarted",
                "Claim the lookup invocation before settling its provider result.",
            ));
        }
        let identity = discussion_lookup::read_identity(&tx, &request.owner, ordinal)?;
        let packet = context_packets::validated_packet_record(&tx, &identity.packet_id)?;
        if identity.state != discussion_lookup::LookupInvocationState::Claimed {
            return Err(CoreError::new(
                "LookupInvocationNotClaimed",
                "A provider result requires a claimed lookup invocation.",
            ));
        }
        if packet.options.provider_binding != request.binding {
            return Err(CoreError::new(
                "ProviderBindingMismatch",
                "The lookup result does not match the immutable packet binding.",
            ));
        }
        validate_lookup_provider_bytes(&tx, &identity, &packet, &request)?;
        let mut response_error = None;
        let parsed = if request.status == ProviderOutcomeStatus::Completed {
            match discussion_lookup::parse_response(&request.assistant_text) {
                Ok(parsed) => {
                    match discussion_lookup::authorize_envelope(&packet, &parsed.envelope) {
                        Ok(()) => Some(parsed),
                        Err(error) => {
                            response_error = Some(error.detail);
                            None
                        }
                    }
                }
                Err(error) => {
                    response_error = Some(error.detail);
                    None
                }
            }
        } else {
            None
        };
        let report_error = request.error.clone().or(response_error);
        let stored_status =
            if parsed.is_none() && request.status == ProviderOutcomeStatus::Completed {
                ProviderOutcomeStatus::Failed
            } else {
                request.status
            };
        let state = discussion_lookup::store_outcome(
            &tx,
            &identity,
            &discussion_lookup::ProviderReport {
                event_id: request.event_id.clone(),
                assistant_text: request.assistant_text.clone(),
                binding: request.binding.clone(),
                status: stored_status,
                confirmed_stdin_bytes: request.confirmed_stdin_bytes.clone(),
                usage: request.usage.clone(),
                cleanup: request.cleanup,
                error: report_error.clone(),
            },
            parsed.as_ref(),
        )?;
        let result = match parsed {
            Some(_) if current.status == DiscussionRunStatus::Stopping => {
                discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
                seal_run(
                    &tx,
                    &current,
                    if request.cleanup == ProviderCleanup::Unresolved {
                        DiscussionRunStatus::Interrupted
                    } else {
                        DiscussionRunStatus::Stopped
                    },
                    if request.cleanup == ProviderCleanup::Unresolved {
                        "stop_cleanup_unresolved"
                    } else {
                        "author_stopped"
                    },
                    &request.event_id,
                    if request.cleanup == ProviderCleanup::Unresolved {
                        STOP_UNRESOLVED_MESSAGE
                    } else {
                        STOP_SETTLED_MESSAGE
                    },
                )?
            }
            Some(parsed) if parsed.kind == discussion_lookup::ResponseKind::Discussion => {
                let discussion_text = match &parsed.envelope {
                    crate::context::lookup::LookupEnvelope::Discussion { text, .. } => text,
                    crate::context::lookup::LookupEnvelope::NeedsContext { .. } => unreachable!(),
                };
                seal_lookup_discussion(
                    &tx,
                    &current,
                    &identity.packet_id,
                    &request.event_id,
                    discussion_text,
                )?
            }
            Some(_) => read_run(&tx, &current.id)?,
            None => {
                discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
                let reason = lookup_failure_reason(request.status, request.cleanup);
                let message = report_error
                    .as_deref()
                    .filter(|error| !error.is_empty())
                    .unwrap_or(reason);
                seal_run(
                    &tx,
                    &current,
                    if request.cleanup == ProviderCleanup::Unresolved {
                        DiscussionRunStatus::Interrupted
                    } else if request.status == ProviderOutcomeStatus::Stopped {
                        DiscussionRunStatus::Stopped
                    } else {
                        DiscussionRunStatus::Failed
                    },
                    reason,
                    &request.event_id,
                    message,
                )?
            }
        };
        if state == discussion_lookup::LookupInvocationState::NeedsContext {
            // The root run intentionally remains Running until a later final
            // envelope or an explicit halt settles the chain.
        }
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }

    pub(super) fn advance_lookup(
        &mut self,
        request: discussion_lookup::LookupAdvanceRequest,
    ) -> CoreResult<discussion_lookup::LookupAdvance> {
        check_id(&request.owner.project_id)?;
        check_id(&request.owner.operation_namespace)?;
        check_id(&request.owner.run_id)?;
        validate_runtime_owner(self, &request.owner)?;
        let completed = parse_lookup_ordinal(&request.completed_ordinal)?;
        let current_source_epoch = self.context_source_epoch()?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        let Some(lookup_summary) = current.lookup.as_ref() else {
            return Err(CoreError::new(
                "LookupNotEnabled",
                "This discussion did not opt into bounded context lookup.",
            ));
        };
        if current.status != DiscussionRunStatus::Running {
            return Err(CoreError::new(
                "RunSealed",
                "Only a running lookup discussion can be expanded.",
            ));
        }
        let identity = discussion_lookup::read_identity(&tx, &request.owner, completed)?;
        if identity.state != discussion_lookup::LookupInvocationState::NeedsContext {
            return Err(CoreError::new(
                "LookupExpansionUnavailable",
                "Only a needs-context invocation can be expanded.",
            ));
        }
        if completed
            >= lookup_summary
                .allowance
                .max_additional_invocations
                .saturating_add(1)
            || completed >= 2
        {
            discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
            let finished = seal_run(
                &tx,
                &current,
                DiscussionRunStatus::Failed,
                "lookup_limit",
                &format!("lookup-limit-{}", completed),
                "This lookup request reached its bounded context-expansion limit.",
            )?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(discussion_lookup::LookupAdvance::Finished { run: finished });
        }
        if identity.source_epoch != current_source_epoch {
            discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
            let finished = seal_run(
                &tx,
                &current,
                DiscussionRunStatus::Failed,
                "context_stale",
                &format!("lookup-stale-{}", completed),
                "The story changed before the next lookup request; no new provider call was made.",
            )?;
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(discussion_lookup::LookupAdvance::Finished { run: finished });
        }
        let packet = context_packets::validated_packet_record(&tx, &identity.packet_id)?;
        let prepare_json: String = tx.query_row(
            "SELECT request_json FROM context_packets WHERE id=?",
            [&identity.packet_id],
            |row| row.get(0),
        )?;
        let mut prepare: PrepareContext = serde_json::from_str(&prepare_json)?;
        let frozen = story_context::load_snapshot(&tx, &prepare.access, &identity.snapshot_id)?;
        let reads = discussion_lookup::pending_reads(&tx, &current.id, completed)?;
        let lookup_packet = packet.receipt.lookup.as_ref().ok_or_else(|| {
            CoreError::new(
                "InvalidLookupCapability",
                "The lookup packet is missing its authorized lookup receipt.",
            )
        })?;
        lookup_packet
            .validate_capability()
            .map_err(|error| CoreError::new("InvalidLookupCapability", &error.to_string()))?;
        let mut round_exchanges = Vec::with_capacity(reads.len());
        for read in &reads {
            lookup_packet
                .authorize_read(read)
                .map_err(|error| CoreError::new("InvalidLookupCapability", &error.to_string()))?;
            let (result, truncated) = discussion_lookup::execute_read(&tx, &frozen, read);
            let value = serde_json::to_value(&result)?;
            discussion_lookup::store_read(
                &tx,
                &request.owner,
                completed,
                read.id(),
                read,
                &value,
                truncated,
            )?;
            round_exchanges.push(crate::context::lookup::LookupExchange {
                request: read.clone(),
                result,
            });
        }
        let mut exchanges = if completed == 0 {
            Vec::new()
        } else {
            discussion_lookup::read_exchanges(&tx, &current.id, completed - 1)?
        };
        exchanges.extend(round_exchanges);
        let next = completed.checked_add(1).ok_or_else(|| {
            CoreError::new("InvalidLookupCounter", "The lookup ordinal overflowed.")
        })?;
        let source_projection = Some(
            crate::context::lookup::LookupSourceProjection::from_exchanges(&frozen, &exchanges)?,
        );
        prepare.operation_id = crate::projects::new_id();
        prepare.snapshot_id = frozen.snapshot.snapshot_id.clone();
        prepare.lookup = Some(crate::context::lookup::LookupPacketInput {
            allowance: lookup_summary.allowance.clone(),
            completed_invocations: next,
            exchanges,
            source_projection,
            reviewed_memory: lookup_packet.reviewed_memory.clone(),
        });
        let sources = frozen
            .snapshot
            .sources
            .iter()
            .map(|source| story_context::read_source(&tx, &frozen, &source.handle))
            .collect::<CoreResult<Vec<_>>>()?;
        let packet_request = PacketRequest {
            packet_id: crate::projects::new_id(),
            session_id: packet.receipt.session_id.clone(),
            invocation_ordinal: next.to_string(),
            frozen: frozen.clone(),
            instruction: prepare.instruction.clone(),
            sources,
            mandatory_handles: prepare.mandatory_handles.clone(),
            safe_brief: prepare.safe_brief.clone(),
            scope: prepare.scope.clone(),
            budget: prepare.budget.clone(),
            provider_binding: prepare.provider_binding.clone(),
            response_contract: Some(LOOKUP_RESPONSE_CONTRACT.to_owned()),
            workshop_metadata: None,
            lookup: prepare.lookup.clone(),
        };
        let child_packet = compile_packet(&packet_request).map_err(packet_error)?;
        context_packets::persist_compiled_packet_at(&tx, &prepare, &child_packet)?;
        discussion_lookup::insert_child(
            &tx,
            &current.id,
            next,
            &child_packet,
            &frozen,
            &request.owner.operation_namespace,
            &lookup_summary.allowance,
        )?;
        let run = read_run(&tx, &current.id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(discussion_lookup::LookupAdvance::Prepared {
            dispatch: Box::new(discussion_lookup::LookupDispatch {
                run,
                ordinal: next.to_string(),
                packet: child_packet,
            }),
        })
    }

    pub(super) fn halt_lookup(
        &mut self,
        request: discussion_lookup::LookupHaltRequest,
    ) -> CoreResult<DiscussionRun> {
        check_id(&request.owner.project_id)?;
        check_id(&request.owner.operation_namespace)?;
        check_id(&request.owner.run_id)?;
        validate_runtime_owner(self, &request.owner)?;
        if request.reason.trim().is_empty() || request.reason.len() > 4096 {
            return Err(CoreError::new(
                "InvalidRequest",
                "A lookup halt reason must be nonblank and at most 4 KiB.",
            ));
        }
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        if current.lookup.is_none() {
            return Err(CoreError::new(
                "LookupNotEnabled",
                "This discussion did not opt into bounded context lookup.",
            ));
        }
        if current.status.terminal() {
            tx.commit().map_err(CoreError::uncertain)?;
            return Ok(current);
        }
        discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
        let result = seal_run(
            &tx,
            &current,
            DiscussionRunStatus::Interrupted,
            "lookup_halted",
            &format!("lookup-halt-{}", current.sequence),
            &request.reason,
        )?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
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
        if current.provider_binding.is_some() {
            return Err(CoreError::new(
                "ProviderResultRequired",
                "A live discussion can be delivered only through its typed provider result.",
            ));
        }
        if current.status == DiscussionRunStatus::Stopping {
            return Err(CoreError::new(
                "RunStopping",
                "The discussion is stopping and cannot be marked delivered before cleanup settles.",
            ));
        }
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
        validate_finish_request(&request.owner, &request.event_id, &request.assistant_text)?;
        validate_runtime_owner(self, &request.owner)?;
        let expected = parse_version(&request.expected_sequence)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        reject_legacy_lookup_path(&current)?;
        if current.provider_binding.is_some() {
            return Err(CoreError::new(
                "ProviderResultRequired",
                "A live discussion can be completed only through its typed provider result.",
            ));
        }
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
        validate_final_output(&current.output_text, &request.assistant_text, false)?;
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
        // Propose-edits runs retain their bounded candidate set in the same
        // transaction as the terminal assistant message. A malformed or
        // non-proposal response remains a normal completed discussion; the
        // proposal store simply retains no candidates for it.
        proposals::retain_candidates_at(&tx, &current, &request.assistant_text)?;
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
        reject_legacy_lookup_path(&current)?;
        if current.status == DiscussionRunStatus::Stopping {
            return Err(CoreError::new(
                "RunStopping",
                "The discussion is stopping and cannot be failed before cleanup settles.",
            ));
        }
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
        let run = match current.status {
            DiscussionRunStatus::Queued => seal_run(
                &tx,
                &current,
                DiscussionRunStatus::Stopped,
                "author_stopped",
                &format!("system-stop-{}", current.id),
                STOP_SETTLED_MESSAGE,
            )?,
            DiscussionRunStatus::Running => {
                tx.execute(
                    "UPDATE discussion_runs SET status='stopping',stop_reason='author_stopped',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running'",
                    params![run_id, access.project_id, access.operation_namespace],
                )?;
                read_run(&tx, &run_id)?
            }
            DiscussionRunStatus::Stopping
            | DiscussionRunStatus::Completed
            | DiscussionRunStatus::Stopped
            | DiscussionRunStatus::Failed
            | DiscussionRunStatus::Interrupted => current,
        };
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(DiscussionStop { run })
    }

    pub(super) fn settle_discussion_stop(
        &mut self,
        request: DiscussionStopSettled,
    ) -> CoreResult<DiscussionRun> {
        validate_settlement_request(&request)?;
        validate_runtime_owner(self, &request.owner)?;
        let expected = parse_version(&request.expected_sequence)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        let status = match request.cleanup {
            DiscussionStopCleanup::Settled => DiscussionRunStatus::Stopped,
            DiscussionStopCleanup::Unresolved => DiscussionRunStatus::Interrupted,
        };
        if let Some((kind, text, event_sequence)) =
            existing_event(&tx, &request.owner.run_id, &request.event_id)?
        {
            if kind == "terminal"
                && text == request.assistant_text
                && expected.checked_add(1) == Some(event_sequence)
                && current.status == status
                && current.output_text == request.assistant_text
            {
                tx.commit().map_err(CoreError::uncertain)?;
                return Ok(current);
            }
            return Err(CoreError::new(
                "EventIdReused",
                "A stop settlement event ID was reused with a different outcome, content, or sequence.",
            ));
        }
        if current.status != DiscussionRunStatus::Stopping {
            return Err(CoreError::new(
                "RunSealed",
                "Only a stopping discussion can be settled.",
            ));
        }
        let sequence = parse_version(&current.sequence)?;
        if expected != sequence {
            return Err(CoreError::new(
                "SequenceConflict",
                "The stop settlement sequence is stale; reconcile the run before retrying.",
            ));
        }
        validate_final_output(&current.output_text, &request.assistant_text, true)?;
        let next = sequence
            .checked_add(1)
            .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
        tx.execute(
            "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
            params![
                request.owner.run_id,
                next,
                request.event_id,
                "terminal",
                request.assistant_text
            ],
        )?;
        let reason = match request.cleanup {
            DiscussionStopCleanup::Settled => "author_stopped",
            DiscussionStopCleanup::Unresolved => "stop_cleanup_unresolved",
        };
        let changed = tx.execute(
            "UPDATE discussion_runs SET status=?,sequence=?,output_text=?,stop_reason=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='stopping' AND sequence=?",
            params![
                status.as_str(),
                next,
                request.assistant_text,
                reason,
                request.owner.run_id,
                request.owner.project_id,
                request.owner.operation_namespace,
                sequence
            ],
        )?;
        if changed != 1 {
            return Err(CoreError::new(
                "SequenceConflict",
                "The stop settlement changed before its terminal state was committed.",
            ));
        }
        let explanation = match request.cleanup {
            DiscussionStopCleanup::Settled => STOP_SETTLED_MESSAGE,
            DiscussionStopCleanup::Unresolved => STOP_UNRESOLVED_MESSAGE,
        };
        let message = if request.assistant_text.is_empty() {
            explanation.to_owned()
        } else {
            format!("{}\n\n[{}]", request.assistant_text, explanation)
        };
        tx.execute(
            "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,packet_id) VALUES(?,?,?,?,?,?)",
            params![
                new_id(),
                current.thread_id,
                request.owner.run_id,
                DiscussionMessageRole::Assistant.as_str(),
                message,
                current.packet_id
            ],
        )?;
        let result = read_run(&tx, &request.owner.run_id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(result)
    }

    /// Persist the terminal result returned by the bounded provider worker.
    /// The packet, output sequence, terminal event, assistant message, and
    /// provider receipt commit together. A receipt already present for the run
    /// is treated as the reconciliation authority after a lost acknowledgment.
    pub(super) fn settle_provider_discussion(
        &mut self,
        request: ProviderTerminalReport,
    ) -> CoreResult<ProviderDiscussionSettlement> {
        validate_provider_report_shape(&request)?;
        validate_runtime_owner(self, &request.owner)?;
        let expected = parse_version(&request.expected_sequence)?;
        let confirmed_stdin_bytes = parse_decimal_u64(&request.confirmed_stdin_bytes)?;
        let tx = self
            .db_mut()?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_run(&tx, &request.owner.run_id)?;
        validate_owner(&current, &request.owner)?;
        reject_legacy_lookup_path(&current)?;
        if let Some(saved) = read_provider_result(&tx, &current.id, &current.packet_id)? {
            if provider_result_matches_report(&saved, &request) {
                tx.commit().map_err(CoreError::uncertain)?;
                return Ok(ProviderDiscussionSettlement {
                    run: current,
                    provider_result: saved,
                });
            }
            return Err(CoreError::new(
                "ProviderResultConflict",
                "A provider result is already durably recorded for this run.",
            ));
        }
        if !matches!(
            current.status,
            DiscussionRunStatus::Running | DiscussionRunStatus::Stopping
        ) {
            return Err(CoreError::new(
                "RunNotStarted",
                "Claim the queued discussion before settling a provider result.",
            ));
        }
        let packet = context_packets::validated_packet_record(&tx, &current.packet_id)?;
        let binding = packet.options.provider_binding.as_ref().ok_or_else(|| {
            CoreError::new(
                "ProviderBindingMissing",
                "This discussion was prepared for the local mock and cannot accept a live result.",
            )
        })?;
        if binding != &request.binding {
            return Err(CoreError::new(
                "ProviderBindingMismatch",
                "The provider result does not match the immutable packet binding.",
            ));
        }
        let serialized =
            serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
        let delivered = if crate::providers::codex_app_server::is_app_server(binding) {
            app_server::validate_delivery(
                &tx,
                &current.id,
                &packet,
                request.app_server.as_ref(),
                request.status,
                request.cleanup,
            )?
        } else if binding.is_http() {
            validate_http_delivery(&packet, &request)?;
            matches!(
                request.delivery.as_ref().map(|receipt| receipt.submission),
                Some(HttpDeliverySubmission::ResponseReceived)
            )
        } else {
            if request.delivery.is_some()
                || confirmed_stdin_bytes > serialized.len() as u64
                || (request.status == ProviderOutcomeStatus::Completed
                    && confirmed_stdin_bytes != serialized.len() as u64)
            {
                return Err(CoreError::new(
                    "ProviderInputMismatch",
                    "The Codex provider reported invalid stdin delivery evidence.",
                ));
            }
            confirmed_stdin_bytes == serialized.len() as u64
        };
        let output_limit = binding
            .output_limit()
            .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
        if request.assistant_text.len() > output_limit {
            return Err(CoreError::new(
                "OutputTooLarge",
                "The provider output exceeds the application byte cap.",
            ));
        }
        validate_final_output(&current.output_text, &request.assistant_text, true)?;
        if request.status == ProviderOutcomeStatus::Completed && request.assistant_text.is_empty() {
            return Err(CoreError::new(
                "InvalidRequest",
                "A completed provider result must include assistant output.",
            ));
        }
        if request.status == ProviderOutcomeStatus::Completed && request.error.is_some() {
            return Err(CoreError::new(
                "InvalidRequest",
                "A completed provider result cannot include a provider error.",
            ));
        }
        let sequence = parse_version(&current.sequence)?;
        if expected != sequence {
            return Err(CoreError::new(
                "SequenceConflict",
                "The provider result sequence is stale; reconcile the run before retrying.",
            ));
        }
        if existing_event(&tx, &current.id, &request.event_id)?.is_some() {
            return Err(CoreError::new(
                "EventIdReused",
                "The provider terminal event ID is already used by another event.",
            ));
        }
        let status = provider_discussion_status(current.status, request.status, request.cleanup);
        let reason = provider_stop_reason(current.status, request.status, request.cleanup);
        let terminal_message = provider_terminal_message(&request, status)?;
        let next = sequence
            .checked_add(1)
            .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
        tx.execute(
            "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
            params![current.id, next, request.event_id, "terminal", terminal_message],
        )?;
        let changed = tx.execute(
            "UPDATE discussion_runs SET status=?,sequence=?,output_text=?,stop_reason=?,dispatch_state=CASE WHEN ? THEN 'delivered' ELSE dispatch_state END,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status IN ('queued','running','stopping') AND sequence=?",
            params![
                status.as_str(),
                next,
                request.assistant_text,
                reason,
                delivered,
                current.id,
                current.owner.project_id,
                current.owner.operation_namespace,
                sequence
            ],
        )?;
        if changed != 1 {
            return Err(CoreError::new(
                "SequenceConflict",
                "The discussion changed before its provider result was committed.",
            ));
        }
        let message = if status == DiscussionRunStatus::Completed {
            request.assistant_text.clone()
        } else if request.assistant_text.is_empty() {
            terminal_message.clone()
        } else {
            format!("{}\n\n[{}]", request.assistant_text, terminal_message)
        };
        tx.execute(
            "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,packet_id) VALUES(?,?,?,?,?,?)",
            params![
                new_id(),
                current.thread_id,
                current.id,
                DiscussionMessageRole::Assistant.as_str(),
                message,
                current.packet_id
            ],
        )?;
        let binding_json = serde_json::to_string(binding)?;
        let usage_json = request
            .usage
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let delivery_json = request
            .delivery
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let app_server_json = request
            .app_server
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        tx.execute(
            "INSERT INTO provider_results(run_id,packet_id,terminal_event_id,expected_sequence,binding_json,assistant_text,outcome,confirmed_stdin_bytes,usage_json,cleanup,error,effective_identity,reported_model,delivery_json,app_server_delivery_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                current.id,
                current.packet_id,
                request.event_id,
                expected,
                binding_json,
                request.assistant_text,
                request.status.as_str(),
                i64::try_from(confirmed_stdin_bytes).map_err(|_| {
                    CoreError::new("InvalidRequest", "The provider stdin byte count is too large.")
                })?,
                usage_json,
                request.cleanup.as_str(),
                request.error,
                request.effective_identity,
                request.reported_model,
                delivery_json,
                app_server_json,
            ],
        )?;
        if status == DiscussionRunStatus::Completed {
            proposals::retain_candidates_at(&tx, &current, &request.assistant_text)?;
        }
        let result =
            read_provider_result(&tx, &current.id, &current.packet_id)?.ok_or_else(|| {
                CoreError::new(
                    "PersistenceUnavailable",
                    "The provider receipt could not be read.",
                )
            })?;
        let run = read_run(&tx, &current.id)?;
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(ProviderDiscussionSettlement {
            run,
            provider_result: result,
        })
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
            // A reopened project must seal the whole bounded lookup chain in
            // the same transaction as the discussion run. Otherwise a
            // claimed invocation (or a prepared child) can look dispatchable
            // after recovery even though its owning run is interrupted.
            discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
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

    /// Interrupt one exact active run after its local worker has already
    /// been fenced. This is intentionally owner/ID based rather than a
    /// rediscovery sweep, so a later run cannot be settled by an earlier
    /// close census. `seal_run` retains output text, output events, and any
    /// provider receipt already committed for the run.
    pub(super) fn interrupt_discussion(
        &mut self,
        access: ProjectAccess,
        run_id: String,
    ) -> CoreResult<DiscussionRun> {
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
        let run = match current.status {
            DiscussionRunStatus::Queued
            | DiscussionRunStatus::Running
            | DiscussionRunStatus::Stopping => {
                discussion_lookup::mark_chain_stopped(&tx, &current.id)?;
                seal_run(
                    &tx,
                    &current,
                    DiscussionRunStatus::Interrupted,
                    "project_close_cleanup",
                    &format!("system-interrupted-{}", current.id),
                    STOP_UNRESOLVED_MESSAGE,
                )?
            }
            DiscussionRunStatus::Completed
            | DiscussionRunStatus::Stopped
            | DiscussionRunStatus::Failed
            | DiscussionRunStatus::Interrupted => current,
        };
        tx.commit().map_err(CoreError::uncertain)?;
        Ok(run)
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
        validate_feedback_basis(request.intent, request.basis, request.scope.as_ref())?;
        validate_lookup_request(request.intent, request.basis, request.lookup.as_ref())?;
        validate_safe_brief_draft(request.safe_brief.as_ref())?;
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
        validate_previous_run(
            &tx,
            request.previous_run_id.as_deref(),
            &request.access,
            &request.document_id,
        )?;
        if let Some(previous) = request.previous_run_id.as_deref() {
            let previous_run = read_run(&tx, previous)?;
            if previous_run.intent != request.intent {
                return Err(CoreError::new(
                    "RetryRequestChanged",
                    "The saved retry intent changed. Start a new discussion instead.",
                ));
            }
        }
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
        tx.execute("INSERT INTO discussion_drafts(project_id,operation_namespace,document_id,version,text,intent,scope_json,pinned_document_ids_json,previous_run_id,safe_brief_json,basis,lookup_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(project_id,operation_namespace,document_id) DO UPDATE SET version=excluded.version,text=excluded.text,intent=excluded.intent,scope_json=excluded.scope_json,pinned_document_ids_json=excluded.pinned_document_ids_json,previous_run_id=excluded.previous_run_id,safe_brief_json=excluded.safe_brief_json,basis=excluded.basis,lookup_json=excluded.lookup_json,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![request.access.project_id,request.access.operation_namespace,request.document_id,next,request.text,request.intent.as_str(),request.scope.as_ref().map(serde_json::to_string).transpose()?,serde_json::to_string(&request.pinned_document_ids)?,request.previous_run_id,request.safe_brief.as_ref().map(serde_json::to_string).transpose()?,request.basis.map(basis_label),request.lookup.as_ref().map(serde_json::to_string).transpose()?])?;
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

/// Start a discussion using an already-open transaction. Project chat calls
/// this free module seam so it can atomically add its conversation item after
/// the run, packet, and author message have been prepared.
pub(super) fn start_discussion_at(
    tx: &Connection,
    request: &StartDiscussion,
    payload_hash: &str,
    chat: Option<&crate::projects::project_chat_context::ProjectChatFreeze>,
    chapter_range: bool,
) -> CoreResult<DiscussionStart> {
    OwnedProject::start_discussion_at(tx, request, payload_hash, chat, chapter_range)
}

fn validate_feedback_basis(
    intent: FeedbackIntent,
    basis: Option<BasisKind>,
    scope: Option<&DiscussionScopeInput>,
) -> CoreResult<()> {
    let valid = match intent {
        FeedbackIntent::Continue => {
            matches!(basis, Some(BasisKind::Working | BasisKind::Reviewed)) && scope.is_none()
        }
        FeedbackIntent::WorkshopExplore => basis.is_none() && scope.is_none(),
        _ => basis.is_none() && scope.is_none_or(|scope| scope.kind != ScopeKind::Append),
    };
    if !valid {
        return Err(CoreError::new(
            "InvalidContinuationBasis",
            "Continuation needs an explicit Working draft or Reviewed story basis and no passage selection.",
        ));
    }
    Ok(())
}

fn basis_label(basis: BasisKind) -> &'static str {
    match basis {
        BasisKind::Working => "working",
        BasisKind::Reviewed => "reviewed",
        BasisKind::ExplicitHistory => "explicitHistory",
    }
}

pub(super) fn validate_start(request: &StartDiscussion) -> CoreResult<()> {
    validate_feedback_basis(request.intent, request.basis, request.scope.as_ref())?;
    validate_lookup_request(request.intent, request.basis, request.lookup.as_ref())?;
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
    if let Some(binding) = &request.provider_binding {
        binding
            .validate()
            .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
        if binding.is_http() && request.lookup.is_some() {
            return Err(CoreError::new(
                "UnsupportedProviderFeature",
                "OpenAI-compatible HTTP discussions do not support bounded story lookup yet.",
            ));
        }
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
    validate_safe_brief_start(request)?;
    if request.intent == FeedbackIntent::WorkshopExplore {
        if request.previous_run_id.is_some()
            || request.lookup.is_some()
            || request.safe_brief.is_some()
        {
            return Err(CoreError::new(
                "InvalidWorkshop",
                "Workshop exploration cannot resume or use writing-only request features.",
            ));
        }
        crate::projects::workshop_generation::metadata_from_instruction(&request.instruction)?;
    }
    Ok(())
}

fn validate_lookup_request(
    intent: FeedbackIntent,
    basis: Option<BasisKind>,
    lookup: Option<&LookupAllowance>,
) -> CoreResult<()> {
    let Some(lookup) = lookup else {
        return Ok(());
    };
    if intent != FeedbackIntent::Discuss || basis.is_some_and(|basis| basis != BasisKind::Working) {
        return Err(CoreError::new(
            "UnsupportedContextTools",
            "Bounded story lookup is available only for Working AuthorRoom discussions.",
        ));
    }
    discussion_lookup::validate_allowance(lookup)
}

fn validate_safe_brief_shape(brief: &SafeBriefInput) -> CoreResult<()> {
    if brief.text.len() > MAX_SAFE_BRIEF_BYTES {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief must be at most 16 KiB.",
        ));
    }
    if let Some(origin) = brief.origin_message_id.as_deref()
        && (origin.is_empty()
            || origin.len() > 64
            || !origin
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message ID is invalid.",
        ));
    }
    if let Some(origin) = brief.project_origin.as_ref()
        && (origin.version != "project-conversation-brief.v1"
            || origin.project_id.is_empty()
            || origin.operation_namespace.is_empty()
            || origin.conversation_id.is_empty()
            || origin.message_id.is_empty()
            || origin.scope_hash.len() != 64
            || origin.text_hash.len() != 64
            || !origin
                .scope_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !origin
                .text_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The project conversation brief provenance is malformed.",
        ));
    }
    if let Some(origin_id) = brief.origin_message_id.as_deref()
        && brief
            .project_origin
            .as_ref()
            .is_some_and(|origin| origin.message_id != origin_id)
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The legacy and project brief origins do not identify the same message.",
        ));
    }
    Ok(())
}

fn validate_safe_brief_draft(brief: Option<&SafeBriefInput>) -> CoreResult<()> {
    if let Some(brief) = brief {
        validate_safe_brief_shape(brief)?;
    }
    Ok(())
}

fn validate_safe_brief_start(request: &StartDiscussion) -> CoreResult<()> {
    let Some(brief) = request.safe_brief.as_ref() else {
        return Ok(());
    };
    validate_safe_brief_shape(brief)?;
    if brief.text.trim().is_empty() || !brief.confirmed {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "A writing brief must be nonempty and explicitly confirmed before use.",
        ));
    }
    if !matches!(
        request.intent,
        FeedbackIntent::ProposeEdits | FeedbackIntent::Continue
    ) || (request.intent == FeedbackIntent::ProposeEdits
        && request.scope.as_ref().is_none_or(|scope| {
            !matches!(
                scope.kind,
                ScopeKind::Passage | ScopeKind::Blocks | ScopeKind::WholeDocument
            )
        }))
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "A writing brief is available only for a passage revision or chapter continuation.",
        ));
    }
    Ok(())
}

fn validate_safe_brief_origin(tx: &Connection, request: &StartDiscussion) -> CoreResult<()> {
    let Some(brief) = request.safe_brief.as_ref() else {
        return Ok(());
    };
    if let Some(origin) = brief.project_origin.as_ref() {
        validate_project_brief_origin(tx, request, brief, origin)?;
        // A project-conversation origin is intentionally cross-document: the
        // retained AuthorRoom message belongs to the project conversation's
        // control anchor, while this request targets an ordinary chapter.
        // The legacy origin path below requires the message's discussion
        // thread to be the chapter itself, so running it as well would reject
        // every valid project brief after approval. The project-origin
        // validator already authenticates the exact message, conversation,
        // project identity, target, scope, text, and AuthorRoom packet.
        return Ok(());
    }
    let Some(origin_id) = brief.origin_message_id.as_deref() else {
        return Ok(());
    };
    let row: Option<(String, String, String, String, Option<String>)> = tx
        .query_row(
            "SELECT dt.project_id,dt.operation_namespace,dt.document_id,dm.role,dr.packet_id
             FROM discussion_messages dm
             JOIN discussion_threads dt ON dt.id=dm.thread_id
             LEFT JOIN discussion_runs dr ON dr.id=dm.run_id
             WHERE dm.id=?",
            [origin_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((project, namespace, document, role, packet_id)) = row else {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message is not available.",
        ));
    };
    if project != request.access.project_id
        || namespace != request.access.operation_namespace
        || document != request.expected.document_id
        || DiscussionMessageRole::parse(&role)? == DiscussionMessageRole::Assistant
            && packet_id.is_none()
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message does not belong to this chapter discussion.",
        ));
    }
    let packet_id = packet_id.ok_or_else(|| {
        CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message has no readable discussion context.",
        )
    })?;
    let snapshot_id: Option<String> = tx
        .query_row(
            "SELECT snapshot_id FROM context_packets WHERE id=?",
            [&packet_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(snapshot_id) = snapshot_id else {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message has no readable discussion context.",
        ));
    };
    let frozen = story_context::load_snapshot(tx, &request.access, &snapshot_id)?;
    if frozen.policy.audience != Audience::AuthorRoom
        || frozen.snapshot.target.document_id != request.expected.document_id
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The writing brief origin message is not readable in the current AuthorRoom policy.",
        ));
    }
    Ok(())
}

fn validate_project_brief_origin(
    tx: &Connection,
    request: &StartDiscussion,
    brief: &SafeBriefInput,
    origin: &crate::context::ProjectBriefOrigin,
) -> CoreResult<()> {
    if origin.version != "project-conversation-brief.v1"
        || origin.project_id != request.access.project_id
        || origin.operation_namespace != request.access.operation_namespace
        || origin.conversation_id.is_empty()
        || origin.message_id.is_empty()
        || origin.target != request.expected
        || origin.text_hash != sha256_hex(brief.text.as_bytes())
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The project conversation brief is bound to another project, target, or text.",
        ));
    }
    let scope_hash = sha256_hex(&serde_json::to_vec(&request.scope)?);
    if origin.scope_hash != scope_hash {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The project conversation brief is bound to another selection scope.",
        ));
    }
    let source: Option<(String, String, String)> = tx
        .query_row(
            "SELECT ci.kind,r.status,cp.snapshot_id
             FROM conversation_items ci
             JOIN discussion_runs r ON r.id=ci.reference_id
             JOIN context_packets cp ON cp.id=r.packet_id
             JOIN discussion_messages dm ON dm.run_id=r.id
             WHERE ci.conversation_id=? AND ci.project_id=?
               AND ci.operation_namespace=? AND r.project_id=?
               AND r.operation_namespace=?
               AND ci.kind IN ('request','chapterRequest')
               AND dm.id=? AND dm.role IN ('user','assistant')",
            params![
                origin.conversation_id,
                request.access.project_id,
                request.access.operation_namespace,
                request.access.project_id,
                request.access.operation_namespace,
                origin.message_id
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((item_kind, status, snapshot_id)) = source else {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "The project conversation brief origin message is not retained in this project.",
        ));
    };
    if item_kind != "request" || status != "completed" {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "A project conversation brief must come from a completed author-room request.",
        ));
    }
    let frozen = story_context::load_snapshot(tx, &request.access, &snapshot_id)?;
    if frozen.policy.audience != Audience::AuthorRoom
        || frozen
            .project_chat
            .as_ref()
            .is_none_or(|chat| chat.conversation_id != origin.conversation_id)
    {
        return Err(CoreError::new(
            "InvalidSafeBrief",
            "A project conversation brief must come from a completed author-room request.",
        ));
    }
    Ok(())
}

fn discussion_context_policy(
    tx: &Connection,
    request: &StartDiscussion,
) -> CoreResult<(ContextPurpose, InformationPolicy)> {
    let policy_epoch: i64 = tx.query_row(
        "SELECT disclosure_policy_epoch FROM project WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    let version = policy_epoch.to_string();
    let purpose = request.intent.purpose();
    match request.intent {
        FeedbackIntent::Discuss | FeedbackIntent::WorkshopExplore => Ok((
            purpose,
            InformationPolicy {
                version,
                audience: Audience::AuthorRoom,
                reader_frontier: None,
                character_id: None,
                character_grants: Vec::new(),
                allow_alternatives: false,
                allow_historical: false,
            },
        )),
        FeedbackIntent::ProposeEdits | FeedbackIntent::Continue => {
            if request.intent == FeedbackIntent::ProposeEdits
                && request.scope.as_ref().is_none_or(|scope| {
                    !matches!(
                        scope.kind,
                        ScopeKind::Passage | ScopeKind::Blocks | ScopeKind::WholeDocument
                    )
                })
            {
                return Err(CoreError::new(
                    "InvalidScope",
                    "Propose edits requires an explicit passage selection.",
                ));
            }
            let target: Option<(String, i64)> = tx
                .query_row(
                    "SELECT kind,position FROM documents WHERE id=? AND trashed=0",
                    [&request.expected.document_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((kind, position)) = target else {
                return Err(CoreError::new(
                    "DocumentNotFound",
                    "The selected chapter is not available.",
                ));
            };
            if request.intent == FeedbackIntent::Continue && kind != "chapter" {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Only chapter documents can continue prose at the end.",
                ));
            }
            if request.intent == FeedbackIntent::ProposeEdits
                && kind != "chapter"
                && request.scope.as_ref().is_none_or(|scope| {
                    !matches!(scope.kind, ScopeKind::Blocks | ScopeKind::WholeDocument)
                })
            {
                return Err(CoreError::new(
                    "InvalidScope",
                    "Document development requires an explicit block or whole-document scope.",
                ));
            }
            if kind == "chapter" && position < 0 {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The selected chapter has an invalid reader position.",
                ));
            }
            if kind != "chapter" {
                return Ok((
                    purpose,
                    InformationPolicy {
                        version,
                        audience: Audience::AuthorRoom,
                        reader_frontier: None,
                        character_id: None,
                        character_grants: Vec::new(),
                        allow_alternatives: false,
                        allow_historical: false,
                    },
                ));
            }
            Ok((
                purpose,
                InformationPolicy {
                    version,
                    audience: Audience::RestrictedWriting,
                    reader_frontier: Some(position.to_string()),
                    character_id: None,
                    character_grants: Vec::new(),
                    allow_alternatives: false,
                    allow_historical: false,
                },
            ))
        }
    }
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

fn merge_pinned_document_ids(
    persistent: &[String],
    transient: &[String],
) -> CoreResult<Vec<String>> {
    let mut transient_seen = std::collections::HashSet::new();
    for id in transient {
        if !transient_seen.insert(id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A transient source document was pinned more than once.",
            ));
        }
    }
    let mut merged = BTreeSet::new();
    merged.extend(persistent.iter().cloned());
    merged.extend(transient.iter().cloned());
    if merged.len() > MAX_PINNED_DOCUMENTS {
        return Err(CoreError::new(
            "InvalidRequest",
            "A discussion may use at most 64 source documents after persistent pins are merged.",
        ));
    }
    Ok(merged.into_iter().collect())
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
    transient_handles: Option<Vec<String>>,
    scope: Option<&ScopeGrant>,
    response_contract: Option<&str>,
) -> CoreResult<()> {
    // context_packets is validated on packet reads and transfers. Persist its
    // canonical PrepareContext envelope rather than the larger discussion
    // request so the packet remains readable through the shared C2 contract.
    let prepared = PrepareContext {
        access: request.access.clone(),
        operation_id: request.operation_id.clone(),
        snapshot_id: packet.receipt.snapshot_id.clone(),
        instruction: packet_instruction(request, response_contract)?,
        mandatory_handles: mandatory_handles.to_vec(),
        transient_mandatory_handles: transient_handles,
        safe_brief: request.safe_brief.clone(),
        scope: scope.cloned(),
        budget: request.budget.clone(),
        provider_binding: request.provider_binding.clone(),
        lookup: request
            .lookup
            .clone()
            .map(|allowance| crate::context::lookup::LookupPacketInput {
                allowance,
                completed_invocations: 0,
                exchanges: Vec::new(),
                source_projection: None,
                reviewed_memory: Some(
                    crate::context::lookup::REVIEWED_MEMORY_CAPABILITY.to_owned(),
                ),
            }),
        response_contract: response_contract
            .map(str::to_owned)
            .or_else(|| match request.intent {
                FeedbackIntent::Discuss if request.lookup.is_some() => {
                    Some(LOOKUP_RESPONSE_CONTRACT.to_owned())
                }
                FeedbackIntent::Continue => Some(CONTINUATION_RESPONSE_CONTRACT.to_owned()),
                FeedbackIntent::ProposeEdits if packet.options.provider_binding.is_some() => Some(
                    if scope.is_some_and(|scope| {
                        matches!(scope.kind, ScopeKind::Blocks | ScopeKind::WholeDocument)
                    }) {
                        STRUCTURED_PROPOSAL_RESPONSE_CONTRACT.to_owned()
                    } else {
                        PROPOSAL_RESPONSE_CONTRACT.to_owned()
                    },
                ),
                FeedbackIntent::WorkshopExplore => Some(WORKSHOP_RESPONSE_CONTRACT.to_owned()),
                _ => None,
            }),
    };
    let payload_hash = logical_hash(&prepared)?;
    let request_json = serde_json::to_string(&prepared)?;
    let packet_json = serde_json::to_string(packet)?;
    tx.execute("INSERT INTO context_packets(id,project_id,operation_namespace,operation_id,payload_hash,request_json,snapshot_id,session_id,invocation_ordinal,packet_json,packet_hash,input_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)", params![packet.receipt.packet_id, request.access.project_id, request.access.operation_namespace, request.operation_id, payload_hash, request_json, packet.receipt.snapshot_id, packet.receipt.session_id, parse_version(&packet.receipt.invocation_ordinal)?, packet_json, sha256_hex(packet_json.as_bytes()), packet.receipt.input_hash])?;
    Ok(())
}

fn packet_instruction(
    request: &StartDiscussion,
    response_contract: Option<&str>,
) -> CoreResult<String> {
    if response_contract
        == Some(crate::projects::project_chat_output::CHAPTER_DISCUSSION_RESPONSE_CONTRACT)
    {
        let target = serde_json::to_string(&request.expected)?;
        return Ok(format!(
            "{}\n\n{}\n{}",
            request.instruction,
            crate::projects::project_chat_output::CHAPTER_TARGET_HEAD_MARKER,
            target
        ));
    }
    Ok(request.instruction.clone())
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

pub(super) fn read_start(db: &Connection, run_id: &str) -> CoreResult<DiscussionStart> {
    let run = read_run(db, run_id)?;
    let user_message = db.query_row("SELECT id FROM discussion_messages WHERE run_id=? AND role='user' ORDER BY created_at,id LIMIT 1", [run_id], |row| row.get::<_,String>(0)).map_err(CoreError::from).and_then(|id| read_message(db, &id))?;
    // Safe-brief starts are explicit author actions whose receipt must remain
    // replayable after a later policy bump. Ordinary discussion receipts keep
    // the existing current-policy read boundary.
    let retained = context_packets::validated_packet_record(db, &run.packet_id)?;
    let packet = if retained.receipt.safe_brief.is_some() {
        retained
    } else {
        context_packets::read_context_packet_at(
            db,
            &ProjectAccess {
                project_id: run.owner.project_id.clone(),
                operation_namespace: run.owner.operation_namespace.clone(),
                session: String::new(),
                writer_lease: String::new(),
            },
            &run.packet_id,
        )?
    };
    Ok(DiscussionStart {
        thread_id: run.thread_id.clone(),
        run,
        user_message,
        packet,
    })
}

pub(super) fn read_run(db: &Connection, run_id: &str) -> CoreResult<DiscussionRun> {
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
    let (intent, basis) = intent_for_packet(db, &row.9)?;
    let packet = context_packets::validated_packet_record(db, &row.9)?;
    let provider_binding = packet.options.provider_binding;
    let provider_result = read_provider_result(db, &row.0, &row.9)?;
    let lookup = discussion_lookup::read_summary(db, &row.0)?;
    if let Some(result) = &provider_result {
        let packet_binding = provider_binding.as_ref().ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A provider result exists for a packet without a provider binding.",
            )
        })?;
        if &result.binding != packet_binding || result.packet_id != row.9 {
            return Err(CoreError::new(
                "InvalidProject",
                "The saved provider result does not match its immutable packet binding.",
            ));
        }
    }
    Ok(DiscussionRun {
        id: row.0.clone(),
        thread_id: row.1,
        owner: RunOwner {
            project_id: row.2,
            operation_namespace: row.3,
            run_id: row.0,
        },
        operation_id: row.4,
        intent,
        basis,
        payload_hash: row.5,
        target: Head {
            document_id: row.6,
            version: row.7.to_string(),
            body_hash: row.8,
        },
        packet_id: row.9,
        provider_binding,
        provider_result,
        lookup,
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

type ProviderResultRow = (
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
    i64,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
);

fn read_provider_result(
    db: &Connection,
    run_id: &str,
    packet_id: &str,
) -> CoreResult<Option<ProviderResult>> {
    let row: Option<ProviderResultRow> = db
        .query_row(
            "SELECT run_id,packet_id,terminal_event_id,expected_sequence,binding_json,assistant_text,outcome,confirmed_stdin_bytes,usage_json,cleanup,error,effective_identity,reported_model,created_at,delivery_json,app_server_delivery_json FROM provider_results WHERE run_id=?",
            [run_id],
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
        saved_run_id,
        saved_packet_id,
        event_id,
        expected_sequence,
        binding_json,
        assistant_text,
        outcome,
        confirmed_stdin_bytes,
        usage_json,
        cleanup,
        error,
        effective_identity,
        reported_model,
        created_at,
        delivery_json,
        app_server_json,
    )) = row
    else {
        return Ok(None);
    };
    if saved_run_id != run_id || saved_packet_id != packet_id {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved provider result belongs to a different run or packet.",
        ));
    }
    let binding: ProviderBinding = serde_json::from_str(&binding_json)?;
    binding
        .validate()
        .map_err(|message| CoreError::new("InvalidProject", &message))?;
    let confirmed_stdin_bytes = u64::try_from(confirmed_stdin_bytes).map_err(|_| {
        CoreError::new(
            "InvalidProject",
            "The saved provider stdin byte count is negative.",
        )
    })?;
    let expected_sequence = parse_stored_version(expected_sequence)?;
    let usage = usage_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?;
    let delivery = delivery_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?;
    let app_server = app_server_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?;
    let status = ProviderOutcomeStatus::parse(&outcome)?;
    validate_reported_model(&binding, status, reported_model.as_deref(), true)?;
    let cleanup = ProviderCleanup::parse(&cleanup)?;
    let input_limit = binding
        .input_limit()
        .map_err(|message| CoreError::new("InvalidProject", &message))?;
    if crate::providers::codex_app_server::is_app_server(&binding) {
        if confirmed_stdin_bytes != 0 || delivery.is_some() {
            return Err(CoreError::new(
                "InvalidProject",
                "App-server results cannot claim exec or HTTP delivery.",
            ));
        }
        let packet = context_packets::validated_packet_record(db, packet_id)?;
        app_server::validate_delivery(db, run_id, &packet, app_server.as_ref(), status, cleanup)?;
    } else if app_server.is_some() {
        return Err(CoreError::new(
            "InvalidProject",
            "Only app-server results can retain app-server delivery.",
        ));
    } else if binding.is_http() {
        if confirmed_stdin_bytes != 0 {
            return Err(CoreError::new(
                "InvalidProject",
                "An HTTP provider result must retain a zero Codex stdin count.",
            ));
        }
        let packet = context_packets::validated_packet_record(db, packet_id)?;
        validate_stored_http_delivery(&packet, &binding, delivery.as_ref())?;
        if status == ProviderOutcomeStatus::Completed
            && !matches!(
                delivery.as_ref().map(|receipt| receipt.submission),
                Some(HttpDeliverySubmission::ResponseReceived)
            )
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A completed HTTP provider result needs complete response evidence.",
            ));
        }
    } else if delivery.is_some()
        || assistant_text.len() > CODEX_OUTPUT_LIMIT_BYTES
        || confirmed_stdin_bytes > input_limit as u64
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved Codex provider result has invalid transport evidence.",
        ));
    }
    let output_limit = binding
        .output_limit()
        .map_err(|message| CoreError::new("InvalidProject", &message))?;
    if assistant_text.len() > output_limit
        || (status == ProviderOutcomeStatus::Completed && assistant_text.is_empty())
        || (status == ProviderOutcomeStatus::Completed && error.is_some())
        || error.as_deref().is_some_and(|value| {
            value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control)
        })
        || effective_identity.is_some()
    {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved provider result violates the bounded terminal contract.",
        ));
    }
    Ok(Some(ProviderResult {
        run_id: saved_run_id,
        packet_id: saved_packet_id,
        event_id,
        expected_sequence,
        assistant_text,
        binding,
        status,
        confirmed_stdin_bytes: confirmed_stdin_bytes.to_string(),
        usage,
        cleanup,
        error,
        effective_identity,
        reported_model,
        created_at,
        delivery,
        app_server,
    }))
}

/// Validate immutable provider receipts when opening or transferring a
/// project. This checks the receipt's local fences and packet binding; it does
/// not claim that the external process itself can be reconstructed.
pub(crate) fn validate_provider_results(db: &Connection) -> CoreResult<()> {
    app_server::validate_dispatches(db)?;
    let mut statement = db.prepare("SELECT run_id,packet_id FROM provider_results")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (run_id, packet_id) in rows {
        let result = read_provider_result(db, &run_id, &packet_id)?.ok_or_else(|| {
            CoreError::new(
                "InvalidProject",
                "A provider result disappeared during validation.",
            )
        })?;
        let packet = context_packets::validated_packet_record(db, &packet_id)?;
        if packet.options.provider_binding.as_ref() != Some(&result.binding) {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result does not match its immutable packet binding.",
            ));
        }
        let input_len = serialized_input(&packet.messages, &packet.options)
            .map_err(packet_error)?
            .len() as u64;
        let confirmed = parse_decimal_u64(&result.confirmed_stdin_bytes)?;
        if crate::providers::codex_app_server::is_app_server(&result.binding) {
            if confirmed != 0 {
                return Err(CoreError::new(
                    "InvalidProject",
                    "App-server receipts cannot claim exec stdin delivery.",
                ));
            }
            app_server::validate_delivery(
                db,
                &run_id,
                &packet,
                result.app_server.as_ref(),
                result.status,
                result.cleanup,
            )?;
        } else if result.binding.is_http() {
            if confirmed != 0 {
                return Err(CoreError::new(
                    "InvalidProject",
                    "An HTTP provider result must retain a zero Codex stdin count.",
                ));
            }
            validate_stored_http_delivery(&packet, &result.binding, result.delivery.as_ref())?;
        } else if confirmed > input_len
            || (result.status == ProviderOutcomeStatus::Completed && confirmed != input_len)
        {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result has an invalid frozen-packet byte count.",
            ));
        }
        let (status, sequence, output_text): (String, i64, String) = db.query_row(
            "SELECT status,sequence,output_text FROM discussion_runs WHERE id=? AND packet_id=?",
            params![run_id, packet_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let status = DiscussionRunStatus::parse(&status)?;
        if status.active() || output_text != result.assistant_text {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result does not match its terminal discussion run.",
            ));
        }
        let expected_sequence = parse_version(&result.expected_sequence)?;
        if expected_sequence.checked_add(1) != Some(sequence) {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result has an invalid terminal sequence.",
            ));
        }
        let event: Option<(String, i64)> = db
            .query_row(
                "SELECT kind,sequence FROM discussion_output_events WHERE run_id=? AND event_id=?",
                params![run_id, result.event_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if event != Some(("terminal".to_owned(), sequence)) {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result has no matching immutable terminal event.",
            ));
        }
        let allowed = if result.cleanup == ProviderCleanup::Unresolved {
            status == DiscussionRunStatus::Interrupted
        } else {
            match result.status {
                ProviderOutcomeStatus::Completed => {
                    matches!(
                        status,
                        DiscussionRunStatus::Completed | DiscussionRunStatus::Stopped
                    )
                }
                ProviderOutcomeStatus::Stopped => status == DiscussionRunStatus::Stopped,
                ProviderOutcomeStatus::TimedOut
                | ProviderOutcomeStatus::OutputLimit
                | ProviderOutcomeStatus::Failed => {
                    matches!(
                        status,
                        DiscussionRunStatus::Failed | DiscussionRunStatus::Stopped
                    )
                }
            }
        };
        if !allowed {
            return Err(CoreError::new(
                "InvalidProject",
                "A provider result outcome does not match its terminal run status.",
            ));
        }
    }
    let mut live_statement = db.prepare(
        "SELECT dr.id,dr.packet_id,dr.status
         FROM discussion_runs dr
         ORDER BY dr.id",
    )?;
    let live_runs = live_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (run_id, packet_id, status) in live_runs {
        if status != DiscussionRunStatus::Completed.as_str() {
            continue;
        }
        let packet = context_packets::validated_packet_record(db, &packet_id)?;
        if packet.options.provider_binding.is_some() {
            let receipt: Option<String> = db
                .query_row(
                    "SELECT run_id FROM provider_results WHERE run_id=?",
                    [&run_id],
                    |row| row.get(0),
                )
                .optional()?;
            if receipt.is_none() {
                return Err(CoreError::new(
                    "InvalidProject",
                    "A completed live discussion is missing its immutable provider result.",
                ));
            }
        }
    }
    Ok(())
}

/// Discussion intent is part of the immutable context contract. Keeping it
/// there means a recovered database and old run rows do not need a second,
/// mutable intent column whose value could drift from the packet.
fn intent_for_packet(
    db: &Connection,
    packet_id: &str,
) -> CoreResult<(FeedbackIntent, Option<BasisKind>)> {
    let (snapshot_id, manifest, manifest_hash): (String, String, String) = db
        .query_row(
            "SELECT p.snapshot_id,s.manifest_json,s.manifest_hash
             FROM context_packets p JOIN story_snapshots s ON s.id=p.snapshot_id
             WHERE p.id=?",
            [packet_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::new(
                "InvalidContext",
                "The discussion packet has no frozen story context.",
            )
        })?;
    let frozen = story_context::decode_snapshot(&manifest, &manifest_hash)?;
    if frozen.snapshot.snapshot_id != snapshot_id {
        return Err(CoreError::new(
            "InvalidContext",
            "The discussion packet points to a different story snapshot.",
        ));
    }
    let intent = FeedbackIntent::from_purpose(frozen.purpose)?;
    Ok((
        intent,
        (intent == FeedbackIntent::Continue).then_some(frozen.snapshot.basis),
    ))
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
    type DraftRow = (
        String,
        i64,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let row: Option<DraftRow> = db
        .query_row(
            "SELECT document_id,version,text,intent,scope_json,pinned_document_ids_json,updated_at,previous_run_id,safe_brief_json,basis,lookup_json FROM discussion_drafts WHERE project_id=? AND operation_namespace=? AND document_id=?",
            params![access.project_id, access.operation_namespace, document_id],
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
                ))
            },
        )
        .optional()?;
    let Some((
        document_id,
        version,
        text,
        intent,
        scope_json,
        pins_json,
        updated_at,
        previous_run_id,
        safe_brief_json,
        basis,
        lookup_json,
    )) = row
    else {
        return Ok(None);
    };
    let intent = FeedbackIntent::parse(&intent)?;
    let basis = basis
        .map(|label| serde_json::from_value::<BasisKind>(Value::String(label)))
        .transpose()?;
    let scope: Option<DiscussionScopeInput> = scope_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?;
    validate_feedback_basis(intent, basis, scope.as_ref())?;
    Ok(Some(DiscussionDraft {
        document_id,
        version: parse_stored_version(version)?,
        text,
        intent,
        basis,
        scope,
        pinned_document_ids: serde_json::from_str(&pins_json)?,
        safe_brief: safe_brief_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
        previous_run_id,
        updated_at,
        lookup: lookup_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
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

fn reject_legacy_lookup_path(run: &DiscussionRun) -> CoreResult<()> {
    if run.lookup.is_some() {
        return Err(CoreError::new(
            "LookupInvocationRequired",
            "This discussion opted into bounded lookup and must use its lookup invocation protocol.",
        ));
    }
    Ok(())
}

fn parse_lookup_ordinal(value: &str) -> CoreResult<u8> {
    let ordinal = parse_version(value)?;
    u8::try_from(ordinal)
        .ok()
        .filter(|ordinal| *ordinal < discussion_lookup::MAX_LOOKUP_INVOCATIONS)
        .ok_or_else(|| {
            CoreError::new(
                "InvalidLookupCounter",
                "A lookup invocation ordinal must be 0, 1, or 2.",
            )
        })
}

fn validate_lookup_report_shape(
    request: &discussion_lookup::LookupInvocationReport,
) -> CoreResult<()> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.run_id)?;
    check_id(&request.event_id)?;
    parse_lookup_ordinal(&request.ordinal)?;
    if request.assistant_text.len() > MAX_OUTPUT_BYTES
        || request
            .assistant_text
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The lookup provider response is too large or contains control characters.",
        ));
    }
    if let Some(error) = &request.error
        && (error.is_empty() || error.len() > 4096 || error.chars().any(char::is_control))
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The lookup provider error must be sanitized and at most 4 KiB.",
        ));
    }
    if request.status == ProviderOutcomeStatus::Completed {
        if request.assistant_text.is_empty() || request.error.is_some() {
            return Err(CoreError::new(
                "InvalidRequest",
                "A completed lookup invocation must include only a response envelope.",
            ));
        }
        if request.cleanup == ProviderCleanup::Unresolved {
            return Err(CoreError::new(
                "InvalidRequest",
                "A completed lookup invocation must have settled cleanup.",
            ));
        }
    }
    Ok(())
}

fn validate_lookup_provider_bytes(
    tx: &Connection,
    identity: &discussion_lookup::InvocationIdentity,
    packet: &CompiledPacket,
    request: &discussion_lookup::LookupInvocationReport,
) -> CoreResult<()> {
    let confirmed = parse_decimal_u64(&request.confirmed_stdin_bytes)?;
    let serialized = serialized_input(&packet.messages, &packet.options).map_err(packet_error)?;
    if confirmed > serialized.len() as u64
        || (request.status == ProviderOutcomeStatus::Completed
            && confirmed != serialized.len() as u64)
    {
        return Err(CoreError::new(
            "ProviderInputMismatch",
            "The lookup provider did not consume the exact frozen packet input.",
        ));
    }
    let output_limit = packet
        .options
        .provider_binding
        .as_ref()
        .map(|binding| binding.output_limit())
        .transpose()
        .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?
        .unwrap_or(MAX_OUTPUT_BYTES);
    if request.assistant_text.len() > output_limit {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The lookup provider response exceeds the application output cap.",
        ));
    }
    let (previous_input, previous_output): (i64, i64) = tx.query_row(
        "SELECT COALESCE(SUM(confirmed_stdin_bytes),0),COALESCE(SUM(length(CAST(assistant_text AS BLOB))),0) FROM discussion_lookup_results WHERE run_id=?",
        [&identity.run_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let total_input = u128::try_from(previous_input)
        .unwrap_or(u128::MAX)
        .saturating_add(u128::from(confirmed));
    let total_output = u128::try_from(previous_output)
        .unwrap_or(u128::MAX)
        .saturating_add(request.assistant_text.len() as u128);
    let input_limit = parse_decimal_u64(&identity.allowance.total_input_bytes)? as u128;
    let output_limit = parse_decimal_u64(&identity.allowance.total_output_bytes)? as u128;
    if total_input > input_limit || total_output > output_limit {
        return Err(CoreError::new(
            "LookupAllowanceExceeded",
            "The lookup invocation would exceed its application byte allowance.",
        ));
    }
    Ok(())
}

type LookupResultIdentity = (
    String,
    String,
    String,
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn lookup_result_record(
    tx: &Connection,
    run_id: &str,
    ordinal: u8,
) -> CoreResult<Option<LookupResultIdentity>> {
    tx.query_row(
        "SELECT event_id,assistant_text,outcome,confirmed_stdin_bytes,cleanup,error,binding_json,usage_json FROM discussion_lookup_results WHERE run_id=? AND ordinal=?",
        params![run_id, i64::from(ordinal)],
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
            ))
        },
    )
    .optional()
    .map_err(CoreError::from)
}

fn lookup_result_matches(
    saved: &LookupResultIdentity,
    request: &discussion_lookup::LookupInvocationReport,
) -> bool {
    let binding_json = request
        .binding
        .as_ref()
        .and_then(|binding| serde_json::to_string(binding).ok());
    let usage_json = request
        .usage
        .as_ref()
        .and_then(|usage| serde_json::to_string(usage).ok());
    let expected_outcome = request.status.as_str();
    let confirmed_stdin_matches = parse_decimal_u64(&request.confirmed_stdin_bytes)
        .ok()
        .and_then(|value| i64::try_from(value).ok())
        == Some(saved.3);
    let error_matches = saved.5 == request.error
        || (request.status == ProviderOutcomeStatus::Completed
            && saved.2 == "failed"
            && request.error.is_none()
            && saved.5.is_some());
    saved.0 == request.event_id
        && saved.1 == request.assistant_text
        && (saved.2 == expected_outcome
            || (request.status == ProviderOutcomeStatus::Completed && saved.2 == "failed"))
        && confirmed_stdin_matches
        && saved.4 == request.cleanup.as_str()
        && error_matches
        && saved.6 == binding_json
        && saved.7 == usage_json
}

fn read_lookup_packet(
    tx: &Connection,
    owner: &RunOwner,
    packet_id: &str,
) -> CoreResult<CompiledPacket> {
    context_packets::read_context_packet_at(
        tx,
        &ProjectAccess {
            project_id: owner.project_id.clone(),
            operation_namespace: owner.operation_namespace.clone(),
            session: String::new(),
            writer_lease: String::new(),
        },
        packet_id,
    )
}

fn seal_lookup_discussion(
    tx: &Connection,
    current: &DiscussionRun,
    packet_id: &str,
    event_id: &str,
    text: &str,
) -> CoreResult<DiscussionRun> {
    if current.status != DiscussionRunStatus::Running {
        return Err(CoreError::new(
            "RunSealed",
            "A lookup discussion can finish only while running.",
        ));
    }
    validate_finish_request(&current.owner, event_id, text)?;
    let sequence = parse_version(&current.sequence)?;
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| CoreError::new("InvalidRequest", "The output sequence is exhausted."))?;
    tx.execute(
        "INSERT INTO discussion_output_events(run_id,sequence,event_id,kind,chunk) VALUES(?,?,?,?,?)",
        params![current.id, next, event_id, "terminal", text],
    )?;
    let changed = tx.execute(
        "UPDATE discussion_runs SET status='completed',sequence=?,output_text=?,stop_reason=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND project_id=? AND operation_namespace=? AND status='running' AND sequence=?",
        params![next, text, current.id, current.owner.project_id, current.owner.operation_namespace, sequence],
    )?;
    if changed != 1 {
        return Err(CoreError::new(
            "SequenceConflict",
            "The lookup discussion changed before its final response was committed.",
        ));
    }
    tx.execute(
        "INSERT INTO discussion_messages(id,thread_id,run_id,role,content,packet_id) VALUES(?,?,?,?,?,?)",
        params![new_id(), current.thread_id, current.id, DiscussionMessageRole::Assistant.as_str(), text, packet_id],
    )?;
    read_run(tx, &current.id)
}

fn lookup_failure_reason(status: ProviderOutcomeStatus, cleanup: ProviderCleanup) -> &'static str {
    if cleanup == ProviderCleanup::Unresolved {
        "lookup_cleanup_unresolved"
    } else {
        match status {
            ProviderOutcomeStatus::Completed => "lookup_invalid_response",
            ProviderOutcomeStatus::Stopped => "lookup_stopped",
            ProviderOutcomeStatus::TimedOut => "lookup_timed_out",
            ProviderOutcomeStatus::OutputLimit => "lookup_output_limit",
            ProviderOutcomeStatus::Failed => "lookup_failed",
        }
    }
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

fn validate_finish_request(owner: &RunOwner, event_id: &str, text: &str) -> CoreResult<()> {
    check_id(&owner.project_id)?;
    check_id(&owner.operation_namespace)?;
    check_id(&owner.run_id)?;
    check_id(event_id)?;
    if text.is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A completed discussion must include assistant output.",
        ));
    }
    if text.len() > MAX_OUTPUT_BYTES {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The discussion output exceeds the durable limit.",
        ));
    }
    Ok(())
}

fn validate_settlement_request(request: &DiscussionStopSettled) -> CoreResult<()> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.run_id)?;
    check_id(&request.event_id)?;
    if request.assistant_text.len() > MAX_OUTPUT_BYTES {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The stopped discussion output exceeds the durable limit.",
        ));
    }
    Ok(())
}

fn parse_decimal_u64(value: &str) -> CoreResult<u64> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|b| b.is_ascii_digit())
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

fn validate_provider_report_shape(request: &ProviderTerminalReport) -> CoreResult<()> {
    check_id(&request.owner.project_id)?;
    check_id(&request.owner.operation_namespace)?;
    check_id(&request.owner.run_id)?;
    check_id(&request.event_id)?;
    request
        .binding
        .validate()
        .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
    let bytes = parse_decimal_u64(&request.confirmed_stdin_bytes)?;
    if (request.binding.is_http()
        || crate::providers::codex_app_server::is_app_server(&request.binding))
        && bytes != 0
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "An HTTP provider report must retain a zero Codex stdin count.",
        ));
    }
    if !request.binding.is_http() && bytes > CODEX_INPUT_LIMIT_BYTES as u64 {
        return Err(CoreError::new(
            "InputTooLarge",
            "The provider stdin exceeds the application byte cap.",
        ));
    }
    let output_limit = request
        .binding
        .output_limit()
        .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
    if request.assistant_text.len() > output_limit {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The provider output exceeds the application byte cap.",
        ));
    }
    if request.binding.is_http() {
        validate_http_delivery_shape(request.delivery.as_ref(), request.status)?;
    } else if request.delivery.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A Codex provider report cannot contain HTTP delivery evidence.",
        ));
    }
    if crate::providers::codex_app_server::is_app_server(&request.binding) {
        let receipt = request.app_server.as_ref().ok_or_else(|| {
            CoreError::new(
                "InvalidAppServerDelivery",
                "App-server delivery evidence is required.",
            )
        })?;
        receipt.validate()?;
    } else if request.app_server.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "App-server evidence requires an app-server binding.",
        ));
    }
    if let Some(error) = &request.error
        && (error.is_empty() || error.len() > 4096 || error.chars().any(char::is_control))
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The provider error must be sanitized and at most 4 KiB.",
        ));
    }
    if request.effective_identity.is_some() {
        return Err(CoreError::new(
            "InvalidRequest",
            "The current provider boundary cannot confirm an effective identity.",
        ));
    }
    validate_reported_model(
        &request.binding,
        request.status,
        request.reported_model.as_deref(),
        false,
    )?;
    Ok(())
}

/// Claude's stream may claim a terminal model identity.  Keep that claim
/// separate from the immutable requested binding and accept it only for the
/// bounded Claude profile.  A completed Claude result must agree exactly;
/// failed results may retain a bounded, known Claude model for diagnosis.
fn validate_reported_model(
    binding: &ProviderBinding,
    status: ProviderOutcomeStatus,
    reported_model: Option<&str>,
    persisted: bool,
) -> CoreResult<()> {
    let Some(reported_model) = reported_model else {
        if binding.is_claude() && status == ProviderOutcomeStatus::Completed {
            return Err(CoreError::new(
                if persisted {
                    "InvalidProject"
                } else {
                    "InvalidRequest"
                },
                "A completed Claude result must retain its reported model identity.",
            ));
        }
        return Ok(());
    };
    if !binding.is_claude()
        || !crate::providers::claude_profile::valid_reported_model_id(reported_model)
    {
        return Err(CoreError::new(
            if persisted {
                "InvalidProject"
            } else {
                "InvalidRequest"
            },
            "Only a bounded Claude result may retain a known reported model identity.",
        ));
    }
    if status == ProviderOutcomeStatus::Completed && reported_model != binding.model_id {
        return Err(CoreError::new(
            if persisted {
                "InvalidProject"
            } else {
                "InvalidRequest"
            },
            "A completed Claude result reported a different model than the request binding.",
        ));
    }
    Ok(())
}

fn validate_http_delivery_shape(
    delivery: Option<&ProviderDeliveryReceipt>,
    status: ProviderOutcomeStatus,
) -> CoreResult<()> {
    let Some(delivery) = delivery else {
        return Err(CoreError::new(
            "InvalidRequest",
            "An OpenAI-compatible provider report needs HTTP delivery evidence.",
        ));
    };
    if delivery.body_hash.len() != 64
        || !delivery
            .body_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "The HTTP request body hash is invalid.",
        ));
    }
    let body_bytes = parse_decimal_u64(&delivery.body_bytes)?;
    if body_bytes == 0 || body_bytes > crate::context::packet::HTTP_INPUT_LIMIT_BYTES as u64 {
        return Err(CoreError::new(
            "InvalidRequest",
            "The HTTP request body byte count is outside the application limit.",
        ));
    }
    if status == ProviderOutcomeStatus::Completed
        && delivery.submission != HttpDeliverySubmission::ResponseReceived
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "A completed HTTP provider result needs a fully received response.",
        ));
    }
    Ok(())
}

fn validate_http_delivery(
    packet: &CompiledPacket,
    request: &ProviderTerminalReport,
) -> CoreResult<()> {
    validate_http_delivery_shape(request.delivery.as_ref(), request.status)?;
    let delivery = request.delivery.as_ref().expect("validated above");
    let prepared = prepare_http_request(&packet.messages, &packet.options)?;
    if delivery.body_hash != prepared.body_hash
        || delivery.body_bytes != prepared.body_bytes
        || request.status == ProviderOutcomeStatus::Completed
            && delivery.submission != HttpDeliverySubmission::ResponseReceived
    {
        return Err(CoreError::new(
            "ProviderInputMismatch",
            "The HTTP delivery receipt does not match the immutable request body.",
        ));
    }
    Ok(())
}

fn validate_stored_http_delivery(
    packet: &CompiledPacket,
    binding: &ProviderBinding,
    delivery: Option<&ProviderDeliveryReceipt>,
) -> CoreResult<()> {
    let Some(delivery) = delivery else {
        return Err(CoreError::new(
            "InvalidProject",
            "An HTTP provider result is missing its delivery receipt.",
        ));
    };
    validate_http_delivery_shape(Some(delivery), ProviderOutcomeStatus::Failed)?;
    if packet.options.provider_binding.as_ref() != Some(binding) {
        return Err(CoreError::new(
            "InvalidProject",
            "The HTTP delivery receipt does not match its packet binding.",
        ));
    }
    let prepared = prepare_http_request(&packet.messages, &packet.options)?;
    if delivery.body_hash != prepared.body_hash || delivery.body_bytes != prepared.body_bytes {
        return Err(CoreError::new(
            "InvalidProject",
            "The saved HTTP delivery receipt does not match the immutable request body.",
        ));
    }
    Ok(())
}

fn provider_result_matches_report(saved: &ProviderResult, report: &ProviderTerminalReport) -> bool {
    saved.run_id == report.owner.run_id
        && saved.event_id == report.event_id
        && saved.expected_sequence == report.expected_sequence
        && saved.assistant_text == report.assistant_text
        && saved.binding == report.binding
        && saved.status == report.status
        && saved.confirmed_stdin_bytes == report.confirmed_stdin_bytes
        && saved.app_server == report.app_server
        && saved.usage == report.usage
        && saved.cleanup == report.cleanup
        && saved.error == report.error
        && saved.effective_identity == report.effective_identity
        && saved.reported_model == report.reported_model
        && saved.delivery == report.delivery
}

fn provider_discussion_status(
    current: DiscussionRunStatus,
    outcome: ProviderOutcomeStatus,
    cleanup: ProviderCleanup,
) -> DiscussionRunStatus {
    if cleanup == ProviderCleanup::Unresolved {
        DiscussionRunStatus::Interrupted
    } else if current == DiscussionRunStatus::Stopping || outcome == ProviderOutcomeStatus::Stopped
    {
        DiscussionRunStatus::Stopped
    } else if outcome == ProviderOutcomeStatus::Completed {
        DiscussionRunStatus::Completed
    } else {
        DiscussionRunStatus::Failed
    }
}

fn provider_stop_reason(
    current: DiscussionRunStatus,
    outcome: ProviderOutcomeStatus,
    cleanup: ProviderCleanup,
) -> Option<&'static str> {
    if cleanup == ProviderCleanup::Unresolved {
        Some("provider_cleanup_unresolved")
    } else if current == DiscussionRunStatus::Stopping {
        Some("author_stopped")
    } else {
        match outcome {
            ProviderOutcomeStatus::Completed => None,
            ProviderOutcomeStatus::Stopped => Some("provider_stopped"),
            ProviderOutcomeStatus::TimedOut => Some("provider_timed_out"),
            ProviderOutcomeStatus::OutputLimit => Some("provider_output_limit"),
            ProviderOutcomeStatus::Failed => Some("provider_failed"),
        }
    }
}

fn provider_terminal_message(
    request: &ProviderTerminalReport,
    status: DiscussionRunStatus,
) -> CoreResult<String> {
    if status == DiscussionRunStatus::Completed {
        return Ok(request.assistant_text.clone());
    }
    let message = if let Some(error) = request.error.as_deref() {
        format!("Provider request failed: {error}")
    } else {
        match status {
            DiscussionRunStatus::Stopped => STOP_SETTLED_MESSAGE.to_owned(),
            DiscussionRunStatus::Interrupted => STOP_UNRESOLVED_MESSAGE.to_owned(),
            DiscussionRunStatus::Failed => match request.status {
                ProviderOutcomeStatus::TimedOut => "The provider request timed out.".to_owned(),
                ProviderOutcomeStatus::OutputLimit => {
                    "The provider output reached the application limit.".to_owned()
                }
                _ => "The provider request failed.".to_owned(),
            },
            _ => "The provider request ended without a completed response.".to_owned(),
        }
    };
    if message.is_empty() || message.len() > MAX_EVENT_BYTES {
        return Err(CoreError::new(
            "InvalidRequest",
            "The provider terminal message is too large.",
        ));
    }
    Ok(message)
}

fn validate_final_output(current: &str, final_text: &str, allow_empty: bool) -> CoreResult<()> {
    if !allow_empty && final_text.is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A completed discussion must include assistant output.",
        ));
    }
    if final_text.len() > MAX_OUTPUT_BYTES {
        return Err(CoreError::new(
            "OutputTooLarge",
            "The discussion output exceeds the durable limit.",
        ));
    }
    if !final_text.starts_with(current) {
        return Err(CoreError::new(
            "OutputConflict",
            "A terminal discussion result cannot replace persisted output.",
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
