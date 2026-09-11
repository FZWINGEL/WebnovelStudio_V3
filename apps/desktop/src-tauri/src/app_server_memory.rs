//! Story-memory worker for the persistent Codex app-server transport.
//!
//! Memory refreshes have no streamed document writes.  The worker retains one
//! bounded terminal completion and lets `MemoryRecovery` settle or retry that
//! local write without ever replaying the provider request.
#![cfg(windows)]

use crate::memory_recovery::MemoryRecovery;
use crate::provider_runtime::DesktopProviders;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use webnovel_core::context::packet::{packet_input_hash, serialized_input};
use webnovel_core::projects::ProjectSession;
use webnovel_core::projects::memory::{CompleteMemory, MemoryDispatch, MemoryJobStatus};
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

/// Runs one newly claimed app-server memory refresh.  A replayed running job
/// (`newly_dispatched == false`) exits without starting another external turn.
pub fn run_live(
    project: ProjectSession,
    recovery: MemoryRecovery,
    runtime: DesktopProviders,
    reservation: AppServerReservation,
    thread_config: ThreadStartConfig,
    dispatch: MemoryDispatch,
    stop: StopSignal,
) {
    let owner = dispatch.job.owner.clone();
    let document_id = dispatch.job.target.document_id.clone();
    let _registration = Registration {
        runtime,
        owner: owner.clone(),
    };

    // Core deliberately returns a running dispatch for recovery reads.  That
    // result is never permission to send the frozen packet again.
    if !dispatch.newly_dispatched {
        return;
    }

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
                &owner,
                &document_id,
                "The refresh's saved app-server model settings could not be validated.",
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
                &owner,
                &document_id,
                "The exact saved refresh request could not be validated.",
            );
            return;
        }
    };

    let current = match project.read_memory_job(owner.clone()) {
        Ok(current) => current,
        Err(_) => {
            save_failure(
                &project,
                &recovery,
                &owner,
                &document_id,
                "The refresh's saved state could not be checked before starting.",
            );
            return;
        }
    };
    if current.status == MemoryJobStatus::Stopping {
        stop.request_stop();
    }
    if !matches!(
        current.status,
        MemoryJobStatus::Running | MemoryJobStatus::Stopping
    ) {
        return;
    }
    if stop.is_requested() {
        save_completion(
            &project,
            &recovery,
            completion(&owner, stopped(), AppServerDelivery::not_sent(), None, None),
            document_id,
        );
        return;
    }

    // Revalidate the frozen source policy immediately before app-server claim.
    // The returned running dispatch is intentionally ignored: this worker only
    // uses the original `newly_dispatched` claim to authorize one turn.
    if project.begin_memory(owner.clone()).is_err() {
        save_failure(
            &project,
            &recovery,
            &owner,
            &document_id,
            "Story-context permissions changed before this refresh could start.",
        );
        return;
    }

    let callback_state = Arc::new(Mutex::new(CallbackState::default()));
    let before_state = Arc::clone(&callback_state);
    let before_project = project.clone();
    let before_owner = owner.clone();
    let before_turn = move |identity: &AppServerDispatch| {
        before_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .dispatch = Some(identity.clone());
        before_project.claim_memory_app_server_dispatch(before_owner.clone(), identity.clone())?;
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
        turn_project.acknowledge_memory_app_server_turn(
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
            save_completion(
                &project,
                &recovery,
                completion(
                    &owner,
                    failed(CodexRunStatus::ProcessUnavailable),
                    AppServerDelivery::not_sent(),
                    None,
                    None,
                ),
                document_id,
            );
            return;
        }
    };

    let output_limit = binding.output_limit().unwrap_or(64 * 1024);
    let mut observed = String::new();
    let mut output_limited = false;
    loop {
        match stream.next_event(Duration::from_millis(100)) {
            Ok(None) => {}
            Ok(Some(AppServerStreamEvent::AssistantDelta(delta))) => {
                if observed.len().saturating_add(delta.len()) > output_limit {
                    output_limited = true;
                    stream.request_stop();
                    stop.request_stop();
                }
                let remaining = output_limit.saturating_sub(observed.len());
                observed.push_str(prefix(&delta, remaining));
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
                save_completion(
                    &project,
                    &recovery,
                    completion(&owner, result, finished.delivery, failure, local_failure),
                    document_id,
                );
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
                save_completion(
                    &project,
                    &recovery,
                    completion(
                        &owner,
                        result,
                        uncertain_delivery(&callback_state),
                        None,
                        None,
                    ),
                    document_id,
                );
                return;
            }
        }
    }
}

fn completion(
    owner: &webnovel_core::projects::memory::MemoryOwner,
    result: CodexRunResult,
    delivery: AppServerDelivery,
    failure: Option<TurnFailure>,
    local_failure: Option<AppServerLocalFailure>,
) -> CompleteMemory {
    let mut completion = crate::live_memory::report(owner, result);
    // App-server requests use JSON-RPC accounting rather than exec stdin.
    completion.confirmed_stdin_bytes = None;
    completion.app_server = Some(delivery);
    if let Some(failure) = failure {
        completion.error = Some(failure.safe_detail());
    }
    if let Some(local_failure) = local_failure {
        let local_detail = local_failure.safe_detail().to_owned();
        completion.error = Some(match completion.error.take() {
            Some(provider_detail) => format!("{provider_detail} {local_detail}"),
            None => local_detail,
        });
    }
    completion
}

fn save_completion(
    project: &ProjectSession,
    recovery: &MemoryRecovery,
    completion: CompleteMemory,
    document_id: String,
) {
    let _ = recovery.save_or_retain(project, completion, document_id);
}

