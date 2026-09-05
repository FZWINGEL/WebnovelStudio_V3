//! Thin IPC adapters. All SQL and execution-time lease checks stay in the core.
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::State;
use webnovel_core::projects::*;

#[derive(Clone, Default)]
pub struct DesktopProjects(Arc<Mutex<HashMap<String, ProjectSession>>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
    project: ProjectInfo,
    access: ProjectAccess,
    documents: Vec<DocumentRecord>,
}
impl DesktopProjects {
    fn project(&self, id: &str) -> CoreResult<ProjectSession> {
        self.0
            .lock()
            .map_err(|_| {
                CoreError::new(
                    "PersistenceUnavailable",
                    "The project registry is unavailable.",
                )
            })?
            .get(id)
            .cloned()
            .ok_or_else(|| {
                CoreError::new("WrongProjectSession", "Open this project before using it.")
            })
    }
    fn open(
        &self,
        path: PathBuf,
        title: Option<String>,
        session: String,
    ) -> CoreResult<OpenedProject> {
        let mut registry = self.0.lock().map_err(|_| {
            CoreError::new(
                "PersistenceUnavailable",
                "The project registry is unavailable.",
            )
        })?;
        let resolved = std::fs::canonicalize(&path).ok();
        if title.is_none()
            && let Some(project) = registry
                .values()
                .find(|p| Some(&p.path) == resolved.as_ref())
        {
            let access = project.attach(session)?;
            return Ok(OpenedProject {
                project: project.info.clone(),
                documents: project.documents(access.clone())?,
                access,
            });
        }
        let project = match title {
            Some(title) => ProjectSession::create(path, &title)?,
            None => ProjectSession::open(path)?,
        };
        if registry.contains_key(&project.info.project_id) {
            return Err(CoreError::new(
                "DuplicateProjectIdentity",
                "A different folder with this project identity is already open. Recover or duplicate it with a new identity.",
            ));
        }
        let access = project.attach(session)?;
        let opened = OpenedProject {
            project: project.info.clone(),
            documents: project.documents(access.clone())?,
            access,
        };
        registry.insert(project.info.project_id.clone(), project);
        Ok(opened)
    }
}
async fn execute<T: Send + 'static>(
    work: impl FnOnce() -> CoreResult<T> + Send + 'static,
) -> CoreResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| {
            CoreError::new(
                "UncertainOutcome",
                "The desktop command stopped before its outcome could be confirmed.",
            )
        })?
}

#[tauri::command]
pub async fn create_project(
    path: String,
    title: String,
    session: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<OpenedProject> {
    let registry = state.inner().clone();
    execute(move || registry.open(PathBuf::from(path), Some(title), session)).await
}
#[tauri::command]
pub async fn open_project(
    path: String,
    session: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<OpenedProject> {
    let registry = state.inner().clone();
    execute(move || registry.open(PathBuf::from(path), None, session)).await
}
#[tauri::command]
pub async fn create_document(
    request: CreateDocument,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DocumentRecord> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.create_document(request)).await
}
#[tauri::command]
pub async fn list_documents(
    access: ProjectAccess,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Vec<DocumentRecord>> {
    let project = state.project(&access.project_id)?;
    execute(move || project.documents(access)).await
}
#[tauri::command]
pub async fn read_document(
    access: ProjectAccess,
    document_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DocumentRecord> {
    let project = state.project(&access.project_id)?;
    execute(move || project.document(access, document_id)).await
}
#[tauri::command]
pub async fn save_snapshot(
    request: SaveSnapshot,
    state: State<'_, DesktopProjects>,
) -> CoreResult<SaveAck> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.save(request)).await
}
#[tauri::command]
pub async fn reconcile_document(
    request: ReconcileRequest,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ReconciledDocument> {
    let project = state.project(&request.project_id)?;
    execute(move || project.reconcile(request)).await
}
#[tauri::command]
pub async fn checkpoint_document(
    request: CheckpointRequest,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Revision> {
    let project = state.project(&request.access.project_id)?;
    execute(move || project.checkpoint(request)).await
}
#[tauri::command]
pub async fn document_history(
    access: ProjectAccess,
    document_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Vec<Revision>> {
    let project = state.project(&access.project_id)?;
    execute(move || project.history(access, document_id)).await
}
