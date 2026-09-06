use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{
    DraftFormat, create_backup, export_prepared_draft, prepare_draft_export,
    prepare_reviewed_draft_export, recover_backup,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

#[path = "support/schema.rs"]
mod legacy_schema;

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [{
            "type": "paragraph", "attrs": {"id": "p1"},
            "content": [{"type": "text", "text": text}]
        }]}
    })
}

fn reviewed_body() -> Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [
            {"type": "paragraph", "attrs": {"id": "p1"}, "content": [
                {"type": "text", "text": "Reviewed "},
                {"type": "text", "text": "chapter", "marks": [{"type": "bold"}]}
            ]},
            {"type": "sceneBreak", "attrs": {"id": "break"}},
            {"type": "paragraph", "attrs": {"id": "p2"}, "content": [
                {"type": "text", "text": "Ending."}
            ]}
        ]}
    })
}

fn setup() -> (
    Cleanup,
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let root = std::env::temp_dir().join(format!("wns-reviewed-export-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let project = ProjectSession::create(root.join("story"), "Reviewed export").unwrap();
    let access = project.attach("reviewed-export".into()).unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter".into(),
            title: "Chapter".into(),
            kind: "chapter".into(),
            body: reviewed_body(),
        })
        .unwrap();
    (Cleanup(root), project, access, document)
}

fn chapter(
    project: &ProjectSession,
    access: &ProjectAccess,
    id: &str,
    text: &str,
    operation: &str,
) -> webnovel_core::projects::DocumentRecord {
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: operation.into(),
            document_id: id.into(),
            title: id.into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .unwrap()
}

fn mark_ready(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    operation: &str,
) -> String {
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: format!("stage-{operation}"),
            expected: document.head.clone(),
            records: None,
            promises: None,
            summary: None,
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: format!("ready-{operation}"),
            stage_id: stage.id,
        })
        .unwrap()
        .id
}

