//! Pure launch profile for the capability-gated Codex CLI surface.
//!
//! This module only constructs arguments, `-c` overrides, and a restrictive
//! model catalog.  It does not locate an executable, read Codex state, inspect
//! credentials, write the catalog, or start a child process.

use serde::Serialize;
use serde_json::Value;
use std::ffi::OsString;
use std::path::Path;

/// Stable application identity for the stdin-based Codex adapter. The
/// observed CLI version is recorded separately and is capability-gated at
/// connection time.
pub const CODEX_PROFILE_VERSION: &str = "codex-stdin.v1";
/// Author-selected models use a distinct contract so earlier packets retain
/// their fixed Luna settings and exact serialized identity.
pub const CODEX_AUTHOR_PROFILE_VERSION: &str = "codex-stdin.author.v1";
pub const CODEX_LUNA_MODEL: &str = "gpt-5.6-luna";
pub const CODEX_REASONING_EFFORT: &str = "xhigh";
/// Compatibility name for callers that still refer to the old constant.
pub const CODEX_MAX_EFFORT: &str = CODEX_REASONING_EFFORT;
pub const CODEX_PRIORITY_SERVICE_TIER: &str = "priority";
pub const CODEX_FAST_TIER_LABEL: &str = "Fast";

const MAX_VERSION_OUTPUT_BYTES: usize = 512;
const MAX_VERSION_TOKEN_BYTES: usize = 128;

/// The exact version and local launch material that a later adapter may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexLaunchProfile {
    /// The CLI version observed from this executable, without the `codex-cli`
    /// prefix. This is descriptive metadata, not an allowlist decision.
    pub executable_version: String,
    pub model: String,
    pub reasoning_effort: String,
    pub service_tier: Option<String>,
    pub catalog: CodexModelCatalog,
    pub catalog_json: String,
    pub config_overrides: Vec<String>,
}

impl CodexLaunchProfile {
    /// Build the restrictive profile after the caller has obtained `--version`.
    ///
    /// `catalog_path` is only rendered into the `model_catalog_json` override;
    /// this function never checks, opens, or creates that path.
    pub fn for_version(
        version_output: &str,
        catalog_path: &Path,
    ) -> Result<Self, CodexProfileError> {
        let executable_version = parse_version_output(version_output)?;
        let catalog = CodexModelCatalog::restrictive_luna();
        let catalog_json = catalog
            .to_json()
            .map_err(|error| CodexProfileError::CatalogSerialization(error.to_string()))?;
        let catalog_path = config_path(catalog_path)?;

        Ok(Self {
            executable_version,
            model: CODEX_LUNA_MODEL.into(),
            reasoning_effort: CODEX_MAX_EFFORT.into(),
            service_tier: Some(CODEX_PRIORITY_SERVICE_TIER.into()),
            catalog,
            catalog_json,
            config_overrides: restrictive_overrides(&catalog_path),
        })
    }

    /// Build an isolated author invocation from one validated discovery row.
    /// Null author traits resolve once to the observed catalog defaults. The
    /// caller freezes those resolved values in the request binding.
    pub fn for_selection(
        version_output: &str,
        catalog_path: &Path,
        model: &super::codex_catalog::CodexCatalogModel,
        choice: &super::preferences::ModelSelection,
    ) -> Result<Self, CodexProfileError> {
        if model.validate().is_err()
            || choice.provider_id != "codex"
            || choice.model_id != model.model_id
            || choice
                .reasoning
                .as_ref()
                .is_some_and(|value| !model.reasoning_levels.contains(value))
            || choice
                .service_tier
                .as_ref()
                .is_some_and(|value| !model.service_tiers.iter().any(|tier| tier.id == *value))
        {
            return Err(CodexProfileError::InvalidSelection);
        }
        let mut profile = Self::for_version(version_output, catalog_path)?;
        profile.model = model.model_id.clone();
        profile.reasoning_effort = choice
            .reasoning
            .clone()
            .or_else(|| model.default_reasoning.clone())
            .ok_or(CodexProfileError::InvalidSelection)?;
        profile.service_tier = choice
            .service_tier
            .clone()
            .or_else(|| model.default_service_tier.clone());
        let descriptor = &mut profile.catalog.models[0];
        descriptor.slug = model.model_id.clone();
        descriptor.display_name = model.label.clone();
        descriptor.description =
            "Author-selected model reported by the installed Codex CLI.".into();
        descriptor.default_reasoning_level = profile.reasoning_effort.clone();
        descriptor.supported_reasoning_levels = model
            .reasoning_levels
            .iter()
            .map(|effort| CodexReasoningPreset {
                effort: effort.clone(),
                description: String::new(),
            })
            .collect();
        descriptor.service_tiers = model
            .service_tiers
            .iter()
            .map(|tier| CodexServiceTier {
                id: tier.id.clone(),
                name: tier.label.clone(),
                description: String::new(),
            })
            .collect();
        descriptor.default_service_tier = profile.service_tier.clone();
        descriptor.additional_speed_tiers.clear();
        // These optional request parameters are not reported by model/list.
        // Do not copy Luna's provider-specific support claims to other models.
        descriptor.supports_reasoning_summary_parameter = false;
        descriptor.support_verbosity = false;
        // Responses Lite was qualified for Luna. Other discovered models can
        // reject that transport, and model/list does not declare support.
        descriptor.use_responses_lite = model.model_id == CODEX_LUNA_MODEL;
        profile.catalog_json = profile
            .catalog
            .to_json()
            .map_err(|error| CodexProfileError::CatalogSerialization(error.to_string()))?;
        profile.config_overrides.retain(|value| {
            !["model=", "model_reasoning_effort=", "service_tier="]
                .iter()
                .any(|prefix| value.starts_with(prefix))
        });
        profile
            .config_overrides
            .insert(0, format!("model={}", toml_string(&profile.model)));
        profile.config_overrides.insert(
            1,
            format!(
                "model_reasoning_effort={}",
                toml_string(&profile.reasoning_effort)
            ),
        );
        if let Some(tier) = &profile.service_tier {
            profile
                .config_overrides
                .insert(2, format!("service_tier={}", toml_string(tier)));
        }
        Ok(profile)
    }

