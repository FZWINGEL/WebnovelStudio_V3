//! Provider contract vocabulary: identity, binding, limits, and the
//! provider-facing request shape.
//!
//! Moved down from `context/packet.rs` (L2) into `wns-providers` (L1). The
//! packet compiler already sourced its profile constants from this crate
//! (`CODEX_PROFILE_VERSION`, `CLAUDE_INPUT_LIMIT_BYTES`, …) while the provider
//! adapters reached *up* into the compiler for `ProviderBinding`. That is a
//! real cycle between the two layers, and these types are provider vocabulary —
//! so they belong here and the cycle is gone by construction rather than by
//! convention.
//!
//! `context/packet.rs` re-exports every item below at its historical path, so
//! no packet changes shape: the serialized layout of a binding is identical
//! whichever crate owns the type, and the byte-compatibility tests assert it.

use serde::{Deserialize, Serialize};

/// The first packet counter is intentionally a byte counter for one fixed
/// deterministic mock model.  It is not a claim about any provider's
/// tokenizer or context-window accounting.
pub const MOCK_MODEL_ID: &str = "mock-story-context";
pub const MOCK_TOKEN_ACCOUNTING_METHOD: &str = "utf8-byte-count/mock-story-context-v1";
pub const CODEX_PROVIDER_ID: &str = "codex";
pub const CODEX_LUNA_MODEL_ID: &str = "gpt-5.6-luna";
pub const CODEX_REASONING_EFFORT: &str = crate::codex_profile::CODEX_REASONING_EFFORT;
pub const CODEX_MAINTENANCE_MODEL_ID: &str =
    crate::codex_profile::CODEX_MAINTENANCE_MODEL;
pub const CODEX_MAINTENANCE_REASONING: &str =
    crate::codex_profile::CODEX_MAINTENANCE_REASONING_EFFORT;
pub const CODEX_SERVICE_TIER: &str = "priority";
/// The application's launch contract, independent of Codex's release version.
pub const CODEX_PROFILE_VERSION: &str = crate::codex_profile::CODEX_PROFILE_VERSION;
pub const CODEX_MAINTENANCE_PROFILE_VERSION: &str =
    crate::codex_profile::CODEX_MAINTENANCE_PROFILE_VERSION;
pub const CODEX_HISTORICAL_PROFILE_VERSION: &str = "0.153.3";
pub const CODEX_INPUT_LIMIT_BYTES: usize = 24 * 1024;
pub const CODEX_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
pub const CODEX_TOKEN_ACCOUNTING_METHOD: &str = "utf8-byte-count/codex-stdin-application-cap-v1";
pub const CLAUDE_PROVIDER_ID: &str = "claude";
pub const CLAUDE_PROFILE_VERSION: &str = crate::claude_profile::CLAUDE_PROFILE_VERSION;
pub const CLAUDE_INPUT_LIMIT_BYTES: usize =
    crate::claude_profile::CLAUDE_INPUT_LIMIT_BYTES;
pub const CLAUDE_OUTPUT_LIMIT_BYTES: usize =
    crate::claude_profile::CLAUDE_OUTPUT_LIMIT_BYTES;
pub const CLAUDE_TOKEN_ACCOUNTING_METHOD: &str =
    crate::claude_profile::CLAUDE_TOKEN_ACCOUNTING_METHOD;
/// Provider-neutral accounting label for the bounded OpenAI-compatible HTTP
/// transport.  This is a byte cap, not a claim about the provider tokenizer.
pub const HTTP_TOKEN_ACCOUNTING_METHOD: &str =
    "utf8-byte-count/openai-compatible-http-application-cap-v1";
