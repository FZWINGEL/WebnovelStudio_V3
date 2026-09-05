use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::projects::{
    CreateDocument, CreationOrigin, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
    read_creation_origin,
};
use webnovel_core::transfer::{
    BackupManifest, DuplicateBasis, ExportManifest, capture_duplicate_basis, create_backup,
    duplicate_project, duplicate_project_staged_with_basis, duplicate_project_with_basis,
    export_draft_txt, recover_backup, recover_backup_staged,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

struct TempDir(PathBuf);
impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("wns-transfer-{label}-{}", Uuid::new_v4()));
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

fn body(content: Value) -> Value {
    json!({"schemaVersion": 1, "body": {"type": "doc", "content": content}})
}
fn paragraph(id: &str, content: Value) -> Value {
    json!({"type": "paragraph", "attrs": {"id": id}, "content": content})
}
fn text(value: &str) -> Value {
    json!({"type": "text", "text": value})
}
fn open_project(
    root: &Path,
) -> (
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let project = ProjectSession::create(root, "Transfer test").expect("create project");
    let access = project
        .attach("transfer-session".into())
        .expect("attach project");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-document".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body(json!([paragraph("p", json!([text("initial")]))])),
        })
        .expect("create document");
    (project, access, document)
}
fn save(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    operation: &str,
    body: Value,
) -> webnovel_core::projects::SaveAck {
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: operation.into(),
            expected: document.head.clone(),
            local_generation: "1".into(),
            body,
            cause: SaveCause::Typing,
        })
        .expect("save document")
}
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn archive_entries(path: &Path) -> (BackupManifest, Vec<u8>) {
    let file = File::open(path).expect("open backup");
    let mut archive = ZipArchive::new(file).expect("read backup");
    assert_eq!(archive.len(), 2);
    let mut manifest_bytes = Vec::new();
    archive
        .by_name("manifest.json")
        .expect("manifest entry")
        .read_to_end(&mut manifest_bytes)
        .expect("read manifest");
    let manifest: BackupManifest = serde_json::from_slice(&manifest_bytes).expect("parse manifest");
    let mut database = Vec::new();
    archive
        .by_name("project.sqlite3")
        .expect("database entry")
        .read_to_end(&mut database)
        .expect("read database");
    (manifest, database)
}
fn write_archive(path: &Path, manifest: &[u8], database: &[u8], extra: Option<&str>) {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("create test archive");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("manifest.json", options)
        .expect("manifest entry");
    zip.write_all(manifest).expect("manifest bytes");
    zip.start_file("project.sqlite3", options)
        .expect("database entry");
    zip.write_all(database).expect("database bytes");
    if let Some(name) = extra {
        zip.start_file(name, options).expect("extra entry");
        zip.write_all(b"unexpected").expect("extra bytes");
    }
    zip.finish().expect("finish test archive");
}

#[test]
fn online_backup_captures_file_backed_wal_and_actor_can_save_afterward() {
    let temp = TempDir::new("online");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let first = save(
        &project,
        &access,
        &document,
        "save-one",
        body(json!([paragraph("p", json!([text("first")]))])),
    );
    assert!(project.path.join("project.sqlite3").exists());
    let wal = project.path.join("project.sqlite3-wal");
    assert!(wal.exists(), "the live project should have an active WAL");
    assert!(
        fs::metadata(&wal).expect("WAL metadata").len() > 0,
        "the live WAL should contain uncheckpointed writes"
    );
    let backup_path = temp.child("first.wnsbackup");
    let manifest = create_backup(&project, &backup_path).expect("online backup");
    assert_eq!(manifest.source_project_id, project.info.project_id);
    assert_eq!(
        manifest.source_operation_namespace,
        project.info.operation_namespace
    );
    assert_eq!(manifest.assets.len(), 0);
    assert_eq!(
        manifest.database_sha256,
        sha256(&archive_entries(&backup_path).1)
    );
    let current = project
        .document(access.clone(), first.head.document_id.clone())
        .expect("read current");
    let second = save(
        &project,
        &access,
        &current,
        "save-two",
        body(json!([paragraph("p", json!([text("second")]))])),
    );
    assert_eq!(second.head.version, "2");
}

