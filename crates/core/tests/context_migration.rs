use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::documents::Endpoint;
use webnovel_core::projects::story_context::FreezeStory;
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{BackupManifest, create_backup, recover_backup};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("wns-context-migration-{label}-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create temporary test directory");
        Self(path)
    }

    fn child(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "attrs": {"id": "p1"},
                "content": [{"type": "text", "text": text}]
            }]
        }
    })
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_archive(path: &Path, manifest: &BackupManifest, database: &[u8]) {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("create test backup archive");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer
        .start_file("manifest.json", options)
        .expect("write manifest entry");
    writer
        .write_all(&serde_json::to_vec(manifest).expect("serialize manifest"))
        .expect("write manifest");
    writer
        .start_file("project.sqlite3", options)
        .expect("write database entry");
    writer.write_all(database).expect("write database");
    writer.finish().expect("finish backup archive");
}

fn archive_entries(path: &Path) -> (BackupManifest, Vec<u8>) {
    let file = File::open(path).expect("open backup archive");
    let mut archive = ZipArchive::new(file).expect("read backup archive");
    let mut manifest_bytes = Vec::new();
    archive
        .by_name("manifest.json")
        .expect("manifest entry")
        .read_to_end(&mut manifest_bytes)
        .expect("read manifest");
    let manifest = serde_json::from_slice(&manifest_bytes).expect("parse manifest");
    let mut database = Vec::new();
    archive
        .by_name("project.sqlite3")
        .expect("database entry")
        .read_to_end(&mut database)
        .expect("read database");
    (manifest, database)
}

fn setup_project(
    root: &Path,
) -> (
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
    webnovel_core::projects::SaveAck,
) {
    let project = ProjectSession::create(root, "Migration source").expect("create project");
    let access = project
        .attach("migration-session".into())
        .expect("attach project");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-document".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("before schema migration"),
        })
        .expect("create document");
    let saved = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "save-document".into(),
            expected: document.head.clone(),
            local_generation: "1".into(),
            body: body("saved before schema migration"),
            cause: SaveCause::Typing,
        })
        .expect("save document");
    let endpoint = Endpoint {
        block_id: "p1".into(),
        utf16_offset: 5,
    };
    project
        .save_view_state(
            access.clone(),
            saved.head.clone(),
            endpoint.clone(),
            Endpoint {
                block_id: "p1".into(),
                utf16_offset: 17,
            },
        )
        .expect("save view state");
    (project, access, document, saved)
}

/// Convert the current database into the previous schema2 shape. This is a
/// synthetic legacy database used only to exercise the upgrade boundary.
fn downgrade_to_schema2(path: &Path) {
    let database = path.join("project.sqlite3");
    let connection = Connection::open(&database).expect("open current database");
    connection
        .execute_batch(
            "DROP TABLE guidance_request_uses;
             DROP TABLE snapshot_guidance;
             DROP TABLE author_guidance_receipts;
             DROP TABLE author_guidance_heads;
             DROP TABLE author_guidance_versions;
             DROP TABLE discussion_output_events;
             DROP TABLE discussion_messages;
             DROP TABLE discussion_draft_receipts;
             DROP TABLE discussion_runs;
             DROP TABLE discussion_drafts;
             DROP TABLE discussion_threads;
             DROP TABLE context_packets;
             DROP TABLE snapshot_sources;
             DROP TABLE story_snapshots;
             DROP TABLE passage_projections;
             DROP TABLE document_aliases;
             ALTER TABLE project DROP COLUMN disclosure_policy_epoch;
             PRAGMA user_version=2;",
        )
        .expect("downgrade synthetic database to schema2");
    drop(connection);
}

/// Convert the current database into the previous schema3 shape while keeping
/// all frozen story snapshots. This is a synthetic legacy database used only
/// to exercise the context-packet migration boundary.
fn downgrade_to_schema3(path: &Path) {
    let database = path.join("project.sqlite3");
    let connection = Connection::open(&database).expect("open current database");
    connection
        .execute_batch(
            "DROP TABLE guidance_request_uses;
             DROP TABLE snapshot_guidance;
             DROP TABLE author_guidance_receipts;
             DROP TABLE author_guidance_heads;
             DROP TABLE author_guidance_versions;
             DROP TABLE discussion_output_events;
             DROP TABLE discussion_messages;
             DROP TABLE discussion_draft_receipts;
             DROP TABLE discussion_runs;
             DROP TABLE discussion_drafts;
             DROP TABLE discussion_threads;
             DROP TABLE context_packets;
             PRAGMA user_version=3;",
        )
        .expect("downgrade synthetic database to schema3");
    drop(connection);
}