    /// Arguments for the existing stdin packet path in `cli::windows_process`.
    /// The packet itself is deliberately absent and must be written to stdin.
    pub fn exec_arguments(&self, cwd: &Path) -> Result<Vec<OsString>, CodexProfileError> {
        let cwd = config_path(cwd)?;
        let mut arguments = [
            "exec",
            "--strict-config",
            "--ignore-user-config",
            "--ignore-rules",
            "--ephemeral",
            "--cd",
            cwd.as_str(),
            "--skip-git-repo-check",
            "--color",
            "never",
            "--json",
            "-m",
            self.model.as_str(),
        ]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
        for override_value in &self.config_overrides {
            arguments.push(OsString::from("-c"));
            arguments.push(OsString::from(override_value));
        }
        // `-` is the documented exec positional for a prompt read from stdin.
        arguments.push(OsString::from("-"));
        Ok(arguments)
    }

    /// Build the same stdin invocation with an intentionally missing output
    /// schema. The native runtime uses this before accepting a connection so
    /// strict configuration and the JSON/exec surface are tested without a
    /// provider request.
    pub fn preflight_arguments(
        &self,
        cwd: &Path,
        missing_schema: &Path,
        unknown_override: Option<&str>,
    ) -> Result<Vec<OsString>, CodexProfileError> {
        let mut arguments = self.exec_arguments(cwd)?;
        let stdin_position = arguments
            .iter()
            .rposition(|argument| argument == "-")
            .expect("exec_arguments always ends with stdin marker");
        arguments.splice(
            stdin_position..stdin_position,
            [
                OsString::from("--output-schema"),
                OsString::from(config_path(missing_schema)?),
            ],
        );
        if let Some(override_value) = unknown_override {
            arguments.splice(
                stdin_position..stdin_position,
                [OsString::from("-c"), OsString::from(override_value)],
            );
        }
        Ok(arguments)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexProfileError {
    InvalidSelection,
    InvalidVersionOutput { observed: String },
    InvalidPath(String),
    CatalogSerialization(String),
}

impl std::fmt::Display for CodexProfileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSelection => write!(
                formatter,
                "the selected model settings were not reported by Codex"
            ),
            Self::InvalidVersionOutput { observed } => write!(
                formatter,
                "Codex --version output must be a bounded `codex-cli <version>` value; observed {observed}"
            ),
            Self::InvalidPath(path) => {
                write!(formatter, "path cannot be used in Codex config: {path}")
            }
            Self::CatalogSerialization(error) => {
                write!(
                    formatter,
                    "failed to serialize Codex model catalog: {error}"
                )
            }
        }
    }
}

impl std::error::Error for CodexProfileError {}

fn parse_version_output(version_output: &str) -> Result<String, CodexProfileError> {
    let observed = version_output.trim();
    let bounded_observed = observed.chars().take(MAX_VERSION_OUTPUT_BYTES).collect();
    if observed.is_empty() || observed.len() > MAX_VERSION_OUTPUT_BYTES {
        return Err(CodexProfileError::InvalidVersionOutput {
            observed: bounded_observed,
        });
    }
    let mut fields = observed.split_whitespace();
    let Some(prefix) = fields.next() else {
        return Err(CodexProfileError::InvalidVersionOutput {
            observed: bounded_observed,
        });
    };
    let Some(version) = fields.next() else {
        return Err(CodexProfileError::InvalidVersionOutput {
            observed: bounded_observed,
        });
    };
    if prefix != "codex-cli"
        || fields.next().is_some()
        || version.is_empty()
        || version.len() > MAX_VERSION_TOKEN_BYTES
        || !version.as_bytes()[0].is_ascii_digit()
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return Err(CodexProfileError::InvalidVersionOutput {
            observed: bounded_observed,
        });
    }
    Ok(version.to_owned())
}

