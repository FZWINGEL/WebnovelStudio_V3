use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::{Audience, InformationPolicy};
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::story_context::FreezeReviewedContinuation;
use webnovel_core::projects::{
    CreateDocument, DocumentRecord, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-reviewed-context-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create test root");
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
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [{
            "type": "paragraph", "attrs": {"id": "p1"},
            "content": [{"type": "text", "text": text}]
        }]}
    })
}

fn hash_json(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Fixture {
    temp: TempProject,
    project: ProjectSession,
    access: ProjectAccess,
    first: DocumentRecord,
    target: DocumentRecord,
    first_original_bundle: String,
    first_bundle: String,
    frozen_snapshot_id: String,
}

fn chapter(
    project: &ProjectSession,
    access: &ProjectAccess,
    id: &str,
    title: &str,
    text: &str,
) -> DocumentRecord {
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: format!("create-{id}"),
            document_id: id.into(),
            title: title.into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .expect("create chapter")
}

fn mark_ready(
    project: &ProjectSession,
    access: &ProjectAccess,
    stage_operation: &str,
    ready_operation: &str,
    document: &DocumentRecord,
) -> webnovel_core::projects::reviewed_story::ReadyBundle {
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: stage_operation.into(),
            expected: document.head.clone(),
            records: None,
            promises: None,
        })
        .expect("stage review");
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: ready_operation.into(),
            stage_id: stage.id,
        })
        .expect("mark review ready")
}

fn policy(project: &ProjectSession, access: &ProjectAccess, frontier: &str) -> InformationPolicy {
    InformationPolicy {
        version: project.context_epochs(access.clone()).unwrap().policy,
        audience: Audience::RestrictedWriting,
        reader_frontier: Some(frontier.into()),
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    }
}

fn fixture() -> Fixture {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Reviewed context test")
        .expect("create project");
    let access = project.attach("reviewed-context-test".into()).unwrap();
    let first = chapter(&project, &access, "chapter-1", "One", "First original.");
    let second = chapter(&project, &access, "chapter-2", "Two", "Second original.");
    let target = chapter(&project, &access, "chapter-3", "Three", "Current target.");

    let first_original = mark_ready(
        &project,
        &access,
        "stage-first-original",
        "ready-first-original",
        &first,
    );
    let first_current = mark_ready(
        &project,
        &access,
        "stage-first-reaffirmed",
        "ready-first-reaffirmed",
        &first,
    );
    mark_ready(&project, &access, "stage-second", "ready-second", &second);
    let frozen = project
        .freeze_reviewed_continuation(FreezeReviewedContinuation {
            access: access.clone(),
            operation_id: "freeze-reviewed-adversarial".into(),
            expected: target.head.clone(),
            policy: policy(&project, &access, "2"),
        })
        .expect("freeze reviewed continuation");

    Fixture {
        temp,
        project,
        access,
        first,
        target,
        first_original_bundle: first_original.id,
        first_bundle: first_current.id,
        frozen_snapshot_id: frozen.snapshot.snapshot_id,
    }
}

fn rewrite_manifest(path: PathBuf, snapshot_id: &str, mutate: impl FnOnce(&mut Value)) {
    let db = Connection::open(path).expect("open project database");
    db.execute_batch("DROP TRIGGER immutable_story_snapshot_update")
        .expect("disable only the immutable-row test guard");
    let json: String = db
        .query_row(
            "SELECT manifest_json FROM story_snapshots WHERE id=?",
            [snapshot_id],
            |row| row.get(0),
        )
        .expect("read snapshot manifest");
    let mut manifest: Value = serde_json::from_str(&json).expect("decode snapshot manifest");
    mutate(&mut manifest);
    let json = serde_json::to_string(&manifest).expect("encode tampered manifest");
    db.execute(
        "UPDATE story_snapshots SET manifest_json=?,manifest_hash=? WHERE id=?",
        rusqlite::params![json, hash_json(&json), snapshot_id],
    )
    .expect("write rehashed test manifest");
}

fn assert_snapshot_rejected(fixture: Fixture, mutate: impl FnOnce(&mut Value)) {
    let snapshot_id = fixture.frozen_snapshot_id.clone();
    let project_path = fixture.temp.child("story");
    let db_path = project_path.join("project.sqlite3");
    drop(fixture.project);
    rewrite_manifest(db_path, &snapshot_id, mutate);
    let reopened = ProjectSession::open(project_path).expect("reopen tampered project");
    let access = reopened.attach("reviewed-context-reader".into()).unwrap();
    let error = reopened
        .story_snapshot(access, snapshot_id)
        .expect_err("tampered reviewed snapshot must be refused");
    assert!(
        matches!(
            error.code.as_str(),
            "InvalidContext" | "ContextSourceDisallowed"
        ),
        "unexpected tamper refusal code: {} ({})",
        error.code,
        error.detail
    );
}

