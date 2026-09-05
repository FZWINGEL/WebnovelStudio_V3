//! Thin IPC adapters. All SQL and execution-time lease checks stay in the core.
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::State;
use webnovel_core::documents::scope::Endpoint;
use webnovel_core::projects::*;

#[derive(Clone, Default)]
pub struct DesktopProjects(Arc<Mutex<HashMap<String, ProjectSession>>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
    pub project: ProjectInfo,
    pub access: ProjectAccess,
    pub documents: Vec<DocumentRecord>,
    pub library_warning: Option<String>,
    pub metadata_version: String,
    pub view_state: Option<ViewState>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadataResult {
    #[serde(flatten)]
    metadata: ProjectMetadata,
    library_warning: Option<String>,
}
impl DesktopProjects {
    pub(super) fn project(&self, id: &str) -> CoreResult<ProjectSession> {
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
    pub(super) fn open(
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
            let attached = project.attach_snapshot(session)?;
            let metadata = attached.metadata;
            return Ok(OpenedProject {
                project: metadata.project,
                metadata_version: metadata.metadata_version,
                view_state: attached.view_state,
                documents: attached.documents,
                access: attached.access,
                library_warning: None,
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
        let attached = project.attach_snapshot(session)?;
        let metadata = attached.metadata;
        let opened = OpenedProject {
            project: metadata.project,
            metadata_version: metadata.metadata_version,
            view_state: attached.view_state,
            documents: attached.documents,
            access: attached.access,
            library_warning: None,
        };
        registry.insert(project.info.project_id.clone(), project);
        Ok(opened)
    }
    pub(super) fn insert(
        &self,
        project: ProjectSession,
        session: String,
    ) -> CoreResult<OpenedProject> {
        let mut registry = self.0.lock().map_err(|_| {
            CoreError::new(
                "PersistenceUnavailable",
                "The open project list is unavailable.",
            )
        })?;
        if registry.contains_key(&project.info.project_id) {
            return Err(CoreError::new(
                "DuplicateProjectIdentity",
                "This project identity is already open.",
            ));
        }
        let attached = project.attach_snapshot(session)?;
        let metadata = attached.metadata;
        let opened = OpenedProject {
            project: metadata.project,
            metadata_version: metadata.metadata_version,
            view_state: attached.view_state,
            documents: attached.documents,
            access: attached.access,
            library_warning: None,
        };
        registry.insert(project.info.project_id.clone(), project);
        Ok(opened)
    }
    pub(super) fn close(&self, id: &str) -> CoreResult<()> {
        self.0
            .lock()
            .map_err(|_| {
                CoreError::new(
                    "PersistenceUnavailable",
                    "The open project list is unavailable.",
                )
            })?
            .remove(id);
        Ok(())
    }
}
pub(super) async fn execute<T: Send + 'static>(
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

#[tauri::command]
pub async fn project_metadata(
    project_id: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ProjectMetadata> {
    let project = state.project(&project_id)?;
    execute(move || project.project_metadata()).await
}
#[tauri::command]
pub async fn rename_project(
    access: ProjectAccess,
    expected_metadata_version: String,
    title: String,
    state: State<'_, DesktopProjects>,
    library: State<'_, crate::library_commands::DesktopLibrary>,
) -> CoreResult<ProjectMetadataResult> {
    let project = state.project(&access.project_id)?;
    let library = library.inner().clone();
    execute(move || {
        let metadata = project.rename_project(access, expected_metadata_version, title)?;
        let library_warning = match library.0.lock() {
            Ok(mut library) => library.register(&project).err().map(|e| e.detail),
            Err(_) => {
                Some("The project title was saved, but the library index is unavailable.".into())
            }
        };
        Ok(ProjectMetadataResult {
            metadata,
            library_warning,
        })
    })
    .await
}
#[tauri::command]
pub async fn rename_document(
    access: ProjectAccess,
    document_id: String,
    expected_metadata_version: String,
    title: String,
    state: State<'_, DesktopProjects>,
) -> CoreResult<DocumentRecord> {
    let project = state.project(&access.project_id)?;
    execute(move || project.rename_document(access, document_id, expected_metadata_version, title))
        .await
}
#[tauri::command]
pub async fn read_view_state(
    access: ProjectAccess,
    state: State<'_, DesktopProjects>,
) -> CoreResult<Option<ViewState>> {
    let project = state.project(&access.project_id)?;
    execute(move || project.view_state(access)).await
}
#[tauri::command]
pub async fn save_view_state(
    access: ProjectAccess,
    head: Head,
    anchor: Endpoint,
    focus: Endpoint,
    state: State<'_, DesktopProjects>,
) -> CoreResult<ViewState> {
    let project = state.project(&access.project_id)?;
    execute(move || project.save_view_state(access, head, anchor, focus)).await
}
