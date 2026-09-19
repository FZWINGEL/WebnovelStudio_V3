use super::*;

pub(crate) const MAX_REVIEW_CHAPTERS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StageAuthorReview {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records: Option<Vec<PossessionRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises: Option<Vec<PromiseRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<Vec<KnowledgeRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkReady {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub stage_id: String,
}

// Moved to wns-context (L2) with reviewed_summary, which embeds it. Re-exported
// at the historical path.
pub use wns_context::reviewed_prefix::ReviewPrefixItem;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewStage {
    pub id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub target: Head,
    pub revision: Revision,
    pub previous_bundle_id: Option<String>,
    pub prefix: Vec<ReviewPrefixItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records: Option<Vec<PossessionRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises: Option<Vec<PromiseRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<Vec<KnowledgeRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_hash: Option<String>,
    pub source_epoch: SourceEpoch,
    pub policy_epoch: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadyBundle {
    pub id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub stage_id: String,
    pub target: Head,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records: Option<Vec<PossessionRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises: Option<Vec<PromiseRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<Vec<KnowledgeRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_hash: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedRecordSet {
    pub bundle_id: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub target: Head,
    pub revision: Revision,
    pub records: Vec<PossessionRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises: Option<Vec<PromiseRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promises_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<Vec<KnowledgeRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_hash: Option<String>,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewStatus {
    pub document_id: String,
    pub title: String,
    pub head: Head,
    pub state: ReviewState,
    pub active_bundle_id: Option<String>,
    pub pending_stage_id: Option<String>,
    pub reason: Option<String>,
    pub can_stage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ReviewState {
    NoReview,
    Ready,
    ChangedProse,
    EarlierBasisChanged,
    ReviewNeeded,
}

// Public because `Command::Review` in `webnovel-core` carries it, which is a
// legal downward edge: core sits above this crate.
pub enum ReviewCommand {
    Status(ProjectAccess, String, Reply<ReviewStatus>),
    ReadStage(ProjectAccess, String, Reply<ReviewStage>),
    Stage(StageAuthorReview, Reply<ReviewStage>),
    Mark(MarkReady, Reply<ReadyBundle>),
    ReadRecords(ProjectAccess, String, Reply<Option<ReviewedRecordSet>>),
}

#[derive(Debug, Clone)]
pub(crate) struct StageRow {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) operation_namespace: String,
    pub(crate) document_id: String,
    pub(crate) target: Head,
    pub(crate) revision_id: String,
    pub(crate) source_epoch: i64,
    pub(crate) policy_epoch: i64,
    pub(crate) previous_bundle_id: Option<String>,
    pub(crate) prefix: Vec<ReviewPrefixItem>,
    pub(crate) prefix_hash: String,
    pub(crate) records: Option<Vec<PossessionRecord>>,
    pub(crate) records_hash: Option<String>,
    pub(crate) promises: Option<Vec<PromiseRecord>>,
    pub(crate) promises_hash: Option<String>,
    pub(crate) knowledge: Option<Vec<KnowledgeRecord>>,
    pub(crate) knowledge_hash: Option<String>,
    pub(crate) summary: Option<SummaryRevision>,
    pub(crate) summary_hash: Option<String>,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BundleRow {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) operation_namespace: String,
    pub(crate) stage_id: String,
    pub(crate) document_id: String,
    pub(crate) target: Head,
    pub(crate) revision_id: String,
    pub(crate) policy_epoch: i64,
    pub(crate) prefix: Vec<ReviewPrefixItem>,
    pub(crate) coverage: String,
    pub(crate) records: Option<Vec<PossessionRecord>>,
    pub(crate) records_hash: Option<String>,
    pub(crate) promises: Option<Vec<PromiseRecord>>,
    pub(crate) promises_hash: Option<String>,
    pub(crate) knowledge: Option<Vec<KnowledgeRecord>>,
    pub(crate) knowledge_hash: Option<String>,
    pub(crate) summary: Option<SummaryRevision>,
    pub(crate) summary_hash: Option<String>,
    pub(crate) created_at: String,
}