#[test]
fn recover_creates_new_identity_and_old_receipts_are_historical() {
    let temp = TempDir::new("recover");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let ack = save(
        &project,
        &access,
        &document,
        "reused-operation",
        body(json!([paragraph("p", json!([text("saved")]))])),
    );
    project
        .save_view_state(
            access.clone(),
            ack.head.clone(),
            webnovel_core::documents::Endpoint {
                block_id: "p".into(),
                utf16_offset: 0,
            },
            webnovel_core::documents::Endpoint {
                block_id: "p".into(),
                utf16_offset: 5,
            },
        )
        .expect("save source view state");
    let source_epoch = project.context_source_epoch().expect("source epoch");
    let backup = temp.child("project.wnsbackup");
    create_backup(&project, &backup).expect("create backup");
    let source_marker = fs::read(project.path.join("project.wns.json")).expect("source marker");
    let recovered_path = temp.child("recovered");
    let recovered =
        recover_backup(&backup, &recovered_path, "Recovered transfer").expect("recover backup");
    assert_ne!(recovered.info.project_id, project.info.project_id);
    assert_ne!(
        recovered.info.operation_namespace,
        project.info.operation_namespace
    );
    assert_eq!(
        fs::read(project.path.join("project.wns.json")).expect("source marker after recover"),
        source_marker
    );
    let recovered_access = recovered
        .attach("recovered-session".into())
        .expect("attach recovered");
    assert_eq!(
        recovered.context_source_epoch().expect("recovered epoch"),
        source_epoch
    );
    assert!(
        recovered
            .view_state(recovered_access.clone())
            .expect("read recovered view")
            .is_some()
    );
    let reconciled = recovered
        .reconcile(webnovel_core::projects::ReconcileRequest {
            project_id: recovered.info.project_id.clone(),
            operation_namespace: recovered.info.operation_namespace.clone(),
            session: recovered_access.session.clone(),
            document_id: ack.head.document_id.clone(),
            pending_operation_ids: vec!["reused-operation".into()],
        })
        .expect("reconcile recovered");
    assert!(reconciled.receipts.is_empty());
    let reused = recovered
        .save(SaveSnapshot {
            access: reconciled.access.clone(),
            operation_id: "reused-operation".into(),
            expected: reconciled.document.head,
            local_generation: "2".into(),
            body: body(json!([paragraph("p", json!([text("new recovered text")]))])),
            cause: SaveCause::Typing,
        })
        .expect("reuse operation ID in recovered namespace");
    assert_eq!(reused.head.version, "2");
}

#[test]
fn staged_recovery_retry_reopens_completed_destination_by_creation_origin() {
    let temp = TempDir::new("recover-retry");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    save(
        &project,
        &access,
        &document,
        "retry-source",
        body(json!([paragraph("p", json!([text("retry")]))])),
    );
    let backup = temp.child("retry.wnsbackup");
    create_backup(&project, &backup).expect("create retry backup");
    let staging = temp.child(".retry-stage");
    let target = temp.child("retry-target");
    let origin = CreationOrigin {
        operation_namespace: "retry-namespace".into(),
        operation_id: "retry-operation".into(),
    };
    let first = webnovel_core::transfer::recover_backup_staged(
        &backup,
        &staging,
        &target,
        "Retry target",
        &origin,
    )
    .expect("install retry target");
    let first_id = first.info.project_id.clone();
    drop(first);
    assert!(target.exists());
    let resumed = webnovel_core::transfer::recover_backup_staged(
        &temp.child("archive-already-consumed"),
        &staging,
        &target,
        "Retry target",
        &origin,
    )
    .expect("reopen completed target");
    assert_eq!(resumed.info.project_id, first_id);
    assert!(!staging.exists());
    drop(resumed);
    assert_eq!(
        read_creation_origin(&target).expect("target creation origin"),
        origin
    );
}

