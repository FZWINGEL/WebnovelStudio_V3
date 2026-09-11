//! Discussion worker for the persistent Codex app-server transport.
//!
//! The app-server owns process IO, while this worker owns the application
//! protocol: one frozen packet, one durable dispatch claim, coalesced local
//! output, and one explicit local settlement.  A stream or callback failure
//! never authorizes a second provider request.
#![cfg(windows)]

use crate::discussion_recovery::{DiscussionRecovery, PendingSave, SaveOutcome};
use crate::provider_runtime::DesktopProviders;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use webnovel_core::context::packet::{ProviderBinding, packet_input_hash, serialized_input};
use webnovel_core::projects::{CoreResult, ProjectSession, discussions::*};
use webnovel_core::providers::cli::windows_process::StopSignal;
use webnovel_core::providers::codex_app_server::protocol::{ThreadStartConfig, TurnFailure};
use webnovel_core::providers::codex_app_server::runtime::{
    AppServerLocalFailure, AppServerReservation, AppServerStreamEvent,
};
use webnovel_core::providers::codex_app_server::{
    self, AppServerDelivery, AppServerDispatch, AppServerSubmission,
};
use webnovel_core::providers::codex_exec::CodexFailureCode;
use webnovel_core::providers::codex_runner::{CodexRunResult, CodexRunStatus};

const DISCUSSION_FLUSH_BYTES: usize = 1024;
const DISCUSSION_FLUSH_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Default)]
struct AppServerDiagnostics {
    provider: Option<TurnFailure>,
    local: Option<AppServerLocalFailure>,
}

