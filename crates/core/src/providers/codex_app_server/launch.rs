//! Owned app-server launch material.
//!
//! This module prepares an app-server process without borrowing the author's
//! working directory or Codex home.  It checks the installed executable's
//! identity, writes one immutable restrictive multi-model catalog into an
//! application-owned temporary directory, and supplies the external auth
//! handoff separately from the process environment.

#![cfg(windows)]

use super::AppServerRuntimeIdentity;
use super::auth::{ExternalAuth, read_auth_file};
use crate::projects::{CoreError, CoreResult};
use crate::providers::cli::windows_process::{
    ChildLimits, CliInvocation, EnvironmentPolicy, MAX_OVERALL,
};
use crate::providers::codex_catalog::CodexCatalog;
use crate::providers::codex_profile::CodexLaunchProfile;
use crate::providers::preferences::ModelSelection;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const MAX_CATALOG_MODELS: usize = 256;
const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;
const MAX_CLEANUP_ENTRIES: usize = 128;
const CODEX_HOME_ENV: &str = "CODEX_HOME";

/// Compatibility alias for callers that want the launch identity type by its
/// launch-module name.  The binding-owned identity remains defined centrally
/// with the app-server transport contracts.
pub type AppServerLaunchIdentity = AppServerRuntimeIdentity;

/// Resources that must remain alive until the app-server exits.  The pinned
/// executable handle prevents replacement during a run; the temporary root
/// owns only generated files and directories.
pub struct AppServerLaunchResources {
    owned: OwnedLaunch,
    _executable: File,
}

impl Drop for OwnedLaunch {
    fn drop(&mut self) {
        cleanup_owned_root(&self.root);
    }
}

/// Fully prepared native launch.  The auth payload is private and must be
/// moved into the app-server worker; callers cannot serialize the bundle.
pub struct AppServerLaunch {
    pub invocation: CliInvocation,
    pub resources: AppServerLaunchResources,
    pub auth: ExternalAuth,
    pub identity: AppServerLaunchIdentity,
    pub model: String,
    pub reasoning_effort: String,
    pub service_tier: String,
}

