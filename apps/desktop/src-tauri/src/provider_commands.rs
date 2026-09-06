use crate::library_commands::DesktopLibrary;
use crate::project_commands::execute;
use crate::provider_runtime::{DesktopProviderState, DesktopProviders};
use tauri::State;
use webnovel_core::projects::{CoreError, CoreResult};
use webnovel_core::providers::preferences::{ModelKey, ModelSelection};

pub fn unavailable() -> CoreError {
    CoreError::new(
        "ModelSettingsUnavailable",
        "Model settings could not be checked. Open Settings before sending another request.",
    )
}

#[tauri::command]
pub async fn provider_state(
    state: State<'_, DesktopLibrary>,
    runtime: State<'_, DesktopProviders>,
) -> CoreResult<DesktopProviderState> {
    let state = state.inner().clone();
    let runtime = runtime.inner().clone();
    execute(move || runtime.view_library(&*state.0.lock().map_err(|_| unavailable())?)).await
}

#[tauri::command]
pub async fn check_codex_connection(
    state: State<'_, DesktopLibrary>,
    runtime: State<'_, DesktopProviders>,
) -> CoreResult<DesktopProviderState> {
    let state = state.inner().clone();
    let runtime = runtime.inner().clone();
    execute(move || {
        runtime.check_connection()?;
        runtime.view_library(&*state.0.lock().map_err(|_| unavailable())?)
    })
    .await
}

#[tauri::command]
pub async fn save_model_settings(
    expected_revision: String,
    active: ModelSelection,
    favorites: Vec<ModelKey>,
    state: State<'_, DesktopLibrary>,
    runtime: State<'_, DesktopProviders>,
) -> CoreResult<DesktopProviderState> {
    let state = state.inner().clone();
    let runtime = runtime.inner().clone();
    execute(move || {
        let mut library = state.0.lock().map_err(|_| unavailable())?;
        library.save_model_settings(&expected_revision, active, favorites)?;
        runtime.view_library(&library)
    })
    .await
}