fn save_failure(
    project: &ProjectSession,
    recovery: &MemoryRecovery,
    owner: &webnovel_core::projects::memory::MemoryOwner,
    document_id: &str,
    detail: &'static str,
) {
    let mut completion = completion(
        owner,
        failed(CodexRunStatus::ProcessUnavailable),
        AppServerDelivery::not_sent(),
        None,
        None,
    );
    completion.error = Some(detail.to_owned());
    save_completion(project, recovery, completion, document_id.to_owned());
}

pub fn worker_unavailable(
    project: &ProjectSession,
    recovery: &MemoryRecovery,
    owner: webnovel_core::projects::memory::MemoryOwner,
    document_id: String,
    detail: &'static str,
) {
    save_failure(project, recovery, &owner, &document_id, detail);
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

fn stopped() -> CodexRunResult {
    failed(CodexRunStatus::Stopped)
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
    owner: webnovel_core::projects::memory::MemoryOwner,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.runtime.release_memory(&self.owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use webnovel_core::context::packet::MockContextBudget;
    use webnovel_core::projects::memory::StartMemory;
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
                overall: Duration::from_secs(60),
                stop_grace: Duration::from_millis(100),
                max_total_output_bytes: 1024 * 1024,
            },
        }
    }

    fn binding() -> webnovel_core::context::packet::ProviderBinding {
        webnovel_core::context::packet::ProviderBinding::codex_app_server_maintenance_runtime(
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
            service_tier: "priority".into(),
        }
    }

    fn memory_fixture(label: &str) -> (ProjectSession, ProjectAccess, MemoryDispatch) {
        let path = std::env::temp_dir().join(format!(
            "wns-app-server-memory-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let project =
            ProjectSession::create(path, "App-server memory fixture").expect("create project");
        let access = project
            .documents().attach("app-server-memory-test".into())
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
                            "content": [{"type": "text", "text": "Mei walks toward the gate."}]
                        }]
                    }
                }),
            })
            .expect("create chapter");
        let queued = project
            .start_memory(StartMemory {
                access: access.clone(),
                operation_id: format!("memory-{label}"),
                expected: document.head,
                budget: MockContextBudget::new("100000", "4096", "1024"),
                provider_binding: Some(binding()),
            })
            .expect("start memory");
        let dispatch = project.begin_memory(queued.owner).expect("claim memory");
        (project, access, dispatch)
    }

    fn clean_project(project: ProjectSession) {
        let path = project.path.clone();
        drop(project);
        std::fs::remove_dir_all(path).expect("remove memory fixture");
    }

    #[test]
    fn memory_app_server_completion_omits_exec_stdin_bytes() {
        let owner = webnovel_core::projects::memory::MemoryOwner {
            project_id: "project".into(),
            operation_namespace: "session".into(),
            job_id: "job".into(),
        };
        let value = completion(
            &owner,
            failed(CodexRunStatus::ProcessUnavailable),
            AppServerDelivery::not_sent(),
            None,
            None,
        );
        assert!(value.confirmed_stdin_bytes.is_none());
        assert!(value.app_server.is_some());
    }

    #[test]
    fn memory_failure_receipt_keeps_allowlisted_provider_code_and_status() {
        let owner = webnovel_core::projects::memory::MemoryOwner {
            project_id: "project".into(),
            operation_namespace: "session".into(),
            job_id: "job".into(),
        };
        let value = completion(
            &owner,
            failed(CodexRunStatus::ProcessUnavailable),
            AppServerDelivery::not_sent(),
            Some(TurnFailure {
                code: "rateLimitExceeded".into(),
                http_status_code: Some(429),
            }),
            None,
        );
        assert_eq!(
            value.error.as_deref(),
            Some("Codex provider failure: rateLimitExceeded (HTTP 429)")
        );
    }

    #[test]
    fn uncertain_memory_delivery_preserves_known_dispatch() {
        let state = Arc::new(Mutex::new(CallbackState {
            dispatch: Some(AppServerDispatch {
                server_generation: "server-1".into(),
                thread_id: "thread-1".into(),
                rpc_id: "turn-1".into(),
                packet_hash: "a".repeat(64),
                request_hash: "b".repeat(64),
            }),
            turn_id: Some("turn-ack".into()),
        }));
        let delivery = uncertain_delivery(&state);
        assert_eq!(delivery.submission, AppServerSubmission::Acknowledged);
        assert!(delivery.dispatch.is_some());
        assert!(delivery.validate().is_ok());
    }

    #[test]
    fn fixture_invalid_digest_is_durable_and_does_not_leave_a_local_retry() {
        let _fixture_guard = crate::app_server_test_lock::fixture_guard();
        let (project, access, dispatch) = memory_fixture("invalid-digest");
        let owner = dispatch.job.owner.clone();
        let recovery = MemoryRecovery::default();
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
            .read_memory(access, "chapter".into())
            .expect("read settled memory");
        let job = view
            .jobs
            .iter()
            .find(|job| job.id == owner.job_id)
            .expect("memory job");
        assert_eq!(job.status, MemoryJobStatus::Failed);
        let result = job.result.as_ref().expect("memory result");
        assert_eq!(result.raw_output.as_deref(), Some("Hello world"));
        assert!(result.validation_error.is_some());
        assert!(result.candidate.is_none());
        let delivery = result.app_server.as_ref().expect("app-server receipt");
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
        delivery.validate().expect("valid memory receipt");
        assert_eq!(recovery.pending_count(), 0);

        connection.shutdown().expect("shutdown app-server fixture");
        clean_project(project);
    }
}
