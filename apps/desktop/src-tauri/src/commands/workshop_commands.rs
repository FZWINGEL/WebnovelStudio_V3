//! Tauri adapters for the durable Story Workshop actor boundary.
use crate::app_state::AppState;
use crate::commands::project_commands::execute;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use webnovel_core::projects::workshop::{
    PreviewWorkshopAdoption, SaveWorkshop, WorkshopAdoptionAck, WorkshopAdoptionPreview,
    WorkshopPreset, WorkshopSnapshot, WorkshopView, validate_workshop_preset,
};
use webnovel_core::projects::{CoreResult, ProjectAccess};

const WORKSHOP_PRESET_SCHEMA: &str = "workshop-preset.v1";
const MAX_PRESET_FILE_BYTES: u64 = 512 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkshopPresetFile {
    schema_version: String,
    #[serde(default)]
    id: Option<String>,
    name: String,
    preferences: Vec<webnovel_core::projects::workshop::WorkshopPreference>,
}

fn preset_file(preset: &WorkshopPreset) -> CoreResult<WorkshopPresetFile> {
    validate_workshop_preset(preset)?;
    Ok(WorkshopPresetFile {
        schema_version: WORKSHOP_PRESET_SCHEMA.into(),
        id: Some(preset.id.clone()),
        name: preset.name.clone(),
        preferences: preset.preferences.clone(),
    })
}

fn imported_preset(file: WorkshopPresetFile) -> CoreResult<WorkshopPreset> {
    if file.schema_version != WORKSHOP_PRESET_SCHEMA {
        return Err(webnovel_core::projects::CoreError::new(
            "UnsupportedSchema",
            "This is not a Story Workshop preset file.",
        ));
    }
    let id = file.id.unwrap_or_else(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        format!("preset-import-{nanos}")
    });
    let preset = WorkshopPreset {
        id,
        name: file.name,
        preferences: file.preferences,
    };
    validate_workshop_preset(&preset)?;
    Ok(preset)
}

#[tauri::command]
pub async fn read_workshop(
    access: ProjectAccess, state: State<'_, AppState>,
) -> CoreResult<WorkshopView> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.workshop().read(access)).await
}

#[tauri::command]
pub async fn save_workshop(
    request: SaveWorkshop, state: State<'_, AppState>,
) -> CoreResult<WorkshopSnapshot> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.workshop().save(request)).await
}

#[tauri::command]
pub async fn workshop_history(
    access: ProjectAccess, state: State<'_, AppState>,
) -> CoreResult<Vec<WorkshopSnapshot>> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.workshop().history(access)).await
}

#[tauri::command]
pub async fn preview_workshop_adoption(
    request: PreviewWorkshopAdoption, state: State<'_, AppState>,
) -> CoreResult<WorkshopAdoptionPreview> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&request.access.project_id)?;
    execute(move || project.workshop().preview_adoption(request)).await
}

#[tauri::command]
pub async fn adopt_workshop(
    access: ProjectAccess,
    operation_id: String,
    preview_id: String, state: State<'_, AppState>,
) -> CoreResult<WorkshopAdoptionAck> {
    let app = &*state;
    let state = &app.projects;
    let project = state.project(&access.project_id)?;
    execute(move || project.workshop().adopt(access, operation_id, preview_id)).await
}

/// Save one validated preset as a native JSON file. This is an export only;
/// importing or applying it never mutates project workshop state implicitly.
#[tauri::command]
pub async fn export_workshop_preset(preset: WorkshopPreset) -> CoreResult<Option<String>> {
    execute(move || {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save Story Workshop preset")
            .add_filter("Workshop preset", &["json"])
            .set_file_name("Workshop preset.json")
            .save_file()
        else {
            return Ok(None);
        };
        let file = preset_file(&preset)?;
        let bytes = serde_json::to_vec_pretty(&file)?;
        std::fs::write(&path, bytes)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
}

/// Read one validated preset from a native JSON file. The returned value is
/// inert and must be reviewed and merged by the renderer through saveWorkshop.
#[tauri::command]
pub async fn import_workshop_preset() -> CoreResult<Option<WorkshopPreset>> {
    execute(move || {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open Story Workshop preset")
            .add_filter("Workshop preset", &["json"])
            .pick_file()
        else {
            return Ok(None);
        };
        let metadata = std::fs::metadata(&path)?;
        if metadata.len() > MAX_PRESET_FILE_BYTES {
            return Err(webnovel_core::projects::CoreError::new(
                "InvalidRequest",
                "The Story Workshop preset file is too large.",
            ));
        }
        let bytes = std::fs::read(&path)?;
        let file: WorkshopPresetFile = serde_json::from_slice(&bytes)?;
        imported_preset(file).map(Some)
    })
    .await
}
