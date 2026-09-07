//! One frozen OpenAI-compatible request. Only local outcome saves can be retried.
use crate::discussion_recovery::{DiscussionRecovery, PendingSave, SaveOutcome};
use crate::library_commands::DesktopLibrary;
use crate::project_commands::execute;
use crate::provider_runtime::DesktopProviders;
use webnovel_core::context::packet::{
    HTTP_INPUT_LIMIT_BYTES, HTTP_OUTPUT_LIMIT_BYTES, HTTP_PROFILE_VERSION,
    HTTP_TOKEN_ACCOUNTING_METHOD, HttpProviderBinding, HttpResponseFormat, ProviderBinding,
};
use webnovel_core::projects::discussions::*;
use webnovel_core::projects::workshop_generation::StartWorkshop;
use webnovel_core::projects::{CoreError, CoreResult, ProjectSession};
use webnovel_core::providers::adapter::{
    CancellationToken, HttpRequestStage, ProviderErrorKind, StreamEvent,
};
use webnovel_core::providers::credentials::WindowsCredentialStore;
use webnovel_core::providers::endpoints::EndpointProfile;
use webnovel_core::providers::http_request::prepare_request;
use webnovel_core::providers::openai_compatible::OpenAiCompatibleAdapter;
use webnovel_core::providers::preferences::ModelSelection;

fn binding_for(
    profile: &EndpointProfile,
    choice: &ModelSelection,
    intent: FeedbackIntent,
) -> ProviderBinding {
    ProviderBinding {
        provider_id: profile.id.clone(),
        model_id: choice.model_id.clone(),
        reasoning: choice.reasoning.clone(),
        service_tier: choice.service_tier.clone(),
        profile_version: HTTP_PROFILE_VERSION.into(),
        input_limit_bytes: HTTP_INPUT_LIMIT_BYTES.to_string(),
        reserved_output_bytes: "0".into(),
        reserved_protocol_bytes: "0".into(),
        output_limit_bytes: HTTP_OUTPUT_LIMIT_BYTES.to_string(),
        accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.into(),
        runtime: None,
        http: Some(HttpProviderBinding {
            base_url: profile.base_url.clone(),
            config_revision: profile.config_revision.clone(),
            stream: true,
            response_format: if profile.json_mode && intent != FeedbackIntent::Discuss {
                HttpResponseFormat::JsonObject
            } else {
                HttpResponseFormat::Text
            },
        }),
    }
}

pub async fn start(
    mut request: StartDiscussion,
    selected: ModelSelection,
    project: ProjectSession,
    recovery: DiscussionRecovery,
    library: DesktopLibrary,
    runtime: DesktopProviders,
) -> CoreResult<DiscussionStart> {
    execute(move || {
        let _admission = runtime.admit_request()?;
        let existing = crate::discussion_commands::saved_request(&project, &request)?;
        let (started, adapter) = {
            // Keep the saved choice, endpoint configuration and acceptance in
            // one local critical section. The worker captures its own client/key.
            let library = library.0.lock().map_err(|_| crate::provider_commands::unavailable())?;
            if let Some(existing) = &existing {
                let binding = existing.provider_binding.as_ref().ok_or_else(|| CoreError::new("ProviderBindingMismatch", "This saved request used another provider."))?;
                if !binding.is_http() || binding.provider_id != selected.provider_id || binding.model_id != selected.model_id {
                    return Err(CoreError::new("ProviderBindingMismatch", "This saved request used another model."));
                }
                request.provider_binding = Some(binding.clone());
                // Receipt reconciliation never reloads a key, changes a route,
                // or starts another network invocation.
                (project.start_discussion(request)?, None)
            } else {
                let state = library.provider_state()?;
                if state.settings.active != selected {
                    return Err(CoreError::new("ModelChoiceChanged", "The selected model changed. Check the model picker and send again."));
                }
                if request.lookup.is_some() {
                    return Err(CoreError::new("UnsupportedProviderFeature", "Story lookups currently use Codex. Turn off lookups to send one response through this API connection."));
                }
                let profile = library.endpoint_profiles()?.profiles.into_iter().find(|profile| profile.id == selected.provider_id).ok_or_else(|| CoreError::new("ProviderUnavailable", "The API connection is unavailable. Check Settings."))?;
                if !profile.enabled || !(profile.manual_model_ids.contains(&selected.model_id) || profile.cached_model_ids.contains(&selected.model_id))
                    || selected.reasoning.is_some() || selected.service_tier.is_some()
                { return Err(CoreError::new("ProviderUnavailable", "This API model or its selected options are unavailable. Check Settings.")); }
                let adapter = crate::endpoint_commands::adapter_for_profile(&profile, &WindowsCredentialStore)?;
                request.provider_binding = Some(binding_for(&profile, &selected, request.intent));
                (project.start_discussion(request)?, Some(adapter))
            }
        };
        if started.run.status != DiscussionRunStatus::Queued { return Ok(started); }
        let stop = match runtime.register_http(&started.run.owner) {
            Ok(stop) => stop,
            Err(error) if error.code == "RunAlreadyStarted" => return Ok(started),
            Err(error) => return Err(error),
        };
        if let Some(dispatch) = recovery.claim(&project, &started.run) {
            if let Some(adapter) = adapter {
                tauri::async_runtime::spawn(run(project, recovery, runtime, adapter, dispatch, stop));
            } else {
                // An acceptance acknowledgment can be lost before the original
                // worker registers. Own the durable claim before sealing this
                // unsent operation; never reload a possibly rotated credential
                // or leave a workerless queued operation indefinitely active.
                finish_unstarted(&project, &recovery, dispatch);
                runtime.release_http(&started.run.owner);
            }
        } else { runtime.release_http(&started.run.owner); }
        Ok(started)
    }).await
}

