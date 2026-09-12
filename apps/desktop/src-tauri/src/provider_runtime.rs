//! Session-local readiness and Stop ownership; saved preferences remain in Library.
use crate::provider_bindings::{
    ClaudeConnectionView, ConnectionView, claude_binding_for_choice, memory_selection,
};
use crate::endpoint_commands::credential_available;
use serde::Serialize;
#[cfg(windows)]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use webnovel_core::context::packet::{
    CODEX_MAINTENANCE_MODEL_ID, CODEX_MAINTENANCE_REASONING, CODEX_SERVICE_TIER,
};
use webnovel_core::library::codex_transport::CodexTransport;
#[cfg(windows)]
use webnovel_core::projects::discussions::RunOwner;
use webnovel_core::projects::memory::MemoryOwner;
use webnovel_core::projects::{CoreError, CoreResult};
#[cfg(windows)]
use webnovel_core::providers::claude_runtime::ClaudeConnection;
#[cfg(windows)]
use webnovel_core::providers::codex_app_server::connection::ManagedAppServer;
use webnovel_core::providers::{
    catalog::{DispatchResolution, ProviderState},
    credentials::WindowsCredentialStore,
    preferences::ModelSelection,
};
#[cfg(windows)]
use webnovel_core::providers::{cli::windows_process::StopSignal, codex_runtime::CodexConnection};

#[derive(Clone, Default)]
pub struct DesktopProviders(Arc<Mutex<RuntimeState>>);
#[derive(Default)]
struct RuntimeState {
    close_request: Option<String>,
    close_stopping: bool,
    cancelled_close_requests: std::collections::HashSet<String>,
    starting_requests: u32,
    local_workers: u32,
    checking: bool,
    checked: bool,
    #[cfg(windows)]
    connection: Option<CodexConnection>,
    #[cfg(windows)]
    app_server: Option<ManagedAppServer>,
    claude_checking: bool,
    #[cfg(windows)]
    claude_connection: Option<ClaudeConnection>,
    claude_detail: Option<String>,
    #[cfg(windows)]
    stops: HashMap<(String, String, String), StopSignal>,
    detail: Option<String>,
    http_stops: std::collections::HashMap<
        (String, String, String),
        webnovel_core::providers::adapter::CancellationToken,
    >,
    http_memory_stops: std::collections::HashMap<
        (String, String, String),
        webnovel_core::providers::adapter::CancellationToken,
    >,
}

/// Held inside the actual acceptance closure, including when its IPC waiter
/// disappears. Closing cannot observe a gap before worker registration.
pub struct RequestAdmission(DesktopProviders);
impl Drop for RequestAdmission {
    fn drop(&mut self) {
        let mut state = self
            .0
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.starting_requests -= 1;
    }
}

