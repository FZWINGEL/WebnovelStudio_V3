use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::{Audience, InformationPolicy};
use webnovel_core::projects::reviewed_story::{MarkReady, ReviewState, StageAuthorReview};
use webnovel_core::projects::story_context::FreezeReviewedContinuation;
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionRecord, PossessionTiming, PromisePhase,
    PromiseRecord, StoryEntityRef,
};
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

#[path = "support/schema.rs"]
mod legacy_schema;

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-reviewed-promises-{}", Uuid::new_v4()));
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

fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn promise(text: &str, id: &str, phase: PromisePhase) -> PromiseRecord {
    PromiseRecord {
        id: id.into(),
        promise: StoryEntityRef {
            id: "promise-bell".into(),
            label: "The bell vow".into(),
        },
        phase,
        timing: PossessionTiming::AtPassage,
        note: "An explicit author observation".into(),
        audience: EvidenceAudience::Reader,
        evidence: EvidenceAnchor {
            block_id: "p1".into(),
            from_utf16: 0,
            to_utf16: text.encode_utf16().count() as u32,
            quote: text.into(),
            quote_hash: hash(text),
        },
    }
}

fn possession(text: &str, id: &str) -> PossessionRecord {
    PossessionRecord {
        id: id.into(),
        object: StoryEntityRef {
            id: "object-bell".into(),
            label: "The bell".into(),
        },
        holder: None,
        timing: PossessionTiming::AtPassage,
        audience: EvidenceAudience::AuthorRoom,
        evidence: EvidenceAnchor {
            block_id: "p1".into(),
            from_utf16: 0,
            to_utf16: text.encode_utf16().count() as u32,
            quote: text.into(),
            quote_hash: hash(text),
        },
    }
}

fn setup() -> (
    TempProject,
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Reviewed promises").unwrap();
    let access = project.attach("promise-test".into()).unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-1".into(),
            title: "Chapter One".into(),
            kind: "chapter".into(),
            body: body("The bell will ring again."),
        })
        .unwrap();
    (temp, project, access, document)
}

fn stage(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    operation_id: &str,
    promises: Option<Vec<PromiseRecord>>,
) -> webnovel_core::projects::reviewed_story::ReviewStage {
    project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: operation_id.into(),
            expected: document.head.clone(),
            records: None,
            promises,
        })
        .unwrap()
}

#[test]
fn promise_sets_validate_and_support_inheritance_and_explicit_clear() {
    let (_temp, project, access, document) = setup();
    let observation = promise(
        "The bell will ring again.",
        "promise-setup",
        PromisePhase::Setup,
    );
    let first = stage(
        &project,
        &access,
        &document,
        "stage-promises",
        Some(vec![observation.clone()]),
    );
    assert_eq!(first.promises, Some(vec![observation.clone()]));
    let first_bundle = project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-promises".into(),
            stage_id: first.id,
        })
        .unwrap();
    assert_eq!(first_bundle.promises, Some(vec![observation.clone()]));
    assert!(first_bundle.promises_hash.is_some());

    let inherited = stage(
        &project,
        &access,
        &document,
        "stage-inherited-promises",
        None,
    );
    assert_eq!(inherited.promises, Some(vec![observation.clone()]));
    let cleared = stage(
        &project,
        &access,
        &document,
        "stage-cleared-promises",
        Some(Vec::new()),
    );
    assert!(cleared.promises.is_none());
    assert!(cleared.promises_hash.is_none());
}