/// OpenAI-compatible Workshop requests use the same streaming and terminal
/// receipt path as ordinary discussions. The actor resolves the Workshop
/// session and freezes its request before this function claims the queued run.
pub async fn start_workshop(
    mut request: StartWorkshop,
    selected: ModelSelection,
    project: ProjectSession,
    recovery: DiscussionRecovery,
    library: DesktopLibrary,
    runtime: DesktopProviders,
) -> CoreResult<DiscussionStart> {
    execute(move || {
        let _admission = runtime.admit_request()?;
        let saved = crate::discussion_commands::saved_workshop_request(&project, &request)?;
        let (started, adapter) = if let Some(saved) = &saved {
            // Resolve saved retries through the durable receipt before reading
            // endpoint settings. A terminal receipt therefore works offline;
            // a queued receipt is claimed below without replaying an API call.
            request.provider_binding = saved.provider_binding.clone();
            (project.start_workshop(request)?, None)
        } else {
            let library = library
                .0
                .lock()
                .map_err(|_| crate::provider_commands::unavailable())?;
                let state = library.provider_state()?;
                if state.settings.active != selected {
                    return Err(CoreError::new(
                        "ModelChoiceChanged",
                        "The selected model changed before this workshop started. Check the model selector and send again.",
                    ));
                }
                let profile = library
                    .endpoint_profiles()?
                    .profiles
                    .into_iter()
                    .find(|profile| profile.id == selected.provider_id)
                    .ok_or_else(|| {
                        CoreError::new(
                            "ProviderUnavailable",
                            "The API connection is unavailable. Check Settings.",
                        )
                    })?;
                if !profile.enabled
                    || !(profile.manual_model_ids.contains(&selected.model_id)
                        || profile.cached_model_ids.contains(&selected.model_id))
                    || selected.reasoning.is_some()
                    || selected.service_tier.is_some()
                {
                    return Err(CoreError::new(
                        "ProviderUnavailable",
                        "This API model or its selected options are unavailable. Check Settings.",
                    ));
                }
                let adapter = crate::endpoint_commands::adapter_for_profile(
                    &profile,
                    &WindowsCredentialStore,
                )?;
                request.provider_binding = Some(binding_for(
                    &profile,
                    &selected,
                    FeedbackIntent::WorkshopExplore,
                ));
                (project.start_workshop(request)?, Some(adapter))
        };
        if started.run.status != DiscussionRunStatus::Queued {
            return Ok(started);
        }
        let stop = match runtime.register_http(&started.run.owner) {
            Ok(stop) => stop,
            Err(error) if error.code == "RunAlreadyStarted" => return Ok(started),
            Err(error) => return Err(error),
        };
        if let Some(dispatch) = recovery.claim(&project, &started.run) {
            if let Some(adapter) = adapter {
                tauri::async_runtime::spawn(run(
                    project,
                    recovery,
                    runtime,
                    adapter,
                    dispatch,
                    stop,
                ));
            } else {
                finish_unstarted(&project, &recovery, dispatch);
                runtime.release_http(&started.run.owner);
            }
        } else {
            runtime.release_http(&started.run.owner);
        }
        Ok(started)
    })
    .await
}

