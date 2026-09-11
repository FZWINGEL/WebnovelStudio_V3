//! Project chat uses the existing provider workers and local recovery ledger.
use crate::discussion_commands::{AuthorStart, start_author_native};
use crate::discussion_recovery::{DiscussionRecovery, PendingSave, SaveOutcome, WorkerIssue};
use crate::library_commands::DesktopLibrary;
use crate::project_commands::{DesktopProjects, execute};
use crate::provider_runtime::DesktopProviders;
use serde::Serialize;
use tauri::State;
use webnovel_core::projects::discussions::{DiscussionRun, DiscussionStart};
use webnovel_core::projects::project_chat::*;
use webnovel_core::projects::{
    CheckpointRequest, CoreResult, ProjectAccess, ReconcileRequest, ReconciledDocument, Revision,
    SaveAck,
};
use webnovel_core::providers::preferences::ModelSelection;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectActivitySnapshot {
    pub project_id: String,
    pub operation_namespace: String,
    pub active_work_count: usize,
    pub pending_drafts: usize,
}

fn collect_project_activity(
    projects: Vec<webnovel_core::projects::ProjectSession>,
) -> CoreResult<Vec<ProjectActivitySnapshot>> {
    let mut snapshots = projects
        .into_iter()
        .map(|project| {
            let active_work_count = project.work().census()?.items.len();
            let pending_drafts = project.project_chat_activity()?.pending_drafts;
            Ok(ProjectActivitySnapshot {
                project_id: project.info.project_id.clone(),
                operation_namespace: project.info.operation_namespace.clone(),
                active_work_count,
                pending_drafts,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    snapshots.sort_by(|left, right| {
        left.project_id
            .cmp(&right.project_id)
            .then_with(|| left.operation_namespace.cmp(&right.operation_namespace))
    });
    Ok(snapshots)
}

/// Return activity for actors that are already open in the desktop registry.
/// This command never opens a path, attaches a renderer, acquires a lease, or
/// contacts a provider; it returns counts and identities only.
#[tauri::command]
pub async fn project_activity(
    state: State<'_, DesktopProjects>,
) -> CoreResult<Vec<ProjectActivitySnapshot>> {
    let projects = state.all_open()?;
    execute(move || collect_project_activity(projects)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProjectConversation {
    #[serde(flatten)]
    conversation: ProjectConversation,
    worker_issues: Vec<WorkerIssue>,
}

#[tauri::command]
pub async fn read_project_conversation(
    request: ReadProjectConversation,
    state: State<'_, DesktopProjects>,
    recovery: State<'_, DiscussionRecovery>,
) -> CoreResult<DesktopProjectConversation> {
    let project = state.project(&request.access.project_id)?;
    let recovery = recovery.inner().clone();
    execute(move || {
        let view = project.read_project_conversation(request.clone())?;
        let runs = view
            .items
            .iter()
            .filter_map(|item| item.payload.get("run"))
            .map(|v| serde_json::from_value::<DiscussionRun>(v.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        for item in view.items.iter().filter(|item| item.kind == "request") {
            let Some(value) = item.payload.get("run") else {
                continue;
            };
            let run: DiscussionRun = serde_json::from_value(value.clone())?;
            if run.status == webnovel_core::projects::discussions::DiscussionRunStatus::Completed {
                match project.materialize_chat_result(run.owner.clone()) {
                    Ok(_) => recovery.clear_completed_chat(&run),
                    Err(_) => recovery.retain(PendingSave {
                        run: run.clone(),
                        outcome: SaveOutcome::Materialize,
                    }),
                }
            }
        }
        let conversation = project.read_project_conversation(request)?;
        Ok(DesktopProjectConversation {
            conversation,
            worker_issues: recovery.project_chat_issues(&runs),
        })
    })
    .await
}

/// Read retained conversation evidence by an explicit historical identity.
/// This route is intentionally read-only: it never materializes, adopts, or
/// resumes work from the retained conversation.
#[tauri::command]
pub async fn read_project_chat_history(
    request: ReadProjectChatHistory,
    state: State<'_, DesktopProjects>,
) -> CoreResult<HistoricalConversation> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.read_project_chat_history(request)).await
}

#[tauri::command]
pub async fn list_project_chat_history(
    access: ProjectAccess,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Vec<HistoricalConversationSummary>> {
    let project = state.project(&access.project_id)?;
    execute(move || project.list_project_chat_history(access)).await
}

#[tauri::command]
pub async fn save_project_composer(
    request: SaveProjectComposer,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ProjectComposerSnapshot> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.save_project_composer(request)).await
}

#[tauri::command]
pub async fn start_project_chat(
    request: StartProjectChat,
    model_selection: Option<ModelSelection>,
    state: State<'_, DesktopProjects>,
    recovery: State<'_, DiscussionRecovery>,
    library: State<'_, DesktopLibrary>,
    runtime: State<'_, DesktopProviders>,
) -> CoreResult<DiscussionStart> {
    let project = state.project(&request.access.project_id)?;
    let selected = model_selection.unwrap_or_else(ModelSelection::local_mock);
    let recovery = recovery.inner().clone();
    let library = library.inner().clone();
    let runtime = runtime.inner().clone();
    let request = AuthorStart::ProjectChat(request);
    if selected.provider_id.starts_with("openai-compatible:") {
        return crate::http_discussion::start_author(
            request, selected, project, recovery, library, runtime,
        )
        .await;
    }
    execute(move || start_author_native(request, selected, project, recovery, library, runtime))
        .await
}

#[tauri::command]
pub async fn start_project_chapter(
    request: StartProjectChapter,
    model_selection: Option<ModelSelection>,
    state: State<'_, DesktopProjects>,
    recovery: State<'_, DiscussionRecovery>,
    library: State<'_, DesktopLibrary>,
    runtime: State<'_, DesktopProviders>,
) -> CoreResult<DiscussionStart> {
    let project = state.project(&request.access.project_id)?;
    let selected = model_selection.unwrap_or_else(ModelSelection::local_mock);
    let recovery = recovery.inner().clone();
    let library = library.inner().clone();
    let runtime = runtime.inner().clone();
    let request = AuthorStart::ProjectChapter(request);
    if selected.provider_id.starts_with("openai-compatible:") {
        return crate::http_discussion::start_author(
            request, selected, project, recovery, library, runtime,
        )
        .await;
    }
    execute(move || start_author_native(request, selected, project, recovery, library, runtime))
        .await
}

/// Read a completed chapter Discuss answer and its source-bound range hint.
/// This is read-only; selecting or staging an edit remains an explicit Writer
/// operation against a freshly captured chapter head.
#[tauri::command]
pub async fn read_project_chapter_feedback(
    access: ProjectAccess,
    run_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Option<ChapterDiscussionFeedback>> {
    let project = state.project(&access.project_id)?;
    execute(move || project.read_project_chapter_feedback(access, run_id)).await
}

#[tauri::command]
pub async fn retry_project_chat_save(
    access: ProjectAccess,
    conversation_id: String,
    run_id: String,
    state: State<'_, DesktopProjects>,
    recovery: State<'_, DiscussionRecovery>,
) -> CoreResult<()> {
    let project = state.project(&access.project_id)?;
    let recovery = recovery.inner().clone();
    execute(move || recovery.retry_project_chat(&project, access, conversation_id, run_id)).await
}

#[tauri::command]
pub async fn read_assistant_draft(
    access: ProjectAccess,
    conversation_id: String,
    document_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<AssistantDraft> {
    let project = state.project(&access.project_id)?;
    execute(move || project.read_assistant_draft(access, conversation_id, document_id)).await
}

#[tauri::command]
pub async fn save_assistant_draft(
    request: SaveAssistantDraft,
    state: State<'_, DesktopProjects>,
) -> CoreResult<SaveAck> {
    let project = state.project(&request.snapshot.access.project_id)?;
    execute(move || project.save_assistant_draft(request)).await
}

#[tauri::command]
pub async fn checkpoint_assistant_draft(
    conversation_id: String,
    request: CheckpointRequest,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Revision> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.checkpoint_assistant_draft(conversation_id, request)).await
}

#[tauri::command]
pub async fn reconcile_assistant_draft(
    conversation_id: String,
    request: ReconcileRequest,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReconciledDocument> {
    let project = state.project(&request.project_id)?;
    execute(move || project.reconcile_assistant_draft(conversation_id, request)).await
}

#[tauri::command]
pub async fn set_chat_disposition(
    request: SetChatDisposition,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ConversationItem> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.set_chat_disposition(request)).await
}

#[tauri::command]
pub async fn prepare_chat_adoption(
    request: PrepareChatAdoption,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ChatAdoptionPreview> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.prepare_chat_adoption(request)).await
}

#[tauri::command]
pub async fn adopt_chat_preview(
    request: AdoptChatPreview,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ChatAdoptionAck> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.adopt_chat_preview(request)).await
}

#[tauri::command]
pub async fn read_chat_adoption_preview(
    access: ProjectAccess,
    conversation_id: String,
    preview_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ChatAdoptionPreview> {
    let project = state.project(&access.project_id)?;
    execute(move || project.read_chat_adoption_preview(access, conversation_id, preview_id)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discussion_commands::{AuthorStart, start_author_native};
    use crate::discussion_recovery::DiscussionRecovery;
    use crate::library_commands::DesktopLibrary;
    use crate::provider_runtime::DesktopProviders;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::thread::sleep;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use webnovel_core::context::packet::MockContextBudget;
    use webnovel_core::documents::{Endpoint, ScopeKind};
    use webnovel_core::library::Library;
    use webnovel_core::projects::discussions::{
        DiscussionRun, DiscussionRunStatus, DiscussionScopeInput, FeedbackIntent, StartDiscussion,
    };
    use webnovel_core::projects::project_chat::{
        ProjectChapterComposer, ProjectComposer, ReadProjectConversation, SaveProjectComposer,
        StartProjectChapter, StartProjectChat,
    };
    use webnovel_core::projects::{CreateDocument, ProjectAccess, ProjectSession};
    use webnovel_core::providers::preferences::ModelSelection;

    struct Fixture {
        root: PathBuf,
        project: Option<ProjectSession>,
        access: ProjectAccess,
        chapter: webnovel_core::projects::DocumentRecord,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "wns-native-project-chat-{label}-{}-{suffix}",
                std::process::id()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let project =
                ProjectSession::create(root.join("project"), "Native project chat").unwrap();
            let access = project.attach("native-test".into()).unwrap();
            let chapter = project
                .create_document(CreateDocument {
                    access: access.clone(),
                    operation_id: "create-chapter".into(),
                    document_id: "chapter".into(),
                    title: "Chapter".into(),
                    kind: "chapter".into(),
                    body: serde_json::json!({
                        "schemaVersion": 1,
                        "body": {"type": "doc", "content": [{
                            "type": "paragraph", "attrs": {"id": "p1"},
                            "content": [{"type": "text", "text": "The ending stays."}]
                        }]}
                    }),
                })
                .unwrap();
            Self {
                root,
                project: Some(project),
                access,
                chapter,
            }
        }

        fn project(&self) -> &ProjectSession {
            self.project.as_ref().expect("fixture project")
        }

        fn library(&self) -> DesktopLibrary {
            DesktopLibrary(Arc::new(Mutex::new(
                Library::open(self.root.join("library")).unwrap(),
            )))
        }

        fn conversation(&self) -> webnovel_core::projects::project_chat::ProjectConversation {
            self.project()
                .read_project_conversation(ReadProjectConversation {
                    access: self.access.clone(),
                    before: None,
                    limit: 40,
                })
                .unwrap()
        }

        fn root_request(&self, operation_id: &str) -> (StartProjectChat, String) {
            let conversation = self.conversation();
            let composer = ProjectComposer {
                text: "Develop two possible directions for this story.".into(),
                ..ProjectComposer::default()
            };
            let saved = self
                .project()
                .save_project_composer(SaveProjectComposer {
                    access: self.access.clone(),
                    operation_id: format!("{operation_id}-save"),
                    conversation_id: conversation.id.clone(),
                    expected_version: conversation.composer.version,
                    body: composer.clone(),
                })
                .unwrap();
            (
                StartProjectChat {
                    access: self.access.clone(),
                    operation_id: operation_id.into(),
                    conversation_id: conversation.id.clone(),
                    expected_composer_version: saved.version,
                    composer,
                    budget: MockContextBudget::new("100000", "8192", "100"),
                    provider_binding: None,
                },
                conversation.id,
            )
        }

        fn chapter_request(&self, operation_id: &str) -> (StartProjectChapter, String) {
            let conversation = self.conversation();
            let composer = ProjectComposer {
                text: "Revise only the selected passage.".into(),
                chapter: Some(ProjectChapterComposer {
                    target: self.chapter.head.clone(),
                    intent: FeedbackIntent::ProposeEdits,
                    basis: None,
                    scope: Some(DiscussionScopeInput {
                        kind: ScopeKind::Passage,
                        start: Some(Endpoint {
                            block_id: "p1".into(),
                            utf16_offset: 0,
                        }),
                        end: Some(Endpoint {
                            block_id: "p1".into(),
                            utf16_offset: 3,
                        }),
                        quote: "The".into(),
                        source_body_hash: self.chapter.head.body_hash.clone(),
                    }),
                    safe_brief: None,
                }),
                ..ProjectComposer::default()
            };
            let saved = self
                .project()
                .save_project_composer(SaveProjectComposer {
                    access: self.access.clone(),
                    operation_id: format!("{operation_id}-save"),
                    conversation_id: conversation.id.clone(),
                    expected_version: conversation.composer.version,
                    body: composer.clone(),
                })
                .unwrap();
            (
                StartProjectChapter {
                    access: self.access.clone(),
                    operation_id: operation_id.into(),
                    conversation_id: conversation.id.clone(),
                    expected_composer_version: saved.version,
                    composer,
                    budget: MockContextBudget::new("100000", "8192", "100"),
                    provider_binding: None,
                },
                conversation.id,
            )
        }

        fn chapter_discuss_request(&self, operation_id: &str) -> (StartProjectChapter, String) {
            let conversation = self.conversation();
            let composer = ProjectComposer {
                text: "Give feedback on the chapter and identify the most relevant paragraph."
                    .into(),
                chapter: Some(ProjectChapterComposer {
                    target: self.chapter.head.clone(),
                    intent: FeedbackIntent::Discuss,
                    basis: None,
                    scope: None,
                    safe_brief: None,
                }),
                ..ProjectComposer::default()
            };
            let saved = self
                .project()
                .save_project_composer(SaveProjectComposer {
                    access: self.access.clone(),
                    operation_id: format!("{operation_id}-save"),
                    conversation_id: conversation.id.clone(),
                    expected_version: conversation.composer.version,
                    body: composer.clone(),
                })
                .unwrap();
            (
                StartProjectChapter {
                    access: self.access.clone(),
                    operation_id: operation_id.into(),
                    conversation_id: conversation.id.clone(),
                    expected_composer_version: saved.version,
                    composer,
                    budget: MockContextBudget::new("100000", "8192", "100"),
                    provider_binding: None,
                },
                conversation.id,
            )
        }

        fn wait_root(&self, conversation_id: &str, operation_id: &str) -> DiscussionRun {
            self.wait_run(conversation_id, operation_id, true)
        }

        fn wait_chapter(&self, conversation_id: &str, operation_id: &str) -> DiscussionRun {
            self.wait_run(conversation_id, operation_id, false)
        }

        fn wait_run(&self, conversation_id: &str, operation_id: &str, root: bool) -> DiscussionRun {
            for _ in 0..300 {
                let run = if root {
                    self.project()
                        .find_project_chat_request(
                            self.access.clone(),
                            conversation_id.to_owned(),
                            operation_id.to_owned(),
                        )
                        .unwrap()
                } else {
                    self.project()
                        .find_project_chapter_request(
                            self.access.clone(),
                            conversation_id.to_owned(),
                            operation_id.to_owned(),
                        )
                        .unwrap()
                };
                if let Some(run) = run
                    && !matches!(
                        run.status,
                        DiscussionRunStatus::Queued
                            | DiscussionRunStatus::Running
                            | DiscussionRunStatus::Stopping
                    )
                {
                    return run;
                }
                sleep(Duration::from_millis(10));
            }
            panic!("native worker did not reach a terminal state");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(self.project.take());
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn start_native(
        request: AuthorStart,
        fixture: &Fixture,
        recovery: DiscussionRecovery,
        runtime: DesktopProviders,
    ) -> DiscussionStart {
        start_author_native(
            request,
            ModelSelection::local_mock(),
            fixture.project().clone(),
            recovery,
            fixture.library(),
            runtime,
        )
        .unwrap()
    }

    fn terminal_fault(fixture: &Fixture, enabled: bool) {
        let db =
            rusqlite::Connection::open(fixture.project().path.join("project.sqlite3")).unwrap();
        db.execute_batch(if enabled {
            "CREATE TRIGGER native_terminal_fault BEFORE INSERT ON discussion_messages WHEN NEW.role='assistant' BEGIN SELECT RAISE(ABORT,'native test terminal write failure'); END;"
        } else {
            "DROP TRIGGER native_terminal_fault;"
        })
        .unwrap();
    }

    #[test]
    fn native_project_chat_reaches_terminal_and_materializes_once() {
        let fixture = Fixture::new("root-terminal");
        let recovery = DiscussionRecovery::default();
        let runtime = DesktopProviders::default();
        let (request, conversation_id) = fixture.root_request("root-terminal");
        let started = start_native(
            AuthorStart::ProjectChat(request),
            &fixture,
            recovery.clone(),
            runtime,
        );
        let run = fixture.wait_root(&conversation_id, &started.run.operation_id);
        assert_eq!(run.status, DiscussionRunStatus::Completed);
        // Terminal run persistence precedes the worker's local materialization.
        // Wait for that separate boundary before inspecting its exact effects.
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut conversation = fixture.conversation();
        while conversation.drafts.is_empty() && Instant::now() < deadline {
            sleep(Duration::from_millis(10));
            conversation = fixture.conversation();
        }
        assert_eq!(recovery.pending_count(), 0);
        assert_eq!(
            conversation
                .items
                .iter()
                .filter(|item| item.kind == "request")
                .count(),
            1
        );
        assert_eq!(conversation.drafts.len(), 2);
    }

    #[test]
    fn native_project_chapter_keeps_proposals_without_project_chat_materialization() {
        let fixture = Fixture::new("chapter-terminal");
        let recovery = DiscussionRecovery::default();
        let (request, conversation_id) = fixture.chapter_request("chapter-terminal");
        let started = start_native(
            AuthorStart::ProjectChapter(request),
            &fixture,
            recovery,
            DesktopProviders::default(),
        );
        let run = fixture.wait_chapter(&conversation_id, &started.run.operation_id);
        assert_eq!(run.status, DiscussionRunStatus::Completed);
        let proposals = fixture
            .project()
            .proposals(fixture.access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(proposals.len(), 3);
        assert!(proposals.iter().all(|proposal| {
            proposal.run_id == run.id
                && proposal.source == run.target
                && proposal.packet_id == run.packet_id
                && proposal.current
                && !proposal.historical_copy
        }));
        let conversation = fixture.conversation();
        assert!(conversation.drafts.is_empty());
        assert!(
            conversation
                .items
                .iter()
                .any(|item| item.kind == "chapterRequest")
        );
    }

    #[test]
    fn native_unscoped_chapter_discuss_returns_source_bound_range_projection() {
        let fixture = Fixture::new("chapter-discuss-range");
        let (request, conversation_id) = fixture.chapter_discuss_request("chapter-discuss-range");
        let started = start_native(
            AuthorStart::ProjectChapter(request),
            &fixture,
            DiscussionRecovery::default(),
            DesktopProviders::default(),
        );
        let run = fixture.wait_chapter(&conversation_id, &started.run.operation_id);
        assert_eq!(run.status, DiscussionRunStatus::Completed);
        assert_eq!(run.dispatch_state, "delivered");
        let feedback = fixture
            .project()
            .read_project_chapter_feedback(fixture.access.clone(), run.id)
            .unwrap()
            .expect("structured chapter feedback projection");
        assert!(feedback.answer.contains("first nonempty chapter paragraph"));
        let range = feedback.range_proposal.expect("mock range proposal");
        assert_eq!(range.first_block_id, "p1");
        assert_eq!(range.last_block_id, "p1");
        assert_eq!(range.quote, "The ending stays.");
        assert!(feedback.range_error.is_none());
    }

    #[test]
    fn native_stopped_project_chat_never_materializes_drafts() {
        let fixture = Fixture::new("root-stopped");
        let recovery = DiscussionRecovery::default();
        let (request, conversation_id) = fixture.root_request("root-stopped");
        let started = start_native(
            AuthorStart::ProjectChat(request),
            &fixture,
            recovery,
            DesktopProviders::default(),
        );
        fixture
            .project()
            .stop_discussion(fixture.access.clone(), started.run.id.clone())
            .unwrap();
        let run = fixture.wait_root(&conversation_id, &started.run.operation_id);
        assert_eq!(run.status, DiscussionRunStatus::Stopped);
        assert!(fixture.conversation().drafts.is_empty());
    }

    #[test]
    fn native_project_chat_save_failure_is_retained_and_retried_without_redispatch() {
        let fixture = Fixture::new("root-save-retry");
        let recovery = DiscussionRecovery::default();
        let runtime = DesktopProviders::default();
        let (request, conversation_id) = fixture.root_request("root-save-retry");
        let started = start_native(
            AuthorStart::ProjectChat(request.clone()),
            &fixture,
            recovery.clone(),
            runtime.clone(),
        );
        terminal_fault(&fixture, true);
        for _ in 0..300 {
            if recovery.pending_count() > 0 {
                break;
            }
            sleep(Duration::from_millis(10));
        }
        assert_eq!(recovery.pending_count(), 1);

        // The durable run is already accepted and still owns its output. A
        // repeated native start resolves that saved run and must not create a
        // second local worker/provider dispatch.
        let replay = start_native(
            AuthorStart::ProjectChat(request),
            &fixture,
            recovery.clone(),
            runtime,
        );
        assert_eq!(replay.run.id, started.run.id);
        assert_eq!(recovery.pending_count(), 1);

        terminal_fault(&fixture, false);
        recovery
            .retry_project_chat(
                fixture.project(),
                fixture.access.clone(),
                conversation_id.clone(),
                started.run.id.clone(),
            )
            .unwrap();
        let run = fixture.wait_root(&conversation_id, &started.run.operation_id);
        assert_eq!(run.status, DiscussionRunStatus::Completed);
        assert_eq!(recovery.pending_count(), 0);
        assert_eq!(fixture.conversation().drafts.len(), 2);
    }

    #[test]
    fn project_activity_is_read_only_and_bound_to_each_open_project_namespace() {
        let fixture = Fixture::new("activity-open");
        let recovery = DiscussionRecovery::default();
        let (request, conversation_id) = fixture.root_request("activity-root");
        start_native(
            AuthorStart::ProjectChat(request),
            &fixture,
            recovery,
            DesktopProviders::default(),
        );
        let _ = fixture.wait_root(&conversation_id, "activity-root");
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut before = fixture.conversation();
        while before.drafts.len() < 2 && Instant::now() < deadline {
            sleep(Duration::from_millis(10));
            before = fixture.conversation();
        }
        assert_eq!(before.drafts.len(), 2);

        // Leave one ordinary discussion queued so the activity result proves
        // it reuses the actor-owned background census without dispatching it.
        fixture
            .project()
            .start_discussion(StartDiscussion {
                access: fixture.access.clone(),
                operation_id: "activity-queued".into(),
                expected: fixture.chapter.head.clone(),
                instruction: "A queued activity check.".into(),
                intent: Default::default(),
                basis: None,
                scope: None,
                pinned_document_ids: Vec::new(),
                safe_brief: None,
                budget: MockContextBudget::new("100000", "8192", "100"),
                provider_binding: None,
                previous_run_id: None,
                lookup: None,
            })
            .unwrap();
        let second = Fixture::new("activity-second");
        let snapshots =
            collect_project_activity(vec![fixture.project().clone(), second.project().clone()])
                .unwrap();
        let current = snapshots
            .iter()
            .find(|item| item.project_id == fixture.project().info.project_id)
            .expect("current project activity");
        assert_eq!(current.active_work_count, 1);
        assert_eq!(current.pending_drafts, 2);
        let other = snapshots
            .iter()
            .find(|item| item.project_id == second.project().info.project_id)
            .expect("second project activity");
        assert_eq!(other.active_work_count, 0);
        assert_eq!(other.pending_drafts, 0);
        assert_ne!(current.operation_namespace, other.operation_namespace);

        let after = fixture.conversation();
        assert_eq!(after.items.len(), before.items.len());
        assert_eq!(after.drafts.len(), before.drafts.len());
        assert_eq!(fixture.project().work().census().unwrap().items.len(), 1);
    }
}
