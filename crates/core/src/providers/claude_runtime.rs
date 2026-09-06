//! Checked native Claude installation and isolated, single-invocation launches.
//! Connection probes never send a manuscript or start a generation. Account
//! metadata and raw CLI diagnostics remain inside the bounded probe parser.

use super::claude_profile::{
    CLAUDE_FABLE_MODEL, CLAUDE_INPUT_LIMIT_BYTES, CLAUDE_OPUS_MODEL, ClaudeLaunchProfile,
};
use super::claude_runner::ClaudeStream;
use super::cli::windows_process::{
    self, ChildLimits, ChildOutcome, ChildTermination, CliInvocation, EnvironmentPolicy, StopSignal,
};
use crate::projects::{CoreError, CoreResult};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::Read,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

const REQUIRED_OPTIONS: &[&str] = &[
    "--print",
    "--safe-mode",
    "--model",
    "--effort",
    "--input-format",
    "--output-format",
    "--verbose",
    "--include-partial-messages",
    "--no-session-persistence",
    "--tools",
    "--permission-mode",
    "--strict-mcp-config",
    "--mcp-config",
    "--disable-slash-commands",
    "--no-chrome",
    "--setting-sources",
    "--system-prompt",
];

#[derive(Clone, Debug)]
pub struct ClaudeConnection {
    install_path: PathBuf,
    observed_version: String,
    fingerprint: String,
}

