//! Read and persist the selected Codex transport without starting a request.
//!
//! The preference is intentionally independent from the model picker and from
//! any accepted project operation. Runtime dispatch may consume it later;
//! changing it here never starts, stops, or retries generation.

use crate::app_state::AppState;
use crate::project_commands::execute;
use tauri::State;
use webnovel_core::library::Library;
use webnovel_core::library::codex_transport::{CodexTransport, CodexTransportSettings};
use webnovel_core::projects::{CoreError, CoreResult};

fn unavailable() -> CoreError {
    CoreError::new(
        "LibraryUnavailable",
        "Codex transport settings are unavailable. Reload Settings before saving again.",
    )
}

fn read_with_library(library: &Library) -> CoreResult<CodexTransportSettings> {
    library.codex_transport_settings()
}

fn save_with_library(
    library: &mut Library,
    expected_revision: &str,
    transport: CodexTransport,
) -> CoreResult<CodexTransportSettings> {
    library.save_codex_transport(expected_revision, transport)
}

#[tauri::command]
pub async fn codex_transport_settings( state: State<'_, AppState>,
) -> CoreResult<CodexTransportSettings> {
    let app = &*state;
    let state = &app.library;
    let state = state.clone();
    execute(move || {
        let library = state.0.lock().map_err(|_| unavailable())?;
        read_with_library(&library)
    })
    .await
}

#[tauri::command]
pub async fn save_codex_transport(
    expected_revision: String,
    transport: CodexTransport, state: State<'_, AppState>,
) -> CoreResult<CodexTransportSettings> {
    let app = &*state;
    let state = &app.library;
    let state = state.clone();
    execute(move || {
        let mut library = state.0.lock().map_err(|_| unavailable())?;
        save_with_library(&mut library, &expected_revision, transport)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "wns-v3-codex-transport-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after the Unix epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn transport_settings_default_and_persist_with_cas() {
        let root = temp_root("persist");
        let mut library = Library::open(&root).expect("library opens");
        let initial = read_with_library(&library).expect("default transport reads");
        assert_eq!(initial.revision, "0");
        assert_eq!(initial.transport, CodexTransport::Exec);

        let saved = save_with_library(&mut library, "0", CodexTransport::AppServer)
            .expect("app-server preference saves");
        assert_eq!(saved.revision, "1");
        assert_eq!(saved.transport, CodexTransport::AppServer);

        let conflict = save_with_library(&mut library, "0", CodexTransport::Exec)
            .expect_err("stale renderer must not overwrite the preference");
        assert_eq!(conflict.code, "PreferenceConflict");

        drop(library);
        let reopened = Library::open(&root).expect("library reopens");
        let persisted = read_with_library(&reopened).expect("saved transport reads");
        assert_eq!(persisted, saved);
        drop(reopened);
        std::fs::remove_dir_all(root).expect("synthetic library is removed");
    }

    #[test]
    fn stale_save_leaves_the_current_transport_unchanged() {
        let root = temp_root("stale");
        let mut library = Library::open(&root).expect("library opens");
        save_with_library(&mut library, "0", CodexTransport::AppServer)
            .expect("first preference saves");
        let conflict = save_with_library(&mut library, "0", CodexTransport::Exec)
            .expect_err("stale save is rejected");
        assert_eq!(conflict.code, "PreferenceConflict");
        let current = read_with_library(&library).expect("current preference reads");
        assert_eq!(current.transport, CodexTransport::AppServer);
        assert_eq!(current.revision, "1");
        drop(library);
        std::fs::remove_dir_all(root).expect("synthetic library is removed");
    }
}
