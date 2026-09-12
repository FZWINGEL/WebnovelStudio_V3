use crate::app_state::AppState;
use crate::project_commands::execute;
use tauri::State;
use webnovel_core::context::guidance::GuidanceVersion;
use webnovel_core::projects::guidance::SaveGuidance;
use webnovel_core::projects::{CoreResult, ProjectAccess};

#[tauri::command]
pub async fn read_guidance(
    access: ProjectAccess,
    document_id: String, state: State<'_, AppState>,
) -> CoreResult<Vec<GuidanceVersion>> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.guidance(access, document_id)).await
}

#[tauri::command]
pub async fn save_guidance(
    request: SaveGuidance, state: State<'_, AppState>,
) -> CoreResult<GuidanceVersion> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.save_guidance(request)).await
}
