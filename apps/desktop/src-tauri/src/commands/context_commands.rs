//! Context IPC carries identities, never client-asserted source authority.
use crate::app_state::AppState;
use crate::commands::project_commands::execute;
use tauri::State;
use webnovel_core::context::packet::CompiledPacket;
use webnovel_core::documents::{Endpoint, ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::context_packets::{PreparationResult, PrepareContext};
use webnovel_core::projects::story_context::{
    ContextEpochs, DocumentAliases, FreezeReviewedContinuation, FreezeStory, FrozenContext,
    SearchResult, SearchStory, SourceRead,
};
use webnovel_core::projects::{CoreResult, ProjectAccess};

#[tauri::command]
pub async fn context_epochs(
    access: ProjectAccess, state: State<'_, AppState>,
) -> CoreResult<ContextEpochs> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.context_epochs(access)).await
}

#[tauri::command]
pub async fn read_document_aliases(
    access: ProjectAccess,
    document_id: String, state: State<'_, AppState>,
) -> CoreResult<DocumentAliases> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.read_document_aliases(access, document_id)).await
}

#[tauri::command]
pub async fn set_document_aliases(
    access: ProjectAccess,
    document_id: String,
    expected_source_epoch: String,
    aliases: Vec<String>, state: State<'_, AppState>,
) -> CoreResult<ContextEpochs> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || {
        project.set_document_aliases(access, document_id, expected_source_epoch, aliases)
    })
    .await
}

#[tauri::command]
pub async fn freeze_story_context(
    request: FreezeStory, state: State<'_, AppState>,
) -> CoreResult<FrozenContext> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.freeze_story(request)).await
}

#[tauri::command]
pub async fn freeze_reviewed_continuation(
    request: FreezeReviewedContinuation, state: State<'_, AppState>,
) -> CoreResult<FrozenContext> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.freeze_reviewed_continuation(request)).await
}

#[tauri::command]
pub async fn story_context_snapshot(
    access: ProjectAccess,
    snapshot_id: String, state: State<'_, AppState>,
) -> CoreResult<FrozenContext> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.story_snapshot(access, snapshot_id)).await
}

#[tauri::command]
pub async fn read_story_context_source(
    access: ProjectAccess,
    snapshot_id: String,
    handle: String, state: State<'_, AppState>,
) -> CoreResult<SourceRead> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.read_story_source(access, snapshot_id, handle)).await
}

#[tauri::command]
pub async fn search_story_context(
    request: SearchStory, state: State<'_, AppState>,
) -> CoreResult<SearchResult> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.search_story(request)).await
}

#[tauri::command]
pub async fn prepare_story_context(
    request: PrepareContext, state: State<'_, AppState>,
) -> CoreResult<PreparationResult> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.prepare_context(request)).await
}

#[tauri::command]
pub async fn prepared_story_context(
    access: ProjectAccess,
    packet_id: String, state: State<'_, AppState>,
) -> CoreResult<CompiledPacket> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.prepared_context(access, packet_id)).await
}

#[tauri::command]
pub async fn prepared_story_context_is_current(
    access: ProjectAccess,
    packet_id: String, state: State<'_, AppState>,
) -> CoreResult<bool> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.prepared_context_is_current(access, packet_id)).await
}

#[tauri::command]
pub async fn revoke_story_context(
    access: ProjectAccess,
    expected_policy: String, state: State<'_, AppState>,
) -> CoreResult<ContextEpochs> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.revoke_story_context(access, expected_policy)).await
}

#[tauri::command]
pub async fn rebuild_story_index(
    access: ProjectAccess, state: State<'_, AppState>,
) -> CoreResult<u32> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.rebuild_story_index(access)).await
}

#[tauri::command]
pub async fn capture_story_scope(
    access: ProjectAccess,
    snapshot_id: String,
    kind: ScopeKind,
    start: Option<Endpoint>,
    end: Option<Endpoint>, state: State<'_, AppState>,
) -> CoreResult<ScopeGrant> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || {
        let frozen = project.story_snapshot(access.clone(), snapshot_id.clone())?;
        let source = frozen
            .snapshot
            .sources
            .iter()
            .find(|source| source.source == frozen.snapshot.target)
            .ok_or_else(|| {
                webnovel_core::projects::CoreError::new(
                    "InvalidContext",
                    "The request target is missing.",
                )
            })?;
        let read = project.read_story_source(access, snapshot_id, source.handle.clone())?;
        capture_scope(
            &read.body,
            ScopeGrant {
                kind,
                start,
                end,
                source_hash: String::new(),
                quote: String::new(),
                quote_hash: String::new(),
                prefix: None,
                suffix: None,
            },
        )
        .map_err(|error| webnovel_core::projects::CoreError::new("InvalidScope", &error))
    })
    .await
}
