//! Author commands use renderer leases; the local test worker owns only its run.
use crate::discussion_recovery::{
    DesktopDiscussionView, DiscussionRecovery, PendingSave, SaveOutcome,
};
use crate::library_commands::DesktopLibrary;
use crate::project_commands::{DesktopProjects, execute};
use tauri::State;
use webnovel_core::context::packet::{CompiledPacket, MOCK_MODEL_ID, packet_input_hash};
use webnovel_core::projects::discussions::*;
use webnovel_core::projects::proposals::*;
use webnovel_core::projects::{CoreError, CoreResult, ProjectAccess, ProjectSession};
use webnovel_core::providers::preferences::ModelSelection;

#[tauri::command]
pub async fn read_discussion(
    access: ProjectAccess,
    document_id: String,
    state: State<'_, DesktopProjects>,
    recovery: State<'_, DiscussionRecovery>,
) -> CoreResult<DesktopDiscussionView> {
    let project = state.project(&access.project_id)?;
    let recovery = recovery.inner().clone();
    execute(move || Ok(recovery.view(project.read_discussion(access, document_id)?))).await
}

#[tauri::command]
pub async fn retry_discussion_save(
    access: ProjectAccess,
    document_id: String,
    run_id: String,
    state: State<'_, DesktopProjects>,
    recovery: State<'_, DiscussionRecovery>,
) -> CoreResult<DesktopDiscussionView> {
    let project = state.project(&access.project_id)?;
    let recovery = recovery.inner().clone();
    execute(move || recovery.retry(&project, access, document_id, run_id)).await
}

#[tauri::command]
pub async fn discussion_retry(
    access: ProjectAccess,
    run_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DiscussionRetry> {
    let project = state.project(&access.project_id)?;
    execute(move || project.discussion_retry(access, run_id)).await
}

#[tauri::command]
pub async fn save_discussion_draft(
    request: SaveDiscussionDraft,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DiscussionDraft> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.save_discussion_draft(request)).await
}

#[tauri::command]
pub async fn stop_discussion(
    access: ProjectAccess,
    run_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DiscussionStop> {
    let project = state.project(&access.project_id)?;
    execute(move || project.stop_discussion(access, run_id)).await
}

#[tauri::command]
pub async fn proposals(
    access: ProjectAccess,
    document_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Vec<Proposal>> {
    let project = state.project(&access.project_id)?;
    execute(move || project.proposals(access, document_id)).await
}

#[tauri::command]
pub async fn prepare_proposal(
    request: PrepareProposal,
    state: State<'_, DesktopProjects>,
) -> CoreResult<PreparedProposal> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.prepare_proposal(request)).await
}

#[tauri::command]
pub async fn apply_proposal(
    request: ApplyProposal,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ApplyAck> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.apply_proposal(request)).await
}

#[tauri::command]
pub async fn reject_proposal(
    request: RejectProposal,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ProposalDecision> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.reject_proposal(request)).await
}

#[tauri::command]
pub async fn start_discussion(
    request: StartDiscussion,
    model_selection: Option<ModelSelection>,
    state: State<'_, DesktopProjects>,
    recovery: State<'_, DiscussionRecovery>,
    library: State<'_, DesktopLibrary>,
) -> CoreResult<DiscussionStart> {
    let project = state.project(&request.access.project_id)?;
    let recovery = recovery.inner().clone();
    let library = library.inner().clone();
    execute(move || {
        let started = {
            // Preference acceptance and new request acceptance are serialized.
            // Subsequent setting changes cannot redirect this frozen request.
            let library = library
                .0
                .lock()
                .map_err(|_| crate::provider_commands::unavailable())?;
            let active = library.provider_state()?.settings.active;
            check_model_choice(&project, &request, model_selection.as_ref(), &active)?;
            project.start_discussion(request)?
        };
        if started.run.status == DiscussionRunStatus::Queued {
            // A duplicate lost-ack retry can reach here. Only one worker can
            // claim the durable queued run. Claim before spawning, so failure
            // to spawn a duplicate cannot seal another worker's running job.
            if let Some(dispatch) = recovery.claim(&project, &started.run) {
                let failure_project = project.clone();
                let failure_run = dispatch.run.clone();
                let worker_recovery = recovery.clone();
                if std::thread::Builder::new()
                    .name("webnovel-test-response".into())
                    .spawn(move || run_mock(project, worker_recovery, dispatch))
                    .is_err()
                {
                    record_worker_failure(
                        &failure_project,
                        &recovery,
                        &failure_run,
                        CoreError::new(
                            "WorkerUnavailable",
                            "The local test worker could not start.",
                        ),
                    );
                }
            }
        }
        Ok(started)
    })
    .await
}