/// Runs one already admitted app-server discussion request.
///
/// `reservation` is consumed exactly once by `start`.  The caller has already
/// claimed the ordinary project run; this worker performs the separate
/// app-server dispatch claim in the driver's before-turn callback.
pub fn run_live(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    runtime: DesktopProviders,
    reservation: AppServerReservation,
    thread_config: ThreadStartConfig,
    dispatch: DiscussionDispatch,
    stop: StopSignal,
) {
    let owner = dispatch.run.owner.clone();
    let _registration = Registration {
        runtime,
        owner: owner.clone(),
    };

    if dispatch.run.lookup.is_some() {
        save_failure(
            &project,
            &recovery,
            dispatch.run,
            "Bounded lookup discussions use their lookup dispatcher.",
        );
        return;
    }

    let mut run = dispatch.run;

    let binding = match dispatch.packet.options.provider_binding.clone() {
        Some(binding)
            if codex_app_server::is_app_server(&binding) && binding.validate().is_ok() =>
        {
            binding
        }
        _ => {
            save_failure(
                &project,
                &recovery,
                run,
                "The request's saved app-server model settings could not be validated.",
            );
            return;
        }
    };
    let input = match serialized_input(&dispatch.packet.messages, &dispatch.packet.options) {
        Ok(input)
            if packet_input_hash(&dispatch.packet.messages, &dispatch.packet.options)
                .ok()
                .as_ref()
                == Some(&dispatch.packet.receipt.input_hash) =>
        {
            input
        }
        _ => {
            save_failure(
                &project,
                &recovery,
                run,
                "The exact saved request could not be validated.",
            );
            return;
        }
    };

    // Refresh the authoritative run before starting.  This handles a Stop or
    // project close that arrived after dispatch admission without changing the
    // frozen packet or permitting a new dispatch.
    match project.read_discussion_run(owner.clone()) {
        Ok(current) => {
            if current.status == DiscussionRunStatus::Stopping {
                stop.request_stop();
            }
            run = current;
        }
        Err(_) => {
            save_failure(
                &project,
                &recovery,
                run,
                "The request's saved state could not be checked before starting.",
            );
            return;
        }
    }
    if !matches!(
        run.status,
        DiscussionRunStatus::Running | DiscussionRunStatus::Stopping
    ) {
        return;
    }
    if stop.is_requested() {
        save_report(
            &project,
            &recovery,
            run,
            &binding,
            failed(CodexRunStatus::Stopped),
            AppServerDelivery::not_sent(),
            AppServerDiagnostics::default(),
        );
        return;
    }

    let callback_state = Arc::new(Mutex::new(CallbackState::default()));
    let before_state = Arc::clone(&callback_state);
    let before_project = project.clone();
    let before_owner = owner.clone();
    let before_turn = move |identity: &AppServerDispatch| {
        // This is the external-submit fence.  If the transaction is uncertain,
        // the driver returns unresolved evidence and this worker never retries.
        before_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .dispatch = Some(identity.clone());
        before_project.claim_app_server_dispatch(before_owner.clone(), identity.clone())?;
        Ok(())
    };
    let turn_state = Arc::clone(&callback_state);
    let turn_project = project.clone();
    let turn_owner = owner.clone();
    let on_turn = move |identity: &AppServerDispatch, turn_id: &str| {
        {
            let mut state = turn_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.dispatch = Some(identity.clone());
            state.turn_id = Some(turn_id.to_owned());
        }
        turn_project.acknowledge_app_server_turn(
            turn_owner.clone(),
            identity.clone(),
            turn_id.to_owned(),
        )?;
        Ok(())
    };

    let mut stream = match reservation.start(
        binding.clone(),
        input,
        thread_config,
        stop.clone(),
        before_turn,
        on_turn,
    ) {
        Ok(stream) => stream,
        Err(_) => {
            save_report(
                &project,
                &recovery,
                run,
                &binding,
                failed(CodexRunStatus::ProcessUnavailable),
                AppServerDelivery::not_sent(),
                AppServerDiagnostics::default(),
            );
            return;
        }
    };

    let output_limit = binding.output_limit().unwrap_or(64 * 1024);
    let mut observed = String::new();
    let mut pending = String::new();
    let mut last_flush = Instant::now();
    let mut chunk_index = 0_u64;
    let mut local_write_failed = false;
    let mut output_limited = false;

    loop {
        match stream.next_event(Duration::from_millis(100)) {
            Ok(None) => {
                if !pending.is_empty()
                    && !local_write_failed
                    && !stop.is_requested()
                    && last_flush.elapsed() >= DISCUSSION_FLUSH_INTERVAL
                {
                    if append_pending(&project, &owner, &mut run, &mut pending, &mut chunk_index)
                        .is_err()
                    {
                        local_write_failed = true;
                        stream.request_stop();
                        stop.request_stop();
                    }
                    last_flush = Instant::now();
                }
            }
            Ok(Some(AppServerStreamEvent::AssistantDelta(delta))) => {
                if observed.len().saturating_add(delta.len()) > output_limit {
                    output_limited = true;
                    stream.request_stop();
                    stop.request_stop();
                }
                let remaining = output_limit.saturating_sub(observed.len());
                let chunk = prefix(&delta, remaining);
                observed.push_str(chunk);
                pending.push_str(chunk);
                if !pending.is_empty()
                    && !local_write_failed
                    && !stop.is_requested()
                    && (pending.len() >= DISCUSSION_FLUSH_BYTES
                        || last_flush.elapsed() >= DISCUSSION_FLUSH_INTERVAL)
                {
                    if append_pending(&project, &owner, &mut run, &mut pending, &mut chunk_index)
                        .is_err()
                    {
                        local_write_failed = true;
                        stream.request_stop();
                        stop.request_stop();
                    }
                    last_flush = Instant::now();
                }
            }
            Ok(Some(AppServerStreamEvent::Finished(finished))) => {
                let mut result = finished.result;
                let failure = finished.failure;
                let local_failure = finished.local_failure;
                if !result.assistant_text.starts_with(&observed) {
                    result.status = CodexRunStatus::ProtocolFailure(CodexFailureCode::Protocol);
                    result.assistant_text = observed.clone();
                }
                if output_limited || result.assistant_text.len() > output_limit {
                    result.status = CodexRunStatus::OutputLimit;
                    result.assistant_text = prefix(&result.assistant_text, output_limit).into();
                }
                if !local_write_failed
                    && !stop.is_requested()
                    && !pending.is_empty()
                    && append_pending(&project, &owner, &mut run, &mut pending, &mut chunk_index)
                        .is_err()
                {
                    local_write_failed = true;
                }
                if local_write_failed {
                    // The provider result is retained for explicit local retry;
                    // no further external request can be made by recovery.
                    if result.status == CodexRunStatus::Stopped {
                        result.status =
                            CodexRunStatus::ProtocolFailure(CodexFailureCode::Incomplete);
                    }
                    recovery.retain(PendingSave {
                        run: run.clone(),
                        outcome: SaveOutcome::Provider(Box::new(app_server_report(
                            &run,
                            &binding,
                            result,
                            finished.delivery,
                            AppServerDiagnostics {
                                provider: failure.clone(),
                                local: local_failure,
                            },
                        ))),
                    });
                } else {
                    save_report(
                        &project,
                        &recovery,
                        run,
                        &binding,
                        result,
                        finished.delivery,
                        AppServerDiagnostics {
                            provider: failure,
                            local: local_failure,
                        },
                    );
                }
                return;
            }
            Err(_) => {
                stream.request_stop();
                let result = CodexRunResult {
                    status: CodexRunStatus::CleanupUnresolved,
                    assistant_text: observed,
                    usage: None,
                    confirmed_stdin_bytes: 0,
                    warning_count: 0,
                    cleanup_settled: false,
                };
                let delivery = uncertain_delivery(&callback_state);
                if local_write_failed {
                    recovery.retain(PendingSave {
                        run: run.clone(),
                        outcome: SaveOutcome::Provider(Box::new(app_server_report(
                            &run,
                            &binding,
                            result,
                            delivery,
                            AppServerDiagnostics::default(),
                        ))),
                    });
                } else {
                    save_report(
                        &project,
                        &recovery,
                        run,
                        &binding,
                        result,
                        delivery,
                        AppServerDiagnostics::default(),
                    );
                }
                return;
            }
        }
    }
}

