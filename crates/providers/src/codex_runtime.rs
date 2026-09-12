//! Capability-checked installed Windows Codex candidate and isolated
//! invocation material.
//!
//! Connection checks run bounded version, capability-preflight, and
//! login-status commands. They never read authentication files or send story
//! text. Raw diagnostics do not escape.
use super::cli::windows_process::{
    self, ChildIoError, ChildLimits, ChildOutcome, ChildTermination, CliInvocation,
    EnvironmentPolicy, StopSignal,
};
use super::codex_catalog::CodexCatalog;
use super::codex_discovery;
use super::codex_profile::CodexLaunchProfile;
use super::codex_runner::CodexStream;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::Read,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use wns_kernel::{CoreError, CoreResult};

pub const MAX_CODEX_STDIN_BYTES: usize = 24 * 1024;
const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(15);
const PREFLIGHT_SENTINEL: &str = "codex_qualification_sentinel=true";
// `exec -` rejects an empty stdin stream as "No prompt provided" before it
// reaches output-schema validation. This short synthetic prompt is still
// rejected by the preflight boundary before any model/provider request.
const PREFLIGHT_PROMPT: &[u8] = b"WebnovelStudio capability preflight.";
const MISSING_SCHEMA_MARKER: &str = "Failed to read output schema file";
const UNKNOWN_CONFIG_MARKER: &str = "unknown configuration field";
// The sentinel intentionally exits while the stdin writer is still open. The
// Windows pipe reports one of these two close conditions when that happens.
const WINDOWS_ERROR_BROKEN_PIPE: i32 = 109;
const WINDOWS_ERROR_NO_DATA: i32 = 232;

#[derive(Debug, Clone)]
pub struct CodexConnection {
    executable: PathBuf,
    observed_version: String,
    fingerprint: String,
    catalog: CodexCatalog,
}

fn unavailable(code: &str, detail: &str) -> CoreError {
    CoreError::new(code, detail)
}