fn config_path(path: &Path) -> Result<String, CodexProfileError> {
    let path = path
        .to_str()
        .ok_or_else(|| CodexProfileError::InvalidPath("path is not valid UTF-8".to_owned()))?;
    if path.chars().any(char::is_control) {
        return Err(CodexProfileError::InvalidPath(
            "path contains a control character".to_owned(),
        ));
    }
    Ok(path.replace('\\', "/"))
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn restrictive_overrides(catalog_path: &str) -> Vec<String> {
    let mut overrides = vec![
        format!("model={}", toml_string(CODEX_LUNA_MODEL)),
        format!("model_reasoning_effort={}", toml_string(CODEX_MAX_EFFORT)),
        format!("service_tier={}", toml_string(CODEX_PRIORITY_SERVICE_TIER)),
        format!("model_catalog_json={}", toml_string(catalog_path)),
        "approval_policy=\"never\"".to_owned(),
        "web_search=\"disabled\"".to_owned(),
        "default_permissions=\"story-context\"".to_owned(),
        "project_doc_max_bytes=0".to_owned(),
        "include_apps_instructions=false".to_owned(),
        "include_permissions_instructions=false".to_owned(),
        "include_collaboration_mode_instructions=false".to_owned(),
        "mcp_servers={}".to_owned(),
        "plugins={}".to_owned(),
        "agents.enabled=false".to_owned(),
        "skills.include_instructions=false".to_owned(),
        "skills.bundled.enabled=false".to_owned(),
        "apps._default.enabled=false".to_owned(),
        "tools.experimental_request_user_input.enabled=false".to_owned(),
        "tools.update_plan.enabled=false".to_owned(),
        "features.tool_registry.turn_metadata_includes_tool_info=false".to_owned(),
        "features.tool_registry.error_on_tool_collisions=true".to_owned(),
        "permissions.story-context.filesystem.:root=\"deny\"".to_owned(),
        "permissions.story-context.filesystem.:minimal=\"read\"".to_owned(),
        "permissions.story-context.filesystem.:workspace_roots=\"read\"".to_owned(),
        "permissions.story-context.network.enabled=false".to_owned(),
    ];

    // These are the exact 0.153.4 feature keys needed to close the core and
    // installed extension registration paths. Apply-patch availability is
    // controlled by model metadata; tool search is removed in this release.
    for (key, enabled) in [
        ("shell_tool", false),
        ("unified_exec", false),
        ("request_permissions_tool", false),
        ("view_image", false),
        ("sleep_tool", false),
        ("deferred_executor", false),
        ("exec_permission_approvals", false),
        ("write_stdin_approval", false),
        ("token_budget", false),
        ("current_time_reminder", false),
        ("multi_agent", false),
        ("multi_agent_v2", false),
        ("apps", false),
        ("plugins", false),
        ("enable_mcp_apps", false),
        ("mcp_2026_07_28", false),
        ("mcp_oauth_refresh_coordination", false),
        ("non_prefixed_mcp_tool_names", false),
        ("skill_search", false),
        ("skill_mcp_dependency_install", false),
        ("skip_host_skill_discovery", true),
        ("tool_suggest", false),
        ("memories", false),
        ("image_generation", false),
        ("standalone_web_search", false),
        ("browser_use", false),
        ("browser_use_full_cdp_access", false),
        ("browser_use_external", false),
        ("computer_use", false),
        ("recommended_plugins", false),
        ("code_mode", false),
        ("code_mode_only", false),
        ("code_mode_host", false),
        ("code_mode_prewarm", false),
        ("code_mode_interrupt", false),
        ("hooks", false),
        ("goals", false),
        ("guardian_approval", false),
        ("guardianv2", false),
        ("remote_plugin", false),
        ("plugin_sharing", false),
        ("in_app_browser", false),
        ("in_app_chat", false),
        ("in_app_dictation", false),
        ("in_app_local_automation", false),
        ("in_app_updates", false),
        ("workspace_dependencies", false),
        ("shell_snapshot", false),
        ("executor_capability_discovery", false),
        ("unbounded_connection_retries", false),
        ("prevent_idle_sleep", false),
        ("tool_call_mcp_elicitation", false),
        ("auth_elicitation", false),
    ] {
        overrides.push(format!("features.{key}={enabled}"));
    }
    overrides
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexModelCatalog {
    pub models: Vec<CodexModelDescriptor>,
}

impl CodexModelCatalog {
    pub fn restrictive_luna() -> Self {
        Self {
            models: vec![CodexModelDescriptor::restrictive_luna()],
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexModelDescriptor {
    pub slug: String,
    pub display_name: String,
    pub description: String,
    pub default_reasoning_level: String,
    pub supported_reasoning_levels: Vec<CodexReasoningPreset>,
    pub shell_type: String,
    pub visibility: String,
    pub supported_in_api: bool,
    pub priority: i32,
    pub availability_nux: Option<Value>,
    pub upgrade: Option<Value>,
    pub additional_speed_tiers: Vec<String>,
    pub service_tiers: Vec<CodexServiceTier>,
    pub default_service_tier: Option<String>,
    pub model_messages: Value,
    pub include_skills_usage_instructions: bool,
    pub include_plugin_usage_instructions: bool,
    pub include_apps_usage_instructions: bool,
    pub supports_reasoning_summary_parameter: bool,
    pub default_reasoning_summary: String,
    pub support_verbosity: bool,
    pub default_verbosity: String,
    pub apply_patch_tool_type: Option<String>,
    pub web_search_tool_type: String,
    pub truncation_policy: CodexTruncationPolicy,
    pub supports_image_detail_original: bool,
    pub context_window: Option<i64>,
    pub max_context_window: Option<i64>,
    pub auto_compact_token_limit: Option<i64>,
    pub effective_context_window_percent: i64,
    pub experimental_supported_tools: Vec<String>,
    pub input_modalities: Vec<String>,
    pub supports_search_tool: bool,
    pub use_responses_lite: bool,
    pub node_repl_auto_review_required: bool,
    pub node_repl_disabled: bool,
    pub tool_mode: String,
    pub multi_agent_version: Option<String>,
}

impl CodexModelDescriptor {
    fn restrictive_luna() -> Self {
        Self {
            slug: CODEX_LUNA_MODEL.to_owned(),
            display_name: "GPT-5.6-Luna".to_owned(),
            description: "Fast and affordable agentic coding model.".to_owned(),
            default_reasoning_level: "medium".to_owned(),
            supported_reasoning_levels: ["low", "medium", "high", "xhigh", "max"]
                .into_iter()
                .map(|effort| CodexReasoningPreset {
                    effort: effort.to_owned(),
                    description: String::new(),
                })
                .collect(),
            shell_type: "disabled".to_owned(),
            visibility: "list".to_owned(),
            supported_in_api: true,
            priority: 8,
            availability_nux: None,
            upgrade: None,
            additional_speed_tiers: vec!["fast".to_owned()],
            service_tiers: vec![CodexServiceTier {
                id: CODEX_PRIORITY_SERVICE_TIER.to_owned(),
                name: CODEX_FAST_TIER_LABEL.to_owned(),
                description: "1.5x speed, increased usage".to_owned(),
            }],
            default_service_tier: Some(CODEX_PRIORITY_SERVICE_TIER.to_owned()),
            model_messages: serde_json::json!({
                "persistent_instructions": "",
                "instructions_template": "",
                "instructions_variables": null,
                "tools": null,
                "approvals": null,
                "collaboration_modes": null,
                "auto_review": null,
                "permissions": null,
                "multi_agent": null,
                "token_budget": null,
                "guardian_v2": null,
                "confirmation_policies": null
            }),
            include_skills_usage_instructions: false,
            include_plugin_usage_instructions: false,
            include_apps_usage_instructions: false,
            supports_reasoning_summary_parameter: true,
            default_reasoning_summary: "none".to_owned(),
            support_verbosity: true,
            default_verbosity: "low".to_owned(),
            apply_patch_tool_type: None,
            web_search_tool_type: "text".to_owned(),
            truncation_policy: CodexTruncationPolicy {
                mode: "tokens".to_owned(),
                limit: 10_000,
            },
            supports_image_detail_original: false,
            // The live model/list response did not expose context limits.  Keep
            // these absent instead of promoting cache-only values to a contract.
            context_window: None,
            max_context_window: None,
            auto_compact_token_limit: None,
            effective_context_window_percent: 95,
            experimental_supported_tools: Vec::new(),
            input_modalities: vec!["text".to_owned()],
            supports_search_tool: false,
            use_responses_lite: true,
            node_repl_auto_review_required: false,
            node_repl_disabled: true,
            tool_mode: "direct".to_owned(),
            multi_agent_version: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexReasoningPreset {
    pub effort: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexServiceTier {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexTruncationPolicy {
    pub mode: String,
    pub limit: i64,
}