fn append_pending(
    project: &ProjectSession,
    owner: &RunOwner,
    run: &mut DiscussionRun,
    pending: &mut String,
    chunk_index: &mut u64,
) -> CoreResult<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let updated = project.append_discussion_output(DiscussionOutputAppend {
        owner: owner.clone(),
        expected_sequence: run.sequence.clone(),
        event_id: format!("{}-app-server-part-{}", owner.run_id, *chunk_index),
        chunk: std::mem::take(pending),
    })?;
    *run = updated;
    *chunk_index = chunk_index.saturating_add(1);
    Ok(())
}

fn app_server_report(
    run: &DiscussionRun,
    binding: &ProviderBinding,
    result: CodexRunResult,
    delivery: AppServerDelivery,
    diagnostics: AppServerDiagnostics,
) -> ProviderTerminalReport {
    let (status, error) = status_detail(result.status);
    let error = match (diagnostics.provider.as_ref(), diagnostics.local) {
        (Some(provider), Some(local)) => Some(format!(
            "{} {}",
            provider.safe_detail(),
            local.safe_detail()
        )),
        (Some(provider), None) => Some(provider.safe_detail()),
        (None, Some(local)) => Some(local.safe_detail().to_owned()),
        (None, None) => error.map(str::to_owned),
    };
    ProviderTerminalReport {
        app_server: Some(delivery),
        owner: run.owner.clone(),
        expected_sequence: run.sequence.clone(),
        event_id: format!("{}-provider-finish", run.id),
        assistant_text: result.assistant_text,
        binding: binding.clone(),
        status,
        // stdin is a JSON-RPC request body, so the app-server accounting is
        // kept separate from the exec worker's confirmed stdin byte count.
        confirmed_stdin_bytes: "0".into(),
        usage: result.usage.map(provider_usage),
        cleanup: if result.cleanup_settled {
            ProviderCleanup::Settled
        } else {
            ProviderCleanup::Unresolved
        },
        error,
        effective_identity: None,
        reported_model: None,
        delivery: None,
    }
}

fn save_report(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
    binding: &ProviderBinding,
    result: CodexRunResult,
    delivery: AppServerDelivery,
    diagnostics: AppServerDiagnostics,
) {
    crate::live_discussion::save_report(
        project,
        recovery,
        run.clone(),
        app_server_report(&run, binding, result, delivery, diagnostics),
    );
}

