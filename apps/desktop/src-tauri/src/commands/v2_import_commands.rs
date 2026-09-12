use crate::app_state::AppState;
use crate::commands::project_commands::{OpenedProject, execute};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;
use webnovel_core::projects::import::V2ImportRequest;
use webnovel_core::projects::{CoreError, CoreResult};
use webnovel_core::v2_import::{
    V2ImportPreview, V2ProjectSummary, list_v2_projects, preview_v2_import,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct V2ImportProjectList {
    pub source_path: String,
    pub projects: Vec<V2ProjectSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct V2ImportPreviewResult {
    pub source_path: String,
    pub preview: V2ImportPreview,
}

fn choose_source(path: Option<String>) -> CoreResult<Option<PathBuf>> {
    if let Some(path) = path {
        if path.trim().is_empty() {
            return Err(CoreError::new(
                "InvalidRequest",
                "Choose a V2 database file.",
            ));
        }
        return Ok(Some(PathBuf::from(path)));
    }
    Ok(rfd::FileDialog::new()
        .set_title("Choose a WebnovelStudio V2 database")
        .add_filter("SQLite database", &["db", "sqlite3", "sqlite"])
        .pick_file())
}

#[tauri::command]
pub async fn v2_import_list_projects(
    path: Option<String>,
) -> CoreResult<Option<V2ImportProjectList>> {
    execute(move || {
        let Some(path) = choose_source(path)? else {
            return Ok(None);
        };
        let projects = list_v2_projects(&path)?;
        Ok(Some(V2ImportProjectList {
            source_path: path.to_string_lossy().into_owned(),
            projects,
        }))
    })
    .await
}

#[tauri::command]
pub async fn v2_import_preview(
    path: String,
    source_project_id: String,
) -> CoreResult<V2ImportPreviewResult> {
    execute(move || {
        let path = PathBuf::from(path);
        let preview = preview_v2_import(&path, &source_project_id)?;
        Ok(V2ImportPreviewResult {
            source_path: path.to_string_lossy().into_owned(),
            preview,
        })
    })
    .await
}

#[tauri::command]
pub async fn v2_import(
    request: V2ImportRequest,
    session: String, state: State<'_, AppState>,
) -> CoreResult<OpenedProject> {
    let app = &*state;
    let library_state = &app.library;
    let projects = &app.projects;
    let library_state = library_state.clone();
    let projects = projects.clone();
    execute(move || {
        let mut library = library_state.0.lock().map_err(|_| {
            CoreError::new(
                "LibraryUnavailable",
                "The library index is unavailable. Project folders still contain your writing.",
            )
        })?;
        library.import_v2(request.clone())?;
        let pending = library.operation(&request.operation_id)?.ok_or_else(|| {
            CoreError::new(
                "PersistenceUnavailable",
                "The completed import operation is missing from the library index.",
            )
        })?;
        pending.require_available()?;
        let final_path = pending.final_path;
        drop(library);
        let project = projects.open(final_path, None, session)?;
        Ok(project)
    })
    .await
}
