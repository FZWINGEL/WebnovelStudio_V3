//! Author commands use renderer leases; the local test worker owns only its run.
use crate::author_start::{
    AuthorStart, app_server_lookup_unavailable, check_model_choice, dispatch_mock,
    has_saved_request, saved_request,
    start_author_native,
};
use crate::app_state::AppState;
use crate::discussion_recovery::{
    DesktopDiscussionView, DiscussionRecovery,
};
use crate::library_commands::DesktopLibrary;
use crate::project_commands::execute;
use crate::provider_runtime::DesktopProviders;
use tauri::State;
#[cfg(windows)]
use webnovel_core::library::codex_transport::CodexTransport;
use webnovel_core::projects::discussions::*;
use webnovel_core::projects::proposals::*;
use webnovel_core::projects::workshop_generation::StartWorkshop;
use webnovel_core::projects::{CoreError, CoreResult, ProjectAccess, ProjectSession};
#[cfg(windows)]
use webnovel_core::providers::codex_app_server::{connection::AppServerRequest, is_app_server};
use webnovel_core::providers::preferences::ModelSelection;
#[cfg(windows)]
use webnovel_core::providers::{claude_runtime::ClaudeConnection, codex_runtime::CodexConnection};

#[tauri::command]
pub async fn read_discussion(
    access: ProjectAccess,
    document_id: String, state: State<'_, AppState>,
) -> CoreResult<DesktopDiscussionView> {
    let app = &*state;
    let state = &app.projects;
    let recovery = &app.discussion_recovery;
    let project = state.project(&access.project_id)?;
    let recovery = recovery.clone();
    execute(move || Ok(recovery.view(project.read_discussion(access, document_id)?))).await
}

#[tauri::command]
pub async fn retry_discussion_save(
    access: ProjectAccess,
    document_id: String,
    run_id: String, state: State<'_, AppState>,
) -> CoreResult<DesktopDiscussionView> {
    let app = &*state;
    let state = &app.projects;
    let recovery = &app.discussion_recovery;
    let project = state.project(&access.project_id)?;
    let recovery = recovery.clone();
    execute(move || recovery.retry(&project, access, document_id, run_id)).await
}

#[tauri::command]
pub async fn discussion_retry(
    access: ProjectAccess,
    run_id: String, state: State<'_, AppState>,
) -> CoreResult<DiscussionRetry> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.discussion_retry(access, run_id)).await
}

#[tauri::command]
pub async fn save_discussion_draft(
    request: SaveDiscussionDraft, state: State<'_, AppState>,
) -> CoreResult<DiscussionDraft> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.save_discussion_draft(request)).await
}

#[tauri::command]
pub async fn stop_discussion(
    access: ProjectAccess,
    run_id: String, state: State<'_, AppState>,
) -> CoreResult<DiscussionStop> {
    let app = &*state;
    let state = &app.projects;
    let runtime = &app.providers;
    let project = state.project(&access.project_id)?;
    let runtime = runtime.clone();
    execute(move || {
        let owner = RunOwner {
            project_id: access.project_id.clone(),
            operation_namespace: access.operation_namespace.clone(),
            run_id: run_id.clone(),
        };
        match project.stop_discussion(access, run_id) {
            Ok(stopped) => {
                runtime.stop(&stopped.run.owner);
                Ok(stopped)
            }
            Err(error) => {
                // An uncertain commit has already passed core ownership
                // validation. Stop local work even if its acknowledgment was lost.
                if error.code == "UncertainOutcome" {
                    runtime.stop(&owner);
                }
                Err(error)
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn proposals(
    access: ProjectAccess,
    document_id: String, state: State<'_, AppState>,
) -> CoreResult<Vec<Proposal>> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.proposals(access, document_id)).await
}

#[tauri::command]
pub async fn prepare_proposal(
    request: PrepareProposal, state: State<'_, AppState>,
) -> CoreResult<PreparedProposal> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.prepare_proposal(request)).await
}

#[tauri::command]
pub async fn prepare_continuation(
    request: PrepareContinuation, state: State<'_, AppState>,
) -> CoreResult<PreparedProposal> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.prepare_continuation(request)).await
}

#[tauri::command]
pub async fn prepare_structured(
    request: PrepareStructured, state: State<'_, AppState>,
) -> CoreResult<PreparedProposal> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.prepare_structured(request)).await
}

#[tauri::command]
pub async fn apply_proposal(
    request: ApplyProposal, state: State<'_, AppState>,
) -> CoreResult<ApplyAck> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.apply_proposal(request)).await
}

#[tauri::command]
pub async fn reject_proposal(
    request: RejectProposal, state: State<'_, AppState>,
) -> CoreResult<ProposalDecision> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.reject_proposal(request)).await
}

