use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::guidance::GuidanceScope;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::StartDiscussion;
use webnovel_core::projects::guidance::SaveGuidance;
use webnovel_core::projects::{CreateDocument, ProjectAccess, ProjectSession};
use webnovel_core::transfer::{create_backup, recover_backup};

#[path = "support/schema.rs"]
mod legacy_schema;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("wns-guidance-{label}-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create temporary directory");
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

fn setup(
    root: &Path,
) -> (
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let project = ProjectSession::create(root, "Guidance story").expect("create project");
    let access = project
        .attach("guidance-session".into())
        .expect("attach project");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-document".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("The lantern waits."),
        })
        .expect("create document");
    (project, access, document)
}

#[allow(clippy::too_many_arguments)]
fn request(
    access: &ProjectAccess,
    operation_id: &str,
    guidance_id: &str,
    expected_version: &str,
    text: &str,
    scope: GuidanceScope,
    document_id: Option<&str>,
    active: bool,
) -> SaveGuidance {
    SaveGuidance {
        access: access.clone(),
        operation_id: operation_id.into(),
        guidance_id: guidance_id.into(),
        expected_version: expected_version.into(),
        text: text.into(),
        scope,
        document_id: document_id.map(str::to_owned),
        active,
        origin_message_id: None,
    }
}

#[test]
fn guidance_versions_are_immutable_cas_and_epoch_bound() {
    let temp = TempDir::new("lifecycle");
    let (project, access, document) = setup(&temp.child("project"));

    let before = project.context_source_epoch().expect("read epoch");
    let first = project
        .save_guidance(request(
            &access,
            "guide-create",
            "tone-guidance",
            "0",
            "Keep the ending quiet.",
            GuidanceScope::Project,
            None,
            true,
        ))
        .expect("create guidance");
    assert_eq!(first.version, "1");
    assert!(!first.version_id.is_empty());
    assert_ne!(project.context_source_epoch().expect("read epoch"), before);
    assert_eq!(
        project
            .guidance(access.clone(), document.head.document_id.clone())
            .unwrap(),
        vec![first.clone()]
    );

    let after_create = project.context_source_epoch().expect("read epoch");
    let second = project
        .save_guidance(request(
            &access,
            "guide-edit",
            "tone-guidance",
            "1",
            "Keep the ending restrained.",
            GuidanceScope::Document,
            Some("chapter-one"),
            true,
        ))
        .expect("edit guidance");
    assert_eq!(second.version, "2");
    assert_ne!(second.version_id, first.version_id);
    assert!(project.context_source_epoch().expect("read epoch") > after_create);

    let after_edit = project.context_source_epoch().expect("read epoch");
    let replay = project
        .save_guidance(request(
            &access,
            "guide-edit",
            "tone-guidance",
            "1",
            "Keep the ending restrained.",
            GuidanceScope::Document,
            Some("chapter-one"),
            true,
        ))
        .expect("replay guidance edit");
    assert_eq!(replay, second);
    assert_eq!(
        project.context_source_epoch().expect("read epoch"),
        after_edit
    );

    let changed_payload = project
        .save_guidance(request(
            &access,
            "guide-edit",
            "tone-guidance",
            "1",
            "A different instruction.",
            GuidanceScope::Document,
            Some("chapter-one"),
            true,
        ))
        .expect_err("reused operation must reject changed payload");
    assert_eq!(
        changed_payload.code,
        "OperationIdReusedWithDifferentPayload"
    );

    let noop = project
        .save_guidance(request(
            &access,
            "guide-noop",
            "tone-guidance",
            "2",
            "Keep the ending restrained.",
            GuidanceScope::Document,
            Some("chapter-one"),
            true,
        ))
        .expect("exact no-op");
    assert_eq!(noop, second);
    assert_eq!(
        project.context_source_epoch().expect("read epoch"),
        after_edit
    );

    let stale = project
        .save_guidance(request(
            &access,
            "guide-stale",
            "tone-guidance",
            "1",
            "Another instruction.",
            GuidanceScope::Document,
            Some("chapter-one"),
            true,
        ))
        .expect_err("stale CAS version must reject");
    assert_eq!(stale.code, "GuidanceVersionConflict");

    let retired = project
        .save_guidance(request(
            &access,
            "guide-retire",
            "tone-guidance",
            "2",
            "Keep the ending restrained.",
            GuidanceScope::Document,
            Some("chapter-one"),
            false,
        ))
        .expect("retire guidance");
    assert_eq!(retired.version, "3");
    assert!(
        project
            .guidance(access, document.head.document_id)
            .expect("read applicable guidance")
            .is_empty()
    );
}