impl CodexConnection {
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }
    pub fn version(&self) -> &str {
        &self.observed_version
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn catalog(&self) -> &CodexCatalog {
        &self.catalog
    }

    /// Explicit discovery, bounded to Codex's installed native-bin directory.
    /// Opening the picker never calls this method.
    pub fn check_installed() -> CoreResult<Self> {
        let path = newest_installed_executable()?;

        // Pin the newest candidate while the version, profile and capability
        // checks run. A later start reopens the file and compares this session
        // fingerprint; there is no global executable allowlist.
        let pinned = pin_executable(&path)?;
        let run = OwnedRun::new()?;
        let version_output = probe(&path, &run.cwd, &["--version"])?;
        let profile = CodexLaunchProfile::for_maintenance_version(&version_output, &run.catalog)
            .map_err(|_| {
                unavailable(
                    "CodexCapabilityUnavailable",
                    "This Codex installation returned an invalid version and cannot be checked safely.",
                )
            })?;
        std::fs::write(&run.catalog, &profile.catalog_json).map_err(|_| {
            unavailable(
                "ProviderWorkspaceUnavailable",
                "The temporary Codex workspace could not be prepared.",
            )
        })?;
        check_profile_compatibility(&path, &run, &profile)?;
        let login = probe(&path, &run.cwd, &["login", "status"])?;
        if !login.lines().any(|line| line.starts_with("Logged in")) {
            return Err(unavailable(
                "CodexSignInRequired",
                "Sign in to Codex, then check the connection again.",
            ));
        }
        let discovered_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| unavailable("CodexDiscoveryFailed", "Codex discovery time was invalid."))?
            .as_millis()
            .to_string();
        let catalog = codex_discovery::discover(
            &path,
            &run.cwd,
            &profile.executable_version,
            &pinned.fingerprint,
            &discovered_at,
        )?;
        drop(pinned.file);
        Ok(Self {
            executable: path,
            observed_version: profile.executable_version,
            fingerprint: pinned.fingerprint,
            catalog,
        })
    }

    /// Check whether the newest installed Codex candidate still resolves to
    /// the executable that was qualified for this connection. This is a
    /// bounded path check only: it does not re-run version, login, or model
    /// discovery and never reads author credentials.
    pub fn is_current_installation(&self) -> CoreResult<bool> {
        let newest = newest_installed_executable()?;
        let checked = std::fs::canonicalize(&self.executable).map_err(|_| {
            unavailable(
                "CodexUnavailable",
                "The checked Codex installation is no longer available.",
            )
        })?;
        let newest = std::fs::canonicalize(newest).map_err(|_| {
            unavailable(
                "CodexUnavailable",
                "The installed Codex candidate could not be inspected.",
            )
        })?;
        Ok(checked == newest)
    }

    /// One explicit invocation. No automatic retry, model substitution, shell,
    /// author working directory, transcript file, or packet command argument.
    pub fn start(&self, packet: Vec<u8>, stop: StopSignal) -> CoreResult<CodexStream> {
        self.start_with_choice(None, false, packet, stop)
    }

    pub fn start_bound(
        &self,
        binding: &crate::vocabulary::ProviderBinding,
        packet: Vec<u8>,
        stop: StopSignal,
    ) -> CoreResult<CodexStream> {
        use crate::vocabulary::ProviderBinding;
        if binding
            == &ProviderBinding::codex_maintenance_runtime(self.version(), self.fingerprint())
        {
            return self.start(packet, stop);
        }
        if binding == &ProviderBinding::codex_luna_runtime(self.version(), self.fingerprint()) {
            return self.start_with_choice(None, true, packet, stop);
        }
        if binding.validate().is_err()
            || binding.profile_version != super::codex_profile::CODEX_AUTHOR_PROFILE_VERSION
            || !binding.runtime.as_ref().is_some_and(|identity| {
                identity.cli_version == self.observed_version
                    && identity.executable_sha256 == self.fingerprint
            })
        {
            return Err(unavailable(
                "ProviderProfileInvalid",
                "The saved Codex settings do not match this checked connection.",
            ));
        }
        let model = self.catalog().model(&binding.model_id).ok_or_else(|| {
            unavailable(
                "ProviderProfileInvalid",
                "The saved model is absent from this checked Codex catalog.",
            )
        })?;
        if binding
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.catalog_sha256.as_deref())
            != Some(model.fingerprint()?.as_str())
            || (binding.service_tier.is_none() && model.default_service_tier.is_some())
        {
            return Err(unavailable(
                "ProviderProfileInvalid",
                "The saved Codex capabilities changed. This request will not be sent again.",
            ));
        }
        self.start_with_choice(
            Some(super::preferences::ModelSelection {
                provider_id: binding.provider_id.clone(),
                model_id: binding.model_id.clone(),
                reasoning: binding.reasoning.clone(),
                service_tier: binding.service_tier.clone(),
            }),
            false,
            packet,
            stop,
        )
    }

    fn start_with_choice(
        &self,
        choice: Option<super::preferences::ModelSelection>,
        legacy_luna_maintenance: bool,
        packet: Vec<u8>,
        stop: StopSignal,
    ) -> CoreResult<CodexStream> {
        if packet.is_empty() || packet.len() > MAX_CODEX_STDIN_BYTES {
            return Err(unavailable(
                "ProviderInputTooLarge",
                "This request exceeds the current Codex input allowance. Narrow the requested context and try again.",
            ));
        }
        let pinned = pin_executable(&self.executable)?;
        if pinned.fingerprint != self.fingerprint {
            return Err(unavailable(
                "CodexVersionChanged",
                "The checked Codex executable changed. Check its connection again before starting a request.",
            ));
        }
        let owned = OwnedRun::new()?;
        let maintenance_reasoning = if legacy_luna_maintenance {
            super::codex_profile::CODEX_REASONING_EFFORT
        } else {
            super::codex_profile::CODEX_MAINTENANCE_REASONING_EFFORT
        };
        let maintenance = super::preferences::ModelSelection {
            provider_id: "codex".into(),
            model_id: if legacy_luna_maintenance {
                super::codex_profile::CODEX_LUNA_MODEL.into()
            } else {
                super::codex_profile::CODEX_MAINTENANCE_MODEL.into()
            },
            reasoning: Some(maintenance_reasoning.into()),
            service_tier: Some(super::codex_profile::CODEX_PRIORITY_SERVICE_TIER.into()),
        };
        if !self
            .catalog()
            .supports(choice.as_ref().unwrap_or(&maintenance))
        {
            return Err(unavailable(
                "ProviderUnavailable",
                "The requested Codex model settings are not available in this checked catalog.",
            ));
        }
        let version = format!("codex-cli {}", self.observed_version);
        let profile = if let Some(choice) = &choice {
            let model = self
                .catalog()
                .models
                .iter()
                .find(|model| model.model_id == choice.model_id)
                .ok_or_else(|| {
                    unavailable("ProviderUnavailable", "The selected model is unavailable.")
                })?;
            CodexLaunchProfile::for_selection(&version, &owned.catalog, model, choice)
        } else {
            if legacy_luna_maintenance {
                CodexLaunchProfile::for_version(&version, &owned.catalog)
            } else {
                CodexLaunchProfile::for_maintenance_version(&version, &owned.catalog)
            }
        }
        .map_err(|_| {
            unavailable(
                "ProviderProfileInvalid",
                "The Codex launch settings could not be prepared.",
            )
        })?;
        std::fs::write(&owned.catalog, &profile.catalog_json).map_err(|_| {
            unavailable(
                "ProviderWorkspaceUnavailable",
                "The temporary Codex workspace could not be prepared.",
            )
        })?;
        let arguments = profile.exec_arguments(&owned.cwd).map_err(|_| {
            unavailable(
                "ProviderProfileInvalid",
                "The Codex launch settings could not be prepared.",
            )
        })?;
        let invocation = CliInvocation {
            executable: self.executable.clone(),
            arguments,
            cwd: owned.cwd.clone(),
            environment: EnvironmentPolicy::Inherit,
            packet,
            limits: ChildLimits::default(),
        };
        // Resources move into the process worker and outlive navigation/Drop.
        CodexStream::start_with_resources(invocation, stop, (pinned.file, owned)).map_err(|_| {
            unavailable(
                "WorkerUnavailable",
                "The Codex worker could not start. No new response was requested.",
            )
        })
    }
}