impl AppServerLaunch {
    /// Prepare a fresh app-owned launch from a checked catalog and selected
    /// model.  `expected_executable_sha256` is the fingerprint captured by
    /// discovery; the file is re-opened and checked before any process starts.
    pub fn prepare(
        executable: &Path,
        observed_version: &str,
        expected_executable_sha256: &str,
        catalog: &CodexCatalog,
        selection: &ModelSelection,
        auth: ExternalAuth,
    ) -> CoreResult<Self> {
        catalog.validate()?;
        validate_version(observed_version)?;
        validate_fingerprint(expected_executable_sha256)?;
        if catalog.cli_version != observed_version
            || catalog.executable_sha256 != expected_executable_sha256
        {
            return Err(unavailable(
                "ProviderProfileInvalid",
                "The checked Codex catalog does not belong to this executable identity.",
            ));
        }
        if !catalog.supports(selection) {
            return Err(unavailable(
                "ProviderProfileInvalid",
                "The requested Codex model settings are not available in this checked catalog.",
            ));
        }

        let pinned = pin_executable(executable, expected_executable_sha256)?;
        let owned = OwnedLaunch::new()?;
        let version_line = format!("codex-cli {observed_version}");
        let selected_profile = CodexLaunchProfile::for_selection(
            &version_line,
            &owned.catalog,
            catalog.model(&selection.model_id).ok_or_else(|| {
                unavailable(
                    "ProviderProfileInvalid",
                    "The selected Codex model is unavailable.",
                )
            })?,
            selection,
        )
        .map_err(|_| {
            unavailable(
                "ProviderProfileInvalid",
                "The Codex launch settings are invalid.",
            )
        })?;

        let catalog_json = restrictive_multi_model_catalog(&version_line, &owned.catalog, catalog)?;
        if catalog_json.len() > MAX_CATALOG_BYTES {
            return Err(unavailable(
                "ProviderProfileInvalid",
                "The checked Codex catalog is too large for an app-server launch.",
            ));
        }
        std::fs::write(&owned.catalog, catalog_json.as_bytes()).map_err(|_| {
            unavailable(
                "ProviderWorkspaceUnavailable",
                "The app-owned Codex catalog could not be prepared.",
            )
        })?;

        let mut arguments = vec![
            OsString::from("app-server"),
            OsString::from("--strict-config"),
            OsString::from("--stdio"),
        ];
        let mut config_overrides = selected_profile.config_overrides.clone();
        config_overrides.push("cli_auth_credentials_store=\"ephemeral\"".to_owned());
        for override_value in &config_overrides {
            // The catalog path is generated inside this owned root, and all
            // other overrides are the restrictive profile's explicit values.
            arguments.push(OsString::from("-c"));
            arguments.push(OsString::from(override_value));
        }
        let mut environment = BTreeMap::new();
        environment.insert(
            OsString::from(CODEX_HOME_ENV),
            owned.codex_home.clone().into_os_string(),
        );
        // Windows components used by networking may need SystemRoot even when
        // the executable itself starts with an otherwise empty environment.
        // Keep this narrow: do not inherit author config, proxy, or tool paths.
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.is_dir())
            .ok_or_else(|| {
                unavailable(
                    "ProviderWorkspaceUnavailable",
                    "The Windows system directory is unavailable for the isolated Codex process.",
                )
            })?;
        environment.insert(OsString::from("SystemRoot"), system_root.into_os_string());
        let security_sha256 = security_hash(&config_overrides);
        let catalog_sha256 = sha256_hex(catalog_json.as_bytes());
        let identity = AppServerLaunchIdentity {
            account_sha256: auth.account_hash().to_owned(),
            security_config_sha256: security_sha256,
            restrictive_catalog_sha256: catalog_sha256,
        };
        let invocation = CliInvocation {
            executable: executable.to_owned(),
            arguments,
            cwd: owned.cwd.clone(),
            environment: EnvironmentPolicy::Explicit(environment),
            packet: Vec::new(),
            limits: ChildLimits {
                overall: MAX_OVERALL,
                ..ChildLimits::default()
            },
        };
        Ok(Self {
            invocation,
            resources: AppServerLaunchResources {
                owned,
                _executable: pinned.file,
            },
            auth,
            identity,
            model: selected_profile.model,
            reasoning_effort: selected_profile.reasoning_effort,
            service_tier: selected_profile
                .service_tier
                .unwrap_or_else(|| "default".to_owned()),
        })
    }

    /// Defaults for the first ephemeral thread. Every request still validates
    /// and sends its exact selected binding at `turn/start`.
    pub fn thread_config(&self) -> CoreResult<super::protocol::ThreadStartConfig> {
        let cwd = self
            .invocation
            .cwd
            .to_str()
            .ok_or_else(|| {
                unavailable(
                    "ProviderWorkspaceUnavailable",
                    "The app-owned Codex working directory is not valid UTF-8.",
                )
            })?
            .to_owned();
        Ok(super::protocol::ThreadStartConfig {
            cwd: Some(cwd),
            base_instructions: None,
            developer_instructions: None,
            model: self.model.clone(),
            reasoning_effort: Some(self.reasoning_effort.clone()),
            service_tier: self.service_tier.clone(),
        })
    }

    /// Read the author's existing login exactly once, then prepare the same
    /// isolated launch.  This never writes or copies the author's auth file.
    pub fn prepare_from_auth_file(
        executable: &Path,
        observed_version: &str,
        expected_executable_sha256: &str,
        catalog: &CodexCatalog,
        selection: &ModelSelection,
        auth_path: &Path,
    ) -> CoreResult<Self> {
        let auth = read_auth_file(auth_path)?;
        Self::prepare(
            executable,
            observed_version,
            expected_executable_sha256,
            catalog,
            selection,
            auth,
        )
    }
}

struct OwnedLaunch {
    root: PathBuf,
    cwd: PathBuf,
    codex_home: PathBuf,
    catalog: PathBuf,
}

impl OwnedLaunch {
    fn new() -> CoreResult<Self> {
        let root = std::env::temp_dir().join(format!(
            "webnovel-codex-app-server-{}",
            uuid::Uuid::new_v4()
        ));
        let cwd = root.join("work");
        let codex_home = root.join("codex-home");
        let catalog = root.join("catalog.json");
        std::fs::create_dir(&root).map_err(|_| {
            unavailable(
                "ProviderWorkspaceUnavailable",
                "The app-owned Codex launch directory could not be created.",
            )
        })?;
        if std::fs::create_dir(&cwd)
            .and_then(|_| std::fs::create_dir(&codex_home))
            .is_err()
        {
            let _ = std::fs::remove_dir(&codex_home);
            let _ = std::fs::remove_dir(&cwd);
            let _ = std::fs::remove_dir(&root);
            return Err(unavailable(
                "ProviderWorkspaceUnavailable",
                "The app-owned Codex launch directories could not be created.",
            ));
        }
        Ok(Self {
            root,
            cwd,
            codex_home,
            catalog,
        })
    }
}

