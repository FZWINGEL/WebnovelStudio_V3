use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::library::Library;
use webnovel_core::providers::endpoints::EndpointProfileDraft;
use webnovel_core::providers::preferences::{
    STORY_MEMORY_CODEX_PROVIDER_ID, STORY_MEMORY_MOCK_PROVIDER_ID, STORY_MEMORY_MODEL_ID,
    STORY_MEMORY_REASONING,
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-story-memory-{}", Uuid::new_v4()));
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

#[test]
fn missing_preference_keeps_codex_maintenance_default_without_changing_author_settings() {
    let fixture = Fixture::new();
    let library = Library::open(fixture.0.join("app")).unwrap();
    let story_memory = library.story_memory_settings().unwrap();
    assert_eq!(story_memory.revision, "0");
    assert_eq!(story_memory.provider_id, STORY_MEMORY_CODEX_PROVIDER_ID);
    assert_eq!(STORY_MEMORY_MODEL_ID, "gpt-6-astra");
    assert_eq!(STORY_MEMORY_REASONING, "low");
    assert_eq!(library.provider_state().unwrap().settings.revision, "0");
}

#[test]
fn endpoint_target_is_saved_with_its_own_cas_revision_and_disabled_profiles_remain_selectable() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let mut draft = EndpointProfileDraft::new("Maintenance gateway", "http://localhost:1234/v1");
    draft.enabled = false;
    draft.manual_model_ids = vec![STORY_MEMORY_MODEL_ID.to_owned()];
    library.save_endpoint_profiles("0", vec![draft]).unwrap();
    let endpoint_id = library.endpoint_profiles().unwrap().profiles[0].id.clone();

    let saved = library
        .save_story_memory_provider("0", &endpoint_id)
        .unwrap();
    assert_eq!(saved.revision, "1");
    assert_eq!(saved.provider_id, endpoint_id);
    assert_eq!(library.story_memory_settings().unwrap(), saved);
    assert_eq!(library.provider_state().unwrap().settings.revision, "0");

    let conflict = library
        .save_story_memory_provider("0", STORY_MEMORY_MOCK_PROVIDER_ID)
        .unwrap_err();
    assert_eq!(conflict.code, "PreferenceConflict");
    assert_eq!(library.story_memory_settings().unwrap(), saved);
}

#[test]
fn unknown_endpoint_is_rejected_without_a_preference_write() {
    let fixture = Fixture::new();
    let mut library = Library::open(fixture.0.join("app")).unwrap();
    let error = library
        .save_story_memory_provider(
            "0",
            "openai-compatible:00000000-0000-0000-0000-000000000099",
        )
        .unwrap_err();
    assert_eq!(error.code, "UnknownStoryMemoryProvider");
    assert_eq!(library.story_memory_settings().unwrap().revision, "0");
    assert_eq!(
        library.story_memory_settings().unwrap().provider_id,
        "codex"
    );
}
