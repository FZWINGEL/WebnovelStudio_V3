//! Preview a frozen draft before the explicit native destination choice.
use crate::app_state::AppState;
use crate::commands::project_commands::execute;
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
    state: State<'_, AppState>,
) -> CoreResult<DraftExportPreview> {
    let app = &*state;
    let projects = &app.projects;
    let project = projects.project(&access.project_id)?;
    execute(move || transfer::prepare_draft_export(&project, &access, expected, format)).await
}

#[tauri::command]
pub async fn prepare_reviewed_draft_export(
    access: ProjectAccess,
    expected: Head,
    format: DraftFormat,
    state: State<'_, AppState>,
) -> CoreResult<DraftExportPreview> {
    let app = &*state;
    let projects = &app.projects;
    let project = projects.project(&access.project_id)?;
    execute(move || transfer::prepare_reviewed_draft_export(&project, &access, expected, format))
        .await
}

#[tauri::command]
pub async fn export_prepared_draft(
    access: ProjectAccess,
    preview: DraftExportPreview,
    state: State<'_, AppState>,
) -> CoreResult<Option<DraftExportResult>> {
    let app = &*state;
    let projects = &app.projects;
    let project = projects.project(&access.project_id)?;
    execute(move || {
        let reviewed = preview.review_bundle_id.is_some();
        let (label, extension, filename) = match (preview.format, reviewed) {
            (DraftFormat::PlainText, false) => ("Plain text", "txt", "Chapter draft.txt"),
            (DraftFormat::Markdown, false) => ("Markdown", "md", "Chapter draft.md"),
            (DraftFormat::PlainText, true) => ("Plain text", "txt", "Author-reviewed chapter.txt"),
            (DraftFormat::Markdown, true) => ("Markdown", "md", "Author-reviewed chapter.md"),
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title(if reviewed {
                "Save author-reviewed snapshot as a new file"
            } else {
                "Save draft as a new file"
            })
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
