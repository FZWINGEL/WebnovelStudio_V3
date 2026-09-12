//! One explicit API memory refresh, frozen before submission. Reconciliation
//! only settles the saved operation and never reloads credentials or resends it.
use crate::commands::library_commands::DesktopLibrary;
use crate::commands::memory_commands::{StartMemoryRequest, check_maintenance_choice};
use crate::memory_recovery::MemoryRecovery;
use crate::commands::project_commands::execute;
use crate::provider_runtime::DesktopProviders;
use webnovel_core::context::packet::{HTTP_MEMORY_MODEL_ID, ProviderBinding};
use webnovel_core::projects::discussions::{
    HttpDeliverySubmission, HttpProviderUsage, ProviderCleanup, ProviderDeliveryReceipt,
    ProviderOutcomeStatus,
};
use webnovel_core::projects::memory::{
    CompleteMemory, MemoryDispatch, MemoryJob, MemoryJobStatus, MemoryOwner, StartMemory,
};
use webnovel_core::projects::{CoreError, CoreResult, ProjectSession};
use webnovel_core::providers::adapter::{
    CancellationToken, HttpRequestStage, ProviderErrorKind, StreamEvent,
};
use webnovel_core::providers::credentials::{CredentialStore, WindowsCredentialStore};
use webnovel_core::providers::endpoints::EndpointProfile;
use webnovel_core::providers::http_request::prepare_request;
use webnovel_core::providers::openai_compatible::OpenAiCompatibleAdapter;

const OUTPUT_LIMIT: usize = 64 * 1024;

fn binding_for(profile: &EndpointProfile) -> ProviderBinding {
    ProviderBinding::http_memory(
        &profile.id,
        &profile.base_url,
        &profile.config_revision,
        true,
        if profile.json_mode {
            webnovel_core::context::packet::HttpResponseFormat::JsonObject
        } else {
            webnovel_core::context::packet::HttpResponseFormat::Text
        },
    )
}

/// Captures the endpoint and OS credential together with preference acceptance.
/// Existing operations take a separate path that cannot read a credential.
fn accept(
    request: StartMemoryRequest,
    project: &ProjectSession,
    library: &DesktopLibrary,
    store: &dyn CredentialStore,
) -> CoreResult<(MemoryJob, Option<OpenAiCompatibleAdapter>)> {
    let existing = project
        .list_memory(request.access.clone())?
        .jobs
        .into_iter()
        .find(|job| {
            job.operation_id == request.operation_id
                && job.owner.operation_namespace == request.access.operation_namespace
        });
    let library = library
        .0
        .lock()
        .map_err(|_| crate::commands::provider_commands::unavailable())?;
    let (binding, adapter) = if let Some(existing) = existing {
        let binding = existing.provider_binding.ok_or_else(|| {
            CoreError::new(
                "ProviderBindingMismatch",
                "This saved refresh used another provider.",
            )
        })?;
        if !binding.is_http_memory()
            || !crate::provider_bindings::binding_matches_choice(&binding, &request.model_selection)
        {
            return Err(CoreError::new(
                "ProviderBindingMismatch",
                "This saved refresh used another model or settings.",
            ));
        }
        (binding, None)
    } else {
        check_maintenance_choice(
            &request.model_selection,
            request.maintenance_revision.as_deref(),
            &library,
        )?;
        let profile = library
            .endpoint_profiles()?
            .profiles
            .into_iter()
            .find(|profile| profile.id == request.model_selection.provider_id)
            .ok_or_else(|| {
                CoreError::new(
                    "ProviderUnavailable",
                    "The story-memory API connection is unavailable. Check Settings.",
                )
            })?;
        if !profile.enabled
            || !(profile
                .manual_model_ids
                .iter()
                .chain(&profile.cached_model_ids)
                .any(|model| model == HTTP_MEMORY_MODEL_ID))
        {
            return Err(CoreError::new(
                "ProviderUnavailable",
                "Enable this API connection and add gpt-6-astra to its models. The service must support low reasoning.",
            ));
        }
        let adapter = crate::commands::endpoint_commands::adapter_for_profile(&profile, store)?;
        (binding_for(&profile), Some(adapter))
    };
    let job = project.start_memory(StartMemory {
        access: request.access,
        operation_id: request.operation_id,
        expected: request.expected,
        budget: request.budget,
        provider_binding: Some(binding),
    })?;
    Ok((job, adapter))
}