#[tauri::command]
pub async fn start_discussion(
    mut request: StartDiscussion,
    model_selection: Option<ModelSelection>, state: State<'_, AppState>,
) -> CoreResult<DiscussionStart> {
    let app = &*state;
    let state = &app.projects;
    let recovery = &app.discussion_recovery;
    let library = &app.library;
    let runtime = &app.providers;
    let project = state.project(&request.access.project_id)?;
    let recovery = recovery.clone();
    let library = library.clone();
    let runtime = runtime.clone();
    if model_selection
        .as_ref()
        .is_some_and(|choice| choice.provider_id.starts_with("openai-compatible:"))
    {
        return crate::http_discussion::start(
            request,
            model_selection.unwrap(),
            project,
            recovery,
            library,
            runtime,
        )
        .await;
    }
    execute(move || {
        let _admission = runtime.admit_request()?;
        let selected = model_selection
            .clone()
            .unwrap_or_else(ModelSelection::local_mock);
        let existing = saved_request(&project, &request)?;
        let library = library.0.lock().map_err(|_| crate::provider_commands::unavailable())?;
        #[cfg(windows)]
        let mut app_server_request = None;
        #[cfg(windows)]
        let connection = runtime.connection().ok();
        #[cfg(windows)]
        let claude_connection = if existing.is_none() && selected.provider_id == "claude" {
            runtime.claude_connection().ok()
        } else {
            None
        };
        // Native code supplies the trusted binding, never renderer budgets or
        // arbitrary command options. Existing mock payloads stay unchanged.
        request.provider_binding = if let Some(existing) = &existing {
            #[cfg(windows)]
            if existing.status == DiscussionRunStatus::Queued
                && let Some(binding) = existing.provider_binding.as_ref().filter(|binding| is_app_server(binding)) {
                app_server_request = Some(runtime.app_server()?.reserve(binding)?);
            }
            existing.provider_binding.clone()
        } else if selected.provider_id == "codex" {
            #[cfg(windows)]
            {
                let checked = connection.as_ref().ok_or_else(|| {
                    CoreError::new(
                        "ProviderUnavailable",
                        "Check the Codex connection in Settings before sending this request.",
                    )
                })?;
                if library.codex_transport_settings()?.transport == CodexTransport::AppServer {
                    if request.lookup.is_some() { return Err(app_server_lookup_unavailable()); }
                    let server = runtime.app_server()?;
                    let binding = server.author_binding(&selected)?;
                    app_server_request = Some(server.reserve(&binding)?);
                    Some(binding)
                } else {
                    Some(crate::provider_runtime::connection_author_binding(checked, &selected)?)
                }
            }
            #[cfg(not(windows))]
            {
                return Err(CoreError::new(
                    "ProviderUnavailable",
                    "This Codex connection is currently available on Windows only.",
                ));
            }
        } else if selected.provider_id == "claude" {
            #[cfg(windows)]
            {
                let checked = claude_connection.as_ref().ok_or_else(|| {
                    CoreError::new(
                        "ProviderUnavailable",
                        "Check the Claude Code connection in Settings before sending this request.",
                    )
                })?;
                Some(crate::provider_runtime::claude_binding_for_choice(
                    checked, &selected,
                )?)
            }
            #[cfg(not(windows))]
            {
                return Err(CoreError::new(
                    "ProviderUnavailable",
                    "This Claude Code connection is currently available on Windows only.",
                ));
            }
        } else {
            None
        };
        let started = {
            // Preference acceptance and new request acceptance are serialized.
            // Subsequent setting changes cannot redirect this frozen request.
            let active = library.provider_state()?.settings.active;
            check_model_choice(&project, &request, model_selection.as_ref(), &active)?;
            if request.provider_binding.is_some() {
                #[cfg(windows)]
                let available = connection.is_some() || claude_connection.is_some();
                #[cfg(not(windows))]
                let available = false;
                if !available && !has_saved_request(&project, &request)? {
                    return Err(CoreError::new(
                        "ProviderUnavailable",
                        "Check the selected provider connection in Settings before sending this request.",
                    ));
                }
            }
            project.start_discussion(request)?
        };
        drop(library);
        dispatch_started(
            project,
            recovery,
            runtime,
            started,
            #[cfg(windows)]
            connection,
            #[cfg(windows)]
            claude_connection,
            #[cfg(windows)]
            app_server_request,
        )
    })
    .await
}

/// Dispatch a queued discussion through the same provider ownership, stop,
/// recovery, and mock worker lifecycle used by ordinary discussions. Workshop
/// starts call this after the actor has frozen and durably queued their run.
#[cfg(windows)]
pub(crate) fn dispatch_started(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    runtime: DesktopProviders,
    started: DiscussionStart,
    connection: Option<CodexConnection>,
    claude_connection: Option<ClaudeConnection>,
    app_server_request: Option<AppServerRequest>,
) -> CoreResult<DiscussionStart> {
    if started.run.status == DiscussionRunStatus::Queued
        && started.packet.options.provider_binding.is_some()
    {
        let app_server = started
            .packet
            .options
            .provider_binding
            .as_ref()
            .is_some_and(is_app_server);
        if app_server && app_server_request.is_none() {
            return Err(CoreError::new(
                "ProviderUnavailable",
                "The original app-server connection is unavailable. Check Codex in Settings; this request has not been sent.",
            ));
        }
        let stop = match runtime.register(&started.run.owner) {
            Ok(stop) => stop,
            Err(error) if error.code == "RunAlreadyStarted" => return Ok(started),
            Err(error) => return Err(error),
        };
        if let Some(dispatch) = recovery.claim(&project, &started.run) {
            let failure_project = project.clone();
            let failure_run = dispatch.run.clone();
            let worker_runtime = runtime.clone();
            let worker_recovery = recovery.clone();
            let is_claude = dispatch
                .packet
                .options
                .provider_binding
                .as_ref()
                .is_some_and(|binding| binding.provider_id == "claude");
            if std::thread::Builder::new()
                .name(
                    if is_claude {
                        "webnovel-claude-response"
                    } else {
                        "webnovel-codex-response"
                    }
                    .into(),
                )
                .spawn(move || {
                    if app_server {
                        let request = app_server_request.expect("reserved before acceptance");
                        crate::app_server_discussion::run_live(
                            project,
                            worker_recovery,
                            worker_runtime,
                            request.reservation,
                            request.thread,
                            dispatch,
                            stop,
                        );
                    } else if is_claude {
                        crate::claude_live_discussion::run_live(
                            project,
                            worker_recovery,
                            worker_runtime,
                            claude_connection,
                            dispatch,
                            stop,
                        );
                    } else {
                        crate::live_discussion::run_live(
                            project,
                            worker_recovery,
                            worker_runtime,
                            connection,
                            dispatch,
                            stop,
                        );
                    }
                })
                .is_err()
            {
                runtime.release(&failure_run.owner);
                if app_server {
                    crate::app_server_discussion::worker_unavailable(
                        &failure_project,
                        &recovery,
                        failure_run,
                    );
                } else if is_claude {
                    crate::claude_live_discussion::worker_unavailable(
                        &failure_project,
                        &recovery,
                        failure_run,
                    );
                } else {
                    crate::live_discussion::worker_unavailable(
                        &failure_project,
                        &recovery,
                        failure_run,
                    );
                }
            }
        } else {
            runtime.release(&started.run.owner);
        }
        return Ok(started);
    }
    dispatch_mock(project, recovery, runtime, started)
}

