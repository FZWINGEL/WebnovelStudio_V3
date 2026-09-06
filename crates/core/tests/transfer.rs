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
    BackupManifest, DraftFormat, DuplicateBasis, capture_duplicate_basis, create_backup,
    duplicate_project, duplicate_project_staged_with_basis, duplicate_project_with_basis,
    export_prepared_draft, prepare_draft_export, recover_backup, recover_backup_staged,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

#[path = "support/schema.rs"]
mod legacy_schema;

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
    let preview = prepare_draft_export(&project, &access, ack.head.clone(), DraftFormat::PlainText)
        .expect("prepare draft");
    assert!(preview.format_loss.contains("omitted"));
    let manifest = export_prepared_draft(&project, access, preview, &output).expect("export draft");
    let bytes = fs::read(&output).expect("read exported UTF-8");
    assert_eq!(
        String::from_utf8(bytes.clone()).expect("valid UTF-8"),
        "A é🙂\nB\n\n[Scene break]\n\nC"
    );
    assert_eq!(manifest.utf8_bytes, bytes.len() as u64);
    assert_eq!(manifest.sha256, sha256(&bytes));
    assert_eq!(manifest.source_head, ack.head);
}

#[test]
fn prepared_plain_text_and_markdown_exports_have_exact_preview_and_record() {
    let temp = TempDir::new("prepared-export");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let content = body(json!([
        {"type":"heading","attrs":{"id":"title","level":2},"content":[text("Title")]},
        paragraph("p", json!([
            text("A "),
            {"type":"text","text":"bold","marks":[{"type":"bold"}]},
            text(" and "),
            {"type":"text","text":"site","marks":[{"type":"link","attrs":{"href":"https://example.com/a_(b)"}}]},
            {"type":"hardBreak"},
            {"type":"text","text":"next","marks":[{"type":"italic"},{"type":"bold"}]}
        ])),
        {"type":"sceneBreak","attrs":{"id":"scene"}}
    ]));
    let saved = save(
        &project,
        &access,
        &document,
        "prepared-export-save",
        content,
    );

    let plain = prepare_draft_export(
        &project,
        &access,
        saved.head.clone(),
        DraftFormat::PlainText,
    )
    .expect("prepare plain export");
    assert_eq!(
        plain.preview_text,
        "Title\n\nA bold and site\nnext\n\n[Scene break]"
    );
    assert_eq!(plain.utf8_bytes, plain.preview_text.len() as u64);
    assert_eq!(plain.format, DraftFormat::PlainText);
    let plain_target = temp.child("draft.txt");
    let plain_record =
        export_prepared_draft(&project, access.clone(), plain.clone(), &plain_target)
            .expect("install plain export");
    assert_eq!(
        fs::read_to_string(&plain_target).unwrap(),
        plain.preview_text
    );
    assert!(plain_record.working_draft);
    assert_eq!(plain_record.basename, "draft.txt");
    let retry = export_prepared_draft(&project, access.clone(), plain.clone(), &plain_target)
        .expect_err("a recorded export must not install a duplicate file");
    assert_eq!(retry.code, "ExportAlreadyRecorded");

    let markdown = prepare_draft_export(&project, &access, saved.head, DraftFormat::Markdown)
        .expect("prepare markdown export");
    assert_eq!(
        markdown.preview_text,
        "## Title\n\nA **bold** and [site](<https://example.com/a_(b)>)  \n***next***\n\n---"
    );
    assert!(!markdown.preview_text.ends_with('\n'));
    let markdown_target = temp.child("draft.md");
    let markdown_record =
        export_prepared_draft(&project, access.clone(), markdown.clone(), &markdown_target)
            .expect("install markdown export");
    assert_eq!(
        fs::read_to_string(&markdown_target).unwrap(),
        markdown.preview_text
    );
    assert_eq!(markdown_record.format, DraftFormat::Markdown);
    assert_eq!(markdown_record.sha256, markdown.sha256);

    drop(project);
    let reopened = ProjectSession::open(&source).expect("reopen exported project");
    let reopened_access = reopened
        .attach("export-reader".into())
        .expect("attach export reader");
    assert_eq!(
        reopened.read_export_record(reopened_access, markdown.id),
        Ok(markdown_record)
    );
}

