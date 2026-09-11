//! Nonsecret app-local profiles for OpenAI-compatible endpoints.
//!
//! A profile contains routing and discovery metadata only.  API keys live in
//! the native secret store and are referenced by `credential_ref`; this module
//! never accepts or persists the key itself.

use super::credentials::CredentialTarget;
use super::openai_compatible::normalize_base_url;
use wns_kernel::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

pub const ENDPOINT_PROFILES_SCHEMA_VERSION: u32 = 1;
pub const ENDPOINT_PROFILES_KEY: &str = "openai-compatible-endpoints-v1";
pub const ENDPOINT_PROVIDER_PREFIX: &str = "openai-compatible:";
pub const MAX_ENDPOINT_PROFILES: usize = 32;
pub const MAX_ENDPOINT_LABEL_BYTES: usize = 128;
pub const MAX_ENDPOINT_URL_BYTES: usize = 2048;
pub const MAX_ENDPOINT_MODEL_IDS: usize = 256;
pub const MAX_ENDPOINT_MODEL_ID_BYTES: usize = 256;
pub const MAX_CREDENTIAL_REF_BYTES: usize = 256;

/// Persisted endpoint configuration and non-authoritative model discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointProfile {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub enabled: bool,
    #[serde(default)]
    pub json_mode: bool,
    pub config_revision: String,
    #[serde(default)]
    pub credential_ref: Option<String>,
    #[serde(default)]
    pub manual_model_ids: Vec<String>,
    #[serde(default)]
    pub cached_model_ids: Vec<String>,
}

/// User-owned fields for creating or updating a profile.  Discovery cache and
/// the profile-local revision are maintained by the library, never supplied by
/// an untrusted picker payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointProfileDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    pub base_url: String,
    pub enabled: bool,
    #[serde(default)]
    pub json_mode: bool,
    #[serde(default)]
    pub credential_ref: Option<String>,
    #[serde(default)]
    pub manual_model_ids: Vec<String>,
}

impl EndpointProfileDraft {
    pub fn new(label: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            id: None,
            label: label.into(),
            base_url: base_url.into(),
            enabled: true,
            json_mode: false,
            credential_ref: None,
            manual_model_ids: Vec::new(),
        }
    }

    pub fn validate(&self) -> CoreResult<String> {
        if let Some(id) = &self.id {
            validate_profile_id(id)?;
        }
        validate_label(&self.label)?;
        let normalized = normalize_url(&self.base_url)?;
        validate_credential_ref(self.credential_ref.as_deref())?;
        validate_model_ids(&self.manual_model_ids)?;
        Ok(normalized)
    }
}