fn read_file(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn archive_entries(path: &std::path::Path) -> (Value, Vec<u8>) {
    let file = fs::File::open(path).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let mut manifest = Vec::new();
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_end(&mut manifest)
        .unwrap();
    let mut database = Vec::new();
    archive
        .by_name("project.sqlite3")
        .unwrap()
        .read_to_end(&mut database)
        .unwrap();
    (serde_json::from_slice(&manifest).unwrap(), database)
}

fn write_archive(path: &std::path::Path, manifest: &Value, database: &[u8]) {
    let file = fs::File::create(path).unwrap();
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    archive.start_file("manifest.json", options).unwrap();
    archive
        .write_all(serde_json::to_string(manifest).unwrap().as_bytes())
        .unwrap();
    archive.start_file("project.sqlite3", options).unwrap();
    archive.write_all(database).unwrap();
    archive.finish().unwrap();
}

fn make_schema19_archive(
    source: &std::path::Path,
    target: &std::path::Path,
    root: &std::path::Path,
) {
    let (mut manifest, database_bytes) = archive_entries(source);
    let legacy_path = root.join("schema19.sqlite3");
    fs::write(&legacy_path, database_bytes).unwrap();
    let connection = Connection::open(&legacy_path).unwrap();
    legacy_schema::remove_schema24_features(&connection).unwrap();
    connection
        .execute_batch(
            "ALTER TABLE export_records DROP COLUMN review_bundle_id;
             ALTER TABLE review_stages DROP COLUMN records_json;
             ALTER TABLE review_stages DROP COLUMN records_hash;
             ALTER TABLE ready_bundles DROP COLUMN records_json;
             ALTER TABLE ready_bundles DROP COLUMN records_hash;
             ALTER TABLE review_stages DROP COLUMN promises_json;
             ALTER TABLE review_stages DROP COLUMN promises_hash;
             ALTER TABLE ready_bundles DROP COLUMN promises_json;
             ALTER TABLE ready_bundles DROP COLUMN promises_hash;
             PRAGMA user_version=19;",
        )
        .unwrap();
    drop(connection);
    let database = fs::read(&legacy_path).unwrap();
    let _ = fs::remove_file(&legacy_path);
    manifest["databaseSchemaVersion"] = json!(19);
    manifest["databaseSha256"] = json!(sha256(&database));
    write_archive(target, &manifest, &database);
}

#[test]
fn reviewed_preview_uses_exact_txt_and_markdown_bytes_without_checkpoint() {
    let (cleanup, project, access, document) = setup();
    let bundle_id = mark_ready(&project, &access, &document, "chapter");
    let history_len = project
        .history(access.clone(), document.head.document_id.clone())
        .unwrap()
        .len();

    let plain = prepare_reviewed_draft_export(
        &project,
        &access,
        document.head.clone(),
        DraftFormat::PlainText,
    )
    .unwrap();
    assert_eq!(plain.review_bundle_id.as_deref(), Some(bundle_id.as_str()));
    assert_eq!(plain.source_head, document.head);
    assert_eq!(
        plain.preview_text,
        "Reviewed chapter\n\n[Scene break]\n\nEnding."
    );
    assert_eq!(plain.utf8_bytes as usize, plain.preview_text.len());
    assert_eq!(
        project
            .history(access.clone(), "chapter".into())
            .unwrap()
            .len(),
        history_len
    );
    let plain_path = cleanup.0.join("reviewed.txt");
    let plain_record =
        export_prepared_draft(&project, access.clone(), plain.clone(), &plain_path).unwrap();
    assert!(!plain_record.working_draft);
    assert_eq!(plain_record.review_bundle_id, plain.review_bundle_id);
    assert_eq!(read_file(&plain_path), plain.preview_text);
    let replay_path = cleanup.0.join("reviewed.txt");
    let replay_error =
        export_prepared_draft(&project, access.clone(), plain.clone(), &replay_path).unwrap_err();
    assert_eq!(replay_error.code, "ExportAlreadyRecorded");
    assert_eq!(read_file(&replay_path), plain.preview_text);
    let mut different_basis = plain.clone();
    different_basis.review_bundle_id = Some("different-reviewed-bundle".into());
    let different_path = cleanup.0.join("reviewed-different-basis.txt");
    let different_error =
        export_prepared_draft(&project, access.clone(), different_basis, &different_path)
            .unwrap_err();
    assert_eq!(different_error.code, "ReviewBundleNotFound");
    assert!(!different_path.exists());

    let markdown =
        prepare_reviewed_draft_export(&project, &access, document.head, DraftFormat::Markdown)
            .unwrap();
    assert_eq!(markdown.review_bundle_id, plain.review_bundle_id);
    assert_eq!(
        markdown.preview_text,
        "Reviewed **chapter**\n\n---\n\nEnding."
    );
    let markdown_path = cleanup.0.join("reviewed.md");
    let markdown_record =
        export_prepared_draft(&project, access, markdown.clone(), &markdown_path).unwrap();
    assert!(!markdown_record.working_draft);
    assert_eq!(read_file(&markdown_path), markdown.preview_text);
}

#[test]
fn reviewed_prepare_requires_review_and_accepts_a_first_chapter() {
    let (cleanup, project, access, document) = setup();
    let error = prepare_reviewed_draft_export(
        &project,
        &access,
        document.head.clone(),
        DraftFormat::PlainText,
    )
    .unwrap_err();
    assert_eq!(error.code, "ReviewRequired");

    let bundle_id = mark_ready(&project, &access, &document, "first");
    let preview =
        prepare_reviewed_draft_export(&project, &access, document.head, DraftFormat::PlainText)
            .unwrap();
    assert_eq!(
        preview.review_bundle_id.as_deref(),
        Some(bundle_id.as_str())
    );
    assert!(!cleanup.0.join("never-written.txt").exists());
}

#[test]
fn reviewed_install_refuses_target_change_earlier_basis_and_policy_without_file() {
    let (cleanup, project, access, document) = setup();
    let bundle_id = mark_ready(&project, &access, &document, "target");
    let preview = prepare_reviewed_draft_export(
        &project,
        &access,
        document.head.clone(),
        DraftFormat::PlainText,
    )
    .unwrap();
    assert_eq!(
        preview.review_bundle_id.as_deref(),
        Some(bundle_id.as_str())
    );
    let changed = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "change-target".into(),
            expected: document.head.clone(),
            local_generation: "1".into(),
            body: body("changed target"),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let target_path = cleanup.0.join("target-stale.txt");
    let error = export_prepared_draft(&project, access.clone(), preview, &target_path).unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "ReviewStale" | "VersionConflict"
    ));
    assert!(!target_path.exists());
    assert_ne!(changed.head.body_hash, document.head.body_hash);

    let (_earlier_cleanup, earlier_project, earlier_access, earlier) = setup();
    let target = chapter(
        &earlier_project,
        &earlier_access,
        "target-2",
        "Target.",
        "create-target-2",
    );
    mark_ready(&earlier_project, &earlier_access, &earlier, "earlier");
    mark_ready(&earlier_project, &earlier_access, &target, "target-2");
    let earlier_preview = prepare_reviewed_draft_export(
        &earlier_project,
        &earlier_access,
        target.head.clone(),
        DraftFormat::PlainText,
    )
    .unwrap();
    earlier_project
        .save(SaveSnapshot {
            access: earlier_access.clone(),
            operation_id: "change-earlier".into(),
            expected: earlier.head,
            local_generation: "1".into(),
            body: body("Changed earlier."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let earlier_path = cleanup.0.join("earlier-stale.txt");
    let error = export_prepared_draft(
        &earlier_project,
        earlier_access.clone(),
        earlier_preview,
        &earlier_path,
    )
    .unwrap_err();
    assert_eq!(error.code, "ReviewStale");
    assert!(!earlier_path.exists());

    let policy = earlier_project
        .context_epochs(earlier_access.clone())
        .unwrap()
        .policy;
    earlier_project
        .revoke_story_context(earlier_access.clone(), policy)
        .unwrap();
    let policy_preview = prepare_reviewed_draft_export(
        &earlier_project,
        &earlier_access,
        target.head,
        DraftFormat::PlainText,
    )
    .unwrap_err();
    assert_eq!(policy_preview.code, "ReviewStale");
}

#[test]
fn historical_reviewed_record_survives_new_bundle_and_recovered_copy_loses_authority() {
    let (cleanup, project, access, document) = setup();
    let old_bundle = mark_ready(&project, &access, &document, "old");
    let preview = prepare_reviewed_draft_export(
        &project,
        &access,
        document.head.clone(),
        DraftFormat::Markdown,
    )
    .unwrap();
    let output = cleanup.0.join("old-reviewed.md");
    let record = export_prepared_draft(&project, access.clone(), preview, &output).unwrap();
    assert_eq!(
        record.review_bundle_id.as_deref(),
        Some(old_bundle.as_str())
    );

    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "new-review-prose".into(),
            expected: document.head,
            local_generation: "2".into(),
            body: body("new reviewed prose"),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let changed_document = project.document(access.clone(), "chapter".into()).unwrap();
    let new_bundle = mark_ready(&project, &access, &changed_document, "new");
    assert_ne!(new_bundle, old_bundle);
    let historical = project
        .read_export_record(access.clone(), record.id.clone())
        .unwrap();
    assert_eq!(
        historical.review_bundle_id.as_deref(),
        Some(old_bundle.as_str())
    );
    let current = prepare_reviewed_draft_export(
        &project,
        &access,
        changed_document.head.clone(),
        DraftFormat::PlainText,
    )
    .unwrap();
    assert_eq!(
        current.review_bundle_id.as_deref(),
        Some(new_bundle.as_str())
    );

    let archive = cleanup.0.join("reviewed.wnsbackup");
    create_backup(&project, &archive).unwrap();
    let recovered_path = cleanup.0.join("recovered");
    let recovered = recover_backup(&archive, &recovered_path, "Recovered reviewed export").unwrap();
    let connection = Connection::open(recovered_path.join("project.sqlite3")).unwrap();
    let retained_bundle: String = connection
        .query_row(
            "SELECT review_bundle_id FROM export_records WHERE id=?",
            [&record.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained_bundle, old_bundle);
    drop(connection);
    let recovered_access = recovered.attach("recovered-review".into()).unwrap();
    let recovered_head = recovered
        .document(recovered_access.clone(), "chapter".into())
        .unwrap()
        .head;
    let error = prepare_reviewed_draft_export(
        &recovered,
        &recovered_access,
        recovered_head,
        DraftFormat::PlainText,
    )
    .unwrap_err();
    assert_eq!(error.code, "ReviewRequired");
}

#[test]
fn tampered_review_bundle_fails_before_file_and_record_failure_preserves_file_boundary() {
    let (cleanup, project, access, document) = setup();
    mark_ready(&project, &access, &document, "tamper");
    let mut tampered = prepare_reviewed_draft_export(
        &project,
        &access,
        document.head.clone(),
        DraftFormat::PlainText,
    )
    .unwrap();
    tampered.review_bundle_id = Some("foreign-bundle".into());
    let tampered_path = cleanup.0.join("tampered.txt");
    let error =
        export_prepared_draft(&project, access.clone(), tampered, &tampered_path).unwrap_err();
    assert_eq!(error.code, "ReviewBundleNotFound");
    assert!(!tampered_path.exists());

    let preview =
        prepare_reviewed_draft_export(&project, &access, document.head, DraftFormat::PlainText)
            .unwrap();
    let db = Connection::open(cleanup.0.join("story/project.sqlite3")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_reviewed_export_record BEFORE INSERT ON export_records
         BEGIN SELECT RAISE(ABORT,'reviewed export record failure'); END;",
    )
    .unwrap();
    drop(db);
    let boundary_path = cleanup.0.join("record-failure.txt");
    let error = export_prepared_draft(&project, access, preview, &boundary_path).unwrap_err();
    assert_eq!(error.code, "ExportRecordUnavailable");
    assert!(boundary_path.exists());
}

#[test]
fn legacy_working_preview_serialization_omits_review_bundle_id() {
    let (_cleanup, project, access, document) = setup();
    let preview = webnovel_core::transfer::prepare_draft_export(
        &project,
        &access,
        document.head,
        DraftFormat::PlainText,
    )
    .unwrap();
    let encoded = serde_json::to_value(&preview).unwrap();
    assert!(encoded.get("reviewBundleId").is_none());
    let decoded: webnovel_core::transfer::DraftExportPreview =
        serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.review_bundle_id, None);
}

#[test]
fn schema19_working_export_archive_migrates_to21_without_changing_record_or_bytes() {
    let (cleanup, project, access, document) = setup();
    let preview =
        prepare_draft_export(&project, &access, document.head, DraftFormat::PlainText).unwrap();
    let preview_json = serde_json::to_string(&preview).unwrap();
    assert!(!preview_json.contains("reviewBundleId"));
    let output = cleanup.0.join("legacy-working.txt");
    let record = export_prepared_draft(&project, access, preview.clone(), &output).unwrap();
    let record_json = serde_json::to_string(&record).unwrap();
    assert!(!record_json.contains("reviewBundleId"));
    let original_bytes = fs::read(&output).unwrap();
    let current_archive = cleanup.0.join("current.wnsbackup");
    create_backup(&project, &current_archive).unwrap();
    let legacy_archive = cleanup.0.join("schema19.wnsbackup");
    make_schema19_archive(&current_archive, &legacy_archive, &cleanup.0);
    let before_recovery = fs::read(&legacy_archive).unwrap();

    let recovered_path = cleanup.0.join("schema19-recovered");
    let recovered = recover_backup(&legacy_archive, &recovered_path, "Recovered schema19").unwrap();
    assert_eq!(fs::read(&legacy_archive).unwrap(), before_recovery);
    let connection = Connection::open(recovered_path.join("project.sqlite3")).unwrap();
    let schema: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(schema, 32);
    let row: (String, String, String, i64, String, i64, String, Option<String>) = connection
        .query_row(
            "SELECT id,project_id,operation_namespace,working_draft,format,format_version,sha256,review_bundle_id
             FROM export_records WHERE id=?",
            [&record.id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(row.0, record.id);
    assert_eq!(row.1, record.project_id);
    assert_eq!(row.2, record.operation_namespace);
    assert_eq!(row.3, 1);
    assert_eq!(row.4, "plainText");
    assert_eq!(row.5, 1);
    assert_eq!(row.6, record.sha256);
    assert_eq!(row.7, None);
    assert_eq!(sha256(&original_bytes), record.sha256);
    drop(connection);
    drop(recovered);
}