pub const HTTP_PROFILE_VERSION: &str = "openai-chat-completions.v1";
/// The fixed OpenAI-compatible profile used for chapter-memory analysis.
///
/// This is deliberately a separate application contract from the author-room
/// HTTP profile.  It keeps the memory worker's model and trait selection
/// stable while allowing ordinary author requests to evolve independently.
pub const HTTP_MEMORY_PROFILE_VERSION: &str = "openai-chat-completions.memory.v2";
pub const HTTP_MEMORY_LEGACY_PROFILE_VERSION: &str = "openai-chat-completions.memory.v1";
pub const HTTP_MEMORY_LEGACY_MODEL_ID: &str = "gpt-5.6-luna";
pub const HTTP_MEMORY_LEGACY_REASONING: &str = "xhigh";
pub const HTTP_MEMORY_MODEL_ID: &str = "gpt-6-astra";
pub const HTTP_MEMORY_REASONING: &str = "low";
pub const HTTP_MEMORY_INPUT_LIMIT_BYTES: usize = HTTP_INPUT_LIMIT_BYTES;
pub const HTTP_MEMORY_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
pub const HTTP_INPUT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
pub const HTTP_OUTPUT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
/// These limits are application byte caps for the exact serialized stdin and
/// retained output. They are deliberately not model token-window claims. A
/// Provider-specific adapters may replace these fixed profiles with separately
/// qualified contracts; this slice accepts only the explicit Codex and
/// OpenAI-compatible contracts below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderBinding {
    pub provider_id: String,
    pub model_id: String,
    pub reasoning: Option<String>,
    pub service_tier: Option<String>,
    pub profile_version: String,
    pub input_limit_bytes: String,
    /// Explicitly not a provider/model reservation. Kept as zero in this
    /// profile because only the application byte cap is known.
    pub reserved_output_bytes: String,
    /// Explicitly not a provider/model reservation. Kept as zero in this
    /// profile because protocol headroom is not qualified here.
    pub reserved_protocol_bytes: String,
    pub output_limit_bytes: String,
    pub accounting_method: String,
    /// Observed executable identity, not an allowlist or an effective-model claim.
    /// Omitted for historical records and pure compiler fixtures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<ProviderRuntimeIdentity>,
    /// The non-secret, immutable transport contract for an OpenAI-compatible
    /// endpoint.  The endpoint profile ID is carried in `provider_id` so
    /// historical packets do not need a second identity field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpProviderBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpProviderBinding {
    /// The normalized endpoint base URL.  Credentials, query strings, and
    /// fragments are forbidden; the API key lives in the OS credential store.
    pub base_url: String,
    /// The endpoint profile's monotonic configuration revision.
    pub config_revision: String,
    /// Whether the worker must use the streaming endpoint.
    pub stream: bool,
    pub response_format: HttpResponseFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HttpResponseFormat {
    Text,
    JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRuntimeIdentity {
    pub cli_version: String,
    pub executable_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_server: Option<crate::codex_app_server::AppServerRuntimeIdentity>,
}

impl ProviderRuntimeIdentity {
    fn validate(&self) -> bool {
        !self.cli_version.is_empty()
            && self.cli_version.len() <= 128
            && self
                .cli_version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
            && self.executable_sha256.len() == 64
            && self
                .executable_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && self.catalog_sha256.as_ref().is_none_or(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
    }
}

impl ProviderBinding {
    pub fn codex_app_server_author_runtime(
        model_id: &str,
        reasoning: &str,
        service_tier: Option<&str>,
        cli_version: &str,
        executable_sha256: &str,
        catalog_sha256: &str,
        identity: crate::codex_app_server::AppServerRuntimeIdentity,
    ) -> Self {
        let mut binding = Self::codex_author_runtime(
            model_id,
            reasoning,
            service_tier,
            cli_version,
            executable_sha256,
            catalog_sha256,
        );
        binding.profile_version = crate::codex_app_server::AUTHOR_PROFILE.into();
        binding.accounting_method = crate::codex_app_server::ACCOUNTING_METHOD.into();
        binding.runtime.as_mut().expect("author runtime").app_server = Some(identity);
        binding
    }

    pub fn codex_app_server_maintenance_runtime(
        cli_version: &str,
        executable_sha256: &str,
        catalog_sha256: &str,
        identity: crate::codex_app_server::AppServerRuntimeIdentity,
    ) -> Self {
        let mut binding = Self::codex_app_server_author_runtime(
            CODEX_MAINTENANCE_MODEL_ID,
            CODEX_MAINTENANCE_REASONING,
            Some(CODEX_SERVICE_TIER),
            cli_version,
            executable_sha256,
            catalog_sha256,
            identity,
        );
        binding.profile_version = crate::codex_app_server::MAINTENANCE_PROFILE.into();
        binding
    }

    pub fn codex_luna() -> Self {
        Self::codex_luna_with_profile(CODEX_PROFILE_VERSION)
    }

    /// Current fixed Codex maintenance binding for summaries and story-memory
    /// refreshes. The legacy Luna helper remains available so persisted
    /// packets keep their original model, traits, and serialized identity.
    pub fn codex_maintenance() -> Self {
        Self::codex_maintenance_with_profile(CODEX_MAINTENANCE_PROFILE_VERSION)
    }

    /// Exact binding retained for packets created by the qualified 0.153.3
    /// adapter. This is for persistence/reopen/export validation only; native
    /// dispatch accepts the current [`Self::codex_luna`] value.
    pub fn codex_luna_historical() -> Self {
        let mut binding = Self::codex_luna_with_profile(CODEX_HISTORICAL_PROFILE_VERSION);
        binding.reasoning = Some("max".to_owned());
        binding
    }

    pub fn codex_luna_runtime(cli_version: &str, executable_sha256: &str) -> Self {
        let mut binding = Self::codex_luna();
        binding.runtime = Some(ProviderRuntimeIdentity {
            app_server: None,
            cli_version: cli_version.to_owned(),
            executable_sha256: executable_sha256.to_owned(),
            catalog_sha256: None,
        });
        binding
    }

    pub fn codex_maintenance_runtime(cli_version: &str, executable_sha256: &str) -> Self {
        let mut binding = Self::codex_maintenance();
        binding.runtime = Some(ProviderRuntimeIdentity {
            app_server: None,
            cli_version: cli_version.to_owned(),
            executable_sha256: executable_sha256.to_owned(),
            catalog_sha256: None,
        });
        binding
    }

    /// Build the current fixed OpenAI-compatible maintenance binding. The
    /// endpoint ID and normalized transport details are captured here; the
    /// credential remains outside the immutable packet in the OS store.
    pub fn http_memory(
        provider_id: &str,
        base_url: &str,
        config_revision: &str,
        stream: bool,
        response_format: HttpResponseFormat,
    ) -> Self {
        Self {
            provider_id: provider_id.to_owned(),
            model_id: HTTP_MEMORY_MODEL_ID.to_owned(),
            reasoning: Some(HTTP_MEMORY_REASONING.to_owned()),
            service_tier: None,
            profile_version: HTTP_MEMORY_PROFILE_VERSION.to_owned(),
            input_limit_bytes: HTTP_MEMORY_INPUT_LIMIT_BYTES.to_string(),
            reserved_output_bytes: "0".to_owned(),
            reserved_protocol_bytes: "0".to_owned(),
            output_limit_bytes: HTTP_MEMORY_OUTPUT_LIMIT_BYTES.to_string(),
            accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.to_owned(),
            runtime: None,
            http: Some(HttpProviderBinding {
                base_url: base_url.to_owned(),
                config_revision: config_revision.to_owned(),
                stream,
                response_format,
            }),
        }
    }

    pub fn is_current_codex_profile(&self) -> bool {
        matches!(
            self.profile_version.as_str(),
            CODEX_PROFILE_VERSION
                | CODEX_MAINTENANCE_PROFILE_VERSION
                | crate::codex_profile::CODEX_AUTHOR_PROFILE_VERSION
        ) && self.validate().is_ok()
    }

    pub fn is_codex_maintenance_profile(&self) -> bool {
        self.provider_id == CODEX_PROVIDER_ID
            && self.profile_version == CODEX_MAINTENANCE_PROFILE_VERSION
            && self.validate().is_ok()
    }

    /// Concrete author traits have already been resolved against the native
    /// connection's discovery result. Structural validation here does not
    /// authorize dispatch; the native connection rechecks its exact catalog.
    pub fn codex_author_runtime(
        model_id: &str,
        reasoning: &str,
        service_tier: Option<&str>,
        cli_version: &str,
        executable_sha256: &str,
        catalog_sha256: &str,
    ) -> Self {
        let mut binding = Self::codex_luna_runtime(cli_version, executable_sha256);
        binding.profile_version =
            crate::codex_profile::CODEX_AUTHOR_PROFILE_VERSION.into();
        binding.model_id = model_id.into();
        binding.reasoning = Some(reasoning.into());
        binding.service_tier = service_tier.map(str::to_owned);
        binding
            .runtime
            .as_mut()
            .expect("runtime supplied")
            .catalog_sha256 = Some(catalog_sha256.into());
        binding
    }

    /// Create the immutable Claude author binding after native runtime checks
    /// have resolved the exact model, effort, observed version, and executable
    /// fingerprint.  Structural validation remains the authority for stored
    /// packets; native dispatch must still compare the runtime identity.
    pub fn claude_author_runtime(
        model_id: &str,
        effort: &str,
        cli_version: &str,
        executable_sha256: &str,
    ) -> Self {
        Self {
            provider_id: CLAUDE_PROVIDER_ID.to_owned(),
            model_id: model_id.to_owned(),
            reasoning: Some(effort.to_owned()),
            service_tier: None,
            profile_version: CLAUDE_PROFILE_VERSION.to_owned(),
            input_limit_bytes: CLAUDE_INPUT_LIMIT_BYTES.to_string(),
            reserved_output_bytes: "0".to_owned(),
            reserved_protocol_bytes: "0".to_owned(),
            output_limit_bytes: CLAUDE_OUTPUT_LIMIT_BYTES.to_string(),
            accounting_method: CLAUDE_TOKEN_ACCOUNTING_METHOD.to_owned(),
            runtime: Some(ProviderRuntimeIdentity {
                app_server: None,
                cli_version: cli_version.to_owned(),
                executable_sha256: executable_sha256.to_owned(),
                catalog_sha256: None,
            }),
            http: None,
        }
    }

    pub fn is_claude(&self) -> bool {
        self.provider_id == CLAUDE_PROVIDER_ID
    }

    pub fn is_current_claude_profile(&self) -> bool {
        self.is_claude()
            && self.profile_version == CLAUDE_PROFILE_VERSION
            && self.validate().is_ok()
    }

    fn codex_luna_with_profile(profile_version: &str) -> Self {
        Self {
            provider_id: CODEX_PROVIDER_ID.to_owned(),
            model_id: CODEX_LUNA_MODEL_ID.to_owned(),
            reasoning: Some(CODEX_REASONING_EFFORT.to_owned()),
            service_tier: Some(CODEX_SERVICE_TIER.to_owned()),
            profile_version: profile_version.to_owned(),
            input_limit_bytes: CODEX_INPUT_LIMIT_BYTES.to_string(),
            reserved_output_bytes: "0".to_owned(),
            reserved_protocol_bytes: "0".to_owned(),
            output_limit_bytes: CODEX_OUTPUT_LIMIT_BYTES.to_string(),
            accounting_method: CODEX_TOKEN_ACCOUNTING_METHOD.to_owned(),
            runtime: None,
            http: None,
        }
    }

    fn codex_maintenance_with_profile(profile_version: &str) -> Self {
        Self {
            provider_id: CODEX_PROVIDER_ID.to_owned(),
            model_id: CODEX_MAINTENANCE_MODEL_ID.to_owned(),
            reasoning: Some(CODEX_MAINTENANCE_REASONING.to_owned()),
            service_tier: Some(CODEX_SERVICE_TIER.to_owned()),
            profile_version: profile_version.to_owned(),
            input_limit_bytes: CODEX_INPUT_LIMIT_BYTES.to_string(),
            reserved_output_bytes: "0".to_owned(),
            reserved_protocol_bytes: "0".to_owned(),
            output_limit_bytes: CODEX_OUTPUT_LIMIT_BYTES.to_string(),
            accounting_method: CODEX_TOKEN_ACCOUNTING_METHOD.to_owned(),
            runtime: None,
            http: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if crate::codex_app_server::is_app_server(self) {
            if !self
                .runtime
                .as_ref()
                .and_then(|runtime| runtime.app_server.as_ref())
                .is_some_and(|identity| identity.is_valid())
            {
                return Err(
                    "the app-server requires its checked account and isolation identity".into(),
                );
            }
            if self.accounting_method != crate::codex_app_server::ACCOUNTING_METHOD
                || (self.profile_version == crate::codex_app_server::MAINTENANCE_PROFILE
                    && (self.model_id != CODEX_MAINTENANCE_MODEL_ID
                        || self.reasoning.as_deref() != Some(CODEX_MAINTENANCE_REASONING)
                        || self.service_tier.as_deref() != Some(CODEX_SERVICE_TIER)))
            {
                return Err(
                    "the app-server binding has invalid accounting or maintenance settings".into(),
                );
            }
            let mut shape = self.clone();
            shape.profile_version =
                crate::codex_profile::CODEX_AUTHOR_PROFILE_VERSION.into();
            shape.accounting_method = CODEX_TOKEN_ACCOUNTING_METHOD.into();
            shape
                .runtime
                .as_mut()
                .expect("validated runtime")
                .app_server = None;
            return shape.validate();
        }
        if self
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.app_server.is_some())
        {
            return Err("app-server identity cannot authorize another transport".into());
        }
        if self == &Self::codex_luna_historical() {
            return Ok(());
        }
        if self.is_http() {
            return self.validate_http();
        }
        if self.is_claude() {
            return self.validate_claude();
        }
        let mut expected = match self.profile_version.as_str() {
            CODEX_PROFILE_VERSION => Self::codex_luna(),
            CODEX_MAINTENANCE_PROFILE_VERSION => Self::codex_maintenance(),
            crate::codex_profile::CODEX_AUTHOR_PROFILE_VERSION => Self::codex_luna(),
            _ => {
                return Err(
                    "the Codex application profile, model settings, or byte allowances are invalid"
                        .to_owned(),
                );
            }
        };
        expected.runtime = self.runtime.clone();
        if self.profile_version == crate::codex_profile::CODEX_AUTHOR_PROFILE_VERSION {
            let identifier = |value: &str| {
                !value.is_empty()
                    && value.len() <= 128
                    && !value.starts_with('-')
                    && value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric()
                            || matches!(byte, b'.' | b'-' | b'_' | b':' | b'/')
                    })
            };
            if !self
                .runtime
                .as_ref()
                .is_some_and(|runtime| runtime.catalog_sha256.is_some())
                || !identifier(&self.model_id)
                || !self.reasoning.as_deref().is_some_and(identifier)
                || self
                    .service_tier
                    .as_deref()
                    .is_some_and(|value| !identifier(value))
            {
                return Err("the author-selected Codex model or traits are invalid".into());
            }
            expected.profile_version = self.profile_version.clone();
            expected.model_id = self.model_id.clone();
            expected.reasoning = self.reasoning.clone();
            expected.service_tier = self.service_tier.clone();
        } else if self
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.catalog_sha256.is_some())
        {
            return Err(
                "the fixed Codex profile does not use an author catalog fingerprint".into(),
            );
        }
        if self != &expected
            || self
                .runtime
                .as_ref()
                .is_some_and(|runtime| !runtime.validate())
        {
            return Err(
                "the Codex application profile, model settings, runtime identity, or byte allowances are invalid"
                    .to_owned(),
            );
        }
        Ok(())
    }

    fn validate_claude(&self) -> Result<(), String> {
        if self.profile_version != CLAUDE_PROFILE_VERSION
            || self.provider_id != CLAUDE_PROVIDER_ID
            || self.service_tier.is_some()
            || self.http.is_some()
            || self.input_limit_bytes.parse::<usize>().ok() != Some(CLAUDE_INPUT_LIMIT_BYTES)
            || self.output_limit_bytes.parse::<usize>().ok() != Some(CLAUDE_OUTPUT_LIMIT_BYTES)
            || self.reserved_output_bytes != "0"
            || self.reserved_protocol_bytes != "0"
            || self.accounting_method != CLAUDE_TOKEN_ACCOUNTING_METHOD
        {
            return Err("the Claude application profile or byte allowances are invalid".into());
        }
        let Some(runtime) = self.runtime.as_ref() else {
            return Err("the Claude author profile requires observed runtime identity".into());
        };
        if runtime.catalog_sha256.is_some() || !runtime.validate() {
            return Err("the Claude runtime identity is invalid".into());
        }
        let Some(effort) = self.reasoning.as_deref() else {
            return Err("the Claude author profile requires an exact effort".into());
        };
        if !crate::claude_profile::CLAUDE_MODEL_IDS.contains(&self.model_id.as_str())
            || !crate::claude_profile::CLAUDE_EFFORTS.contains(&effort)
        {
            return Err("the Claude model or effort is not in the bounded catalog".into());
        }
        let profile = crate::claude_profile::ClaudeLaunchProfile::for_version(
            &runtime.cli_version,
            &self.model_id,
            Some(effort),
        )
        .map_err(|_| "the Claude CLI version, model, or effort is invalid".to_owned())?;
        if profile.cli_version.as_deref() != Some(runtime.cli_version.as_str())
            || !crate::claude_profile::model_available_for_version(
                &runtime.cli_version,
                &self.model_id,
            )
            .map_err(|_| "the Claude CLI version is invalid".to_owned())?
        {
            return Err("the Claude model is not supported by the observed CLI version".into());
        }
        Ok(())
    }

    pub fn is_http(&self) -> bool {
        self.provider_id.starts_with("openai-compatible:")
    }

    /// Whether this binding uses the fixed HTTP chapter-memory profile.
    /// Keeping this separate from [`Self::is_http`] lets callers distinguish
    /// the memory contract from ordinary author-room HTTP requests without
    /// treating the profile as a provider identity.
    pub fn is_http_memory(&self) -> bool {
        self.is_http()
            && matches!(
                self.profile_version.as_str(),
                HTTP_MEMORY_PROFILE_VERSION | HTTP_MEMORY_LEGACY_PROFILE_VERSION
            )
    }

    fn validate_http(&self) -> Result<(), String> {
        if !matches!(
            self.profile_version.as_str(),
            HTTP_PROFILE_VERSION | HTTP_MEMORY_PROFILE_VERSION | HTTP_MEMORY_LEGACY_PROFILE_VERSION
        ) || self.accounting_method != HTTP_TOKEN_ACCOUNTING_METHOD
            || self.runtime.is_some()
        {
            return Err(
                "the OpenAI-compatible HTTP profile or runtime identity is invalid".to_owned(),
            );
        }
        let profile_id = self
            .provider_id
            .strip_prefix("openai-compatible:")
            .unwrap_or_default();
        let profile_uuid = uuid::Uuid::parse_str(profile_id).map_err(|_| {
            "the OpenAI-compatible provider ID must contain a profile UUID".to_owned()
        })?;
        if profile_uuid.to_string() != profile_id {
            return Err("the OpenAI-compatible provider ID must contain a profile UUID".to_owned());
        }
        validate_http_text(&self.model_id, 256, "HTTP model ID")?;
        validate_http_text(&self.base_http_url()?, 2048, "HTTP endpoint URL")?;
        let http = self.http.as_ref().ok_or_else(|| {
            "the OpenAI-compatible binding is missing its HTTP transport contract".to_owned()
        })?;
        if http.config_revision.is_empty()
            || (http.config_revision.len() > 1 && http.config_revision.starts_with('0'))
            || !http
                .config_revision
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        {
            return Err("the HTTP endpoint configuration revision is not canonical".to_owned());
        }
        if self.input_limit_bytes.parse::<usize>().ok() != Some(HTTP_INPUT_LIMIT_BYTES)
            || self.output_limit_bytes.parse::<usize>().ok()
                != Some(if self.is_http_memory() {
                    HTTP_MEMORY_OUTPUT_LIMIT_BYTES
                } else {
                    HTTP_OUTPUT_LIMIT_BYTES
                })
            || self.reserved_output_bytes != "0"
            || self.reserved_protocol_bytes != "0"
        {
            return Err("the OpenAI-compatible byte allowances are invalid".to_owned());
        }
        if self.is_http_memory() {
            let (model, reasoning) = if self.profile_version == HTTP_MEMORY_LEGACY_PROFILE_VERSION {
                (HTTP_MEMORY_LEGACY_MODEL_ID, HTTP_MEMORY_LEGACY_REASONING)
            } else {
                (HTTP_MEMORY_MODEL_ID, HTTP_MEMORY_REASONING)
            };
            if self.model_id != model
                || self.reasoning.as_deref() != Some(reasoning)
                || self.service_tier.is_some()
            {
                return Err(
                    "the OpenAI-compatible chapter-memory profile has an invalid fixed model or reasoning level"
                        .to_owned(),
                );
            }
        }
        if self.reasoning.as_deref().is_some_and(|value| {
            value.is_empty() || value.len() > 64 || value.chars().any(char::is_control)
        }) || self.service_tier.as_deref().is_some_and(|value| {
            value.is_empty() || value.len() > 64 || value.chars().any(char::is_control)
        }) {
            return Err("the OpenAI-compatible model traits are invalid".to_owned());
        }
        Ok(())
    }

    fn base_http_url(&self) -> Result<String, String> {
        let http = self.http.as_ref().ok_or_else(|| {
            "the OpenAI-compatible binding is missing its HTTP transport contract".to_owned()
        })?;
        let normalized = crate::openai_compatible::normalize_base_url(&http.base_url)
            .map_err(|_| "the HTTP endpoint URL is invalid".to_owned())?;
        if normalized.as_str() != http.base_url
            || !matches!(normalized.scheme(), "http" | "https")
            || normalized.host_str().is_none_or(str::is_empty)
            || !normalized.username().is_empty()
            || normalized.password().is_some()
            || normalized.query().is_some()
            || normalized.fragment().is_some()
            || http
                .base_url
                .chars()
                .any(|value| value.is_control() || value.is_whitespace())
        {
            return Err("the HTTP endpoint URL must be normalized without credentials".to_owned());
        }
        Ok(http.base_url.clone())
    }

    pub fn input_limit(&self) -> Result<usize, String> {
        self.validate()?;
        parse_decimal(&self.input_limit_bytes).and_then(|value| {
            usize::try_from(value).map_err(|_| "input limit is too large".to_owned())
        })
    }

    pub fn output_limit(&self) -> Result<usize, String> {
        self.validate()?;
        parse_decimal(&self.output_limit_bytes).and_then(|value| {
            usize::try_from(value).map_err(|_| "output limit is too large".to_owned())
        })
    }
}