impl AppServerLaunchResources {
    /// The process working directory is owned by this resource bundle and
    /// remains valid until the worker releases it.
    pub fn cwd(&self) -> &Path {
        &self.owned.cwd
    }

    /// The app-owned CODEX_HOME is intentionally empty. It is exposed only
    /// for launch qualification and never for author-data access.
    pub fn codex_home(&self) -> &Path {
        &self.owned.codex_home
    }
}

fn restrictive_multi_model_catalog(
    version_line: &str,
    catalog_path: &Path,
    catalog: &CodexCatalog,
) -> CoreResult<String> {
    let mut models = Vec::with_capacity(catalog.models.len());
    for model in catalog.models.iter().take(MAX_CATALOG_MODELS) {
        let selection = ModelSelection {
            provider_id: "codex".to_owned(),
            model_id: model.model_id.clone(),
            reasoning: model.default_reasoning.clone(),
            service_tier: model.default_service_tier.clone(),
        };
        if !catalog.supports(&selection) {
            continue;
        }
        let profile = CodexLaunchProfile::for_selection(
            version_line,
            catalog_path,
            model,
            &selection,
        )
        .map_err(|_| {
            unavailable(
                "ProviderProfileInvalid",
                "A checked Codex model could not be represented in the restrictive catalog.",
            )
        })?;
        let value: serde_json::Value =
            serde_json::from_str(&profile.catalog_json).map_err(|_| {
                unavailable(
                    "ProviderProfileInvalid",
                    "The restrictive Codex catalog could not be encoded.",
                )
            })?;
        if let Some(row) = value
            .get("models")
            .and_then(serde_json::Value::as_array)
            .and_then(|rows| rows.first())
        {
            models.push(row.clone());
        }
    }
    if models.is_empty() {
        return Err(unavailable(
            "ProviderProfileInvalid",
            "The checked Codex catalog has no launchable model.",
        ));
    }
    serde_json::to_string(&serde_json::json!({ "models": models })).map_err(|_| {
        unavailable(
            "ProviderProfileInvalid",
            "The restrictive Codex catalog could not be encoded.",
        )
    })
}

fn security_hash(overrides: &[String]) -> String {
    let mut stable = overrides
        .iter()
        .filter(|value| !value.starts_with("model_catalog_json="))
        .cloned()
        .collect::<Vec<_>>();
    stable.push("environment=explicit:CODEX_HOME,SystemRoot".to_owned());
    stable.sort();
    sha256_hex(stable.join("\n").as_bytes())
}

struct PinnedExecutable {
    file: File,
}

