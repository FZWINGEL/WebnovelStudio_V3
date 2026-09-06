use std::path::PathBuf;

use serde_json::json;
use uuid::Uuid;
use webnovel_core::library::Library;
use webnovel_core::providers::catalog::CatalogOrigin;
use webnovel_core::providers::codex_catalog::{CodexCatalog, CodexCatalogModel, parse_model_page};
use webnovel_core::providers::preferences::ModelSelection;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-codex-catalog-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.0.starts_with(std::env::temp_dir()));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn catalog(model_id: &str, reasoning: &[&str]) -> CodexCatalog {
    CodexCatalog {
        cli_version: "1.2.3".into(),
        executable_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .into(),
        discovered_at: "1700000000000".into(),
        models: vec![CodexCatalogModel {
            model_id: model_id.into(),
            label: model_id.into(),
            reasoning_levels: reasoning.iter().map(|value| (*value).into()).collect(),
            default_reasoning: reasoning.first().map(|value| (*value).into()),
            service_tiers: Vec::new(),
            default_service_tier: None,
        }],
    }
}

#[test]
fn parser_ignores_unknown_and_hidden_rows_but_keeps_declared_defaults() {
    let page = json!({
        "data": [
            {"id": "hidden", "hidden": true},
            {"model": "writer-v3", "displayName": "Writer V3", "supportedReasoningEfforts": ["high"], "defaultReasoningEffort": "high", "unknown": true}
        ],
        "nextCursor": "cursor"
    });
    let parsed = parse_model_page(page.to_string().as_bytes()).unwrap();
    assert_eq!(parsed.next_cursor.as_deref(), Some("cursor"));
    assert_eq!(parsed.models[0].model_id, "writer-v3");
    assert_eq!(parsed.models[0].default_reasoning.as_deref(), Some("high"));
}

#[test]
fn malformed_page_and_invalid_cache_are_rejected_without_replacing_good_cache() {
    assert!(parse_model_page(br#"{"data":"not-an-array"}"#).is_err());
    let fixture = Fixture::new();
    let app = fixture.0.join("app");
    let mut library = Library::open(&app).unwrap();
    let good = catalog("writer-v3", &["high"]);
    library.save_codex_catalog(good.clone()).unwrap();
    let mut invalid = catalog("writer-v3", &["high"]);
    invalid.models.push(invalid.models[0].clone());
    assert!(library.save_codex_catalog(invalid).is_err());
    assert_eq!(library.codex_catalog().unwrap(), Some(good));
}

#[test]
fn discovered_catalog_reopens_and_removed_active_traits_remain_readable_but_blocked() {
    let fixture = Fixture::new();
    let app = fixture.0.join("app");
    let mut library = Library::open(&app).unwrap();
    library
        .save_codex_catalog(catalog("writer-v3", &["high"]))
        .unwrap();
    let saved = library
        .save_model_settings(
            "0",
            ModelSelection {
                provider_id: "codex".into(),
                model_id: "writer-v3".into(),
                reasoning: Some("high".into()),
                service_tier: None,
            },
            Vec::new(),
        )
        .unwrap();
    assert_eq!(saved.settings.revision, "1");
    assert_eq!(
        saved
            .catalog
            .models
            .iter()
            .find(|model| model.key.model_id == "writer-v3")
            .unwrap()
            .origin,
        CatalogOrigin::CodexDiscovery
    );
    drop(library);

    let mut library = Library::open(&app).unwrap();
    library
        .save_codex_catalog(catalog("writer-v3", &["low"]))
        .unwrap();
    let state = library.provider_state().unwrap();
    assert_eq!(state.settings.active.reasoning.as_deref(), Some("high"));
    let model = state
        .catalog
        .models
        .iter()
        .find(|model| model.key.model_id == "writer-v3")
        .unwrap();
    assert!(!model.ready);
    assert!(model.status_detail.contains("selection was preserved"));

    let error = library
        .save_model_settings(
            "1",
            ModelSelection {
                provider_id: "codex".into(),
                model_id: "writer-v3".into(),
                reasoning: Some("max".into()),
                service_tier: None,
            },
            Vec::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, "UnsupportedModelTrait");
}

#[test]
fn removed_builtin_codex_model_falls_back_to_reference_and_preserves_selection() {
    let fixture = Fixture::new();
    let app = fixture.0.join("app");
    let mut library = Library::open(&app).unwrap();

    // Astra is present in the static reference catalog as well as discovery.
    // Save an explicitly declared trait while discovery still reports it.
    library
        .save_codex_catalog(catalog("gpt-6-astra", &["ultra"]))
        .unwrap();
    library
        .save_model_settings(
            "0",
            ModelSelection {
                provider_id: "codex".into(),
                model_id: "gpt-6-astra".into(),
                reasoning: Some("ultra".into()),
                service_tier: None,
            },
            Vec::new(),
        )
        .unwrap();

    // A later complete discovery omits Astra.  The static reference row is
    // retained, so reopening must preserve the prior choice even though its
    // current row no longer declares the saved trait.
    library
        .save_codex_catalog(catalog("gpt-5.4-mini", &["low"]))
        .unwrap();
    let state = library.provider_state().unwrap();
    assert_eq!(state.settings.active.model_id, "gpt-6-astra");
    assert_eq!(state.settings.active.reasoning.as_deref(), Some("ultra"));
    let row = state
        .catalog
        .models
        .iter()
        .find(|model| model.key.model_id == "gpt-6-astra")
        .unwrap();
    assert_eq!(row.origin, CatalogOrigin::Reference);
    assert!(!row.reasoning_levels.contains(&"ultra".to_owned()));

    // Preservation only applies to the unchanged stored active selection;
    // a newly requested unsupported trait remains rejected.
    let error = library
        .save_model_settings(
            "1",
            ModelSelection {
                provider_id: "codex".into(),
                model_id: "gpt-6-astra".into(),
                reasoning: Some("max".into()),
                service_tier: None,
            },
            Vec::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, "UnsupportedModelTrait");
}