#[test]
fn rehashed_snapshot_cannot_drop_or_reorder_a_true_reviewed_prefix_member() {
    for mutation in ["drop", "reorder"] {
        let fixture = fixture();
        assert_snapshot_rejected(fixture, |manifest| {
            let prefix = manifest["snapshot"]["reviewedBasis"]["prefix"]
                .as_array_mut()
                .expect("reviewed prefix");
            if mutation == "drop" {
                prefix.remove(0);
            } else {
                prefix.swap(0, 1);
            }
        });
    }
}

#[test]
fn foreign_reviewed_manifest_namespace_is_not_bound_by_a_matching_snapshot_row() {
    let fixture = fixture();
    assert_snapshot_rejected(fixture, |manifest| {
        manifest["snapshot"]["reviewedBasis"]["operationNamespace"] =
            Value::String(Uuid::new_v4().to_string());
    });
}

#[test]
fn same_revision_from_a_different_ready_bundle_cannot_replace_snapshot_ancestry() {
    let fixture = fixture();
    let original_bundle = fixture.first_original_bundle.clone();
    assert_snapshot_rejected(fixture, |manifest| {
        manifest["snapshot"]["reviewedBasis"]["prefix"][0]["bundleId"] =
            Value::String(original_bundle);
    });
}

#[test]
fn historical_reviewed_snapshot_remains_readable_but_becomes_stale_after_edit_and_reaffirmation() {
    let fixture = fixture();
    let old_snapshot = fixture.frozen_snapshot_id.clone();
    let old_handle = format!("reviewed-{}", fixture.first_bundle);
    fixture
        .project
        .save(SaveSnapshot {
            access: fixture.access.clone(),
            operation_id: "edit-reviewed-first".into(),
            expected: fixture.first.head.clone(),
            local_generation: "1".into(),
            body: body("First revised after the snapshot."),
            cause: SaveCause::Typing,
        })
        .expect("edit reviewed source");
    let current_first = fixture
        .project
        .document(
            fixture.access.clone(),
            fixture.first.head.document_id.clone(),
        )
        .unwrap();
    mark_ready(
        &fixture.project,
        &fixture.access,
        "stage-first-after-snapshot",
        "ready-first-after-snapshot",
        &current_first,
    );

    assert!(
        !fixture
            .project
            .story_snapshot_is_current(fixture.access.clone(), old_snapshot.clone())
            .expect("check historical snapshot freshness")
    );
    let historical = fixture
        .project
        .story_snapshot(fixture.access.clone(), old_snapshot.clone())
        .expect("historical snapshot remains readable");
    let source = fixture
        .project
        .read_story_source(fixture.access, old_snapshot, old_handle)
        .expect("read historical reviewed source");
    assert_eq!(source.body, body("First original."));
    assert_eq!(
        historical.snapshot.basis,
        webnovel_core::context::BasisKind::Reviewed
    );
}

#[test]
fn recovered_copy_cannot_replay_or_initiate_the_source_snapshot_operation() {
    let fixture = fixture();
    let backup = fixture.temp.child("reviewed-context.wnsbackup");
    create_backup(&fixture.project, &backup).expect("backup reviewed context");
    let snapshot_id = fixture.frozen_snapshot_id.clone();
    let target = fixture.target.clone();
    drop(fixture.project);
    let recovered = recover_backup(&backup, &fixture.temp.child("recovered"), "Recovered")
        .expect("recover reviewed context");
    let access = recovered
        .attach("recovered-reviewed-context".into())
        .unwrap();
    assert_eq!(
        recovered
            .story_snapshot(access.clone(), snapshot_id)
            .expect_err("copied snapshot must not authorize reads")
            .code,
        "ContextProjectMismatch"
    );
    let error = recovered
        .freeze_reviewed_continuation(FreezeReviewedContinuation {
            access: access.clone(),
            operation_id: "freeze-reviewed-adversarial".into(),
            expected: target.head,
            policy: policy(&recovered, &access, "2"),
        })
        .expect_err("copied operation must not initiate reviewed authority");
    assert_eq!(error.code, "ReviewBasisUnavailable");
}
