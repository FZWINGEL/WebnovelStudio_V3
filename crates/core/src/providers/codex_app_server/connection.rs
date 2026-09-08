//! Checked process reuse and admission; application jobs still own all story state.
#![cfg(windows)]

use super::auth::read_discovered_auth;
use super::launch::AppServerLaunch;
use super::protocol::ThreadStartConfig;
use super::runtime::{AppServerAuth, AppServerConnection, AppServerHealth, AppServerReservation};
use super::{AppServerRuntimeIdentity, MAINTENANCE_PROFILE};
use crate::context::packet::{
    CODEX_MAINTENANCE_MODEL_ID, CODEX_MAINTENANCE_REASONING, CODEX_SERVICE_TIER, ProviderBinding,
};
use crate::projects::{CoreError, CoreResult};
use crate::providers::codex_runtime::CodexConnection;
use crate::providers::preferences::ModelSelection;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ManagedAppServer(Arc<CheckedServer>);

struct CheckedServer {
    checked: CodexConnection,
    identity: AppServerRuntimeIdentity,
    cwd: String,
    token_fingerprint: String,
    process: Mutex<Option<AppServerConnection>>,
}

/// Reserved before durable author-request acceptance. Dropping it releases the
/// slot; it never represents queued permission to generate in the future.
pub struct AppServerRequest {
    pub binding: ProviderBinding,
    pub reservation: AppServerReservation,
    pub thread: ThreadStartConfig,
}

impl ManagedAppServer {
    pub fn start(checked: CodexConnection, initial_choice: &ModelSelection) -> CoreResult<Self> {
        let auth = read_discovered_auth()?;
        let token_fingerprint = auth.token_fingerprint();
        let launch = AppServerLaunch::prepare(
            checked.executable(),
            checked.version(),
            checked.fingerprint(),
            checked.catalog(),
            initial_choice,
            auth,
        )?;
        let cwd = launch
            .invocation
            .cwd
            .to_str()
            .ok_or_else(unavailable)?
            .to_owned();
        let auth = AppServerAuth {
            method: super::auth::ExternalAuth::login_method().into(),
            params: launch.auth.login_params()?,
        };
        let process =
            AppServerConnection::start_with_auth(launch.invocation, launch.resources, Some(auth))?;
        Ok(Self(Arc::new(CheckedServer {
            checked,
            identity: launch.identity,
            cwd,
            token_fingerprint,
            process: Mutex::new(Some(process)),
        })))
    }

    pub fn healthy(&self) -> bool {
        self.0.process.lock().is_ok_and(|process| {
            process
                .as_ref()
                .is_some_and(|process| process.health() == AppServerHealth::Ready)
        })
    }

    pub fn active_count(&self) -> usize {
        self.0
            .process
            .lock()
            .map(|process| {
                process
                    .as_ref()
                    .map_or(0, AppServerConnection::active_count)
            })
            .unwrap_or(usize::MAX)
    }

    pub fn author_binding(&self, choice: &ModelSelection) -> CoreResult<ProviderBinding> {
        let catalog = self.0.checked.catalog();
        if !catalog.supports(choice) {
            return Err(unavailable());
        }
        let model = catalog.model(&choice.model_id).ok_or_else(unavailable)?;
        let binding = ProviderBinding::codex_app_server_author_runtime(
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
            self.0.checked.version(),
            self.0.checked.fingerprint(),
            &model.fingerprint()?,
            self.0.identity.clone(),
        );
        binding.validate().map_err(|_| unavailable())?;
        Ok(binding)
    }

    pub fn maintenance_binding(&self) -> CoreResult<ProviderBinding> {
        let mut binding = self.author_binding(&ModelSelection {
            provider_id: "codex".into(),
            model_id: CODEX_MAINTENANCE_MODEL_ID.into(),
            reasoning: Some(CODEX_MAINTENANCE_REASONING.into()),
            service_tier: Some(CODEX_SERVICE_TIER.into()),
        })?;
        binding.profile_version = MAINTENANCE_PROFILE.into();
        binding.validate().map_err(|_| unavailable())?;
        Ok(binding)
    }

    pub fn reserve(&self, binding: &ProviderBinding) -> CoreResult<AppServerRequest> {
        if !self.0.checked.is_current_installation()? {
            return Err(CoreError::new(
                "CodexVersionChanged",
                "Codex was updated. Current replies can finish; check the connection again before sending new work.",
            ));
        }
        let expected = if binding.profile_version == MAINTENANCE_PROFILE {
            self.maintenance_binding()?
        } else {
            self.author_binding(&ModelSelection {
                provider_id: binding.provider_id.clone(),
                model_id: binding.model_id.clone(),
                reasoning: binding.reasoning.clone(),
                service_tier: binding.service_tier.clone(),
            })?
        };
        if *binding != expected {
            return Err(unavailable());
        }
        // A newly selected account never enters an existing server. Only the
        // read-only external token adapter touches the author's auth file.
        let current_auth = read_discovered_auth()?;
        if current_auth.account_hash() != self.0.identity.account_sha256 {
            return Err(CoreError::new(
                "CodexAccountChanged",
                "The Codex account changed. Let current replies finish, then check the connection again.",
            ));
        }
        if current_auth.token_fingerprint() != self.0.token_fingerprint {
            return Err(CoreError::new(
                "CodexSignInChanged",
                "The Codex sign-in was refreshed. Let current replies finish, then check the connection again.",
            ));
        }
        let process = self.0.process.lock().map_err(|_| unavailable())?;
        let reservation = process.as_ref().ok_or_else(unavailable)?.try_reserve()?;
        Ok(AppServerRequest {
            binding: binding.clone(),
            reservation,
            thread: ThreadStartConfig {
                cwd: Some(self.0.cwd.clone()),
                base_instructions: None,
                developer_instructions: None,
                model: binding.model_id.clone(),
                reasoning_effort: binding.reasoning.clone(),
                service_tier: binding
                    .service_tier
                    .clone()
                    .unwrap_or_else(|| "default".into()),
            },
        })
    }

    /// Call after application admission is closed and all request owners settle.
    pub fn shutdown_idle(&self) -> CoreResult<()> {
        let process = {
            let mut slot = self.0.process.lock().map_err(|_| unavailable())?;
            if slot
                .as_ref()
                .is_some_and(|process| process.active_count() != 0)
            {
                return Err(CoreError::new(
                    "ProviderBusy",
                    "Codex still owns active replies.",
                ));
            }
            slot.take()
        };
        process.map_or(Ok(()), AppServerConnection::shutdown)
    }
}

fn unavailable() -> CoreError {
    CoreError::new(
        "ProviderUnavailable",
        "This app-server model or connection is unavailable. Check Codex in Settings.",
    )
}