fn pin_executable(path: &Path, expected: &str) -> CoreResult<PinnedExecutable> {
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(unavailable(
            "CodexUnavailable",
            "The Codex executable path is invalid.",
        ));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .map_err(|_| {
            unavailable(
                "CodexUnavailable",
                "The checked Codex executable is unavailable.",
            )
        })?;
    let metadata = file.metadata().map_err(|_| {
        unavailable(
            "CodexUnavailable",
            "The checked Codex executable could not be inspected.",
        )
    })?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 * 1024 {
        return Err(unavailable(
            "CodexVersionChanged",
            "The Codex installation changed. Check the connection again.",
        ));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            unavailable(
                "CodexUnavailable",
                "The Codex executable could not be fingerprinted.",
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let fingerprint = hex_digest(&hasher.finalize());
    if fingerprint != expected {
        return Err(unavailable(
            "CodexVersionChanged",
            "The checked Codex executable changed. Check its connection again before starting a request.",
        ));
    }
    Ok(PinnedExecutable { file })
}

fn validate_version(version: &str) -> CoreResult<()> {
    if version.is_empty() || version.len() > 128 || version.chars().any(char::is_control) {
        return Err(unavailable(
            "CodexCapabilityUnavailable",
            "The installed Codex version is invalid.",
        ));
    }
    Ok(())
}

fn validate_fingerprint(value: &str) -> CoreResult<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(unavailable(
            "CodexCapabilityUnavailable",
            "The checked Codex executable fingerprint is invalid.",
        ));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn cleanup_owned_root(root: &Path) {
    let Some(name) = root.file_name().and_then(|value| value.to_str()) else {
        return;
    };
    let expected_parent = std::env::temp_dir();
    if root.parent() != Some(expected_parent.as_path())
        || !name.starts_with("webnovel-codex-app-server-")
    {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.take(MAX_CLEANUP_ENTRIES).flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            let _ = std::fs::remove_dir_all(path);
        } else {
            // Symlinks are removed as directory entries and never traversed.
            let _ = std::fs::remove_file(path);
        }
    }
    let _ = std::fs::remove_dir(root);
}

fn hex_digest(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unavailable(code: &'static str, message: &'static str) -> CoreError {
    CoreError::new(code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::catalog::ServiceTier;
    use crate::providers::codex_catalog::CodexCatalogModel;

    #[test]
    fn security_hash_ignores_only_volatile_catalog_path() {
        let first = security_hash(&[
            "model=\"gpt-6-astra\"".into(),
            "model_catalog_json=\"C:/one/catalog.json\"".into(),
            "features.shell_tool=false".into(),
        ]);
        let second = security_hash(&[
            "model_catalog_json=\"D:/two/catalog.json\"".into(),
            "features.shell_tool=false".into(),
            "model=\"gpt-6-astra\"".into(),
        ]);
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn invalid_version_and_fingerprint_are_refused() {
        assert!(validate_version("codex-cli 0.153.4\n").is_err());
        assert!(validate_version("0.153.4").is_ok());
        assert!(validate_fingerprint(&"a".repeat(63)).is_err());
        assert!(validate_fingerprint(&"g".repeat(64)).is_err());
        assert!(validate_fingerprint(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn prepare_pins_synthetic_executable_and_refuses_changed_identity() {
        let root = std::env::temp_dir().join(format!(
            "webnovel-codex-app-server-fixture-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&root).expect("fixture root");
        let executable = root.join("codex.exe");
        std::fs::write(&executable, b"synthetic codex executable").expect("fixture executable");
        let mut bytes = Vec::new();
        File::open(&executable)
            .expect("open fixture")
            .read_to_end(&mut bytes)
            .expect("read fixture");
        let expected = sha256_hex(&bytes);
        let auth_path = root.join("auth.json");
        std::fs::write(
            &auth_path,
            r#"{"tokens":{"access_token":"eyJhbGciOiJub25lIn0.eyJleHAiOjQxMDI0NDQ4MDB9.ZmFrZQ","account_id":"fixture-account"}}"#,
        )
        .expect("fixture auth");
        let catalog = CodexCatalog {
            cli_version: "0.153.4".into(),
            executable_sha256: expected.clone(),
            discovered_at: "0".into(),
            models: vec![CodexCatalogModel {
                model_id: "gpt-6-astra".into(),
                label: "GPT-6 Astra".into(),
                reasoning_levels: vec!["low".into()],
                default_reasoning: Some("low".into()),
                service_tiers: vec![ServiceTier {
                    id: "priority".into(),
                    label: "Fast".into(),
                }],
                default_service_tier: Some("priority".into()),
            }],
        };
        let selection = ModelSelection {
            provider_id: "codex".into(),
            model_id: "gpt-6-astra".into(),
            reasoning: Some("low".into()),
            service_tier: Some("priority".into()),
        };
        let launch = AppServerLaunch::prepare_from_auth_file(
            &executable,
            "0.153.4",
            &expected,
            &catalog,
            &selection,
            &auth_path,
        )
        .expect("synthetic launch");
        let owned_root = launch
            .resources
            .cwd()
            .parent()
            .expect("owned root")
            .to_owned();
        assert!(
            launch
                .invocation
                .arguments
                .iter()
                .any(|arg| arg == "--stdio")
        );
        assert_eq!(launch.model, "gpt-6-astra");
        assert_eq!(launch.reasoning_effort, "low");
        assert_eq!(launch.service_tier, "priority");
        drop(launch);
        assert!(!owned_root.exists());

        let auth = read_auth_file(&auth_path).expect("fixture auth again");
        let mut mismatched_catalog = catalog.clone();
        mismatched_catalog.executable_sha256 = "a".repeat(64);
        let mismatch = AppServerLaunch::prepare(
            &executable,
            "0.153.4",
            &"a".repeat(64),
            &mismatched_catalog,
            &selection,
            auth,
        )
        .err()
        .unwrap();
        assert_eq!(mismatch.code, "CodexVersionChanged");
        std::fs::remove_file(auth_path).expect("remove fixture auth");
        std::fs::remove_file(executable).expect("remove fixture executable");
        std::fs::remove_dir(root).expect("remove fixture root");
    }
}
