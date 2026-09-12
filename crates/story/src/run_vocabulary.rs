//! The discussion run lifecycle's vocabulary, shared below both conversation crates.
//!
//! Moved down from `webnovel-core`'s `projects/discussions.rs` and
//! `projects/discussion_lookup.rs`. A workshop request *is* a discussion run:
//! `workshop` (bound for `wns-workshop`, L5) reads `DiscussionRun`,
//! `DiscussionRunStatus` and `DiscussionStart`, while `discussions`,
//! `discussion_lookup`, `proposals` and `project_chat` (all bound for
//! `wns-conversation`, L5) own them. Vocabulary two future siblings both need
//! has to sit below both.
//!
//! **The readers did not come.** `read_run` and `read_start` call
//! `intent_for_packet`, `read_provider_result` and `read_message`, and their call
//! graph is roughly as large again as these types. They stay in `discussions`,
//! and the two workshop call sites reach a run through the host trait instead —
//! the same inversion `hold_context_after_commit_before_ack` already uses.
//!
//! Both source modules re-export everything at the historical paths.

use crate::discussion_vocabulary::FeedbackIntent;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use wns_context::lookup::LookupAllowance;
use wns_context::packet::{CompiledPacket, ProviderBinding};
use wns_context::BasisKind;
use wns_documents::ScopeGrant;
use wns_kernel::{CoreError, CoreResult, Head};
use wns_providers::vocabulary::{
    ProviderCleanup, ProviderDeliveryReceipt, ProviderOutcomeStatus, ProviderUsage,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunOwner {
    pub project_id: String,
    pub operation_namespace: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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
    pub fn as_str(self) -> &'static str {
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
    pub fn parse(value: &str) -> CoreResult<Self> {
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
    pub fn active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Stopping)
    }
    pub fn terminal(self) -> bool {
        !self.active()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DiscussionMessageRole {
    User,
    Assistant,
}

impl DiscussionMessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
    pub fn parse(value: &str) -> CoreResult<Self> {
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
    pub app_server: Option<wns_providers::codex_app_server::AppServerDelivery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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
    pub app_server: Option<wns_providers::codex_app_server::AppServerDelivery>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDiscussionSettlement {
    pub run: DiscussionRun,
    pub provider_result: ProviderResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
    pub lookup: Option<LookupRunSummary>,
    pub previous_run_id: Option<String>,
    pub status: DiscussionRunStatus,
    pub dispatch_state: String,
    pub sequence: String,
    pub output_text: String,
    pub stop_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionStart {
    pub thread_id: String,
    pub run: DiscussionRun,
    pub user_message: DiscussionMessage,
    pub packet: CompiledPacket,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LookupInvocationState {
    Prepared,
    Claimed,
    NeedsContext,
    Completed,
    Failed,
    Stopped,
    Unknown,
}

impl LookupInvocationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Claimed => "claimed",
            Self::NeedsContext => "needs_context",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "prepared" => Ok(Self::Prepared),
            "claimed" => Ok(Self::Claimed),
            "needs_context" => Ok(Self::NeedsContext),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "stopped" => Ok(Self::Stopped),
            "unknown" => Ok(Self::Unknown),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved lookup invocation has an unknown state.",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LookupInvocationSummary {
    pub ordinal: String,
    pub packet_id: String,
    pub state: LookupInvocationState,
    /// Whether the provider receipt confirms the complete serialized packet was
    /// written. This is independent of whether the invocation produced a
    /// usable answer or failed after receiving its input.
    pub input_delivered: bool,
    pub response: Option<Value>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LookupRunSummary {
    pub allowance: LookupAllowance,
    pub invocations: Vec<LookupInvocationSummary>,
}