#[test]
fn export_uses_frozen_revision_after_a_later_working_edit() {
    let temp = TempDir::new("frozen-export");
    let (project, access, document) = open_project(&temp.child("source"));
    let saved = save(
        &project,
        &access,
        &document,
        "frozen-export-save",
        body(json!([paragraph("p", json!([text("frozen source")]))])),
    );
    let preview =
        prepare_draft_export(&project, &access, saved.head.clone(), DraftFormat::Markdown)
            .expect("prepare frozen export");
    let current = project
        .document(access.clone(), "chapter-one".into())
        .unwrap();
    let later = save(
        &project,
        &access,
        &current,
        "frozen-export-later",
        body(json!([paragraph("p", json!([text("later working edit")]))])),
    );
    assert_ne!(later.head, preview.source_head);
    let target = temp.child("frozen.md");
    export_prepared_draft(&project, access.clone(), preview.clone(), &target)
        .expect("export retained revision");
    assert_eq!(fs::read_to_string(target).unwrap(), "frozen source");
    assert_eq!(
        project.document(access, "chapter-one".into()).unwrap().head,
        later.head
    );
}

#[test]
fn tampered_foreign_or_missing_export_source_is_rejected_before_file_creation() {
    let temp = TempDir::new("export-rejections");
    let (project, access, document) = open_project(&temp.child("source"));
    let saved = save(
        &project,
        &access,
        &document,
        "rejection-save",
        body(json!([paragraph("p", json!([text("source")]))])),
    );
    let preview = prepare_draft_export(&project, &access, saved.head, DraftFormat::PlainText)
        .expect("prepare source export");

    let mut tampered = preview.clone();
    tampered.preview_text.push('!');
    let tampered_target = temp.child("tampered.txt");
    let error = export_prepared_draft(&project, access.clone(), tampered, &tampered_target)
        .expect_err("tampered preview must be refused");
    assert_eq!(error.code, "ExportPreviewMismatch");
    assert!(!tampered_target.exists());

    let mut missing = preview.clone();
    missing.revision_id = "missing-revision".into();
    let missing_target = temp.child("missing.txt");
    let error = export_prepared_draft(&project, access.clone(), missing, &missing_target)
        .expect_err("missing revision must be refused");
    assert_eq!(error.code, "RevisionNotFound");
    assert!(!missing_target.exists());

    let other_root = temp.child("other");
    let (other, other_access, other_document) = open_project(&other_root);
    let other_saved = save(
        &other,
        &other_access,
        &other_document,
        "foreign-save",
        body(json!([paragraph("p", json!([text("foreign")]))])),
    );
    let foreign = prepare_draft_export(
        &other,
        &other_access,
        other_saved.head,
        DraftFormat::PlainText,
    )
    .expect("prepare foreign export");
    let foreign_target = temp.child("foreign.txt");
    let error = export_prepared_draft(&project, access, foreign, &foreign_target)
        .expect_err("foreign preview must be refused");
    assert_eq!(error.code, "WrongProjectSession");
    assert!(!foreign_target.exists());
}

#[test]
fn export_refuses_existing_or_in_project_destinations() {
    let temp = TempDir::new("export-targets");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let saved = save(
        &project,
        &access,
        &document,
        "target-save",
        body(json!([paragraph("p", json!([text("source")]))])),
    );
    let inside = source.join("inside.txt");
    let preview = prepare_draft_export(
        &project,
        &access,
        saved.head.clone(),
        DraftFormat::PlainText,
    )
    .expect("prepare in-project test");
    let error = export_prepared_draft(&project, access.clone(), preview, &inside)
        .expect_err("in-project destination must be refused");
    assert_eq!(error.code, "InvalidRequest");

    let existing = temp.child("existing.txt");
    fs::write(&existing, "keep").unwrap();
    let preview = prepare_draft_export(&project, &access, saved.head, DraftFormat::PlainText)
        .expect("prepare existing target test");
    let error = export_prepared_draft(&project, access, preview, &existing)
        .expect_err("existing destination must be refused");
    assert_eq!(error.code, "TargetExists");
    assert_eq!(fs::read_to_string(existing).unwrap(), "keep");
}