fn schema_version(path: &Path) -> i64 {
    Connection::open(path)
        .expect("open database")
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read schema version")
}

fn source_policy(project: &ProjectSession, access: &ProjectAccess) -> InformationPolicy {
    InformationPolicy {
        version: project
            .context_epochs(access.clone())
            .expect("read policy version")
            .policy,
        audience: Audience::AuthorRoom,
        reader_frontier: None,
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    }
}

fn freeze_one_snapshot(
    project: &ProjectSession,
    access: &ProjectAccess,
    target: &webnovel_core::projects::DocumentRecord,
) -> String {
    project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: "context-migration-freeze".into(),
            expected: target.head.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: source_policy(project, access),
        })
        .expect("freeze story context")
        .snapshot
        .snapshot_id
}

#[test]
fn schema2_upgrade_preserves_documents_view_state_epoch_and_durable_pre_upgrade_backup() {
    let temp = TempDir::new("upgrade");
    let path = temp.child("legacy");
    let (project, access, document, saved) = setup_project(&path);
    let view = project
        .view_state(access.clone())
        .expect("read source view state")
        .expect("view state exists");
    let epoch = project.context_source_epoch().expect("read source epoch");
    drop(project);
    downgrade_to_schema2(&path);
    assert_eq!(schema_version(&path.join("project.sqlite3")), 2);

    let upgraded = ProjectSession::open(&path).expect("upgrade schema2 project");
    assert_eq!(schema_version(&path.join("project.sqlite3")), 7);
    assert_eq!(
        upgraded
            .context_source_epoch()
            .expect("read upgraded epoch"),
        epoch
    );
    let attached = upgraded
        .attach_snapshot("after-upgrade".into())
        .expect("attach upgraded project");
    assert_eq!(attached.documents.len(), 1);
    assert_eq!(attached.documents[0].head, saved.head);
    assert_eq!(
        attached.documents[0].body,
        body("saved before schema migration")
    );
    assert_eq!(attached.view_state, Some(view.clone()));
    assert_eq!(view.head, saved.head);
    assert_eq!(document.head.version, "0");
    drop(upgraded);

    let backups = fs::read_dir(path.join("migrations"))
        .expect("read pre-upgrade backups")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    assert_eq!(schema_version(&backups[0]), 2);
    let backup = Connection::open(&backups[0]).expect("open durable schema2 backup");
    let integrity: String = backup
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("check pre-upgrade backup");
    assert_eq!(integrity, "ok");
    let backed_body: String = backup
        .query_row(
            "SELECT body_json FROM documents WHERE id='chapter-one'",
            [],
            |row| row.get(0),
        )
        .expect("read backed document");
    assert!(backed_body.contains("saved before schema migration"));
    let backed_view: (String, i64, i64) = backup
        .query_row(
            "SELECT document_id,head_version,anchor_utf16_offset FROM view_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read backed view state");
    assert_eq!(backed_view, ("chapter-one".into(), 1, 5));
}

