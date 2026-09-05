use rusqlite::Connection;
use std::fs;
#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::v2_import::preview_v2_import;
#[cfg(windows)]
use webnovel_core::v2_import::{V2BodySelection, V2WorkingProse};

const FIXTURE: &str = include_str!("../../../tests/fixtures/v2-import/schema8.sql");

struct TempSource {
    path: PathBuf,
}

impl TempSource {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("v2-import-{}.db", Uuid::new_v4()));
        let connection = Connection::open(&path).expect("fixture database");
        connection.execute_batch(FIXTURE).expect("fixture schema");
        drop(connection);
        Self { path }
    }

    #[cfg(windows)]
    fn edit(&self, sql: &str) {
        let connection = Connection::open(&self.path).expect("open fixture");
        connection.execute_batch(sql).expect("fixture edit");
    }
}

impl Drop for TempSource {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn bytes(path: &Path) -> Vec<u8> {
    fs::read(path).expect("source bytes")
}

#[cfg(windows)]
#[test]
fn selected_project_preview_preserves_body_states_and_filters_records() {
    let source = TempSource::new();
    let before = bytes(&source.path);
    let preview = preview_v2_import(&source.path, "p-alpha").expect("valid V2 source");

    assert_eq!(preview.import_format_version, 1);
    assert_eq!(preview.source.schema_version, 8);
    assert_eq!(preview.source.project_count, 2);
    assert_eq!(preview.source.migration_versions.len(), 8);
    assert_eq!(preview.project.title, "Alpha Story");
    assert_eq!(preview.chapters.len(), 4);
    assert_eq!(preview.chapters[0].source_id, "a-null");
    assert_eq!(preview.chapters[0].working_prose, V2WorkingProse::Missing);
    assert_eq!(
        preview.chapters[0].body_selection,
        V2BodySelection::RequiresAuthorChoice
    );
    assert_eq!(
        preview.chapters[1].working_prose,
        V2WorkingProse::Present(String::new())
    );
    assert_eq!(
        preview.chapters[1].body_selection,
        V2BodySelection::WorkingProse
    );
    assert_eq!(
        preview.chapters[2].working_prose,
        V2WorkingProse::Present("Working newer \r\ntext".into())
    );
    assert_eq!(
        preview.chapters[2].approved_draft_id.as_deref(),
        Some("a-newer-old")
    );
    assert_eq!(
        preview.chapters[3].retired_at.as_deref(),
        Some("2026-01-03")
    );
    assert!(preview.legacy.records.iter().all(|record| {
        record
            .payload
            .get("project_id")
            .and_then(|value| value.as_str())
            != Some("p-beta")
    }));
    assert!(preview.legacy.record_counts["chapters"] == 4);
    assert_eq!(
        before,
        bytes(&source.path),
        "preview must not mutate V2 source"
    );
}

#[cfg(windows)]
#[test]
fn preview_of_second_project_is_independent() {
    let source = TempSource::new();
    let preview = preview_v2_import(&source.path, "p-beta").expect("beta project");
    assert_eq!(preview.project.source_project_id, "p-beta");
    assert_eq!(preview.chapters.len(), 1);
    assert_eq!(preview.chapters[0].source_id, "b-one");
    assert!(preview.legacy.records.iter().all(|record| {
        record
            .payload
            .get("project_id")
            .and_then(|value| value.as_str())
            != Some("p-alpha")
    }));
}

#[cfg(windows)]
#[test]
fn cross_project_reference_is_rejected_before_projection() {
    let source = TempSource::new();
    source.edit("UPDATE chapters SET active_canon_ids='[\"c-beta\"]' WHERE id='a-newer'");
    let error = preview_v2_import(&source.path, "p-alpha").expect_err("foreign canon ref");
    assert_eq!(error.code, "InvalidV2Reference");
}

#[cfg(windows)]
#[test]
fn unsupported_schema_and_missing_ledger_are_rejected() {
    let source = TempSource::new();
    source.edit("PRAGMA user_version=9");
    let error = preview_v2_import(&source.path, "p-alpha").expect_err("schema 9");
    assert_eq!(error.code, "UnsupportedV2Schema");

    let source = TempSource::new();
    source.edit("DELETE FROM schema_migrations WHERE version=7");
    let error = preview_v2_import(&source.path, "p-alpha").expect_err("missing ledger row");
    assert_eq!(error.code, "InvalidV2Snapshot");
}

#[cfg(windows)]
#[test]
fn missing_required_table_is_rejected() {
    let source = TempSource::new();
    source.edit("PRAGMA foreign_keys=OFF; DROP TABLE audit_findings;");
    let error = preview_v2_import(&source.path, "p-alpha").expect_err("missing table");
    assert_eq!(error.code, "InvalidV2Snapshot");
}

#[cfg(windows)]
#[test]
fn missing_required_column_and_foreign_key_corruption_are_rejected() {
    let source = TempSource::new();
    source.edit("ALTER TABLE chapters DROP COLUMN retired_at");
    let error = preview_v2_import(&source.path, "p-alpha").expect_err("missing column");
    assert_eq!(error.code, "InvalidV2Snapshot");

    let source = TempSource::new();
    source.edit(
        "PRAGMA foreign_keys=OFF; UPDATE chapters SET project_id='missing' WHERE id='a-newer';",
    );
    let error = preview_v2_import(&source.path, "p-alpha").expect_err("foreign key corruption");
    assert_eq!(error.code, "InvalidV2Snapshot");
}

#[cfg(windows)]
#[test]
fn live_sqlite_sidecar_is_refused_without_touching_source() {
    let source = TempSource::new();
    let sidecar = PathBuf::from(format!("{}-wal", source.path.display()));
    fs::write(&sidecar, b"test marker").expect("sidecar marker");
    let error = preview_v2_import(&source.path, "p-alpha").expect_err("live sidecar");
    assert_eq!(error.code, "InvalidV2Snapshot");
    assert_eq!(fs::read(&sidecar).expect("marker remains"), b"test marker");
    fs::remove_file(sidecar).expect("remove test marker");
}

#[cfg(windows)]
#[test]
fn persistent_wal_header_is_refused_before_sqlite_open() {
    let source = TempSource::new();
    let mut file = OpenOptions::new()
        .write(true)
        .open(&source.path)
        .expect("open header");
    file.seek(SeekFrom::Start(18)).expect("seek header");
    file.write_all(&[2, 2]).expect("mark WAL header");
    file.sync_all().expect("flush header");
    drop(file);
    let error = preview_v2_import(&source.path, "p-alpha").expect_err("WAL header");
    assert_eq!(error.code, "InvalidV2Snapshot");
}

#[cfg(not(windows))]
#[test]
fn non_windows_import_is_explicitly_unsupported_and_source_is_untouched() {
    let source = TempSource::new();
    let before = bytes(&source.path);
    let error = preview_v2_import(&source.path, "p-alpha").expect_err("platform guard");
    assert_eq!(error.code, "UnsupportedPlatform");
    assert_eq!(before, bytes(&source.path));
}
