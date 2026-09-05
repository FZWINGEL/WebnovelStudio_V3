//! Session-local readiness and Stop ownership; saved preferences remain in Library.
use serde::Serialize;
#[cfg(windows)]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use webnovel_core::projects::{CoreError, CoreResult};
use webnovel_core::providers::{
    catalog::{DispatchResolution, ProviderState},
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
    #[cfg(windows)]
    stops: HashMap<(String, String, String), StopSignal>,
    detail: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProviderState {
    #[serde(flatten)]
    state: ProviderState,
    codex_connection: ConnectionView,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionView {
    ready: bool,
    detail: String,
}

pub fn is_supported_choice(choice: &ModelSelection) -> bool {
    choice.provider_id == "codex"
        && choice.model_id == "gpt-5.6-luna"
        && choice.reasoning.as_deref() == Some("max")
        && choice.service_tier.as_deref() == Some("priority")
}
fn unavailable() -> CoreError {
    CoreError::new(
        "ProviderUnavailable",
        "Check the Codex connection in Settings before sending this request.",
    )
}

impl DesktopProviders {
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
            if let Some(model) = state.catalog.models.iter_mut().find(|model| {
                model.key.provider_id == "codex" && model.key.model_id == "gpt-5.6-luna"
            }) {
                model.ready = true;
                model.status_detail = "Connected through Codex. Available with Max reasoning and Fast response speed.".into();
            }
            if state.settings.active.provider_id == "codex" {
                state.dispatch = if is_supported_choice(&state.settings.active) {
                    DispatchResolution::CodexCli {
                        detail: "Uses your Codex sign-in. Sending starts one live response.".into(),
                    }
                } else {
                    DispatchResolution::Blocked { detail: "This connection currently supports GPT-5.6-Luna with Max reasoning and Fast response speed. Choose those settings to send.".into() }
                };
            }
        }
        Ok(DesktopProviderState {
            state,
            codex_connection: ConnectionView { ready, detail },
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
                    state.detail = Some("Signed in through Codex. GPT-5.6-Luna is available with Max reasoning and Fast response speed.".into());
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

    #[cfg(windows)]
    pub fn connection(&self) -> CoreResult<CodexConnection> {
        self.lock()?.connection.clone().ok_or_else(unavailable)
    }
    #[cfg(windows)]
    pub fn register(
        &self,
        owner: &webnovel_core::projects::discussions::RunOwner,
    ) -> CoreResult<StopSignal> {
        let mut state = self.lock()?;
        let key = (
            owner.project_id.clone(),
            owner.operation_namespace.clone(),
            owner.run_id.clone(),
        );
        if state.stops.contains_key(&key) {
            return Err(CoreError::new(
                "RunAlreadyStarted",
                "This response already has a worker.",
            ));
        }
        let stop = StopSignal::new();
        state.stops.insert(key, stop.clone());
        Ok(stop)
    }
    pub fn stop(&self, owner: &webnovel_core::projects::discussions::RunOwner) {
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
    #[cfg(windows)]
    pub fn release(&self, owner: &webnovel_core::projects::discussions::RunOwner) {
        if let Ok(mut state) = self.lock() {
            state.stops.remove(&(
                owner.project_id.clone(),
                owner.operation_namespace.clone(),
                owner.run_id.clone(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requested_traits_are_exact_and_unchecked_runtime_never_enables_codex() {
        let choice = ModelSelection {
            provider_id: "codex".into(),
            model_id: "gpt-5.6-luna".into(),
            reasoning: Some("max".into()),
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
}
