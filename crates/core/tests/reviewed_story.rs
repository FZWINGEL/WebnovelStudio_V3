use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::projects::reviewed_story::{MarkReady, ReviewState, StageAuthorReview};
use webnovel_core::projects::story_context::FreezeStory;
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionRecord, PossessionTiming, StoryEntityRef,
};
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

use crate::legacy_schema;

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-reviewed-story-{}", Uuid::new_v4()));
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

fn hash_bytes(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn evidence(
    text: &str,
    id: &str,
    audience: EvidenceAudience,
    holder: Option<&str>,
) -> PossessionRecord {
    PossessionRecord {
        id: id.into(),
        object: StoryEntityRef {
            id: "object-sword".into(),
            label: "The sword".into(),
        },
        holder: holder.map(|value| StoryEntityRef {
            id: value.into(),
            label: value.into(),
        }),
        timing: PossessionTiming::AtPassage,
        audience,
        evidence: EvidenceAnchor {
            block_id: "p1".into(),
            from_utf16: 0,
            to_utf16: text.encode_utf16().count() as u32,
            quote: text.into(),
            quote_hash: hash_json(text),
        },
    }
}

fn setup() -> (TempProject, ProjectSession, ProjectAccess) {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Reviewed story test").unwrap();
    let access = project.attach("review-test".into()).unwrap();
    (temp, project, access)
}

fn chapter(
    project: &ProjectSession,
    access: &ProjectAccess,
    id: &str,
    title: &str,
    text: &str,
) -> webnovel_core::projects::DocumentRecord {
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: format!("create-{id}"),
            document_id: id.into(),
            title: title.into(),
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
) -> webnovel_core::projects::reviewed_story::ReviewStage {
    project
        .stage_author_review(StageAuthorReview {
            knowledge: None,
            access: access.clone(),
            operation_id: operation_id.into(),
            expected: document.head.clone(),
            records: None,
            promises: None,
            summary: None,
        })
        .unwrap()
}

fn stage_with_records(
    project: &ProjectSession,
    access: &ProjectAccess,
    operation_id: &str,
    document: &webnovel_core::projects::DocumentRecord,
    records: Option<Vec<PossessionRecord>>,
) -> webnovel_core::projects::reviewed_story::ReviewStage {
    project
        .stage_author_review(StageAuthorReview {
            knowledge: None,
            access: access.clone(),
            operation_id: operation_id.into(),
            expected: document.head.clone(),
            records,
            promises: None,
            summary: None,
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
fn author_review_pins_exact_revisions_and_requires_an_earlier_prefix() {
    let (temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "The first chapter.");
    let second = chapter(&project, &access, "chapter-2", "Two", "The second chapter.");

    let before_prefix = project
        .chapter_review_status(access.clone(), "chapter-2".into())
        .unwrap();
    assert_eq!(before_prefix.state, ReviewState::NoReview);
    assert!(!before_prefix.can_stage);

    let missing = project
        .stage_author_review(StageAuthorReview {
            knowledge: None,
            access: access.clone(),
            operation_id: "stage-second-too-early".into(),
            expected: second.head.clone(),
            records: None,
            promises: None,
            summary: None,
        })
        .unwrap_err();
    assert_eq!(missing.code, "ReviewBasisUnavailable");

    let first_stage = stage(&project, &access, "stage-first", &first);
    assert_eq!(first_stage.target, first.head);
    assert_eq!(first_stage.revision.head, first.head);
    assert!(first_stage.prefix.is_empty());
    let staged_status = project
        .chapter_review_status(access.clone(), "chapter-1".into())
        .unwrap();
    assert_eq!(
        staged_status.pending_stage_id.as_deref(),
        Some(first_stage.id.as_str())
    );
    let first_stage_replay = stage(&project, &access, "stage-first", &first);
    assert_eq!(first_stage_replay.id, first_stage.id);
    let first_bundle = ready(&project, &access, "ready-first", &first_stage.id);
    assert_eq!(first_bundle.target, first.head);
    assert_eq!(
        ready(&project, &access, "ready-first", &first_stage.id).id,
        first_bundle.id
    );
    assert_eq!(
        project
            .chapter_review_status(access.clone(), first.head.document_id.clone())
            .unwrap()
            .state,
        ReviewState::Ready
    );
    assert!(
        project
            .chapter_review_status(access.clone(), "chapter-1".into())
            .unwrap()
            .pending_stage_id
            .is_none()
    );

    let second_stage = stage(&project, &access, "stage-second", &second);
    assert_eq!(second_stage.prefix.len(), 1);
    assert_eq!(second_stage.prefix[0].bundle_id, first_bundle.id);
    let second_bundle = ready(&project, &access, "ready-second", &second_stage.id);
    assert_eq!(second_bundle.target, second.head);
    assert_eq!(
        project
            .chapter_review_status(access.clone(), second.head.document_id.clone())
            .unwrap()
            .state,
        ReviewState::Ready
    );
    let renamed = project
        .rename_document(
            access.clone(),
            "chapter-1".into(),
            "0".into(),
            "One (renamed)".into(),
        )
        .unwrap();
    assert_eq!(renamed.title, "One (renamed)");
    assert_eq!(
        project
            .chapter_review_status(access.clone(), "chapter-2".into())
            .unwrap()
            .state,
        ReviewState::Ready
    );

    drop(project);
    let reopened = ProjectSession::open(temp.child("story")).unwrap();
    let reopened_access = reopened.attach("review-reopen".into()).unwrap();
    assert_eq!(
        reopened
            .chapter_review_status(reopened_access, "chapter-2".into())
            .unwrap()
            .state,
        ReviewState::Ready
    );
}

#[test]
fn changing_an_earlier_chapter_fences_later_review_until_reaffirmed() {
    let (_temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "Original first.");
    let second = chapter(&project, &access, "chapter-2", "Two", "Original second.");
    let first_stage = stage(&project, &access, "stage-first", &first);
    ready(&project, &access, "ready-first", &first_stage.id);
    let second_stage = stage(&project, &access, "stage-second", &second);
    ready(&project, &access, "ready-second", &second_stage.id);

    let first_saved = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-first".into(),
            expected: first.head.clone(),
            local_generation: "1".into(),
            body: body("Revised first."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    assert_eq!(
        project
            .chapter_review_status(access.clone(), "chapter-1".into())
            .unwrap()
            .state,
        ReviewState::ChangedProse
    );
    assert_eq!(
        project
            .chapter_review_status(access.clone(), "chapter-2".into())
            .unwrap()
            .state,
        ReviewState::EarlierBasisChanged
    );

    let new_first = project
        .document(access.clone(), "chapter-1".into())
        .unwrap();
    assert_eq!(new_first.head, first_saved.head);
    let reaffirm_first = stage(&project, &access, "stage-first-again", &new_first);
    ready(&project, &access, "ready-first-again", &reaffirm_first.id);
    assert_eq!(
        project
            .chapter_review_status(access.clone(), "chapter-2".into())
            .unwrap()
            .state,
        ReviewState::EarlierBasisChanged
    );
    let current_second = project
        .document(access.clone(), "chapter-2".into())
        .unwrap();
    let reaffirm_second = stage(&project, &access, "stage-second-again", &current_second);
    assert_eq!(reaffirm_second.prefix[0].head, new_first.head);
    ready(&project, &access, "ready-second-again", &reaffirm_second.id);
    assert_eq!(
        project
            .chapter_review_status(access, "chapter-2".into())
            .unwrap()
            .state,
        ReviewState::Ready
    );
}

#[test]
fn stale_stage_is_rejected_and_history_remains_immutable() {
    let (_temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "Original.");
    let staged = stage(&project, &access, "stage-first", &first);
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-before-mark".into(),
            expected: first.head.clone(),
            local_generation: "1".into(),
            body: body("Changed before mark."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let error = project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-stale".into(),
            stage_id: staged.id.clone(),
        })
        .unwrap_err();
    assert_eq!(error.code, "ReviewStageStale");
    let read = project
        .read_review_stage(access.clone(), staged.id)
        .expect("historical stage remains readable");
    assert_eq!(read.revision.body, body("Original."));
}

#[test]
fn stale_selected_review_survives_backup_and_recovery_clears_active_head() {
    let (temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "Original.");
    let first_stage = stage(&project, &access, "stage-first", &first);
    ready(&project, &access, "ready-first", &first_stage.id);
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-first".into(),
            expected: first.head,
            local_generation: "1".into(),
            body: body("Changed after review."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    assert_eq!(
        project
            .chapter_review_status(access, "chapter-1".into())
            .unwrap()
            .state,
        ReviewState::ChangedProse
    );
    let archive = temp.child("stale.wnsbackup");
    create_backup(&project, &archive).expect("stale review remains backupable");
    drop(project);

    let recovered = recover_backup(&archive, &temp.child("recovered"), "Recovered story")
        .expect("recover backup");
    let recovered_access = recovered.attach("recovered-review".into()).unwrap();
    let status = recovered
        .chapter_review_status(recovered_access, "chapter-1".into())
        .unwrap();
    assert_eq!(status.state, ReviewState::NoReview);
    let connection = Connection::open(recovered.path.join("project.sqlite3")).unwrap();
    let historical: i64 = connection
        .query_row("SELECT COUNT(*) FROM ready_bundles", [], |row| row.get(0))
        .unwrap();
    let active: i64 = connection
        .query_row("SELECT COUNT(*) FROM ready_heads", [], |row| row.get(0))
        .unwrap();
    assert_eq!(historical, 1);
    assert_eq!(active, 0);
}

#[test]
fn rehashed_corrupt_prefix_is_rejected_by_backup_validation() {
    let (temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "First.");
    let second = chapter(&project, &access, "chapter-2", "Two", "Second.");
    let first_stage = stage(&project, &access, "stage-first", &first);
    ready(&project, &access, "ready-first", &first_stage.id);
    let second_stage = stage(&project, &access, "stage-second", &second);
    let second_bundle = ready(&project, &access, "ready-second", &second_stage.id);
    drop(project);

    let database = temp.child("story").join("project.sqlite3");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch("DROP TRIGGER ready_bundles_no_update")
        .unwrap();
    let fake_prefix = "[]";
    connection
        .execute(
            "UPDATE ready_bundles SET prefix_json=?,prefix_hash=? WHERE id=?",
            rusqlite::params![fake_prefix, hash_json(fake_prefix), second_bundle.id],
        )
        .unwrap();
    drop(connection);
    let corrupted = ProjectSession::open(temp.child("story")).unwrap();
    let error = create_backup(&corrupted, &temp.child("corrupt.wnsbackup")).unwrap_err();
    assert_eq!(error.code, "InvalidProject");
}

#[test]
fn rehashed_corrupt_reviewed_evidence_is_rejected_by_backup_validation() {
    let (temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "The first chapter.");
    let records = vec![evidence(
        "The first chapter.",
        "possession-tampered",
        EvidenceAudience::AuthorRoom,
        Some("hero"),
    )];
    let staged = stage_with_records(
        &project,
        &access,
        "stage-tampered-evidence",
        &first,
        Some(records),
    );
    let bundle = ready(&project, &access, "ready-tampered-evidence", &staged.id);
    drop(project);

    let connection = Connection::open(temp.child("story").join("project.sqlite3")).unwrap();
    connection
        .execute_batch("DROP TRIGGER ready_bundles_no_update")
        .unwrap();
    connection
        .execute(
            "UPDATE ready_bundles SET records_hash='tampered' WHERE id=?",
            [&bundle.id],
        )
        .unwrap();
    drop(connection);
    let corrupted = ProjectSession::open(temp.child("story")).unwrap();
    let error = create_backup(&corrupted, &temp.child("corrupt-evidence.wnsbackup")).unwrap_err();
    assert_eq!(error.code, "InvalidProject");
}

#[test]
fn corrupt_ready_bundle_target_is_rejected_by_backup_validation() {
    let (temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "First.");
    let first_stage = stage(&project, &access, "stage-first", &first);
    let first_bundle = ready(&project, &access, "ready-first", &first_stage.id);
    drop(project);

    let connection = Connection::open(temp.child("story").join("project.sqlite3")).unwrap();
    connection
        .execute_batch("DROP TRIGGER ready_bundles_no_update")
        .unwrap();
    connection
        .execute(
            "UPDATE ready_bundles SET target_body_hash='tampered' WHERE id=?",
            [&first_bundle.id],
        )
        .unwrap();
    drop(connection);
    let corrupted = ProjectSession::open(temp.child("story")).unwrap();
    let error = create_backup(&corrupted, &temp.child("corrupt-bundle.wnsbackup")).unwrap_err();
    assert_eq!(error.code, "InvalidProject");
}

#[test]
fn corrupt_suffix_fence_is_rejected_by_backup_validation() {
    let (temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "First.");
    let second = chapter(&project, &access, "chapter-2", "Two", "Second.");
    let first_stage = stage(&project, &access, "stage-first", &first);
    ready(&project, &access, "ready-first", &first_stage.id);
    let second_stage = stage(&project, &access, "stage-second", &second);
    ready(&project, &access, "ready-second", &second_stage.id);
    let first_now = project
        .document(access.clone(), "chapter-1".into())
        .unwrap();
    let first_again = stage(&project, &access, "stage-first-again", &first_now);
    ready(&project, &access, "ready-first-again", &first_again.id);
    drop(project);

    let connection = Connection::open(temp.child("story").join("project.sqlite3")).unwrap();
    connection
        .execute_batch("DROP TRIGGER review_fences_no_update")
        .unwrap();
    connection
        .execute(
            "UPDATE review_fences SET changed_body_hash='tampered' WHERE rowid=(SELECT rowid FROM review_fences LIMIT 1)",
            [],
        )
        .unwrap();
    drop(connection);
    let corrupted = ProjectSession::open(temp.child("story")).unwrap();
    let error = create_backup(&corrupted, &temp.child("corrupt-fence.wnsbackup")).unwrap_err();
    assert_eq!(error.code, "InvalidProject");
}

#[test]
fn policy_change_marks_selected_review_stale_until_reaffirmed() {
    let (_temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "First.");
    let first_stage = stage(&project, &access, "stage-first", &first);
    let first_bundle = ready(&project, &access, "ready-first", &first_stage.id);

    let epochs = project.context_epochs(access.clone()).unwrap();
    let changed = project
        .revoke_story_context(access.clone(), epochs.policy)
        .unwrap();
    assert_eq!(changed.policy, "1");
    let stale = project
        .chapter_review_status(access.clone(), "chapter-1".into())
        .unwrap();
    assert_eq!(stale.state, ReviewState::ReviewNeeded);
    assert_eq!(stale.active_bundle_id, Some(first_bundle.id));

    let current = project
        .document(access.clone(), "chapter-1".into())
        .unwrap();
    let reaffirm = stage(&project, &access, "stage-first-policy", &current);
    ready(&project, &access, "ready-first-policy", &reaffirm.id);
    assert_eq!(
        project
            .chapter_review_status(access, "chapter-1".into())
            .unwrap()
            .state,
        ReviewState::Ready
    );
}

#[test]
fn tied_position_reordering_invalidates_prefix_and_can_be_reaffirmed() {
    let (temp, project, access) = setup();
    let first = chapter(&project, &access, "a", "A", "First.");
    let second = chapter(&project, &access, "b", "B", "Second.");
    let third = chapter(&project, &access, "c", "C", "Third.");
    let first_stage = stage(&project, &access, "stage-a", &first);
    ready(&project, &access, "ready-a", &first_stage.id);
    let second_stage = stage(&project, &access, "stage-b", &second);
    ready(&project, &access, "ready-b", &second_stage.id);
    let third_stage = stage(&project, &access, "stage-c", &third);
    ready(&project, &access, "ready-c", &third_stage.id);
    drop(project);

    let connection = Connection::open(temp.child("story").join("project.sqlite3")).unwrap();
    connection
        .execute("UPDATE documents SET position=4 WHERE id='a'", [])
        .unwrap();
    connection
        .execute("UPDATE documents SET position=2 WHERE id='b'", [])
        .unwrap();
    connection
        .execute("UPDATE documents SET position=2 WHERE id='c'", [])
        .unwrap();
    drop(connection);

    let reopened = ProjectSession::open(temp.child("story")).unwrap();
    let access = reopened.attach("reorder-review".into()).unwrap();
    let stale = reopened
        .chapter_review_status(access.clone(), "c".into())
        .unwrap();
    assert_eq!(stale.state, ReviewState::EarlierBasisChanged);
    assert!(!stale.can_stage);

    let current_b = reopened.document(access.clone(), "b".into()).unwrap();
    let reaffirm_b = stage(&reopened, &access, "stage-b-reordered", &current_b);
    assert!(reaffirm_b.prefix.is_empty());
    ready(&reopened, &access, "ready-b-reordered", &reaffirm_b.id);
    let current_c = reopened.document(access.clone(), "c".into()).unwrap();
    let reaffirm_c = stage(&reopened, &access, "stage-c-reordered", &current_c);
    assert_eq!(reaffirm_c.prefix.len(), 1);
    assert_eq!(reaffirm_c.prefix[0].document_id, "b");
    ready(&reopened, &access, "ready-c-reordered", &reaffirm_c.id);
    assert_eq!(
        reopened
            .chapter_review_status(access, "c".into())
            .unwrap()
            .state,
        ReviewState::Ready
    );
}

#[test]
fn review_operations_replay_after_writer_lease_rotation() {
    let (_temp, project, access_one) = setup();
    let first = chapter(&project, &access_one, "chapter-1", "One", "First.");
    let staged = stage(&project, &access_one, "stage-lease", &first);
    let access_two = project.attach("review-test-new-lease".into()).unwrap();

    let old_stage = project
        .stage_author_review(StageAuthorReview {
            knowledge: None,
            access: access_one.clone(),
            operation_id: "stage-lease".into(),
            expected: first.head.clone(),
            records: None,
            promises: None,
            summary: None,
        })
        .unwrap_err();
    assert_eq!(old_stage.code, "WriterLeaseExpired");
    let replayed_stage = project
        .stage_author_review(StageAuthorReview {
            knowledge: None,
            access: access_two.clone(),
            operation_id: "stage-lease".into(),
            expected: first.head,
            records: None,
            promises: None,
            summary: None,
        })
        .unwrap();
    assert_eq!(replayed_stage.id, staged.id);

    let old_ready = project
        .mark_ready(MarkReady {
            access: access_one,
            operation_id: "ready-lease".into(),
            stage_id: staged.id.clone(),
        })
        .unwrap_err();
    assert_eq!(old_ready.code, "WriterLeaseExpired");
    let bundle = ready(&project, &access_two, "ready-lease", &staged.id);
    let replay = ready(&project, &access_two, "ready-lease", &staged.id);
    assert_eq!(replay.id, bundle.id);
}

#[test]
fn reviewed_evidence_accepts_unknown_reader_holder_and_exact_replay() {
    let (_temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "The first chapter.");
    let records = vec![evidence(
        "The first chapter.",
        "possession-1",
        EvidenceAudience::Reader,
        None,
    )];
    let staged = stage_with_records(
        &project,
        &access,
        "stage-evidence",
        &first,
        Some(records.clone()),
    );
    assert_eq!(staged.records.as_ref(), Some(&records));
    assert!(staged.records_hash.is_some());
    let replay = stage_with_records(
        &project,
        &access,
        "stage-evidence",
        &first,
        Some(records.clone()),
    );
    assert_eq!(replay.id, staged.id);

    let bundle = ready(&project, &access, "ready-evidence", &staged.id);
    assert_eq!(bundle.records.as_ref(), Some(&records));
    let selected = project
        .read_reviewed_record_set(access, "chapter-1".into())
        .unwrap()
        .expect("selected evidence set");
    assert!(selected.current);
    assert_eq!(selected.records, records);
    assert_eq!(selected.records_hash, bundle.records_hash);
}

#[test]
fn replacing_only_evidence_fences_later_bundle_and_keeps_old_bundle_immutable() {
    let (_temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "The first chapter.");
    let second = chapter(&project, &access, "chapter-2", "Two", "The second chapter.");
    let first_record = evidence(
        "The first chapter.",
        "possession-old",
        EvidenceAudience::AuthorRoom,
        Some("hero"),
    );
    let first_stage = stage_with_records(
        &project,
        &access,
        "stage-first-old-evidence",
        &first,
        Some(vec![first_record.clone()]),
    );
    let old_bundle = ready(
        &project,
        &access,
        "ready-first-old-evidence",
        &first_stage.id,
    );
    let second_stage = stage(&project, &access, "stage-second", &second);
    let second_bundle = ready(&project, &access, "ready-second", &second_stage.id);

    let replacement = evidence(
        "The first chapter.",
        "possession-new",
        EvidenceAudience::Reader,
        None,
    );
    let replacement_stage = stage_with_records(
        &project,
        &access,
        "stage-first-new-evidence",
        &first,
        Some(vec![replacement.clone()]),
    );
    let replacement_bundle = ready(
        &project,
        &access,
        "ready-first-new-evidence",
        &replacement_stage.id,
    );
    assert_ne!(replacement_bundle.id, old_bundle.id);
    assert_eq!(old_bundle.records, Some(vec![first_record]));
    assert_eq!(replacement_bundle.records, Some(vec![replacement.clone()]));
    assert_eq!(
        project
            .chapter_review_status(access.clone(), "chapter-2".into())
            .unwrap()
            .state,
        ReviewState::EarlierBasisChanged
    );
    let selected = project
        .read_reviewed_record_set(access, "chapter-1".into())
        .unwrap()
        .expect("replacement evidence set");
    assert!(selected.current);
    assert_eq!(selected.records, vec![replacement]);
    assert_eq!(second_bundle.records, None);
}

#[test]
fn inherited_evidence_is_revalidated_against_new_revision() {
    let (_temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "The first chapter.");
    let original = evidence(
        "The first chapter.",
        "possession-inherited",
        EvidenceAudience::AuthorRoom,
        Some("hero"),
    );
    let staged = stage_with_records(
        &project,
        &access,
        "stage-inherited-original",
        &first,
        Some(vec![original]),
    );
    ready(&project, &access, "ready-inherited-original", &staged.id);
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-inherited-source".into(),
            expected: first.head,
            local_generation: "1".into(),
            body: body("The changed chapter."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let changed = project
        .document(access.clone(), "chapter-1".into())
        .unwrap();
    let error = project
        .stage_author_review(StageAuthorReview {
            knowledge: None,
            access: access.clone(),
            operation_id: "stage-invalid-inheritance".into(),
            expected: changed.head.clone(),
            records: None,
            promises: None,
            summary: None,
        })
        .unwrap_err();
    assert_eq!(error.code, "InvalidReviewedRecords");

    let cleared = stage_with_records(
        &project,
        &access,
        "stage-explicit-clear",
        &changed,
        Some(Vec::new()),
    );
    assert_eq!(cleared.records, None);
    let cleared_bundle = ready(&project, &access, "ready-explicit-clear", &cleared.id);
    assert_eq!(cleared_bundle.records, None);
}

#[test]
fn recovered_backup_preserves_historical_reviewed_evidence() {
    let (temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "The first chapter.");
    let records = vec![evidence(
        "The first chapter.",
        "possession-recovery",
        EvidenceAudience::AuthorRoom,
        Some("hero"),
    )];
    let staged = stage_with_records(
        &project,
        &access,
        "stage-recovery-evidence",
        &first,
        Some(records.clone()),
    );
    let bundle = ready(&project, &access, "ready-recovery-evidence", &staged.id);
    let archive = temp.child("evidence.wnsbackup");
    create_backup(&project, &archive).unwrap();
    drop(project);

    let recovered_path = temp.child("recovered-evidence");
    let recovered = recover_backup(&archive, &recovered_path, "Recovered evidence").unwrap();
    drop(recovered);
    let connection = Connection::open(recovered_path.join("project.sqlite3")).unwrap();
    let (records_json, records_hash): (String, String) = connection
        .query_row(
            "SELECT records_json,records_hash FROM ready_bundles WHERE id=?",
            [&bundle.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<PossessionRecord>>(&records_json).unwrap(),
        records
    );
    assert_eq!(bundle.records_hash.as_deref(), Some(records_hash.as_str()));
}

#[test]
fn schema20_archive_migrates_legacy_empty_evidence_rows() {
    let (temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "The first chapter.");
    let staged = stage(&project, &access, "stage-schema20-evidence", &first);
    let bundle = ready(&project, &access, "ready-schema20-evidence", &staged.id);
    let current_archive = temp.child("evidence-current.wnsbackup");
    create_backup(&project, &current_archive).unwrap();

    let legacy_archive = temp.child("evidence-schema20.wnsbackup");
    let current_file = fs::File::open(&current_archive).unwrap();
    let mut current_zip = zip::ZipArchive::new(current_file).unwrap();
    let mut manifest = String::new();
    current_zip
        .by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    let mut database = Vec::new();
    current_zip
        .by_name("project.sqlite3")
        .unwrap()
        .read_to_end(&mut database)
        .unwrap();
    let legacy_db = temp.child("schema20.sqlite3");
    fs::write(&legacy_db, database).unwrap();
    let connection = Connection::open(&legacy_db).unwrap();
    legacy_schema::remove_schema24_features(&connection).unwrap();
    connection
        .execute_batch(
            "ALTER TABLE review_stages DROP COLUMN records_json;
             ALTER TABLE review_stages DROP COLUMN records_hash;
             ALTER TABLE ready_bundles DROP COLUMN records_json;
             ALTER TABLE ready_bundles DROP COLUMN records_hash;
             ALTER TABLE review_stages DROP COLUMN promises_json;
             ALTER TABLE review_stages DROP COLUMN promises_hash;
             ALTER TABLE ready_bundles DROP COLUMN promises_json;
             ALTER TABLE ready_bundles DROP COLUMN promises_hash;
             PRAGMA user_version=20;",
        )
        .unwrap();
    drop(connection);
    let legacy_database = fs::read(&legacy_db).unwrap();
    let mut manifest_value: Value = serde_json::from_str(&manifest).unwrap();
    manifest_value["databaseSchemaVersion"] = json!(20);
    manifest_value["databaseSha256"] = json!(hash_bytes(&legacy_database));
    let output = fs::File::create(&legacy_archive).unwrap();
    let mut writer = zip::ZipWriter::new(output);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("manifest.json", options).unwrap();
    writer
        .write_all(serde_json::to_string(&manifest_value).unwrap().as_bytes())
        .unwrap();
    writer.start_file("project.sqlite3", options).unwrap();
    writer.write_all(&legacy_database).unwrap();
    writer.finish().unwrap();

    let recovered_path = temp.child("schema20-recovered");
    let recovered = recover_backup(&legacy_archive, &recovered_path, "Recovered schema20").unwrap();
    drop(recovered);
    let connection = Connection::open(recovered_path.join("project.sqlite3")).unwrap();
    let (schema, migrated_bundle, records_json, records_hash):
        (i64, String, Option<String>, Option<String>) =
        connection
            .query_row(
                "SELECT (SELECT user_version FROM pragma_user_version),id,records_json,records_hash FROM ready_bundles WHERE id=?",
                [&bundle.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
    assert_eq!(schema, 36);
    assert_eq!(migrated_bundle, bundle.id);
    assert_eq!(records_json, None);
    assert_eq!(records_hash, None);
}

#[test]
fn reviewed_freeze_remains_explicitly_unavailable() {
    let (_temp, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "One", "First.");
    let policy = InformationPolicy {
        version: project.context_epochs(access.clone()).unwrap().policy,
        audience: Audience::AuthorRoom,
        reader_frontier: None,
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    };
    let error = project
        .freeze_story(FreezeStory {
            access,
            operation_id: "reviewed-freeze".into(),
            expected: first.head,
            basis: BasisKind::Reviewed,
            purpose: ContextPurpose::StoryQuestion,
            policy,
        })
        .unwrap_err();
    assert_eq!(error.code, "BasisUnavailable");
}