fn finish_unstarted(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    dispatch: DiscussionDispatch,
) {
    let prepared = match prepare_request(&dispatch.packet.messages, &dispatch.packet.options) {
        Ok(prepared) => prepared,
        Err(_) => {
            let pending = PendingSave {
                run: dispatch.run.clone(),
                outcome: SaveOutcome::Fail,
            };
            if pending.attempt(project, &dispatch.run).is_err() {
                recovery.retain(pending);
            }
            return;
        }
    };
    let report = ProviderTerminalReport {
        owner: dispatch.run.owner.clone(), expected_sequence: dispatch.run.sequence.clone(),
        event_id: format!("{}-http-not-started", dispatch.run.id), assistant_text: String::new(),
        binding: dispatch.packet.options.provider_binding.expect("claimed HTTP binding"),
        status: ProviderOutcomeStatus::Failed, confirmed_stdin_bytes: "0".into(), usage: None,
        cleanup: ProviderCleanup::Settled, effective_identity: None,
        reported_model: None,
        error: Some("This saved request was interrupted before dispatch. No API request was sent. You can send a new request to try again.".into()),
        delivery: Some(ProviderDeliveryReceipt {
            body_hash: prepared.body_hash, body_bytes: prepared.body_bytes,
            submission: HttpDeliverySubmission::NotSent, usage: None,
        }),
    };
    save_report(project, recovery, dispatch.run, report);
}

async fn run(
    project: ProjectSession,
    recovery: DiscussionRecovery,
    runtime: DesktopProviders,
    adapter: OpenAiCompatibleAdapter,
    dispatch: DiscussionDispatch,
    stop: CancellationToken,
) {
    let owner = dispatch.run.owner.clone();
    run_response(&project, &recovery, adapter, dispatch, &stop).await;
    runtime.release_http(&owner);
}

async fn run_response(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    adapter: OpenAiCompatibleAdapter,
    dispatch: DiscussionDispatch,
    stop: &CancellationToken,
) {
    let mut run = dispatch.run;
    let binding = dispatch
        .packet
        .options
        .provider_binding
        .clone()
        .expect("HTTP claim retains a provider binding");
    let prepared = match prepare_request(&dispatch.packet.messages, &dispatch.packet.options) {
        Ok(prepared) => prepared,
        Err(_) => {
            let pending = PendingSave {
                run: run.clone(),
                outcome: SaveOutcome::Fail,
            };
            if pending.attempt(project, &run).is_err() {
                recovery.retain(pending);
            }
            return;
        }
    };
    let mut submission = HttpDeliverySubmission::NotSent;
    let mut local_error = None;
    let mut output_limit = false;
    let response = adapter
        .stream_packet_async(
            &dispatch.packet,
            stop,
            &mut |event| {
                if let StreamEvent::ContentDelta(chunk) = event {
                    if local_error.is_some() || output_limit || stop.is_cancelled() {
                        return;
                    }
                    if run.output_text.len().saturating_add(chunk.len()) > HTTP_OUTPUT_LIMIT_BYTES {
                        output_limit = true;
                        stop.cancel();
                        return;
                    }
                    match project.append_discussion_output(DiscussionOutputAppend {
                        owner: run.owner.clone(),
                        expected_sequence: run.sequence.clone(),
                        event_id: format!("{}-http-part-{}", run.id, run.sequence),
                        chunk,
                    }) {
                        Ok(updated) => run = updated,
                        Err(error) => {
                            local_error = Some(error);
                            stop.cancel();
                        }
                    }
                }
            },
            &mut |stage| {
                submission = match stage {
                    HttpRequestStage::Submitted => HttpDeliverySubmission::Uncertain,
                    HttpRequestStage::ResponseReceived => HttpDeliverySubmission::ResponseReceived,
                }
            },
        )
        .await;
    let (status, text, error, usage) = match response {
        Ok(value) => (
            ProviderOutcomeStatus::Completed,
            value.text,
            None,
            Some(value.usage),
        ),
        Err(error) => {
            let status = if output_limit {
                ProviderOutcomeStatus::OutputLimit
            } else if error.kind == ProviderErrorKind::Cancelled && local_error.is_none() {
                ProviderOutcomeStatus::Stopped
            } else {
                ProviderOutcomeStatus::Failed
            };
            let detail = if output_limit {
                "The response reached the app's output limit.".to_owned()
            } else if local_error.is_some() {
                "The response stopped because local progress could not be saved. Its retained output is available.".to_owned()
            } else {
                error.detail
            };
            (
                status,
                bounded_output(error.partial_text, &run.output_text),
                Some(detail),
                None,
            )
        }
    };
    let mut report = ProviderTerminalReport {
        owner: run.owner.clone(),
        expected_sequence: run.sequence.clone(),
        event_id: format!("{}-http-finish", run.id),
        assistant_text: text,
        binding,
        status,
        confirmed_stdin_bytes: "0".into(),
        usage: None,
        cleanup: ProviderCleanup::Settled,
        error,
        effective_identity: None,
        reported_model: None,
        delivery: Some(ProviderDeliveryReceipt {
            body_hash: prepared.body_hash,
            body_bytes: prepared.body_bytes,
            submission,
            usage: usage.map(|u| HttpProviderUsage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                total_tokens: u.total_tokens,
            }),
        }),
    };
    if local_error
        .as_ref()
        .is_some_and(|error| error.code == "RunStopping")
    {
        run.status = DiscussionRunStatus::Stopping;
        report.status = ProviderOutcomeStatus::Stopped;
    }
    save_report(project, recovery, run, report);
}

