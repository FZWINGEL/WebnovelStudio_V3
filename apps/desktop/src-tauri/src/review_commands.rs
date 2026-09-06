//! Explicit author review. The core owns source validation and activation.
use crate::project_commands::{DesktopProjects, execute};
use tauri::State;
use webnovel_core::projects::reviewed_story::{
    MarkReady, ReadyBundle, ReviewStage, ReviewStatus, ReviewedRecordSet, StageAuthorReview,
};
use webnovel_core::projects::{CoreResult, ProjectAccess};

#[tauri::command]
pub async fn chapter_review_status(
    access: ProjectAccess,
    document_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewStatus> {
    let project = state.project(&access.project_id)?;
    execute(move || project.chapter_review_status(access, document_id)).await
}

#[tauri::command]
pub async fn read_reviewed_record_set(
    access: ProjectAccess,
    document_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Option<ReviewedRecordSet>> {
    let project = state.project(&access.project_id)?;
    execute(move || project.read_reviewed_record_set(access, document_id)).await
}

#[tauri::command]
pub async fn stage_author_review(
    request: StageAuthorReview,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewStage> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.stage_author_review(request)).await
}

#[tauri::command]
pub async fn read_review_stage(
    access: ProjectAccess,
    stage_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewStage> {
    let project = state.project(&access.project_id)?;
    execute(move || project.read_review_stage(access, stage_id)).await
}

#[tauri::command]
pub async fn mark_ready(
    request: MarkReady,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReadyBundle> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.mark_ready(request)).await
}
