use rusqlite::Connection;
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::library::Library;
use webnovel_core::providers::catalog::{CatalogOrigin, DispatchResolution};
use webnovel_core::providers::preferences::{ModelKey, ModelSelection};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-model-settings-{}", Uuid::new_v4()));
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
                    .starts_with("wns-model-settings-")
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn luna() -> ModelSelection {
    ModelSelection {
        provider_id: "codex".into(),
        model_id: "gpt-5.6-luna".into(),
        reasoning: Some("max".into()),
        service_tier: Some("priority".into()),
    }
}

#[test]
fn fresh_state_is_explicit_mock_and_reference_catalog_is_bounded() {
    let fixture = Fixture::new();
    let library = Library::open(fixture.0.join("app")).unwrap();
    let state = library.provider_state().unwrap();

    assert_eq!(state.settings.revision, "0");
    assert_eq!(state.settings.active.provider_id, "mock");
    assert_eq!(state.settings.active.model_id, "mock-story-context");
    assert!(state.settings.active.reasoning.is_none());
    assert!(state.settings.active.service_tier.is_none());
    assert!(matches!(
        state.dispatch,
        DispatchResolution::LocalMock { .. }
    ));
    assert_eq!(state.catalog.models.len(), 8);
    assert!(
        state
            .catalog
            .models
            .iter()
            .filter(|model| model.key.provider_id == "codex")
            .all(|model| model.origin == CatalogOrigin::Reference && !model.ready)
    );
    assert!(state
        .catalog
        .models
        .iter()
        .all(|model| model.context_window_tokens.is_none() && model.max_output_tokens.is_none()));
    let luna = state
        .catalog
        .models
        .iter()
        .find(|model| model.key.model_id == "gpt-5.6-luna")
        .unwrap();
    assert_eq!(
        luna.reasoning_levels,
        vec!["low", "medium", "high", "xhigh", "max"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );
    assert_eq!(luna.service_tiers[0].id, "priority");
}

#[test]
fn model_selection_keeps_optional_traits_as_explicit_json_nulls() {
    let json = serde_json::to_value(ModelSelection::local_mock()).unwrap();
    assert_eq!(json["reasoning"], Value::Null);
    assert_eq!(json["serviceTier"], Value::Null);
    let parsed: ModelSelection = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, ModelSelection::local_mock());
}

#[test]
fn selection_traits_are_validated_without_claiming_provider_readiness() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let error = library
        .save_model_settings(
            "0",
            ModelSelection {
                provider_id: "codex".into(),
                model_id: "gpt-5.5".into(),
                reasoning: Some("max".into()),
                service_tier: None,
            },
            Vec::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, "UnsupportedModelTrait");

    let error = library
        .save_model_settings(
            "0",
            ModelSelection {
                provider_id: "codex".into(),
                model_id: "gpt-5.6-luna".into(),
                reasoning: Some("max".into()),
                service_tier: Some("unknown".into()),
            },
            Vec::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, "UnsupportedModelTrait");

    let error = library
        .save_model_settings(
            "0",
            ModelSelection {
                provider_id: "codex".into(),
                model_id: "not-in-catalog".into(),
                reasoning: None,
                service_tier: None,
            },
            Vec::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, "UnknownModel");
}

#[test]
fn settings_round_trip_and_stale_ack_are_safe() {
    let fixture = Fixture::new();
    let app = fixture.0.join("app");
    let mut library = Library::open(&app).unwrap();
    let saved = library
        .save_model_settings(
            "0",
            luna(),
            vec![ModelKey::new("mock", "mock-story-context")],
        )
        .unwrap();
    assert_eq!(saved.settings.revision, "1");
    assert!(matches!(saved.dispatch, DispatchResolution::Blocked { .. }));
    assert_eq!(saved.settings.favorites.len(), 1);
    drop(library);

    let library = Library::open(&app).unwrap();
    let reread = library.provider_state().unwrap();
    assert_eq!(reread.settings, saved.settings);
    drop(library);

    let mut library = Library::open(&app).unwrap();
    let error = library
        .save_model_settings("0", ModelSelection::local_mock(), Vec::new())
        .unwrap_err();
    assert_eq!(error.code, "PreferenceConflict");
    let restored = library
        .save_model_settings("1", ModelSelection::local_mock(), Vec::new())
        .unwrap();
    assert_eq!(restored.settings.revision, "2");
    assert!(matches!(
        restored.dispatch,
        DispatchResolution::LocalMock { .. }
    ));
}

#[test]
fn duplicate_favorites_are_rejected() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let error = library
        .save_model_settings(
            "0",
            ModelSelection::local_mock(),
            vec![
                ModelKey::new("mock", "mock-story-context"),
                ModelKey::new("mock", "mock-story-context"),
            ],
        )
        .unwrap_err();
    assert_eq!(error.code, "DuplicateFavorite");
}

#[test]
fn signed_or_noncanonical_revisions_are_rejected() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    for revision in ["+0", "-0", "01"] {
        let error = library
            .save_model_settings(revision, ModelSelection::local_mock(), Vec::new())
            .unwrap_err();
        assert_eq!(error.code, "InvalidPreferenceRevision", "{revision}");
    }
}

#[test]
fn library_v1_migrates_without_touching_an_author_database_and_future_versions_refuse() {
    let fixture = Fixture::new();
    let app = fixture.0.join("app");
    std::fs::create_dir_all(&app).unwrap();
    let connection = Connection::open(app.join("library.sqlite3")).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE identity (namespace TEXT NOT NULL) STRICT;
             CREATE TABLE entries (project_id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL, path TEXT NOT NULL UNIQUE,
                 archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN(0,1)), last_opened TEXT NOT NULL) STRICT;
             CREATE TABLE operations (operation_id TEXT PRIMARY KEY NOT NULL, kind TEXT NOT NULL, title TEXT NOT NULL,
                 staging_path TEXT NOT NULL UNIQUE, final_path TEXT NOT NULL UNIQUE, source_path TEXT, source_fingerprint TEXT,
                 completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN(0,1))) STRICT;
             INSERT INTO identity(namespace) VALUES('old-library');
             PRAGMA user_version=1;",
        )
        .unwrap();
    drop(connection);
    let author_db = fixture.0.join("author-project").join("project.sqlite3");
    std::fs::create_dir_all(author_db.parent().unwrap()).unwrap();
    std::fs::write(&author_db, b"author database must remain untouched").unwrap();
    let before = std::fs::read(&author_db).unwrap();

    let library = Library::open(&app).unwrap();
    assert_eq!(library.provider_state().unwrap().settings.revision, "0");
    drop(library);
    assert_eq!(std::fs::read(&author_db).unwrap(), before);
    let connection = Connection::open(app.join("library.sqlite3")).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 3);
    let table: String = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='app_preferences'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table, "app_preferences");
    drop(connection);

    let connection = Connection::open(app.join("library.sqlite3")).unwrap();
    connection
        .pragma_update(None, "user_version", 99_i64)
        .unwrap();
    drop(connection);
    let error = match Library::open(&app) {
        Ok(_) => panic!("future library schema unexpectedly opened"),
        Err(error) => error,
    };
    assert_eq!(error.code, "UnsupportedSchema");
}