#[test]
fn schema3_upgrade_to_schema7_preserves_frozen_snapshot_and_useful_backup() {
    let temp = TempDir::new("schema3-upgrade");
    let path = temp.child("legacy");
    let (project, access, _document, _saved) = setup_project(&path);
    let target = project
        .document(access.clone(), "chapter-one".into())
        .expect("read current target");
    let snapshot_id = freeze_one_snapshot(&project, &access, &target);
    drop(project);
    downgrade_to_schema3(&path);
    assert_eq!(schema_version(&path.join("project.sqlite3")), 3);

    let upgraded = ProjectSession::open(&path).expect("upgrade schema3 project");
    assert_eq!(schema_version(&path.join("project.sqlite3")), 7);
    let connection =
        Connection::open(path.join("project.sqlite3")).expect("open migrated schema7 database");
    let discussion_tables: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='discussion_runs'",
            [],
            |row| row.get(0),
        )
        .expect("check schema7 discussion tables");
    assert_eq!(discussion_tables, 1);
    drop(connection);
    let attached = upgraded
        .attach_snapshot("schema3-upgrade-reader".into())
        .expect("attach upgraded project");
    let restored = upgraded
        .story_snapshot(attached.access.clone(), snapshot_id.clone())
        .expect("read preserved frozen snapshot");
    assert_eq!(restored.snapshot.snapshot_id, snapshot_id);
    assert_eq!(restored.snapshot.sources.len(), 1);
    assert_eq!(
        restored.snapshot.sources[0].source.document_id,
        "chapter-one"
    );
    assert!(!restored.snapshot.sources[0].source.revision_id.is_empty());
    drop(upgraded);

    let backups = fs::read_dir(path.join("migrations"))
        .expect("read schema3 pre-upgrade backups")
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let backup = Connection::open(backups[0].path()).expect("open schema3 backup");
    let backup_schema: i64 = backup
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read schema3 backup version");
    assert_eq!(backup_schema, 3);
    let backup_snapshot: (i64, String) = backup
        .query_row("SELECT COUNT(*),MIN(id) FROM story_snapshots", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("read backed frozen snapshot");
    assert_eq!(backup_snapshot, (1, snapshot_id));
    let backup_pins: i64 = backup
        .query_row("SELECT COUNT(*) FROM snapshot_sources", [], |row| {
            row.get(0)
        })
        .expect("read backed snapshot pins");
    assert_eq!(backup_pins, 1);
}