#[derive(Debug)]
struct PinnedExecutable {
    file: File,
    fingerprint: String,
}

fn newest_installed_executable() -> CoreResult<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| {
            unavailable(
                "CodexUnavailable",
                "The supported Codex installation could not be found.",
            )
        })?
        .join("OpenAI")
        .join("Codex")
        .join("bin");
    newest_installed_executable_in(&base)
}

fn newest_installed_executable_in(base: &Path) -> CoreResult<PathBuf> {
    let candidates = std::fs::read_dir(base).map_err(|_| {
        unavailable(
            "CodexUnavailable",
            "Install and sign in to Codex on this computer, then check the connection again.",
        )
    })?;
    let paths = candidates
        .take(64)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let path = entry.path().join("codex.exe");
            let modified = path.metadata().ok()?.modified().unwrap_or(UNIX_EPOCH);
            Some((path, modified))
        })
        .collect::<Vec<_>>();
    select_newest_executable(paths).ok_or_else(|| {
        unavailable(
            "CodexVersionUnavailable",
            "No native Codex installation was found. Manual writing and the local test model remain available.",
        )
    })
}

fn select_newest_executable(mut paths: Vec<(PathBuf, SystemTime)>) -> Option<PathBuf> {
    paths.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    paths.into_iter().next().map(|(path, _)| path)
}

