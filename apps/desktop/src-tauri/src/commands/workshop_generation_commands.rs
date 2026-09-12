//! Tauri boundary for one Story Workshop generation request.
//!
//! The project actor owns the session/CAS snapshot and queues the durable
//! discussion run. This adapter supplies only the native model binding and
//! dispatches that run through the existing provider/recovery workers.

use crate::app_state::AppState;
use crate::commands::project_commands::execute;
use serde::Deserialize;
use tauri::State;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::DiscussionStart;
use webnovel_core::projects::workshop_generation::{StartWorkshop, WorkshopExploration};
use webnovel_core::projects::{CoreResult, ProjectAccess};
use webnovel_core::providers::preferences::ModelSelection;

fn budget_for() -> MockContextBudget {
    MockContextBudget::new("128000", "65536", "2048")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkshopCommandRequest {
    access: ProjectAccess,
    operation_id: String,
    exploration: WorkshopExploration,
    model_selection: ModelSelection,
}

#[tauri::command]
pub async fn start_workshop(
    request: WorkshopCommandRequest, state: State<'_, AppState>,
) -> CoreResult<DiscussionStart> {
    let app = &*state;
    let state = &app.projects;
    let recovery = &app.discussion_recovery;
    let library = &app.library;
    let runtime = &app.providers;
    let WorkshopCommandRequest {
        access,
        operation_id,
        exploration,
        model_selection,
    } = request;
    let project = state.project(&access.project_id)?;
    let recovery = recovery.clone();
    let library = library.clone();
    let runtime = runtime.clone();
    let selected = model_selection;
    let start = StartWorkshop {
        access,
        operation_id,
        exploration,
        budget: budget_for(),
        provider_binding: None,
    };
    if selected.provider_id.starts_with("openai-compatible:") {
        return crate::http_discussion::start_workshop(
            start, selected, project, recovery, library, runtime,
        )
        .await;
    }
    execute(move || {
        crate::commands::discussion_commands::start_workshop_native(
            start, selected, project, recovery, library, runtime,
        )
    })
    .await
}
