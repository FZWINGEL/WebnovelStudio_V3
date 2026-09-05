use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::documents::Endpoint;
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectInfo, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::validate_snapshot_json;

struct TempProject(PathBuf);
impl TempProject {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("wns-metadata-{label}-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create temporary project root");
        Self(path)
    }
    fn child(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> Value {
    let mut paragraph = json!({"type":"paragraph","attrs":{"id":"p1"}});
    if !text.is_empty() {
        paragraph["content"] = json!([{"type":"text","text":text}]);
    }
    json!({"schemaVersion":1,"body":{"type":"doc","content":[paragraph]}})
}

fn write_marker(path: &Path, info: &ProjectInfo) {
    fs::write(
        path.join("project.wns.json"),
        serde_json::to_vec_pretty(info).expect("serialize project marker"),
    )
    .expect("write project marker");
}

fn create_v1_project(path: &Path, with_document: bool) -> ProjectInfo {
    fs::create_dir(path).expect("create v1 project folder");
    let connection = Connection::open(path.join("project.sqlite3")).expect("create v1 database");
    connection
        .execute_batch(include_str!("../src/storage/001_projects.sql"))
        .expect("create v1 schema");
    let info = ProjectInfo {
        project_id: format!("v1-project-{}", Uuid::new_v4()),
        operation_namespace: format!("v1-namespace-{}", Uuid::new_v4()),
        title: "Legacy project".into(),
        format_version: 1,
    };
    connection
        .execute(
            "INSERT INTO project(singleton,id,operation_namespace,title,format_version) VALUES(1,?,?,?,1)",
            params![info.project_id, info.operation_namespace, info.title],
        )
        .expect("insert v1 project identity");
    if with_document {
        let snapshot = serde_json::to_string(&body("legacy body")).expect("serialize body");
        let validated = validate_snapshot_json(&snapshot).expect("validate body");
        connection
            .execute(
                "INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash) VALUES('chapter-one','chapter','Legacy chapter',0,0,1,?,?)",
                params![validated.canonical_json, validated.hash],
            )
            .expect("insert v1 document");
    }
    connection
        .pragma_update(None, "user_version", 1)
        .expect("set v1 schema version");
    drop(connection);
    write_marker(path, &info);
    info
}

fn setup_current() -> (
    TempProject,
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let temp = TempProject::new("current");
    let project =
        ProjectSession::create(temp.child("project"), "Current project").expect("create project");
    let access = project
        .attach("metadata-renderer".into())
        .expect("attach renderer");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-document".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("e\u{301}👩‍🚀x"),
        })
        .expect("create document");
    (temp, project, access, document)
}

#[test]
fn scene_break_selection_is_a_valid_saved_view() {
    let (_temp, project, access, document) = setup_current();
    let snapshot = json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"sceneBreak","attrs":{"id":"scene"}}]}});
    let saved = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "scene-body".into(),
            expected: document.head,
            local_generation: "1".into(),
            body: snapshot,
            cause: SaveCause::Typing,
        })
        .unwrap();
    let endpoint = Endpoint {
        block_id: "scene".into(),
        utf16_offset: 0,
    };
    project
        .save_view_state(access.clone(), saved.head, endpoint.clone(), endpoint)
        .unwrap();
    assert!(project.view_state(access).unwrap().is_some());
    let reopened = project.attach_snapshot("scene-reader".into()).unwrap();
    assert_eq!(reopened.view_state.unwrap().anchor.block_id, "scene");
}

#[test]
fn a_trashed_last_document_does_not_prevent_project_opening() {
    let (_temp, project, access, document) = setup_current();
    let endpoint = Endpoint {
        block_id: "p1".into(),
        utf16_offset: 0,
    };
    project
        .save_view_state(access.clone(), document.head, endpoint.clone(), endpoint)
        .unwrap();
    let db = Connection::open(project.path.join("project.sqlite3")).unwrap();
    db.execute("UPDATE documents SET trashed=1 WHERE id='chapter-one'", [])
        .unwrap();
    assert!(project.view_state(access).unwrap().is_none());
    let opened = project
        .attach_snapshot("reopen-after-trash".into())
        .unwrap();
    assert!(opened.documents.is_empty());
    assert!(opened.view_state.is_none());
}

