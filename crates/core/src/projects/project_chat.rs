//! Session façade for the project-conversation concern.
//!
//! The logic moved to `wns-conversation` (L5); what is left is the half that
//! cannot travel — an `impl` block for `ProjectSession`, which core owns. Every
//! item the moved module exposed is re-exported below, so the desktop app, the
//! examples and the integration tests keep their existing paths.

use super::*;
use super::discussions::{DiscussionRun, DiscussionStart, RunOwner};

impl ProjectSession {
    /// Count pending project-chat drafts through the owning actor.  The
    /// actor's current project and operation namespace are the authority;
    /// callers cannot supply another namespace or cause an attach.
    pub fn project_chat_activity(&self) -> CoreResult<ProjectChatActivity> {
        self.request(|r| Command::ProjectChat(Box::new(ProjectChatCommand::Activity(r))))
    }

    pub fn find_project_chat_request(
        &self,
        access: ProjectAccess,
        conversation_id: String,
        operation_id: String,
    ) -> CoreResult<Option<DiscussionRun>> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::FindRun(
                access,
                conversation_id,
                operation_id,
                r,
            )))
        })
    }
    pub fn find_project_chapter_request(
        &self,
        access: ProjectAccess,
        conversation_id: String,
        operation_id: String,
    ) -> CoreResult<Option<DiscussionRun>> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::FindChapterRun(
                access,
                conversation_id,
                operation_id,
                r,
            )))
        })
    }
    pub fn read_project_chapter_feedback(
        &self,
        access: ProjectAccess,
        run_id: String,
    ) -> CoreResult<Option<ChapterDiscussionFeedback>> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::ReadChapterFeedback(
                access, run_id, r,
            )))
        })
    }
    pub fn is_root_project_chat_run(&self, owner: RunOwner) -> CoreResult<bool> {
        self.request(|r| Command::ProjectChat(Box::new(ProjectChatCommand::IsRootRun(owner, r))))
    }
    pub fn read_project_conversation(
        &self,
        request: ReadProjectConversation,
    ) -> CoreResult<ProjectConversation> {
        self.request(|r| Command::ProjectChat(Box::new(ProjectChatCommand::Read(request, r))))
    }
    pub fn read_project_chat_history(
        &self,
        request: ReadProjectChatHistory,
    ) -> CoreResult<HistoricalConversation> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::ReadHistory(request, r)))
        })
    }
    pub fn list_project_chat_history(
        &self,
        access: ProjectAccess,
    ) -> CoreResult<Vec<HistoricalConversationSummary>> {
        self.request(|r| Command::ProjectChat(Box::new(ProjectChatCommand::ListHistory(access, r))))
    }
    pub fn save_project_composer(
        &self,
        request: SaveProjectComposer,
    ) -> CoreResult<ProjectComposerSnapshot> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::SaveComposer(request, r)))
        })
    }
    pub fn start_project_chat(&self, request: StartProjectChat) -> CoreResult<DiscussionStart> {
        self.request(|r| Command::ProjectChat(Box::new(ProjectChatCommand::Start(request, r))))
    }
    pub fn start_project_chapter(
        &self,
        request: StartProjectChapter,
    ) -> CoreResult<DiscussionStart> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::StartChapter(request, r)))
        })
    }
    pub fn materialize_chat_result(
        &self,
        owner: RunOwner,
    ) -> CoreResult<Option<ChatMaterialization>> {
        self.request(|r| Command::ProjectChat(Box::new(ProjectChatCommand::Materialize(owner, r))))
    }
    pub fn read_assistant_draft(
        &self,
        access: ProjectAccess,
        conversation_id: String,
        document_id: String,
    ) -> CoreResult<AssistantDraft> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::ReadDraft(
                access,
                conversation_id,
                document_id,
                r,
            )))
        })
    }
    pub fn save_assistant_draft(&self, request: SaveAssistantDraft) -> CoreResult<SaveAck> {
        self.request(|r| Command::ProjectChat(Box::new(ProjectChatCommand::SaveDraft(request, r))))
    }
    pub fn checkpoint_assistant_draft(
        &self,
        conversation_id: String,
        request: CheckpointRequest,
    ) -> CoreResult<Revision> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::CheckpointDraft(
                conversation_id,
                request,
                r,
            )))
        })
    }
    pub fn reconcile_assistant_draft(
        &self,
        conversation_id: String,
        request: ReconcileRequest,
    ) -> CoreResult<ReconciledDocument> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::ReconcileDraft(
                conversation_id,
                request,
                r,
            )))
        })
    }
    pub fn set_chat_disposition(
        &self,
        request: SetChatDisposition,
    ) -> CoreResult<ConversationItem> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::Disposition(request, r)))
        })
    }
    pub fn prepare_chat_adoption(
        &self,
        request: PrepareChatAdoption,
    ) -> CoreResult<ChatAdoptionPreview> {
        self.request(|r| Command::ProjectChat(Box::new(ProjectChatCommand::Preview(request, r))))
    }
    pub fn adopt_chat_preview(&self, request: AdoptChatPreview) -> CoreResult<ChatAdoptionAck> {
        self.request(|r| Command::ProjectChat(Box::new(ProjectChatCommand::Adopt(request, r))))
    }
    pub fn read_chat_adoption_preview(
        &self,
        access: ProjectAccess,
        conversation_id: String,
        preview_id: String,
    ) -> CoreResult<ChatAdoptionPreview> {
        self.request(|r| {
            Command::ProjectChat(Box::new(ProjectChatCommand::ReadPreview(
                access,
                conversation_id,
                preview_id,
                r,
            )))
        })
    }
}

pub use wns_conversation::project_chat::*;

/// The host half of the chat concern.
///
/// `ProjectChatHost` is declared in `wns-conversation` because that is where
/// its lowest consumer lives, and an `impl` must be in the crate that owns the
/// type. `attach` and `recover_connection` are already inherent methods on
/// `OwnedProject`; the `Type::method` form is used deliberately so the
/// delegation cannot be mistaken for the trait method it implements.
impl wns_conversation::host::ProjectChatHost for OwnedProject {
    fn attach(&mut self, session: String) -> CoreResult<ProjectAccess> {
        OwnedProject::attach(self, session)
    }

    fn recover_connection(&mut self) -> CoreResult<()> {
        OwnedProject::recover_connection(self)
    }
}
