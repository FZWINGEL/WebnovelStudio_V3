use crate::app_state::AppState;
use crate::project_commands::execute;
use crate::provider_runtime::DesktopProviderState;
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
pub async fn provider_state( state: State<'_, AppState>,
) -> CoreResult<DesktopProviderState> {
    let app = &*state;
    let state = &app.library;
    let runtime = &app.providers;
    let state = state.clone();
    let runtime = runtime.clone();
    execute(move || runtime.view_library(&*state.0.lock().map_err(|_| unavailable())?)).await
}

#[tauri::command]
pub async fn check_codex_connection( state: State<'_, AppState>,
) -> CoreResult<DesktopProviderState> {
    let app = &*state;
    let state = &app.library;
    let runtime = &app.providers;
    let state = state.clone();
    let runtime = runtime.clone();
    execute(move || {
        #[cfg(windows)]
        let (transport, selection) = {
            let library = state.0.lock().map_err(|_| unavailable())?;
            (
                library.codex_transport_settings()?.transport,
                library.provider_state()?.settings.active,
            )
        };
        #[cfg(windows)]
        runtime.check_selected_connection(transport, &selection, |connection| {
            let mut library = state.0.lock().map_err(|_| unavailable())?;
            library.save_codex_catalog(connection.catalog().clone())
        })?;
        #[cfg(not(windows))]
        runtime.check_connection()?;
        let library = state.0.lock().map_err(|_| unavailable())?;
        runtime.view_library(&library)
    })
    .await
}

#[tauri::command]
pub async fn save_model_settings(
    expected_revision: String,
    active: ModelSelection,
    favorites: Vec<ModelKey>, state: State<'_, AppState>,
) -> CoreResult<DesktopProviderState> {
    let app = &*state;
    let state = &app.library;
    let runtime = &app.providers;
    let state = state.clone();
    let runtime = runtime.clone();
    execute(move || {
        let mut library = state.0.lock().map_err(|_| unavailable())?;
        library.save_model_settings(&expected_revision, active, favorites)?;
        runtime.view_library(&library)
    })
    .await
}

#[tauri::command]
pub async fn check_claude_connection( state: State<'_, AppState>,
) -> CoreResult<DesktopProviderState> {
    let app = &*state;
    let state = &app.library;
    let runtime = &app.providers;
    let state = state.clone();
    let runtime = runtime.clone();
    execute(move || {
        runtime.check_claude_connection()?;
        runtime.view_library(&*state.0.lock().map_err(|_| unavailable())?)
    })
    .await
}

#[tauri::command]
pub async fn save_story_memory_provider(
    expected_revision: String,
    provider_id: String, state: State<'_, AppState>,
) -> CoreResult<DesktopProviderState> {
    let app = &*state;
    let state = &app.library;
    let runtime = &app.providers;
    let state = state.clone();
    let runtime = runtime.clone();
    execute(move || {
        let mut library = state.0.lock().map_err(|_| unavailable())?;
        library.save_story_memory_provider(&expected_revision, &provider_id)?;
        runtime.view_library(&library)
    })
    .await
}