pub async fn start(
    request: StartMemoryRequest,
    project: ProjectSession,
    recovery: MemoryRecovery,
    library: DesktopLibrary,
    runtime: DesktopProviders,
) -> CoreResult<MemoryJob> {
    execute(move || {
        let _admission = runtime.admit_request()?;
        let (started, adapter) = accept(request, &project, &library, &WindowsCredentialStore)?;
        dispatch_accepted(started, adapter, project, recovery, runtime)
    })
    .await
}

fn dispatch_accepted(
    started: MemoryJob,
    adapter: Option<OpenAiCompatibleAdapter>,
    project: ProjectSession,
    recovery: MemoryRecovery,
    runtime: DesktopProviders,
) -> CoreResult<MemoryJob> {
    if recovery.claim_pending(&started.owner) {
        return recovery.retry(&project, started.owner);
    }
    if started.status != MemoryJobStatus::Queued {
        return Ok(started);
    }
    let stop = match runtime.register_http_memory(&started.owner) {
        Ok(stop) => stop,
        Err(error) if error.code == "RunAlreadyStarted" => {
            return project.read_memory_job(started.owner);
        }
        Err(error) => {
            recovery.worker_not_registered(&project, &started);
            return Err(error);
        }
    };
    let dispatch = match recovery.claim(&project, &started) {
        Ok(dispatch) => dispatch,
        Err(error) => {
            runtime.release_http_memory(&started.owner);
            return Err(error);
        }
    };
    if !dispatch.newly_dispatched {
        runtime.release_http_memory(&started.owner);
        return Ok(dispatch.job);
    }
    if let Some(adapter) = adapter {
        tauri::async_runtime::spawn(async move {
            let _registration = Registration {
                runtime,
                owner: dispatch.job.owner.clone(),
            };
            run_response(&project, &recovery, adapter, dispatch, &stop).await;
        });
        Ok(started)
    } else {
        // No original owner survived the lost acceptance acknowledgement.
        // A new owner may settle the unsent claim, but cannot issue its POST.
        save_unsent(
            &project,
            &recovery,
            &dispatch,
            ProviderOutcomeStatus::Failed,
            "This saved refresh was interrupted before dispatch. No API request was sent. Start a new refresh to try again.",
        );
        runtime.release_http_memory(&started.owner);
        project.read_memory_job(started.owner)
    }
}

struct Registration {
    runtime: DesktopProviders,
    owner: MemoryOwner,
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.runtime.release_http_memory(&self.owner);
    }
}

fn completion(
    dispatch: &MemoryDispatch,
    outcome: ProviderOutcomeStatus,
    raw_output: String,
    error: Option<String>,
    submission: HttpDeliverySubmission,
    usage: Option<HttpProviderUsage>,
) -> CoreResult<CompleteMemory> {
    let prepared = prepare_request(&dispatch.packet.messages, &dispatch.packet.options)?;
    Ok(CompleteMemory {
        owner: dispatch.job.owner.clone(),
        event_id: format!("{}-http-memory-finish", dispatch.job.id),
        raw_output,
        outcome,
        confirmed_stdin_bytes: None,
        usage: None,
        cleanup: Some(ProviderCleanup::Settled),
        error,
        effective_identity: None,
        delivery: Some(ProviderDeliveryReceipt {
            body_hash: prepared.body_hash,
            body_bytes: prepared.body_bytes,
            submission,
            usage,
        }),
        app_server: None,
    })
}

fn save_unsent(
    project: &ProjectSession,
    recovery: &MemoryRecovery,
    dispatch: &MemoryDispatch,
    outcome: ProviderOutcomeStatus,
    detail: &str,
) {
    if let Ok(result) = completion(
        dispatch,
        outcome,
        String::new(),
        Some(detail.into()),
        HttpDeliverySubmission::NotSent,
        None,
    ) {
        let _ = recovery.save_or_retain(project, result, dispatch.job.target.document_id.clone());
    } else {
        // A corrupt packet cannot form a valid delivery receipt. Keep a local
        // interrupted-claim reconciliation instead of inventing HTTP evidence.
        recovery.worker_not_registered(project, &dispatch.job);
    }
}