impl ClaudeConnection {
    pub fn version(&self) -> &str {
        &self.observed_version
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Check the standard native installation, never a shell shim or a
    /// renderer-supplied command. Picker opens do not run these probes.
    pub fn check_installed() -> CoreResult<Self> {
        let install_path = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .ok_or_else(|| {
                unavailable(
                    "ClaudeUnavailable",
                    "The native Claude installation could not be found.",
                )
            })?
            .join(".local")
            .join("bin")
            .join("claude.exe");
        let pinned = pin_executable(&install_path)?;
        let owned = OwnedRun::new()?;
        let version = probe(&pinned.path, &owned.cwd, &["--safe-mode", "--version"])?;
        let version_text = successful_text(&version)?;
        let profile =
            ClaudeLaunchProfile::for_version(&version_text, CLAUDE_FABLE_MODEL, Some("high"))
                .map_err(|_| {
                    unavailable(
                        "ClaudeCapabilityUnavailable",
                        "Claude returned an unsupported version response.",
                    )
                })?;
        let observed_version = profile.cli_version.ok_or_else(|| {
            unavailable(
                "ClaudeCapabilityUnavailable",
                "Claude did not report its version.",
            )
        })?;
        let help = probe(&pinned.path, &owned.cwd, &["--safe-mode", "--help"])?;
        validate_help(&successful_text(&help)?)?;
        let auth = probe(
            &pinned.path,
            &owned.cwd,
            &["--safe-mode", "auth", "status", "--json"],
        )?;
        validate_auth(&auth)?;
        Ok(Self {
            install_path,
            observed_version,
            fingerprint: pinned.fingerprint,
        })
    }

    /// Static V2 reference-model version gates, not live account entitlement.
    /// The stream separately retains whatever model Claude reports.
    pub fn permits_model(&self, model: &str) -> bool {
        model_available_for_version(&self.observed_version, model)
    }

    /// Start once from an already checked connection and frozen author choice.
    /// A changed executable requires another explicit connection check.
    pub fn start(
        &self,
        profile: &ClaudeLaunchProfile,
        packet: Vec<u8>,
        stop: StopSignal,
    ) -> CoreResult<ClaudeStream> {
        if packet.is_empty() || packet.len() > CLAUDE_INPUT_LIMIT_BYTES {
            return Err(unavailable(
                "ProviderInputTooLarge",
                "This request exceeds the Claude input allowance. Narrow its context and try again.",
            ));
        }
        if profile.cli_version.as_deref() != Some(self.version())
            || !self.permits_model(&profile.model)
        {
            return Err(unavailable(
                "ProviderProfileInvalid",
                "The saved Claude choice does not match this checked connection.",
            ));
        }
        // Rebuild from bounded fields instead of trusting mutable public fields
        // on a caller's profile or accepting additional command arguments.
        let checked = ClaudeLaunchProfile::for_version(
            self.version(),
            &profile.model,
            profile.effort.as_deref(),
        )
        .map_err(|_| {
            unavailable(
                "ProviderProfileInvalid",
                "The saved Claude launch settings are invalid.",
            )
        })?;
        let pinned = pin_executable(&self.install_path)?;
        if pinned.fingerprint != self.fingerprint {
            return Err(unavailable(
                "ClaudeVersionChanged",
                "The checked Claude executable changed. Check its connection again before sending a request.",
            ));
        }
        let owned = OwnedRun::new()?;
        let invocation = CliInvocation {
            executable: pinned.path.clone(),
            arguments: checked.arguments(),
            cwd: owned.cwd.clone(),
            environment: EnvironmentPolicy::Inherit,
            packet,
            limits: ChildLimits::default(),
        };
        // Safe mode retains the CLI's managed login while disabling user
        // customizations. Admin policy still applies. No auth files are copied.
        ClaudeStream::start_with_resources(invocation, stop, (pinned.file, owned)).map_err(|_| {
            unavailable(
                "WorkerUnavailable",
                "The Claude worker could not start. No new response was requested.",
            )
        })
    }
}

fn unavailable(code: &str, detail: &str) -> CoreError {
    CoreError::new(code, detail)
}

fn model_available_for_version(version: &str, model: &str) -> bool {
    if !super::claude_profile::CLAUDE_MODEL_IDS.contains(&model) {
        return false;
    }
    let numbers = version
        .split(['-', '+'])
        .next()
        .unwrap_or_default()
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>();
    let Ok(numbers) = numbers else {
        return false;
    };
    let [major, minor, patch] = numbers.as_slice() else {
        return false;
    };
    let minimum = match model {
        CLAUDE_FABLE_MODEL => (2, 1, 169),
        CLAUDE_OPUS_MODEL => (2, 1, 219),
        _ => (0, 0, 0),
    };
    let current = (*major, *minor, *patch);
    let prerelease = version.split('+').next().unwrap_or_default().contains('-');
    current > minimum || (current == minimum && !prerelease)
}

fn validate_help(help: &str) -> CoreResult<()> {
    let declares = |option: &&str| {
        help.lines().any(|line| {
            line.split_whitespace()
                .take_while(|token| token.starts_with('-'))
                .any(|token| token.trim_end_matches(',') == *option)
        })
    };
    if !REQUIRED_OPTIONS.iter().all(declares)
        || !help.contains("stream-json")
        || !help.contains("dontAsk")
    {
        return Err(unavailable(
            "ClaudeCapabilityUnavailable",
            "This Claude installation does not declare the required isolated streaming controls. Update Claude and check again.",
        ));
    }
    Ok(())
}

fn probe(executable: &Path, cwd: &Path, arguments: &[&str]) -> CoreResult<ChildOutcome> {
    let child = windows_process::spawn(CliInvocation {
        executable: executable.to_owned(),
        arguments: arguments.iter().map(OsString::from).collect(),
        cwd: cwd.to_owned(),
        environment: EnvironmentPolicy::Inherit,
        packet: Vec::new(),
        limits: ChildLimits {
            overall: Duration::from_secs(10),
            stop_grace: Duration::from_millis(200),
            max_total_output_bytes: 64 * 1024,
        },
    })
    .map_err(|_| {
        unavailable(
            "ClaudeUnavailable",
            "Claude could not start its connection check.",
        )
    })?;
    let result = child.finish_or_stop(StopSignal::new()).map_err(|_| {
        unavailable(
            "ClaudeCheckInterrupted",
            "The Claude connection check did not finish cleanly.",
        )
    })?;
    if result.termination != ChildTermination::Completed
        || result.output.truncated
        || !result.output.io_errors.is_empty()
    {
        return Err(unavailable(
            "ClaudeCheckInterrupted",
            "The Claude connection check did not finish cleanly.",
        ));
    }
    Ok(result)
}

fn successful_text(result: &ChildOutcome) -> CoreResult<String> {
    if result.output.exit_code != Some(0) {
        return Err(unavailable(
            "ClaudeCapabilityUnavailable",
            "Claude could not confirm its connection capabilities.",
        ));
    }
    String::from_utf8(result.output.stdout.clone()).map_err(|_| {
        unavailable(
            "ClaudeCheckInvalid",
            "Claude returned an unreadable connection status.",
        )
    })
}

fn validate_auth(result: &ChildOutcome) -> CoreResult<()> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AuthStatus {
        logged_in: bool,
    }
    let auth: AuthStatus = serde_json::from_slice(&result.output.stdout).map_err(|_| {
        unavailable(
            "ClaudeCheckInvalid",
            "Claude returned an unreadable sign-in status.",
        )
    })?;
    if result.output.exit_code != Some(0) || !auth.logged_in {
        return Err(unavailable(
            "ClaudeSignInRequired",
            "Sign in to Claude Code, then check its connection again. Manual writing remains available.",
        ));
    }
    Ok(())
}

struct PinnedExecutable {
    path: PathBuf,
    file: File,
    fingerprint: String,
}

