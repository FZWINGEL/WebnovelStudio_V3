//! Save a captured live buffer independently of failed project storage.
use crate::commands::project_commands::execute;
use serde::Serialize;
use serde_json::Value;
use webnovel_core::projects::{CoreError, CoreResult};
use webnovel_core::transfer::{self, RecoveryCopyReceipt};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCopyResult {
    path: String,
    #[serde(flatten)]
    receipt: RecoveryCopyReceipt,
}

#[tauri::command]
pub async fn save_recovery_copy(body: Value) -> CoreResult<Option<RecoveryCopyResult>> {
    execute(move || {
        // Reject unsupported content before asking for a destination. The core
        // revalidates the same immutable capture before installing its bytes.
        webnovel_core::validate_snapshot_json(&serde_json::to_string(&body)?)
            .map_err(|error| CoreError::new("InvalidDocument", &error))?;
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save recovery copy as a new file")
            .add_filter("Markdown", &["md"])
            .set_file_name("Manuscript recovery.md")
            .save_file()
        else {
            return Ok(None);
        };
        let receipt = transfer::save_recovery_copy(&body, &path)?;
        Ok(Some(RecoveryCopyResult {
            path: path.to_string_lossy().into_owned(),
            receipt,
        }))
    })
    .await
}