fn save_failure(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
    detail: &'static str,
) {
    // A malformed app-server binding is rejected before this path in normal
    // operation. The saved binding remains authoritative for all valid calls;
    // this fallback only makes a corrupt queued record settle as a failure.
    let binding = run
        .provider_binding
        .clone()
        .unwrap_or_else(ProviderBinding::codex_luna);
    let result = failed(CodexRunStatus::ProcessUnavailable);
    let mut report = app_server_report(
        &run,
        &binding,
        result.clone(),
        AppServerDelivery::not_sent(),
        AppServerDiagnostics::default(),
    );
    report.error = Some(detail.into());
    crate::live_discussion::save_report(project, recovery, run, report);
}

pub fn worker_unavailable(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
) {
    save_failure(
        project,
        recovery,
        run,
        "The response worker could not start. No model request was sent.",
    );
}

fn status_detail(status: CodexRunStatus) -> (ProviderOutcomeStatus, Option<&'static str>) {
    match status {
        CodexRunStatus::Completed => (ProviderOutcomeStatus::Completed, None),
        CodexRunStatus::Stopped => (ProviderOutcomeStatus::Stopped, None),
        CodexRunStatus::TimedOut => (
            ProviderOutcomeStatus::TimedOut,
            Some("The request timed out before the response finished."),
        ),
        CodexRunStatus::OutputLimit => (
            ProviderOutcomeStatus::OutputLimit,
            Some("The response reached the app's output limit."),
        ),
        CodexRunStatus::ProtocolFailure(_) => (
            ProviderOutcomeStatus::Failed,
            Some("The provider returned an incomplete or unsupported response."),
        ),
        CodexRunStatus::ConsumerTooSlow => (
            ProviderOutcomeStatus::Failed,
            Some("The response stopped because progress could not be processed quickly enough."),
        ),
        CodexRunStatus::ProcessUnavailable => (
            ProviderOutcomeStatus::Failed,
            Some("Codex could not start this request. Check its connection in Settings."),
        ),
        CodexRunStatus::CleanupUnresolved => (
            ProviderOutcomeStatus::Failed,
            Some("Local app-server cleanup could not be confirmed."),
        ),
    }
}

fn provider_usage(usage: webnovel_core::providers::codex_exec::CodexUsage) -> ProviderUsage {
    ProviderUsage {
        input_tokens: usage.input_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        cache_write_input_tokens: usage.cache_write_input_tokens,
        output_tokens: usage.output_tokens,
        reasoning_output_tokens: usage.reasoning_output_tokens,
    }
}

fn failed(status: CodexRunStatus) -> CodexRunResult {
    CodexRunResult {
        status,
        assistant_text: String::new(),
        usage: None,
        confirmed_stdin_bytes: 0,
        warning_count: 0,
        cleanup_settled: true,
    }
}

