//! App-local endpoint setup. Credentials never enter project or renderer reads.
use crate::app_state::AppState;
use crate::project_commands::execute;
use serde::{Deserialize, Serialize};
use tauri::State;
use webnovel_core::library::Library;
use webnovel_core::projects::{CoreError, CoreResult};
use webnovel_core::providers::credentials::{
    CredentialStore, CredentialTarget, WindowsCredentialStore,
};
use webnovel_core::providers::endpoints::{
    EndpointProfile, EndpointProfileDraft, EndpointProfilesSettings,
};
use webnovel_core::providers::openai_compatible::{
    OpenAiCompatibleAdapter, OpenAiCompatibleConfig,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointSettingsView {
    revision: String,
    profiles: Vec<EndpointView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EndpointView {
    id: String,
    label: String,
    base_url: String,
    enabled: bool,
    json_mode: bool,
    config_revision: String,
    has_api_key: bool,
    api_key_configured: bool,
    manual_model_ids: Vec<String>,
    cached_model_ids: Vec<String>,
}

impl From<EndpointProfilesSettings> for EndpointSettingsView {
    fn from(settings: EndpointProfilesSettings) -> Self {
        Self::from_store(settings, &WindowsCredentialStore)
    }
}

impl EndpointSettingsView {
    /// Build the renderer-safe endpoint view while checking referenced
    /// credentials through the native store.  The secret is only borrowed by
    /// the validator and is never returned to the renderer.
    pub(crate) fn from_store(
        settings: EndpointProfilesSettings,
        store: &dyn CredentialStore,
    ) -> Self {
        Self {
            revision: settings.revision,
            profiles: settings
                .profiles
                .into_iter()
                .map(|profile| {
                    let has_api_key = has_usable_api_key(&profile, store);
                    EndpointView {
                        id: profile.id,
                        label: profile.label,
                        base_url: profile.base_url,
                        enabled: profile.enabled,
                        json_mode: profile.json_mode,
                        config_revision: profile.config_revision,
                        has_api_key,
                        api_key_configured: profile.credential_ref.is_some(),
                        manual_model_ids: profile.manual_model_ids,
                        cached_model_ids: profile.cached_model_ids,
                    }
                })
                .collect(),
        }
    }
}

// Deliberately no Debug/Serialize: replace carries a transient renderer input.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ApiKeyChange {
    Keep,
    Replace { value: String },
    Remove,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveEndpoint {
    expected_revision: String,
    profile_id: Option<String>,
    label: String,
    base_url: String,
    enabled: bool,
    json_mode: bool,
    manual_model_ids: Vec<String>,
    api_key: ApiKeyChange,
}

fn unknown_profile() -> CoreError {
    CoreError::new(
        "UnknownEndpointProfile",
        "This API connection no longer exists. Reload Settings.",
    )
}

fn validate_api_key_bytes(bytes: &[u8]) -> CoreResult<&str> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        CoreError::new(
            "InvalidCredential",
            "The stored API key is invalid. Enter it again in Settings.",
        )
    })?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(CoreError::new(
            "InvalidCredential",
            "The stored API key is invalid. Enter it again in Settings.",
        ));
    }
    Ok(text)
}

fn read_api_key(
    profile: &EndpointProfile,
    store: &dyn CredentialStore,
) -> CoreResult<Option<String>> {
    let Some(reference) = &profile.credential_ref else {
        return Ok(None);
    };
    let target = CredentialTarget::parse(reference)?;
    let secret = store.read(&target)?.ok_or_else(|| {
        CoreError::new(
            "CredentialMissing",
            "The saved API key is unavailable on this computer. Enter it again in Settings.",
        )
    })?;
    Ok(Some(
        validate_api_key_bytes(secret.expose_bytes())?.to_owned(),
    ))
}

fn validate_credential_reference(
    profile: &EndpointProfile,
    store: &dyn CredentialStore,
) -> CoreResult<()> {
    let Some(reference) = &profile.credential_ref else {
        return Ok(());
    };
    let target = CredentialTarget::parse(reference)?;
    let secret = store.read(&target)?.ok_or_else(|| {
        CoreError::new(
            "CredentialMissing",
            "The saved API key is unavailable on this computer. Enter it again in Settings.",
        )
    })?;
    validate_api_key_bytes(secret.expose_bytes()).map(|_| ())
}