fn check_model_choice(
    project: &ProjectSession,
    request: &StartDiscussion,
    requested: Option<&ModelSelection>,
    active: &ModelSelection,
) -> CoreResult<()> {
    let local = ModelSelection::local_mock();
    // Older native callers have an explicit mock-only packet contract.
    let requested = requested.unwrap_or(&local);
    if requested != &local {
        return Err(CoreError::new(
            "ProviderUnavailable",
            "The selected model is not connected in this preview. Choose the local test model to try the discussion flow.",
        ));
    }
    if active == requested {
        return Ok(());
    }
    // An old uncertain acknowledgment must still be resolvable after a model
    // preference change. Core verifies the immutable operation payload before
    // returning its receipt; a new operation cannot use this exception.
    let view =
        project.read_discussion(request.access.clone(), request.expected.document_id.clone())?;
    if view.runs.iter().any(|run| {
        run.operation_id == request.operation_id
            && run.owner.operation_namespace == request.access.operation_namespace
            && run.owner.project_id == request.access.project_id
    }) {
        return Ok(());
    }
    Err(CoreError::new(
        "ModelChoiceChanged",
        "The selected model changed before this request started. Check the model selector and send again.",
    ))
}

fn run_mock(project: ProjectSession, recovery: DiscussionRecovery, dispatch: DiscussionDispatch) {
    run_mock_with_pause(project, recovery, dispatch, || {
        std::thread::sleep(std::time::Duration::from_millis(150));
    });
}

fn run_mock_with_pause(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    dispatch: DiscussionDispatch,
    mut pause: impl FnMut(),
) {
    let owner = dispatch.run.owner.clone();
    let chunks = match mock_output(&dispatch.packet, dispatch.run.intent) {
        Ok(chunks) => chunks,
        Err(error) => {
            record_worker_failure(&project, &recovery, &dispatch.run, error);
            return;
        }
    };
    let mut run = match project.mark_discussion_delivered(owner.clone()) {
        Ok(run) => run,
        Err(error) => {
            record_worker_failure(&project, &recovery, &dispatch.run, error);
            return;
        }
    };
    for chunk in &chunks {
        pause();
        match project.append_discussion_output(DiscussionOutputAppend {
            owner: owner.clone(),
            expected_sequence: run.sequence.clone(),
            event_id: format!("{}-part-{}", owner.run_id, run.sequence),
            chunk: chunk.clone(),
        }) {
            Ok(updated) => run = updated,
            Err(error) => {
                record_worker_failure(&project, &recovery, &run, error);
                return;
            }
        }
    }
    let event_id = format!("{}-finish", owner.run_id);
    let finish = DiscussionFinish {
        owner: owner.clone(),
        expected_sequence: run.sequence.clone(),
        event_id,
        assistant_text: chunks.concat(),
    };
    save_worker_outcome(
        &project,
        &recovery,
        PendingSave {
            run,
            outcome: SaveOutcome::Complete(finish),
        },
    );
}

fn record_worker_failure(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: &DiscussionRun,
    error: CoreError,
) {
    if error.code == "RunStopping" {
        let mut current = run.clone();
        current.status = DiscussionRunStatus::Stopping;
        save_worker_outcome(
            project,
            recovery,
            PendingSave {
                run: current,
                outcome: SaveOutcome::Stop,
            },
        );
        return;
    }
    // An uncertain commit remains fenced by the core; never replay output or
    // bypass reconciliation. A completed terminal record is immutable.
    if error.code == "RunSealed" {
        return;
    }
    save_worker_outcome(
        project,
        recovery,
        PendingSave {
            run: run.clone(),
            outcome: SaveOutcome::Fail,
        },
    );
}

fn save_worker_outcome(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    mut pending: PendingSave,
) {
    // The mock owns no external process or asynchronous I/O. All its output
    // work is finished here; only then may the actor acknowledge cleanup.
    if let Err(error) = pending.attempt(project, &pending.run) {
        if error.code == "RunSealed" {
            return;
        }
        if error.code == "RunStopping" {
            pending.run.status = DiscussionRunStatus::Stopping;
            pending.outcome = SaveOutcome::Stop;
            if pending.attempt(project, &pending.run).is_ok() {
                return;
            }
        }
        recovery.retain(pending);
    }
}

