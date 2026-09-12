//! The small, offline model catalog used by the native settings surface.
//!
//! The built-in reference catalog is complemented by a persisted, sanitized
//! Codex discovery when one exists.  Both remain descriptive until a provider
//! adapter passes its readiness gates.  Keeping that state in the descriptor
//! prevents the picker from turning a remembered choice into a claim that the
//! provider is ready.

use super::claude_profile::{CLAUDE_FABLE_MODEL, CLAUDE_OPUS_MODEL, CLAUDE_SONNET_MODEL};
use super::codex_catalog::CodexCatalog;
use super::endpoints::{ENDPOINT_PROVIDER_PREFIX, EndpointProfilesSettings};
use super::preferences::{ModelKey, ModelSelection, ModelSettings};
use serde::{Deserialize, Serialize};
use wns_kernel::{CoreError, CoreResult};

pub const CATALOG_SCHEMA_VERSION: u32 = 1;
pub const MOCK_PROVIDER_ID: &str = "mock";
pub const MOCK_MODEL_ID: &str = "mock-story-context";
pub const CODEX_PROVIDER_ID: &str = "codex";
pub const CLAUDE_PROVIDER_ID: &str = "claude";
pub const CATALOG_REFERENCE: &str =
    "docs/CODEX_QUALIFICATION.md (native 0.153.3 reference; not live discovery)";