pub(crate) fn validate_http_text(value: &str, max_bytes: usize, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        Err(format!(
            "the {label} is empty, too long, or contains control characters"
        ))
    } else {
        Ok(())
    }
}
/// Provider-facing chat message. The evidence message is a canonical JSON
/// context envelope; the final user content is the instruction byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PacketMessage {
    pub role: String,
    pub content: String,
}

/// Exact options sent with the deterministic packet. Provider-specific
/// options are intentionally deferred until a qualified adapter exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PacketOptions {
    pub model_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub max_output_tokens: String,
    pub token_accounting_method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_binding: Option<ProviderBinding>,
}
pub fn parse_decimal(value: &str) -> Result<u128, String> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return Err("expected a canonical nonnegative decimal string".to_owned());
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("expected a canonical nonnegative decimal string".to_owned());
    }
    value
        .parse::<u128>()
        .map_err(|_| "decimal value exceeds the supported counter range".to_owned())
}


// ---------------------------------------------------------------------------
// Provider delivery vocabulary.
//
// Moved down from `webnovel-core::projects::discussions`, where they sat in a
// 4,619-line module bound for `wns-conversation` (L5). Three modules use them
// and they land in *different* crates: `memory` goes to `wns-story` (L4), and
// `discussions` and `discussion_lookup` to `wns-conversation` (L5). Vocabulary
// that two future siblings both need has to sit below both, so `memory`'s
// `use crate::projects::discussions::{...}` was an L4->L5 upward edge that no
// amount of moving either module could resolve.
//
// This is the second time this exact shape has been fixed here, and the
// argument is the one in this file's own header: these are provider contract
// types — an outcome, a cleanup state, a delivery receipt — not discussion
// logic, and they lived in `discussions` only because that is where the first
// adapter needing them happened to be written.
//
// Byte-compatibility is unaffected: a serialized receipt is identical whichever
// crate owns the type, and `discussions` re-exports every item at its
// historical path.
// ---------------------------------------------------------------------------

