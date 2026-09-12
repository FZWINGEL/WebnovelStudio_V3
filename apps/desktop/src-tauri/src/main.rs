#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
#[cfg(debug_assertions)]
use std::hash::{DefaultHasher, Hash, Hasher};
#[cfg(debug_assertions)]
use std::path::PathBuf;
use tauri::Manager;
use webnovel_core::{SnapshotReceipt, validate_snapshot_json};
mod app_close_commands;
#[cfg(windows)]
mod app_server_discussion;
#[cfg(windows)]
mod app_server_memory;
#[cfg(all(test, windows))]
mod app_server_test_lock {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    // The synthetic app-server tests exercise one shared native fixture
    // executable and its process lifecycle.  Keep those fixture lifecycles
    // out of each other's way across test modules; this test-only lock does
    // not constrain production reservations or real app-server concurrency.
    static FIXTURE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    pub fn fixture_guard() -> MutexGuard<'static, ()> {
        FIXTURE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
#[cfg(windows)]
mod claude_live_discussion;
mod codex_transport_commands;
mod context_commands;
mod discussion_commands;
mod discussion_recovery;
mod endpoint_commands;
mod endpoint_discovery;
mod export_commands;
mod guidance_commands;
mod http_discussion;
mod http_memory;
mod library_commands;
#[cfg(windows)]
mod live_discussion;
#[cfg(windows)]
mod live_memory;
mod lookup_discussion;
mod memory_commands;
mod memory_recovery;
mod project_chat_commands;
mod project_commands;
mod provider_commands;
mod provider_runtime;
mod recovery_commands;
#[cfg(windows)]
mod reload_accelerators;
mod review_commands;
mod app_state;
mod author_start;
mod source_pin_commands;
mod v2_import_commands;
mod workshop_commands;
mod workshop_generation_commands;

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
    editor_trial: bool,
}

#[tauri::command]
fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        host: "Tauri",
        app_version: env!("CARGO_PKG_VERSION"),
        webview_version: tauri::webview_version().unwrap_or_else(|error| error.to_string()),
        persistence: true,
        editor_trial: cfg!(debug_assertions),
    }
}

