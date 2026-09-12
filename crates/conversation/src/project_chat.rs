//! Project conversation projection. Text remains in discussion messages and
//! document revisions; this module owns references, composer CAS and decisions.
use crate::discussions::{
    DiscussionRun, DiscussionScopeInput, DiscussionStart, FeedbackIntent, RunOwner,
};
pub use super::project_chat_context::ProjectChatDraftRef;
// Moved to wns-context (L2) as part of the packet-compiler inversion:
// FrozenContext embeds FrozenProjectChat, so this vocabulary cannot sit above
// the compiler it feeds. Re-exported at the historical path.
pub use wns_context::chat_vocabulary::{
    ChatDispositionScope, ChatDispositionScopeKind, ChatUnknownTo,
};
use wns_context::project_chat_output::ChapterRangeProposal;
pub use wns_context::project_chat_output::ChatGroupEffectsOutput;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wns_documents::records::{
    CheckpointRequest, OperationReceipt, ReconcileRequest, ReconciledDocument, SaveAck,
    SaveCause, SaveSnapshot,
};
use wns_kernel::{
    CoreError, CoreResult, DocumentRecord, DocumentRole, Head, ProjectAccess, Reply, Revision,
    StoredResult, check_id, logical_hash, new_id, parse_stored_version, parse_version, require_head,
    sha256_hex, valid_hash, validate_snapshot_json,
};
use wns_storage::{
    checkpoint_at, existing_receipt, insert_receipt, read_document, read_document_with_role,
    read_revision,
};
use crate::host::ProjectChatHost;
use crate::discussions;
use wns_context::packet::{MockContextBudget, ProviderBinding};
use wns_context::{BasisKind, SafeBriefInput};

mod adoption;
mod chapters;
mod draft_lifecycle;
mod history;
mod materialize;
mod save_recap;
mod store;
mod transfer;

pub use history::{
    HistoricalConversation, HistoricalConversationItem, HistoricalConversationRef,
    HistoricalConversationSummary, HistoricalDraftRevision, HistoricalSourceRevision,
    ReadProjectChatHistory,
};
pub use save_recap::ChatDocumentSave;

/// Validate project-chat's durable projection before a backup is accepted.
///
/// The general transfer validator owns the database snapshot and calls this
/// narrow projection validator after the shared story tables have passed their
/// checks.  Keeping the entry point here prevents backup code from reaching
/// into project-chat's private storage layout.
pub fn validate_storage(connection: &rusqlite::Connection) -> CoreResult<()> {
    transfer::validate_storage(connection)
}

