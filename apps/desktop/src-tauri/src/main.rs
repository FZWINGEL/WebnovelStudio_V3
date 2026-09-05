#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use webnovel_core::{SnapshotReceipt, validate_snapshot_json};

#[tauri::command]
fn validate_snapshot(snapshot_json: String) -> Result<SnapshotReceipt, String> {
    validate_snapshot_json(&snapshot_json)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeInfo {
    host: &'static str,
    app_version: &'static str,
    webview_version: String,
    persistence: bool,
}

#[tauri::command]
fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        host: "Tauri",
        app_version: env!("CARGO_PKG_VERSION"),
        webview_version: tauri::webview_version().unwrap_or_else(|error| error.to_string()),
        persistence: false,
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let mut checkout = DefaultHasher::new();
            env!("CARGO_MANIFEST_DIR").hash(&mut checkout);
            let root = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let data_directory = root
                .join("WebnovelStudioV3-Dev")
                .join(format!("{:016x}", checkout.finish()))
                .join("webview");
            #[cfg(debug_assertions)]
            let data_directory = std::env::var_os("WNS_V3_TRIAL_WEBVIEW_DIR")
                .map(PathBuf::from)
                .unwrap_or(data_directory);
            tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .data_directory(data_directory)
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![validate_snapshot, runtime_info])
        .run(tauri::generate_context!())
        .expect("Could not launch the editor trial");
}