#[test]
fn promise_only_replacement_advances_epoch_and_fences_pending_later_review() {
    let (_temp, project, access, first) = setup();
    let second = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter-2".into(),
            document_id: "chapter-2".into(),
            title: "Chapter Two".into(),
            kind: "chapter".into(),
            body: body("The bell is still silent."),
        })
        .unwrap();
    let old_promise = promise(
        "The bell will ring again.",
        "promise-old",
        PromisePhase::Setup,
    );
    let old_stage = stage(
        &project,
        &access,
        &first,
        "stage-old-promise",
        Some(vec![old_promise]),
    );
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-old-promise".into(),
            stage_id: old_stage.id,
        })
        .unwrap();
    let pending_initial = stage(&project, &access, &second, "stage-pending-later", None);
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-pending-base".into(),
            stage_id: pending_initial.id,
        })
        .unwrap();
    let pending = stage(
        &project,
        &access,
        &second,
        "stage-pending-later-again",
        None,
    );
    let epoch_before = project.context_source_epoch().unwrap();

    let replacement = promise(
        "The bell will ring again.",
        "promise-new",
        PromisePhase::Unclear,
    );
    let replacement_stage = stage(
        &project,
        &access,
        &first,
        "stage-new-promise",
        Some(vec![replacement]),
    );
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-new-promise".into(),
            stage_id: replacement_stage.id,
        })
        .unwrap();
    assert_ne!(project.context_source_epoch().unwrap(), epoch_before);
    assert_eq!(
        first.head.body_hash,
        project
            .document(access.clone(), "chapter-1".into())
            .unwrap()
            .head
            .body_hash
    );
    assert_eq!(
        project
            .chapter_review_status(access.clone(), "chapter-2".into())
            .unwrap()
            .state,
        ReviewState::EarlierBasisChanged
    );
    let error = project
        .mark_ready(MarkReady {
            access,
            operation_id: "ready-pending-later".into(),
            stage_id: pending.id,
        })
        .unwrap_err();
    assert_eq!(error.code, "ReviewStageStale");
}

fn restricted_policy(
    project: &ProjectSession,
    access: &ProjectAccess,
    frontier: &str,
) -> InformationPolicy {
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

#[test]
fn historical_promise_snapshot_survives_edit_and_restricted_history_filters_private_rows() {
    let (temp, project, access, first) = setup();
    let second = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-history-target".into(),
            document_id: "chapter-2".into(),
            title: "Chapter Two".into(),
            kind: "chapter".into(),
            body: body("The bell is still silent."),
        })
        .unwrap();
    let mut private = promise(
        "The bell will ring again.",
        "private-promise-record",
        PromisePhase::Setup,
    );
    private.audience = EvidenceAudience::AuthorRoom;
    let reader = promise(
        "The bell will ring again.",
        "reader-promise-record",
        PromisePhase::Setup,
    );
    let mut promises = vec![private, reader];
    promises[1].audience = EvidenceAudience::Reader;
    let reviewed = stage(
        &project,
        &access,
        &first,
        "stage-history-promises",
        Some(promises),
    );
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-history-promises".into(),
            stage_id: reviewed.id,
        })
        .unwrap();
    let frozen = project
        .freeze_reviewed_continuation(FreezeReviewedContinuation {
            access: access.clone(),
            operation_id: "freeze-history-promises".into(),
            expected: second.head.clone(),
            policy: restricted_policy(&project, &access, "1"),
        })
        .unwrap();
    assert_eq!(frozen.reviewed_promises.len(), 1);
    let snapshot_id = frozen.snapshot.snapshot_id.clone();
    let history = project
        .reviewed_promise_history(access.clone(), snapshot_id.clone(), "promise-bell".into())
        .unwrap();
    assert!(history.current);
    assert_eq!(history.history.observations.len(), 1);
    assert_eq!(
        history.history.observations[0].record_id,
        "reader-promise-record"
    );
    let restricted_wire = serde_json::to_string(&history).unwrap();
    assert!(!restricted_wire.contains("private-promise-record"));
    assert!(history.history.uncertainty.iter().any(|item| matches!(
        item,
        webnovel_core::context::PromiseHistoryUncertainty::DisclosureLimited
    )));

    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-history-source".into(),
            expected: first.head,
            local_generation: "1".into(),
            body: body("The edited bell passage."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let historical = project
        .reviewed_promise_history(access, snapshot_id, "promise-bell".into())
        .unwrap();
    assert!(!historical.current);
    assert_eq!(historical.history.observations.len(), 1);
    assert_eq!(
        historical.history.observations[0].record_id,
        "reader-promise-record"
    );
    drop(project);
    let _ = temp;
}

