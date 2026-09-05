use crate::project_commands::{DesktopProjects, OpenedProject, execute};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::State;
use webnovel_core::{
    library::{Library, LibraryEntry, PendingProject},
    projects::*,
    transfer,
};

#[derive(Clone)]
pub struct DesktopLibrary(pub Arc<Mutex<Library>>);
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    entries: Vec<LibraryEntry>,
    pending: Vec<PendingProject>,
}
fn lock_error() -> CoreError {
    CoreError::new(
        "LibraryUnavailable",
        "The library index is unavailable. Project folders still contain your writing.",
    )
}

#[tauri::command]
pub async fn library_snapshot(state: State<'_, DesktopLibrary>) -> CoreResult<LibrarySnapshot> {
    let state = state.inner().clone();
    execute(move || {
        let library = state.0.lock().map_err(|_| lock_error())?;
        Ok(LibrarySnapshot {
            entries: library.list()?,
            pending: library.pending()?,
        })
    })
    .await
}
#[tauri::command]
pub async fn library_create(
    operation_id: String,
    title: String,
    session: String,
    state: State<'_, DesktopLibrary>,
    projects: State<'_, DesktopProjects>,
) -> CoreResult<OpenedProject> {
    let state = state.inner().clone();
    let projects = projects.inner().clone();
    execute(move || {
        let mut library = state.0.lock().map_err(|_| lock_error())?;
        let pending = library.begin(&operation_id, "create", &title, None)?;
        // An installed folder is reusable after either a renderer or registry
        // acknowledgment was lost; the operation never allocates another copy.
        if pending.final_path.exists()
            && read_creation_origin(&pending.final_path)? == pending.origin
        {
            let mut opened = projects.open(pending.final_path, None, session)?;
            if let Err(error) = library.finish(
                &operation_id,
                &projects.project(&opened.project.project_id)?,
            ) {
                opened.library_warning = Some(error.detail);
            }
            return Ok(opened);
        }
        let project = ProjectSession::create_staged(
            &pending.staging_path,
            &pending.final_path,
            &title,
            &pending.origin,
        )?;
        let warning = library
            .finish(&operation_id, &project)
            .err()
            .map(|e| e.detail);
        let mut opened = projects.insert(project, session)?;
        opened.library_warning = warning;
        Ok(opened)
    })
    .await
}
#[tauri::command]
pub async fn library_open(
    path: Option<String>,
    session: String,
    state: State<'_, DesktopLibrary>,
    projects: State<'_, DesktopProjects>,
) -> CoreResult<Option<OpenedProject>> {
    let state = state.inner().clone();
    let projects = projects.inner().clone();
    execute(move || {
        let path = path.map(PathBuf::from).or_else(|| {
            rfd::FileDialog::new()
                .set_title("Open a WebnovelStudio project folder")
                .pick_folder()
        });
        let Some(path) = path else { return Ok(None) };
        let mut opened = projects.open(path, None, session)?;
        let project = projects.project(&opened.project.project_id)?;
        match state.0.lock().map_err(|_| lock_error())?.register(&project) {
            Ok(()) => {}
            Err(error) if error.code == "DuplicateProjectIdentity" => {
                projects.close(&project.info.project_id)?;
                return Err(error);
            }
            Err(error) => opened.library_warning = Some(error.detail),
        }
        Ok(Some(opened))
    })
    .await
}
#[tauri::command]
pub async fn library_archive(
    project_id: String,
    archived: bool,
    state: State<'_, DesktopLibrary>,
    projects: State<'_, DesktopProjects>,
) -> CoreResult<()> {
    let state = state.inner().clone();
    let projects = projects.inner().clone();
    execute(move || {
        if archived {
            projects.close(&project_id)?;
        }
        state
            .0
            .lock()
            .map_err(|_| lock_error())?
            .archive(&project_id, archived)
    })
    .await
}
#[tauri::command]
pub async fn library_recover(
    operation_id: String,
    title: String,
    session: String,
    state: State<'_, DesktopLibrary>,
    projects: State<'_, DesktopProjects>,
) -> CoreResult<Option<OpenedProject>> {
    let state = state.inner().clone();
    let projects = projects.inner().clone();
    execute(move || {
        let mut library = state.0.lock().map_err(|_| lock_error())?;
        let pending = if let Some(pending) = library.operation(&operation_id)? {
            if pending.kind != "recover" || pending.title != title {
                return Err(CoreError::new(
                    "OperationIdReuse",
                    "This recovery operation has a different request.",
                ));
            }
            pending.require_available()?;
            pending
        } else {
            let Some(archive) = rfd::FileDialog::new()
                .set_title("Recover a backup as a new project")
                .add_filter("WebnovelStudio backup", &["wnsbackup"])
                .pick_file()
            else {
                return Ok(None);
            };
            library.begin_recovery(&operation_id, &title, &archive)?
        };
        if pending.final_path.exists()
            && read_creation_origin(&pending.final_path)? == pending.origin
        {
            let mut opened = projects.open(pending.final_path, None, session)?;
            opened.library_warning = library
                .finish(
                    &operation_id,
                    &projects.project(&opened.project.project_id)?,
                )
                .err()
                .map(|e| e.detail);
            return Ok(Some(opened));
        }
        if pending.staging_path.join("creation.json").is_file()
            && read_creation_origin(&pending.staging_path)? == pending.origin
        {
            // A prepared recovery is validated from its own database. It does
            // not depend on the original archive still being available.
            let project = transfer::recover_backup_staged(
                &pending.staging_path.join("unused-resume-source.wnsbackup"),
                &pending.staging_path,
                &pending.final_path,
                &title,
                &pending.origin,
            )?;
            let warning = library
                .finish(&operation_id, &project)
                .err()
                .map(|error| error.detail);
            let mut opened = projects.insert(project, session)?;
            opened.library_warning = warning;
            return Ok(Some(opened));
        }
        let archive = pending.source_path.as_ref().ok_or_else(|| {
            CoreError::new(
                "InvalidBackup",
                "The pending recovery has no source backup.",
            )
        })?;
        library.begin_recovery(&operation_id, &title, archive)?;
        let project = transfer::recover_backup_staged(
            archive,
            &pending.staging_path,
            &pending.final_path,
            &title,
            &pending.origin,
        )?;
        let warning = library
            .finish(&operation_id, &project)
            .err()
            .map(|e| e.detail);
        let mut opened = projects.insert(project, session)?;
        opened.library_warning = warning;
        Ok(Some(opened))
    })
    .await
}
#[tauri::command]
pub async fn library_duplicate(
    operation_id: String,
    access: Option<ProjectAccess>,
    title: String,
    session: String,
    state: State<'_, DesktopLibrary>,
    projects: State<'_, DesktopProjects>,
) -> CoreResult<OpenedProject> {
    let state = state.inner().clone();
    let projects = projects.inner().clone();
    execute(move || {
        let mut library = state.0.lock().map_err(|_| lock_error())?;
        let pending = if let Some(pending) = library.operation(&operation_id)? {
            if pending.kind != "duplicate" || pending.title != title {
                return Err(CoreError::new(
                    "OperationIdReuse",
                    "This copy operation has a different request.",
                ));
            }
            pending.require_available()?;
            if let Some(access) = &access {
                let source = projects.project(&access.project_id)?;
                let basis: transfer::DuplicateBasis =
                    serde_json::from_str(pending.source_fingerprint.as_deref().unwrap_or(""))?;
                if pending.source_path.as_ref() != Some(&source.path)
                    || basis.project_id != access.project_id
                    || basis.operation_namespace != access.operation_namespace
                {
                    return Err(CoreError::new(
                        "OperationIdReuse",
                        "This copy operation belongs to another source project.",
                    ));
                }
                source.documents(access.clone())?;
            }
            pending
        } else {
            let access = access.clone().ok_or_else(|| {
                CoreError::new(
                    "InvalidRequest",
                    "Open a source project to create a new copy.",
                )
            })?;
            let source = projects.project(&access.project_id)?;
            let basis = transfer::capture_duplicate_basis(&source, access)?;
            let fingerprint = serde_json::to_string(&basis)?;
            library.begin(
                &operation_id,
                "duplicate",
                &title,
                Some((&source.path, &fingerprint)),
            )?
        };
        // Reconcile an installed copy before consulting a source that may since
        // have changed or moved. A retry always refers to this one destination.
        if pending.final_path.exists()
            && read_creation_origin(&pending.final_path)? == pending.origin
        {
            let mut opened = projects.open(pending.final_path, None, session)?;
            opened.library_warning = library
                .finish(
                    &operation_id,
                    &projects.project(&opened.project.project_id)?,
                )
                .err()
                .map(|e| e.detail);
            return Ok(opened);
        }
        let project = if pending.staging_path.join("creation.json").is_file()
            && read_creation_origin(&pending.staging_path)? == pending.origin
        {
            transfer::recover_backup_staged(
                &pending.staging_path.join("unused-resume-source.wnsbackup"),
                &pending.staging_path,
                &pending.final_path,
                &title,
                &pending.origin,
            )?
        } else {
            let basis: transfer::DuplicateBasis =
                serde_json::from_str(pending.source_fingerprint.as_deref().unwrap_or(""))?;
            let source = if let Some(access) = access {
                projects.project(&access.project_id)?
            } else {
                let path = pending.source_path.clone().ok_or_else(|| {
                    CoreError::new("InvalidRequest", "The pending copy has no source project.")
                })?;
                let opened = projects.open(path, None, session.clone())?;
                projects.project(&opened.project.project_id)?
            };
            transfer::duplicate_project_staged_with_basis(
                &source,
                &pending.staging_path,
                &pending.final_path,
                &title,
                &pending.origin,
                &basis,
            )?
        };
        let warning = library
            .finish(&operation_id, &project)
            .err()
            .map(|e| e.detail);
        let mut opened = projects.insert(project, session)?;
        opened.library_warning = warning;
        Ok(opened)
    })
    .await
}
#[tauri::command]
pub async fn project_backup(
    access: ProjectAccess,
    projects: State<'_, DesktopProjects>,
) -> CoreResult<Option<String>> {
    let project = projects.project(&access.project_id)?;
    execute(move || {
        project.documents(access)?;
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save project backup")
            .add_filter("WebnovelStudio backup", &["wnsbackup"])
            .set_file_name("Story backup.wnsbackup")
            .save_file()
        else {
            return Ok(None);
        };
        transfer::create_backup(&project, &path)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
}
#[tauri::command]
pub async fn project_export_draft(
    access: ProjectAccess,
    expected: Head,
    projects: State<'_, DesktopProjects>,
) -> CoreResult<Option<String>> {
    let project = projects.project(&access.project_id)?;
    execute(move || {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export draft as plain text (formatting is omitted)")
            .add_filter("Plain text", &["txt"])
            .set_file_name("Chapter draft.txt")
            .save_file()
        else {
            return Ok(None);
        };
        transfer::export_draft_txt(&project, access, expected, &path)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
}