#[test]
fn failed_destination_read_preserves_the_existing_writer_lease() {
    let (_temp, project, access, document) = setup_current();
    let endpoint = Endpoint {
        block_id: "p1".into(),
        utf16_offset: 0,
    };
    project
        .save_view_state(
            access.clone(),
            document.head.clone(),
            endpoint.clone(),
            endpoint,
        )
        .unwrap();
    let db = Connection::open(project.path.join("project.sqlite3")).unwrap();
    db.execute("UPDATE view_state SET anchor_utf16_offset=999", [])
        .unwrap();
    assert!(
        project
            .attach_snapshot("failed-next-renderer".into())
            .is_err()
    );
    let saved = project
        .save(SaveSnapshot {
            access,
            operation_id: "old-editor-still-writes".into(),
            expected: document.head,
            local_generation: "1".into(),
            body: body("The current editor keeps writing."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    assert_eq!(saved.head.version, "1");
}

#[test]
fn v1_upgrade_takes_local_online_backup_and_preserves_data() {
    let temp = TempProject::new("migration");
    let path = temp.child("legacy");
    create_v1_project(&path, true);
    let project = ProjectSession::open(&path).expect("upgrade v1 project");
    assert_eq!(
        project
            .project_metadata()
            .expect("metadata")
            .metadata_version,
        "0"
    );
    drop(project);
    let connection = Connection::open(path.join("project.sqlite3")).expect("open upgraded db");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read upgraded schema version");
    assert_eq!(version, 9);
    let title: String = connection
        .query_row(
            "SELECT title FROM documents WHERE id='chapter-one'",
            [],
            |row| row.get(0),
        )
        .expect("read migrated document");
    assert_eq!(title, "Legacy chapter");
    let backups = fs::read_dir(path.join("migrations"))
        .expect("read migration backups")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let backup = Connection::open(&backups[0]).expect("open pre-upgrade backup");
    let backup_version: i64 = backup
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read backup schema version");
    assert_eq!(backup_version, 1);
    let legacy_body: String = backup
        .query_row(
            "SELECT body_json FROM documents WHERE id='chapter-one'",
            [],
            |row| row.get(0),
        )
        .expect("read backed up body");
    assert!(legacy_body.contains("legacy body"));
}

#[test]
fn v1_upgrade_failure_rolls_back_schema_and_retains_backup() {
    let temp = TempProject::new("migration-failure");
    let path = temp.child("legacy");
    create_v1_project(&path, false);
    let connection = Connection::open(path.join("project.sqlite3")).expect("open v1 database");
    connection
        .execute_batch(
            "CREATE TABLE view_state (singleton INTEGER PRIMARY KEY CHECK(singleton=1)) STRICT;",
        )
        .expect("install conflicting migration table");
    drop(connection);
    let error = ProjectSession::open(&path)
        .err()
        .expect("conflicting migration should fail");
    assert_eq!(error.code, "PersistenceUnavailable");
    let connection = Connection::open(path.join("project.sqlite3")).expect("reopen v1 database");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read rolled back schema version");
    assert_eq!(version, 1);
    let has_epoch: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('project') WHERE name='context_source_epoch'",
            [],
            |row| row.get(0),
        )
        .expect("inspect rolled back project table");
    assert_eq!(has_epoch, 0);
    assert_eq!(fs::read_dir(path.join("migrations")).unwrap().count(), 1);
}

#[test]
fn view_state_round_trips_unicode_endpoints_and_keeps_exact_historic_head() {
    let (temp, project, access, document) = setup_current();
    let state = project
        .save_view_state(
            access.clone(),
            document.head.clone(),
            Endpoint {
                block_id: "p1".into(),
                utf16_offset: 0,
            },
            Endpoint {
                block_id: "p1".into(),
                utf16_offset: 2,
            },
        )
        .expect("save view state");
    assert_eq!(state.head, document.head);
    assert_eq!(project.context_source_epoch().expect("read epoch"), "1");
    let later = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "later-body".into(),
            expected: document.head.clone(),
            local_generation: "2".into(),
            body: body("changed"),
            cause: SaveCause::Typing,
        })
        .expect("save later body");
    assert_ne!(later.head, state.head);
    let stale = project
        .view_state(access.clone())
        .expect("read historic view state")
        .expect("view state exists");
    assert_eq!(stale.head, document.head);
    drop(project);
    let reopened = ProjectSession::open(temp.child("project")).expect("reopen project");
    let reopened_access = reopened
        .attach("metadata-renderer-2".into())
        .expect("attach again");
    assert_eq!(
        reopened
            .view_state(reopened_access)
            .expect("read resumed view")
            .expect("resumed view")
            .anchor,
        state.anchor
    );
}

