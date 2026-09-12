//! Explicit story-memory refresh commands.
//!
//! Reading memory is safe on project open.  Starting a refresh is always an
//! author action and captures the selected model into the immutable core job.
use crate::app_state::AppState;
use crate::memory_recovery::MemoryRecovery;
use crate::project_commands::execute;
use crate::provider_bindings::{binding_matches_choice, is_legacy_maintenance_choice, is_supported_choice};
use serde::{Deserialize, Serialize};
use tauri::State;
use webnovel_core::context::memory::mock_navigation_digest;
use webnovel_core::context::packet::{MockContextBudget, ProviderBinding};
use webnovel_core::projects::discussions::{ProviderCleanup, ProviderOutcomeStatus};
use webnovel_core::projects::memory::{
    CompleteMemory, MemoryDispatch, MemoryJob, MemoryJobStatus, MemoryOwner, MemoryRead,
    StartMemory,
};
use webnovel_core::projects::{CoreError, CoreResult, Head, ProjectAccess, ProjectSession};
use webnovel_core::providers::preferences::ModelSelection;
#[cfg(windows)]
use webnovel_core::{
    library::codex_transport::CodexTransport, providers::codex_app_server::is_app_server,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartMemoryRequest {
    pub access: ProjectAccess,
    pub operation_id: String,
    pub expected: Head,
    pub budget: MockContextBudget,
    pub model_selection: ModelSelection,
    /// Absent only on a replay of an operation accepted by an older app.
    #[serde(default)]
    pub maintenance_revision: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMemoryRead {
    #[serde(flatten)]
    pub read: MemoryRead,
    pub pending_save: bool,
    pub pending_job_ids: Vec<String>,
}

fn model_settings_error() -> CoreError {
    CoreError::new(
        "ModelSettingsUnavailable",
        "Model settings could not be checked. Open Settings before refreshing story memory.",
    )
}

#[tauri::command]
pub async fn read_memory_source(
    access: ProjectAccess,
    view_id: String, state: State<'_, AppState>,
) -> CoreResult<webnovel_core::projects::story_context::SourceRead> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.read_memory_source(access, view_id)).await
}

#[tauri::command]
pub async fn read_memory(
    access: ProjectAccess,
    document_id: String, state: State<'_, AppState>,
) -> CoreResult<DesktopMemoryRead> {
    let app = &*state;
    let state = &app.projects;
    let recovery = &app.memory_recovery;
    let project = state.project(&access.project_id)?;
    let recovery = recovery.clone();
    execute(move || {
        let pending_job_ids = recovery.pending_job_ids(
            &access.project_id,
            &access.operation_namespace,
            &document_id,
        );
        Ok(DesktopMemoryRead {
            read: project.read_memory(access, document_id)?,
            pending_save: !pending_job_ids.is_empty(),
            pending_job_ids,
        })
    })
    .await
}

#[tauri::command]
pub async fn retry_memory_save(
    access: ProjectAccess,
    job_id: String, state: State<'_, AppState>,
) -> CoreResult<MemoryJob> {
    let app = &*state;
    let state = &app.projects;
    let recovery = &app.memory_recovery;
    let project = state.project(&access.project_id)?;
    let recovery = recovery.clone();
    execute(move || {
        let owner = MemoryOwner {
            project_id: access.project_id.clone(),
            operation_namespace: access.operation_namespace.clone(),
            job_id,
        };
        // Recovery writes are still renderer-authorized writes.  The owner
        // key protects project identity, while this read validates the live
        // renderer lease before retrying an uncertain local commit.
        let current = project.read_memory_job(owner.clone())?;
        project.read_memory(access, current.target.document_id)?;
        recovery.retry(&project, owner)
    })
    .await
}