#[test]
fn staged_recovery_resumes_prepared_folder_with_matching_creation_origin() {
    let temp = TempDir::new("recover-stage");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    save(
        &project,
        &access,
        &document,
        "stage-source",
        body(json!([paragraph("p", json!([text("prepared")]))])),
    );
    let backup = temp.child("stage.wnsbackup");
    create_backup(&project, &backup).expect("create staging backup");
    let initial_staging = temp.child(".prepared-initial");
    let initial_target = temp.child("prepared-initial-target");
    let origin = CreationOrigin {
        operation_namespace: "stage-namespace".into(),
        operation_id: "stage-operation".into(),
    };
    let prepared = webnovel_core::transfer::recover_backup_staged(
        &backup,
        &initial_staging,
        &initial_target,
        "Prepared target",
        &origin,
    )
    .expect("prepare staging project");
    drop(prepared);
    fs::rename(&initial_target, temp.child(".prepared-resume"))
        .expect("move completed folder into retry staging");
    let staging = temp.child(".prepared-resume");
    let target = temp.child("prepared-target");
    let resumed = webnovel_core::transfer::recover_backup_staged(
        &backup,
        &staging,
        &target,
        "Prepared target",
        &origin,
    )
    .expect("resume prepared staging project");
    let resumed_access = resumed
        .attach("resume-session".into())
        .expect("attach resumed project");
    let resumed_document = resumed
        .document(resumed_access, "chapter-one".into())
        .expect("read resumed document");
    assert_eq!(
        resumed_document
            .body
            .pointer("/body/content/0/content/0/text")
            .and_then(Value::as_str),
        Some("prepared")
    );
    assert!(!staging.exists());
    drop(resumed);
    assert!(target.exists());
}

#[test]
fn duplicate_uses_consistent_copy_and_preserves_source() {
    let temp = TempDir::new("duplicate");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    save(
        &project,
        &access,
        &document,
        "save-duplicate",
        body(json!([paragraph("p", json!([text("source")]))])),
    );
    let duplicate_path = temp.child("duplicate");
    let duplicate = duplicate_project(&project, &duplicate_path, "Duplicate transfer")
        .expect("duplicate project");
    assert_ne!(duplicate.info.project_id, project.info.project_id);
    let source_doc = project
        .document(access, "chapter-one".into())
        .expect("source remains readable");
    let duplicate_access = duplicate
        .attach("duplicate-session".into())
        .expect("attach duplicate");
    let duplicate_doc = duplicate
        .document(duplicate_access, "chapter-one".into())
        .expect("duplicate body");
    assert_eq!(source_doc.body, duplicate_doc.body);
    assert!(project.path.join("project.wns.json").exists());
}

#[test]
fn duplicate_expected_basis_rejects_a_changed_epoch_or_head_before_install() {
    let temp = TempDir::new("duplicate-basis");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let saved = save(
        &project,
        &access,
        &document,
        "save-basis",
        body(json!([paragraph("p", json!([text("basis")]))])),
    );
    let metadata = project.project_metadata().expect("read source identity");
    let basis = DuplicateBasis {
        project_id: metadata.project.project_id,
        operation_namespace: metadata.project.operation_namespace,
        context_source_epoch: project.context_source_epoch().expect("read source epoch"),
        document_heads: vec![saved.head.clone()],
    };
    let duplicate =
        duplicate_project_with_basis(&project, &temp.child("basis-copy"), "Basis copy", &basis)
            .expect("duplicate matching frozen basis");
    assert_ne!(duplicate.info.project_id, project.info.project_id);
    drop(duplicate);
    let mut stale = basis;
    stale.context_source_epoch = "0".into();
    let error = duplicate_project_with_basis(
        &project,
        &temp.child("stale-copy"),
        "Stale basis copy",
        &stale,
    )
    .err()
    .expect("stale duplicate basis must reject");
    assert_eq!(error.code, "VersionConflict");
    assert!(!temp.child("stale-copy").exists());
}

