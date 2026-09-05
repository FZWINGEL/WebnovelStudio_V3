use crate::project_commands::{DesktopProjects, execute};
use tauri::State;
use webnovel_core::context::guidance::GuidanceVersion;
use webnovel_core::projects::guidance::SaveGuidance;
use webnovel_core::projects::{CoreResult, ProjectAccess};

#[tauri::command]
pub async fn read_guidance(
    access: ProjectAccess,
    document_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Vec<GuidanceVersion>> {
    let project = state.project(&access.project_id)?;
    execute(move || project.guidance(access, document_id)).await
}

#[tauri::command]
pub async fn save_guidance(
    request: SaveGuidance,
    state: State<'_, DesktopProjects>,
) -> CoreResult<GuidanceVersion> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.save_guidance(request)).await
}
