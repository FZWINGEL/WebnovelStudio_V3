//! Pure launch material for the bounded Claude Code CLI author surface.
//!
//! This module does not locate Claude, inspect authentication, read a
//! settings file, or start a process.  The native runtime supplies the
//! executable and an application-owned working directory.  The profile only
//! validates the selected full model ID and effort, then builds the fixed
//! non-interactive argument vector used by `super::claude_runner`.

use std::ffi::OsString;

/// Stable application identity for Claude author requests.
pub const CLAUDE_PROFILE_VERSION: &str = "claude-stdin.author.v1";
/// Application cap for one exact serialized packet written to stdin.
pub const CLAUDE_INPUT_LIMIT_BYTES: usize = 24 * 1024;
/// Application cap for retained combined provider output.
pub const CLAUDE_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
/// Usage accounting is provider-reported when Claude includes it.  The
/// application cap is not a model context-window claim.
pub const CLAUDE_TOKEN_ACCOUNTING_METHOD: &str = "utf8-byte-count/claude-stdin-application-cap-v1";
/// Fixed application instruction. Story text and author instructions remain
/// in the exact serialized stdin packet and are never interpolated into CLI
/// arguments.
pub const CLAUDE_SYSTEM_PROMPT: &str = "You are WebnovelStudio's English fiction assistant. The stdin JSON contains ordered messages and request options. Follow those instructions, use story material only as evidence, and return only the requested answer or structured response. No tools or file access are available.";

pub const CLAUDE_FABLE_MODEL: &str = "claude-fable-5";
pub const CLAUDE_OPUS_MODEL: &str = "claude-opus-5";
pub const CLAUDE_SONNET_MODEL: &str = "claude-sonnet-5";

/// The full model IDs currently exposed by V2's version-gated Claude catalog.
/// Claude does not provide a model-list protocol used by this adapter; the
/// native runtime gates these rows with its observed CLI version.
pub const CLAUDE_MODEL_IDS: &[&str] = &[CLAUDE_FABLE_MODEL, CLAUDE_OPUS_MODEL, CLAUDE_SONNET_MODEL];

/// Effort values accepted by Claude's `--effort` flag for this slice.  Values
/// such as `ultrathink` and `ultracode` are prompt/workflow features rather
/// than exact CLI effort values and are intentionally excluded.
pub const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// Empty MCP configuration passed with strict validation so a user or project
/// MCP file cannot silently add tools to an author request.
pub const EMPTY_MCP_CONFIG: &str = r#"{"mcpServers":{}}"#;

/// Claude launch material after the selected author settings have been
/// validated.  `cli_version` is descriptive observed identity only; it does
/// not pin a version or authorize dispatch by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeLaunchProfile {
    pub cli_version: Option<String>,
    pub model: String,
    pub effort: Option<String>,
}

/// Short name for callers that do not need the more explicit launch-profile
/// spelling.
pub type ClaudeProfile = ClaudeLaunchProfile;

impl ClaudeLaunchProfile {
    /// Validate a full model ID and optional exact CLI effort without making a
    /// version or installation decision.
    pub fn new(model: &str, effort: Option<&str>) -> Result<Self, ClaudeProfileError> {
        Self::build(None, model, effort)
    }

    /// Build the same profile while retaining a bounded observed version
    /// token for the later runtime binding.  Any valid version is accepted;
    /// capability gating remains a runtime/catalog concern.
    pub fn for_version(
        version_output: &str,
        model: &str,
        effort: Option<&str>,
    ) -> Result<Self, ClaudeProfileError> {
        let version = parse_version_output(version_output)?;
        Self::build(Some(version), model, effort)
    }

