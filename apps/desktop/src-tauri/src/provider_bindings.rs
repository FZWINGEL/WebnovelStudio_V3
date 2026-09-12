//! Which saved model a provider binding corresponds to.
//!
//! Split out of `provider_runtime`, which was 1,647 lines. These are pure
//! functions over a `ProviderBinding`, a `ModelSelection` and a live
//! connection: no state, no locks, no runtime. The runtime's own state and its
//! `impl DesktopProviders` stay behind, and thirteen call sites elsewhere in
//! the shell already reach these through the runtime module.

use serde::Serialize;
use webnovel_core::projects::{CoreError, CoreResult};
use webnovel_core::context::packet::{
    CODEX_MAINTENANCE_MODEL_ID, CODEX_MAINTENANCE_REASONING, CODEX_SERVICE_TIER, ProviderBinding,
};
use webnovel_core::providers::codex_runtime::CodexConnection;
use webnovel_core::providers::preferences::{
    ModelSelection, STORY_MEMORY_MODEL_ID, STORY_MEMORY_REASONING,
};
#[cfg(windows)]
use webnovel_core::providers::claude_runtime::ClaudeConnection;

use crate::provider_runtime::unavailable;

pub fn memory_selection(provider_id: &str) -> ModelSelection {
    if provider_id == "mock" {
        ModelSelection::local_mock()
    } else {
        ModelSelection {
            provider_id: provider_id.into(),
            model_id: STORY_MEMORY_MODEL_ID.into(),
            reasoning: Some(STORY_MEMORY_REASONING.into()),
            service_tier: (provider_id == "codex").then(|| CODEX_SERVICE_TIER.into()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectionView {
    pub(crate) ready: bool,
    pub(crate) checked: bool,
    pub(crate) checking: bool,
    pub(crate) memory_ready: bool,
    pub(crate) detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeConnectionView {
    pub(crate) ready: bool,
    pub(crate) detail: String,
}

pub fn is_supported_choice(choice: &ModelSelection) -> bool {
    choice.provider_id == "codex"
        && choice.model_id == CODEX_MAINTENANCE_MODEL_ID
        && choice.reasoning.as_deref() == Some(CODEX_MAINTENANCE_REASONING)
        && choice.service_tier.as_deref() == Some(CODEX_SERVICE_TIER)
}

pub fn is_legacy_maintenance_choice(choice: &ModelSelection) -> bool {
    choice.provider_id == "codex"
        && choice.model_id == webnovel_core::context::packet::CODEX_LUNA_MODEL_ID
        && choice.reasoning.as_deref()
            == Some(webnovel_core::context::packet::CODEX_REASONING_EFFORT)
        && choice.service_tier.as_deref() == Some(CODEX_SERVICE_TIER)
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
                | webnovel_core::providers::codex_app_server::AUTHOR_PROFILE
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
    ProviderBinding::codex_maintenance_runtime(connection.version(), connection.fingerprint())
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
        || binding
            == &ProviderBinding::codex_luna_runtime(connection.version(), connection.fingerprint())
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
