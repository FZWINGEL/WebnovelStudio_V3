use rusqlite::Connection;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::library::Library;
use webnovel_core::providers::catalog::DispatchResolution;
use webnovel_core::providers::endpoints::{
    ENDPOINT_PROVIDER_PREFIX, EndpointProfileDraft, MAX_ENDPOINT_MODEL_IDS,
};
use webnovel_core::providers::preferences::{ModelKey, ModelSelection};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-provider-endpoints-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0.starts_with(std::env::temp_dir())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("wns-provider-endpoints-")
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn local_profile() -> EndpointProfileDraft {
    let mut draft = EndpointProfileDraft::new("Local gateway", "http://localhost:1234");
    draft.credential_ref =
        Some("WebnovelStudioV3/Profile/00000000-0000-0000-0000-000000000001".to_owned());
    draft.manual_model_ids = vec!["manual-model".to_owned()];
    draft
}

#[test]
fn profiles_normalize_urls_and_store_only_an_opaque_credential_reference() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let state = library
        .save_endpoint_profiles("0", vec![local_profile()])
        .unwrap();
    assert!(matches!(
        state.dispatch,
        DispatchResolution::LocalMock { .. }
    ));
    let settings = library.endpoint_profiles().unwrap();
    assert_eq!(settings.revision, "1");
    let profile = &settings.profiles[0];
    assert!(profile.id.starts_with(ENDPOINT_PROVIDER_PREFIX));
    assert_eq!(profile.base_url, "http://localhost:1234/v1");
    assert_eq!(profile.config_revision, "0");
    assert_eq!(
        profile.credential_ref.as_deref(),
        Some("WebnovelStudioV3/Profile/00000000-0000-0000-0000-000000000001")
    );
    let serialized = serde_json::to_string(profile).unwrap();
    assert!(!serialized.contains("sk-"));
}

#[test]
fn discovery_is_fenced_and_saved_model_references_survive_omission_and_disable() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    library
        .save_endpoint_profiles("0", vec![local_profile()])
        .unwrap();
    let profile = library.endpoint_profiles().unwrap().profiles[0].clone();
    library
        .refresh_endpoint_models(
            &profile.id,
            &profile.config_revision,
            vec!["manual-model".to_owned(), "cached-model".to_owned()],
        )
        .unwrap();
    let stale = library
        .refresh_endpoint_models(&profile.id, "1", vec!["new-model".to_owned()])
        .unwrap_err();
    assert_eq!(stale.code, "EndpointConfigConflict");

    let state = library
        .save_model_settings(
            "0",
            ModelSelection {
                provider_id: profile.id.clone(),
                model_id: "cached-model".to_owned(),
                reasoning: None,
                service_tier: None,
            },
            vec![ModelKey::new(profile.id.clone(), "cached-model")],
        )
        .unwrap();
    assert!(matches!(state.dispatch, DispatchResolution::Blocked { .. }));
    let cached = state
        .catalog
        .models
        .iter()
        .find(|model| model.key.model_id == "cached-model")
        .unwrap();
    assert!(!cached.ready);

    let mut changed = EndpointProfileDraft::new("Local gateway", "http://localhost:1234");
    changed.id = Some(profile.id.clone());
    changed.enabled = false;
    changed.credential_ref = profile.credential_ref.clone();
    changed.manual_model_ids = profile.manual_model_ids.clone();
    let disabled = library.save_endpoint_profiles("2", vec![changed]).unwrap();
    assert_eq!(disabled.settings.active.model_id, "cached-model");
    let disabled_model = disabled
        .catalog
        .models
        .iter()
        .find(|model| model.key.model_id == "cached-model")
        .unwrap();
    assert!(disabled_model.status_detail.contains("disabled"));
    let stale_after_disable = library
        .refresh_endpoint_models(&profile.id, &profile.config_revision, vec![])
        .unwrap_err();
    assert_eq!(stale_after_disable.code, "EndpointConfigConflict");
}