/// Validate the immutable authority chain for a Workshop snapshot created by
/// grouped project-chat adoption.  Workshop owns the snapshot table; keeping
/// this narrow forwarding seam here avoids exposing the adoption storage
/// layout to the backup/history implementation.
pub fn validate_chat_workshop_snapshot(
    connection: &rusqlite::Connection,
    origin: wns_story::workshop_vocabulary::WorkshopSnapshotOrigin<'_>,
    state: &wns_story::workshop_vocabulary::WorkshopState,
    previous_state: &wns_story::workshop_vocabulary::WorkshopState,
) -> CoreResult<()> {
    adoption::validate_chat_workshop_snapshot(connection, origin, state, previous_state)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectComposer {
    pub text: String,
    #[serde(default)]
    pub source_refs: Vec<Head>,
    #[serde(default)]
    pub task_draft_refs: Vec<ProjectChatDraftRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_document_ref: Option<Head>,
    /// A chapter task is persisted with the same composer CAS as the
    /// project conversation. It is consumed atomically when submitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<ProjectChapterComposer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectChapterComposer {
    pub target: Head,
    pub intent: FeedbackIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<DiscussionScopeInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_brief: Option<SafeBriefInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectComposerSnapshot {
    pub conversation_id: String,
    pub version: String,
    pub body: ProjectComposer,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationItem {
    pub id: String,
    pub sequence: String,
    pub kind: String,
    pub reference_id: Option<String>,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssistantDraft {
    pub document: DocumentRecord,
    pub conversation_id: String,
    pub origin_run_id: String,
    pub packet_id: String,
    pub initial_revision_id: String,
    pub target: Option<Head>,
    /// Prior assistant draft explicitly named by the response as its source.
    /// This is lineage metadata only; the predecessor remains immutable and
    /// independently reviewable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_document_id: Option<String>,
    pub disposition: String,
    pub disposition_version: String,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectConversation {
    pub id: String,
    pub composer: ProjectComposerSnapshot,
    pub items: Vec<ConversationItem>,
    pub older_before: Option<String>,
    pub active_run: Option<DiscussionRun>,
    pub drafts: Vec<AssistantDraft>,
    pub source_epoch: String,
    pub policy_epoch: String,
    pub earlier_workshop: bool,
    pub document_saves: Vec<ChatDocumentSave>,
}

/// Read-only activity owned by this project's current actor namespace.
///
/// The desktop picker uses this count to show project activity without
/// opening another project, attaching a renderer, or returning any story
/// content.  The active-work count is combined with the existing
/// `background_work` census by the native command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectChatActivity {
    pub pending_drafts: usize,
}

/// Read-only, source-bound feedback from an unscoped chapter discussion. The
/// range is a suggestion for the writer; it is not a scope grant and cannot be
/// adopted without a fresh editor-captured request.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChapterDiscussionFeedback {
    pub run_id: String,
    pub target: Head,
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range_proposal: Option<ChapterRangeProposal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadProjectConversation {
    pub access: ProjectAccess,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default = "page_size")]
    pub limit: u32,
}
fn page_size() -> u32 {
    40
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveProjectComposer {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub conversation_id: String,
    pub expected_version: String,
    pub body: ProjectComposer,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartProjectChat {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub conversation_id: String,
    pub expected_composer_version: String,
    pub composer: ProjectComposer,
    pub budget: MockContextBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartProjectChapter {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub conversation_id: String,
    pub expected_composer_version: String,
    pub composer: ProjectComposer,
    pub budget: MockContextBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveAssistantDraft {
    pub conversation_id: String,
    pub disposition_version: String,
    pub snapshot: SaveSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetChatDisposition {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub conversation_id: String,
    pub reference_id: String,
    pub expected_version: String,
    /// draft: rejected/reconsider; question: notNow/notRelevant/keepMysterious/reconsider.
    pub disposition: String,
    #[serde(default)]
    pub rationale: String,
    /// Omitted for compatibility with older clients and interpreted as the
    /// project-wide scope. Non-project references are bounded by the
    /// producing conversation and document roles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ChatDispositionScope>,
    /// Only meaningful for a question kept mysterious. It records which
    /// audience is intentionally denied the answer; it does not alter source
    /// epochs or establish canon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_to: Option<ChatUnknownTo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatMaterialization {
    pub run_id: String,
    pub item_id: String,
    pub draft_ids: Vec<String>,
    pub output_valid: bool,
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_effects: Option<ChatGroupEffectsOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareChatAdoption {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub conversation_id: String,
    pub drafts: Vec<ProjectChatDraftRef>,
    /// Optional effects copied from the exact retained materialization.  A
    /// missing value means body-only adoption; it never authorizes inferred
    /// relationship or placement changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_effects: Option<ChatGroupEffectsOutput>,
}

pub const CHAT_ADOPTION_EFFECTS_VERSION: &str = "chat-adoption-effects.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatRelationshipDependency {
    pub relationship_id: String,
    pub from_document_id: String,
    pub to_document_id: String,
    pub relationship_type: String,
    pub from_head: Head,
    pub to_head: Head,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAdoptionRelationship {
    pub key: String,
    pub relationship_id: String,
    pub from_document_id: String,
    pub to_document_id: String,
    #[serde(rename = "type")]
    pub relationship_type: String,
    pub description: String,
    pub uncertainty: String,
    pub from_head: Head,
    pub to_head: Head,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAdoptionImpact {
    pub target_document_id: String,
    pub kind: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAdoptionSupersession {
    pub target_document_id: String,
    pub superseded_document_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAdoptionPlacement {
    pub target_document_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_document_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_document_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatProtectedContent {
    pub target_document_id: String,
    pub source_head: Head,
    pub text: String,
    pub text_hash: String,
}

/// Complete immutable grouped-adoption manifest. Relationship dependencies
/// are read-only drift fences; proposed effects are explicit author-review
/// material and are never inferred from document prose.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAdoptionEffects {
    pub version: String,
    pub source_output_hash: String,
    pub relationship_dependencies: Vec<ChatRelationshipDependency>,
    pub protected_content: Vec<ChatProtectedContent>,
    pub proposed_relationships: Vec<ChatAdoptionRelationship>,
    pub impacts: Vec<ChatAdoptionImpact>,
    pub supersessions: Vec<ChatAdoptionSupersession>,
    pub placements: Vec<ChatAdoptionPlacement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAdoptionTarget {
    pub draft: ProjectChatDraftRef,
    pub draft_revision_id: String,
    pub document_id: String,
    pub title: String,
    pub kind: String,
    pub before: Option<DocumentRecord>,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAdoptionPreview {
    pub id: String,
    pub version: String,
    pub digest: String,
    pub project_id: String,
    pub operation_namespace: String,
    pub conversation_id: String,
    pub source_epoch: String,
    pub policy_epoch: String,
    pub workshop_version: String,
    pub targets: Vec<ChatAdoptionTarget>,
    pub effects: Option<ChatAdoptionEffects>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdoptChatPreview {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub conversation_id: String,
    pub preview_id: String,
    pub preview_version: String,
    pub preview_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAdoptionAck {
    pub preview_id: String,
    pub documents: Vec<DocumentRecord>,
    pub decision_id: String,
}

pub enum ProjectChatCommand {
    FindRun(ProjectAccess, String, String, Reply<Option<DiscussionRun>>),
    FindChapterRun(ProjectAccess, String, String, Reply<Option<DiscussionRun>>),
    ReadChapterFeedback(
        ProjectAccess,
        String,
        Reply<Option<ChapterDiscussionFeedback>>,
    ),
    IsRootRun(RunOwner, Reply<bool>),
    Activity(Reply<ProjectChatActivity>),
    Read(ReadProjectConversation, Reply<ProjectConversation>),
    ListHistory(ProjectAccess, Reply<Vec<HistoricalConversationSummary>>),
    ReadHistory(ReadProjectChatHistory, Reply<HistoricalConversation>),
    SaveComposer(SaveProjectComposer, Reply<ProjectComposerSnapshot>),
    Start(StartProjectChat, Reply<DiscussionStart>),
    StartChapter(StartProjectChapter, Reply<DiscussionStart>),
    Materialize(RunOwner, Reply<Option<ChatMaterialization>>),
    ReadDraft(ProjectAccess, String, String, Reply<AssistantDraft>),
    SaveDraft(SaveAssistantDraft, Reply<SaveAck>),
    CheckpointDraft(String, CheckpointRequest, Reply<Revision>),
    ReconcileDraft(String, ReconcileRequest, Reply<ReconciledDocument>),
    Disposition(SetChatDisposition, Reply<ConversationItem>),
    Preview(PrepareChatAdoption, Reply<ChatAdoptionPreview>),
    ReadPreview(ProjectAccess, String, String, Reply<ChatAdoptionPreview>),
    Adopt(AdoptChatPreview, Reply<ChatAdoptionAck>),
}


// Actor-side logic, as free functions over `ProjectChatHost`.

pub fn handle_project_chat(host: &mut impl ProjectChatHost, command: ProjectChatCommand) {
    macro_rules! reply {
        ($r:expr, $op:expr) => {{
            let result = $op;
            host.fence_uncertain(&result);
            let _ = $r.send(result);
        }};
    }
    match command {
        ProjectChatCommand::FindRun(a, c, o, r) => reply!(
            r,
            host.check_access(&a)
                .and_then(|()| store::find_run(host.db()?, &a, &c, &o))
        ),
        ProjectChatCommand::FindChapterRun(a, c, o, r) => reply!(
            r,
            host.check_access(&a).and_then(|()| store::find_chapter_run(
                host.db()?,
                &a,
                &c,
                &o
            ))
        ),
        ProjectChatCommand::ReadChapterFeedback(a, run_id, r) => reply!(
            r,
            host.check_access(&a).and_then(|()| {
                host.db()
                    .and_then(|db| store::read_chapter_feedback(db, &a, &run_id))
            })
        ),
        ProjectChatCommand::IsRootRun(owner, r) => reply!(
            r,
            if owner.project_id != host.info().project_id
                || owner.operation_namespace != host.info().operation_namespace
            {
                Err(CoreError::new(
                    "DiscussionProjectMismatch",
                    "The discussion run belongs to another project identity.",
                ))
            } else {
                let access = ProjectAccess {
                    project_id: owner.project_id.clone(),
                    operation_namespace: owner.operation_namespace.clone(),
                    session: String::new(),
                    writer_lease: String::new(),
                };
                host.db()
                    .and_then(|db| store::is_root_project_chat_run(db, &access, &owner.run_id))
            }
        ),
        ProjectChatCommand::Activity(r) => reply!(r, project_chat_activity_snapshot(host, )),
        ProjectChatCommand::Read(q, r) => reply!(r, store::read_project_conversation(host, q)),
        ProjectChatCommand::ListHistory(a, r) => reply!(
            r,
            host.check_access(&a)
                .and_then(|()| history::list(host.db()?, &a))
        ),
        ProjectChatCommand::ReadHistory(q, r) => reply!(
            r,
            host.check_access(&q.access)
                .and_then(|()| history::read(host.db()?, q))
        ),
        ProjectChatCommand::SaveComposer(q, r) => reply!(r, store::save_project_composer(host, q)),
        ProjectChatCommand::Start(q, r) => reply!(r, store::start_project_chat(host, q)),
        ProjectChatCommand::StartChapter(q, r) => reply!(r, chapters::start_project_chapter(host, q)),
        ProjectChatCommand::Materialize(q, r) => reply!(r, materialize::materialize_chat_result(host, q)),
        ProjectChatCommand::ReadDraft(a, c, d, r) => reply!(
            r,
            host.check_access(&a)
                .and_then(|()| store::read_draft(host.db()?, &a, &c, &d))
        ),
        ProjectChatCommand::SaveDraft(q, r) => reply!(r, draft_lifecycle::save_assistant_draft(host, q)),
        ProjectChatCommand::CheckpointDraft(c, q, r) => {
            reply!(r, draft_lifecycle::checkpoint_assistant_draft(host, c, q))
        }
        ProjectChatCommand::ReconcileDraft(c, q, r) => {
            reply!(r, draft_lifecycle::reconcile_assistant_draft(host, c, q))
        }
        ProjectChatCommand::Disposition(q, r) => reply!(r, draft_lifecycle::set_chat_disposition(host, q)),
        ProjectChatCommand::Preview(q, r) => reply!(r, adoption::prepare(host, q)),
        ProjectChatCommand::ReadPreview(a, c, p, r) => {
            reply!(r, adoption::read_preview(host, &a, &c, &p))
        }
        ProjectChatCommand::Adopt(q, r) => reply!(r, adoption::adopt(host, q)),
    }
}

pub fn project_chat_activity_snapshot(host: &impl ProjectChatHost) -> CoreResult<ProjectChatActivity> {
    host.current_access()?;
    let db = host.db()?;
    let pending_drafts: i64 = db.query_row(
        "SELECT COUNT(*)
           FROM assistant_drafts d
           JOIN project_conversations c
             ON c.id=d.conversation_id
            AND c.project_id=d.project_id
            AND c.operation_namespace=d.operation_namespace
          WHERE d.project_id=?
            AND d.operation_namespace=?
            AND d.disposition='pending'",
        rusqlite::params![host.info().project_id, host.info().operation_namespace],
        |row| row.get(0),
    )?;
    Ok(ProjectChatActivity {
        pending_drafts: usize::try_from(pending_drafts).map_err(|_| {
            CoreError::new(
                "InvalidProject",
                "The project has an invalid pending-draft count.",
            )
        })?,
    })
}