async fn run_response(
    project: &ProjectSession,
    recovery: &MemoryRecovery,
    adapter: OpenAiCompatibleAdapter,
    dispatch: MemoryDispatch,
    stop: &CancellationToken,
) {
    // Revalidate the current policy after worker scheduling, before submission.
    let current = match project.begin_memory(dispatch.job.owner.clone()) {
        Ok(current) => current.job,
        Err(_)
            if stop.is_cancelled()
                || project
                    .read_memory_job(dispatch.job.owner.clone())
                    .is_ok_and(|job| job.status == MemoryJobStatus::Stopping) =>
        {
            save_unsent(
                project,
                recovery,
                &dispatch,
                ProviderOutcomeStatus::Stopped,
                "The refresh was stopped before an API request was sent.",
            );
            return;
        }
        Err(_) => {
            save_unsent(
                project,
                recovery,
                &dispatch,
                ProviderOutcomeStatus::Failed,
                "The saved story-memory source or permissions could not be checked before submission.",
            );
            return;
        }
    };
    if current.status == MemoryJobStatus::Stopping {
        stop.cancel();
    }
    if !matches!(
        current.status,
        MemoryJobStatus::Running | MemoryJobStatus::Stopping
    ) {
        return;
    }
    if stop.is_cancelled() {
        save_unsent(
            project,
            recovery,
            &dispatch,
            ProviderOutcomeStatus::Stopped,
            "The refresh was stopped before an API request was sent.",
        );
        return;
    }
    let mut submission = HttpDeliverySubmission::NotSent;
    let mut observed = String::new();
    let mut output_limited = false;
    let response = adapter
        .stream_packet_async(
            &dispatch.packet.messages,
            &dispatch.packet.options,
            stop,
            &mut |event| {
                if let StreamEvent::ContentDelta(chunk) = event {
                    if output_limited || stop.is_cancelled() {
                        return;
                    }
                    let remaining = OUTPUT_LIMIT.saturating_sub(observed.len());
                    if chunk.len() > remaining {
                        output_limited = true;
                        stop.cancel();
                    }
                    observed.push_str(prefix(&chunk, remaining));
                }
            },
            &mut |stage| {
                submission = match stage {
                    HttpRequestStage::Submitted => HttpDeliverySubmission::Uncertain,
                    HttpRequestStage::ResponseReceived => HttpDeliverySubmission::ResponseReceived,
                };
            },
        )
        .await;
    let (mut outcome, mut text, mut error, usage) = match response {
        Ok(value) => (
            ProviderOutcomeStatus::Completed,
            value.text,
            None,
            (value.usage.input_tokens.is_some()
                || value.usage.output_tokens.is_some()
                || value.usage.total_tokens.is_some())
            .then_some(HttpProviderUsage {
                input_tokens: value.usage.input_tokens,
                output_tokens: value.usage.output_tokens,
                total_tokens: value.usage.total_tokens,
            }),
        ),
        Err(error) => (
            if error.kind == ProviderErrorKind::Cancelled {
                ProviderOutcomeStatus::Stopped
            } else {
                ProviderOutcomeStatus::Failed
            },
            error.partial_text,
            Some(error.detail),
            None,
        ),
    };
    if !text.starts_with(&observed) {
        text = observed;
        outcome = ProviderOutcomeStatus::Failed;
        error = Some("The API response did not agree with its validated partial text.".into());
    }
    if output_limited || text.len() > OUTPUT_LIMIT {
        text = prefix(&text, OUTPUT_LIMIT).into();
        outcome = ProviderOutcomeStatus::OutputLimit;
        error = Some("The memory refresh reached the app's retained response limit.".into());
    }
    if let Ok(result) = completion(&dispatch, outcome, text, error, submission, usage) {
        let _ = recovery.save_or_retain(project, result, dispatch.job.target.document_id.clone());
    } else {
        recovery.worker_not_registered(project, &dispatch.job);
    }
}