/// A fixed local fixture, deliberately labelled and incapable of writing prose.
fn mock_output(packet: &CompiledPacket, intent: FeedbackIntent) -> CoreResult<Vec<String>> {
    let invalid = || {
        CoreError::new(
            "InvalidMockInput",
            "The local test request failed its exact-input check.",
        )
    };
    if packet.options.model_id != MOCK_MODEL_ID
        || packet_input_hash(&packet.messages, &packet.options).map_err(|_| invalid())?
            != packet.receipt.input_hash
    {
        return Err(invalid());
    }
    let instruction = packet
        .messages
        .last()
        .ok_or_else(invalid)?
        .content
        .chars()
        .take(220)
        .collect::<String>();
    let envelope: serde_json::Value =
        serde_json::from_str(&packet.messages.get(1).ok_or_else(invalid)?.content)
            .map_err(|_| invalid())?;
    let focus = envelope
        .get("scope")
        .and_then(|scope| scope.get("quote"))
        .and_then(|quote| quote.as_str())
        .map(|quote| {
            format!(
                "Selected passage: “{}”\n\n",
                quote.chars().take(180).collect::<String>()
            )
        })
        .unwrap_or_else(|| "Focus: the whole document.\n\n".into());
    let chunks = vec![
        "Local test response — no live AI model is connected.\n\n".to_owned(),
        format!("{focus}Your request: {instruction}\n\n"),
        format!(
            "This request supplied {} story sources. Open Story context to inspect their exact saved versions and any omissions. This test confirms discussion and context handling; it does not evaluate or rewrite your story.",
            packet.receipt.source_handles.len()
        ),
    ];
    let allowance = packet
        .options
        .max_output_tokens
        .parse::<usize>()
        .map_err(|_| invalid())?;
    if intent == FeedbackIntent::ProposeEdits {
        let output = serde_json::json!({
            "suggestions": [
                {
                    "title": "Mock clarity option",
                    "replacementText": "A clearer test phrase.",
                    "explanation": "Deterministic local test alternative; not generated prose."
                },
                {
                    "title": "Mock focus option",
                    "replacementText": "A more focused test phrase.",
                    "explanation": "Deterministic local test alternative; not generated prose."
                },
                {
                    "title": "Mock simplicity option",
                    "replacementText": "A simpler test phrase.",
                    "explanation": "Deterministic local test alternative; not generated prose."
                }
            ]
        });
        let encoded = serde_json::to_string(&output).map_err(|_| invalid())?;
        if encoded.len() > allowance {
            return Err(CoreError::new(
                "OutputBudgetTooSmall",
                "The reserved response allowance is too small for the local test response.",
            ));
        }
        return Ok(vec![encoded]);
    }
    if chunks.iter().map(String::len).sum::<usize>() > allowance {
        return Err(CoreError::new(
            "OutputBudgetTooSmall",
            "The reserved response allowance is too small for the local test response.",
        ));
    }
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use webnovel_core::context::PacketReceipt;
    use webnovel_core::context::packet::MockContextBudget;
    use webnovel_core::context::packet::{PacketMessage, PacketOptions};
    use webnovel_core::projects::CreateDocument;

    fn started_project(label: &str) -> (ProjectSession, ProjectAccess, DiscussionStart) {
        let path = std::env::temp_dir().join(format!(
            "wns-desktop-worker-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = ProjectSession::create(path, "Worker test").unwrap();
        let access = project.attach("test-session".into()).unwrap();
        let document = project.create_document(CreateDocument {
            access: access.clone(), operation_id: "create".into(), document_id: "chapter".into(),
            title: "Chapter".into(), kind: "chapter".into(),
            body: serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":"The ending stays."}]}]}}),
        }).unwrap();
        let started = project
            .start_discussion(StartDiscussion {
                access: access.clone(),
                operation_id: "start".into(),
                expected: document.head,
                instruction: "Discuss the ending.".into(),
                intent: FeedbackIntent::Discuss,
                scope: None,
                pinned_document_ids: Vec::new(),
                safe_brief: None,
                previous_run_id: None,
                budget: MockContextBudget::new("100000", "4096", "1024"),
            })
            .unwrap();
        (project, access, started)
    }

    #[test]
    fn model_selection_blocks_new_requests_without_rebinding_a_saved_request() {
        let (project, access, started) = started_project("model-binding");
        let request = StartDiscussion {
            access: access.clone(),
            operation_id: "start".into(),
            expected: started.run.target.clone(),
            instruction: "Discuss the ending.".into(),
            intent: FeedbackIntent::Discuss,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            previous_run_id: None,
            budget: MockContextBudget::new("100000", "4096", "1024"),
        };
        let local = ModelSelection::local_mock();
        let codex = ModelSelection {
            provider_id: "codex".into(),
            model_id: "gpt-5.6-luna".into(),
            reasoning: Some("max".into()),
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
        let fresh = project.attach("next-renderer".into()).unwrap();
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
        let document = project.document(access, "chapter".into()).unwrap();
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
        };
        let input_hash = packet_input_hash(&messages, &options).unwrap();
        CompiledPacket {
            messages,
            options,
            receipt: PacketReceipt {
                packet_id: "packet".into(),
                session_id: "session".into(),
                snapshot_id: "snapshot".into(),
                invocation_ordinal: "0".into(),
                source_handles: vec!["source".into()],
                mandatory_source_handles: Vec::new(),
                guidance_handles: Vec::new(),
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
