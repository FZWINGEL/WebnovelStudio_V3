//! Pure launch profile for the exact Codex CLI surface qualified by W8.
//!
//! This module only constructs arguments, `-c` overrides, and a restrictive
//! model catalog.  It does not locate an executable, read Codex state, inspect
//! credentials, write the catalog, or start a child process.

use serde::Serialize;
use serde_json::Value;
use std::ffi::OsString;
use std::path::Path;

pub const CODEX_CLI_VERSION: &str = "0.153.3";
pub const CODEX_LUNA_MODEL: &str = "gpt-5.6-luna";
pub const CODEX_MAX_EFFORT: &str = "max";
pub const CODEX_PRIORITY_SERVICE_TIER: &str = "priority";
pub const CODEX_FAST_TIER_LABEL: &str = "Fast";

const MAX_VERSION_OUTPUT_BYTES: usize = 512;

/// The exact version and local launch material that a later adapter may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexLaunchProfile {
    pub executable_version: &'static str,
    pub model: &'static str,
    pub reasoning_effort: &'static str,
    pub service_tier: &'static str,
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
        validate_version_output(version_output)?;
        let catalog = CodexModelCatalog::restrictive_luna();
        let catalog_json = catalog
            .to_json()
            .map_err(|error| CodexProfileError::CatalogSerialization(error.to_string()))?;
        let catalog_path = config_path(catalog_path)?;

        Ok(Self {
            executable_version: CODEX_CLI_VERSION,
            model: CODEX_LUNA_MODEL,
            reasoning_effort: CODEX_MAX_EFFORT,
            service_tier: CODEX_PRIORITY_SERVICE_TIER,
            catalog,
            catalog_json,
            config_overrides: restrictive_overrides(&catalog_path),
        })
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
            self.model,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexProfileError {
    UnsupportedVersion {
        expected: &'static str,
        observed: String,
    },
    InvalidPath(String),
    CatalogSerialization(String),
}

impl std::fmt::Display for CodexProfileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion { expected, observed } => write!(
                formatter,
                "unsupported Codex CLI version; expected {expected}, observed {observed}"
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

fn validate_version_output(version_output: &str) -> Result<(), CodexProfileError> {
    let exact = version_output
        .split_whitespace()
        .any(|token| token.strip_prefix('v').unwrap_or(token) == CODEX_CLI_VERSION);
    if exact {
        return Ok(());
    }
    let observed = version_output
        .trim()
        .chars()
        .take(MAX_VERSION_OUTPUT_BYTES)
        .collect();
    Err(CodexProfileError::UnsupportedVersion {
        expected: CODEX_CLI_VERSION,
        observed,
    })
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

    // These are the exact 0.153.3 feature keys needed to close the core and
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
    pub default_service_tier: String,
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
            default_service_tier: CODEX_PRIORITY_SERVICE_TIER.to_owned(),
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