#[tauri::command]
pub async fn stop_memory(
    access: ProjectAccess,
    job_id: String, state: State<'_, AppState>,
) -> CoreResult<MemoryJob> {
    let app = &*state;
    let state = &app.projects;
    let runtime = &app.providers;
    let project = state.project(&access.project_id)?;
    let runtime = runtime.clone();
    execute(move || {
        let owner = MemoryOwner {
            project_id: access.project_id.clone(),
            operation_namespace: access.operation_namespace.clone(),
            job_id: job_id.clone(),
        };
        match project.stop_memory(access, job_id) {
            Ok(job) => {
                runtime.stop_memory(&job.owner);
                Ok(job)
            }
            Err(error) => {
                if error.code == "UncertainOutcome" {
                    runtime.stop_memory(&owner);
                }
                Err(error)
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn start_memory(
    request: StartMemoryRequest, state: State<'_, AppState>,
) -> CoreResult<MemoryJob> {
    let app = &*state;
    let state = &app.projects;
    let recovery = &app.memory_recovery;
    let library = &app.library;
    let runtime = &app.providers;
    let project = state.project(&request.access.project_id)?;
    let recovery = recovery.clone();
    let library = library.clone();
    if request
        .model_selection
        .provider_id
        .starts_with("openai-compatible:")
    {
        return crate::http_memory::start(
            request,
            project,
            recovery,
            library,
            runtime.clone(),
        )
        .await;
    }
    let runtime = runtime.clone();
    execute(move || {
        let _admission = runtime.admit_request()?;
        let selected = request.model_selection.clone();
        let library_guard = library.0.lock().map_err(|_| model_settings_error())?;
        #[cfg(windows)]
        let mut app_server_request = None;
        let mut provider_binding = if is_supported_choice(&selected) {
            Some(ProviderBinding::codex_maintenance())
        } else {
            None
        };
        let existing = project
            .list_memory(request.access.clone())?
            .jobs
            .into_iter()
            .find(|job| {
                job.operation_id == request.operation_id
                && job.owner.operation_namespace == request.access.operation_namespace
            });
        #[cfg(windows)]
        let connection = if provider_binding.is_some() {
            runtime.connection().ok().filter(|connection| connection.catalog().supports(&selected))
        } else {
            None
        };
        if let Some(existing) = &existing {
            provider_binding = existing.provider_binding.clone();
            #[cfg(windows)]
            if existing.status == MemoryJobStatus::Queued
                && let Some(binding) = provider_binding.as_ref().filter(|binding| is_app_server(binding)) {
                app_server_request = Some(runtime.app_server()?.reserve(binding)?);
            }
        } else {
            #[cfg(windows)]
            if let Some(connection) = &connection {
                if library_guard.codex_transport_settings()?.transport == CodexTransport::AppServer {
                    let server = runtime.app_server()?;
                    let binding = server.maintenance_binding()?;
                    app_server_request = Some(server.reserve(&binding)?);
                    provider_binding = Some(binding);
                } else {
                    provider_binding = Some(crate::provider_bindings::connection_binding(connection));
                }
            }
        }
        #[cfg(windows)]
        if provider_binding.is_some() && existing.is_none() && connection.is_none() {
            return Err(CoreError::new(
                "ProviderUnavailable",
                "Check the Codex connection in Settings before refreshing story memory.",
            ));
        }
        #[cfg(not(windows))]
        if provider_binding.is_some() && existing.is_none() {
            return Err(CoreError::new(
                "ProviderUnavailable",
                "The Codex memory worker is currently available on Windows only.",
            ));
        }
        let start = StartMemory {
            access: request.access,
            operation_id: request.operation_id,
            expected: request.expected,
            budget: request.budget,
            provider_binding,
        };
        let started = {
            if existing.is_some() {
                check_saved_choice(&selected, &start.provider_binding)?;
            } else {
                check_maintenance_choice(&selected, request.maintenance_revision.as_deref(), &library_guard)?;
                check_saved_choice(&selected, &start.provider_binding)?;
            }
            // Keep preference acceptance and creation of a new immutable job
            // in one critical section.  Provider work starts only afterward.
            project.start_memory(start)?
        };
        drop(library_guard);
        if recovery.claim_pending(&started.owner) {
            // Replay only reconciles the local claim. It never submits the
            // request whose dispatch acknowledgment was uncertain.
            return recovery.retry(&project, started.owner);
        }
        if started.status != MemoryJobStatus::Queued {
            return Ok(started);
        }
        if started.provider_binding.is_some() {
            #[cfg(windows)]
            {
                let app_server = started.provider_binding.as_ref().is_some_and(is_app_server);
                if app_server && app_server_request.is_none() { return Err(CoreError::new("ProviderUnavailable", "The original app-server connection is unavailable. Check Codex in Settings; this request has not been sent.")); }
                let stop = match runtime.register_memory(&started.owner) {
                    Ok(stop) => stop,
                    Err(error) if error.code == "RunAlreadyStarted" => {
                        return project.read_memory_job(started.owner)
                    }
                    Err(error) => {
                        recovery.worker_not_registered(&project, &started);
                        return Err(error);
                    }
                };
                let dispatch = match recovery.claim(&project, &started) {
                    Ok(dispatch) => dispatch,
                    Err(error) => {
                        runtime.release_memory(&started.owner);
                        return Err(error);
                    }
                };
                if !dispatch.newly_dispatched {
                    // A lost start acknowledgment may observe a running job.
                    // Core's replay is deliberately not permission to submit
                    // the packet again.
                    runtime.release_memory(&started.owner);
                    return Ok(dispatch.job);
                }
                let authoritative = dispatch.job.clone();
                let failure_project = project.clone();
                let failure_owner = dispatch.job.owner.clone();
                let failure_document = dispatch.job.target.document_id.clone();
                let worker_runtime = runtime.clone();
                let worker_recovery = recovery.clone();
                if std::thread::Builder::new()
                    .name("webnovel-codex-memory".into())
                    .spawn(move || {
                        if app_server {
                            let request = app_server_request.expect("reserved before acceptance");
                            crate::app_server_memory::run_live(project, worker_recovery, worker_runtime,
                                request.reservation, request.thread, dispatch, stop);
                        } else { crate::live_memory::run_live(
                            project,
                            worker_recovery,
                            worker_runtime,
                            connection,
                            dispatch,
                            stop,
                        ) }
                    })
                    .is_err()
                {
                    runtime.release_memory(&failure_owner);
                    if app_server {
                        crate::app_server_memory::worker_unavailable(&failure_project, &recovery, failure_owner, failure_document,
                            "The memory refresh worker could not start. No new provider request was sent.");
                    } else { crate::live_memory::worker_unavailable(
                        &failure_project,
                        &recovery,
                        failure_owner,
                        failure_document,
                        "The memory refresh worker could not start. No new provider request was sent.",
                    ); }
                }
                return Ok(authoritative);
            }
            #[cfg(not(windows))]
            {
                let dispatch = recovery.claim(&project, &started)?;
                if !dispatch.newly_dispatched {
                    return Ok(dispatch.job);
                }
                record_worker_failure(
                    &project,
                    &recovery,
                    &dispatch,
                    "The Codex memory worker is currently available on Windows only.",
                );
                return project.read_memory_job(dispatch.job.owner);
            }
        }

        let worker_registration = runtime.track_local_worker()?;
        let dispatch = recovery.claim(&project, &started)?;
        if !dispatch.newly_dispatched {
            return Ok(dispatch.job);
        }
        let authoritative = dispatch.job.clone();
        let failure_project = project.clone();
        let failure_dispatch = dispatch.clone();
        let worker_recovery = recovery.clone();
        if std::thread::Builder::new()
            .name("webnovel-local-memory".into())
            .spawn(move || {
                let _worker_registration = worker_registration;
                run_mock(failure_project, worker_recovery, dispatch);
            })
            .is_err()
        {
            record_worker_failure(
                &project,
                &recovery,
                &failure_dispatch,
                "The local memory worker could not start.",
            );
        }
        Ok(authoritative)
    })
    .await
}

pub(crate) fn check_maintenance_choice(
    selected: &ModelSelection,
    revision: Option<&str>,
    library: &webnovel_core::library::Library,
) -> CoreResult<()> {
    let settings = library.story_memory_settings()?;
    if revision == Some(settings.revision.as_str())
        && selected == &crate::provider_bindings::memory_selection(&settings.provider_id)
    {
        Ok(())
    } else {
        Err(CoreError::new(
            "ModelChoiceChanged",
            "The story-memory connection changed before this refresh started. Check Settings and try again.",
        ))
    }
}

fn check_saved_choice(
    selected: &ModelSelection,
    binding: &Option<ProviderBinding>,
) -> CoreResult<()> {
    let matches = match binding {
        None => selected == &ModelSelection::local_mock(),
        Some(binding) => {
            !binding.is_http()
                && (binding_matches_choice(binding, selected)
                    || ((is_supported_choice(selected) || is_legacy_maintenance_choice(selected))
                        && binding == &ProviderBinding::codex_luna_historical()))
        }
    };
    if matches {
        Ok(())
    } else {
        Err(CoreError::new(
            "ProviderUnavailable",
            "This refresh's saved provider differs from the requested story-memory connection.",
        ))
    }
}

fn run_mock(project: ProjectSession, recovery: MemoryRecovery, dispatch: MemoryDispatch) {
    std::thread::sleep(std::time::Duration::from_millis(150));
    let owner = dispatch.job.owner.clone();
    let document_id = dispatch.job.target.document_id.clone();
    let completion = match mock_navigation_digest(&dispatch.source)
        .and_then(|candidate| serde_json::to_string(&candidate).map_err(CoreError::from))
    {
        Ok(raw_output) => CompleteMemory {
            owner,
            event_id: format!("{}-local-finish", dispatch.job.id),
            raw_output,
            outcome: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: Some(ProviderCleanup::Settled),
            error: None,
            effective_identity: None,
            delivery: None,
            app_server: None,
        },
        Err(_error) => CompleteMemory {
            owner,
            event_id: format!("{}-local-finish", dispatch.job.id),
            raw_output: String::new(),
            outcome: ProviderOutcomeStatus::Failed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: Some(ProviderCleanup::Settled),
            error: Some("The local memory response could not be prepared.".to_owned()),
            effective_identity: None,
            delivery: None,
            app_server: None,
        },
    };
    let _ = recovery.save_or_retain(&project, completion, document_id);
}

fn record_worker_failure(
    project: &ProjectSession,
    recovery: &MemoryRecovery,
    dispatch: &MemoryDispatch,
    detail: &'static str,
) {
    let completion = CompleteMemory {
        owner: dispatch.job.owner.clone(),
        event_id: format!("{}-worker-failure", dispatch.job.id),
        raw_output: String::new(),
        outcome: ProviderOutcomeStatus::Failed,
        confirmed_stdin_bytes: Some("0".to_owned()),
        usage: None,
        cleanup: Some(ProviderCleanup::Settled),
        error: Some(detail.to_owned()),
        effective_identity: None,
        delivery: None,
        app_server: None,
    };
    let _ = recovery.save_or_retain(project, completion, dispatch.job.target.document_id.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codex_luna() -> ModelSelection {
        ModelSelection {
            provider_id: "codex".into(),
            model_id: "gpt-5.6-luna".into(),
            reasoning: Some("xhigh".into()),
            service_tier: Some("priority".into()),
        }
    }

    fn codex_astra() -> ModelSelection {
        ModelSelection {
            provider_id: "codex".into(),
            model_id: "gpt-6-astra".into(),
            reasoning: Some("low".into()),
            service_tier: Some("priority".into()),
        }
    }

    #[test]
    fn saved_refresh_cannot_replace_its_binding_with_local_or_another_model() {
        assert!(check_saved_choice(&codex_luna(), &Some(ProviderBinding::codex_luna())).is_ok());
        assert!(check_saved_choice(&ModelSelection::local_mock(), &None).is_ok());
        assert!(check_saved_choice(&codex_luna(), &None).is_err());
        assert!(
            check_saved_choice(
                &ModelSelection::local_mock(),
                &Some(ProviderBinding::codex_luna())
            )
            .is_err()
        );
    }

    #[test]
    fn memory_uses_astra_low_independently_of_the_drafting_model_and_preserves_old_receipts() {
        let astra = codex_astra();
        let drafting = ModelSelection {
            provider_id: "claude".into(),
            model_id: "claude-sonnet".into(),
            reasoning: None,
            service_tier: None,
        };
        assert!(check_saved_choice(&astra, &Some(ProviderBinding::codex_maintenance())).is_ok());
        assert!(
            check_saved_choice(&drafting, &Some(ProviderBinding::codex_maintenance())).is_err()
        );
        let luna = codex_luna();
        assert!(check_saved_choice(&luna, &Some(ProviderBinding::codex_luna())).is_ok());
        let historical = Some(ProviderBinding::codex_luna_historical());
        assert!(check_saved_choice(&luna, &historical).is_ok());
    }

    #[test]
    fn unsupported_model_is_rejected_even_when_a_saved_operation_exists() {
        let unsupported = ModelSelection {
            provider_id: "claude".into(),
            model_id: "claude-sonnet".into(),
            reasoning: None,
            service_tier: None,
        };
        assert_eq!(
            check_saved_choice(&unsupported, &None)
                .expect_err("a saved operation cannot authorize a different provider")
                .code,
            "ProviderUnavailable"
        );
    }
}