impl EndpointProfile {
    pub fn new(id: String, draft: &EndpointProfileDraft) -> CoreResult<Self> {
        let normalized = draft.validate()?;
        let profile = Self {
            id,
            label: draft.label.clone(),
            base_url: normalized,
            enabled: draft.enabled,
            json_mode: draft.json_mode,
            config_revision: "0".to_owned(),
            credential_ref: draft.credential_ref.clone(),
            manual_model_ids: draft.manual_model_ids.clone(),
            cached_model_ids: Vec::new(),
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> CoreResult<()> {
        validate_profile_id(&self.id)?;
        validate_label(&self.label)?;
        let normalized = normalize_url(&self.base_url)?;
        if normalized != self.base_url {
            return Err(CoreError::new(
                "InvalidEndpointProfile",
                "The endpoint URL must be normalized before it is stored.",
            ));
        }
        parse_revision(&self.config_revision)?;
        validate_credential_ref(self.credential_ref.as_deref())?;
        validate_model_ids(&self.manual_model_ids)?;
        validate_model_ids(&self.cached_model_ids)?;
        if self.manual_model_ids.len() + self.cached_model_ids.len() > MAX_ENDPOINT_MODEL_IDS {
            return Err(CoreError::new(
                "TooManyEndpointModels",
                "An endpoint profile can retain at most 256 manual and discovered model IDs.",
            ));
        }
        Ok(())
    }

    pub(crate) fn all_model_ids(&self) -> impl Iterator<Item = &String> {
        self.manual_model_ids
            .iter()
            .chain(self.cached_model_ids.iter())
    }

    pub fn config_equals(&self, draft: &EndpointProfileDraft, normalized: &str) -> bool {
        self.label == draft.label
            && self.base_url == normalized
            && self.enabled == draft.enabled
            && self.json_mode == draft.json_mode
            && self.credential_ref == draft.credential_ref
            && self.manual_model_ids == draft.manual_model_ids
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointProfilesSettings {
    pub revision: String,
    pub profiles: Vec<EndpointProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredEndpointProfiles {
    pub profiles: Vec<EndpointProfile>,
}

impl Default for EndpointProfilesSettings {
    fn default() -> Self {
        Self {
            revision: "0".to_owned(),
            profiles: Vec::new(),
        }
    }
}

impl EndpointProfilesSettings {
    pub fn stored(&self) -> StoredEndpointProfiles {
        StoredEndpointProfiles {
            profiles: self.profiles.clone(),
        }
    }

    pub fn from_stored(revision: String, stored: StoredEndpointProfiles) -> Self {
        Self {
            revision,
            profiles: stored.profiles,
        }
    }

    pub fn validate(&self) -> CoreResult<()> {
        parse_revision(&self.revision)?;
        if self.profiles.len() > MAX_ENDPOINT_PROFILES {
            return Err(CoreError::new(
                "TooManyEndpointProfiles",
                "A maximum of 32 endpoint profiles can be saved.",
            ));
        }
        let mut ids = HashSet::new();
        for profile in &self.profiles {
            profile.validate()?;
            if !ids.insert(&profile.id) {
                return Err(CoreError::new(
                    "DuplicateEndpointProfile",
                    "An endpoint profile ID may appear only once.",
                ));
            }
        }
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&EndpointProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }
}

pub fn validate_discovered_model_ids(model_ids: &[String]) -> CoreResult<()> {
    validate_model_ids(model_ids)
}

pub fn parse_revision(value: &str) -> CoreResult<i64> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(CoreError::new(
            "InvalidEndpointRevision",
            "The endpoint revision must be a nonnegative decimal string.",
        ));
    }
    value.parse::<i64>().map_err(|_| {
        CoreError::new(
            "InvalidEndpointRevision",
            "The endpoint revision exceeds the supported range.",
        )
    })
}

fn validate_profile_id(id: &str) -> CoreResult<()> {
    let uuid = id.strip_prefix(ENDPOINT_PROVIDER_PREFIX).ok_or_else(|| {
        CoreError::new(
            "InvalidEndpointProfile",
            "Endpoint profile IDs must use the openai-compatible:<uuid> form.",
        )
    })?;
    let parsed = Uuid::parse_str(uuid).map_err(|_| {
        CoreError::new(
            "InvalidEndpointProfile",
            "Endpoint profile IDs must contain a valid UUID.",
        )
    })?;
    if parsed.to_string() != uuid {
        return Err(CoreError::new(
            "InvalidEndpointProfile",
            "Endpoint profile UUIDs must use canonical lowercase form.",
        ));
    }
    Ok(())
}

fn validate_label(label: &str) -> CoreResult<()> {
    if label.is_empty()
        || label.len() > MAX_ENDPOINT_LABEL_BYTES
        || label.chars().any(char::is_control)
    {
        return Err(CoreError::new(
            "InvalidEndpointProfile",
            "Endpoint labels must be 1..128 bytes and contain no control characters.",
        ));
    }
    Ok(())
}

fn normalize_url(value: &str) -> CoreResult<String> {
    if value.len() > MAX_ENDPOINT_URL_BYTES {
        return Err(CoreError::new(
            "InvalidEndpointProfile",
            "Endpoint URLs exceed the supported length.",
        ));
    }
    normalize_base_url(value)
        .map(|url| url.to_string())
        .map_err(|error| CoreError::new("InvalidEndpointProfile", &error.detail))
}

fn validate_credential_ref(value: Option<&str>) -> CoreResult<()> {
    if let Some(value) = value {
        if value.is_empty()
            || value.len() > MAX_CREDENTIAL_REF_BYTES
            || value.chars().any(|character| character.is_control())
        {
            return Err(CoreError::new(
                "InvalidCredentialReference",
                "Credential references must be 1..256 bytes and contain no control characters.",
            ));
        }
        CredentialTarget::parse(value).map_err(|_| {
            CoreError::new(
                "InvalidCredentialReference",
                "Credential references must be app-owned secret-store targets.",
            )
        })?;
    }
    Ok(())
}

fn validate_model_ids(model_ids: &[String]) -> CoreResult<()> {
    if model_ids.len() > MAX_ENDPOINT_MODEL_IDS {
        return Err(CoreError::new(
            "TooManyEndpointModels",
            "An endpoint profile can retain at most 256 model IDs.",
        ));
    }
    let mut seen = HashSet::new();
    for model_id in model_ids {
        if model_id.is_empty()
            || model_id.len() > MAX_ENDPOINT_MODEL_ID_BYTES
            || model_id.chars().any(char::is_control)
        {
            return Err(CoreError::new(
                "InvalidEndpointModel",
                "Endpoint model IDs must be 1..256 bytes and contain no control characters.",
            ));
        }
        if !seen.insert(model_id) {
            return Err(CoreError::new(
                "DuplicateEndpointModel",
                "An endpoint model ID may appear only once.",
            ));
        }
    }
    Ok(())
}