/// A serializable catalog entry identified by the exact provider and model
/// IDs.  Display labels are descriptive only; dispatch must use the IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelDescriptor {
    pub key: ModelKey,
    pub label: String,
    pub provider_label: String,
    pub reasoning_levels: Vec<String>,
    pub service_tiers: Vec<ServiceTier>,
    pub context_window_tokens: Option<String>,
    pub max_output_tokens: Option<String>,
    #[serde(default)]
    pub default_reasoning: Option<String>,
    #[serde(default)]
    pub default_service_tier: Option<String>,
    pub origin: CatalogOrigin,
    pub ready: bool,
    pub status_detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceTier {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CatalogOrigin {
    BuiltIn,
    Reference,
    CodexDiscovery,
    OpenAiCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogSnapshot {
    pub schema_version: u32,
    pub models: Vec<ModelDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum DispatchResolution {
    #[specta(rename_all = "camelCase")]
    LocalMock { detail: String },
    #[specta(rename_all = "camelCase")]
    CodexCli { detail: String },
    #[specta(rename_all = "camelCase")]
    ClaudeCli { detail: String },
    #[specta(rename_all = "camelCase")]
    OpenAiCompatible { detail: String },
    #[specta(rename_all = "camelCase")]
    Blocked { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderState {
    pub settings: ModelSettings,
    pub catalog: CatalogSnapshot,
    pub dispatch: DispatchResolution,
}

pub fn built_in_catalog() -> CatalogSnapshot {
    let mut models = Vec::with_capacity(11);
    models.push(ModelDescriptor {
        key: ModelKey::new(MOCK_PROVIDER_ID, MOCK_MODEL_ID),
        label: "Local test model".to_owned(),
        provider_label: "WebnovelStudio".to_owned(),
        reasoning_levels: Vec::new(),
        service_tiers: Vec::new(),
        // The mock uses its fixed compiler contract, not a provider token
        // budget.  Keep these values absent rather than inventing a limit.
        context_window_tokens: None,
        max_output_tokens: None,
        default_reasoning: None,
        default_service_tier: None,
        origin: CatalogOrigin::BuiltIn,
        ready: true,
        status_detail: "Local test model. No live AI connected.".to_owned(),
    });

    let codex_models = [
        ("gpt-6-astra", "GPT-6-Astra"),
        ("gpt-5.6-sol", "GPT-5.6-Sol"),
        ("gpt-5.6-terra", "GPT-5.6-Terra"),
        ("gpt-5.6-luna", "GPT-5.6-Luna"),
        ("gpt-5.5", "GPT-5.5"),
        ("gpt-5.4-mini", "GPT-5.4-mini"),
        ("gpt-5.3-codex-spark", "GPT-5.3-Codex-Spark"),
    ];
    for (id, label) in codex_models {
        let luna = id == "gpt-5.6-luna";
        models.push(ModelDescriptor {
            key: ModelKey::new(CODEX_PROVIDER_ID, id),
            label: label.to_owned(),
            provider_label: "Codex CLI".to_owned(),
            reasoning_levels: if luna {
                ["low", "medium", "high", "xhigh", "max"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            } else {
                Vec::new()
            },
            service_tiers: if luna {
                vec![ServiceTier {
                    id: "priority".to_owned(),
                    label: "Fast".to_owned(),
                }]
            } else {
                Vec::new()
            },
            context_window_tokens: None,
            max_output_tokens: None,
            default_reasoning: if luna { Some("xhigh".to_owned()) } else { None },
            default_service_tier: if luna { Some("priority".to_owned()) } else { None },
            origin: CatalogOrigin::Reference,
            ready: false,
            status_detail:
                "Codex CLI is a reference-only catalog entry; live provider support is not qualified."
                    .to_owned(),
        });
    }

    let claude_models = [
        (CLAUDE_FABLE_MODEL, "Claude Fable 5"),
        (CLAUDE_OPUS_MODEL, "Claude Opus 5"),
        (CLAUDE_SONNET_MODEL, "Claude Sonnet 5"),
    ];
    for (id, label) in claude_models {
        models.push(ModelDescriptor {
            key: ModelKey::new(CLAUDE_PROVIDER_ID, id),
            label: label.to_owned(),
            provider_label: "Claude Code".to_owned(),
            reasoning_levels: ["low", "medium", "high", "xhigh", "max"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            service_tiers: Vec::new(),
            context_window_tokens: None,
            max_output_tokens: None,
            default_reasoning: Some("high".to_owned()),
            default_service_tier: None,
            origin: CatalogOrigin::Reference,
            ready: false,
            status_detail:
                "Claude Code is a reference-only catalog entry; check the connection before sending."
                    .to_owned(),
        });
    }

    CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_VERSION,
        models,
    }
}

/// Build the picker catalog from the static fallback, an optional completed
/// Codex discovery, and endpoint profiles.  Discovery is descriptive only;
/// native readiness is overlaid by the desktop runtime after a connection
/// check.  The active/favorite selections are preserved as unavailable rows
/// when a model disappears from the latest discovery.
pub fn catalog_with_endpoints_and_codex(
    endpoints: &EndpointProfilesSettings,
    settings: &ModelSettings,
    codex: Option<&CodexCatalog>,
) -> CoreResult<CatalogSnapshot> {
    endpoints.validate()?;
    let mut catalog = built_in_catalog();
    if let Some(codex) = codex {
        codex.validate()?;
        for discovered in &codex.models {
            let key = ModelKey::new(CODEX_PROVIDER_ID, discovered.model_id.clone());
            let descriptor = ModelDescriptor {
                key: key.clone(),
                label: discovered.label.clone(),
                provider_label: "Codex CLI".to_owned(),
                reasoning_levels: discovered.reasoning_levels.clone(),
                service_tiers: discovered.service_tiers.clone(),
                context_window_tokens: None,
                max_output_tokens: None,
                default_reasoning: discovered.default_reasoning.clone(),
                default_service_tier: discovered.default_service_tier.clone(),
                origin: CatalogOrigin::CodexDiscovery,
                ready: false,
                status_detail:
                    "Discovered from the installed Codex CLI; check the connection before sending."
                        .to_owned(),
            };
            if let Some(existing) = catalog.models.iter_mut().find(|model| model.key == key) {
                *existing = descriptor;
            } else {
                catalog.models.push(descriptor);
            }
        }
    }
    let mut known = catalog
        .models
        .iter()
        .map(|model| model.key.clone())
        .collect::<std::collections::HashSet<_>>();

    for profile in &endpoints.profiles {
        for model_id in profile.all_model_ids() {
            let key = ModelKey::new(profile.id.clone(), model_id.clone());
            if known.insert(key.clone()) {
                catalog.models.push(endpoint_model(profile, model_id));
            }
        }
    }

    // A profile can be disabled, omitted from a later settings payload, or
    // have a model disappear from discovery.  Keep remembered selections as
    // explicit unavailable tombstones so the picker never silently changes
    // author intent.
    let mut remembered = Vec::with_capacity(1 + settings.favorites.len());
    remembered.push(settings.active.key());
    remembered.extend(settings.favorites.iter().cloned());
    for selection in remembered {
        if !selection.provider_id.starts_with(ENDPOINT_PROVIDER_PREFIX) {
            continue;
        }
        let key = selection.clone();
        if known.insert(key.clone()) {
            let profile = endpoints.find(&selection.provider_id);
            let mut descriptor = ModelDescriptor {
                key,
                label: selection.model_id.clone(),
                provider_label: profile
                    .map(|profile| profile.label.clone())
                    .unwrap_or_else(|| "OpenAI-compatible endpoint".to_owned()),
                reasoning_levels: Vec::new(),
                service_tiers: Vec::new(),
                context_window_tokens: None,
                max_output_tokens: None,
                default_reasoning: None,
                default_service_tier: None,
                origin: CatalogOrigin::OpenAiCompatible,
                ready: false,
                status_detail: "The saved model is unavailable; its selection was preserved."
                    .to_owned(),
            };
            if let Some(profile) = profile {
                if !profile.enabled {
                    descriptor.status_detail =
                        "The endpoint is disabled; the saved model selection was preserved."
                            .to_owned();
                } else {
                    descriptor.status_detail = "The model was omitted by the latest discovery result; the saved selection was preserved.".to_owned();
                }
            }
            catalog.models.push(descriptor);
        }
    }

    // Preserve Codex author intent across discovery changes.  An explicit
    // trait that disappeared is also retained, but marked unavailable so it
    // cannot be mistaken for a newly supported choice.
    let mut remembered = Vec::with_capacity(1 + settings.favorites.len());
    remembered.push(settings.active.key());
    remembered.extend(settings.favorites.iter().cloned());
    for key in remembered {
        if key.provider_id != CODEX_PROVIDER_ID {
            continue;
        }
        let Some(descriptor) = catalog.models.iter_mut().find(|model| model.key == key) else {
            catalog.models.push(codex_unavailable_model(&key));
            continue;
        };
        if key == settings.active.key()
            && codex.is_some_and(|discovered| !discovered.supports(&settings.active))
        {
            descriptor.ready = false;
            descriptor.status_detail =
                "The saved Codex selection is no longer declared; its selection was preserved."
                    .to_owned();
        }
    }
    validate_catalog(&catalog)?;
    Ok(catalog)
}

fn codex_unavailable_model(key: &ModelKey) -> ModelDescriptor {
    ModelDescriptor {
        key: key.clone(),
        label: key.model_id.clone(),
        provider_label: "Codex CLI".to_owned(),
        reasoning_levels: Vec::new(),
        service_tiers: Vec::new(),
        context_window_tokens: None,
        max_output_tokens: None,
        default_reasoning: None,
        default_service_tier: None,
        origin: CatalogOrigin::CodexDiscovery,
        ready: false,
        status_detail: "The saved Codex model is unavailable; its selection was preserved."
            .to_owned(),
    }
}

fn endpoint_model(profile: &super::endpoints::EndpointProfile, model_id: &str) -> ModelDescriptor {
    let status_detail = if !profile.enabled {
        "The endpoint is disabled. Enable it before dispatch.".to_owned()
    } else {
        "The endpoint is configured, but native dispatch is not qualified yet.".to_owned()
    };
    ModelDescriptor {
        key: ModelKey::new(profile.id.clone(), model_id.to_owned()),
        label: model_id.to_owned(),
        provider_label: profile.label.clone(),
        reasoning_levels: Vec::new(),
        service_tiers: Vec::new(),
        context_window_tokens: None,
        max_output_tokens: None,
        default_reasoning: None,
        default_service_tier: None,
        origin: CatalogOrigin::OpenAiCompatible,
        ready: false,
        status_detail,
    }
}

pub(crate) fn validate_catalog(catalog: &CatalogSnapshot) -> CoreResult<()> {
    if catalog.schema_version != CATALOG_SCHEMA_VERSION {
        return Err(CoreError::new(
            "UnsupportedCatalog",
            "This model catalog uses an unsupported schema.",
        ));
    }
    if catalog.models.is_empty() {
        return Err(CoreError::new(
            "InvalidCatalog",
            "The model catalog must contain at least the local mock.",
        ));
    }
    let mut keys = std::collections::HashSet::new();
    for model in &catalog.models {
        validate_text(&model.key.provider_id, "provider ID")?;
        validate_text(&model.key.model_id, "model ID")?;
        validate_text(&model.label, "model label")?;
        validate_text(&model.provider_label, "provider label")?;
        if !keys.insert(model.key.clone()) {
            return Err(CoreError::new(
                "InvalidCatalog",
                "The model catalog contains a duplicate model key.",
            ));
        }
        validate_unique_strings(&model.reasoning_levels, "reasoning level")?;
        if let Some(default) = &model.default_reasoning {
            validate_text(default, "default reasoning level")?;
            if !model.reasoning_levels.iter().any(|value| value == default) {
                return Err(CoreError::new(
                    "InvalidCatalog",
                    "The model default reasoning level is not declared.",
                ));
            }
        }
        let mut tiers = std::collections::HashSet::new();
        for tier in &model.service_tiers {
            validate_text(&tier.id, "service tier ID")?;
            validate_text(&tier.label, "service tier label")?;
            if !tiers.insert(&tier.id) {
                return Err(CoreError::new(
                    "InvalidCatalog",
                    "The model catalog contains a duplicate service tier.",
                ));
            }
        }
        if let Some(default) = &model.default_service_tier {
            validate_text(default, "default service tier")?;
            if !model.service_tiers.iter().any(|tier| &tier.id == default) {
                return Err(CoreError::new(
                    "InvalidCatalog",
                    "The model default service tier is not declared.",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn find_model<'a>(
    catalog: &'a CatalogSnapshot,
    key: &ModelKey,
) -> CoreResult<&'a ModelDescriptor> {
    catalog
        .models
        .iter()
        .find(|model| model.key == *key)
        .ok_or_else(|| CoreError::new("UnknownModel", "The selected model is not in the catalog."))
}

pub(crate) fn validate_selection(
    catalog: &CatalogSnapshot,
    selection: &ModelSelection,
) -> CoreResult<()> {
    let model = find_model(catalog, &selection.key())?;
    if let Some(reasoning) = &selection.reasoning
        && !model
            .reasoning_levels
            .iter()
            .any(|value| value == reasoning)
    {
        return Err(CoreError::new(
            "UnsupportedModelTrait",
            "The selected reasoning level is not declared for this model.",
        ));
    }
    if let Some(service_tier) = &selection.service_tier
        && !model
            .service_tiers
            .iter()
            .any(|value| &value.id == service_tier)
    {
        return Err(CoreError::new(
            "UnsupportedModelTrait",
            "The selected service tier is not declared for this model.",
        ));
    }
    if selection.key() == ModelKey::new(MOCK_PROVIDER_ID, MOCK_MODEL_ID)
        && (selection.reasoning.is_some() || selection.service_tier.is_some())
    {
        return Err(CoreError::new(
            "UnsupportedModelTrait",
            "The local mock does not accept provider-specific traits.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_text(value: &str, label: &str) -> CoreResult<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(CoreError::new(
            "InvalidCatalog",
            &format!("The {label} is empty, too long, or contains control characters."),
        ));
    }
    Ok(())
}

fn validate_unique_strings(values: &[String], label: &str) -> CoreResult<()> {
    let mut seen = std::collections::HashSet::new();
    for value in values {
        validate_text(value, label)?;
        if !seen.insert(value) {
            return Err(CoreError::new(
                "InvalidCatalog",
                &format!("The model catalog contains a duplicate {label}."),
            ));
        }
    }
    Ok(())
}