/// Start a Workshop request after the renderer's model selection has been
/// checked and bound to the native provider runtime.  The actor still owns
/// session/CAS resolution; this helper only mirrors the ordinary discussion
/// admission and dispatch boundary.
pub(crate) fn start_workshop_native(
    request: StartWorkshop,
    selected: ModelSelection,
    project: ProjectSession,
    recovery: DiscussionRecovery,
    library: DesktopLibrary,
    runtime: DesktopProviders,
) -> CoreResult<DiscussionStart> {
    start_author_native(
        AuthorStart::Workshop(request),
        selected,
        project,
        recovery,
        library,
        runtime,
    )
}


#[cfg(test)]
mod tests {
    use super::*;
    // The mock dispatch moved to `author_start`; these tests still exercise it
    // through the commands, so they name it directly.
    use crate::author_start::{mock_output, record_worker_failure, run_mock_with_pause};
    use webnovel_core::context::packet::{CompiledPacket, MOCK_MODEL_ID, packet_input_hash};
    use webnovel_core::context::PacketReceipt;
    use webnovel_core::context::packet::MockContextBudget;
    use webnovel_core::context::packet::{PacketMessage, PacketOptions};
    use webnovel_core::documents::{Endpoint, ScopeGrant, ScopeKind, capture_scope};
    use webnovel_core::projects::CreateDocument;

    fn started_project(label: &str) -> (ProjectSession, ProjectAccess, DiscussionStart) {
        started_project_with_intent(label, FeedbackIntent::Discuss)
    }

