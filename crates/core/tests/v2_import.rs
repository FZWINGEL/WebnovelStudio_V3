use rusqlite::Connection;
use std::fs;
#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
#[cfg(windows)]
use webnovel_core::context::packet::MockContextBudget;
#[cfg(windows)]
use webnovel_core::library::Library;
#[cfg(windows)]
use webnovel_core::projects::ProjectSession;
#[cfg(windows)]
use webnovel_core::projects::discussions::{DiscussionBegin, StartDiscussion};
#[cfg(windows)]
use webnovel_core::projects::import::{
    V2ChapterBodyChoice, V2ChapterBodyDecision, V2ImportRequest,
};
#[cfg(windows)]
use webnovel_core::transfer::{create_backup, recover_backup};
#[cfg(windows)]
use webnovel_core::v2_import::{V2BodySelection, V2WorkingProse};
use webnovel_core::v2_import::{list_v2_projects, preview_v2_import};

const FIXTURE: &str = include_str!("../../../tests/fixtures/v2-import/schema8.sql");

struct TempSource {
    path: PathBuf,
}

impl TempSource {
    fn new() -> Self {
        let started = std::time::Instant::now();
        let path = std::env::temp_dir().join(format!("v2-import-{}.db", Uuid::new_v4()));
        let connection = Connection::open(&path).expect("fixture database");
        connection.execute_batch(FIXTURE).expect("fixture schema");
        assert!(connection.is_autocommit(), "fixture must be committed");
        assert_eq!(
            connection
                .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            8
        );
        assert!(
            !connection
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .exists([])
                .unwrap()
        );
        connection.close().expect("close committed fixture");
        if std::env::var_os("WNS_V3_FIXTURE_TIMINGS").is_some() {
            eprintln!(
                "{}",
                serde_json::json!({
                    "phase": "v2-fixture-setup",
                    "test": std::thread::current().name(),
                    "durationMs": started.elapsed().as_secs_f64() * 1000.0
                })
            );
        }
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

#[test]
fn import_request_accepts_tauri_camel_case_draft_choice() {
    let request: webnovel_core::projects::import::V2ImportRequest = serde_json::from_str(
        r#"{
            "operationId":"import-1",
            "sourcePath":"C:/author/story.db",
            "sourceProjectId":"p-alpha",
            "title":"Imported story",
            "expectedSourceSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "choices":[{"sourceChapterId":"chapter-1","choice":{"draft":{"sourceDraftId":"draft-2"}}}]
        }"#,
    )
    .expect("camelCase IPC request");
    assert_eq!(request.choices.len(), 1);
    assert_eq!(
        request.choices[0].choice,
        webnovel_core::projects::import::V2ChapterBodyChoice::Draft {
            source_draft_id: "draft-2".into()
        }
    );
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
    assert_eq!(preview.chapters[0].drafts[0].source_id, "a-old");
    assert_eq!(preview.chapters[0].drafts[0].prose, "Approved null");
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
fn source_project_listing_is_bounded_and_does_not_choose_for_the_author() {
    let source = TempSource::new();
    let projects = list_v2_projects(&source.path).expect("list source projects");
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].source_project_id, "p-alpha");
    assert_eq!(projects[0].title, "Alpha Story");
    assert_eq!(projects[0].chapter_count, 4);
    assert_eq!(projects[1].source_project_id, "p-beta");
    assert_eq!(projects[1].chapter_count, 1);
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
    let error = list_v2_projects(&source.path).expect_err("list platform guard");
    assert_eq!(error.code, "UnsupportedPlatform");
    assert_eq!(before, bytes(&source.path));
}

