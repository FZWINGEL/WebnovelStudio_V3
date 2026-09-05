use crate::library_commands::DesktopLibrary;
use crate::project_commands::execute;
use tauri::State;
use webnovel_core::projects::{CoreError, CoreResult};
use webnovel_core::providers::{
    catalog::ProviderState,
    preferences::{ModelKey, ModelSelection},
};

pub fn unavailable() -> CoreError {
    CoreError::new(
        "ModelSettingsUnavailable",
        "Model settings could not be checked. Open Settings before sending another request.",
    )
}

#[tauri::command]
pub async fn provider_state(state: State<'_, DesktopLibrary>) -> CoreResult<ProviderState> {
    let state = state.inner().clone();
    execute(move || state.0.lock().map_err(|_| unavailable())?.provider_state()).await
}

#[tauri::command]
pub async fn save_model_settings(
    expected_revision: String,
    active: ModelSelection,
    favorites: Vec<ModelKey>,
    state: State<'_, DesktopLibrary>,
) -> CoreResult<ProviderState> {
    let state = state.inner().clone();
    execute(move || {
        state
            .0
            .lock()
            .map_err(|_| unavailable())?
            .save_model_settings(&expected_revision, active, favorites)
    })
    .await
}