use wns_kernel::{CoreError, CoreResult};
/// The provider-side outcome is kept separate from the discussion lifecycle.
/// For example, a timed-out provider request with settled cleanup becomes a
/// durable failed discussion while retaining any validated prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderOutcomeStatus {
    Completed,
    Stopped,
    TimedOut,
    OutputLimit,
    Failed,
}

impl ProviderOutcomeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Stopped => "stopped",
            Self::TimedOut => "timed_out",
            Self::OutputLimit => "output_limit",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "completed" => Ok(Self::Completed),
            "stopped" => Ok(Self::Stopped),
            "timed_out" => Ok(Self::TimedOut),
            "output_limit" => Ok(Self::OutputLimit),
            "failed" => Ok(Self::Failed),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved provider result has an unknown outcome.",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderCleanup {
    Settled,
    Unresolved,
}

impl ProviderCleanup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Settled => "settled",
            Self::Unresolved => "unresolved",
        }
    }

    pub fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "settled" => Ok(Self::Settled),
            "unresolved" => Ok(Self::Unresolved),
            _ => Err(CoreError::new(
                "InvalidProject",
                "The saved provider result has an unknown cleanup state.",
            )),
        }
    }
}

/// Evidence about the HTTP request itself.  This is intentionally separate
/// from Codex's local stdin count: an HTTP request can be accepted by a remote
/// server even when the local process loses the response before it is parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HttpDeliverySubmission {
    NotSent,
    Uncertain,
    ResponseReceived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpProviderUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDeliveryReceipt {
    pub body_hash: String,
    pub body_bytes: String,
    pub submission: HttpDeliverySubmission,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<HttpProviderUsage>,
}

/// Raw provider usage is optional. Missing usage is an explicit unknown value;
/// no estimate is substituted from the packet's byte accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}