#[test]
fn schema2_upgrade_failure_rolls_back_and_retains_durable_backup() {
    let temp = TempDir::new("upgrade-failure");
    let path = temp.child("legacy");
    let (project, _access, _document, saved) = setup_project(&path);
    let epoch = project.context_source_epoch().expect("read source epoch");
    drop(project);
    downgrade_to_schema2(&path);
    let connection = Connection::open(path.join("project.sqlite3")).expect("open schema2 database");
    connection
        .execute_batch(
            "CREATE TABLE story_snapshots (id TEXT PRIMARY KEY);
             PRAGMA user_version=2;",
        )
        .expect("install migration conflict");
    drop(connection);

    let error = ProjectSession::open(&path)
        .err()
        .expect("conflicting later migration must fail");
    assert_eq!(error.code, "PersistenceUnavailable");
    assert_eq!(schema_version(&path.join("project.sqlite3")), 2);
    let connection = Connection::open(path.join("project.sqlite3")).expect("reopen rolled back db");
    let has_disclosure_epoch: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('project') WHERE name='disclosure_policy_epoch'",
            [],
            |row| row.get(0),
        )
        .expect("inspect rolled back schema");
    assert_eq!(has_disclosure_epoch, 0);
    let (version, body_json): (i64, String) = connection
        .query_row(
            "SELECT working_version,body_json FROM documents WHERE id='chapter-one'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read rolled back document");
    assert_eq!(version, 1);
    assert!(body_json.contains("saved before schema migration"));
    let rolled_epoch: i64 = connection
        .query_row(
            "SELECT context_source_epoch FROM project WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .expect("read rolled back epoch");
    assert_eq!(rolled_epoch.to_string(), epoch);
    let view_head: i64 = connection
        .query_row(
            "SELECT head_version FROM view_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .expect("read rolled back view");
    assert_eq!(
        view_head,
        saved.head.version.parse::<i64>().expect("numeric head")
    );
    let backup = path
        .join("migrations")
        .read_dir()
        .expect("read migration dir")
        .next()
        .expect("backup entry")
        .expect("backup path");
    assert_eq!(schema_version(&backup.path()), 2);
}

#[test]
fn schema2_backup_recovers_forward_to_schema7_with_document_view_and_epoch() {
    let temp = TempDir::new("schema2-recovery");
    let source_path = temp.child("source");
    let (project, access, _document, saved) = setup_project(&source_path);
    let view = project
        .view_state(access.clone())
        .expect("read source view")
        .expect("source view exists");
    let epoch = project.context_source_epoch().expect("read source epoch");
    let current_archive = temp.child("current.wnsbackup");
    let current_manifest =
        create_backup(&project, &current_archive).expect("capture current manifest");
    drop(project);
    downgrade_to_schema2(&source_path);
    let database = fs::read(source_path.join("project.sqlite3")).expect("read schema2 database");
    let mut manifest = current_manifest;
    manifest.database_schema_version = 2;
    manifest.database_sha256 = sha256(&database);
    let archive = temp.child("schema2.wnsbackup");
    write_archive(&archive, &manifest, &database);

    let target = temp.child("recovered");
    let recovered =
        recover_backup(&archive, &target, "Recovered schema2").expect("recover schema2 backup");
    assert_eq!(schema_version(&target.join("project.sqlite3")), 7);
    assert_eq!(
        recovered
            .context_source_epoch()
            .expect("read recovered epoch"),
        epoch
    );
    let attached = recovered
        .attach_snapshot("schema2-recovery-reader".into())
        .expect("attach recovered project");
    assert_eq!(attached.documents[0].head, saved.head);
    assert_eq!(
        attached.documents[0].body,
        body("saved before schema migration")
    );
    assert_eq!(attached.view_state, Some(view));
    let migration_backups = fs::read_dir(target.join("migrations"))
        .expect("read recovered migration backups")
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    assert_eq!(migration_backups.len(), 1);
    assert_eq!(schema_version(&migration_backups[0].path()), 2);
}

#[test]
fn create_backup_rejects_malformed_persisted_context_manifest() {
    let temp = TempDir::new("malformed-manifest");
    let path = temp.child("source");
    let (project, access, _document, _saved) = setup_project(&path);
    let target = project
        .document(access.clone(), "chapter-one".into())
        .expect("read current target");
    let snapshot_id = freeze_one_snapshot(&project, &access, &target);
    drop(project);
    let connection = Connection::open(path.join("project.sqlite3")).expect("open source db");
    connection
        .execute_batch("DROP TRIGGER immutable_story_snapshot_update;")
        .expect("disable immutable manifest trigger for synthetic sabotage");
    connection
        .execute(
            "UPDATE story_snapshots SET manifest_json='{}' WHERE id=?",
            params![snapshot_id],
        )
        .expect("corrupt persisted context manifest");
    drop(connection);
    let reopened = ProjectSession::open(&path).expect("open source with persisted corruption");
    let target = temp.child("should-not-exist.wnsbackup");
    let error = create_backup(&reopened, &target)
        .expect_err("malformed persisted context must block backup");
    assert_eq!(error.code, "InvalidBackup");
    assert!(!target.exists());
}

#[test]
fn recovery_rejects_malformed_persisted_context_pin_without_installing_target() {
    let temp = TempDir::new("malformed-pin");
    let source_path = temp.child("source");
    let (project, access, _document, _saved) = setup_project(&source_path);
    let target_document = project
        .document(access.clone(), "chapter-one".into())
        .expect("read current target");
    freeze_one_snapshot(&project, &access, &target_document);
    let valid_archive = temp.child("valid.wnsbackup");
    create_backup(&project, &valid_archive).expect("create valid context backup");
    drop(project);

    let (mut manifest, mut database) = archive_entries(&valid_archive);
    let file_database = temp.child("pin-sabotaged.sqlite3");
    fs::write(&file_database, &database).expect("write scratch database");
    let connection = Connection::open(&file_database).expect("open scratch database");
    connection
        .execute_batch("DROP TRIGGER immutable_snapshot_source_update;")
        .expect("disable immutable pin trigger for synthetic sabotage");
    connection
        .execute("UPDATE snapshot_sources SET body_hash=?", ["0".repeat(64)])
        .expect("sabotage persisted source pin");
    drop(connection);
    database = fs::read(&file_database).expect("read sabotaged database");
    manifest.database_sha256 = sha256(&database);
    let bad_archive = temp.child("bad-pin.wnsbackup");
    write_archive(&bad_archive, &manifest, &database);

    let target = temp.child("should-not-install");
    let error = recover_backup(&bad_archive, &target, "Bad pin")
        .err()
        .expect("malformed persisted pin must block recovery");
    assert_eq!(error.code, "InvalidBackup");
    assert!(!target.exists());
}