/// Return whether this profile can construct an adapter without exposing its
/// credential.  Anonymous endpoints are usable; referenced credentials must
/// exist and pass the same byte validation used by adapter construction.
pub(crate) fn credential_available(profile: &EndpointProfile, store: &dyn CredentialStore) -> bool {
    validate_credential_reference(profile, store).is_ok()
}

/// Return whether a referenced API key is usable for renderer metadata.  An
/// anonymous profile intentionally reports `false` because no key is present.
pub(crate) fn has_usable_api_key(profile: &EndpointProfile, store: &dyn CredentialStore) -> bool {
    profile.credential_ref.is_some() && credential_available(profile, store)
}

pub fn adapter_for_profile(
    profile: &EndpointProfile,
    store: &dyn CredentialStore,
) -> CoreResult<OpenAiCompatibleAdapter> {
    if !profile.enabled {
        return Err(CoreError::new(
            "ProviderUnavailable",
            "This API connection is disabled in Settings.",
        ));
    }
    let key = read_api_key(profile, store)?;
    let config = OpenAiCompatibleConfig::new(&profile.base_url, key).map_err(provider_error)?;
    OpenAiCompatibleAdapter::new(config).map_err(provider_error)
}

fn provider_error(error: webnovel_core::providers::adapter::ProviderError) -> CoreError {
    CoreError::new("EndpointUnavailable", &error.detail)
}

fn save_with_store(
    library: &mut Library,
    store: &dyn CredentialStore,
    request: SaveEndpoint,
) -> CoreResult<EndpointSettingsView> {
    let before = library.endpoint_profiles()?;
    if request.expected_revision != before.revision {
        return Err(CoreError::new(
            "PreferenceConflict",
            "API connections changed. Reload Settings before saving.",
        ));
    }
    let previous = request
        .profile_id
        .as_ref()
        .map(|id| {
            before
                .profiles
                .iter()
                .find(|p| &p.id == id)
                .ok_or_else(unknown_profile)
        })
        .transpose()?;
    let old_reference = previous.and_then(|p| p.credential_ref.clone());
    let new_target = match &request.api_key {
        ApiKeyChange::Replace { value } => {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(CoreError::new(
                    "InvalidCredential",
                    "Enter a nonempty API key without line breaks.",
                ));
            }
            Some(store.write_new(value.as_bytes())?)
        }
        _ => None,
    };
    let credential_ref = match &request.api_key {
        ApiKeyChange::Keep => old_reference.clone(),
        ApiKeyChange::Remove => None,
        ApiKeyChange::Replace { .. } => {
            new_target.as_ref().map(|target| target.as_str().to_owned())
        }
    };
    let draft = EndpointProfileDraft {
        id: request.profile_id,
        label: request.label,
        base_url: request.base_url,
        enabled: request.enabled,
        json_mode: request.json_mode,
        manual_model_ids: request.manual_model_ids,
        credential_ref,
    };
    let saved = library.save_endpoint_profiles(&request.expected_revision, vec![draft]);
    let after = library.endpoint_profiles();
    // A lost SQLite acknowledgment can still have published the new key. Only
    // delete a new target after a successful read proves it is unreferenced.
    if let (Some(target), Ok(after)) = (&new_target, &after)
        && !after
            .profiles
            .iter()
            .any(|p| p.credential_ref.as_deref() == Some(target.as_str()))
    {
        store.delete(target).map_err(|_| CoreError::new(
            "CredentialCleanupIncomplete",
            "The connection was not saved, and its unused API key could not be removed from this computer's credential store. Reload saved connections before trying again.",
        ))?;
    }
    saved?;
    let after = after?;
    if let Some(old) = old_reference
        && !after
            .profiles
            .iter()
            .any(|p| p.credential_ref.as_deref() == Some(&old))
        && let Ok(target) = CredentialTarget::parse(&old)
    {
        store.delete(&target).map_err(|_| CoreError::new(
            "CredentialCleanupIncomplete",
            "The connection was saved, but its old API key could not be removed from this computer's credential store. Reload saved connections to confirm the saved configuration.",
        ))?;
    }
    Ok(after.into())
}

