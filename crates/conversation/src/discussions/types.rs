use super::*;

pub(crate) const MAX_INSTRUCTION_BYTES: usize = 64 * 1024;
pub(crate) const MAX_SCOPE_QUOTE_BYTES: usize = 256 * 1024;
pub(crate) const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_EVENT_BYTES: usize = 128 * 1024;
pub(crate) const MAX_PINNED_DOCUMENTS: usize = 64;
pub(crate) const STOP_SETTLED_MESSAGE: &str =
    "You stopped this response. Any partial text shown here is saved.";
pub(crate) const STOP_UNRESOLVED_MESSAGE: &str =
    "This response was interrupted. Any partial text shown here is saved.";

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionDispatch {
    pub run: DiscussionRun,
    pub packet: CompiledPacket,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionFail {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionView {
    pub document_id: String,
    pub thread_id: Option<String>,
    pub messages: Vec<DiscussionMessage>,
    pub runs: Vec<DiscussionRun>,
    pub draft: Option<DiscussionDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionOutputAppend {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub chunk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionFinish {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub assistant_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DiscussionStopCleanup {
    Settled,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionStopSettled {
    pub owner: RunOwner,
    pub expected_sequence: String,
    pub event_id: String,
    pub assistant_text: String,
    pub cleanup: DiscussionStopCleanup,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionStop {
    pub run: DiscussionRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscussionBegin {
    pub owner: RunOwner,
}

#[allow(clippy::large_enum_variant)]
pub enum DiscussionCommand {
    ClaimAppServer(
        RunOwner,
        wns_providers::codex_app_server::AppServerDispatch,
        Reply<()>,
    ),
    AckAppServer(
        RunOwner,
        wns_providers::codex_app_server::AppServerDispatch,
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
        Reply<crate::discussion_lookup::LookupDispatch>,
    ),
    SettleLookup(
        crate::discussion_lookup::LookupInvocationReport,
        Reply<DiscussionRun>,
    ),
    AdvanceLookup(
        crate::discussion_lookup::LookupAdvanceRequest,
        Reply<crate::discussion_lookup::LookupAdvance>,
    ),
    HaltLookup(
        crate::discussion_lookup::LookupHaltRequest,
        Reply<DiscussionRun>,
    ),
    ReadRun(RunOwner, Reply<DiscussionRun>),
    Read(ProjectAccess, String, Reply<DiscussionView>),
    Retry(ProjectAccess, String, Reply<DiscussionRetry>),
    SaveDraft(SaveDiscussionDraft, Reply<DiscussionDraft>),
}

// Actor-side logic, as free functions over `StoryHost`.