fn pin_executable(path: &Path) -> CoreResult<PinnedExecutable> {
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .map_err(|_| {
            unavailable(
                "CodexUnavailable",
                "The checked Codex executable is unavailable. Check its connection again.",
            )
        })?;
    let metadata = file
        .metadata()
        .map_err(|_| unavailable("CodexUnavailable", "Codex could not be checked."))?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 * 1024 {
        return Err(unavailable(
            "CodexVersionChanged",
            "The Codex installation changed. Check its connection again.",
        ));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| unavailable("CodexUnavailable", "Codex could not be checked."))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let fingerprint = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(PinnedExecutable { file, fingerprint })
}

fn probe(executable: &Path, cwd: &Path, arguments: &[&str]) -> CoreResult<String> {
    let arguments = arguments.iter().map(OsString::from).collect::<Vec<_>>();
    let result = probe_outcome(
        executable,
        cwd,
        &arguments,
        Vec::new(),
        ChildLimits {
            overall: Duration::from_secs(10),
            stop_grace: Duration::from_millis(200),
            max_total_output_bytes: 16 * 1024,
        },
    )?;
    if result.termination != ChildTermination::Completed
        || result.output.exit_code != Some(0)
        || result.output.truncated
        || !result.output.io_errors.is_empty()
    {
        return Err(unavailable(
            "CodexSignInRequired",
            "Codex could not confirm a signed-in connection. Open Codex and sign in, then check again.",
        ));
    }
    output_text(&result.output)
}

fn probe_outcome(
    executable: &Path,
    cwd: &Path,
    arguments: &[OsString],
    packet: Vec<u8>,
    limits: ChildLimits,
) -> CoreResult<ChildOutcome> {
    let process = windows_process::spawn(CliInvocation {
        executable: executable.into(),
        cwd: cwd.to_owned(),
        arguments: arguments.to_vec(),
        environment: EnvironmentPolicy::Inherit,
        packet,
        limits,
    })
    .map_err(|_| {
        unavailable(
            "CodexUnavailable",
            "Codex could not start its connection check.",
        )
    })?;
    process.finish_or_stop(StopSignal::new()).map_err(|_| {
        unavailable(
            "CodexCheckInterrupted",
            "The Codex connection check did not finish cleanly.",
        )
    })
}

fn output_text(output: &super::cli::windows_process::ChildOutput) -> CoreResult<String> {
    // `login status` prints to stderr in some builds. Return only in-memory
    // text to the fixed parser above; never expose it through the public DTO.
    let mut bytes = output.stdout.clone();
    bytes.push(b'\n');
    bytes.extend(&output.stderr);
    String::from_utf8(bytes).map_err(|_| {
        unavailable(
            "CodexCheckInvalid",
            "Codex returned an unreadable connection status.",
        )
    })
}

fn check_profile_compatibility(
    executable: &Path,
    run: &OwnedRun,
    profile: &CodexLaunchProfile,
) -> CoreResult<()> {
    if run.missing_schema.exists() {
        return Err(unavailable(
            "CodexCapabilityUnavailable",
            "The owned preflight output-schema path unexpectedly exists.",
        ));
    }
    let sentinel = profile
        .preflight_arguments(&run.cwd, &run.missing_schema, Some(PREFLIGHT_SENTINEL))
        .map_err(|_| {
            unavailable(
                "CodexCapabilityUnavailable",
                "Codex preflight arguments are invalid.",
            )
        })?;
    let sentinel_result = probe_outcome(
        executable,
        &run.cwd,
        &sentinel,
        PREFLIGHT_PROMPT.to_vec(),
        ChildLimits {
            overall: PREFLIGHT_TIMEOUT,
            stop_grace: Duration::from_millis(200),
            max_total_output_bytes: 16 * 1024,
        },
    )?;
    validate_preflight_result(
        &sentinel_result,
        UNKNOWN_CONFIG_MARKER,
        Some("codex_qualification_sentinel"),
        true,
    )?;
    ensure_preflight_schema_absent(run)?;

    let valid = profile
        .preflight_arguments(&run.cwd, &run.missing_schema, None)
        .map_err(|_| {
            unavailable(
                "CodexCapabilityUnavailable",
                "Codex preflight arguments are invalid.",
            )
        })?;
    let valid_result = probe_outcome(
        executable,
        &run.cwd,
        &valid,
        PREFLIGHT_PROMPT.to_vec(),
        ChildLimits {
            overall: PREFLIGHT_TIMEOUT,
            stop_grace: Duration::from_millis(200),
            max_total_output_bytes: 16 * 1024,
        },
    )?;
    let schema_path = run.missing_schema.to_string_lossy().replace('\\', "/");
    validate_preflight_result(
        &valid_result,
        MISSING_SCHEMA_MARKER,
        Some(&schema_path),
        false,
    )?;
    ensure_preflight_schema_absent(run)
}

