#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use webnovel_core::{SnapshotReceipt, validate_snapshot_json};
mod project_commands;

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
        .manage(project_commands::DesktopProjects::default())
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
            #[cfg(debug_assertions)]
            eprintln!(
                "Starting WebView2 {:?}, data directory {:?}",
                tauri::webview_version(),
                data_directory
            );
            let window =
                tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                    .data_directory(data_directory);
            // Hosted Windows runners may be elevated. WebView2 ignores its own
            // environment overrides there, so the debug harness uses the API.
            // This opt-in branch is absent from shipping release binaries.
            #[cfg(debug_assertions)]
            let window = if let Ok(port) = std::env::var("WNS_V3_NATIVE_CDP_PORT") {
                let port: std::num::NonZeroU16 = port.parse()?;
                window.additional_browser_args(&format!(
                    "--remote-debugging-port={port} --remote-debugging-address=127.0.0.1"
                ))
            } else {
                window
            };
            window.build()?;
            #[cfg(debug_assertions)]
            eprintln!("WebView2 window created");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            validate_snapshot,
            runtime_info,
            project_commands::create_project,
            project_commands::open_project,
            project_commands::create_document,
            project_commands::list_documents,
            project_commands::read_document,
            project_commands::save_snapshot,
            project_commands::reconcile_document,
            project_commands::checkpoint_document,
            project_commands::document_history
        ])
        .run(tauri::generate_context!())
        .expect("Could not launch the editor trial");
}