#[cfg(windows)]
#[test]
fn staged_import_replays_by_operation_and_recovery_keeps_inert_evidence() {
    let source = TempSource::new();
    let source_before = bytes(&source.path);
    let preview = preview_v2_import(&source.path, "p-alpha").expect("valid V2 source");
    let root = std::env::temp_dir().join(format!("v2-import-library-{}", Uuid::new_v4()));
    fs::create_dir(&root).expect("library root");
    let mut library = Library::open(root.join("app")).expect("open library");
    let request = V2ImportRequest {
        operation_id: format!("import-{}", Uuid::new_v4()),
        source_path: source.path.clone(),
        source_project_id: "p-alpha".into(),
        title: "Imported Alpha".into(),
        expected_source_sha256: preview.source.source_sha256.clone(),
        choices: vec![V2ChapterBodyDecision {
            source_chapter_id: "a-null".into(),
            choice: V2ChapterBodyChoice::Empty,
        }],
    };
    let result = library.import_v2(request.clone()).expect("stage import");
    assert_ne!(result.project.project_id, "p-alpha");
    assert_ne!(result.project.operation_namespace, "p-alpha");
    assert_eq!(result.chapter_document_ids.len(), 4);
    assert_eq!(
        source_before,
        bytes(&source.path),
        "import must not mutate source"
    );

    let imported_path = root
        .join("app")
        .join("Projects")
        .join(format!("project-{}", request.operation_id));
    let database = imported_path.clone();
    let connection = Connection::open(database.join("project.sqlite3")).expect("open imported DB");
    let documents: i64 = connection
        .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
        .expect("count imported documents");
    let legacy: i64 = connection
        .query_row("SELECT COUNT(*) FROM import_legacy_records", [], |row| {
            row.get(0)
        })
        .expect("count inert records");
    assert_eq!(documents, 9, "chapters plus five narrative source groups");
    assert!(legacy > 0);
    for (table, expected_label, source_id) in [
        ("story_bibles", "Story Bible: premise: premise", "b-alpha"),
        ("canon_entities", "Ari: category: character", "c-alpha"),
        ("termbase", "Qi: concept_id: qi", "t-alpha"),
        ("plot_threads", "The Gate: title: The Gate", "pt-alpha"),
        ("story_arcs", "Arc 1: Arrival: arc_number: 1", "arc-alpha"),
    ] {
        let body: String = connection
            .query_row(
                "SELECT d.body_json FROM documents d
                 JOIN import_id_map m ON m.v3_document_id=d.id
                 WHERE m.source_table=? AND m.source_id=?",
                [table, "p-alpha-summary"],
                |row| row.get(0),
            )
            .unwrap_or_else(|error| panic!("read imported {table} note: {error}"));
        assert!(
            body.contains(expected_label),
            "{table} note should use its readable label"
        );
        assert!(
            !body.contains(&format!("{source_id}:")),
            "{table} note must not expose its internal source id"
        );
        let preserved_id: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM import_legacy_records WHERE source_table=? AND source_id=?",
                [table, source_id],
                |row| row.get(0),
            )
            .expect("read preserved legacy source id");
        assert_eq!(preserved_id, 1, "legacy evidence keeps exact source id");
    }
    drop(connection);

    let registry =
        Connection::open(root.join("app/library.sqlite3")).expect("open library registry");
    registry
        .execute(
            "UPDATE operations SET completed=0 WHERE operation_id=?",
            [&request.operation_id],
        )
        .expect("simulate lost registry acknowledgment");
    drop(registry);
    let reconciled = library
        .import_v2(request.clone())
        .expect("reconcile installed destination");
    assert_eq!(reconciled.project.project_id, result.project.project_id);
    assert!(library.pending().expect("pending operations").is_empty());

    // The imported folder can already be open when a lost library
    // acknowledgment is reconciled. The receipt is self-contained: replay
    // must not acquire a second project writer or reopen the V2 source.
    let imported_session = ProjectSession::open(&imported_path).expect("open imported project");
    let imported_access = imported_session
        .attach("import-replay-session".into())
        .expect("attach imported project");
    let imported_document = imported_session
        .document(
            imported_access.clone(),
            result.chapter_document_ids["a-null"].clone(),
        )
        .expect("read imported chapter");
    let started = imported_session
        .start_discussion(StartDiscussion {
            access: imported_access,
            operation_id: "import-replay-discussion".into(),
            expected: imported_document.head,
            instruction: "Keep the chapter discussion open while reconciling import.".into(),
            intent: Default::default(),
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: None,
        })
        .expect("start imported discussion");
    let running = imported_session
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner,
        })
        .expect("begin imported discussion")
        .run;
    assert_eq!(
        running.status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Running
    );
    fs::remove_file(&source.path).expect("source can disappear after import");
    let replay = library.import_v2(request.clone()).expect("replay import");
    assert_eq!(replay.project.project_id, result.project.project_id);
    assert_eq!(library.list().expect("list library").len(), 1);
    let tampered = Connection::open(imported_path.join("project.sqlite3"))
        .expect("open imported DB for replay tamper");
    tampered
        .execute(
            "UPDATE import_manifest SET source_sha256=? WHERE singleton=1",
            ["b".repeat(64)],
        )
        .expect("tamper import source hash");
    drop(tampered);
    let error = library
        .import_v2(request.clone())
        .expect_err("replay must reject a tampered import hash");
    assert_eq!(error.code, "InvalidProject");
    let restored = Connection::open(imported_path.join("project.sqlite3"))
        .expect("reopen imported DB after replay tamper");
    restored
        .execute(
            "UPDATE import_manifest SET source_sha256=? WHERE singleton=1",
            [&preview.source.source_sha256],
        )
        .expect("restore import source hash");
    drop(restored);
    let mut changed = request.clone();
    changed.title = "Changed title".into();
    let error = library
        .import_v2(changed)
        .expect_err("changed operation must refuse");
    assert_eq!(error.code, "OperationIdReuse");

    drop(imported_session);
    let imported = ProjectSession::open(&imported_path).expect("open imported project");
    let backup = root.join("import.wnsbackup");
    create_backup(&imported, &backup).expect("backup imported project");
    drop(imported);
    let recovered_path = root.join("recovered");
    let recovered = recover_backup(&backup, &recovered_path, "Recovered Alpha")
        .expect("recover imported project");
    assert_ne!(recovered.info.project_id, result.project.project_id);
    drop(recovered);
    let recovered_db =
        Connection::open(recovered_path.join("project.sqlite3")).expect("open recovered DB");
    let (current_id, current_namespace, source_id, operation_id, evidence): (
        String,
        String,
        String,
        String,
        i64,
    ) = recovered_db
        .query_row(
            "SELECT m.project_id,m.operation_namespace,m.source_project_id,m.operation_id,
                    (SELECT COUNT(*) FROM import_legacy_records)
             FROM import_manifest m WHERE m.singleton=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .expect("read recovered import evidence");
    assert_ne!(current_id, result.project.project_id);
    assert_ne!(current_namespace, result.project.operation_namespace);
    assert_eq!(source_id, "p-alpha");
    assert_eq!(operation_id, request.operation_id);
    assert!(evidence > 0);
    drop(recovered_db);
    drop(library);
    let connection = Connection::open(imported_path.join("project.sqlite3"))
        .expect("open imported DB for tamper");
    connection
        .execute(
            "UPDATE import_legacy_records SET source_project_id='other-project' WHERE rowid=(SELECT rowid FROM import_legacy_records LIMIT 1)",
            [],
        )
        .expect("tamper inert import evidence");
    drop(connection);
    let tampered = ProjectSession::open(&imported_path).expect("open tampered project");
    let error = create_backup(&tampered, &root.join("tampered.wnsbackup"))
        .expect_err("backup must reject cross-project import evidence");
    assert_eq!(error.code, "InvalidBackup");
    drop(tampered);
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn missing_body_choice_and_foreign_choice_fail_before_install() {
    let source = TempSource::new();
    let preview = preview_v2_import(&source.path, "p-alpha").expect("valid V2 source");
    let root = std::env::temp_dir().join(format!("v2-import-choice-{}", Uuid::new_v4()));
    fs::create_dir(&root).expect("choice root");
    let mut library = Library::open(root.join("app")).expect("open library");
    let request = |operation_id: String, choices| V2ImportRequest {
        operation_id,
        source_path: source.path.clone(),
        source_project_id: "p-alpha".into(),
        title: "Choice import".into(),
        expected_source_sha256: preview.source.source_sha256.clone(),
        choices,
    };
    let missing = request("choice-missing".into(), Vec::new());
    let error = library.import_v2(missing).expect_err("missing choice");
    assert_eq!(error.code, "BodyChoiceRequired");
    let foreign = request(
        "choice-foreign".into(),
        vec![
            V2ChapterBodyDecision {
                source_chapter_id: "a-null".into(),
                choice: V2ChapterBodyChoice::Empty,
            },
            V2ChapterBodyDecision {
                source_chapter_id: "b-one".into(),
                choice: V2ChapterBodyChoice::Draft {
                    source_draft_id: "b-old".into(),
                },
            },
        ],
    );
    let error = library.import_v2(foreign).expect_err("foreign choice");
    assert_eq!(error.code, "InvalidBodyChoice");
    let draft = request(
        "choice-draft".into(),
        vec![V2ChapterBodyDecision {
            source_chapter_id: "a-null".into(),
            choice: V2ChapterBodyChoice::Draft {
                source_draft_id: "a-old".into(),
            },
        }],
    );
    let imported = library.import_v2(draft).expect("same-chapter draft choice");
    assert_eq!(imported.chapter_document_ids.len(), 4);
    let draft_path = root
        .join("app")
        .join("Projects")
        .join("project-choice-draft");
    let body_json: String = Connection::open(draft_path.join("project.sqlite3"))
        .expect("open draft import")
        .query_row(
            "SELECT d.body_json FROM documents d JOIN import_id_map m ON m.v3_document_id=d.id
             WHERE m.source_table='chapters' AND m.source_id='a-null'",
            [],
            |row| row.get(0),
        )
        .expect("read imported draft body");
    assert!(body_json.contains("Approved null"));
    assert_eq!(library.list().expect("list library").len(), 1);
    drop(library);
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn incomplete_import_resumes_from_record_after_library_restart_without_source() {
    let source = TempSource::new();
    let preview = preview_v2_import(&source.path, "p-alpha").expect("valid V2 source");
    let root = std::env::temp_dir().join(format!("v2-import-resume-{}", Uuid::new_v4()));
    fs::create_dir(&root).expect("resume root");
    let request = V2ImportRequest {
        operation_id: format!("resume-{}", Uuid::new_v4()),
        source_path: source.path.clone(),
        source_project_id: "p-alpha".into(),
        title: "Resumed Alpha".into(),
        expected_source_sha256: preview.source.source_sha256.clone(),
        choices: vec![V2ChapterBodyDecision {
            source_chapter_id: "a-null".into(),
            choice: V2ChapterBodyChoice::Draft {
                source_draft_id: "a-old".into(),
            },
        }],
    };
    let library_root = root.join("app");
    let mut library = Library::open(&library_root).expect("open library");
    let first = library.import_v2(request.clone()).expect("initial import");
    let imported_path = library
        .operation(&request.operation_id)
        .expect("operation")
        .expect("pending row")
        .final_path;
    drop(library);

    let registry = Connection::open(library_root.join("library.sqlite3")).expect("registry");
    registry
        .execute(
            "UPDATE operations SET completed=0 WHERE operation_id=?",
            [&request.operation_id],
        )
        .expect("simulate lost library acknowledgment");
    drop(registry);
    fs::remove_file(&source.path).expect("source may disappear after staging");

    let mut restarted = Library::open(&library_root).expect("restart library");
    let resumed = restarted
        .resume_v2_import(&request.operation_id)
        .expect("resume retained import");
    assert_eq!(resumed.project.project_id, first.project.project_id);
    assert!(restarted.pending().expect("pending operations").is_empty());
    assert_eq!(restarted.list().expect("library entries").len(), 1);
    let body: String = Connection::open(imported_path.join("project.sqlite3"))
        .expect("imported project")
        .query_row(
            "SELECT d.body_json FROM documents d JOIN import_id_map m ON m.v3_document_id=d.id
             WHERE m.source_table='chapters' AND m.source_id='a-null'",
            [],
            |row| row.get(0),
        )
        .expect("read resumed chapter");
    assert!(body.contains("Approved null"));
    drop(restarted);
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn legacy_incomplete_import_requires_a_new_review() {
    let source = TempSource::new();
    let preview = preview_v2_import(&source.path, "p-alpha").expect("valid V2 source");
    let root = std::env::temp_dir().join(format!("v2-import-legacy-resume-{}", Uuid::new_v4()));
    fs::create_dir(&root).expect("legacy resume root");
    let request = V2ImportRequest {
        operation_id: format!("legacy-{}", Uuid::new_v4()),
        source_path: source.path.clone(),
        source_project_id: "p-alpha".into(),
        title: "Legacy Alpha".into(),
        expected_source_sha256: preview.source.source_sha256.clone(),
        choices: vec![V2ChapterBodyDecision {
            source_chapter_id: "a-null".into(),
            choice: V2ChapterBodyChoice::Empty,
        }],
    };
    let library_root = root.join("app");
    let mut library = Library::open(&library_root).expect("open library");
    let legacy_fingerprint = String::from("legacy-fingerprint");
    library
        .begin(
            &request.operation_id,
            "import",
            &request.title,
            Some((&request.source_path, &legacy_fingerprint)),
        )
        .expect("legacy operation");
    let error = library
        .resume_v2_import(&request.operation_id)
        .expect_err("legacy incomplete request must refuse");
    assert_eq!(error.code, "ImportRecoveryUnavailable");
    drop(library);
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn tampered_sealed_import_staging_is_rejected_before_move() {
    let source = TempSource::new();
    let preview = preview_v2_import(&source.path, "p-alpha").expect("valid V2 source");
    let root = std::env::temp_dir().join(format!("v2-import-tampered-stage-{}", Uuid::new_v4()));
    fs::create_dir(&root).expect("tampered stage root");
    let library_root = root.join("app");
    let mut library = Library::open(&library_root).expect("open library");
    let request = V2ImportRequest {
        operation_id: format!("tampered-{}", Uuid::new_v4()),
        source_path: source.path.clone(),
        source_project_id: "p-alpha".into(),
        title: "Tampered Alpha".into(),
        expected_source_sha256: preview.source.source_sha256.clone(),
        choices: vec![V2ChapterBodyDecision {
            source_chapter_id: "a-null".into(),
            choice: V2ChapterBodyChoice::Empty,
        }],
    };
    let _ = library.import_v2(request.clone()).expect("initial import");
    let pending = library
        .operation(&request.operation_id)
        .expect("operation")
        .expect("operation row");
    let staging = pending.staging_path.clone();
    let final_path = pending.final_path.clone();
    drop(library);
    fs::rename(&final_path, &staging).expect("move installed folder back to staging");
    let connection = Connection::open(staging.join("project.sqlite3")).expect("staging db");
    connection
        .execute(
            "UPDATE import_manifest SET operation_id='other-operation' WHERE singleton=1",
            [],
        )
        .expect("tamper staging manifest");
    drop(connection);
    let registry = Connection::open(library_root.join("library.sqlite3")).expect("registry");
    registry
        .execute(
            "UPDATE operations SET completed=0 WHERE operation_id=?",
            [&request.operation_id],
        )
        .expect("mark operation incomplete");
    drop(registry);

    let mut restarted = Library::open(&library_root).expect("restart library");
    let error = restarted
        .resume_v2_import(&request.operation_id)
        .expect_err("tampered staging must be rejected");
    assert_eq!(error.code, "InvalidProject");
    assert!(
        staging.is_dir(),
        "failed validation must leave staging in place"
    );
    assert!(
        !final_path.exists(),
        "failed validation must not install final folder"
    );
    drop(restarted);
    let _ = fs::remove_dir_all(root);
}