#[test]
fn guidance_requires_explicit_stable_id_and_valid_targets_and_provenance() {
    let temp = TempDir::new("validation");
    let (project, access, _document) = setup(&temp.child("project"));

    let mut missing_id = request(
        &access,
        "missing-id",
        "",
        "0",
        "An instruction.",
        GuidanceScope::Project,
        None,
        true,
    );
    assert_eq!(
        project.save_guidance(missing_id.clone()).unwrap_err().code,
        "InvalidRequest"
    );

    missing_id.guidance_id = "bad-scope".into();
    missing_id.scope = GuidanceScope::Project;
    missing_id.document_id = Some("chapter-one".into());
    assert_eq!(
        project.save_guidance(missing_id).unwrap_err().code,
        "InvalidRequest"
    );

    let mut missing_target = request(
        &access,
        "missing-target",
        "target-guidance",
        "0",
        "An instruction.",
        GuidanceScope::Request,
        None,
        true,
    );
    assert_eq!(
        project
            .save_guidance(missing_target.clone())
            .unwrap_err()
            .code,
        "InvalidRequest"
    );

    missing_target.document_id = Some("chapter-one".into());
    missing_target.origin_message_id = Some("no-such-message".into());
    assert_eq!(
        project.save_guidance(missing_target).unwrap_err().code,
        "GuidanceOriginNotFound"
    );

    let too_large = request(
        &access,
        "too-large",
        "large-guidance",
        "0",
        &"x".repeat(16 * 1024 + 1),
        GuidanceScope::Project,
        None,
        true,
    );
    assert_eq!(
        project.save_guidance(too_large).unwrap_err().code,
        "InvalidRequest"
    );
}

#[test]
fn guidance_is_copied_into_recovery_and_remains_editable_with_local_history() {
    let temp = TempDir::new("recovery");
    let (project, access, document) = setup(&temp.child("source"));
    let started = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "origin-discussion".into(),
            expected: document.head.clone(),
            instruction: "Describe the lantern's role in this chapter.".into(),
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
        .expect("create provenance discussion");
    let original = project
        .save_guidance(SaveGuidance {
            origin_message_id: Some(started.user_message.id),
            ..request(
                &access,
                "guide-create",
                "recovery-guidance",
                "0",
                "Keep the lantern motif.",
                GuidanceScope::Document,
                Some("chapter-one"),
                true,
            )
        })
        .expect("create source guidance");
    let backup = temp.child("source.wnsbackup");
    create_backup(&project, &backup).expect("create backup");
    drop(project);

    let recovered = recover_backup(&backup, &temp.child("recovered"), "Recovered story")
        .expect("recover project");
    let recovered_access = recovered
        .attach("recovered-session".into())
        .expect("attach recovered");
    let copied = recovered
        .guidance(recovered_access.clone(), document.head.document_id.clone())
        .expect("read copied guidance");
    assert_eq!(copied, vec![original.clone()]);

    let edited = recovered
        .save_guidance(SaveGuidance {
            origin_message_id: original.origin_message_id.clone(),
            ..request(
                &recovered_access,
                "recovered-edit",
                "recovery-guidance",
                "1",
                "Keep the lantern motif subtle.",
                GuidanceScope::Document,
                Some("chapter-one"),
                true,
            )
        })
        .expect("edit copied guidance");
    assert_eq!(edited.version, "2");
    assert_eq!(
        recovered
            .guidance(recovered_access, "chapter-one".into())
            .unwrap(),
        vec![edited]
    );
}

#[test]
fn schema_five_upgrade_adds_empty_guidance_tables() {
    let temp = TempDir::new("migration");
    let (project, _access, _document) = setup(&temp.child("project"));
    let database = project.path.join("project.sqlite3");
    drop(project);
    let connection = Connection::open(&database).expect("open current database");
    legacy_schema::remove_schema24_features(&connection).unwrap();
    legacy_schema::remove_schema19_features(&connection).unwrap();
    connection
        .execute_batch(
            "DROP TABLE snapshot_navigation_views; DROP TABLE memory_view_sources; DROP TABLE memory_views; DROP TABLE memory_results; DROP TABLE memory_jobs; ALTER TABLE snapshot_sources DROP COLUMN reader_position;
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
             DROP TABLE provider_results;
             DROP TABLE import_manifest; DROP TABLE import_id_map;
             DROP TABLE import_body_decisions; DROP TABLE import_legacy_records;
             DROP TRIGGER source_pin_receipts_no_update;
             DROP TRIGGER source_pin_receipts_no_delete;
             DROP TABLE source_pin_receipts;
             DROP TABLE source_pin_sets;
             DROP TABLE export_records;
             DROP TRIGGER command_receipts_no_proposal_collision;
             DROP TABLE proposal_receipts; DROP TABLE proposal_decisions; DROP TABLE proposal_versions; DROP TABLE proposals;
             DROP TABLE guidance_request_uses;
             DROP TABLE snapshot_guidance;
             DROP TABLE author_guidance_receipts;
             DROP TABLE author_guidance_heads;
             DROP TABLE author_guidance_versions;
             ALTER TABLE discussion_drafts DROP COLUMN safe_brief_json;
             ALTER TABLE discussion_drafts DROP COLUMN intent;
             ALTER TABLE discussion_drafts DROP COLUMN previous_run_id;
             PRAGMA user_version=5;",
        )
        .expect("downgrade synthetic schema");
    drop(connection);

    let upgraded = ProjectSession::open(temp.child("project")).expect("upgrade schema five");
    let connection =
        Connection::open(upgraded.path.join("project.sqlite3")).expect("open upgraded database");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read schema version");
    assert_eq!(version, 29);
    for table in [
        "author_guidance_versions",
        "author_guidance_heads",
        "author_guidance_receipts",
        "snapshot_guidance",
        "guidance_request_uses",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?)",
                [table],
                |row| row.get(0),
            )
            .expect("check guidance table");
        assert_eq!(exists, 1, "missing migrated table {table}");
    }
}