#[test]
fn export_record_failure_keeps_installed_output_and_concurrent_retry_cannot_write_twice() {
    let temp = TempDir::new("export-record-boundary");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let saved = save(
        &project,
        &access,
        &document,
        "record-failure-save",
        body(json!([paragraph("p", json!([text("source")]))])),
    );
    let preview = prepare_draft_export(&project, &access, saved.head, DraftFormat::PlainText)
        .expect("prepare record failure export");
    let connection = Connection::open(source.join("project.sqlite3")).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_export_record BEFORE INSERT ON export_records
             BEGIN SELECT RAISE(ABORT,'test export record failure'); END;",
        )
        .unwrap();
    drop(connection);
    let failed_target = temp.child("record-failure.txt");
    let error = export_prepared_draft(&project, access.clone(), preview.clone(), &failed_target)
        .expect_err("record failure must be explicit");
    assert_eq!(error.code, "ExportRecordUnavailable");
    assert!(failed_target.exists());
    let count: i64 = Connection::open(source.join("project.sqlite3"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM export_records", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);

    let connection = Connection::open(source.join("project.sqlite3")).unwrap();
    connection
        .execute_batch("DROP TRIGGER fail_export_record;")
        .unwrap();
    drop(connection);
    let first_target = temp.child("concurrent-a.txt");
    let second_target = temp.child("concurrent-b.txt");
    let first_project = project.clone();
    let first_access = access.clone();
    let first_preview = preview.clone();
    let second_project = project.clone();
    let second_access = access.clone();
    let second_preview = preview;
    let (first, second) = std::thread::scope(|scope| {
        let left = scope.spawn(|| {
            export_prepared_draft(&first_project, first_access, first_preview, &first_target)
        });
        let right = scope.spawn(|| {
            export_prepared_draft(
                &second_project,
                second_access,
                second_preview,
                &second_target,
            )
        });
        (left.join().unwrap(), right.join().unwrap())
    });
    let successes = usize::from(first.is_ok()) + usize::from(second.is_ok());
    assert_eq!(successes, 1);
    let rejected = [first.as_ref().err(), second.as_ref().err()]
        .into_iter()
        .flatten()
        .next()
        .expect("one concurrent finalization must be rejected");
    assert_eq!(
        rejected.code, "OperationIdReusedWithDifferentPayload",
        "first={first:?} second={second:?}"
    );
    assert_ne!(first_target.exists(), second_target.exists());
}

#[test]
fn export_record_begin_failure_keeps_installed_output() {
    let temp = TempDir::new("export-begin-failure");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let saved = save(
        &project,
        &access,
        &document,
        "begin-failure-save",
        body(json!([paragraph("p", json!([text("source")]))])),
    );
    let preview = prepare_draft_export(&project, &access, saved.head, DraftFormat::PlainText)
        .expect("prepare begin failure export");
    let blocker = Connection::open(source.join("project.sqlite3")).unwrap();
    blocker
        .execute_batch("BEGIN IMMEDIATE;")
        .expect("hold the database writer lock");
    let target = temp.child("begin-failure.txt");
    let error = export_prepared_draft(&project, access, preview, &target)
        .expect_err("transaction begin failure must be explicit");
    assert_eq!(error.code, "ExportRecordUnavailable");
    assert!(target.exists());
    let count: i64 = blocker
        .query_row("SELECT COUNT(*) FROM export_records", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    blocker.execute_batch("ROLLBACK;").unwrap();
}

#[test]
fn markdown_preserves_trailing_breaks_and_safe_url_delimiters() {
    let temp = TempDir::new("markdown-delimiters");
    let (project, access, document) = open_project(&temp.child("source"));
    let saved = save(
        &project,
        &access,
        &document,
        "markdown-delimiters-save",
        body(json!([paragraph(
            "p",
            json!([
                {"type":"text","text":"    bold  ","marks":[{"type":"bold"}]},
                text(" and "),
                {"type":"text","text":"site","marks":[{"type":"link","attrs":{"href":"https://example.com/?q=<tag>&copy;"}}]},
                {"type":"hardBreak"}
            ])
        )])),
    );
    let preview = prepare_draft_export(&project, &access, saved.head, DraftFormat::Markdown)
        .expect("prepare delimiter export");
    assert_eq!(
        preview.preview_text,
        "&#32;   **bold**   and [site](<https://example.com/?q=&lt;tag&gt;&amp;copy;>)  \n"
    );
    assert!(preview.preview_text.ends_with("  \n"));
    let target = temp.child("delimiters.md");
    export_prepared_draft(&project, access, preview.clone(), &target)
        .expect("install delimiter export");
    assert_eq!(fs::read_to_string(target).unwrap(), preview.preview_text);
}

#[test]
fn markdown_keeps_adjacent_marks_and_text_syntax_in_one_paragraph() {
    let temp = TempDir::new("markdown-syntax");
    let (project, access, document) = open_project(&temp.child("source"));
    let saved = save(
        &project,
        &access,
        &document,
        "markdown-syntax-save",
        body(json!([paragraph(
            "p",
            json!([
                {"type":"text","text":"bold","marks":[{"type":"bold"}]},
                {"type":"text","text":"italic","marks":[{"type":"italic"}]},
                {"type":"text","text":"both","marks":[{"type":"italic"},{"type":"bold"}]},
                text(" # heading\n    indented")
            ])
        )])),
    );
    let preview = prepare_draft_export(&project, &access, saved.head, DraftFormat::Markdown)
        .expect("prepare syntax export");
    assert_eq!(
        preview.preview_text,
        "**bold***italic****both*** \\# heading  \n    indented"
    );
    assert!(!preview.preview_text.starts_with("    "));
    assert!(preview.preview_text.contains("\\# heading"));
}

#[test]
fn export_refuses_windows_device_stream_and_ambiguous_filenames() {
    let temp = TempDir::new("export-filename");
    let (project, access, document) = open_project(&temp.child("source"));
    let preview = prepare_draft_export(&project, &access, document.head, DraftFormat::PlainText)
        .expect("prepare filename check");
    for name in [
        "NUL.txt",
        "con",
        "COM1.md",
        "Lpt².txt",
        "CON .txt",
        "CONOUT$.txt",
        "draft:stream",
        "draft.md.",
        "draft.md ",
        "draft?.md",
    ] {
        let error =
            export_prepared_draft(&project, access.clone(), preview.clone(), &temp.child(name))
                .expect_err("reject invalid Windows basename before any write");
        assert_eq!(error.code, "InvalidRequest", "{name}");
    }
    let record =
        export_prepared_draft(&project, access, preview, &temp.child("A valid chapter.md"))
            .expect("a valid new basename still works");
    assert_eq!(record.basename, "A valid chapter.md");
}

#[test]
fn markdown_keeps_sentence_periods_but_protects_literal_numbered_lines() {
    let temp = TempDir::new("markdown-periods");
    let (project, access, document) = open_project(&temp.child("source"));
    let saved = save(
        &project,
        &access,
        &document,
        "markdown-periods-save",
        body(json!([paragraph(
            "p",
            json!([text(
                "It ended. 3.14. Part 1.\n1. This is prose.\n12.\n1234567890. Still prose."
            )])
        )])),
    );
    let preview = prepare_draft_export(&project, &access, saved.head, DraftFormat::Markdown)
        .expect("prepare punctuation export");
    assert_eq!(
        preview.preview_text,
        "It ended. 3.14. Part 1.  \n1\\. This is prose.  \n12\\.  \n1234567890. Still prose."
    );
}

#[test]
fn markdown_tab_indent_does_not_turn_a_paragraph_into_a_code_block() {
    let temp = TempDir::new("markdown-tab");
    let (project, access, document) = open_project(&temp.child("source"));
    let saved = save(
        &project,
        &access,
        &document,
        "markdown-tab-save",
        body(json!([paragraph(
            "p",
            json!([
                text("\t"),
                {"type":"text","text":"a","marks":[{"type":"bold"}]},
                {"type":"text","text":"b","marks":[{"type":"bold"}]}
            ])
        )])),
    );
    let preview =
        prepare_draft_export(&project, &access, saved.head, DraftFormat::Markdown).unwrap();
    // An escaped tab starts an ordinary paragraph; canonical source validation
    // merges the equal-mark text runs before Markdown delimiters are generated.
    assert_eq!(preview.preview_text, "&#9;**ab**");
}

#[test]
fn backup_and_recovery_preserve_historical_export_records_and_schema_nine_migration() {
    let temp = TempDir::new("export-backup");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let saved = save(
        &project,
        &access,
        &document,
        "backup-export-save",
        body(json!([paragraph("p", json!([text("source")]))])),
    );
    let preview = prepare_draft_export(&project, &access, saved.head, DraftFormat::Markdown)
        .expect("prepare backup export");
    let output = temp.child("backup-export.md");
    let record = export_prepared_draft(&project, access.clone(), preview, &output)
        .expect("record backup export");
    let archive = temp.child("export.wnsbackup");
    create_backup(&project, &archive).expect("backup export record");
    let recovered_path = temp.child("recovered");
    let recovered = recover_backup(&archive, &recovered_path, "Recovered exports")
        .expect("recover export record");
    let recovered_namespace = recovered.info.operation_namespace.clone();
    let recovered_count: i64 = Connection::open(recovered_path.join("project.sqlite3"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM export_records", [], |row| row.get(0))
        .unwrap();
    let copied_namespace: String = Connection::open(recovered_path.join("project.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT operation_namespace FROM export_records WHERE id=?",
            [&record.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(recovered_count, 1);
    assert_ne!(copied_namespace, recovered_namespace);

    drop(recovered);
    drop(project);
    let connection = Connection::open(source.join("project.sqlite3")).unwrap();
    legacy_schema::remove_schema19_features(&connection).unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER discussion_lookup_results_no_update;
             DROP TRIGGER discussion_lookup_results_no_delete;
             DROP TRIGGER discussion_lookup_reads_no_update;
             DROP TRIGGER discussion_lookup_reads_no_delete;
             DROP TABLE discussion_lookup_reads;
             DROP TABLE discussion_lookup_results;
             DROP TABLE discussion_lookup_invocations;
             DROP TABLE snapshot_navigation_views; DROP TABLE memory_view_sources; DROP TABLE memory_views; DROP TABLE memory_results; DROP TABLE memory_jobs; ALTER TABLE snapshot_sources DROP COLUMN reader_position;
             DROP TRIGGER review_stages_no_update;
             DROP TRIGGER review_stages_no_delete;
             DROP TRIGGER ready_bundles_no_update;
             DROP TRIGGER ready_bundles_no_delete;
             DROP TRIGGER review_fences_no_update;
             DROP TRIGGER review_fences_no_delete;
             DROP TABLE review_fences;
             DROP TABLE ready_heads;
             DROP TABLE ready_bundles;
             DROP TABLE review_stages;
             DROP TRIGGER source_pin_receipts_no_update;
             DROP TRIGGER source_pin_receipts_no_delete;
             DROP TABLE source_pin_receipts;
             DROP TABLE source_pin_sets;
             DROP TABLE import_legacy_records;
             DROP TABLE import_body_decisions;
             DROP TABLE import_id_map;
             DROP TABLE import_manifest;
             DROP TRIGGER provider_results_no_update;
             DROP TRIGGER provider_results_no_delete;
             DROP TABLE provider_results;
             DROP TABLE export_records;
             ALTER TABLE discussion_drafts DROP COLUMN safe_brief_json;
             PRAGMA user_version=8;",
        )
        .unwrap();
    drop(connection);
    let migrated = ProjectSession::open(&source).expect("migrate schema eight to fourteen");
    let version: i64 = Connection::open(source.join("project.sqlite3"))
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 25);
    let table: i64 = Connection::open(source.join("project.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='export_records'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table, 1);
    drop(migrated);
}

#[test]
fn tampered_export_record_is_rejected_by_backup_validation() {
    let temp = TempDir::new("export-backup-tamper");
    let source = temp.child("source");
    let (project, access, document) = open_project(&source);
    let saved = save(
        &project,
        &access,
        &document,
        "backup-tamper-save",
        body(json!([paragraph("p", json!([text("source")]))])),
    );
    let preview = prepare_draft_export(&project, &access, saved.head, DraftFormat::PlainText)
        .expect("prepare tamper export");
    export_prepared_draft(&project, access, preview, &temp.child("tamper.txt"))
        .expect("record tamper export");
    let backup = temp.child("valid.wnsbackup");
    create_backup(&project, &backup).expect("create export backup");
    let (mut manifest, database) = archive_entries(&backup);
    let database_path = temp.child("tampered.sqlite3");
    fs::write(&database_path, database).unwrap();
    let connection = Connection::open(&database_path).unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER export_records_immutable_update;
             UPDATE export_records SET project_id='other-project';",
        )
        .unwrap();
    drop(connection);
    let database = fs::read(&database_path).unwrap();
    manifest.database_sha256 = sha256(&database);
    let tampered_manifest = serde_json::to_vec(&manifest).unwrap();
    let tampered_backup = temp.child("tampered.wnsbackup");
    write_archive(&tampered_backup, &tampered_manifest, &database, None);
    let error = recover_backup(
        &tampered_backup,
        &temp.child("rejected-recovery"),
        "Tampered export",
    )
    .err()
    .expect("tampered export metadata must reject recovery");
    assert_eq!(error.code, "InvalidBackup");
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
            "DROP TRIGGER discussion_lookup_results_no_update;
             DROP TRIGGER discussion_lookup_results_no_delete;
             DROP TRIGGER discussion_lookup_reads_no_update;
             DROP TRIGGER discussion_lookup_reads_no_delete;
             DROP TABLE discussion_lookup_reads;
             DROP TABLE discussion_lookup_results;
             DROP TABLE discussion_lookup_invocations;
             DROP TRIGGER review_stages_no_update;
             DROP TRIGGER review_stages_no_delete;
             DROP TRIGGER ready_bundles_no_update;
             DROP TRIGGER ready_bundles_no_delete;
             DROP TRIGGER review_fences_no_update;
             DROP TRIGGER review_fences_no_delete;
             DROP TABLE review_fences;
             DROP TABLE ready_heads;
             DROP TABLE ready_bundles;
             DROP TABLE review_stages;
             DROP TRIGGER source_pin_receipts_no_update;
             DROP TRIGGER source_pin_receipts_no_delete;
             DROP TABLE source_pin_receipts;
             DROP TABLE source_pin_sets;
             DROP TABLE import_legacy_records;
             DROP TABLE import_body_decisions;
             DROP TABLE import_id_map;
             DROP TABLE import_manifest;
             DROP TRIGGER provider_results_no_update;
             DROP TRIGGER provider_results_no_delete;
             DROP TABLE provider_results;
             DROP TABLE export_records;
             DROP TRIGGER command_receipts_no_proposal_collision;
             DROP TABLE proposal_receipts; DROP TABLE proposal_decisions; DROP TABLE proposal_versions; DROP TABLE proposals;
             DROP TABLE guidance_request_uses; DROP TABLE snapshot_guidance;
             DROP TABLE author_guidance_receipts; DROP TABLE author_guidance_heads;
             DROP TABLE author_guidance_versions;
             DROP TABLE discussion_output_events; DROP TABLE discussion_messages;
             DROP TABLE discussion_draft_receipts; DROP TABLE discussion_runs;
             DROP TABLE discussion_drafts; DROP TABLE discussion_threads;
             DROP TABLE snapshot_navigation_views; DROP TABLE memory_view_sources; DROP TABLE memory_views; DROP TABLE memory_results; DROP TABLE memory_jobs; DROP TABLE context_packets; DROP TABLE snapshot_sources; DROP TABLE story_snapshots;
             DROP TABLE passage_projections; DROP TABLE document_aliases;
             ALTER TABLE project DROP COLUMN disclosure_policy_epoch;
             DROP TABLE view_state; ALTER TABLE project DROP COLUMN context_source_epoch;",
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
        .expect("open recovered migrated database");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read recovered schema");
    assert_eq!(version, 25);
}
