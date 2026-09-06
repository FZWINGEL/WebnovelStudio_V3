use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::reviewed_summary::{SummaryAudience, SummaryChange};
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-reviewed-summary-{}", Uuid::new_v4()));
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

fn hash_bytes(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn setup() -> (TempProject, ProjectSession, ProjectAccess) {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Reviewed summary test").unwrap();
    let access = project.attach("summary-test".into()).unwrap();
    (temp, project, access)
}

fn chapter(
    project: &ProjectSession,
    access: &ProjectAccess,
    text: &str,
) -> webnovel_core::projects::DocumentRecord {
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "One".into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .unwrap()
}

fn stage(
    project: &ProjectSession,
    access: &ProjectAccess,
    operation_id: &str,
    document: &webnovel_core::projects::DocumentRecord,
    summary: Option<SummaryChange>,
) -> webnovel_core::projects::reviewed_story::ReviewStage {
    project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: operation_id.into(),
            expected: document.head.clone(),
            records: None,
            promises: None,
            summary,
        })
        .unwrap()
}

fn ready(
    project: &ProjectSession,
    access: &ProjectAccess,
    operation_id: &str,
    stage_id: &str,
) -> webnovel_core::projects::reviewed_story::ReadyBundle {
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: operation_id.into(),
            stage_id: stage_id.into(),
        })
        .unwrap()
}

#[test]
fn summary_set_is_immutable_hashed_and_inherited_only_on_exact_basis() {
    let (_temp, project, access) = setup();
    let document = chapter(&project, &access, "The lantern waits.");
    let change = SummaryChange::Set {
        text: "A lantern waits at the gate.\nThe choice remains open.".into(),
        audience: SummaryAudience::AuthorRoom,
    };
    let staged = stage(
        &project,
        &access,
        "stage-summary",
        &document,
        Some(change.clone()),
    );
    let summary = staged.summary.clone().expect("staged summary");
    assert_eq!(summary.audience, SummaryAudience::AuthorRoom);
    assert_eq!(summary.source.document_id, document.head.document_id);
    assert_eq!(summary.dependencies, staged.prefix);
    let bundle = ready(&project, &access, "ready-summary", &staged.id);
    assert_eq!(bundle.summary, Some(summary.clone()));
    assert_eq!(bundle.summary_hash, staged.summary_hash);

    let replay = stage(&project, &access, "stage-summary", &document, Some(change));
    assert_eq!(replay.id, staged.id);
    assert_eq!(replay.summary, Some(summary.clone()));

    let inherited = stage(&project, &access, "stage-inherit", &document, None);
    assert_eq!(inherited.summary, Some(summary.clone()));
    let inherited_bundle = ready(&project, &access, "ready-inherit", &inherited.id);
    assert_eq!(inherited_bundle.summary, Some(summary));

    let read = project
        .read_reviewed_record_set(access, document.head.document_id)
        .unwrap()
        .expect("summary-only reviewed set");
    assert!(read.records.is_empty());
    assert!(read.summary.is_some());
    assert!(read.summary_hash.is_some());
}

#[test]
fn changed_source_requires_explicit_summary_decision_and_clear_is_atomic() {
    let (temp, project, access) = setup();
    let original = chapter(&project, &access, "Original chapter.");
    let staged = stage(
        &project,
        &access,
        "stage-summary",
        &original,
        Some(SummaryChange::Set {
            text: "The original choice waits.".into(),
            audience: SummaryAudience::Reader,
        }),
    );
    ready(&project, &access, "ready-summary", &staged.id);
    let saved = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-chapter".into(),
            expected: original.head,
            local_generation: "1".into(),
            body: body("Rewritten chapter."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let changed = project
        .document(access.clone(), saved.head.document_id.clone())
        .unwrap();
    let stale = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-missing-decision".into(),
            expected: changed.head.clone(),
            records: None,
            promises: None,
            summary: None,
        })
        .unwrap_err();
    assert_eq!(stale.code, "ReviewSummaryRequired");

    let cleared = stage(
        &project,
        &access,
        "stage-clear-summary",
        &changed,
        Some(SummaryChange::Clear),
    );
    assert!(cleared.summary.is_none());
    let cleared_bundle = ready(&project, &access, "ready-clear-summary", &cleared.id);
    assert!(cleared_bundle.summary.is_none());

    drop(project);
    let reopened = ProjectSession::open(temp.child("story")).unwrap();
    let reopened_access = reopened.attach("summary-reopen".into()).unwrap();
    let read = reopened
        .read_reviewed_record_set(reopened_access, "chapter-one".into())
        .unwrap()
        .expect("reviewed set remains readable");
    assert!(read.summary.is_none());
}

#[test]
fn summary_text_rejects_invalid_controls_and_payload_is_canonical() {
    let (temp, project, access) = setup();
    let document = chapter(&project, &access, "The chapter.");
    for (operation_id, text) in [
        ("summary-blank", " \n\t "),
        ("summary-control", "line\u{000b}break"),
    ] {
        let error = project
            .stage_author_review(StageAuthorReview {
                access: access.clone(),
                operation_id: operation_id.into(),
                expected: document.head.clone(),
                records: None,
                promises: None,
                summary: Some(SummaryChange::Set {
                    text: text.into(),
                    audience: SummaryAudience::Reader,
                }),
            })
            .unwrap_err();
        assert_eq!(error.code, "InvalidReviewedSummary");
    }

    let staged = stage(
        &project,
        &access,
        "summary-canonical",
        &document,
        Some(SummaryChange::Set {
            text: "A canonical summary.\nWith a tab\t.".into(),
            audience: SummaryAudience::Reader,
        }),
    );
    let database = Connection::open(temp.child("story").join("project.sqlite3")).unwrap();
    let row: (String, String) = database
        .query_row(
            "SELECT summary_json,summary_hash FROM review_stages WHERE id=?",
            [&staged.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        row.0,
        serde_json::to_string(staged.summary.as_ref().unwrap()).unwrap()
    );
    assert_eq!(row.1, hash_bytes(row.0.as_bytes()));
}

#[test]
fn failed_mark_ready_leaves_summary_stage_and_no_selected_head() {
    let (temp, project, access) = setup();
    let document = chapter(&project, &access, "The chapter.");
    let staged = stage(
        &project,
        &access,
        "summary-rollback-stage",
        &document,
        Some(SummaryChange::Set {
            text: "The choice is still open.".into(),
            audience: SummaryAudience::AuthorRoom,
        }),
    );
    let database = Connection::open(temp.child("story").join("project.sqlite3")).unwrap();
    database
        .execute_batch(
            "CREATE TRIGGER fail_summary_ready BEFORE INSERT ON ready_bundles
             BEGIN SELECT RAISE(ABORT,'summary ready fault'); END;",
        )
        .unwrap();
    let error = project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "summary-rollback-ready".into(),
            stage_id: staged.id.clone(),
        })
        .unwrap_err();
    assert_eq!(error.code, "PersistenceUnavailable");
    database
        .execute_batch("DROP TRIGGER fail_summary_ready")
        .unwrap();
    assert_eq!(
        database
            .query_row("SELECT COUNT(*) FROM ready_heads", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let reread = project.read_review_stage(access, staged.id).unwrap();
    assert!(reread.summary.is_some());
}