fn prefix(text: &str, limit: usize) -> &str {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};
    use webnovel_core::context::{memory::mock_navigation_digest, packet::MockContextBudget};
    use webnovel_core::library::Library;
    use webnovel_core::projects::{CreateDocument, ProjectAccess};
    use webnovel_core::providers::credentials::{CredentialTarget, SecretValue};
    use webnovel_core::providers::endpoints::EndpointProfileDraft;

    #[derive(Default)]
    struct SyntheticStore {
        reads: AtomicUsize,
    }
    impl CredentialStore for SyntheticStore {
        fn read(&self, _: &CredentialTarget) -> CoreResult<Option<SecretValue>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(Some(SecretValue::new(b"fixture-key-original".to_vec())?))
        }
        fn write_new(&self, _: &[u8]) -> CoreResult<CredentialTarget> {
            panic!("fixture cannot write OS credentials")
        }
        fn delete(&self, _: &CredentialTarget) -> CoreResult<()> {
            panic!("fixture cannot delete OS credentials")
        }
    }
    struct Fixture {
        root: std::path::PathBuf,
        project: ProjectSession,
        access: ProjectAccess,
        request: StartMemoryRequest,
        library: DesktopLibrary,
        store: SyntheticStore,
    }
    impl Fixture {
        fn new(url: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "wns-http-memory-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&root).unwrap();
            let project =
                ProjectSession::create(root.join("project"), "API memory fixture").unwrap();
            let access = project.documents().attach("fixture-session".into()).unwrap();
            let chapter = project.documents().create(CreateDocument {
                access: access.clone(), operation_id: "create-chapter".into(), document_id: "chapter".into(),
                title: "A promise".into(), kind: "chapter".into(),
                body: serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":"Mei promised to return the silver key."}]}]}}),
            }).unwrap();
            let mut library = Library::open(root.join("library")).unwrap();
            let mut draft = EndpointProfileDraft::new("Memory endpoint", url);
            draft.enabled = true;
            draft.manual_model_ids = vec![HTTP_MEMORY_MODEL_ID.into()];
            draft.credential_ref =
                Some("WebnovelStudioV3/Profile/00000000-0000-0000-0000-000000000001".into());
            library.save_endpoint_profiles("0", vec![draft]).unwrap();
            let profile = library.endpoint_profiles().unwrap().profiles.remove(0);
            library
                .save_story_memory_provider("0", &profile.id)
                .unwrap();
            let request = StartMemoryRequest {
                access: access.clone(),
                operation_id: "refresh".into(),
                expected: chapter.head,
                budget: MockContextBudget::new("200000", "1000", "1000"),
                model_selection: crate::provider_bindings::memory_selection(&profile.id),
                maintenance_revision: Some("1".into()),
            };
            Self {
                root,
                project,
                access,
                request,
                library: DesktopLibrary(Arc::new(Mutex::new(library))),
                store: SyntheticStore::default(),
            }
        }
        fn accept(&self) -> (MemoryJob, Option<OpenAiCompatibleAdapter>) {
            accept(
                self.request.clone(),
                &self.project,
                &self.library,
                &self.store,
            )
            .unwrap()
        }
        fn change_settings(&self) {
            let mut library = self.library.0.lock().unwrap();
            let profile = library.endpoint_profiles().unwrap().profiles.remove(0);
            library
                .save_endpoint_profiles(
                    "1",
                    vec![EndpointProfileDraft {
                        id: Some(profile.id),
                        label: "Different endpoint now".into(),
                        base_url: "http://127.0.0.1:1/v1".into(),
                        enabled: false,
                        json_mode: false,
                        credential_ref: None,
                        manual_model_ids: vec!["other-model".into()],
                    }],
                )
                .unwrap();
            library.save_story_memory_provider("1", "codex").unwrap();
        }
        fn finish(self) {
            let root = self.root.clone();
            drop(self);
            let canonical = std::fs::canonicalize(&root).unwrap();
            assert_eq!(
                canonical.parent(),
                Some(
                    std::fs::canonicalize(std::env::temp_dir())
                        .unwrap()
                        .as_path()
                )
            );
            assert!(
                canonical
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("wns-http-memory-")
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    fn server(
        listener: TcpListener,
        text: String,
        stop: Option<CancellationToken>,
    ) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                if let Ok((stream, _)) = listener.accept() {
                    break stream;
                }
                assert!(Instant::now() < deadline, "fixture request timed out");
                std::thread::sleep(Duration::from_millis(5));
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 8192];
            loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let len: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let delta = serde_json::json!({"choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]});
            let end = if stop.is_some() {
                String::new()
            } else {
                "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".into()
            };
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {delta}\n\n{end}").unwrap();
            stream.flush().unwrap();
            if let Some(stop) = stop {
                stop.cancel();
            }
            bytes
        })
    }

    #[test]
    fn api_only_memory_freezes_route_key_and_astra_settings_and_reconciles_without_resubmission() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
        let (job, adapter) = fixture.accept();
        let dispatch = fixture.project.begin_memory(job.owner.clone()).unwrap();
        let output =
            serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap()).unwrap();
        let server = server(listener, output, None);
        fixture.change_settings();
        let recovery = MemoryRecovery::default();
        tauri::async_runtime::block_on(run_response(
            &fixture.project,
            &recovery,
            adapter.unwrap(),
            dispatch,
            &CancellationToken::new(),
        ));
        let captured = server.join().unwrap();
        let end = captured.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
        let headers = String::from_utf8_lossy(&captured[..end]).to_ascii_lowercase();
        assert!(headers.starts_with("post /v1/chat/completions http/1.1"));
        assert!(headers.contains("authorization: bearer fixture-key-original"));
        let body: serde_json::Value = serde_json::from_slice(&captured[end + 4..]).unwrap();
        assert_eq!(body["model"], HTTP_MEMORY_MODEL_ID);
        assert_eq!(body["reasoning_effort"], "low");
        assert!(body.get("service_tier").is_none());
        let result = fixture.project.read_memory_job(job.owner.clone()).unwrap();
        assert_eq!(result.status, MemoryJobStatus::Completed);
        assert!(result.view.is_some());
        let receipt = result.result.as_ref().unwrap();
        assert!(receipt.confirmed_stdin_bytes.is_none());
        assert!(receipt.usage.is_none());
        assert!(receipt.delivery.as_ref().unwrap().usage.is_none());
        assert_eq!(
            receipt.delivery.as_ref().unwrap().submission,
            HttpDeliverySubmission::ResponseReceived
        );
        assert_eq!(
            receipt.delivery.as_ref().unwrap().body_bytes,
            (captured.len() - end - 4).to_string()
        );
        let (again, adapter) = fixture.accept();
        assert!(adapter.is_none());
        let replay = dispatch_accepted(
            again,
            adapter,
            fixture.project.clone(),
            recovery,
            DesktopProviders::default(),
        )
        .unwrap();
        assert_eq!(replay.id, job.id);
        assert_eq!(fixture.store.reads.load(Ordering::SeqCst), 1);
        assert_eq!(
            fixture
                .project
                .documents().read(fixture.access.clone(), "chapter".into())
                .unwrap()
                .head,
            fixture.request.expected
        );
        fixture.finish();
    }

    #[test]
    fn lost_acceptance_ack_keeps_active_owner_or_seals_unsent_without_reloading_credentials() {
        let fixture = Fixture::new("http://127.0.0.1:1/v1");
        let (job, original_adapter) = fixture.accept();
        drop(original_adapter);
        fixture.change_settings();
        let runtime = DesktopProviders::default();
        let recovery = MemoryRecovery::default();
        runtime.register_http_memory(&job.owner).unwrap();
        let (again, adapter) = fixture.accept();
        let active = dispatch_accepted(
            again,
            adapter,
            fixture.project.clone(),
            recovery.clone(),
            runtime.clone(),
        )
        .unwrap();
        assert_eq!(active.status, MemoryJobStatus::Queued);
        runtime.release_http_memory(&job.owner);
        let (again, adapter) = fixture.accept();
        let failed =
            dispatch_accepted(again, adapter, fixture.project.clone(), recovery, runtime).unwrap();
        assert_eq!(failed.status, MemoryJobStatus::Failed);
        assert_eq!(
            failed.result.unwrap().delivery.unwrap().submission,
            HttpDeliverySubmission::NotSent
        );
        assert_eq!(fixture.store.reads.load(Ordering::SeqCst), 1);
        fixture.finish();
    }

    #[test]
    fn stopped_http_result_with_failed_local_save_retries_only_the_retained_receipt() {
        let fixture = Fixture::new("http://127.0.0.1:1/v1");
        let (job, adapter) = fixture.accept();
        drop(adapter);
        let dispatch = fixture.project.begin_memory(job.owner.clone()).unwrap();
        let db = rusqlite::Connection::open(fixture.project.path.join("project.sqlite3")).unwrap();
        db.execute_batch("CREATE TRIGGER fail_memory_receipt BEFORE INSERT ON memory_results BEGIN SELECT RAISE(ABORT,'synthetic save failure'); END;").unwrap();
        let recovery = MemoryRecovery::default();
        let result = completion(
            &dispatch,
            ProviderOutcomeStatus::Stopped,
            "retained partial".into(),
            None,
            HttpDeliverySubmission::ResponseReceived,
            None,
        )
        .unwrap();
        assert!(
            recovery
                .save_or_retain(&fixture.project, result, "chapter".into())
                .is_err()
        );
        assert_eq!(
            recovery.pending_job_ids(
                &job.owner.project_id,
                &job.owner.operation_namespace,
                "chapter"
            ),
            vec![job.id.clone()]
        );
        db.execute_batch("DROP TRIGGER fail_memory_receipt;")
            .unwrap();
        drop(db);
        let saved = recovery.retry(&fixture.project, job.owner.clone()).unwrap();
        assert_eq!(saved.status, MemoryJobStatus::Stopped);
        assert_eq!(
            saved.result.unwrap().raw_output.as_deref(),
            Some("retained partial")
        );
        assert_eq!(
            fixture
                .project
                .list_memory(fixture.access.clone())
                .unwrap()
                .jobs
                .len(),
            1
        );
        assert_eq!(fixture.store.reads.load(Ordering::SeqCst), 1);
        fixture.finish();
    }

    #[test]
    fn new_refresh_requires_exact_maintenance_revision_and_settings_before_credential_read() {
        let fixture = Fixture::new("http://127.0.0.1:1/v1");
        for mutated in 0..3 {
            let mut request = fixture.request.clone();
            match mutated {
                0 => request.maintenance_revision = None,
                1 => request.maintenance_revision = Some("0".into()),
                _ => request.model_selection.reasoning = Some("high".into()),
            }
            assert_eq!(
                accept(request, &fixture.project, &fixture.library, &fixture.store)
                    .err()
                    .unwrap()
                    .code,
                "ModelChoiceChanged"
            );
        }
        assert_eq!(fixture.store.reads.load(Ordering::SeqCst), 0);
        assert!(
            fixture
                .project
                .list_memory(fixture.access.clone())
                .unwrap()
                .jobs
                .is_empty()
        );
        fixture.finish();
    }

    #[test]
    fn output_limit_preserves_a_bounded_utf8_prefix_and_does_not_install_memory() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
        let (job, adapter) = fixture.accept();
        let dispatch = fixture.project.begin_memory(job.owner.clone()).unwrap();
        let server = server(listener, "é".repeat(OUTPUT_LIMIT), None);
        tauri::async_runtime::block_on(run_response(
            &fixture.project,
            &MemoryRecovery::default(),
            adapter.unwrap(),
            dispatch,
            &CancellationToken::new(),
        ));
        server.join().unwrap();
        let saved = fixture.project.read_memory_job(job.owner).unwrap();
        assert_eq!(
            saved.result.as_ref().unwrap().outcome,
            ProviderOutcomeStatus::OutputLimit
        );
        assert_eq!(
            saved.result.unwrap().raw_output.unwrap().len(),
            OUTPUT_LIMIT
        );
        assert!(saved.view.is_none());
        fixture.finish();
    }

    #[test]
    fn stop_before_submission_records_not_sent_without_contacting_the_endpoint() {
        let fixture = Fixture::new("http://127.0.0.1:1/v1");
        let (job, adapter) = fixture.accept();
        let dispatch = fixture.project.begin_memory(job.owner.clone()).unwrap();
        let runtime = DesktopProviders::default();
        let stop = runtime.register_http_memory(&job.owner).unwrap();
        fixture
            .project
            .stop_memory(fixture.access.clone(), job.id)
            .unwrap();
        runtime.stop_memory(&job.owner);
        assert!(stop.is_cancelled());
        tauri::async_runtime::block_on(run_response(
            &fixture.project,
            &MemoryRecovery::default(),
            adapter.unwrap(),
            dispatch,
            &stop,
        ));
        runtime.release_http_memory(&job.owner);
        let saved = fixture.project.read_memory_job(job.owner).unwrap();
        assert_eq!(saved.status, MemoryJobStatus::Stopped);
        assert_eq!(
            saved.result.as_ref().unwrap().outcome,
            ProviderOutcomeStatus::Stopped
        );
        assert!(
            saved
                .result
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("stopped before")
        );
        assert_eq!(
            saved.result.unwrap().delivery.unwrap().submission,
            HttpDeliverySubmission::NotSent
        );
        fixture.finish();
    }
}
