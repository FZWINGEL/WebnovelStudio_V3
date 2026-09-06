//! Explicit author review. The core owns source validation and activation.
use crate::project_commands::{DesktopProjects, execute};
use tauri::State;
use webnovel_core::projects::evidence_queries::{
    ReviewedEntityCatalog, ReviewedHistoryResult, ReviewedKnowledgeHistoryResult,
    ReviewedPromiseHistoryResult,
};
use webnovel_core::projects::reviewed_story::{
    MarkReady, ReadyBundle, ReviewStage, ReviewStatus, ReviewedRecordSet, StageAuthorReview,
};
use webnovel_core::projects::{CoreResult, ProjectAccess};

#[tauri::command]
pub async fn reviewed_entity_catalog(
    access: ProjectAccess,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewedEntityCatalog> {
    let project = state.project(&access.project_id)?;
    execute(move || project.reviewed_entity_catalog(access)).await
}

#[tauri::command]
pub async fn reviewed_evidence_history(
    access: ProjectAccess,
    snapshot_id: String,
    object_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewedHistoryResult> {
    let project = state.project(&access.project_id)?;
    execute(move || project.reviewed_evidence_history(access, snapshot_id, object_id)).await
}

#[tauri::command]
pub async fn reviewed_promise_catalog(
    access: ProjectAccess,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewedEntityCatalog> {
    let project = state.project(&access.project_id)?;
    execute(move || project.reviewed_promise_catalog(access)).await
}

#[tauri::command]
pub async fn reviewed_promise_history(
    access: ProjectAccess,
    snapshot_id: String,
    promise_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewedPromiseHistoryResult> {
    let project = state.project(&access.project_id)?;
    execute(move || project.reviewed_promise_history(access, snapshot_id, promise_id)).await
}

#[tauri::command]
pub async fn reviewed_knowledge_character_catalog(
    access: ProjectAccess,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewedEntityCatalog> {
    let project = state.project(&access.project_id)?;
    execute(move || project.reviewed_knowledge_character_catalog(access)).await
}

#[tauri::command]
pub async fn reviewed_knowledge_topic_catalog(
    access: ProjectAccess,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewedEntityCatalog> {
    let project = state.project(&access.project_id)?;
    execute(move || project.reviewed_knowledge_topic_catalog(access)).await
}

#[tauri::command]
pub async fn reviewed_knowledge_history(
    access: ProjectAccess,
    snapshot_id: String,
    character_id: String,
    topic_id: Option<String>,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReviewedKnowledgeHistoryResult> {
    let project = state.project(&access.project_id)?;
    execute(move || project.reviewed_knowledge_history(access, snapshot_id, character_id, topic_id))
        .await
}

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