#[test]
fn changing_endpoint_route_invalidates_discovery_but_preserves_the_selected_tombstone() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    library
        .save_endpoint_profiles("0", vec![local_profile()])
        .unwrap();
    let profile = library.endpoint_profiles().unwrap().profiles[0].clone();
    library
        .refresh_endpoint_models(
            &profile.id,
            &profile.config_revision,
            vec!["manual-model".to_owned(), "cached-model".to_owned()],
        )
        .unwrap();
    library
        .save_model_settings(
            "0",
            ModelSelection {
                provider_id: profile.id.clone(),
                model_id: "cached-model".to_owned(),
                reasoning: None,
                service_tier: None,
            },
            Vec::new(),
        )
        .unwrap();

    let mut renamed = EndpointProfileDraft::new("Local gateway", "http://localhost:4321");
    renamed.id = Some(profile.id.clone());
    renamed.credential_ref = profile.credential_ref.clone();
    renamed.manual_model_ids = profile.manual_model_ids.clone();
    let state = library.save_endpoint_profiles("2", vec![renamed]).unwrap();
    let updated = library.endpoint_profiles().unwrap().profiles[0].clone();
    assert_eq!(updated.config_revision, "1");
    assert!(updated.cached_model_ids.is_empty());
    assert_eq!(updated.manual_model_ids, vec!["manual-model"]);
    let selected = state
        .catalog
        .models
        .iter()
        .find(|model| model.key.model_id == "cached-model")
        .unwrap();
    assert!(!selected.ready);
    assert!(selected.status_detail.contains("omitted"));
}

#[test]
fn omitted_profiles_are_retained_and_preference_cas_is_enforced() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    library
        .save_endpoint_profiles("0", vec![local_profile()])
        .unwrap();
    let mut second = EndpointProfileDraft::new("Second", "https://example.test/api/v1");
    second.manual_model_ids = vec!["second-model".to_owned()];
    library.save_endpoint_profiles("1", vec![second]).unwrap();
    let conflict = library.save_endpoint_profiles("0", Vec::new()).unwrap_err();
    assert_eq!(conflict.code, "PreferenceConflict");
    library.save_endpoint_profiles("2", Vec::new()).unwrap();
    assert_eq!(library.endpoint_profiles().unwrap().profiles.len(), 2);
}

#[test]
fn profile_and_model_bounds_and_endpoint_paths_are_validated() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let mut path = EndpointProfileDraft::new("Gateway", "https://example.test/api/chat/");
    path.manual_model_ids = vec!["model".to_owned()];
    library.save_endpoint_profiles("0", vec![path]).unwrap();
    assert_eq!(
        library.endpoint_profiles().unwrap().profiles[0].base_url,
        "https://example.test/api/chat"
    );

    let mut duplicate = EndpointProfileDraft::new("Duplicate", "http://localhost:1234");
    duplicate.manual_model_ids = vec!["same".to_owned(), "same".to_owned()];
    let error = library
        .save_endpoint_profiles("1", vec![duplicate])
        .unwrap_err();
    assert_eq!(error.code, "DuplicateEndpointModel");

    let mut too_many = EndpointProfileDraft::new("Too many", "http://localhost:1234");
    too_many.manual_model_ids = (0..=MAX_ENDPOINT_MODEL_IDS)
        .map(|index| format!("model-{index}"))
        .collect();
    let error = library
        .save_endpoint_profiles("1", vec![too_many])
        .unwrap_err();
    assert_eq!(error.code, "TooManyEndpointModels");
}

#[test]
fn schema_two_migrates_to_three_without_losing_preferences() {
    let fixture = Fixture::new();
    let app = fixture.0.join("app");
    let mut library = Library::open(&app).unwrap();
    library
        .save_endpoint_profiles("0", vec![local_profile()])
        .unwrap();
    drop(library);
    let connection = Connection::open(app.join("library.sqlite3")).unwrap();
    connection
        .pragma_update(None, "user_version", 2_i64)
        .unwrap();
    drop(connection);

    let library = Library::open(&app).unwrap();
    assert_eq!(library.endpoint_profiles().unwrap().profiles.len(), 1);
    drop(library);
    let connection = Connection::open(app.join("library.sqlite3")).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 4);
}
