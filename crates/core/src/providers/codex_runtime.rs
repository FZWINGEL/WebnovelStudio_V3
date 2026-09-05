//! Exact installed Windows Codex candidate and isolated invocation material.
//!
//! Connection checks run only version/login-status commands. They never read
//! authentication files or send story text. Raw diagnostics do not escape.
use super::cli::windows_process::{
    self, ChildLimits, ChildTermination, CliInvocation, EnvironmentPolicy, StopSignal,
};
use super::codex_profile::CodexLaunchProfile;
use super::codex_runner::CodexStream;
use crate::projects::{CoreError, CoreResult};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::Read,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

// Qualification is specific to this Windows executable, not every program
// that happens to print the same version. Updates require requalification.
pub const QUALIFIED_EXECUTABLE_SHA256: &str =
    "bc15d59a3062bf165181a30007c3d0f5b1ee0ca4855e33d9b486ad59c984a31b";
pub const MAX_CODEX_STDIN_BYTES: usize = 24 * 1024;

#[derive(Debug, Clone)]
pub struct CodexConnection {
    executable: PathBuf,
}

fn unavailable(code: &str, detail: &str) -> CoreError {
    CoreError::new(code, detail)
}

impl CodexConnection {
    /// Explicit discovery, bounded to Codex's installed native-bin directory.
    /// Opening the picker never calls this method.
    pub fn check_installed() -> CoreResult<Self> {
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
        let candidates = std::fs::read_dir(base).map_err(|_| {
            unavailable(
                "CodexUnavailable",
                "Install and sign in to Codex on this computer, then check the connection again.",
            )
        })?;
        let mut paths = candidates
            .take(64)
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path().join("codex.exe"))
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            if let Ok(file) = pin_executable(&path) {
                // Pin the checked binary against modification while probing.
                let run = OwnedRun::new()?;
                let version = probe(&path, &run.cwd, &["--version"])?;
                if version.trim() != "codex-cli 0.153.3" {
                    return Err(unavailable(
                        "CodexVersionChanged",
                        "This Codex version has not been checked with WebnovelStudio.",
                    ));
                }
                let login = probe(&path, &run.cwd, &["login", "status"])?;
                if !login.lines().any(|line| line.starts_with("Logged in")) {
                    return Err(unavailable(
                        "CodexSignInRequired",
                        "Sign in to Codex, then check the connection again.",
                    ));
                }
                drop(file);
                return Ok(Self { executable: path });
            }
        }
        Err(unavailable(
            "CodexVersionUnavailable",
            "The supported Codex version is not installed. Manual writing and the local test model remain available.",
        ))
    }

    /// One explicit invocation. No automatic retry, model substitution, shell,
    /// author working directory, transcript file, or packet command argument.
    pub fn start(&self, packet: Vec<u8>, stop: StopSignal) -> CoreResult<CodexStream> {
        if packet.is_empty() || packet.len() > MAX_CODEX_STDIN_BYTES {
            return Err(unavailable(
                "ProviderInputTooLarge",
                "This request exceeds the current Codex input allowance. Narrow the requested context and try again.",
            ));
        }
        let binary = pin_executable(&self.executable)?;
        let owned = OwnedRun::new()?;
        let profile = CodexLaunchProfile::for_version("codex-cli 0.153.3", &owned.catalog)
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
        CodexStream::start_with_resources(invocation, stop, (binary, owned)).map_err(|_| {
            unavailable(
                "WorkerUnavailable",
                "The Codex worker could not start. No new response was requested.",
            )
        })
    }
}

fn pin_executable(path: &Path) -> CoreResult<File> {
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
    if fingerprint != QUALIFIED_EXECUTABLE_SHA256 {
        return Err(unavailable(
            "CodexVersionChanged",
            "The Codex installation changed. This version needs a compatibility check.",
        ));
    }
    Ok(file)
}

fn probe(executable: &Path, cwd: &Path, arguments: &[&str]) -> CoreResult<String> {
    let process = windows_process::spawn(CliInvocation {
        executable: executable.into(),
        cwd: cwd.into(),
        arguments: arguments.iter().map(OsString::from).collect(),
        environment: EnvironmentPolicy::Inherit,
        packet: Vec::new(),
        limits: ChildLimits {
            overall: Duration::from_secs(10),
            stop_grace: Duration::from_millis(200),
            max_total_output_bytes: 16 * 1024,
        },
    })
    .map_err(|_| {
        unavailable(
            "CodexUnavailable",
            "Codex could not start its connection check.",
        )
    })?;
    let result = process.finish_or_stop(StopSignal::new()).map_err(|_| {
        unavailable(
            "CodexCheckInterrupted",
            "The Codex connection check did not finish cleanly.",
        )
    })?;
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
    // `login status` prints to stderr in some builds. Return only in-memory
    // text to the fixed parser above; never expose it through the public DTO.
    let mut output = result.output.stdout;
    output.push(b'\n');
    output.extend(result.output.stderr);
    String::from_utf8(output).map_err(|_| {
        unavailable(
            "CodexCheckInvalid",
            "Codex returned an unreadable connection status.",
        )
    })
}

struct OwnedRun {
    root: PathBuf,
    cwd: PathBuf,
    catalog: PathBuf,
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
        let _ = std::fs::remove_dir(&self.cwd);
        let _ = std::fs::remove_dir(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_executable_and_oversized_input_are_refused_before_spawn() {
        let run = OwnedRun::new().unwrap();
        let fake = run.root.join("fake.exe");
        std::fs::write(&fake, b"not codex").unwrap();
        assert_eq!(
            pin_executable(&fake).unwrap_err().code,
            "CodexVersionChanged"
        );
        let connection = CodexConnection {
            executable: fake.clone(),
        };
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
