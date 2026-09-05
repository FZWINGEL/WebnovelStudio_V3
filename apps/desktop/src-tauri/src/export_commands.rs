//! Preview a frozen draft before the explicit native destination choice.
use crate::project_commands::{DesktopProjects, execute};
use serde::Serialize;
use tauri::State;
use webnovel_core::projects::{CoreResult, Head, ProjectAccess};
use webnovel_core::transfer::{self, DraftExportPreview, DraftFormat};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftExportResult {
    path: String,
    preview_id: String,
    sha256: String,
    utf8_bytes: u64,
}

#[tauri::command]
pub async fn prepare_draft_export(
    access: ProjectAccess,
    expected: Head,
    format: DraftFormat,
    projects: State<'_, DesktopProjects>,
) -> CoreResult<DraftExportPreview> {
    let project = projects.project(&access.project_id)?;
    execute(move || transfer::prepare_draft_export(&project, &access, expected, format)).await
}

#[tauri::command]
pub async fn export_prepared_draft(
    access: ProjectAccess,
    preview: DraftExportPreview,
    projects: State<'_, DesktopProjects>,
) -> CoreResult<Option<DraftExportResult>> {
    let project = projects.project(&access.project_id)?;
    execute(move || {
        let (label, extension, filename) = match preview.format {
            DraftFormat::PlainText => ("Plain text", "txt", "Chapter draft.txt"),
            DraftFormat::Markdown => ("Markdown", "md", "Chapter draft.md"),
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save draft as a new file")
            .add_filter(label, &[extension])
            .set_file_name(filename)
            .save_file()
        else {
            return Ok(None);
        };
        let record = transfer::export_prepared_draft(&project, access, preview, &path)?;
        Ok(Some(DraftExportResult {
            path: path.to_string_lossy().into_owned(),
            preview_id: record.id,
            sha256: record.sha256,
            utf8_bytes: record.utf8_bytes,
        }))
    })
    .await
}
