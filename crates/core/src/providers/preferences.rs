//! Persistent, nonsecret model selection preferences.
//!
//! These values represent author intent only.  They do not prove that a
//! provider is installed, authenticated, isolated, or qualified.  The
//! app-local Library stores them separately from project databases.

use super::catalog::{
    DispatchResolution, ProviderState, built_in_catalog, catalog_with_endpoints_and_codex,
    find_model, validate_catalog, validate_selection,
};
use super::codex_catalog::CodexCatalog;
use super::endpoints::EndpointProfilesSettings;
use crate::projects::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MODEL_SETTINGS_SCHEMA_VERSION: u32 = 1;
pub const MODEL_SETTINGS_KEY: &str = "model-selection-v1";
pub const STORY_MEMORY_SETTINGS_SCHEMA_VERSION: u32 = 1;
pub const STORY_MEMORY_SETTINGS_KEY: &str = "story-memory-provider-v1";
pub const STORY_MEMORY_CODEX_PROVIDER_ID: &str = "codex";
pub const STORY_MEMORY_MOCK_PROVIDER_ID: &str = "mock";
pub const STORY_MEMORY_MODEL_ID: &str = "gpt-5.6-luna";
pub const STORY_MEMORY_REASONING: &str = "xhigh";
pub const MAX_FAVORITES: usize = 32;
pub const DEFAULT_REVISION: &str = "0";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelKey {
    pub provider_id: String,
    pub model_id: String,
}

impl ModelKey {
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelSelection {
    pub provider_id: String,
    pub model_id: String,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
}

impl ModelSelection {
    pub fn key(&self) -> ModelKey {
        ModelKey::new(self.provider_id.clone(), self.model_id.clone())
    }

    pub fn local_mock() -> Self {
        Self {
            provider_id: super::catalog::MOCK_PROVIDER_ID.to_owned(),
            model_id: super::catalog::MOCK_MODEL_ID.to_owned(),
            reasoning: None,
            service_tier: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelSettings {
    pub revision: String,
    pub active: ModelSelection,
    pub favorites: Vec<ModelKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredModelSettings {
    pub active: ModelSelection,
    pub favorites: Vec<ModelKey>,
}

/// The provider selected for story-memory and summary maintenance.  The
/// model and reasoning level are deliberately not user-editable here: live
/// maintenance always uses GPT-5.6 Luna with Extra high reasoning.  Endpoint
/// profile IDs are kept as opaque provider IDs and never contain credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryMemorySettings {
    pub revision: String,
    pub provider_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredStoryMemorySettings {
    pub provider_id: String,
}

impl Default for StoryMemorySettings {
    fn default() -> Self {
        Self {
            revision: DEFAULT_REVISION.to_owned(),
            provider_id: STORY_MEMORY_CODEX_PROVIDER_ID.to_owned(),
        }
    }
}

impl StoryMemorySettings {
    pub fn stored(&self) -> StoredStoryMemorySettings {
        StoredStoryMemorySettings {
            provider_id: self.provider_id.clone(),
        }
    }

    pub fn from_stored(revision: String, stored: StoredStoryMemorySettings) -> Self {
        Self {
            revision,
            provider_id: stored.provider_id,
        }
    }

    pub fn validate(&self, endpoints: &EndpointProfilesSettings) -> CoreResult<()> {
        parse_revision(&self.revision)?;
        validate_story_memory_provider_id(&self.provider_id, endpoints)
    }
}

pub(crate) fn validate_story_memory_provider_id(
    provider_id: &str,
    endpoints: &EndpointProfilesSettings,
) -> CoreResult<()> {
    if matches!(
        provider_id,
        STORY_MEMORY_CODEX_PROVIDER_ID | STORY_MEMORY_MOCK_PROVIDER_ID
    ) {
        return Ok(());
    }
    if !provider_id.starts_with(super::endpoints::ENDPOINT_PROVIDER_PREFIX) {
        return Err(CoreError::new(
            "InvalidStoryMemoryProvider",
            "Choose Codex, the local test provider, or a saved OpenAI-compatible endpoint.",
        ));
    }
    if endpoints.find(provider_id).is_none() {
        return Err(CoreError::new(
            "UnknownStoryMemoryProvider",
            "The selected story-memory endpoint is no longer saved.",
        ));
    }
    Ok(())
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            revision: DEFAULT_REVISION.to_owned(),
            active: ModelSelection::local_mock(),
            favorites: Vec::new(),
        }
    }
}

impl ModelSettings {
    pub fn stored(&self) -> StoredModelSettings {
        StoredModelSettings {
            active: self.active.clone(),
            favorites: self.favorites.clone(),
        }
    }

    pub fn from_stored(revision: String, stored: StoredModelSettings) -> Self {
        Self {
            revision,
            active: stored.active,
            favorites: stored.favorites,
        }
    }

    pub fn validate(&self) -> CoreResult<()> {
        let catalog = built_in_catalog();
        validate_catalog(&catalog)?;
        validate_settings_against_catalog(self, &catalog)
    }
}

pub fn validate_settings_against_catalog(
    settings: &ModelSettings,
    catalog: &super::catalog::CatalogSnapshot,
) -> CoreResult<()> {
    parse_revision(&settings.revision)?;
    validate_selection(catalog, &settings.active)?;
    if settings.favorites.len() > MAX_FAVORITES {
        return Err(CoreError::new(
            "TooManyFavorites",
            "A maximum of 32 favorite models can be saved.",
        ));
    }
    let mut seen = HashSet::new();
    for favorite in &settings.favorites {
        find_model(catalog, favorite)?;
        if !seen.insert(favorite) {
            return Err(CoreError::new(
                "DuplicateFavorite",
                "A model cannot appear more than once in favorites.",
            ));
        }
    }
    Ok(())
}

/// Validate a stored selection while allowing an unchanged active Codex
/// selection whose model or explicit trait disappeared from the latest
/// discovery.  New choices still use the strict validator above.
pub(crate) fn validate_settings_preserving_unavailable_active(
    settings: &ModelSettings,
    catalog: &super::catalog::CatalogSnapshot,
) -> CoreResult<()> {
    parse_revision(&settings.revision)?;
    if let Err(error) = validate_selection(catalog, &settings.active) {
        // A stored active Codex choice may outlive the discovery row that
        // originally declared its traits.  Preserve it when its model key is
        // still a known Codex fallback/tombstone and all persisted identifiers
        // remain bounded.  Do not infer permission from user-facing copy or
        // catalog origin: both may legitimately change across refreshes.
        let preserved = find_model(catalog, &settings.active.key()).is_ok_and(|model| {
            (model.key.provider_id == super::catalog::CODEX_PROVIDER_ID
                || model.key.provider_id == super::catalog::CLAUDE_PROVIDER_ID)
                && safe_identifier(&settings.active.model_id)
                && settings
                    .active
                    .reasoning
                    .as_deref()
                    .is_none_or(safe_identifier)
                && settings
                    .active
                    .service_tier
                    .as_deref()
                    .is_none_or(safe_identifier)
        });
        if !preserved {
            return Err(error);
        }
    }
    if settings.favorites.len() > MAX_FAVORITES {
        return Err(CoreError::new(
            "TooManyFavorites",
            "A maximum of 32 favorite models can be saved.",
        ));
    }
    let mut seen = HashSet::new();
    for favorite in &settings.favorites {
        find_model(catalog, favorite)?;
        if !seen.insert(favorite) {
            return Err(CoreError::new(
                "DuplicateFavorite",
                "A model cannot appear more than once in favorites.",
            ));
        }
    }
    Ok(())
}

fn safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('-')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'/')
        })
}