    /// Build the fixed, non-interactive Claude argument vector.  The process
    /// caller supplies its application-owned cwd through `CliInvocation`; no
    /// author directory is ever placed in the arguments.
    pub fn arguments(&self) -> Vec<OsString> {
        let mut arguments = vec![
            OsString::from("-p"),
            OsString::from("--safe-mode"),
            OsString::from("--model"),
            OsString::from(&self.model),
            OsString::from("--input-format"),
            OsString::from("text"),
            OsString::from("--output-format"),
            OsString::from("stream-json"),
            OsString::from("--verbose"),
            OsString::from("--include-partial-messages"),
            OsString::from("--no-session-persistence"),
            OsString::from("--tools"),
            OsString::new(),
            OsString::from("--permission-mode"),
            OsString::from("dontAsk"),
            OsString::from("--strict-mcp-config"),
            OsString::from("--mcp-config"),
            OsString::from(EMPTY_MCP_CONFIG),
            OsString::from("--disable-slash-commands"),
            OsString::from("--no-chrome"),
            OsString::from("--setting-sources"),
            OsString::new(),
            OsString::from("--system-prompt"),
            OsString::from(CLAUDE_SYSTEM_PROMPT),
        ];
        if let Some(effort) = &self.effort {
            arguments.extend([OsString::from("--effort"), OsString::from(effort)]);
        }
        arguments
    }

    /// The selected model's exact ID, retained separately from any model name
    /// Claude reports in a stream result.
    pub fn requested_model(&self) -> &str {
        &self.model
    }

    fn build(
        cli_version: Option<String>,
        model: &str,
        effort: Option<&str>,
    ) -> Result<Self, ClaudeProfileError> {
        if !CLAUDE_MODEL_IDS.contains(&model) {
            return Err(ClaudeProfileError::UnsupportedModel {
                observed: bounded_value(model),
            });
        }
        if let Some(effort) = effort
            && !CLAUDE_EFFORTS.contains(&effort)
        {
            return Err(ClaudeProfileError::UnsupportedEffort {
                observed: bounded_value(effort),
            });
        }
        Ok(Self {
            cli_version,
            model: model.to_owned(),
            effort: effort.map(str::to_owned),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeProfileError {
    UnsupportedModel { observed: String },
    UnsupportedEffort { observed: String },
    InvalidVersionOutput,
}

impl std::fmt::Display for ClaudeProfileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedModel { .. } => formatter
                .write_str("the selected Claude model is not in the bounded full-ID catalog"),
            Self::UnsupportedEffort { .. } => formatter
                .write_str("the selected Claude effort is not an exact supported CLI value"),
            Self::InvalidVersionOutput => {
                formatter.write_str("Claude --version output is not a bounded version token")
            }
        }
    }
}

impl std::error::Error for ClaudeProfileError {}

fn parse_version_output(value: &str) -> Result<String, ClaudeProfileError> {
    if value.is_empty() || value.len() > 512 {
        return Err(ClaudeProfileError::InvalidVersionOutput);
    }
    let trimmed = value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .or_else(|| value.strip_suffix('\r'))
        .unwrap_or(value);
    if trimmed.is_empty() || trimmed.chars().next().is_some_and(char::is_whitespace) {
        return Err(ClaudeProfileError::InvalidVersionOutput);
    }
    let token = trimmed.split_whitespace().next().unwrap_or_default();
    let mut numeric_parts = token.splitn(3, '.');
    let major = numeric_parts.next().unwrap_or_default();
    let minor = numeric_parts.next().unwrap_or_default();
    let patch_and_suffix = numeric_parts.next().unwrap_or_default();
    let suffix_start = patch_and_suffix.find(['-', '+']);
    let patch = suffix_start.map_or(patch_and_suffix, |index| &patch_and_suffix[..index]);
    let suffix = patch_and_suffix.strip_prefix(patch).unwrap_or_default();
    let valid = !trimmed.is_empty()
        && token.len() <= 128
        && major.bytes().all(|byte| byte.is_ascii_digit())
        && minor.bytes().all(|byte| byte.is_ascii_digit())
        && patch.bytes().all(|byte| byte.is_ascii_digit())
        && !major.is_empty()
        && !minor.is_empty()
        && !patch.is_empty()
        && (suffix.is_empty()
            || (matches!(suffix.as_bytes()[0], b'-' | b'+')
                && suffix.len() > 1
                && suffix[1..].bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+')
                })))
        && !trimmed.chars().any(char::is_control)
        && !trimmed.contains('\n')
        && !trimmed.contains('\r');
    if !valid {
        return Err(ClaudeProfileError::InvalidVersionOutput);
    }
    Ok(token.to_owned())
}