#[test]
fn metadata_cas_and_context_epoch_distinguish_changes_from_noops() {
    let (temp, project, access, document) = setup_current();
    assert_eq!(
        project
            .project_metadata()
            .expect("initial metadata")
            .metadata_version,
        "0"
    );
    let renamed = project
        .rename_project(access.clone(), "0".into(), "Renamed project".into())
        .expect("rename project");
    assert_eq!(renamed.metadata_version, "1");
    assert_eq!(renamed.project.title, "Renamed project");
    assert_eq!(
        project
            .context_source_epoch()
            .expect("epoch after project rename"),
        "2"
    );
    assert_eq!(
        project
            .rename_project(access.clone(), "0".into(), "Stale rename".into())
            .unwrap_err()
            .code,
        "MetadataConflict"
    );
    let doc_renamed = project
        .rename_document(
            access.clone(),
            "chapter-one".into(),
            "0".into(),
            "Renamed chapter".into(),
        )
        .expect("rename document");
    assert_eq!(doc_renamed.metadata_version, "1");
    assert_eq!(
        project
            .context_source_epoch()
            .expect("epoch after document rename"),
        "3"
    );
    let noop = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "noop-body".into(),
            expected: document.head.clone(),
            local_generation: "3".into(),
            body: document.body.clone(),
            cause: SaveCause::Typing,
        })
        .expect("noop save");
    assert_eq!(noop.head, document.head);
    assert_eq!(
        project.context_source_epoch().expect("epoch after noop"),
        "3"
    );
    let changed = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "body-change".into(),
            expected: document.head.clone(),
            local_generation: "4".into(),
            body: body("changed"),
            cause: SaveCause::Typing,
        })
        .expect("body save");
    assert_eq!(
        project.context_source_epoch().expect("epoch after body"),
        "4"
    );
    let mut undo = SaveSnapshot {
        access: access.clone(),
        operation_id: "undo-change".into(),
        expected: changed.head.clone(),
        local_generation: "5".into(),
        body: document.body.clone(),
        cause: SaveCause::Undo,
    };
    let undone = project.save(undo.clone()).expect("undo save");
    assert_eq!(
        project.context_source_epoch().expect("epoch after undo"),
        "5"
    );
    let checkpoint = project
        .checkpoint(webnovel_core::projects::CheckpointRequest {
            access: access.clone(),
            expected: undone.head.clone(),
            reason: webnovel_core::projects::CheckpointReason::Manual,
        })
        .expect("checkpoint");
    assert!(!checkpoint.id.is_empty());
    assert_eq!(
        project
            .context_source_epoch()
            .expect("epoch after checkpoint"),
        "5"
    );
    undo.operation_id = "failed-change".into();
    let connection =
        Connection::open(temp.child("project/project.sqlite3")).expect("open fault db");
    connection
        .execute_batch("CREATE TRIGGER fail_metadata_epoch BEFORE INSERT ON command_receipts WHEN NEW.operation_id='failed-change' BEGIN SELECT RAISE(ABORT,'test failure'); END;")
        .expect("install save fault");
    assert!(project.save(undo).is_err());
    drop(connection);
    assert_eq!(
        project
            .context_source_epoch()
            .expect("epoch after failed save"),
        "5"
    );
}

#[test]
fn wrong_view_head_or_grapheme_boundary_is_rejected() {
    let (_temp, project, access, document) = setup_current();
    let mut wrong = document.head.clone();
    wrong.body_hash = "0".repeat(64);
    assert_eq!(
        project
            .save_view_state(
                access.clone(),
                wrong,
                Endpoint {
                    block_id: "p1".into(),
                    utf16_offset: 0,
                },
                Endpoint {
                    block_id: "p1".into(),
                    utf16_offset: 0,
                },
            )
            .unwrap_err()
            .code,
        "VersionConflict"
    );
    assert_eq!(
        project
            .save_view_state(
                access,
                document.head,
                Endpoint {
                    block_id: "p1".into(),
                    utf16_offset: 1,
                },
                Endpoint {
                    block_id: "p1".into(),
                    utf16_offset: 1,
                },
            )
            .unwrap_err()
            .code,
        "InvalidRequest"
    );
}

#[test]
fn newer_schema_is_rejected_without_touching_database() {
    let temp = TempProject::new("newer");
    let path = temp.child("project");
    let project = ProjectSession::create(&path, "Newer test").expect("create project");
    drop(project);
    let database = path.join("project.sqlite3");
    let before = fs::read(&database).expect("read database");
    let connection = Connection::open(&database).expect("open database");
    connection
        .pragma_update(None, "user_version", 99)
        .expect("set newer schema");
    drop(connection);
    let error = ProjectSession::open(&path)
        .err()
        .expect("reject newer schema");
    assert_eq!(error.code, "UnsupportedSchema");
    let after = fs::read(&database).expect("read rejected database");
    assert_ne!(before, after, "test version edit should be visible");
    let connection = Connection::open(&database).expect("reopen rejected database");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read newer schema");
    assert_eq!(version, 99);
}
