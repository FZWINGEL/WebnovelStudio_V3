#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
#[cfg(debug_assertions)]
use std::hash::{DefaultHasher, Hash, Hasher};
#[cfg(debug_assertions)]
use std::path::PathBuf;
use tauri::Manager;
use webnovel_core::{SnapshotReceipt, validate_snapshot_json};
mod context_commands;
mod discussion_commands;
mod export_commands;
mod guidance_commands;
mod library_commands;
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
        persistence: true,
    }
}

fn main() {
    tauri::Builder::default()
        .manage(project_commands::DesktopProjects::default())
        .setup(|app| {
            // Installed releases keep their library across rebuilds and upgrades.
            // Development checkouts and synthetic qualification data stay separate.
            #[cfg(not(debug_assertions))]
            let library_root = app.path().app_local_data_dir()?;
            #[cfg(debug_assertions)]
            let library_root = {
                let mut checkout = DefaultHasher::new();
                env!("CARGO_MANIFEST_DIR").hash(&mut checkout);
                let root = std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir);
                std::env::var_os("WNS_V3_TEST_DATA_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        root.join("WebnovelStudioV3-Dev")
                            .join(format!("{:016x}", checkout.finish()))
                    })
            };
            let data_directory = library_root.join("webview");
            app.manage(library_commands::DesktopLibrary(std::sync::Arc::new(
                std::sync::Mutex::new(webnovel_core::library::Library::open(library_root)?),
            )));
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
            #[cfg(debug_assertions)]
            let window = window.title("WebnovelStudio V3 — Development");
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
            export_commands::prepare_draft_export,
            export_commands::export_prepared_draft,
            guidance_commands::read_guidance,
            guidance_commands::save_guidance,
            discussion_commands::read_discussion,
            discussion_commands::discussion_retry,
            discussion_commands::save_discussion_draft,
            discussion_commands::start_discussion,
            discussion_commands::stop_discussion,
            discussion_commands::proposals,
            discussion_commands::prepare_proposal,
            discussion_commands::apply_proposal,
            discussion_commands::reject_proposal,
            context_commands::context_epochs,
            context_commands::freeze_story_context,
            context_commands::story_context_snapshot,
            context_commands::read_story_context_source,
            context_commands::search_story_context,
            context_commands::prepare_story_context,
            context_commands::prepared_story_context,
            context_commands::prepared_story_context_is_current,
            context_commands::revoke_story_context,
            context_commands::rebuild_story_index,
            context_commands::capture_story_scope,
            validate_snapshot,
            runtime_info,
            project_commands::create_project,
            project_commands::open_project,
            project_commands::reconcile_project,
            project_commands::create_document,
            project_commands::list_documents,
            project_commands::read_document,
            project_commands::list_document_history,
            project_commands::read_document_revision,
            project_commands::restore_revision,
            project_commands::save_snapshot,
            project_commands::reconcile_document,
            project_commands::checkpoint_document,
            project_commands::document_history,
            project_commands::project_metadata,
            project_commands::rename_project,
            project_commands::rename_document,
            project_commands::read_view_state,
            project_commands::save_view_state,
            library_commands::library_snapshot,
            library_commands::library_create,
            library_commands::library_open,
            library_commands::library_archive,
            library_commands::library_recover,
            library_commands::library_duplicate,
            library_commands::project_backup
        ])
        .run(tauri::generate_context!())
        .expect("Could not launch WebnovelStudio V3");
}