fn validate_preflight_result(
    result: &ChildOutcome,
    marker: &str,
    detail: Option<&str>,
    allow_expected_stdin_close: bool,
) -> CoreResult<()> {
    let expected_stdin_close = allow_expected_stdin_close
        && !result.output.io_errors.is_empty()
        && result.output.stdin_bytes_written == 0
        && result.output.io_errors.iter().all(|error| {
            matches!(
                error,
                ChildIoError::WriteStdin(code)
                    if *code == WINDOWS_ERROR_BROKEN_PIPE || *code == WINDOWS_ERROR_NO_DATA
            )
        });
    if result.termination != ChildTermination::Completed
        || result.output.truncated
        || (!result.output.io_errors.is_empty() && !expected_stdin_close)
        || !result.output.exit_code.is_some_and(|code| code != 0)
    {
        return Err(unavailable(
            "CodexCapabilityUnavailable",
            "Codex could not complete its connection check. Check the connection again after restarting Codex.",
        ));
    }
    let text = output_text(&result.output)?;
    if !text.contains(marker)
        || detail.is_some_and(|detail| !text.contains(detail))
        || text.lines().any(|line| line.trim_start().starts_with('{'))
    {
        return Err(unavailable(
            "CodexCapabilityUnavailable",
            "Codex returned an unexpected connection-check response. Check the connection again after restarting Codex.",
        ));
    }
    Ok(())
}

fn ensure_preflight_schema_absent(run: &OwnedRun) -> CoreResult<()> {
    if run.missing_schema.exists() {
        return Err(unavailable(
            "CodexCapabilityUnavailable",
            "Codex created the intentionally missing output schema during preflight.",
        ));
    }
    Ok(())
}