    fn started_project_with_intent(
        label: &str,
        intent: FeedbackIntent,
    ) -> (ProjectSession, ProjectAccess, DiscussionStart) {
        let path = std::env::temp_dir().join(format!(
            "wns-desktop-worker-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = ProjectSession::create(path, "Worker test").unwrap();
        let access = project.documents().attach("test-session".into()).unwrap();
        let document = project.documents().create(CreateDocument {
            access: access.clone(), operation_id: "create".into(), document_id: "chapter".into(),
            title: "Chapter".into(), kind: "chapter".into(),
            body: serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":"The ending stays."}]}]}}),
        }).unwrap();
        let started = project
            .start_discussion(StartDiscussion {
                lookup: None,
                access: access.clone(),
                operation_id: "start".into(),
                expected: document.head,
                instruction: "Discuss the ending.".into(),
                intent,
                basis: (intent == FeedbackIntent::Continue)
                    .then_some(webnovel_core::context::BasisKind::Working),
                scope: None,
                pinned_document_ids: Vec::new(),
                safe_brief: None,
                previous_run_id: None,
                budget: MockContextBudget::new("100000", "4096", "1024"),
                provider_binding: None,
            })
            .unwrap();
        (project, access, started)
    }

    fn started_project_with_scope(
        label: &str,
        kind: ScopeKind,
    ) -> (ProjectSession, ProjectAccess, DiscussionStart) {
        let path = std::env::temp_dir().join(format!(
            "wns-desktop-worker-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = ProjectSession::create(path, "Worker structured test").unwrap();
        let access = project.documents().attach("test-session".into()).unwrap();
        let body = serde_json::json!({
            "schemaVersion": 1,
            "body": {"type":"doc","content":[
                {"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":"The ending stays."}]},
                {"type":"paragraph","attrs":{"id":"p2"},"content":[{"type":"text","text":"A protected neighbor."}]}
            ]}
        });
        let document = project
            .documents().create(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: body.clone(),
            })
            .unwrap();
        let captured = capture_scope(
            &body,
            ScopeGrant {
                kind,
                start: (kind == ScopeKind::Blocks).then_some(Endpoint {
                    block_id: "p1".into(),
                    utf16_offset: 0,
                }),
                end: (kind == ScopeKind::Blocks).then_some(Endpoint {
                    block_id: "p1".into(),
                    utf16_offset: 17,
                }),
                source_hash: String::new(),
                quote: String::new(),
                quote_hash: String::new(),
                prefix: None,
                suffix: None,
            },
        )
        .unwrap();
        let started = project
            .start_discussion(StartDiscussion {
                lookup: None,
                access: access.clone(),
                operation_id: "start".into(),
                expected: document.head.clone(),
                instruction: "Revise the selected blocks.".into(),
                intent: FeedbackIntent::ProposeEdits,
                basis: None,
                scope: Some(DiscussionScopeInput {
                    kind: captured.kind,
                    start: captured.start,
                    end: captured.end,
                    quote: captured.quote,
                    source_body_hash: captured.source_hash,
                }),
                pinned_document_ids: Vec::new(),
                safe_brief: None,
                previous_run_id: None,
                budget: MockContextBudget::new("100000", "4096", "1024"),
                provider_binding: None,
            })
            .unwrap();
        (project, access, started)
    }

    #[test]
    fn mock_continuation_retains_one_append_candidate_without_changing_the_chapter() {
        let (project, access, started) =
            started_project_with_intent("continuation-worker", FeedbackIntent::Continue);
        let recovery = DiscussionRecovery::default();
        let dispatch = recovery.claim(&project, &started.run).unwrap();
        run_mock_with_pause(project.clone(), recovery, dispatch, || {});
        let view = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(view.runs[0].status, DiscussionRunStatus::Completed);
        assert_eq!(view.runs[0].intent, FeedbackIntent::Continue);
        assert_eq!(
            view.runs[0].basis,
            Some(webnovel_core::context::BasisKind::Working)
        );
        let candidates = project.proposals(access.clone(), "chapter".into()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, ProposalKind::Continuation);
        assert!(candidates[0].prepared.is_none());
        assert!(candidates[0].decision.is_none());
        let document = project.documents().read(access, "chapter".into()).unwrap();
        assert_eq!(document.head, started.run.target);
        assert_eq!(
            document.body["body"]["content"][0]["content"][0]["text"],
            "The ending stays."
        );
        clean_project(project);
    }

    #[test]
    fn mock_structured_scope_output_is_retained_for_blocks_and_whole_document() {
        for (label, kind) in [
            ("structured-blocks-worker", ScopeKind::Blocks),
            ("structured-whole-worker", ScopeKind::WholeDocument),
        ] {
            let (project, access, started) = started_project_with_scope(label, kind);
            let recovery = DiscussionRecovery::default();
            let dispatch = recovery.claim(&project, &started.run).unwrap();
            run_mock_with_pause(project.clone(), recovery, dispatch, || {});

            let view = project
                .read_discussion(access.clone(), "chapter".into())
                .unwrap();
            assert_eq!(view.runs[0].status, DiscussionRunStatus::Completed);
            let candidates = project.proposals(access, "chapter".into()).unwrap();
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].kind, ProposalKind::Structured);
            let ProposalContent::Structured(candidate) = &candidates[0].candidate else {
                panic!("mock structured scope was retained as a legacy proposal");
            };
            assert_eq!(candidate.blocks.len(), 2);
            assert!(matches!(
                candidate.blocks[0],
                webnovel_core::documents::TypedReplacementBlock::Paragraph { .. }
            ));
            assert!(matches!(
                candidate.blocks[1],
                webnovel_core::documents::TypedReplacementBlock::Heading { .. }
            ));
            clean_project(project);
        }
    }

    #[test]
    fn mock_workshop_output_is_completed_as_raw_json_and_validates_to_stable_candidates() {
        for (action, instruction) in [
            ("directions", "Offer three different mechanisms."),
            ("moment", "Offer a concrete story moment."),
        ] {
            let path = std::env::temp_dir().join(format!(
                "wns-desktop-worker-workshop-{action}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let project = ProjectSession::create(path, "Workshop worker test").unwrap();
            let access = project.documents().attach("test-session".into()).unwrap();
            let document = project
            .documents().create(CreateDocument {
                access: access.clone(),
                operation_id: "create-anchor".into(),
                document_id: "workshop-anchor".into(),
                title: "Workshop anchor".into(),
                kind: "note".into(),
                body: serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":"The current story element."}]}]}}),
            })
            .unwrap();
            let request = webnovel_core::projects::workshop_generation::WorkshopGenerationRequest {
                access: access.clone(),
                operation_id: "workshop-operation".into(),
                exploration: webnovel_core::projects::workshop_generation::WorkshopExploration {
                    session_id: "session-1".into(),
                    expected_version: "0".into(),
                    working_generation: "0".into(),
                    action: action.into(),
                    instruction: instruction.into(),
                    selected_scope: "Whole working version".into(),
                    selected_text: "The editable selected passage".into(),
                    working_selection: None,
                },
                context: webnovel_core::projects::workshop_generation::WorkshopContext {
                    expected: document.head,
                    lens: webnovel_core::projects::workshop::Lens::Possibilities,
                    depth: webnovel_core::projects::workshop::WorkshopDepth::Develop,
                    current_element: "The current story element.".into(),
                    author_brief: None,
                    direction: "An editable direction".into(),
                    still_open: "Its consequences remain open".into(),
                    focus_question: "What changes next?".into(),
                    focus_reason: "Compare mechanisms before choosing one.".into(),
                    selected_details: vec![
                        webnovel_core::projects::workshop_generation::WorkshopLiteral {
                            text: "Keep this fixed detail".into(),
                            fixed: true,
                        },
                    ],
                    chosen_details: vec![],
                    fixed_details: vec!["Keep this fixed detail".into()],
                    fixed_source_refs: vec!["workshop-anchor@0".into()],
                    preferences: vec!["want ordinary life".into()],
                    hard_constraints: vec!["hard constraint: avoid hidden destiny".into()],
                    included_document_ids: vec![],
                    included_alternatives: vec![],
                    rejected_rationales: vec!["rejected-1: too narrow".into()],
                    questions: vec![],
                    story_possibilities: vec![],
                    original_notes: "An intentionally included author note.".into(),
                    outside_direction: false,
                    relationship: None,
                },
                budget: MockContextBudget::new("100000", "4096", "1024"),
                provider_binding: None,
            };
            let (start, metadata) = request.into_discussion().unwrap();
            let started = project.start_discussion(start).unwrap();
            let recovery = DiscussionRecovery::default();
            let dispatch = recovery.claim(&project, &started.run).unwrap();
            run_mock_with_pause(project.clone(), recovery, dispatch, || {});
            let view = project
                .read_discussion(access.clone(), "workshop-anchor".into())
                .unwrap();
            assert_eq!(view.runs[0].status, DiscussionRunStatus::Completed);
            let output = webnovel_core::projects::workshop_generation::validate_workshop_output(
                &view.runs[0].output_text,
                &metadata,
                &view.runs[0].id,
            )
            .unwrap();
            if action == "directions" {
                assert_eq!(output.candidates.len(), 3);
            } else {
                assert!((2..=3).contains(&output.candidates.len()));
            }
            assert_eq!(output.candidates[0].id, format!("{}-0", view.runs[0].id));
            assert!(
                output.candidates[0]
                    .content
                    .contains("Keep this fixed detail")
            );
            let document = project
                .documents().read(access.clone(), "workshop-anchor".into())
                .unwrap();
            assert_eq!(
                document.body["body"]["content"][0]["content"][0]["text"],
                "The current story element."
            );
            let workshop = project.workshop().read(access.clone()).unwrap();
            assert!(workshop.state.decisions.is_empty());
            assert!(workshop.state.relationships.is_empty());
            clean_project(project);
        }
    }

    #[test]
    fn mock_voice_guidance_completes_three_candidates_without_changing_source_events() {
        let path = std::env::temp_dir().join(format!(
            "wns-desktop-worker-voice-guidance-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = ProjectSession::create(path, "Voice guidance worker test").unwrap();
        let access = project.documents().attach("test-session".into()).unwrap();
        let source_text = "Rain ticked against the workshop glass while she counted each drop.";
        let document = project
            .documents().create(CreateDocument {
                access: access.clone(),
                operation_id: "create-voice-anchor".into(),
                document_id: "workshop-voice-anchor".into(),
                title: "Voice sample".into(),
                kind: "note".into(),
                body: serde_json::json!({
                    "schemaVersion": 1,
                    "body": {
                        "type": "doc",
                        "content": [{
                            "type": "paragraph",
                            "attrs": {"id": "p1"},
                            "content": [{"type": "text", "text": source_text}]
                        }]
                    }
                }),
            })
            .unwrap();
        let request = webnovel_core::projects::workshop_generation::WorkshopGenerationRequest {
            access: access.clone(),
            operation_id: "voice-guidance-operation".into(),
            exploration: webnovel_core::projects::workshop_generation::WorkshopExploration {
                session_id: "session-voice".into(),
                expected_version: "0".into(),
                working_generation: "0".into(),
                action: "voiceGuidance".into(),
                instruction: "Use this sample as voice evidence only; do not adopt its events."
                    .into(),
                selected_scope: "Voice qualities from the sample".into(),
                selected_text: source_text.into(),
                working_selection: None,
            },
            context: webnovel_core::projects::workshop_generation::WorkshopContext {
                expected: document.head,
                lens: webnovel_core::projects::workshop::Lens::Themes,
                depth: webnovel_core::projects::workshop::WorkshopDepth::Develop,
                current_element: source_text.into(),
                author_brief: None,
                direction: "Voice sample".into(),
                still_open: "The scene events remain noncanon.".into(),
                focus_question: "Which qualities should carry forward?".into(),
                focus_reason: "Compare reviewable style guidance sets.".into(),
                selected_details: vec![],
                chosen_details: vec![],
                fixed_details: vec![],
                fixed_source_refs: vec![],
                preferences: vec![],
                hard_constraints: vec![],
                included_document_ids: vec![],
                included_alternatives: vec![],
                rejected_rationales: vec![],
                questions: vec![],
                story_possibilities: vec![],
                original_notes: String::new(),
                outside_direction: false,
                relationship: None,
            },
            budget: MockContextBudget::new("100000", "4096", "1024"),
            provider_binding: None,
        };
        let (start, metadata) = request.into_discussion().unwrap();
        let started = project.start_discussion(start).unwrap();
        let recovery = DiscussionRecovery::default();
        let dispatch = recovery.claim(&project, &started.run).unwrap();
        run_mock_with_pause(project.clone(), recovery, dispatch, || {});

        let view = project
            .read_discussion(access.clone(), "workshop-voice-anchor".into())
            .unwrap();
        assert_eq!(view.runs[0].status, DiscussionRunStatus::Completed);
        let output = webnovel_core::projects::workshop_generation::validate_workshop_output(
            &view.runs[0].output_text,
            &metadata,
            &view.runs[0].id,
        )
        .unwrap();
        assert_eq!(output.candidates.len(), 3);
        assert!(
            output
                .candidates
                .iter()
                .all(|candidate| candidate.content.contains("STYLE guidance"))
        );
        assert!(
            output
                .candidates
                .iter()
                .all(|candidate| !candidate.content.contains(source_text))
        );
        assert_eq!(
            project
                .documents().read(access, "workshop-voice-anchor".into())
                .unwrap()
                .body["body"]["content"][0]["content"][0]["text"],
            source_text
        );
        clean_project(project);
    }

    #[test]
    fn model_selection_blocks_new_requests_without_rebinding_a_saved_request() {
        let (project, access, started) = started_project("model-binding");
        let request = StartDiscussion {
            lookup: None,
            access: access.clone(),
            operation_id: "start".into(),
            expected: started.run.target.clone(),
            instruction: "Discuss the ending.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            previous_run_id: None,
            budget: MockContextBudget::new("100000", "4096", "1024"),
            provider_binding: None,
        };
        let local = ModelSelection::local_mock();
        let codex = ModelSelection {
            provider_id: "codex".into(),
            model_id: "gpt-5.6-luna".into(),
            reasoning: Some("xhigh".into()),
            service_tier: Some("priority".into()),
        };
        assert!(check_model_choice(&project, &request, Some(&local), &local).is_ok());
        assert_eq!(
            check_model_choice(&project, &request, Some(&codex), &codex)
                .unwrap_err()
                .code,
            "ProviderUnavailable"
        );
        // A settings change does not prevent checking an old mock receipt.
        assert!(check_model_choice(&project, &request, Some(&local), &codex).is_ok());
        assert!(check_model_choice(&project, &request, None, &codex).is_ok());
        let replay = project.start_discussion(request.clone()).unwrap();
        assert_eq!(replay.run.id, started.run.id);
        let mut changed = request.clone();
        changed.instruction = "A different request".into();
        assert!(check_model_choice(&project, &changed, Some(&local), &codex).is_ok());
        assert!(project.start_discussion(changed).is_err()); // Core still checks exact payload.
        let mut fresh = request;
        fresh.operation_id = "fresh-operation".into();
        assert_eq!(
            check_model_choice(&project, &fresh, Some(&local), &codex)
                .unwrap_err()
                .code,
            "ModelChoiceChanged"
        );
        assert_eq!(
            check_model_choice(&project, &fresh, None, &codex)
                .unwrap_err()
                .code,
            "ModelChoiceChanged"
        );
        let mut invalid_mock = local.clone();
        invalid_mock.reasoning = Some("max".into());
        assert_eq!(
            check_model_choice(&project, &fresh, Some(&invalid_mock), &local)
                .unwrap_err()
                .code,
            "ProviderUnavailable"
        );
        assert_eq!(
            project
                .read_discussion(access, "chapter".into())
                .unwrap()
                .runs
                .len(),
            1
        );
        clean_project(project);
    }

    fn claimed_project(label: &str) -> (ProjectSession, ProjectAccess, DiscussionDispatch) {
        let (project, access, started) = started_project(label);
        let dispatch = project
            .begin_discussion_run(DiscussionBegin {
                owner: started.run.owner,
            })
            .unwrap();
        (project, access, dispatch)
    }

    fn queued_project(label: &str) -> (ProjectSession, ProjectAccess, DiscussionRun) {
        let (project, access, started) = started_project(label);
        (project, access, started.run)
    }

    fn clean_project(project: ProjectSession) {
        let path = project.path.clone();
        let temp_root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let canonical = std::fs::canonicalize(&path).unwrap();
        assert_eq!(canonical.parent(), Some(temp_root.as_path()));
        assert!(
            canonical
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("wns-desktop-worker-"))
        );
        drop(project);
        std::fs::remove_dir_all(path).unwrap();
    }

    fn terminal_fault(project: &ProjectSession, enabled: bool) {
        let db = rusqlite::Connection::open(project.path.join("project.sqlite3")).unwrap();
        db.execute_batch(if enabled {
            "CREATE TRIGGER terminal_fault BEFORE INSERT ON discussion_messages WHEN NEW.role='assistant' BEGIN SELECT RAISE(ABORT,'test terminal write failure'); END;"
        } else { "DROP TRIGGER terminal_fault;" }).unwrap();
    }

    fn claim_fault(project: &ProjectSession, enabled: bool) {
        let db = rusqlite::Connection::open(project.path.join("project.sqlite3")).unwrap();
        db.execute_batch(if enabled {
            "CREATE TRIGGER claim_fault BEFORE UPDATE OF status ON discussion_runs WHEN NEW.status='running' BEGIN SELECT RAISE(ABORT,'test claim failure'); END;"
        } else { "DROP TRIGGER claim_fault;" }).unwrap();
    }

    fn issue_count(
        recovery: &DiscussionRecovery,
        project: &ProjectSession,
        access: &ProjectAccess,
    ) -> usize {
        let view = recovery.view(
            project
                .read_discussion(access.clone(), "chapter".into())
                .unwrap(),
        );
        serde_json::to_value(view).unwrap()["workerIssues"]
            .as_array()
            .unwrap()
            .len()
    }

    #[test]
    fn terminal_write_failure_is_visible_and_retries_only_the_saved_completion() {
        let (project, access, dispatch) = claimed_project("finish-recovery");
        let recovery = DiscussionRecovery::default();
        let expected = mock_output(&dispatch.packet, dispatch.run.intent)
            .unwrap()
            .concat();
        let run_id = dispatch.run.id.clone();
        terminal_fault(&project, true);
        let mut invocations = 0;
        run_mock_with_pause(project.clone(), recovery.clone(), dispatch, || {
            invocations += 1;
        });
        assert_eq!(invocations, 3);
        assert_eq!(issue_count(&recovery, &project, &access), 1);
        let before = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(before.runs[0].status, DiscussionRunStatus::Running);
        assert_eq!(before.runs[0].output_text, expected);
        assert!(
            recovery
                .retry(&project, access.clone(), "chapter".into(), run_id.clone())
                .is_err()
        );
        assert_eq!(issue_count(&recovery, &project, &access), 1);
        terminal_fault(&project, false);
        recovery
            .retry(&project, access.clone(), "chapter".into(), run_id.clone())
            .unwrap();
        recovery
            .retry(&project, access.clone(), "chapter".into(), run_id)
            .unwrap();
        assert_eq!(issue_count(&recovery, &project, &access), 0);
        let after = project.read_discussion(access, "chapter".into()).unwrap();
        assert_eq!(after.runs[0].status, DiscussionRunStatus::Completed);
        assert_eq!(after.messages.len(), 2);
        assert_eq!(after.messages[1].content, expected);
        assert_eq!(invocations, 3);
        clean_project(project);
    }

    #[test]
    fn lost_append_ack_preserves_committed_prefix_during_local_recovery() {
        let (project, access, dispatch) = claimed_project("append-ack-recovery");
        let recovery = DiscussionRecovery::default();
        let owner = dispatch.run.owner.clone();
        let running = project.mark_discussion_delivered(owner.clone()).unwrap();
        let stale_before_append = running.clone();
        let committed = project
            .append_discussion_output(DiscussionOutputAppend {
                owner: owner.clone(),
                expected_sequence: running.sequence,
                event_id: format!("{}-part-0", owner.run_id),
                chunk: "committed prefix".into(),
            })
            .unwrap();
        assert_eq!(committed.sequence, "1");
        assert_eq!(committed.output_text, "committed prefix");

        // Model an append acknowledgment lost after SQLite committed: the
        // worker still holds the pre-append sequence and reports an uncertain
        // outcome. This test does not claim to reproduce the storage fault;
        // it exercises the stale-sequence recovery boundary.
        record_worker_failure(
            &project,
            &recovery,
            &stale_before_append,
            CoreError::new("UncertainOutcome", "simulated lost append acknowledgment"),
        );
        assert_eq!(issue_count(&recovery, &project, &access), 1);

        let before_retry = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(before_retry.runs.len(), 1);
        assert_eq!(before_retry.runs[0].status, DiscussionRunStatus::Running);
        assert_eq!(before_retry.runs[0].sequence, "1");
        assert_eq!(before_retry.runs[0].output_text, "committed prefix");

        recovery
            .retry(&project, access.clone(), "chapter".into(), owner.run_id)
            .unwrap();
        let after_retry = project.read_discussion(access, "chapter".into()).unwrap();
        assert_eq!(after_retry.runs.len(), 1);
        assert_eq!(after_retry.runs[0].status, DiscussionRunStatus::Failed);
        assert_eq!(after_retry.runs[0].sequence, "2");
        assert_eq!(after_retry.runs[0].output_text, "committed prefix");
        assert_eq!(after_retry.messages.len(), 2);
        clean_project(project);
    }

    #[test]
    fn author_stop_preempts_pending_completion_without_proposals() {
        let (project, access, dispatch) = claimed_project("stop-pending-completion");
        let recovery = DiscussionRecovery::default();
        let expected = mock_output(&dispatch.packet, dispatch.run.intent)
            .unwrap()
            .concat();
        let run_id = dispatch.run.id.clone();

        // The worker has durably appended every chunk, but its terminal write
        // cannot commit. This leaves a saved completion pending while the run
        // remains Running.
        terminal_fault(&project, true);
        run_mock_with_pause(project.clone(), recovery.clone(), dispatch, || {});
        let running = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(running.runs[0].status, DiscussionRunStatus::Running);
        assert_eq!(running.runs[0].output_text, expected);
        assert_eq!(issue_count(&recovery, &project, &access), 1);

        let stopped = project
            .stop_discussion(access.clone(), run_id.clone())
            .unwrap();
        assert_eq!(stopped.run.status, DiscussionRunStatus::Stopping);
        assert_eq!(stopped.run.output_text, expected);

        terminal_fault(&project, false);
        recovery
            .retry(&project, access.clone(), "chapter".into(), run_id)
            .unwrap();

        let after = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(after.runs.len(), 1);
        assert_eq!(after.runs[0].status, DiscussionRunStatus::Stopped);
        assert_eq!(after.runs[0].output_text, expected);
        assert_eq!(after.messages.len(), 2);
        assert_eq!(
            project
                .proposals(access.clone(), "chapter".into())
                .unwrap()
                .len(),
            0
        );
        assert_eq!(issue_count(&recovery, &project, &access), 0);
        clean_project(project);
    }

    #[test]
    fn failed_stop_save_survives_navigation_and_rejects_stale_or_wrong_access() {
        let (project, access, dispatch) = claimed_project("stop-recovery");
        let recovery = DiscussionRecovery::default();
        let run_id = dispatch.run.id.clone();
        terminal_fault(&project, true);
        let mut pauses = 0;
        run_mock_with_pause(project.clone(), recovery.clone(), dispatch, || {
            pauses += 1;
            if pauses == 2 {
                project
                    .stop_discussion(access.clone(), run_id.clone())
                    .unwrap();
            }
        });
        assert_eq!(issue_count(&recovery, &project, &access), 1);
        let fresh = project.documents().attach("next-renderer".into()).unwrap();
        assert!(
            recovery
                .retry(&project, access, "chapter".into(), run_id.clone())
                .is_err()
        );
        assert!(
            recovery
                .retry(
                    &project,
                    fresh.clone(),
                    "other-chapter".into(),
                    run_id.clone()
                )
                .is_err()
        );
        let mut wrong = fresh.clone();
        wrong.operation_namespace = "wrong".into();
        assert!(
            recovery
                .retry(&project, wrong, "chapter".into(), run_id.clone())
                .is_err()
        );
        assert_eq!(issue_count(&recovery, &project, &fresh), 1);
        terminal_fault(&project, false);
        recovery
            .retry(&project, fresh.clone(), "chapter".into(), run_id)
            .unwrap();
        let after = project
            .read_discussion(fresh.clone(), "chapter".into())
            .unwrap();
        assert_eq!(after.runs[0].status, DiscussionRunStatus::Stopped);
        assert_eq!(after.messages.len(), 2);
        assert!(
            after.messages[1]
                .content
                .starts_with(&after.runs[0].output_text)
        );
        assert_eq!(issue_count(&recovery, &project, &fresh), 0);
        assert_eq!(pauses, 2);
        clean_project(project);
    }

    #[test]
    fn local_recovery_cannot_take_over_a_run_without_a_finished_worker() {
        let (project, access, dispatch) = claimed_project("active-owner");
        let recovery = DiscussionRecovery::default();
        let error = recovery
            .retry(&project, access.clone(), "chapter".into(), dispatch.run.id)
            .err()
            .unwrap();
        assert_eq!(error.code, "ResponseStillRunning");
        assert_eq!(
            project
                .read_discussion(access, "chapter".into())
                .unwrap()
                .runs[0]
                .status,
            DiscussionRunStatus::Running
        );
        clean_project(project);
    }

    #[test]
    fn failed_claim_stays_pending_until_explicit_local_retry() {
        let (project, access, queued) = queued_project("claim-recovery");
        let recovery = DiscussionRecovery::default();

        claim_fault(&project, true);
        assert!(recovery.claim(&project, &queued).is_none());
        claim_fault(&project, false);

        let before = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(before.runs[0].status, DiscussionRunStatus::Queued);
        assert_eq!(before.runs[0].dispatch_state, "pending");
        assert!(before.runs[0].output_text.is_empty());
        assert_eq!(issue_count(&recovery, &project, &access), 1);

        // The unresolved local claim entry fences a second claim. No
        // DiscussionDispatch is returned, so no worker can be spawned.
        assert!(recovery.claim(&project, &queued).is_none());
        let still_queued = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(still_queued.runs[0].status, DiscussionRunStatus::Queued);
        assert_eq!(still_queued.runs[0].dispatch_state, "pending");
        assert_eq!(issue_count(&recovery, &project, &access), 1);

        recovery
            .retry(
                &project,
                access.clone(),
                "chapter".into(),
                queued.id.clone(),
            )
            .unwrap();
        let failed = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(failed.runs[0].status, DiscussionRunStatus::Failed);
        assert!(failed.runs[0].output_text.is_empty());
        assert_eq!(issue_count(&recovery, &project, &access), 0);

        // A terminally failed run remains non-dispatchable after the local
        // retry has consumed the pending claim outcome.
        assert!(recovery.claim(&project, &failed.runs[0]).is_none());
        assert_eq!(
            project
                .read_discussion(access, "chapter".into())
                .unwrap()
                .runs[0]
                .status,
            DiscussionRunStatus::Failed
        );
        clean_project(project);
    }

    #[test]
    fn mock_acknowledges_stop_before_delivery_without_emitting_output() {
        let (project, access, dispatch) = claimed_project("before-delivery");
        let stopped = project
            .stop_discussion(access.clone(), dispatch.run.id.clone())
            .unwrap();
        assert_eq!(stopped.run.status, DiscussionRunStatus::Stopping);
        run_mock_with_pause(
            project.clone(),
            DiscussionRecovery::default(),
            dispatch,
            || panic!("stopped before output"),
        );
        let view = project.read_discussion(access, "chapter".into()).unwrap();
        assert_eq!(view.runs[0].status, DiscussionRunStatus::Stopped);
        assert!(view.runs[0].output_text.is_empty());
        assert_eq!(view.messages.len(), 2);
        clean_project(project);
    }

    #[test]
    fn mock_retains_the_durable_prefix_when_stop_arrives_between_chunks() {
        let (project, access, dispatch) = claimed_project("between-chunks");
        let run_id = dispatch.run.id.clone();
        let expected = mock_output(&dispatch.packet, dispatch.run.intent).unwrap()[0].clone();
        let mut pause_count = 0;
        run_mock_with_pause(
            project.clone(),
            DiscussionRecovery::default(),
            dispatch,
            || {
                pause_count += 1;
                if pause_count == 2 {
                    let stop = project
                        .stop_discussion(access.clone(), run_id.clone())
                        .unwrap();
                    assert_eq!(stop.run.status, DiscussionRunStatus::Stopping);
                    assert_eq!(stop.run.output_text, expected);
                }
            },
        );
        let view = project
            .read_discussion(access.clone(), "chapter".into())
            .unwrap();
        assert_eq!(pause_count, 2);
        assert_eq!(view.runs[0].status, DiscussionRunStatus::Stopped);
        assert_eq!(view.runs[0].output_text, expected);
        assert_eq!(view.messages.len(), 2);
        let document = project.documents().read(access, "chapter".into()).unwrap();
        assert_eq!(document.head.version, "0");
        assert_eq!(
            document.body["body"]["content"][0]["content"][0]["text"],
            "The ending stays."
        );
        clean_project(project);
    }

    #[test]
    fn mock_validation_failure_cannot_override_an_earlier_stop() {
        let (project, access, mut dispatch) = claimed_project("failed-after-stop");
        project
            .stop_discussion(access.clone(), dispatch.run.id.clone())
            .unwrap();
        dispatch.packet.receipt.input_hash = "changed".into();
        run_mock_with_pause(
            project.clone(),
            DiscussionRecovery::default(),
            dispatch,
            || panic!("invalid input"),
        );
        let view = project.read_discussion(access, "chapter".into()).unwrap();
        assert_eq!(view.runs[0].status, DiscussionRunStatus::Stopped);
        assert!(view.runs[0].output_text.is_empty());
        assert_eq!(view.messages.len(), 2);
        clean_project(project);
    }

    fn packet() -> CompiledPacket {
        let messages = vec![
            PacketMessage {
                role: "system".into(),
                content: "system".into(),
            },
            PacketMessage {
                role: "user".into(),
                content: r#"{"scope":{"quote":"selected"}}"#.into(),
            },
            PacketMessage {
                role: "user".into(),
                content: "Revise the selected passage.".into(),
            },
        ];
        let options = PacketOptions {
            model_id: MOCK_MODEL_ID.into(),
            max_output_tokens: "4096".into(),
            token_accounting_method: "mock".into(),
            provider_binding: None,
        };
        let input_hash = packet_input_hash(&messages, &options).unwrap();
        CompiledPacket {
            messages,
            options,
            receipt: PacketReceipt {
                reviewed_knowledge: Vec::new(),
                reviewed_knowledge_omissions: Vec::new(),
                lookup: None,
                packet_id: "packet".into(),
                session_id: "session".into(),
                snapshot_id: "snapshot".into(),
                invocation_ordinal: "0".into(),
                source_handles: vec!["source".into()],
                mandatory_source_handles: Vec::new(),
                guidance_handles: Vec::new(),
                navigation_views: Vec::new(),
                navigation_omissions: Vec::new(),
                reviewed_evidence: Vec::new(),
                reviewed_evidence_omissions: Vec::new(),
                reviewed_promises: Vec::new(),
                reviewed_summaries: Vec::new(),
                reviewed_promise_omissions: Vec::new(),
                reviewed_summary_omissions: Vec::new(),
                conversation_message_ids: Vec::new(),
                omitted_discussion_turns: 0,
                safe_brief: None,
                coverage: Vec::new(),
                omissions: Vec::new(),
                input_hash,
                input_tokens: "100".into(),
                token_accounting_method: "mock".into(),
            },
        }
    }

    #[test]
    fn proposal_mock_output_is_strict_json_and_discuss_stays_textual() {
        let packet = packet();
        let proposal_chunks = mock_output(&packet, FeedbackIntent::ProposeEdits).unwrap();
        assert_eq!(proposal_chunks.len(), 1);
        let output: ProposalOutput = serde_json::from_str(&proposal_chunks.concat()).unwrap();
        assert_eq!(output.suggestions.len(), 3);
        assert_eq!(
            output.suggestions[0].replacement_text,
            "A clearer test phrase."
        );
        assert!(
            output
                .suggestions
                .iter()
                .all(|candidate| !candidate.replacement_text.contains(['\n', '\r']))
        );

        let discussion_chunks = mock_output(&packet, FeedbackIntent::Discuss).unwrap();
        assert_eq!(discussion_chunks.len(), 3);
        assert!(discussion_chunks[0].contains("no live AI model"));
    }
}