pub fn parse_revision(value: &str) -> CoreResult<i64> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(CoreError::new(
            "InvalidPreferenceRevision",
            "The preference revision must be a nonnegative decimal string.",
        ));
    }
    let revision = value.parse::<i64>().map_err(|_| {
        CoreError::new(
            "InvalidPreferenceRevision",
            "The preference revision must be a nonnegative decimal string.",
        )
    })?;
    if revision < 0 {
        return Err(CoreError::new(
            "InvalidPreferenceRevision",
            "The preference revision must be nonnegative.",
        ));
    }
    Ok(revision)
}

pub(crate) fn provider_state_with_endpoints_and_codex(
    settings: ModelSettings,
    endpoints: &EndpointProfilesSettings,
    codex: Option<&CodexCatalog>,
) -> CoreResult<ProviderState> {
    let catalog = catalog_with_endpoints_and_codex(endpoints, &settings, codex)?;
    validate_settings_preserving_unavailable_active(&settings, &catalog)?;
    let model = find_model(&catalog, &settings.active.key())?;
    let dispatch = if model.key
        == ModelKey::new(
            super::catalog::MOCK_PROVIDER_ID,
            super::catalog::MOCK_MODEL_ID,
        ) {
        DispatchResolution::LocalMock {
            detail: "The deterministic local mock is ready.".to_owned(),
        }
    } else if model.key.provider_id == super::catalog::CLAUDE_PROVIDER_ID {
        DispatchResolution::Blocked {
            detail: format!(
                "{} is saved as the active choice, but the Claude Code connection has not been checked.",
                model.label
            ),
        }
    } else if let Some(profile) = endpoints.find(&model.key.provider_id) {
        let detail = if profile.enabled {
            format!(
                "{} is saved as the active choice, but native OpenAI-compatible dispatch is not ready yet.",
                model.label
            )
        } else {
            format!(
                "{} is saved as the active choice, but its endpoint profile is disabled.",
                model.label
            )
        };
        DispatchResolution::Blocked { detail }
    } else {
        DispatchResolution::Blocked {
            detail: format!(
                "{} is saved as the active choice, but this provider is not qualified yet.",
                model.label
            ),
        }
    };
    Ok(ProviderState {
        settings,
        catalog,
        dispatch,
    })
}
