//! Session-local readiness and Stop ownership; saved preferences remain in Library.
use crate::endpoint_commands::credential_available;
use serde::Serialize;
#[cfg(windows)]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use webnovel_core::context::packet::ProviderBinding;
#[cfg(windows)]
use webnovel_core::projects::discussions::RunOwner;
use webnovel_core::projects::memory::MemoryOwner;
use webnovel_core::projects::{CoreError, CoreResult};
#[cfg(windows)]
use webnovel_core::providers::claude_runtime::ClaudeConnection;
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
    checking: bool,
    #[cfg(windows)]
    connection: Option<CodexConnection>,
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

pub fn memory_selection(provider_id: &str) -> ModelSelection {
    if provider_id == "mock" {
        ModelSelection::local_mock()
    } else {
        ModelSelection {
            provider_id: provider_id.into(),
            model_id: "gpt-5.6-luna".into(),
            reasoning: Some("xhigh".into()),
            service_tier: (provider_id == "codex").then(|| "priority".into()),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionView {
    ready: bool,
    memory_ready: bool,
    detail: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeConnectionView {
    ready: bool,
    detail: String,
}

pub fn is_supported_choice(choice: &ModelSelection) -> bool {
    choice.provider_id == "codex"
        && choice.model_id == "gpt-5.6-luna"
        && choice.reasoning.as_deref() == Some("xhigh")
        && choice.service_tier.as_deref() == Some("priority")
}

pub fn binding_matches_choice(binding: &ProviderBinding, choice: &ModelSelection) -> bool {
    binding_matches_saved_model(binding, choice)
        && binding.reasoning == choice.reasoning
        && binding.service_tier == choice.service_tier
}

pub fn binding_matches_author_choice(binding: &ProviderBinding, choice: &ModelSelection) -> bool {
    binding_matches_choice(binding, choice)
        || (matches!(
            binding.profile_version.as_str(),
            webnovel_core::providers::codex_profile::CODEX_AUTHOR_PROFILE_VERSION
                | webnovel_core::providers::claude_profile::CLAUDE_PROFILE_VERSION
        ) && binding_matches_saved_model(binding, choice)
            && choice
                .reasoning
                .as_ref()
                .is_none_or(|value| binding.reasoning.as_ref() == Some(value))
            && choice
                .service_tier
                .as_ref()
                .is_none_or(|value| binding.service_tier.as_ref() == Some(value)))
}

pub fn binding_matches_saved_model(binding: &ProviderBinding, choice: &ModelSelection) -> bool {
    binding.validate().is_ok()
        && binding.provider_id == choice.provider_id
        && binding.model_id == choice.model_id
}

#[cfg(windows)]
pub fn connection_binding(connection: &CodexConnection) -> ProviderBinding {
    ProviderBinding::codex_luna_runtime(connection.version(), connection.fingerprint())
}

#[cfg(windows)]
pub fn connection_author_binding(
    connection: &CodexConnection,
    choice: &ModelSelection,
) -> CoreResult<ProviderBinding> {
    if !connection.catalog().supports(choice) {
        return Err(CoreError::new(
            "ProviderUnavailable",
            "This model or its selected settings are absent from the checked Codex catalog. Check the connection in Settings.",
        ));
    }
    let model = connection
        .catalog()
        .models
        .iter()
        .find(|model| model.model_id == choice.model_id)
        .ok_or_else(unavailable)?;
    let binding = ProviderBinding::codex_author_runtime(
        &choice.model_id,
        choice
            .reasoning
            .as_deref()
            .or(model.default_reasoning.as_deref())
            .ok_or_else(unavailable)?,
        choice
            .service_tier
            .as_deref()
            .or(model.default_service_tier.as_deref()),
        connection.version(),
        connection.fingerprint(),
        &model.fingerprint()?,
    );
    binding.validate().map_err(|_| unavailable())?;
    Ok(binding)
}

#[cfg(windows)]
pub fn connection_matches_binding(connection: &CodexConnection, binding: &ProviderBinding) -> bool {
    binding == &connection_binding(connection)
        || (binding.profile_version
            == webnovel_core::providers::codex_profile::CODEX_AUTHOR_PROFILE_VERSION
            && connection_author_binding(
                connection,
                &ModelSelection {
                    provider_id: binding.provider_id.clone(),
                    model_id: binding.model_id.clone(),
                    reasoning: binding.reasoning.clone(),
                    service_tier: binding.service_tier.clone(),
                },
            )
            .is_ok_and(|expected| expected == *binding))
}

#[cfg(windows)]
pub fn claude_binding_for_choice(
    connection: &ClaudeConnection,
    choice: &ModelSelection,
) -> CoreResult<ProviderBinding> {
    use webnovel_core::providers::claude_profile::ClaudeLaunchProfile;
    let unavailable = || {
        CoreError::new(
            "ProviderUnavailable",
            "This Claude model or its settings are unavailable. Check the connection in Settings.",
        )
    };
    if choice.provider_id != "claude"
        || choice.service_tier.is_some()
        || !connection.permits_model(&choice.model_id)
    {
        return Err(unavailable());
    }
    let effort = choice.reasoning.as_deref().unwrap_or("high");
    ClaudeLaunchProfile::for_version(connection.version(), &choice.model_id, Some(effort))
        .map_err(|_| unavailable())?;
    let binding = ProviderBinding::claude_author_runtime(
        &choice.model_id,
        effort,
        connection.version(),
        connection.fingerprint(),
    );
    binding.validate().map_err(|_| unavailable())?;
    Ok(binding)
}

#[cfg(windows)]
pub fn claude_connection_matches_binding(
    connection: &ClaudeConnection,
    binding: &ProviderBinding,
) -> bool {
    claude_binding_for_choice(
        connection,
        &ModelSelection {
            provider_id: binding.provider_id.clone(),
            model_id: binding.model_id.clone(),
            reasoning: binding.reasoning.clone(),
            service_tier: binding.service_tier.clone(),
        },
    )
    .is_ok_and(|expected| expected == *binding)
}
fn unavailable() -> CoreError {
    CoreError::new(
        "ProviderUnavailable",
        "Check the Codex connection in Settings before sending this request.",
    )
}

impl DesktopProviders {
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
                memory.detail = if memory.ready { "Refresh sends one request through Codex using GPT-5.6 Luna with Extra high reasoning." } else { "Check Codex in Settings. Story memory requires GPT-5.6 Luna with Extra high reasoning and Fast service." }.into();
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
                    "Add gpt-5.6-luna to this connection's models. The service must support xhigh reasoning."
                } else if !memory.ready {
                    "The saved API key is unavailable. Re-enter it in Settings before refreshing."
                } else {
                    "Configured to request GPT-5.6 Luna with xhigh reasoning. The API service must support these settings."
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
                model_id: "gpt-5.6-luna".into(),
                reasoning: Some("xhigh".into()),
                service_tier: Some("priority".into()),
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
                model_id: "gpt-5.6-luna".into(),
                reasoning: Some("xhigh".into()),
                service_tier: Some("priority".into()),
                ready: memory_ready,
                detail: "Check Codex in Settings before refreshing story memory.".into(),
            },
            codex_connection: ConnectionView {
                ready,
                memory_ready,
                detail,
            },
        })
    }

    pub fn check_connection(&self) -> CoreResult<()> {
        // No process operation holds the Stop registry lock.
        {
            let mut state = self.lock()?;
            if state.checking {
                return Err(CoreError::new(
                    "ConnectionCheckRunning",
                    "A Codex connection check is already running.",
                ));
            }
            state.checking = true;
            #[cfg(windows)]
            {
                state.connection = None;
            }
        }
        #[cfg(windows)]
        {
            let checked = CodexConnection::check_installed();
            let mut state = self.lock()?;
            state.checking = false;
            match checked {
                Ok(connection) => {
                    state.connection = Some(connection);
                    state.detail = Some("Signed in through Codex. Available models and traits were read from this installation.".into());
                }
                Err(error) => {
                    state.detail = Some(error.detail);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let mut state = self.lock()?;
            state.checking = false;
            state.detail =
                Some("This Codex connection is currently available on Windows only.".into());
        }
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
    pub fn checked_catalog(
        &self,
    ) -> CoreResult<Option<webnovel_core::providers::codex_catalog::CodexCatalog>> {
        #[cfg(windows)]
        {
            Ok(self
                .lock()?
                .connection
                .as_ref()
                .map(|connection| connection.catalog().clone()))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
    pub fn invalidate_connection(&self) -> CoreResult<()> {
        let mut state = self.lock()?;
        #[cfg(windows)]
        {
            state.connection = None;
        }
        state.detail = Some(
            "The discovered Codex models could not be saved. Check the connection again.".into(),
        );
        Ok(())
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
        assert_eq!(state.story_memory.model_id, "gpt-5.6-luna");
        assert_eq!(state.story_memory.reasoning.as_deref(), Some("xhigh"));
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
            model_id: "gpt-5.6-luna".into(),
            reasoning: Some("xhigh".into()),
            service_tier: Some("priority".into()),
        };
        assert!(is_supported_choice(&choice));
        assert!(!is_supported_choice(&ModelSelection {
            reasoning: None,
            ..choice.clone()
        }));
        assert!(!is_supported_choice(&ModelSelection {
            model_id: "gpt-6-astra".into(),
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
        let missing_luna = runtime.view_library_with_store(&library, &store).unwrap();
        assert!(!missing_luna.story_memory.ready);
        assert!(missing_luna.story_memory.detail.contains("gpt-5.6-luna"));
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
                    manual_model_ids: vec!["story-model".into(), "gpt-5.6-luna".into()],
                }],
            )
            .unwrap();
        let memory_ready = runtime.view_library_with_store(&library, &store).unwrap();
        assert!(memory_ready.story_memory.ready);
        assert!(!memory_ready.codex_connection.ready);
        assert_eq!(
            memory_ready.story_memory.reasoning.as_deref(),
            Some("xhigh")
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