struct OwnedRun {
    root: PathBuf,
    cwd: PathBuf,
    catalog: PathBuf,
    missing_schema: PathBuf,
}
impl OwnedRun {
    fn new() -> CoreResult<Self> {
        let root = std::env::temp_dir().join(format!("webnovel-codex-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).map_err(|_| {
            unavailable(
                "ProviderWorkspaceUnavailable",
                "The temporary Codex workspace could not be created.",
            )
        })?;
        let owned = Self {
            cwd: root.join("work"),
            catalog: root.join("catalog.json"),
            missing_schema: root.join("missing-output-schema.json"),
            root,
        };
        std::fs::create_dir(&owned.cwd).map_err(|_| {
            unavailable(
                "ProviderWorkspaceUnavailable",
                "The temporary Codex workspace could not be created.",
            )
        })?;
        Ok(owned)
    }
}
impl Drop for OwnedRun {
    fn drop(&mut self) {
        // Remove only our known artifacts and empty directories. Never recurse
        // over unexpected provider files or any author directory.
        let _ = std::fs::remove_file(&self.catalog);
        let _ = std::fs::remove_file(&self.missing_schema);
        let _ = std::fs::remove_dir(&self.cwd);
        let _ = std::fs::remove_dir(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::super::cli::windows_process::{ChildIoError, ChildOutput};
    use super::*;

    #[test]
    fn changed_catalog_metadata_fences_a_saved_author_binding_before_process_creation() {
        use super::super::codex_catalog::CodexCatalogModel;
        use crate::vocabulary::ProviderBinding;
        let model = CodexCatalogModel {
            model_id: "synthetic-model".into(),
            label: "Synthetic model".into(),
            reasoning_levels: vec!["low".into(), "high".into()],
            default_reasoning: Some("low".into()),
            service_tiers: vec![],
            default_service_tier: None,
        };
        let binding = ProviderBinding::codex_author_runtime(
            "synthetic-model",
            "low",
            None,
            "9.1",
            &"a".repeat(64),
            &model.fingerprint().unwrap(),
        );
        let old_bytes = serde_json::to_vec(&binding).unwrap();
        let mut changed = model;
        changed.default_reasoning = Some("high".into());
        let connection = CodexConnection {
            executable: PathBuf::from("intentionally-missing-codex.exe"),
            observed_version: "9.1".into(),
            fingerprint: "a".repeat(64),
            catalog: CodexCatalog {
                cli_version: "9.1".into(),
                executable_sha256: "a".repeat(64),
                discovered_at: "0".into(),
                models: vec![changed],
            },
        };
        let error = connection
            .start_bound(&binding, b"synthetic story".to_vec(), StopSignal::new())
            .err()
            .unwrap();
        assert_eq!(error.code, "ProviderProfileInvalid");
        assert!(
            binding.validate().is_ok(),
            "history stays inspectable without the old catalog"
        );
        assert_eq!(serde_json::to_vec(&binding).unwrap(), old_bytes);
    }

    #[test]
    fn arbitrary_executables_are_fingerprinted_without_a_global_allowlist() {
        let run = OwnedRun::new().unwrap();
        let fake = run.root.join("fake.exe");
        std::fs::write(&fake, b"not codex").unwrap();
        let pinned = pin_executable(&fake).expect("any bounded executable can be fingerprinted");
        assert_eq!(pinned.fingerprint.len(), 64);
        drop(pinned);
        let connection = CodexConnection {
            executable: fake.clone(),
            observed_version: "0.0.0".into(),
            fingerprint: "a different session fingerprint".into(),
            catalog: CodexCatalog {
                cli_version: "codex-cli 0.0.0".into(),
                executable_sha256: "a".repeat(64),
                discovered_at: "1700000000000".into(),
                models: Vec::new(),
            },
        };
        assert_eq!(
            connection
                .start(vec![b'x'; 1], StopSignal::new())
                .err()
                .unwrap()
                .code,
            "CodexVersionChanged"
        );
        assert_eq!(
            connection
                .start(vec![b'x'; MAX_CODEX_STDIN_BYTES + 1], StopSignal::new())
                .err()
                .unwrap()
                .code,
            "ProviderInputTooLarge"
        );
        std::fs::remove_file(fake).unwrap();
    }

    #[test]
    fn newest_candidate_selection_is_newest_then_path_ordered() {
        let timestamp = UNIX_EPOCH + Duration::from_secs(10);
        let selected = select_newest_executable(vec![
            (PathBuf::from("z\\codex.exe"), timestamp),
            (PathBuf::from("a\\codex.exe"), timestamp),
            (
                PathBuf::from("newer\\codex.exe"),
                timestamp + Duration::from_secs(1),
            ),
        ])
        .expect("synthetic candidate");
        assert_eq!(selected, PathBuf::from("newer\\codex.exe"));

        let tie = select_newest_executable(vec![
            (PathBuf::from("z\\codex.exe"), timestamp),
            (PathBuf::from("a\\codex.exe"), timestamp),
        ])
        .expect("synthetic tie");
        assert_eq!(tie, PathBuf::from("a\\codex.exe"));
    }

    fn preflight_outcome(text: &str, exit_code: Option<u32>) -> ChildOutcome {
        ChildOutcome {
            termination: ChildTermination::Completed,
            output: ChildOutput {
                exit_code,
                stdin_bytes_written: 0,
                stdout: text.as_bytes().to_vec(),
                stderr: Vec::new(),
                truncated: false,
                io_errors: Vec::new(),
            },
        }
    }

    #[test]
    fn capability_preflight_requires_plain_expected_failure_without_events() {
        assert!(
            validate_preflight_result(
                &preflight_outcome(
                    "error: unknown configuration field 'codex_qualification_sentinel'",
                    Some(1),
                ),
                UNKNOWN_CONFIG_MARKER,
                Some("codex_qualification_sentinel"),
                false,
            )
            .is_ok()
        );
        assert!(
            validate_preflight_result(
                &preflight_outcome("Failed to read output schema file", Some(1)),
                MISSING_SCHEMA_MARKER,
                Some("schema.json"),
                false,
            )
            .is_err()
        );
        assert!(
            validate_preflight_result(
                &preflight_outcome("Failed to read output schema file: schema.json", Some(1)),
                MISSING_SCHEMA_MARKER,
                Some("schema.json"),
                false,
            )
            .is_ok()
        );
        assert!(
            validate_preflight_result(
                &preflight_outcome(
                    "{\"type\":\"thread.started\"}\nFailed to read output schema file",
                    Some(1),
                ),
                MISSING_SCHEMA_MARKER,
                Some("schema.json"),
                false,
            )
            .is_err()
        );
        assert!(
            validate_preflight_result(
                &preflight_outcome("Failed to read output schema file", Some(0)),
                MISSING_SCHEMA_MARKER,
                Some("schema.json"),
                false,
            )
            .is_err()
        );
        assert!(
            validate_preflight_result(
                &preflight_outcome("unknown configuration field", None),
                UNKNOWN_CONFIG_MARKER,
                Some("configuration"),
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn sentinel_allows_only_expected_early_stdin_close() {
        let mut expected = preflight_outcome(
            "Error loading config.toml: unknown configuration field `codex_qualification_sentinel`",
            Some(1),
        );
        expected.output.io_errors = vec![ChildIoError::WriteStdin(WINDOWS_ERROR_BROKEN_PIPE)];
        assert!(
            validate_preflight_result(
                &expected,
                UNKNOWN_CONFIG_MARKER,
                Some("codex_qualification_sentinel"),
                true,
            )
            .is_ok()
        );
        assert!(
            validate_preflight_result(
                &expected,
                UNKNOWN_CONFIG_MARKER,
                Some("codex_qualification_sentinel"),
                false,
            )
            .is_err()
        );

        expected.output.io_errors = vec![ChildIoError::WriteStdin(5)];
        assert!(
            validate_preflight_result(
                &expected,
                UNKNOWN_CONFIG_MARKER,
                Some("codex_qualification_sentinel"),
                true,
            )
            .is_err()
        );
        expected.output.io_errors =
            vec![ChildIoError::ReadStderr(WINDOWS_ERROR_BROKEN_PIPE as u32)];
        assert!(
            validate_preflight_result(
                &expected,
                UNKNOWN_CONFIG_MARKER,
                Some("codex_qualification_sentinel"),
                true,
            )
            .is_err()
        );
        expected.output.stdin_bytes_written = 1;
        expected.output.io_errors = vec![ChildIoError::WriteStdin(WINDOWS_ERROR_BROKEN_PIPE)];
        assert!(
            validate_preflight_result(
                &expected,
                UNKNOWN_CONFIG_MARKER,
                Some("codex_qualification_sentinel"),
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn unexpected_schema_content_is_reported_without_eager_deletion() {
        let run = OwnedRun::new().unwrap();
        std::fs::write(&run.missing_schema, b"foreign").unwrap();
        assert!(ensure_preflight_schema_absent(&run).is_err());
        assert!(run.missing_schema.exists());
    }

    #[test]
    fn owned_run_removes_only_known_files_and_empty_directories() {
        let run = OwnedRun::new().unwrap();
        let root = run.root.clone();
        std::fs::write(&run.catalog, b"{}").unwrap();
        drop(run);
        assert!(!root.exists());
        let run = OwnedRun::new().unwrap();
        let root = run.root.clone();
        let unexpected = run.cwd.join("unexpected");
        std::fs::write(&unexpected, b"keep").unwrap();
        drop(run);
        assert!(unexpected.exists());
        std::fs::remove_file(unexpected).unwrap();
        std::fs::remove_dir(root.join("work")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