#[tauri::command]
pub async fn endpoint_settings( state: State<'_, AppState>,
) -> CoreResult<EndpointSettingsView> {
    let app = &*state;
    let state = &app.library;
    let state = state.clone();
    execute(move || {
        Ok(state
            .0
            .lock()
            .map_err(|_| crate::provider_commands::unavailable())?
            .endpoint_profiles()?
            .into())
    })
    .await
}

#[tauri::command]
pub async fn save_endpoint_settings(
    request: SaveEndpoint, state: State<'_, AppState>,
) -> CoreResult<EndpointSettingsView> {
    let app = &*state;
    let state = &app.library;
    let state = state.clone();
    execute(move || {
        save_with_store(
            &mut *state
                .0
                .lock()
                .map_err(|_| crate::provider_commands::unavailable())?,
            &WindowsCredentialStore,
            request,
        )
    })
    .await
}

#[tauri::command]
pub async fn discover_endpoint_models(
    profile_id: String,
    config_revision: String,
    discovery_id: String,
    discovery: State<'_, crate::endpoint_discovery::EndpointDiscovery>, state: State<'_, AppState>,
) -> CoreResult<EndpointSettingsView> {
    let app = &*state;
    let state = &app.library;
    let read = discovery.begin(discovery_id)?;
    let state = state.clone();
    let read_state = state.clone();
    let profile_id_copy = profile_id.clone();
    let revision_copy = config_revision.clone();
    let adapter = execute(move || {
        let library = read_state
            .0
            .lock()
            .map_err(|_| crate::provider_commands::unavailable())?;
        let profile = library
            .endpoint_profiles()?
            .profiles
            .into_iter()
            .find(|p| p.id == profile_id_copy)
            .ok_or_else(unknown_profile)?;
        if profile.config_revision != revision_copy {
            return Err(CoreError::new(
                "EndpointConfigConflict",
                "This connection changed. Reload Settings before discovering models.",
            ));
        }
        adapter_for_profile(&profile, &WindowsCredentialStore)
    })
    .await?;
    // Explicit discovery is a read, never a generation or an automatic picker probe.
    let ids = adapter
        .list_models_async(&read.token)
        .await
        .map_err(|error| {
            if error.kind == webnovel_core::providers::adapter::ProviderErrorKind::Cancelled {
                CoreError::new("DiscoveryCancelled", "Model search stopped.")
            } else {
                provider_error(error)
            }
        })?;
    if read.token.is_cancelled() {
        return Err(CoreError::new(
            "DiscoveryCancelled",
            "Model search cancelled.",
        ));
    }
    execute(move || {
        let mut library = state
            .0
            .lock()
            .map_err(|_| crate::provider_commands::unavailable())?;
        library.refresh_endpoint_models(&profile_id, &config_revision, ids)?;
        Ok(library.endpoint_profiles()?.into())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use webnovel_core::providers::credentials::SecretValue;

    const FIRST_KEY: &str = "synthetic-key-one-DO-NOT-PERSIST";
    const SECOND_KEY: &str = "synthetic-key-two-DO-NOT-PERSIST";
    const FAILED_KEY: &str = "synthetic-key-three-DO-NOT-PERSIST";

    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "wns-native-endpoint-{label}-{}",
                std::process::id()
            ));
            if root.exists() {
                std::fs::remove_dir_all(&root).expect("remove prior synthetic endpoint fixture");
            }
            std::fs::create_dir_all(&root).expect("create synthetic endpoint fixture");
            Self(root)
        }

        fn app(&self) -> std::path::PathBuf {
            self.0.join("app")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(
                self.0.starts_with(std::env::temp_dir())
                    && self.0.file_name().is_some_and(|name| name
                        .to_string_lossy()
                        .starts_with("wns-native-endpoint-")),
                "refuse to remove an unexpected test path"
            );
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Default)]
    struct SyntheticCredentialStore {
        fail_delete: Mutex<bool>,
        next: Mutex<u32>,
        values: Mutex<HashMap<CredentialTarget, Vec<u8>>>,
        writes: Mutex<Vec<(CredentialTarget, Vec<u8>)>>,
        deletes: Mutex<Vec<CredentialTarget>>,
    }

    impl SyntheticCredentialStore {
        fn target(index: u32) -> CredentialTarget {
            CredentialTarget::parse(&format!(
                "WebnovelStudioV3/Profile/00000000-0000-0000-0000-{index:012x}"
            ))
            .expect("synthetic target is canonical")
        }

        fn writes(&self) -> Vec<(CredentialTarget, Vec<u8>)> {
            self.writes.lock().unwrap().clone()
        }

        fn deletes(&self) -> Vec<CredentialTarget> {
            self.deletes.lock().unwrap().clone()
        }

        fn has(&self, target: &CredentialTarget, expected: &[u8]) -> bool {
            self.values
                .lock()
                .unwrap()
                .get(target)
                .is_some_and(|value| value == expected)
        }
    }

    impl CredentialStore for SyntheticCredentialStore {
        fn read(&self, target: &CredentialTarget) -> CoreResult<Option<SecretValue>> {
            self.values
                .lock()
                .unwrap()
                .get(target)
                .cloned()
                .map(SecretValue::new)
                .transpose()
        }

        fn write_new(&self, secret: &[u8]) -> CoreResult<CredentialTarget> {
            let owned = SecretValue::new(secret.to_vec())?;
            let bytes = owned.expose_bytes().to_vec();
            let index = {
                let mut next = self.next.lock().unwrap();
                *next += 1;
                *next
            };
            let target = Self::target(index);
            self.values
                .lock()
                .unwrap()
                .insert(target.clone(), bytes.clone());
            self.writes.lock().unwrap().push((target.clone(), bytes));
            Ok(target)
        }

        fn delete(&self, target: &CredentialTarget) -> CoreResult<()> {
            if *self.fail_delete.lock().unwrap() {
                return Err(CoreError::new("SyntheticStoreFailure", FIRST_KEY));
            }
            self.values.lock().unwrap().remove(target);
            self.deletes.lock().unwrap().push(target.clone());
            Ok(())
        }
    }

    fn request(
        expected_revision: &str,
        profile_id: Option<String>,
        base_url: &str,
        api_key: ApiKeyChange,
    ) -> SaveEndpoint {
        SaveEndpoint {
            expected_revision: expected_revision.to_owned(),
            profile_id,
            label: "Synthetic endpoint".to_owned(),
            base_url: base_url.to_owned(),
            enabled: true,
            json_mode: false,
            manual_model_ids: vec!["story-model".to_owned()],
            api_key,
        }
    }

    fn no_secret_in_library_files(root: &std::path::Path, secret: &str) {
        for name in [
            "library.sqlite3",
            "library.sqlite3-wal",
            "library.sqlite3-shm",
        ] {
            let path = root.join(name);
            if let Ok(bytes) = std::fs::read(path) {
                assert!(
                    !bytes
                        .windows(secret.len())
                        .any(|window| window == secret.as_bytes()),
                    "synthetic key leaked into {name}"
                );
            }
        }
    }

    #[test]
    fn rotation_cleanup_failure_reports_saved_configuration_without_exposing_store_errors() {
        let fixture = Fixture::new("cleanup-failure");
        let mut library = Library::open(fixture.app()).unwrap();
        let store = SyntheticCredentialStore::default();
        save_with_store(
            &mut library,
            &store,
            request(
                "0",
                None,
                "https://example.test",
                ApiKeyChange::Replace {
                    value: FIRST_KEY.to_owned(),
                },
            ),
        )
        .unwrap();
        let before = library.endpoint_profiles().unwrap();
        *store.fail_delete.lock().unwrap() = true;
        let error = save_with_store(
            &mut library,
            &store,
            request(
                &before.revision,
                Some(before.profiles[0].id.clone()),
                "https://example.test",
                ApiKeyChange::Replace {
                    value: SECOND_KEY.to_owned(),
                },
            ),
        )
        .err()
        .expect("cleanup failure is surfaced");
        assert_eq!(error.code, "CredentialCleanupIncomplete");
        assert!(error.detail.contains("connection was saved"));
        assert!(!error.detail.contains(FIRST_KEY));
        let after = library.endpoint_profiles().unwrap();
        assert_ne!(
            after.profiles[0].credential_ref,
            before.profiles[0].credential_ref
        );
        assert!(has_usable_api_key(&after.profiles[0], &store));
        assert_eq!(store.writes().len(), 2);
    }

    #[test]
    fn save_with_store_rotates_keys_and_captured_adapters_keep_redacted_config() {
        let fixture = Fixture::new("rotation");
        let mut library = Library::open(fixture.app()).unwrap();
        let store = SyntheticCredentialStore::default();

        save_with_store(
            &mut library,
            &store,
            request(
                "0",
                None,
                "https://example.test",
                ApiKeyChange::Replace {
                    value: FIRST_KEY.to_owned(),
                },
            ),
        )
        .unwrap();
        let first = library.endpoint_profiles().unwrap().profiles[0].clone();
        let first_ref = first.credential_ref.clone().unwrap();
        let first_target = CredentialTarget::parse(&first_ref).unwrap();
        let captured = adapter_for_profile(&first, &store).unwrap();
        assert_eq!(
            captured.config().base_url().as_str(),
            "https://example.test/v1"
        );
        let captured_debug = format!("{captured:?}");
        assert!(!captured_debug.contains(FIRST_KEY));
        assert!(captured_debug.contains("[REDACTED]"));

        save_with_store(
            &mut library,
            &store,
            request(
                "1",
                Some(first.id.clone()),
                "https://example.test/v1",
                ApiKeyChange::Replace {
                    value: SECOND_KEY.to_owned(),
                },
            ),
        )
        .unwrap();
        let second = library.endpoint_profiles().unwrap().profiles[0].clone();
        let second_target =
            CredentialTarget::parse(second.credential_ref.as_deref().unwrap()).unwrap();
        assert_ne!(first_target, second_target);
        assert!(!store.has(&first_target, FIRST_KEY.as_bytes()));
        assert!(store.has(&second_target, SECOND_KEY.as_bytes()));
        assert!(store.deletes().contains(&first_target));
        assert_eq!(second.base_url, first.base_url);
        assert_eq!(second.manual_model_ids, first.manual_model_ids);
        assert_eq!(captured.config().base_url().as_str(), second.base_url);
        let replacement = adapter_for_profile(&second, &store).unwrap();
        let replacement_debug = format!("{replacement:?}");
        assert!(!replacement_debug.contains(FIRST_KEY));
        assert!(!replacement_debug.contains(SECOND_KEY));
        assert!(replacement_debug.contains("[REDACTED]"));
    }

    #[test]
    fn disabled_and_keyless_profiles_are_explicit_without_falling_back_on_missing_keys() {
        let fixture = Fixture::new("profile-boundaries");
        let mut library = Library::open(fixture.app()).unwrap();
        let store = SyntheticCredentialStore::default();
        save_with_store(
            &mut library,
            &store,
            request("0", None, "https://example.test", ApiKeyChange::Keep),
        )
        .unwrap();
        let mut profile = library.endpoint_profiles().unwrap().profiles[0].clone();
        profile.enabled = false;
        profile.credential_ref = None;
        let disabled = adapter_for_profile(&profile, &store).unwrap_err();
        assert_eq!(disabled.code, "ProviderUnavailable");

        profile.enabled = true;
        let anonymous = adapter_for_profile(&profile, &store).unwrap();
        assert_eq!(
            anonymous.config().base_url().as_str(),
            "https://example.test/v1"
        );

        profile.credential_ref = Some(SyntheticCredentialStore::target(99).as_str().to_owned());
        let missing = adapter_for_profile(&profile, &store).unwrap_err();
        assert_eq!(missing.code, "CredentialMissing");
        assert!(!missing.detail.to_ascii_lowercase().contains("anonymous"));
    }

    #[test]
    fn credential_readiness_is_fail_closed_for_missing_or_invalid_store_values() {
        let fixture = Fixture::new("credential-readiness");
        let mut library = Library::open(fixture.app()).unwrap();
        let store = SyntheticCredentialStore::default();
        save_with_store(
            &mut library,
            &store,
            request(
                "0",
                None,
                "https://example.test",
                ApiKeyChange::Replace {
                    value: FIRST_KEY.to_owned(),
                },
            ),
        )
        .unwrap();
        let mut profile = library.endpoint_profiles().unwrap().profiles[0].clone();
        let target = CredentialTarget::parse(profile.credential_ref.as_deref().unwrap()).unwrap();
        assert!(credential_available(&profile, &store));
        assert!(has_usable_api_key(&profile, &store));

        store.values.lock().unwrap().remove(&target);
        assert!(!credential_available(&profile, &store));
        assert!(!has_usable_api_key(&profile, &store));
        let missing_view = serde_json::to_value(EndpointSettingsView::from_store(
            library.endpoint_profiles().unwrap(),
            &store,
        ))
        .unwrap();
        assert_eq!(missing_view["profiles"][0]["hasApiKey"], false);
        assert_eq!(missing_view["profiles"][0]["apiKeyConfigured"], true);

        store
            .values
            .lock()
            .unwrap()
            .insert(target.clone(), vec![0xff]);
        assert!(!credential_available(&profile, &store));
        assert!(!has_usable_api_key(&profile, &store));

        store
            .values
            .lock()
            .unwrap()
            .insert(target, b"key\nwith-control".to_vec());
        assert!(!credential_available(&profile, &store));
        assert!(!has_usable_api_key(&profile, &store));

        profile.credential_ref = Some("WebnovelStudioV3/Profile/not-a-uuid".to_owned());
        assert!(!credential_available(&profile, &store));
        assert!(!has_usable_api_key(&profile, &store));

        profile.credential_ref = None;
        assert!(credential_available(&profile, &store));
        assert!(!has_usable_api_key(&profile, &store));
    }

    #[test]
    fn invalid_url_cleans_only_the_unpublished_key_and_stale_cas_writes_nothing() {
        let fixture = Fixture::new("cleanup");
        let mut library = Library::open(fixture.app()).unwrap();
        let store = SyntheticCredentialStore::default();
        save_with_store(
            &mut library,
            &store,
            request(
                "0",
                None,
                "https://example.test",
                ApiKeyChange::Replace {
                    value: FIRST_KEY.to_owned(),
                },
            ),
        )
        .unwrap();
        let before = library.endpoint_profiles().unwrap();
        let existing_ref = before.profiles[0].credential_ref.clone().unwrap();
        let writes_before_failure = store.writes().len();
        let invalid = save_with_store(
            &mut library,
            &store,
            request(
                &before.revision,
                Some(before.profiles[0].id.clone()),
                "not-a-url",
                ApiKeyChange::Replace {
                    value: FAILED_KEY.to_owned(),
                },
            ),
        )
        .err()
        .expect("invalid endpoint URL must reject before publication");
        assert_eq!(invalid.code, "InvalidEndpointProfile");
        let writes_after_failure = store.writes();
        assert_eq!(writes_after_failure.len(), writes_before_failure + 1);
        let failed_target = writes_after_failure.last().unwrap().0.clone();
        assert!(!store.has(&failed_target, FAILED_KEY.as_bytes()));
        assert!(store.deletes().contains(&failed_target));
        assert_eq!(
            library.endpoint_profiles().unwrap().profiles[0]
                .credential_ref
                .as_deref(),
            Some(existing_ref.as_str())
        );

        let stale_writes = store.writes().len();
        let stale = save_with_store(
            &mut library,
            &store,
            request(
                "0",
                Some(before.profiles[0].id.clone()),
                "https://other.example",
                ApiKeyChange::Replace {
                    value: SECOND_KEY.to_owned(),
                },
            ),
        )
        .err()
        .expect("stale endpoint revision must reject before writing a key");
        assert_eq!(stale.code, "PreferenceConflict");
        assert_eq!(store.writes().len(), stale_writes);
    }

    #[test]
    fn renderer_view_and_sqlite_wal_contain_only_nonsecret_endpoint_metadata() {
        let fixture = Fixture::new("persistence");
        let mut library = Library::open(fixture.app()).unwrap();
        let store = SyntheticCredentialStore::default();
        save_with_store(
            &mut library,
            &store,
            request(
                "0",
                None,
                "https://example.test",
                ApiKeyChange::Replace {
                    value: FIRST_KEY.to_owned(),
                },
            ),
        )
        .unwrap();
        let settings = library.endpoint_profiles().unwrap();
        let view = EndpointSettingsView::from_store(settings.clone(), &store);
        let rendered = serde_json::to_string(&view).unwrap();
        assert!(rendered.contains("\"hasApiKey\":true"));
        assert!(rendered.contains("\"apiKeyConfigured\":true"));
        assert!(!rendered.contains("credentialRef"));
        assert!(!rendered.contains(FIRST_KEY));
        assert!(!rendered.contains("\"apiKey\":"));
        no_secret_in_library_files(&fixture.app(), FIRST_KEY);
        drop(library);
        no_secret_in_library_files(&fixture.app(), FIRST_KEY);
    }
}