#[test]
fn tampered_promise_columns_fail_backup_and_recovered_namespace_cannot_read_snapshot() {
    let (temp, project, access, first) = setup();
    let second = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-tamper-target".into(),
            document_id: "chapter-2".into(),
            title: "Chapter Two".into(),
            kind: "chapter".into(),
            body: body("The bell is still silent."),
        })
        .unwrap();
    let reviewed = stage(
        &project,
        &access,
        &first,
        "stage-tamper-promises",
        Some(vec![promise(
            "The bell will ring again.",
            "tamper-promise",
            PromisePhase::Setup,
        )]),
    );
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-tamper-promises".into(),
            stage_id: reviewed.id,
        })
        .unwrap();
    let frozen = project
        .freeze_reviewed_continuation(FreezeReviewedContinuation {
            access: access.clone(),
            operation_id: "freeze-tamper-promises".into(),
            expected: second.head,
            policy: restricted_policy(&project, &access, "1"),
        })
        .unwrap();
    let snapshot_id = frozen.snapshot.snapshot_id;
    let archive = temp.child("valid.wnsbackup");
    create_backup(&project, &archive).unwrap();
    drop(project);

    let database = temp.child("story").join("project.sqlite3");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch("DROP TRIGGER ready_bundles_no_update;")
        .unwrap();
    let altered = vec![promise(
        "The bell will ring again.",
        "tamper-promise",
        PromisePhase::Payoff,
    )];
    connection
        .execute(
            "UPDATE ready_bundles SET promises_json=? WHERE promises_json IS NOT NULL",
            [serde_json::to_string(&altered).unwrap()],
        )
        .unwrap();
    drop(connection);
    let corrupted = ProjectSession::open(temp.child("story")).unwrap();
    let error = create_backup(&corrupted, &temp.child("corrupt.wnsbackup")).unwrap_err();
    assert_eq!(error.code, "InvalidProject");
    drop(corrupted);

    let recovered = recover_backup(&archive, &temp.child("copy"), "Recovered copy").unwrap();
    let recovered_access = recovered.attach("copy-reader".into()).unwrap();
    let error = recovered
        .reviewed_promise_history(recovered_access, snapshot_id, "promise-bell".into())
        .unwrap_err();
    assert_eq!(error.code, "ContextProjectMismatch");
}

#[test]
fn promise_quote_and_note_are_checked_before_persistence() {
    let (_temp, project, access, document) = setup();
    let mut invalid_quote = promise(
        "The bell will ring again.",
        "promise-bad-quote",
        PromisePhase::Setup,
    );
    invalid_quote.evidence.quote = "Edited source".into();
    let error = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-bad-quote".into(),
            expected: document.head.clone(),
            records: None,
            promises: Some(vec![invalid_quote]),
        })
        .unwrap_err();
    assert_eq!(error.code, "InvalidReviewedPromises");

    let mut invalid_note = promise(
        "The bell will ring again.",
        "promise-bad-note",
        PromisePhase::Setup,
    );
    invalid_note.note = "\n".into();
    let error = project
        .stage_author_review(StageAuthorReview {
            access,
            operation_id: "stage-bad-note".into(),
            expected: document.head,
            records: None,
            promises: Some(vec![invalid_note]),
        })
        .unwrap_err();
    assert_eq!(error.code, "InvalidReviewedPromises");
}

#[test]
fn schema22_fixture_migrates_and_preserves_legacy_evidence_columns() {
    let (temp, project, access, document) = setup();
    let first = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-legacy".into(),
            expected: document.head.clone(),
            records: Some(vec![possession(
                "The bell will ring again.",
                "legacy-possession",
            )]),
            promises: None,
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-legacy".into(),
            stage_id: first.id,
        })
        .unwrap();
    drop(project);

    let database = temp.child("story").join("project.sqlite3");
    let connection = Connection::open(&database).unwrap();
    let old: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT records_json,records_hash FROM ready_bundles LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    legacy_schema::remove_schema24_features(&connection).unwrap();
    legacy_schema::remove_schema22_features(&connection).unwrap();
    connection.pragma_update(None, "user_version", 22).unwrap();
    drop(connection);

    let reopened = ProjectSession::open(temp.child("story")).unwrap();
    drop(reopened);
    let migrated = Connection::open(&database).unwrap();
    let version: i64 = migrated
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 31);
    let columns: Vec<String> = migrated
        .prepare("SELECT name FROM pragma_table_info('ready_bundles')")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(columns.iter().any(|column| column == "promises_json"));
    assert_eq!(
        old,
        migrated
            .query_row(
                "SELECT records_json,records_hash FROM ready_bundles LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    );
    drop(migrated);
    let reopened = ProjectSession::open(temp.child("story")).unwrap();
    create_backup(&reopened, &temp.child("migrated.wnsbackup")).unwrap();
}
