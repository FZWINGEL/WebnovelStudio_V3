//! AuthorRoom persistent source-pin commands.
use crate::app_state::AppState;
use crate::project_commands::execute;
use tauri::State;
use webnovel_core::projects::source_pins::{SaveSourcePins, SourcePinSet, SourcePinsView};
use webnovel_core::projects::{CoreResult, ProjectAccess};

#[tauri::command]
pub async fn read_source_pins(
    access: ProjectAccess,
    document_id: String, state: State<'_, AppState>,
) -> CoreResult<SourcePinsView> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.read_source_pins(access, document_id)).await
}

#[tauri::command]
pub async fn save_source_pins(
    request: SaveSourcePins, state: State<'_, AppState>,
) -> CoreResult<SourcePinSet> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.save_source_pins(request)).await
}