#[test]
fn duplicate_basis_retry_resumes_matching_prepared_stage_before_live_source_read() {
    let temp = TempDir::new("duplicate-stage-retry");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let basis = capture_duplicate_basis(&project, access.clone()).expect("capture basis");
    let backup = temp.child("prepared-source.wnsbackup");
    create_backup(&project, &backup).expect("create prepared backup");
    let origin = CreationOrigin {
        operation_namespace: "prepared-duplicate-namespace".into(),
        operation_id: "prepared-duplicate-operation".into(),
    };
    let prepared_target = temp.child("prepared-target");
    let prepared = recover_backup_staged(
        &backup,
        &temp.child("prepared-stage-initial"),
        &prepared_target,
        "Prepared duplicate",
        &origin,
    )
    .expect("prepare duplicate folder");
    drop(prepared);
    let staging = temp.child(".prepared-duplicate-stage");
    fs::rename(&prepared_target, &staging).expect("move prepared folder into stage");
    let current = project
        .document(access.clone(), document.head.document_id.clone())
        .expect("read current source");
    save(
        &project,
        &access,
        &current,
        "source-changed-after-stage",
        body(json!([paragraph("p", json!([text("changed")]))])),
    );
    let target = temp.child("duplicate-after-retry");
    let duplicate = duplicate_project_staged_with_basis(
        &project,
        &staging,
        &target,
        "Prepared duplicate",
        &origin,
        &basis,
    )
    .expect("resume prepared duplicate without recapturing source");
    let duplicate_access = duplicate
        .attach("prepared-duplicate-session".into())
        .expect("attach duplicate");
    let duplicate_document = duplicate
        .document(duplicate_access, "chapter-one".into())
        .expect("read prepared duplicate");
    assert_eq!(
        duplicate_document
            .body
            .pointer("/body/content/0/content/0/text")
            .and_then(Value::as_str),
        Some("initial")
    );
    assert!(!staging.exists());
}

#[test]
fn invalid_archive_hash_schema_and_entries_leave_no_target() {
    let temp = TempDir::new("invalid");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    save(
        &project,
        &access,
        &document,
        "bad-archive-source",
        body(json!([paragraph("p", json!([text("source")]))])),
    );
    let backup = temp.child("valid.wnsbackup");
    create_backup(&project, &backup).expect("create valid backup");
    let (manifest, database) = archive_entries(&backup);
    let manifest_bytes = serde_json::to_vec(&manifest).expect("manifest bytes");
    let mut corrupted_database = database.clone();
    let corruption_index = corrupted_database.len() / 2;
    corrupted_database[corruption_index] ^= 0x01;
    let bad_hash = temp.child("bad-hash.wnsbackup");
    write_archive(&bad_hash, &manifest_bytes, &corrupted_database, None);
    let missing = temp.child("missing-hash");
    let error = recover_backup(&bad_hash, &missing, "Bad hash")
        .err()
        .expect("hash mismatch must reject");
    assert_eq!(error.code, "InvalidBackup");
    assert!(!missing.exists());
    let mut wrong_schema = manifest.clone();
    wrong_schema.database_schema_version = 99;
    let bad_schema = temp.child("bad-schema.wnsbackup");
    write_archive(
        &bad_schema,
        &serde_json::to_vec(&wrong_schema).expect("schema manifest"),
        &database,
        None,
    );
    let schema_error = recover_backup(&bad_schema, &temp.child("missing-schema"), "Bad schema")
        .err()
        .expect("schema mismatch must reject");
    assert_eq!(schema_error.code, "UnsupportedSchema");
    let extra = temp.child("extra.wnsbackup");
    write_archive(&extra, &manifest_bytes, &database, Some("../escape"));
    let extra_error = recover_backup(&extra, &temp.child("missing-extra"), "Extra")
        .err()
        .expect("unexpected entry must reject");
    assert_eq!(extra_error.code, "InvalidBackup");
}