fn main() {
    tauri::Builder::default()
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
            // One managed state for the whole shell. The library is the only
            // field whose construction can fail, which is why this happens in
            // `setup` rather than on the builder; nothing invokes a command
            // before `setup` returns.
            app.manage(app_state::AppState {
                projects: Default::default(),
                library: library_commands::DesktopLibrary(std::sync::Arc::new(
                    std::sync::Mutex::new(webnovel_core::library::Library::open(library_root)?),
                )),
                providers: Default::default(),
                discussion_recovery: Default::default(),
                memory_recovery: Default::default(),
                endpoint_discovery: Default::default(),
            });
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
                    .data_directory(data_directory)
                    .on_page_load(|window, payload| {
                        if matches!(payload.event(), tauri::webview::PageLoadEvent::Started) {
                            window
                                .state::<app_state::AppState>()
                                .providers
                                .renderer_started();
                        }
                    });
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
            let window = window.build()?;
            // Native accessibility qualification uses the real WebView zoom,
            // not CSS scaling or browser viewport emulation. Shipping windows
            // keep the author's normal WebView zoom shortcuts.
            #[cfg(debug_assertions)]
            if let Ok(zoom) = std::env::var("WNS_V3_TRIAL_ZOOM_FACTOR") {
                let zoom: f64 = zoom.parse()?;
                if !zoom.is_finite() || !(0.5..=3.0).contains(&zoom) {
                    return Err("Native trial zoom must be between 0.5 and 3.0".into());
                }
                window.set_zoom(zoom)?;
            }
            #[cfg(windows)]
            reload_accelerators::install(&window)?;
            #[cfg(debug_assertions)]
            eprintln!("WebView2 window created");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            project_chat_commands::project_activity,
            project_chat_commands::read_project_conversation,
            project_chat_commands::list_project_chat_history,
            project_chat_commands::read_project_chat_history,
            project_chat_commands::save_project_composer,
            project_chat_commands::start_project_chat,
            project_chat_commands::start_project_chapter,
            project_chat_commands::read_project_chapter_feedback,
            project_chat_commands::retry_project_chat_save,
            project_chat_commands::read_assistant_draft,
            project_chat_commands::save_assistant_draft,
            project_chat_commands::checkpoint_assistant_draft,
            project_chat_commands::reconcile_assistant_draft,
            project_chat_commands::set_chat_disposition,
            project_chat_commands::prepare_chat_adoption,
            project_chat_commands::adopt_chat_preview,
            project_chat_commands::read_chat_adoption_preview,
            app_close_commands::begin_app_close,
            app_close_commands::app_close_status,
            app_close_commands::stop_app_jobs,
            app_close_commands::finish_app_close,
            app_close_commands::cancel_app_close,
            review_commands::chapter_review_status,
            review_commands::read_reviewed_record_set,
            review_commands::reviewed_entity_catalog,
            review_commands::reviewed_evidence_history,
            review_commands::reviewed_promise_catalog,
            review_commands::reviewed_promise_history,
            review_commands::reviewed_knowledge_character_catalog,
            review_commands::reviewed_knowledge_topic_catalog,
            review_commands::reviewed_knowledge_history,
            review_commands::stage_author_review,
            review_commands::read_review_stage,
            review_commands::mark_ready,
            provider_commands::provider_state,
            v2_import_commands::v2_import_list_projects,
            v2_import_commands::v2_import_preview,
            v2_import_commands::v2_import,
            provider_commands::check_codex_connection,
            provider_commands::check_claude_connection,
            provider_commands::save_model_settings,
            provider_commands::save_story_memory_provider,
            codex_transport_commands::codex_transport_settings,
            codex_transport_commands::save_codex_transport,
            endpoint_commands::endpoint_settings,
            endpoint_commands::save_endpoint_settings,
            endpoint_commands::discover_endpoint_models,
            endpoint_discovery::cancel_endpoint_discovery,
            export_commands::prepare_draft_export,
            export_commands::prepare_reviewed_draft_export,
            export_commands::export_prepared_draft,
            recovery_commands::save_recovery_copy,
            guidance_commands::read_guidance,
            guidance_commands::save_guidance,
            discussion_commands::read_discussion,
            discussion_commands::retry_discussion_save,
            discussion_commands::discussion_retry,
            discussion_commands::save_discussion_draft,
            discussion_commands::start_discussion,
            discussion_commands::stop_discussion,
            discussion_commands::proposals,
            discussion_commands::prepare_proposal,
            discussion_commands::prepare_continuation,
            discussion_commands::prepare_structured,
            discussion_commands::apply_proposal,
            discussion_commands::reject_proposal,
            memory_commands::read_memory,
            memory_commands::read_memory_source,
            memory_commands::retry_memory_save,
            memory_commands::start_memory,
            memory_commands::stop_memory,
            context_commands::context_epochs,
            context_commands::read_document_aliases,
            context_commands::set_document_aliases,
            context_commands::freeze_story_context,
            context_commands::freeze_reviewed_continuation,
            context_commands::story_context_snapshot,
            context_commands::read_story_context_source,
            context_commands::search_story_context,
            context_commands::prepare_story_context,
            context_commands::prepared_story_context,
            context_commands::prepared_story_context_is_current,
            context_commands::revoke_story_context,
            context_commands::rebuild_story_index,
            context_commands::capture_story_scope,
            source_pin_commands::read_source_pins,
            source_pin_commands::save_source_pins,
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
            library_commands::library_resume_import,
            library_commands::project_backup,
            workshop_commands::read_workshop,
            workshop_commands::save_workshop,
            workshop_commands::workshop_history,
            workshop_commands::preview_workshop_adoption,
            workshop_commands::adopt_workshop,
            workshop_commands::export_workshop_preset,
            workshop_commands::import_workshop_preset,
            workshop_generation_commands::start_workshop
        ])
        .run(tauri::generate_context!())
        .expect("Could not launch WebnovelStudio V3");
}