fn bounded_output(mut text: String, saved: &str) -> String {
    if text.len() > HTTP_OUTPUT_LIMIT_BYTES {
        let mut end = HTTP_OUTPUT_LIMIT_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    if text.starts_with(saved) {
        text
    } else {
        saved.to_owned()
    }
}

fn save_report(
    project: &ProjectSession,
    recovery: &DiscussionRecovery,
    run: DiscussionRun,
    report: ProviderTerminalReport,
) {
    let pending = PendingSave {
        run: run.clone(),
        outcome: SaveOutcome::Provider(Box::new(report)),
    };
    if let Err(error) = pending.attempt(project, &run)
        && error.code != "RunSealed"
    {
        recovery.retain(pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use webnovel_core::{
        context::packet::MockContextBudget,
        library::Library,
        projects::{CreateDocument, ProjectAccess},
    };

    struct Fixture {
        root: std::path::PathBuf,
        project: ProjectSession,
        access: ProjectAccess,
        request: StartDiscussion,
        choice: ModelSelection,
        library: DesktopLibrary,
    }
    impl Fixture {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "wns-http-reconcile-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&root).unwrap();
            let project =
                ProjectSession::create(root.join("project"), "HTTP reconciliation fixture")
                    .unwrap();
            let access = project.attach("test".into()).unwrap();
            let document = project.create_document(CreateDocument { access:access.clone(), operation_id:"create".into(), document_id:"chapter".into(), title:"Chapter".into(), kind:"chapter".into(), body:serde_json::json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":"The ending stays."}]}]}}) }).unwrap();
            let profile = EndpointProfile {
                id: "openai-compatible:00000000-0000-0000-0000-000000000001".into(),
                label: "Historical endpoint".into(),
                base_url: "http://127.0.0.1:1/v1".into(),
                enabled: true,
                json_mode: false,
                config_revision: "1".into(),
                credential_ref: None,
                manual_model_ids: vec!["writer".into()],
                cached_model_ids: vec![],
            };
            let choice = ModelSelection {
                provider_id: profile.id.clone(),
                model_id: "writer".into(),
                reasoning: None,
                service_tier: None,
            };
            let request = StartDiscussion {
                access: access.clone(),
                operation_id: "request".into(),
                expected: document.head,
                instruction: "Discuss the ending.".into(),
                intent: FeedbackIntent::Discuss,
                basis: None,
                scope: None,
                pinned_document_ids: vec![],
                safe_brief: None,
                budget: MockContextBudget::new("1", "0", "0"),
                provider_binding: Some(binding_for(&profile, &choice, FeedbackIntent::Discuss)),
                previous_run_id: None,
                lookup: None,
            };
            // The acceptance committed, but the caller lost its acknowledgement
            // before worker registration. Its old connection no longer exists.
            project.start_discussion(request.clone()).unwrap();
            let library = DesktopLibrary(Arc::new(Mutex::new(
                Library::open(root.join("library")).unwrap(),
            )));
            Self {
                root,
                project,
                access,
                request,
                choice,
                library,
            }
        }
        fn reconcile(
            &self,
            recovery: &DiscussionRecovery,
            runtime: &DesktopProviders,
        ) -> DiscussionStart {
            tauri::async_runtime::block_on(start(
                self.request.clone(),
                self.choice.clone(),
                self.project.clone(),
                recovery.clone(),
                self.library.clone(),
                runtime.clone(),
            ))
            .unwrap()
        }
        fn view(&self) -> DiscussionView {
            self.project
                .read_discussion(self.access.clone(), "chapter".into())
                .unwrap()
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
                    .starts_with("wns-http-reconcile-")
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn lost_start_acknowledgment_preserves_active_owner_or_seals_unsent_without_reloading_keys() {
        let fixture = Fixture::new("start");
        let recovery = DiscussionRecovery::default();
        let runtime = DesktopProviders::default();
        let queued = fixture.view().runs[0].clone();
        runtime.register_http(&queued.owner).unwrap();
        fixture.reconcile(&recovery, &runtime);
        assert_eq!(fixture.view().runs[0].status, DiscussionRunStatus::Queued);
        runtime.release_http(&queued.owner);
        fixture.reconcile(&recovery, &runtime);
        let again = fixture.reconcile(&recovery, &runtime);
        assert_eq!(again.run.status, DiscussionRunStatus::Failed);
        let view = fixture.view();
        assert_eq!(view.runs.len(), 1);
        let result = view.runs[0].provider_result.as_ref().unwrap();
        assert_eq!(
            result.delivery.as_ref().unwrap().submission,
            HttpDeliverySubmission::NotSent
        );
        assert_eq!(result.confirmed_stdin_bytes, "0");
        assert!(
            result
                .error
                .as_ref()
                .unwrap()
                .contains("No API request was sent")
        );
        assert_eq!(
            fixture
                .project
                .document(fixture.access.clone(), "chapter".into())
                .unwrap()
                .head,
            fixture.request.expected
        );
        fixture.finish();
    }

    #[test]
    fn failed_unsent_receipt_save_is_visible_and_retries_only_local_settlement() {
        let fixture = Fixture::new("save");
        let recovery = DiscussionRecovery::default();
        let runtime = DesktopProviders::default();
        let db = rusqlite::Connection::open(fixture.project.path.join("project.sqlite3")).unwrap();
        db.execute_batch("CREATE TRIGGER fail_http_receipt BEFORE INSERT ON provider_results BEGIN SELECT RAISE(ABORT,'synthetic write failure'); END;").unwrap();
        fixture.reconcile(&recovery, &runtime);
        let run = fixture.view().runs[0].clone();
        assert_eq!(run.status, DiscussionRunStatus::Running);
        assert_eq!(
            serde_json::to_value(recovery.view(fixture.view())).unwrap()["workerIssues"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        db.execute_batch("DROP TRIGGER fail_http_receipt;").unwrap();
        drop(db);
        recovery
            .retry(
                &fixture.project,
                fixture.access.clone(),
                "chapter".into(),
                run.id.clone(),
            )
            .unwrap();
        recovery
            .retry(
                &fixture.project,
                fixture.access.clone(),
                "chapter".into(),
                run.id,
            )
            .unwrap();
        let view = fixture.view();
        assert_eq!(view.runs[0].status, DiscussionRunStatus::Failed);
        assert_eq!(
            view.runs[0]
                .provider_result
                .as_ref()
                .unwrap()
                .delivery
                .as_ref()
                .unwrap()
                .submission,
            HttpDeliverySubmission::NotSent
        );
        fixture.finish();
    }
}