fn pin_executable(path: &Path) -> CoreResult<PinnedExecutable> {
    let fail = || {
        unavailable(
            "ClaudeUnavailable",
            "Install native Claude Code and sign in, then check its connection again.",
        )
    };
    let path = path.canonicalize().map_err(|_| fail())?;
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .map_err(|_| fail())?;
    let metadata = file.metadata().map_err(|_| fail())?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 * 1024 {
        return Err(fail());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| fail())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(PinnedExecutable {
        path,
        file,
        fingerprint: hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    })
}

struct OwnedRun {
    cwd: PathBuf,
}
impl OwnedRun {
    fn new() -> CoreResult<Self> {
        let cwd = std::env::temp_dir().join(format!("webnovel-claude-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&cwd).map_err(|_| {
            unavailable(
                "ProviderWorkspaceUnavailable",
                "The temporary Claude workspace could not be created.",
            )
        })?;
        Ok(Self { cwd })
    }
}
impl Drop for OwnedRun {
    fn drop(&mut self) {
        // Only the owned empty directory is removed. Unexpected provider files
        // are retained; never recurse over them or an author directory.
        let _ = std::fs::remove_dir(&self.cwd);
    }
}

#[cfg(test)]
mod tests {
    use super::super::cli::windows_process::ChildOutput;
    use super::*;

    #[test]
    fn help_requires_declared_options_not_just_mentions_in_prose() {
        let help = REQUIRED_OPTIONS
            .iter()
            .map(|flag| format!("  {flag} <value>\n"))
            .collect::<String>()
            + "stream-json dontAsk";
        assert!(validate_help(&help).is_ok());
        assert!(
            validate_help(&help.replace(
                "  --safe-mode <value>",
                "The description mentions --safe-mode"
            ))
            .is_err()
        );
        assert!(validate_help(&help.replace("--tools", "--tools-extra")).is_err());
    }

    fn auth(json: &[u8], exit: u32) -> ChildOutcome {
        ChildOutcome {
            termination: ChildTermination::Completed,
            output: ChildOutput {
                exit_code: Some(exit),
                stdin_bytes_written: 0,
                stdout: json.to_vec(),
                stderr: b"PRIVATE_ACCOUNT_METADATA".to_vec(),
                truncated: false,
                io_errors: vec![],
            },
        }
    }

    #[test]
    fn sign_in_status_keeps_account_metadata_private_and_requires_a_boolean() {
        assert!(
            validate_auth(&auth(
                br#"{"loggedIn":true,"email":"synthetic@example.invalid"}"#,
                0
            ))
            .is_ok()
        );
        for result in [
            auth(br#"{"loggedIn":false}"#, 1),
            auth(br#"{"loggedIn":true}"#, 1),
            auth(br#"{"loggedIn":"true"}"#, 0),
            auth(b"PRIVATE_ACCOUNT_METADATA", 0),
        ] {
            let error = validate_auth(&result).unwrap_err();
            assert!(!error.to_string().contains("PRIVATE_ACCOUNT_METADATA"));
        }
    }

    #[test]
    fn reference_models_follow_version_floors_without_pinning_future_releases() {
        assert!(!model_available_for_version("2.1.168", CLAUDE_FABLE_MODEL));
        assert!(model_available_for_version("2.1.169", CLAUDE_FABLE_MODEL));
        assert!(!model_available_for_version("2.1.218", CLAUDE_OPUS_MODEL));
        assert!(!model_available_for_version(
            "2.1.219-beta",
            CLAUDE_OPUS_MODEL
        ));
        assert!(model_available_for_version(
            "2.1.219+build-1",
            CLAUDE_OPUS_MODEL
        ));
        assert!(model_available_for_version("2.1.220", CLAUDE_OPUS_MODEL));
        assert!(model_available_for_version("3.0.0", CLAUDE_OPUS_MODEL));
        assert!(!model_available_for_version("2.1.220", "unlisted-model"));
    }

    #[test]
    fn invalid_input_and_mismatched_profiles_fail_before_process_creation() {
        let connection = ClaudeConnection {
            install_path: PathBuf::from("Z:/never-launched/claude.exe"),
            observed_version: "2.1.220".into(),
            fingerprint: "a".repeat(64),
        };
        let profile =
            ClaudeLaunchProfile::for_version("2.1.220", CLAUDE_OPUS_MODEL, Some("high")).unwrap();
        assert!(
            matches!(connection.start(&profile, Vec::new(), StopSignal::new()), Err(error) if error.code == "ProviderInputTooLarge")
        );
        let profile =
            ClaudeLaunchProfile::for_version("2.1.219", CLAUDE_OPUS_MODEL, Some("high")).unwrap();
        assert!(
            matches!(connection.start(&profile, b"synthetic".to_vec(), StopSignal::new()), Err(error) if error.code == "ProviderProfileInvalid")
        );
    }
}