fn prefix(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[derive(Default)]
struct CallbackState {
    dispatch: Option<AppServerDispatch>,
    turn_id: Option<String>,
}

fn uncertain_delivery(state: &Arc<Mutex<CallbackState>>) -> AppServerDelivery {
    let state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(dispatch) = state.dispatch.clone() else {
        return AppServerDelivery::not_sent();
    };
    AppServerDelivery {
        dispatch: Some(dispatch),
        submission: if state.turn_id.is_some() {
            AppServerSubmission::Acknowledged
        } else {
            AppServerSubmission::Uncertain
        },
        turn_id: state.turn_id.clone(),
        terminal: None,
        request_settled: false,
        connection: codex_app_server::AppServerConnectionSettlement::Unresolved,
    }
}

struct Registration {
    runtime: DesktopProviders,
    owner: RunOwner,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.runtime.release(&self.owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use webnovel_core::context::packet::MockContextBudget;
    use webnovel_core::projects::{CreateDocument, ProjectAccess};
    use webnovel_core::providers::cli::windows_process::{
        ChildLimits, CliInvocation, EnvironmentPolicy,
    };
    use webnovel_core::providers::codex_app_server::AppServerRuntimeIdentity;
    use webnovel_core::providers::codex_app_server::protocol::ThreadStartConfig;
    use webnovel_core::providers::codex_app_server::runtime::AppServerConnection;

    fn fixture_executable() -> PathBuf {
        let current = std::env::current_exe().expect("desktop test executable");
        let target_debug = current
            .parent()
            .and_then(|deps| deps.parent())
            .expect("desktop test executable under target/debug/deps");
        let fixture = target_debug.join("windows-process-fixture.exe");
        assert!(
            fixture.is_file(),
            "build the core integration target first so {} exists",
            fixture.display()
        );
        fixture
    }

    fn invocation(mode: &str) -> CliInvocation {
        CliInvocation {
            executable: fixture_executable(),
            arguments: vec![OsString::from("--codex-app-server"), OsString::from(mode)],
            cwd: std::env::current_dir().expect("test cwd"),
            environment: EnvironmentPolicy::Clear,
            packet: Vec::new(),
            limits: ChildLimits {
                overall: std::time::Duration::from_secs(60),
                stop_grace: std::time::Duration::from_millis(100),
                max_total_output_bytes: 1024 * 1024,
            },
        }
    }

    fn binding() -> ProviderBinding {
        ProviderBinding::codex_app_server_author_runtime(
            "gpt-6-astra",
            "low",
            Some("default"),
            "fixture-cli",
            &"a".repeat(64),
            &"b".repeat(64),
            AppServerRuntimeIdentity {
                account_sha256: "c".repeat(64),
                security_config_sha256: "d".repeat(64),
                restrictive_catalog_sha256: "e".repeat(64),
            },
        )
    }

    fn thread_config() -> ThreadStartConfig {
        ThreadStartConfig {
            cwd: Some(
                std::env::current_dir()
                    .expect("test cwd")
                    .to_string_lossy()
                    .into_owned(),
            ),
            base_instructions: None,
            developer_instructions: None,
            model: "gpt-6-astra".into(),
            reasoning_effort: Some("low".into()),
            service_tier: "default".into(),
        }
    }

    fn discussion_fixture(label: &str) -> (ProjectSession, ProjectAccess, DiscussionDispatch) {
        let path = std::env::temp_dir().join(format!(
            "wns-app-server-discussion-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let project =
            ProjectSession::create(path, "App-server discussion fixture").expect("create project");
        let access = project
            .documents().attach("app-server-discussion-test".into())
            .expect("attach");
        let document = project
            .documents().create(CreateDocument {
                access: access.clone(),
                operation_id: format!("create-{label}"),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: serde_json::json!({
                    "schemaVersion": 1,
                    "body": {
                        "type": "doc",
                        "content": [{
                            "type": "paragraph",
                            "attrs": {"id": "p1"},
                            "content": [{"type": "text", "text": "The ending stays."}]
                        }]
                    }
                }),
            })
            .expect("create chapter");
        let started = project
            .start_discussion(StartDiscussion {
                lookup: None,
                access: access.clone(),
                operation_id: format!("discussion-{label}"),
                expected: document.head,
                instruction: "Discuss the promise.".into(),
                intent: FeedbackIntent::Discuss,
                basis: None,
                scope: None,
                pinned_document_ids: vec![],
                safe_brief: None,
                budget: MockContextBudget::new("100000", "4096", "1024"),
                provider_binding: Some(binding()),
                previous_run_id: None,
            })
            .expect("start discussion");
        let dispatch = project
            .begin_discussion_run(DiscussionBegin {
                owner: started.run.owner,
            })
            .expect("claim discussion");
        (project, access, dispatch)
    }

    fn clean_project(project: ProjectSession) {
        let path = project.path.clone();
        drop(project);
        std::fs::remove_dir_all(path).expect("remove discussion fixture");
    }

    #[test]
    fn uncertain_delivery_keeps_a_claimed_dispatch_without_fabricating_not_sent() {
        let state = Arc::new(Mutex::new(CallbackState {
            dispatch: Some(AppServerDispatch {
                server_generation: "server-1".into(),
                thread_id: "thread-1".into(),
                rpc_id: "turn-1".into(),
                packet_hash: "a".repeat(64),
                request_hash: "b".repeat(64),
            }),
            turn_id: None,
        }));
        let delivery = uncertain_delivery(&state);
        assert_eq!(delivery.submission, AppServerSubmission::Uncertain);
        assert!(delivery.dispatch.is_some());
        assert!(!delivery.request_settled);
        assert!(delivery.validate().is_ok());
    }

    #[test]
    fn fixture_completion_persists_streamed_text_and_app_server_receipt() {
        let _fixture_guard = crate::app_server_test_lock::fixture_guard();
        let (project, access, dispatch) = discussion_fixture("complete");
        let recovery = DiscussionRecovery::default();
        let connection = AppServerConnection::start(invocation("complete"), ())
            .expect("start app-server fixture");
        let reservation = connection
            .try_reserve()
            .expect("reserve app-server request");

        run_live(
            project.clone(),
            recovery.clone(),
            DesktopProviders::default(),
            reservation,
            thread_config(),
            dispatch,
            StopSignal::new(),
        );

        let view = project
            .read_discussion(access.clone(), "chapter".into())
            .expect("read settled discussion");
        let run = &view.runs[0];
        assert_eq!(run.status, DiscussionRunStatus::Completed);
        assert_eq!(run.output_text, "Hello world");
        assert_eq!(view.messages.last().unwrap().content, "Hello world");
        let provider = run.provider_result.as_ref().expect("provider result");
        assert_eq!(provider.confirmed_stdin_bytes, "0");
        assert_eq!(provider.assistant_text, "Hello world");
        assert_eq!(provider.cleanup, ProviderCleanup::Settled);
        let delivery = provider.app_server.as_ref().expect("app-server receipt");
        assert_eq!(delivery.submission, AppServerSubmission::Acknowledged);
        assert_eq!(
            delivery.terminal,
            Some(codex_app_server::AppServerTerminal::Completed)
        );
        assert!(delivery.request_settled);
        assert_eq!(
            delivery.connection,
            codex_app_server::AppServerConnectionSettlement::Reusable
        );
        delivery.validate().expect("valid completion receipt");
        assert_eq!(recovery.pending_count(), 0);

        connection.shutdown().expect("shutdown app-server fixture");
        clean_project(project);
    }

    #[test]
    fn fixture_crash_persists_acknowledged_unresolved_result_without_local_replay() {
        let _fixture_guard = crate::app_server_test_lock::fixture_guard();
        let (project, access, dispatch) = discussion_fixture("crash");
        let run_id = dispatch.run.id.clone();
        let recovery = DiscussionRecovery::default();
        let connection =
            AppServerConnection::start(invocation("crash"), ()).expect("start app-server fixture");
        let reservation = connection
            .try_reserve()
            .expect("reserve app-server request");

        run_live(
            project.clone(),
            recovery.clone(),
            DesktopProviders::default(),
            reservation,
            thread_config(),
            dispatch,
            StopSignal::new(),
        );

        let view = project
            .read_discussion(access.clone(), "chapter".into())
            .expect("read crashed discussion");
        let run = view.runs.iter().find(|run| run.id == run_id).expect("run");
        assert_eq!(run.status, DiscussionRunStatus::Interrupted);
        assert_eq!(run.output_text, "Hello");
        let provider = run.provider_result.as_ref().expect("provider result");
        assert_eq!(provider.cleanup, ProviderCleanup::Unresolved);
        let delivery = provider.app_server.as_ref().expect("app-server receipt");
        assert_eq!(delivery.submission, AppServerSubmission::Acknowledged);
        assert!(delivery.turn_id.is_some());
        assert_eq!(delivery.terminal, None);
        assert!(!delivery.request_settled);
        assert_eq!(
            delivery.connection,
            codex_app_server::AppServerConnectionSettlement::Closed
        );
        delivery.validate().expect("valid unresolved receipt");
        assert_eq!(recovery.pending_count(), 0);

        // A local retry only reconciles retained writes; it never starts a
        // second provider request after the acknowledged crash.
        recovery
            .retry(&project, access, "chapter".into(), run_id)
            .expect("inactive run has no provider replay");
        connection
            .shutdown()
            .expect("shutdown crashed app-server fixture");
        clean_project(project);
    }
}