fn bounded_value(value: &str) -> String {
    value.chars().take(128).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_restrictive_stream_json_arguments_without_tools_or_session_persistence() {
        let profile = ClaudeProfile::new(CLAUDE_OPUS_MODEL, Some("xhigh")).unwrap();
        let arguments = profile.arguments();
        let as_strings = arguments
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(as_strings[0], "-p");
        assert!(
            as_strings
                .windows(2)
                .any(|pair| pair == ["--model", CLAUDE_OPUS_MODEL])
        );
        assert!(
            as_strings
                .windows(2)
                .any(|pair| pair == ["--effort", "xhigh"])
        );
        assert!(as_strings.windows(2).any(|pair| pair == ["--tools", ""]));
        assert!(
            as_strings
                .windows(2)
                .any(|pair| pair == ["--setting-sources", ""])
        );
        assert!(as_strings.contains(&"--safe-mode".to_owned()));
        assert!(as_strings.contains(&"--strict-mcp-config".to_owned()));
        assert!(as_strings.contains(&"--no-session-persistence".to_owned()));
        assert!(as_strings.contains(&"--include-partial-messages".to_owned()));
        assert!(
            as_strings
                .windows(2)
                .any(|pair| pair == ["--system-prompt", CLAUDE_SYSTEM_PROMPT])
        );
        assert_eq!(
            as_strings
                .iter()
                .filter(|value| value.as_str() == "--system-prompt")
                .count(),
            1
        );
        assert!(!as_strings.contains(&"--dangerously-skip-permissions".to_owned()));
        assert!(!as_strings.contains(&"ultrathink".to_owned()));
    }

    #[test]
    fn accepts_observed_versions_without_pinning_and_rejects_unsafe_values() {
        let profile =
            ClaudeProfile::for_version("2.1.220 (Claude Code)\n", CLAUDE_SONNET_MODEL, None)
                .unwrap();
        assert_eq!(profile.cli_version.as_deref(), Some("2.1.220"));
        assert!(ClaudeProfile::for_version("999.0.0", CLAUDE_FABLE_MODEL, Some("max")).is_ok());
        assert!(matches!(
            ClaudeProfile::new("sonnet", None),
            Err(ClaudeProfileError::UnsupportedModel { .. })
        ));
        assert!(matches!(
            ClaudeProfile::new(CLAUDE_FABLE_MODEL, Some("ultrathink")),
            Err(ClaudeProfileError::UnsupportedEffort { .. })
        ));
        assert!(matches!(
            ClaudeProfile::for_version("Claude Code\n", CLAUDE_FABLE_MODEL, None),
            Err(ClaudeProfileError::InvalidVersionOutput)
        ));
        assert!(matches!(
            ClaudeProfile::for_version("...\n", CLAUDE_FABLE_MODEL, None),
            Err(ClaudeProfileError::InvalidVersionOutput)
        ));
        assert!(ClaudeProfile::for_version("2.1.220\r\n", CLAUDE_FABLE_MODEL, None).is_ok());
        for malformed in [
            " 2.1.220\n",
            "2.1.220\t(Claude Code)\n",
            "2.1.220-\n",
            "2.1.220.1\n",
        ] {
            assert!(matches!(
                ClaudeProfile::for_version(malformed, CLAUDE_FABLE_MODEL, None),
                Err(ClaudeProfileError::InvalidVersionOutput)
            ));
        }
    }
}