#[test]
fn manifest_document_ownership_mismatch_is_rejected_without_completed_copy() {
    let temp = TempDir::new("ownership");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    save(
        &project,
        &access,
        &document,
        "ownership-source",
        body(json!([paragraph("p", json!([text("source")]))])),
    );
    let backup = temp.child("valid.wnsbackup");
    create_backup(&project, &backup).expect("create backup");
    let (mut manifest, database) = archive_entries(&backup);
    manifest.source_project_id = "other-project".into();
    let bad = temp.child("bad-owner.wnsbackup");
    write_archive(
        &bad,
        &serde_json::to_vec(&manifest).expect("owner manifest"),
        &database,
        None,
    );
    let target = temp.child("should-not-install");
    let error = recover_backup(&bad, &target, "Ownership")
        .err()
        .expect("ownership mismatch must reject");
    assert_eq!(error.code, "InvalidBackup");
    assert!(!target.exists());
    assert!(project.path.exists());
}

#[test]
fn export_is_explicit_utf8_plain_text_with_scene_and_hard_break_projection() {
    let temp = TempDir::new("export");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let content = body(json!([
        paragraph("p", json!([
            text("A é🙂"),
            {"type": "hardBreak"},
            {"type": "text", "text": "B", "marks": [{"type": "bold"}]}
        ])),
        {"type": "sceneBreak", "attrs": {"id": "scene"}},
        paragraph("tail", json!([{"type": "text", "text": "C", "marks": [{"type": "link", "attrs": {"href": "https://example.com"}}]}]))
    ]));
    let ack = save(&project, &access, &document, "save-export", content);
    let output = temp.child("draft.txt");
    let manifest: ExportManifest =
        export_draft_txt(&project, access, ack.head.clone(), &output).expect("export draft");
    let bytes = fs::read(&output).expect("read exported UTF-8");
    assert_eq!(
        String::from_utf8(bytes.clone()).expect("valid UTF-8"),
        "A é🙂\nB\n\n[Scene break]\n\nC"
    );
    assert_eq!(manifest.utf8_bytes, bytes.len() as u64);
    assert_eq!(manifest.sha256, sha256(&bytes));
    assert!(manifest.format_loss.contains("omitted"));
    assert_eq!(manifest.source_head, ack.head);
}

#[test]
fn schema1_backup_is_migrated_during_recovery_and_keeps_empty_view_defaults() {
    let temp = TempDir::new("schema1-recovery");
    let source = temp.child("source");
    let project = ProjectSession::create(&source, "Schema one source").expect("create source");
    let metadata = project.project_metadata().expect("read source metadata");
    drop(project);
    let database_path = source.join("project.sqlite3");
    let connection = Connection::open(&database_path).expect("open source database");
    connection
        .execute_batch(
            "DROP TABLE view_state; ALTER TABLE project DROP COLUMN context_source_epoch;",
        )
        .expect("downgrade synthetic schema");
    connection
        .pragma_update(None, "user_version", 1)
        .expect("set schema one");
    drop(connection);
    let database = fs::read(&database_path).expect("read schema one database");
    let manifest = BackupManifest {
        format_version: 1,
        source_project_id: metadata.project.project_id,
        source_operation_namespace: metadata.project.operation_namespace,
        database_sha256: sha256(&database),
        database_schema_version: 1,
        context_source_epoch: "0".into(),
        documents: Vec::new(),
        revisions: Vec::new(),
        assets: Vec::new(),
    };
    let archive = temp.child("schema1.wnsbackup");
    write_archive(
        &archive,
        &serde_json::to_vec(&manifest).expect("schema one manifest"),
        &database,
        None,
    );
    let recovered_path = temp.child("schema2-recovered");
    let recovered = recover_backup(&archive, &recovered_path, "Migrated copy")
        .expect("recover schema one backup");
    assert_eq!(
        recovered
            .context_source_epoch()
            .expect("read migrated epoch"),
        "0"
    );
    assert!(
        recovered
            .view_state(recovered.attach("schema1-session".into()).expect("attach"))
            .expect("read migrated view state")
            .is_none()
    );
    drop(recovered);
    let connection = Connection::open(recovered_path.join("project.sqlite3"))
        .expect("open recovered schema two database");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read recovered schema");
    assert_eq!(version, 2);
}