pub struct LocalWorkerRegistration(DesktopProviders);
impl Drop for LocalWorkerRegistration {
    fn drop(&mut self) {
        let mut state = self
            .0
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.local_workers -= 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseActivity {
    pub starting_requests: u32,
    pub active_workers: usize,
    pub stopping: bool,
}

/// Cloned cancellation handles belong to the workers captured at confirmation.
/// A later Stay open/new request cannot redirect these handles to new work.
pub struct CloseCancellations {
    http: Vec<webnovel_core::providers::adapter::CancellationToken>,
    #[cfg(windows)]
    cli: Vec<StopSignal>,
}
impl CloseCancellations {
    pub fn cancel(self) {
        for stop in self.http {
            stop.cancel();
        }
        #[cfg(windows)]
        for stop in self.cli {
            stop.request_stop();
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProviderState {
    #[serde(flatten)]
    state: ProviderState,
    codex_connection: ConnectionView,
    claude_connection: ClaudeConnectionView,
    story_memory: StoryMemoryView,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoryMemoryView {
    revision: String,
    provider_id: String,
    provider_label: String,
    model_id: String,
    reasoning: Option<String>,
    service_tier: Option<String>,
    ready: bool,
    detail: String,
}











pub(crate) fn unavailable() -> CoreError {
    CoreError::new(
        "ProviderUnavailable",
        "Check the Codex connection in Settings before sending this request.",
    )
}

impl DesktopProviders {
    fn app_server_view(&self, view: &mut DesktopProviderState) -> CoreResult<()> {
        let runtime = self.lock()?;
        #[cfg(windows)]
        let server = runtime
            .app_server
            .as_ref()
            .filter(|server| server.healthy());
        #[cfg(windows)]
        let ready = server.is_some();
        #[cfg(not(windows))]
        let ready = false;
        let detail = if ready {
            "Codex app-server is connected. Each request starts a fresh isolated conversation."
        } else {
            "App-server is selected. Check Codex in Settings to qualify and connect this transport."
        };
        view.codex_connection.ready = ready;
        view.codex_connection.detail = runtime
            .detail
            .as_deref()
            .filter(|_| !ready && !runtime.checking && runtime.connection.is_none())
            .unwrap_or(detail)
            .into();
        #[cfg(windows)]
        {
            view.codex_connection.memory_ready =
                server.is_some_and(|server| server.maintenance_binding().is_ok());
        }
        #[cfg(not(windows))]
        {
            view.codex_connection.memory_ready = false;
        }
        for model in view
            .state
            .catalog
            .models
            .iter_mut()
            .filter(|model| model.key.provider_id == "codex")
        {
            #[cfg(windows)]
            let supported = server.is_some_and(|server| {
                let choice = if model.key == view.state.settings.active.key() {
                    view.state.settings.active.clone()
                } else {
                    ModelSelection {
                        provider_id: "codex".into(),
                        model_id: model.key.model_id.clone(),
                        reasoning: model.default_reasoning.clone(),
                        service_tier: model.default_service_tier.clone(),
                    }
                };
                server.author_binding(&choice).is_ok()
            });
            #[cfg(not(windows))]
            let supported = false;
            model.ready = supported;
            model.status_detail = detail.into();
        }
        if view.state.settings.active.provider_id == "codex" {
            #[cfg(windows)]
            let supported = server
                .is_some_and(|server| server.author_binding(&view.state.settings.active).is_ok());
            #[cfg(not(windows))]
            let supported = false;
            view.state.dispatch = if supported {
                DispatchResolution::CodexCli { detail: "Uses the persistent Codex server with fresh story context. Sending starts one response.".into() }
            } else {
                DispatchResolution::Blocked {
                    detail: detail.into(),
                }
            };
        }
        Ok(())
    }

    #[cfg(windows)]
    pub fn app_server(&self) -> CoreResult<ManagedAppServer> {
        let state = self.lock()?;
        if state.checking {
            return Err(unavailable());
        }
        state
            .app_server
            .as_ref()
            .filter(|server| server.healthy())
            .cloned()
            .ok_or_else(unavailable)
    }

    pub fn shutdown_app_server(&self, close_id: &str) -> CoreResult<()> {
        let activity = self.close_activity(close_id)?;
        if activity.starting_requests != 0 || activity.active_workers != 0 {
            return Err(CoreError::new(
                "CloseNotReady",
                "Provider work is still settling.",
            ));
        }
        #[cfg(windows)]
        let server = self.lock()?.app_server.take();
        #[cfg(windows)]
        if let Some(server) = server {
            server.shutdown_idle()?;
        }
        self.close_activity(close_id)?;
        Ok(())
    }

    #[cfg(windows)]
    pub fn check_selected_connection(
        &self,
        transport: CodexTransport,
        selection: &ModelSelection,
        persist: impl FnOnce(&CodexConnection) -> CoreResult<()>,
    ) -> CoreResult<()> {
        let _admission = self.admit_request()?;
        let old_server = {
            let mut state = self.lock()?;
            if state
                .app_server
                .as_ref()
                .is_some_and(|server| server.active_count() != 0)
            {
                return Err(CoreError::new(
                    "ProviderBusy",
                    "Let current Codex replies finish before checking or replacing the persistent connection.",
                ));
            }
            state.app_server.take()
        };
        if let Some(server) = old_server {
            server.shutdown_idle()?;
        }
        self.begin_codex_check()?;
        let checked = CodexConnection::check_installed().and_then(|connection| {
            let server = if transport == CodexTransport::AppServer {
                let initial = if connection.catalog().supports(selection) {
                    selection.clone()
                } else {
                    let model = connection
                        .catalog()
                        .models
                        .iter()
                        .find(|model| model.default_reasoning.is_some())
                        .ok_or_else(unavailable)?;
                    ModelSelection {
                        provider_id: "codex".into(),
                        model_id: model.model_id.clone(),
                        reasoning: model.default_reasoning.clone(),
                        service_tier: model.default_service_tier.clone(),
                    }
                };
                Some(ManagedAppServer::start(connection.clone(), &initial)?)
            } else {
                None
            };
            Ok((connection, server))
        });
        self.finish_codex_check(
            checked,
            |(connection, _)| persist(connection),
            |(connection, server), state| {
                state.connection = Some(connection);
                state.app_server = server;
            },
        )
    }

    pub fn admit_request(&self) -> CoreResult<RequestAdmission> {
        let mut state = self.lock()?;
        if state.close_request.is_some() {
            return Err(CoreError::new(
                "AppClosing",
                "The app is preparing to close. Stay open before starting another request.",
            ));
        }
        state.starting_requests = state
            .starting_requests
            .checked_add(1)
            .ok_or_else(|| CoreError::new("TooManyRequests", "Too many requests are starting."))?;
        Ok(RequestAdmission(self.clone()))
    }

    pub fn track_local_worker(&self) -> CoreResult<LocalWorkerRegistration> {
        let mut state = self.lock()?;
        state.local_workers = state
            .local_workers
            .checked_add(1)
            .ok_or_else(|| CoreError::new("TooManyRequests", "Too many local jobs are running."))?;
        Ok(LocalWorkerRegistration(self.clone()))
    }

    pub fn begin_close(&self, close_id: &str) -> CoreResult<()> {
        if close_id.is_empty() || close_id.len() > 128 {
            return Err(CoreError::new(
                "InvalidRequest",
                "The close request is invalid.",
            ));
        }
        let mut state = self.lock()?;
        if state.cancelled_close_requests.contains(close_id) {
            return Err(CoreError::new(
                "CloseRequestChanged",
                "This close request was cancelled.",
            ));
        }
        if state
            .close_request
            .as_deref()
            .is_some_and(|current| current != close_id)
        {
            return Err(CoreError::new(
                "CloseRequestChanged",
                "Another close request is already active.",
            ));
        }
        if state.close_request.is_none() {
            state.close_stopping = false;
        }
        state.close_request = Some(close_id.to_owned());
        Ok(())
    }

    pub fn cancel_close(&self, close_id: &str) -> CoreResult<()> {
        if close_id.is_empty() || close_id.len() > 128 {
            return Err(CoreError::new(
                "InvalidRequest",
                "The close request is invalid.",
            ));
        }
        let mut state = self.lock()?;
        if state
            .close_request
            .as_deref()
            .is_some_and(|current| current != close_id)
        {
            return Err(CoreError::new(
                "CloseRequestChanged",
                "This close request is no longer current.",
            ));
        }
        state.close_request = None;
        state.close_stopping = false;
        state.cancelled_close_requests.insert(close_id.to_owned());
        Ok(())
    }

    /// A replacement renderer must not inherit an abandoned modal close gate.
    /// Workers keep their exact owners; only the obsolete close request retires.
    pub fn renderer_started(&self) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(close_id) = state.close_request.take() {
            state.cancelled_close_requests.insert(close_id);
        }
        state.close_stopping = false;
    }

    pub fn confirm_close_stop(&self, close_id: &str) -> CoreResult<()> {
        let mut state = self.lock()?;
        if state.close_request.as_deref() != Some(close_id) {
            return Err(CoreError::new(
                "CloseRequestChanged",
                "This close request is no longer current.",
            ));
        }
        if state.starting_requests != 0 {
            return Err(CoreError::new(
                "CloseNotReady",
                "Wait for requests that are still starting.",
            ));
        }
        state.close_stopping = true;
        Ok(())
    }

    pub fn close_activity(&self, close_id: &str) -> CoreResult<CloseActivity> {
        let state = self.lock()?;
        if state.close_request.as_deref() != Some(close_id) {
            return Err(CoreError::new(
                "CloseRequestChanged",
                "This close request is no longer current.",
            ));
        }
        let active_workers =
            state.local_workers as usize + state.http_stops.len() + state.http_memory_stops.len();
        #[cfg(windows)]
        let active_workers = active_workers + state.stops.len();
        Ok(CloseActivity {
            starting_requests: state.starting_requests,
            active_workers,
            stopping: state.close_stopping,
        })
    }

    /// Author-confirmed app close targets only workers owned by this runtime.
    /// Call even when persisting Stop failed; storage failure cannot require
    /// continued external work. The close gate stays active until cleanup.
    pub fn capture_close_cancellations(&self, close_id: &str) -> CoreResult<CloseCancellations> {
        let state = self.lock()?;
        if state.close_request.as_deref() != Some(close_id) || state.starting_requests != 0 {
            return Err(CoreError::new(
                "CloseNotReady",
                "Wait for requests that are still starting.",
            ));
        }
        Ok(CloseCancellations {
            http: state
                .http_stops
                .values()
                .chain(state.http_memory_stops.values())
                .cloned()
                .collect(),
            #[cfg(windows)]
            cli: state.stops.values().cloned().collect(),
        })
    }

    pub fn register_http_memory(
        &self,
        owner: &MemoryOwner,
    ) -> CoreResult<webnovel_core::providers::adapter::CancellationToken> {
        let mut state = self.lock()?;
        let key = (
            owner.project_id.clone(),
            owner.operation_namespace.clone(),
            owner.job_id.clone(),
        );
        if state.http_memory_stops.contains_key(&key) {
            return Err(CoreError::new(
                "RunAlreadyStarted",
                "This memory refresh already has a worker.",
            ));
        }
        let stop = webnovel_core::providers::adapter::CancellationToken::new();
        state.http_memory_stops.insert(key, stop.clone());
        Ok(stop)
    }

    pub fn release_http_memory(&self, owner: &MemoryOwner) {
        if let Ok(mut state) = self.lock() {
            state.http_memory_stops.remove(&(
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.job_id.clone(),
            ));
        }
    }

    pub fn register_http(
        &self,
        owner: &webnovel_core::projects::discussions::RunOwner,
    ) -> CoreResult<webnovel_core::providers::adapter::CancellationToken> {
        let mut state = self.lock()?;
        let key = (
            owner.project_id.clone(),
            owner.operation_namespace.clone(),
            owner.run_id.clone(),
        );
        if state.http_stops.contains_key(&key) {
            return Err(CoreError::new(
                "RunAlreadyStarted",
                "This response already has a worker.",
            ));
        }
        let stop = webnovel_core::providers::adapter::CancellationToken::new();
        state.http_stops.insert(key, stop.clone());
        Ok(stop)
    }
    pub fn release_http(&self, owner: &webnovel_core::projects::discussions::RunOwner) {
        if let Ok(mut state) = self.lock() {
            state.http_stops.remove(&(
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.run_id.clone(),
            ));
        }
    }
    pub fn view_library(
        &self,
        library: &webnovel_core::library::Library,
    ) -> CoreResult<DesktopProviderState> {
        self.view_library_with_store(library, &WindowsCredentialStore)
    }

    fn view_library_with_store(
        &self,
        library: &webnovel_core::library::Library,
        store: &dyn webnovel_core::providers::credentials::CredentialStore,
    ) -> CoreResult<DesktopProviderState> {
        let mut view = self.view(library.provider_state()?)?;
        if library.codex_transport_settings()?.transport == CodexTransport::AppServer {
            self.app_server_view(&mut view)?;
        }
        let endpoints = library.endpoint_profiles()?;
        for profile in &endpoints.profiles {
            let available = profile.enabled && credential_available(profile, store);
            for model in view
                .state
                .catalog
                .models
                .iter_mut()
                .filter(|m| m.key.provider_id == profile.id)
            {
                let listed = profile.manual_model_ids.contains(&model.key.model_id)
                    || profile.cached_model_ids.contains(&model.key.model_id);
                model.ready = available && listed;
                if model.ready {
                    model.status_detail =
                        "API endpoint configured. Sending starts one request to this service."
                            .into();
                } else if listed
                    && profile.enabled
                    && profile.credential_ref.is_some()
                    && !available
                {
                    model.status_detail =
                        "The saved API key is unavailable. Re-enter it in Settings before sending."
                            .into();
                }
            }
            if view.state.settings.active.provider_id == profile.id
                && view.state.catalog.models.iter().any(|m| {
                    m.key.model_id == view.state.settings.active.model_id
                        && m.key.provider_id == profile.id
                        && m.ready
                })
            {
                view.state.dispatch = DispatchResolution::OpenAiCompatible {
                    detail: format!(
                        "Uses the API connection {}. Sending starts one response.",
                        profile.label
                    ),
                };
            }
        }
        let settings = library.story_memory_settings()?;
        let selected = memory_selection(&settings.provider_id);
        let mut memory = StoryMemoryView {
            revision: settings.revision,
            provider_id: selected.provider_id.clone(),
            provider_label: "Unavailable connection".into(),
            model_id: selected.model_id.clone(),
            reasoning: selected.reasoning.clone(),
            service_tier: selected.service_tier.clone(),
            ready: false,
            detail: "This story-memory connection is unavailable. Choose a connection in Settings."
                .into(),
        };
        match selected.provider_id.as_str() {
            "mock" => {
                memory.provider_label = "Local test model".into();
                memory.ready = true;
                memory.detail =
                    "Creates a local demonstration digest. No LLM or API request is made.".into();
            }
            "codex" => {
                memory.provider_label = "Codex".into();
                memory.ready = view.codex_connection.memory_ready;
                memory.detail = if memory.ready { "Refresh sends one request through Codex using GPT-6 Astra with Low reasoning." } else { "Check Codex in Settings. Story memory requires GPT-6 Astra with Low reasoning and Fast service." }.into();
            }
            _ => {
                if let Some(profile) = endpoints
                    .profiles
                    .iter()
                    .find(|p| p.id == selected.provider_id)
                {
                    memory.provider_label = profile.label.clone();
                    let listed = profile.manual_model_ids.contains(&selected.model_id)
                        || profile.cached_model_ids.contains(&selected.model_id);
                    memory.ready =
                        profile.enabled && listed && credential_available(profile, store);
                    memory.detail = if !profile.enabled {
                    "Enable this API connection in Settings before refreshing story memory."
                } else if !listed {
                    "Add gpt-6-astra to this connection's models. The service must support low reasoning."
                } else if !memory.ready {
                    "The saved API key is unavailable. Re-enter it in Settings before refreshing."
                } else {
                    "Configured to request GPT-6 Astra with low reasoning. The API service must support these settings."
                }.into();
                }
            }
        }
        view.story_memory = memory;
        Ok(view)
    }
    fn lock(&self) -> CoreResult<std::sync::MutexGuard<'_, RuntimeState>> {
        self.0.lock().map_err(|_| unavailable())
    }
    pub fn view(&self, mut state: ProviderState) -> CoreResult<DesktopProviderState> {
        let runtime = self.lock()?;
        #[cfg(windows)]
        let ready = runtime.connection.is_some();
        #[cfg(not(windows))]
        let ready = false;
        let detail = runtime.detail.clone().unwrap_or_else(|| "Use the Codex sign-in on this computer. Check the connection to enable live requests.".into());
        if ready {
            #[cfg(windows)]
            for model in state
                .catalog
                .models
                .iter_mut()
                .filter(|model| model.key.provider_id == "codex")
            {
                model.ready = runtime.connection.as_ref().is_some_and(|connection| {
                    connection
                        .catalog()
                        .models
                        .iter()
                        .any(|current| current.model_id == model.key.model_id)
                        && (model.key != state.settings.active.key()
                            || connection.catalog().supports(&state.settings.active))
                });
                if model.ready {
                    model.status_detail = "Available through the checked Codex connection.".into();
                }
            }
            if state.settings.active.provider_id == "codex" {
                #[cfg(windows)]
                let supported = runtime.connection.as_ref().is_some_and(|connection| {
                    connection.catalog().supports(&state.settings.active)
                });
                #[cfg(not(windows))]
                let supported = false;
                state.dispatch = if supported {
                    DispatchResolution::CodexCli {
                        detail: "Uses your Codex sign-in. Sending starts one live response.".into(),
                    }
                } else {
                    DispatchResolution::Blocked { detail: "This model or its selected traits are no longer available through the checked Codex connection. Choose available settings or check the connection again.".into() }
                };
            }
        }
        #[cfg(windows)]
        let memory_ready = runtime.connection.as_ref().is_some_and(|connection| {
            connection.catalog().supports(&ModelSelection {
                provider_id: "codex".into(),
                model_id: CODEX_MAINTENANCE_MODEL_ID.into(),
                reasoning: Some(CODEX_MAINTENANCE_REASONING.into()),
                service_tier: Some(CODEX_SERVICE_TIER.into()),
            })
        });
        #[cfg(not(windows))]
        let memory_ready = false;
        #[cfg(windows)]
        let claude_ready = runtime.claude_connection.is_some();
        #[cfg(not(windows))]
        let claude_ready = false;
        let claude_detail = runtime.claude_detail.clone().unwrap_or_else(|| {
            "Check the installed Claude Code sign-in to enable this connection.".into()
        });
        for model in state
            .catalog
            .models
            .iter_mut()
            .filter(|model| model.key.provider_id == "claude")
        {
            #[cfg(windows)]
            let supported = runtime
                .claude_connection
                .as_ref()
                .is_some_and(|connection| {
                    connection.permits_model(&model.key.model_id)
                        && (model.key != state.settings.active.key()
                            || claude_binding_for_choice(connection, &state.settings.active)
                                .is_ok())
                });
            #[cfg(not(windows))]
            let supported = false;
            model.ready = supported;
            model.status_detail = if supported {
                "Available through the checked Claude Code connection. These models and effort choices come from the app's reference catalog.".into()
            } else if claude_ready {
                "This model or its selected settings are unavailable in the checked Claude installation.".into()
            } else {
                claude_detail.clone()
            };
        }
        if state.settings.active.provider_id == "claude" {
            #[cfg(windows)]
            let supported = runtime
                .claude_connection
                .as_ref()
                .is_some_and(|connection| {
                    claude_binding_for_choice(connection, &state.settings.active).is_ok()
                });
            #[cfg(not(windows))]
            let supported = false;
            state.dispatch = if supported {
                DispatchResolution::ClaudeCli {
                    detail: "Uses your Claude Code sign-in. Sending starts one live response."
                        .into(),
                }
            } else {
                DispatchResolution::Blocked {
                    detail: if claude_ready {
                        "This Claude model or its selected settings are unavailable. Choose supported traits or check the connection again.".into()
                    } else {
                        claude_detail.clone()
                    },
                }
            };
        }
        Ok(DesktopProviderState {
            state,
            claude_connection: ClaudeConnectionView {
                ready: claude_ready,
                detail: claude_detail,
            },
            story_memory: StoryMemoryView {
                revision: "0".into(),
                provider_id: "codex".into(),
                provider_label: "Codex".into(),
                model_id: CODEX_MAINTENANCE_MODEL_ID.into(),
                reasoning: Some(CODEX_MAINTENANCE_REASONING.into()),
                service_tier: Some(CODEX_SERVICE_TIER.into()),
                ready: memory_ready,
                detail: "Check Codex in Settings before refreshing story memory.".into(),
            },
            codex_connection: ConnectionView {
                ready,
                checked: runtime.checked,
                checking: runtime.checking,
                memory_ready,
                detail,
            },
        })
    }

    fn begin_codex_check(&self) -> CoreResult<()> {
        let mut state = self.lock()?;
        if state.checking {
            return Err(CoreError::new(
                "ConnectionCheckRunning",
                "A Codex connection check is already running.",
            ));
        }
        state.checking = true;
        state.checked = true;
        #[cfg(windows)]
        {
            state.connection = None;
        }
        Ok(())
    }

    /// Finish a check only after its durable publication callback succeeds.
    /// The callback runs without the runtime mutex held, so readers can observe
    /// the checking fence but never a ready connection backed by an unsaved
    /// catalog.
    #[cfg(windows)]
    fn finish_codex_check<T>(
        &self,
        checked: CoreResult<T>,
        persist: impl FnOnce(&T) -> CoreResult<()>,
        publish: impl FnOnce(T, &mut RuntimeState),
    ) -> CoreResult<()> {
        let checked = match checked {
            Ok(value) => value,
            Err(error) => {
                let mut state = self.lock()?;
                state.checking = false;
                #[cfg(windows)]
                {
                    state.connection = None;
                }
                state.detail = Some(error.detail.clone());
                return Err(error);
            }
        };
        if let Err(error) = persist(&checked) {
            let mut state = self.lock()?;
            state.checking = false;
            #[cfg(windows)]
            {
                state.connection = None;
            }
            state.detail = Some(
                "The discovered Codex models could not be saved. Check the connection again."
                    .into(),
            );
            return Err(error);
        }
        let mut state = self.lock()?;
        state.checking = false;
        publish(checked, &mut state);
        state.detail = Some(
            "Signed in through Codex. Available models and traits were read from this installation."
                .into(),
        );
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn check_connection(&self) -> CoreResult<()> {
        self.begin_codex_check()?;
        let mut state = self.lock()?;
        state.checking = false;
        state.detail = Some("This Codex connection is currently available on Windows only.".into());
        Ok(())
    }

    /// An explicit read-only check. No prompt, shell, or paid invocation is used.
    pub fn check_claude_connection(&self) -> CoreResult<()> {
        {
            let mut state = self.lock()?;
            if state.claude_checking {
                return Err(CoreError::new(
                    "ConnectionCheckRunning",
                    "A Claude connection check is already running.",
                ));
            }
            state.claude_checking = true;
            #[cfg(windows)]
            {
                state.claude_connection = None;
            }
        }
        #[cfg(windows)]
        {
            let checked = ClaudeConnection::check_installed();
            let mut state = self.lock()?;
            state.claude_checking = false;
            match checked {
                Ok(connection) => {
                    state.claude_connection = Some(connection);
                    state.claude_detail = Some("Claude Code is signed in and its required command options are available. Model choices use the app's reference catalog.".into());
                }
                Err(error) => {
                    state.claude_detail = Some(error.detail);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let mut state = self.lock()?;
            state.claude_checking = false;
            state.claude_detail =
                Some("This Claude Code connection is currently available on Windows only.".into());
        }
        Ok(())
    }

    #[cfg(windows)]
    pub fn claude_connection(&self) -> CoreResult<ClaudeConnection> {
        self.lock()?.claude_connection.clone().ok_or_else(|| {
            CoreError::new(
                "ProviderUnavailable",
                "Check the Claude Code connection in Settings before sending.",
            )
        })
    }

    #[cfg(windows)]
    pub fn connection(&self) -> CoreResult<CodexConnection> {
        self.lock()?.connection.clone().ok_or_else(unavailable)
    }
    #[cfg(windows)]
    fn register_key(
        &self,
        key: (String, String, String),
        detail: &'static str,
    ) -> CoreResult<StopSignal> {
        let mut state = self.lock()?;
        if state.stops.contains_key(&key) {
            return Err(CoreError::new("RunAlreadyStarted", detail));
        }
        let stop = StopSignal::new();
        state.stops.insert(key, stop.clone());
        Ok(stop)
    }
    #[cfg(windows)]
    pub fn register(&self, owner: &RunOwner) -> CoreResult<StopSignal> {
        self.register_key(
            (
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.run_id.clone(),
            ),
            "This response already has a worker.",
        )
    }
    #[cfg(windows)]
    pub fn register_memory(&self, owner: &MemoryOwner) -> CoreResult<StopSignal> {
        self.register_key(
            (
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.job_id.clone(),
            ),
            "This memory refresh already has a worker.",
        )
    }
    pub fn stop(&self, owner: &webnovel_core::projects::discussions::RunOwner) {
        if let Ok(state) = self.lock()
            && let Some(stop) = state.http_stops.get(&(
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.run_id.clone(),
            ))
        {
            stop.cancel();
        }
        #[cfg(windows)]
        if let Ok(state) = self.lock()
            && let Some(stop) = state.stops.get(&(
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.run_id.clone(),
            ))
        {
            stop.request_stop();
        }
        #[cfg(not(windows))]
        let _ = owner;
    }
    pub fn stop_memory(&self, owner: &MemoryOwner) {
        if let Ok(state) = self.lock()
            && let Some(stop) = state.http_memory_stops.get(&(
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.job_id.clone(),
            ))
        {
            stop.cancel();
        }
        #[cfg(windows)]
        if let Ok(state) = self.lock()
            && let Some(stop) = state.stops.get(&(
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.job_id.clone(),
            ))
        {
            stop.request_stop();
        }
    }
    #[cfg(windows)]
    pub fn release(&self, owner: &RunOwner) {
        if let Ok(mut state) = self.lock() {
            state.stops.remove(&(
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.run_id.clone(),
            ));
        }
    }
    #[cfg(windows)]
    pub fn release_memory(&self, owner: &MemoryOwner) {
        if let Ok(mut state) = self.lock() {
            state.stops.remove(&(
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.job_id.clone(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The binding predicates moved to `provider_bindings`; the tests below
    // exercise them alongside the runtime they were split from.
    use crate::provider_bindings::{
        binding_matches_author_choice, binding_matches_choice, is_supported_choice,
    };
    use webnovel_core::context::packet::ProviderBinding;

    #[test]
    fn close_gate_drains_accepted_starts_and_tracks_local_worker_until_completion() {
        let runtime = DesktopProviders::default();
        let admission = runtime.admit_request().unwrap();
        runtime.begin_close("close-one").unwrap();
        assert_eq!(runtime.admit_request().err().unwrap().code, "AppClosing");
        assert_eq!(
            runtime
                .close_activity("close-one")
                .unwrap()
                .starting_requests,
            1
        );
        assert_eq!(
            runtime
                .capture_close_cancellations("close-one")
                .err()
                .unwrap()
                .code,
            "CloseNotReady"
        );
        // Already-admitted starts may register after the close gate is set.
        let worker = runtime.track_local_worker().unwrap();
        drop(admission);
        assert_eq!(
            runtime.close_activity("close-one").unwrap(),
            CloseActivity {
                starting_requests: 0,
                active_workers: 1,
                stopping: false
            }
        );
        drop(worker);
        assert_eq!(
            runtime.close_activity("close-one").unwrap().active_workers,
            0
        );
        runtime.cancel_close("close-one").unwrap();
        assert!(runtime.admit_request().is_ok());
    }

    #[test]
    fn an_old_close_response_cannot_cancel_or_authorize_a_new_close() {
        let runtime = DesktopProviders::default();
        runtime.begin_close("old").unwrap();
        runtime.cancel_close("old").unwrap();
        runtime.begin_close("new").unwrap();
        assert_eq!(
            runtime.cancel_close("old").unwrap_err().code,
            "CloseRequestChanged"
        );
        assert_eq!(
            runtime.close_activity("old").unwrap_err().code,
            "CloseRequestChanged"
        );
        assert_eq!(runtime.admit_request().err().unwrap().code, "AppClosing");
        runtime.cancel_close("new").unwrap();
        assert!(runtime.admit_request().is_ok());
    }

    #[test]
    fn cancelling_before_a_delayed_begin_keeps_request_admission_open() {
        let runtime = DesktopProviders::default();
        runtime.cancel_close("lost-begin").unwrap();
        assert_eq!(
            runtime.begin_close("lost-begin").unwrap_err().code,
            "CloseRequestChanged"
        );
        assert!(runtime.admit_request().is_ok());
    }

    #[test]
    fn close_cancellation_cannot_stop_new_work_started_after_staying_open() {
        use webnovel_core::projects::discussions::RunOwner;
        let runtime = DesktopProviders::default();
        let old = RunOwner {
            project_id: "project".into(),
            operation_namespace: "namespace".into(),
            run_id: "old".into(),
        };
        let new = RunOwner {
            run_id: "new".into(),
            ..old.clone()
        };
        let old_signal = runtime.register_http(&old).unwrap();
        runtime.begin_close("close").unwrap();
        let captured = runtime.capture_close_cancellations("close").unwrap();
        runtime.cancel_close("close").unwrap();
        let new_signal = runtime.register_http(&new).unwrap();
        captured.cancel();
        assert!(old_signal.is_cancelled());
        assert!(!new_signal.is_cancelled());
    }

    #[test]
    fn unchecked_claude_reference_catalog_cannot_enable_dispatch_or_change_memory() {
        let choice = ModelSelection {
            provider_id: "claude".into(),
            model_id: "claude-sonnet-5".into(),
            reasoning: Some("high".into()),
            service_tier: None,
        };
        let state = ProviderState {
            settings: webnovel_core::providers::preferences::ModelSettings {
                revision: "3".into(),
                active: choice.clone(),
                favorites: vec![],
            },
            catalog: webnovel_core::providers::catalog::built_in_catalog(),
            // Even a precomputed resolution cannot bypass the native check.
            dispatch: DispatchResolution::ClaudeCli {
                detail: "Untrusted precomputed readiness".into(),
            },
        };
        let state = DesktopProviders::default().view(state).unwrap();
        assert_eq!(state.state.settings.active, choice);
        assert!(!state.claude_connection.ready);
        assert!(matches!(
            state.state.dispatch,
            DispatchResolution::Blocked { .. }
        ));
        let models: Vec<_> = state
            .state
            .catalog
            .models
            .iter()
            .filter(|model| model.key.provider_id == "claude")
            .collect();
        assert_eq!(models.len(), 3);
        assert!(models.iter().all(|model| !model.ready));
        assert_eq!(state.story_memory.provider_id, "codex");
        assert_eq!(state.story_memory.model_id, CODEX_MAINTENANCE_MODEL_ID);
        assert_eq!(
            state.story_memory.reasoning.as_deref(),
            Some(CODEX_MAINTENANCE_REASONING)
        );
    }

    #[test]
    fn claude_saved_author_choice_preserves_exact_explicit_model_and_traits() {
        let binding = ProviderBinding::claude_author_runtime(
            "claude-sonnet-5",
            "high",
            "2.1.220",
            &"a".repeat(64),
        );
        let mut choice = ModelSelection {
            provider_id: "claude".into(),
            model_id: "claude-sonnet-5".into(),
            reasoning: None,
            service_tier: None,
        };
        assert!(binding_matches_author_choice(&binding, &choice));
        choice.reasoning = Some("high".into());
        assert!(binding_matches_author_choice(&binding, &choice));
        choice.reasoning = Some("xhigh".into());
        assert!(!binding_matches_author_choice(&binding, &choice));
        choice.reasoning = Some("high".into());
        choice.service_tier = Some("priority".into());
        assert!(!binding_matches_author_choice(&binding, &choice));
        choice.service_tier = None;
        choice.model_id = "claude-opus-5".into();
        assert!(!binding_matches_author_choice(&binding, &choice));
        assert!(!binding_matches_choice(&binding, &choice));
    }

    #[test]
    fn author_defaults_resolve_once_but_explicit_traits_and_legacy_bindings_remain_exact() {
        let binding = ProviderBinding::codex_author_runtime(
            "gpt-5.4-mini",
            "medium",
            None,
            "9.1",
            &"a".repeat(64),
            &"b".repeat(64),
        );
        let mut choice = ModelSelection {
            provider_id: "codex".into(),
            model_id: "gpt-5.4-mini".into(),
            reasoning: None,
            service_tier: None,
        };
        assert!(binding_matches_author_choice(&binding, &choice));
        choice.reasoning = Some("high".into());
        assert!(!binding_matches_author_choice(&binding, &choice));
        choice.model_id = "gpt-5.6-luna".into();
        choice.reasoning = None;
        assert!(!binding_matches_author_choice(
            &ProviderBinding::codex_luna_historical(),
            &choice
        ));
        assert!(!binding_matches_author_choice(&binding, &choice));
    }
    use std::collections::HashMap;
    use std::sync::Mutex;
    use webnovel_core::library::Library;
    use webnovel_core::providers::credentials::{CredentialStore, CredentialTarget, SecretValue};
    use webnovel_core::providers::endpoints::EndpointProfileDraft;

    #[derive(Default)]
    struct SyntheticCredentialStore {
        values: Mutex<HashMap<CredentialTarget, Vec<u8>>>,
    }

    impl SyntheticCredentialStore {
        fn target() -> CredentialTarget {
            CredentialTarget::parse("WebnovelStudioV3/Profile/00000000-0000-0000-0000-000000000042")
                .expect("synthetic target is canonical")
        }

        fn insert(&self, target: CredentialTarget, value: &[u8]) {
            self.values.lock().unwrap().insert(target, value.to_vec());
        }
    }

    impl CredentialStore for SyntheticCredentialStore {
        fn read(&self, target: &CredentialTarget) -> CoreResult<Option<SecretValue>> {
            self.values
                .lock()
                .unwrap()
                .get(target)
                .cloned()
                .map(SecretValue::new)
                .transpose()
        }

        fn write_new(&self, _secret: &[u8]) -> CoreResult<CredentialTarget> {
            Err(CoreError::new(
                "TestOnly",
                "synthetic readiness store does not write credentials",
            ))
        }

        fn delete(&self, target: &CredentialTarget) -> CoreResult<()> {
            self.values.lock().unwrap().remove(target);
            Ok(())
        }
    }

    #[test]
    fn requested_traits_are_exact_and_unchecked_runtime_never_enables_codex() {
        let choice = ModelSelection {
            provider_id: "codex".into(),
            model_id: CODEX_MAINTENANCE_MODEL_ID.into(),
            reasoning: Some(CODEX_MAINTENANCE_REASONING.into()),
            service_tier: Some(CODEX_SERVICE_TIER.into()),
        };
        assert!(is_supported_choice(&choice));
        assert!(!is_supported_choice(&ModelSelection {
            reasoning: None,
            ..choice.clone()
        }));
        assert!(!is_supported_choice(&ModelSelection {
            model_id: webnovel_core::context::packet::CODEX_LUNA_MODEL_ID.into(),
            ..choice.clone()
        }));
        let state = ProviderState {
            settings: webnovel_core::providers::preferences::ModelSettings {
                revision: "0".into(),
                active: choice,
                favorites: vec![],
            },
            catalog: webnovel_core::providers::catalog::built_in_catalog(),
            dispatch: DispatchResolution::Blocked {
                detail: "Unavailable".into(),
            },
        };
        let state = DesktopProviders::default().view(state).unwrap();
        assert!(!state.codex_connection.ready);
        assert!(matches!(
            state.state.dispatch,
            DispatchResolution::Blocked { .. }
        ));
    }

    #[cfg(windows)]
    fn synthetic_codex_state() -> ProviderState {
        ProviderState {
            settings: webnovel_core::providers::preferences::ModelSettings {
                revision: "0".into(),
                active: ModelSelection {
                    provider_id: "codex".into(),
                    model_id: "gpt-5.6-luna".into(),
                    reasoning: Some("xhigh".into()),
                    service_tier: Some("priority".into()),
                },
                favorites: vec![],
            },
            catalog: webnovel_core::providers::catalog::built_in_catalog(),
            dispatch: DispatchResolution::Blocked {
                detail: "Unavailable".into(),
            },
        }
    }

    #[cfg(windows)]
    #[test]
    fn codex_publication_stays_fenced_until_persistence_callback_finishes() {
        let runtime = DesktopProviders::default();
        runtime.begin_codex_check().unwrap();
        runtime
            .finish_codex_check(
                Ok(7_u8),
                |value| {
                    assert_eq!(*value, 7);
                    let during_persistence = runtime.view(synthetic_codex_state()).unwrap();
                    assert!(during_persistence.codex_connection.checking);
                    assert!(!during_persistence.codex_connection.ready);
                    Ok(())
                },
                |value, state| {
                    assert_eq!(value, 7);
                    assert!(!state.checking);
                },
            )
            .unwrap();
        let after = runtime.view(synthetic_codex_state()).unwrap();
        assert!(!after.codex_connection.checking);
        assert!(!after.codex_connection.ready);
    }

    #[cfg(windows)]
    #[test]
    fn failed_codex_publication_clears_the_checking_fence_and_connection() {
        let runtime = DesktopProviders::default();
        runtime.begin_codex_check().unwrap();
        let error = runtime.finish_codex_check(
            Ok(7_u8),
            |_| {
                Err(CoreError::new(
                    "PersistenceUnavailable",
                    "synthetic failure",
                ))
            },
            |_, _| panic!("a failed publication must not publish a connection"),
        );
        assert_eq!(error.unwrap_err().code, "PersistenceUnavailable");
        let after = runtime.view(synthetic_codex_state()).unwrap();
        assert!(!after.codex_connection.checking);
        assert!(!after.codex_connection.ready);
        assert!(after.codex_connection.detail.contains("could not be saved"));
    }

    #[test]
    fn endpoint_readiness_requires_a_usable_referenced_credential() {
        let root = std::env::temp_dir().join(format!(
            "wns-provider-runtime-readiness-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let mut library = Library::open(&root).unwrap();
        let store = SyntheticCredentialStore::default();
        let target = SyntheticCredentialStore::target();
        library
            .save_endpoint_profiles(
                "0",
                vec![EndpointProfileDraft {
                    id: None,
                    label: "Synthetic endpoint".into(),
                    base_url: "https://example.test".into(),
                    enabled: true,
                    json_mode: false,
                    credential_ref: Some(target.as_str().to_owned()),
                    manual_model_ids: vec!["story-model".into()],
                }],
            )
            .unwrap();
        let profile = library.endpoint_profiles().unwrap().profiles[0].clone();
        library
            .save_model_settings(
                "0",
                ModelSelection {
                    provider_id: profile.id.clone(),
                    model_id: "story-model".into(),
                    reasoning: None,
                    service_tier: None,
                },
                vec![],
            )
            .unwrap();

        let runtime = DesktopProviders::default();
        let unavailable = runtime.view_library_with_store(&library, &store).unwrap();
        let unavailable_model = unavailable
            .state
            .catalog
            .models
            .iter()
            .find(|model| model.key.provider_id == profile.id)
            .unwrap();
        assert!(!unavailable_model.ready);
        assert!(unavailable_model.status_detail.contains("API key"));
        assert!(matches!(
            unavailable.state.dispatch,
            DispatchResolution::Blocked { .. }
        ));

        store.insert(target, b"synthetic-key");
        let available = runtime.view_library_with_store(&library, &store).unwrap();
        let available_model = available
            .state
            .catalog
            .models
            .iter()
            .find(|model| model.key.provider_id == profile.id)
            .unwrap();
        assert!(available_model.ready);
        assert!(matches!(
            available.state.dispatch,
            DispatchResolution::OpenAiCompatible { .. }
        ));

        // A configured writing model does not qualify the fixed maintenance
        // model, and changing maintenance never changes the author selection.
        library
            .save_story_memory_provider("0", &profile.id)
            .unwrap();
        let missing_astra = runtime.view_library_with_store(&library, &store).unwrap();
        assert!(!missing_astra.story_memory.ready);
        assert!(
            missing_astra
                .story_memory
                .detail
                .contains(CODEX_MAINTENANCE_MODEL_ID)
        );
        library
            .save_endpoint_profiles(
                "1",
                vec![EndpointProfileDraft {
                    id: Some(profile.id.clone()),
                    label: profile.label,
                    base_url: profile.base_url,
                    enabled: true,
                    json_mode: false,
                    credential_ref: profile.credential_ref,
                    manual_model_ids: vec!["story-model".into(), CODEX_MAINTENANCE_MODEL_ID.into()],
                }],
            )
            .unwrap();
        let memory_ready = runtime.view_library_with_store(&library, &store).unwrap();
        assert!(memory_ready.story_memory.ready);
        assert!(!memory_ready.codex_connection.ready);
        assert_eq!(
            memory_ready.story_memory.reasoning.as_deref(),
            Some(CODEX_MAINTENANCE_REASONING)
        );
        assert!(memory_ready.story_memory.service_tier.is_none());
        assert_eq!(memory_ready.state.settings.active.model_id, "story-model");
        store.delete(&SyntheticCredentialStore::target()).unwrap();
        let unavailable_memory = runtime.view_library_with_store(&library, &store).unwrap();
        assert!(!unavailable_memory.story_memory.ready);
        assert_eq!(unavailable_memory.story_memory.provider_id, profile.id);
        library.save_story_memory_provider("1", "mock").unwrap();
        let local = runtime.view_library_with_store(&library, &store).unwrap();
        assert!(local.story_memory.ready);
        assert_eq!(local.state.settings.active.model_id, "story-model");

        drop(library);
        std::fs::remove_dir_all(root).unwrap();
    }
}
