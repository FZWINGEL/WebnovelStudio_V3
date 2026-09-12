use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::documents::{Endpoint, ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::discussions::{DiscussionScopeInput, FeedbackIntent, StartDiscussion};
use webnovel_core::projects::source_pins::{SaveSourcePins, SourcePinScope};
use webnovel_core::projects::{CreateDocument, ProjectAccess, ProjectSession};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-source-pins-{}", Uuid::new_v4()));
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
        "body": {"type": "doc", "content": [{
            "type": "paragraph", "attrs": {"id": "p1"},
            "content": [{"type": "text", "text": text}]
        }]}
    })
}

fn setup(
    root: &Path,
) -> (
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let project = ProjectSession::create(root, "Source pins").expect("create project");
    let access = project.documents().attach("source-pin-session".into()).expect("attach");
    let chapter = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "chapter-create".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("A chapter passage."),
        })
        .expect("create chapter");
    project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "note-create".into(),
            document_id: "note-one".into(),
            title: "Author note".into(),
            kind: "note".into(),
            body: body("An author-only source."),
        })
        .expect("create note");
    (project, access, chapter)
}

fn save_on(
    project: &ProjectSession,
    access: &ProjectAccess,
    operation_id: &str,
    scope: SourcePinScope,
    target_document_id: Option<&str>,
    expected_version: &str,
    source_document_ids: &[&str],
) -> webnovel_core::projects::source_pins::SourcePinSet {
    project
        .save_source_pins(SaveSourcePins {
            access: access.clone(),
            operation_id: operation_id.into(),
            scope,
            target_document_id: target_document_id.map(str::to_owned),
            expected_version: expected_version.into(),
            source_document_ids: source_document_ids.iter().map(|id| (*id).into()).collect(),
        })
        .expect("save source pins")
}

#[test]
fn scopes_are_sorted_versioned_cas_safe_and_reopenable() {
    let temp = TempDir::new();
    let path = temp.child("project");
    let (project, access, chapter) = setup(&path);
    let before = project.context().source_epoch().expect("epoch");
    let saved = save_on(
        &project,
        &access,
        "pin-project",
        SourcePinScope::Project,
        None,
        "",
        &["note-one"],
    );
    assert_eq!(saved.version, "1");
    assert_eq!(saved.source_document_ids, vec!["note-one"]);
    assert_eq!(saved.target_document_id, None);
    assert_eq!(
        project.context().source_epoch().unwrap().to_string(),
        (before.parse::<u64>().unwrap() + 1).to_string()
    );

    let noop_epoch = project.context().source_epoch().unwrap();
    let replay = save_on(
        &project,
        &access,
        "pin-project-noop",
        SourcePinScope::Project,
        None,
        "1",
        &["note-one"],
    );
    assert_eq!(replay, saved);
    assert_eq!(project.context().source_epoch().unwrap(), noop_epoch);

    let stale = project
        .save_source_pins(SaveSourcePins {
            access: access.clone(),
            operation_id: "pin-stale".into(),
            scope: SourcePinScope::Project,
            target_document_id: None,
            expected_version: "0".into(),
            source_document_ids: Vec::new(),
        })
        .unwrap_err();
    assert_eq!(stale.code, "SourcePinVersionConflict");

    let document = save_on(
        &project,
        &access,
        "pin-document",
        SourcePinScope::Document,
        Some(&chapter.head.document_id),
        "0",
        &["note-one"],
    );
    assert_eq!(document.target_document_id.as_deref(), Some("chapter-one"));
    let view = project
        .read_source_pins(access.clone(), chapter.head.document_id.clone())
        .expect("read source pins");
    assert_eq!(view.project.version, "1");
    assert_eq!(view.document.version, "1");
    drop(project);

    let reopened = ProjectSession::open(&path).expect("reopen project");
    let access = reopened.documents().attach("reopened".into()).expect("reattach");
    let view = reopened
        .read_source_pins(access, "chapter-one".into())
        .expect("read after reopen");
    assert_eq!(view.project.source_document_ids, vec!["note-one"]);
    assert_eq!(view.document.source_document_ids, vec!["note-one"]);
}

#[test]
fn missing_or_duplicate_sources_and_wrong_scope_targets_fail_closed() {
    let temp = TempDir::new();
    let (project, access, chapter) = setup(&temp.child("project"));
    let missing = project
        .save_source_pins(SaveSourcePins {
            access: access.clone(),
            operation_id: "pin-missing".into(),
            scope: SourcePinScope::Project,
            target_document_id: None,
            expected_version: "0".into(),
            source_document_ids: vec!["missing".into()],
        })
        .unwrap_err();
    assert_eq!(missing.code, "DocumentNotFound");
    let duplicate = project
        .save_source_pins(SaveSourcePins {
            access: access.clone(),
            operation_id: "pin-duplicate".into(),
            scope: SourcePinScope::Project,
            target_document_id: None,
            expected_version: "0".into(),
            source_document_ids: vec!["note-one".into(), "note-one".into()],
        })
        .expect("duplicate source IDs are canonicalized");
    assert_eq!(duplicate.source_document_ids, vec!["note-one"]);
    let wrong_target = project
        .save_source_pins(SaveSourcePins {
            access,
            operation_id: "pin-wrong-target".into(),
            scope: SourcePinScope::Project,
            target_document_id: Some(chapter.head.document_id),
            expected_version: "0".into(),
            source_document_ids: Vec::new(),
        })
        .unwrap_err();
    assert_eq!(wrong_target.code, "InvalidRequest");
}

#[test]
fn discussion_uses_persistent_author_sources_but_restricted_edits_do_not() {
    let temp = TempDir::new();
    let (project, access, chapter) = setup(&temp.child("project"));
    save_on(
        &project,
        &access,
        "pin-project",
        SourcePinScope::Project,
        None,
        "0",
        &["note-one"],
    );
    let discussion = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "discussion-persistent".into(),
            expected: chapter.head.clone(),
            instruction: "Discuss the source.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: None,
        })
        .expect("start discussion");
    assert_eq!(discussion.packet.receipt.mandatory_source_handles.len(), 1);
    assert_ne!(
        discussion.packet.receipt.mandatory_source_handles[0],
        discussion.packet.receipt.source_handles[0]
    );

    let target_pin = save_on(
        &project,
        &access,
        "pin-target",
        SourcePinScope::Document,
        Some("chapter-one"),
        "0",
        &["chapter-one"],
    );
    assert_eq!(target_pin.source_document_ids, vec!["chapter-one"]);
    let target_discussion = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "discussion-target-pin".into(),
            expected: chapter.head.clone(),
            instruction: "Discuss the target source too.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: None,
        })
        .expect("start target-pinned discussion");
    // The compiler already includes the target as full text. A persistent
    // target pin therefore does not create a second mandatory source entry.
    assert_eq!(
        target_discussion
            .packet
            .receipt
            .mandatory_source_handles
            .len(),
        1
    );

    let scope = capture_scope(
        &chapter.body,
        ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "p1".into(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: "p1".into(),
                utf16_offset: 9,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .expect("capture passage");
    let restricted = project
        .start_discussion(StartDiscussion {
            access,
            operation_id: "discussion-restricted".into(),
            expected: chapter.head,
            instruction: "Suggest a revision.".into(),
            intent: FeedbackIntent::ProposeEdits,
            basis: None,
            scope: Some(DiscussionScopeInput {
                kind: scope.kind,
                start: scope.start,
                end: scope.end,
                quote: scope.quote,
                source_body_hash: scope.source_hash,
            }),
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: None,
        })
        .expect("start restricted discussion");
    assert!(
        restricted
            .packet
            .receipt
            .mandatory_source_handles
            .is_empty()
    );
}

#[test]
fn retry_keeps_transient_pins_and_refreshes_persistent_pins() {
    let temp = TempDir::new();
    let (project, access, chapter) = setup(&temp.child("project"));
    save_on(
        &project,
        &access,
        "pin-project",
        SourcePinScope::Project,
        None,
        "0",
        &["note-one"],
    );
    let first = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "retry-original".into(),
            expected: chapter.head.clone(),
            instruction: "Discuss this source.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: None,
        })
        .expect("start original");
    project
        .stop_discussion(access.clone(), first.run.id.clone())
        .expect("stop original");
    save_on(
        &project,
        &access,
        "pin-project-changed",
        SourcePinScope::Project,
        None,
        "1",
        &[],
    );
    let retry = project
        .discussion_retry(access.clone(), first.run.id.clone())
        .expect("read retry");
    assert!(retry.pinned_document_ids.is_empty());
    let second = project
        .start_discussion(StartDiscussion {
            access,
            operation_id: "retry-follow-up".into(),
            expected: chapter.head,
            instruction: retry.text,
            intent: retry.intent,
            basis: None,
            scope: retry.scope,
            pinned_document_ids: retry.pinned_document_ids,
            safe_brief: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            previous_run_id: Some(retry.previous_run_id),
            lookup: None,
        })
        .expect("start retry");
    assert!(second.packet.receipt.mandatory_source_handles.is_empty());
}

#[test]
fn source_pin_receipt_collision_is_namespaced_and_immutable() {
    let temp = TempDir::new();
    let path = temp.child("project");
    let (project, access, _) = setup(&path);
    let saved = save_on(
        &project,
        &access,
        "same-operation",
        SourcePinScope::Project,
        None,
        "0",
        &["note-one"],
    );
    let same = project
        .save_source_pins(SaveSourcePins {
            access: access.clone(),
            operation_id: "same-operation".into(),
            scope: SourcePinScope::Project,
            target_document_id: None,
            expected_version: "0".into(),
            source_document_ids: vec!["note-one".into()],
        })
        .expect("receipt replay");
    assert_eq!(same, saved);
    let changed = project
        .save_source_pins(SaveSourcePins {
            access,
            operation_id: "same-operation".into(),
            scope: SourcePinScope::Project,
            target_document_id: None,
            expected_version: "0".into(),
            source_document_ids: Vec::new(),
        })
        .unwrap_err();
    assert_eq!(changed.code, "OperationIdReusedWithDifferentPayload");
    drop(project);
    let db = Connection::open(path.join("project.sqlite3")).expect("open database");
    let immutable = db
        .execute(
            "UPDATE source_pin_receipts SET result_json='{}' WHERE operation_id='same-operation'",
            [],
        )
        .unwrap_err();
    assert!(immutable.to_string().contains("immutable"));
}

#[test]
fn tampered_receipt_result_is_rejected_on_replay_and_backup() {
    let temp = TempDir::new();
    let path = temp.child("project");
    let (project, access, _) = setup(&path);
    save_on(
        &project,
        &access,
        "tampered-result",
        SourcePinScope::Project,
        None,
        "0",
        &["note-one"],
    );
    drop(project);

    let connection = Connection::open(path.join("project.sqlite3")).expect("open database");
    connection
        .execute_batch(
            r#"DROP TRIGGER source_pin_receipts_no_update;
             UPDATE source_pin_receipts
             SET result_json='{"scope":"project","targetDocumentId":null,"version":"1","sourceDocumentIds":["chapter-one"],"audience":"authorRoom"}'
             WHERE operation_id='tampered-result';"#,
        )
        .expect("tamper source-pin receipt result");
    drop(connection);

    let reopened = ProjectSession::open(&path).expect("reopen tampered project");
    let reopened_access = reopened.documents().attach("tampered-replay".into()).unwrap();
    let replay = reopened
        .save_source_pins(SaveSourcePins {
            access: reopened_access,
            operation_id: "tampered-result".into(),
            scope: SourcePinScope::Project,
            target_document_id: None,
            expected_version: "0".into(),
            source_document_ids: vec!["note-one".into()],
        })
        .unwrap_err();
    assert_eq!(replay.code, "InvalidProject");
    drop(reopened);

    let connection = Connection::open(path.join("project.sqlite3")).expect("reopen database");
    connection
        .execute(
            r#"UPDATE source_pin_receipts
             SET result_json='{"scope":"project","targetDocumentId":null,"version":"99","sourceDocumentIds":["note-one"],"audience":"authorRoom"}'
             WHERE operation_id='tampered-result'"#,
            [],
        )
        .expect("tamper source-pin result version");
    drop(connection);
    let reopened = ProjectSession::open(&path).expect("reopen version-tampered project");
    let error = create_backup(&reopened, &temp.child("tampered.wnsbackup"))
        .expect_err("invalid receipt version must reject backup");
    assert_eq!(error.code, "InvalidBackup");
}

#[test]
fn recovery_rebinds_both_pin_scopes_but_keeps_receipts_historical() {
    let temp = TempDir::new();
    let source_path = temp.child("source");
    let (project, access, chapter) = setup(&source_path);
    save_on(
        &project,
        &access,
        "recover-project-pin",
        SourcePinScope::Project,
        None,
        "0",
        &["note-one"],
    );
    save_on(
        &project,
        &access,
        "recover-document-pin",
        SourcePinScope::Document,
        Some("chapter-one"),
        "0",
        &["note-one"],
    );
    let old_project_id = project.info.project_id.clone();
    let old_namespace = project.info.operation_namespace.clone();
    let backup = temp.child("source.wnsbackup");
    create_backup(&project, &backup).expect("create source-pin backup");

    let recovered = recover_backup(&backup, &temp.child("recovered"), "Recovered pins")
        .expect("recover source-pin backup");
    let recovered_access = recovered
        .documents().attach("recovered-source-pins".into())
        .expect("attach recovered copy");
    let view = recovered
        .read_source_pins(recovered_access.clone(), chapter.head.document_id.clone())
        .expect("read recovered source pins");
    assert_eq!(view.project.source_document_ids, vec!["note-one"]);
    assert_eq!(view.document.source_document_ids, vec!["note-one"]);
    assert_ne!(recovered.info.project_id, old_project_id);
    assert_ne!(recovered.info.operation_namespace, old_namespace);

    let connection = Connection::open(recovered.path.join("project.sqlite3"))
        .expect("open recovered source-pin database");
    let (set_project, set_namespace): (String, String) = connection
        .query_row(
            "SELECT project_id,operation_namespace FROM source_pin_sets
             WHERE scope='project' AND target_document_id=''",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read rebound source-pin set");
    assert_eq!(set_project, recovered.info.project_id);
    assert_eq!(set_namespace, recovered.info.operation_namespace);
    let (receipt_project, receipt_namespace): (String, String) = connection
        .query_row(
            "SELECT project_id,operation_namespace FROM source_pin_receipts
             WHERE operation_id='recover-project-pin'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read historical source-pin receipt");
    assert_eq!(receipt_project, old_project_id);
    assert_eq!(receipt_namespace, old_namespace);

    let old_access_replay = recovered
        .save_source_pins(SaveSourcePins {
            access,
            operation_id: "recover-project-pin".into(),
            scope: SourcePinScope::Project,
            target_document_id: None,
            expected_version: "1".into(),
            source_document_ids: vec!["note-one".into()],
        })
        .unwrap_err();
    assert_eq!(old_access_replay.code, "WrongProjectSession");
}

#[test]
fn transfer_allows_stale_source_ids_so_the_author_can_remove_them() {
    let temp = TempDir::new();
    let source_path = temp.child("source");
    let (project, access, _) = setup(&source_path);
    save_on(
        &project,
        &access,
        "stale-source-pin",
        SourcePinScope::Project,
        None,
        "0",
        &["note-one"],
    );
    drop(project);
    let connection =
        Connection::open(source_path.join("project.sqlite3")).expect("open source database");
    connection
        .execute("UPDATE documents SET trashed=1 WHERE id='note-one'", [])
        .expect("trash pinned source");
    drop(connection);

    let reopened = ProjectSession::open(&source_path).expect("reopen source");
    let reopened_access = reopened.documents().attach("stale-source-session".into()).unwrap();
    let replay = reopened
        .save_source_pins(SaveSourcePins {
            access: reopened_access.clone(),
            operation_id: "stale-source-pin".into(),
            scope: SourcePinScope::Project,
            target_document_id: None,
            expected_version: "0".into(),
            source_document_ids: vec!["note-one".into()],
        })
        .expect("a historical receipt replays before source lookup");
    assert_eq!(replay.source_document_ids, vec!["note-one"]);
    let backup = temp.child("stale-source.wnsbackup");
    create_backup(&reopened, &backup).expect("backup stale source pin");
    let recovered = recover_backup(&backup, &temp.child("recovered"), "Recovered stale")
        .expect("recover stale source pin");
    let view = recovered
        .read_source_pins(
            recovered.documents().attach("stale-recovered".into()).unwrap(),
            "chapter-one".into(),
        )
        .expect("read stale source pin for removal");
    assert_eq!(view.project.source_document_ids, vec!["note-one"]);
    let _ = reopened_access;
}

#[test]
fn transfer_rejects_malformed_source_pin_sets_before_backup_install() {
    let temp = TempDir::new();
    let source_path = temp.child("source");
    let (project, access, _) = setup(&source_path);
    save_on(
        &project,
        &access,
        "malformed-source-pin",
        SourcePinScope::Project,
        None,
        "0",
        &["note-one"],
    );
    drop(project);
    let connection =
        Connection::open(source_path.join("project.sqlite3")).expect("open source database");
    connection
        .execute(
            "UPDATE source_pin_sets SET source_document_ids_json='[\"not valid\"]'",
            [],
        )
        .expect("tamper source-pin set");
    drop(connection);
    let reopened = ProjectSession::open(&source_path).expect("reopen source");
    let error = create_backup(&reopened, &temp.child("malformed.wnsbackup"))
        .expect_err("malformed source-pin state must reject backup");
    assert_eq!(error.code, "InvalidBackup");
}

#[test]
fn persistent_pin_budget_failure_does_not_start_a_discussion() {
    let temp = TempDir::new();
    let (project, access, chapter) = setup(&temp.child("project"));
    save_on(
        &project,
        &access,
        "tiny-budget-pin",
        SourcePinScope::Project,
        None,
        "0",
        &["note-one"],
    );
    let error = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "tiny-budget-discussion".into(),
            expected: chapter.head,
            instruction: "Use the pinned source.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("1", "0", "0"),
            provider_binding: None,
            previous_run_id: None,
            lookup: None,
        })
        .unwrap_err();
    assert_eq!(error.code, "ContextPreparationFailed");
    let connection = Connection::open(project.path.join("project.sqlite3")).unwrap();
    let runs: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM discussion_runs WHERE operation_id='tiny-budget-discussion'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(runs, 0);
}

#[test]
fn changing_persistent_pins_stales_an_existing_prepared_packet() {
    let temp = TempDir::new();
    let (project, access, chapter) = setup(&temp.child("project"));
    let started = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "pin-epoch-discussion".into(),
            expected: chapter.head,
            instruction: "Prepare before pins change.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: None,
        })
        .expect("prepare discussion");
    assert!(
        project
            .prepared_context_is_current(access.clone(), started.run.packet_id.clone())
            .expect("check prepared packet")
    );
    save_on(
        &project,
        &access,
        "pin-epoch-change",
        SourcePinScope::Project,
        None,
        "0",
        &["note-one"],
    );
    assert!(
        !project
            .prepared_context_is_current(access, started.run.packet_id)
            .expect("check stale prepared packet")
    );
}

#[test]
fn foreign_project_access_cannot_read_or_save_source_pins() {
    let temp = TempDir::new();
    let (project, access, _) = setup(&temp.child("project-one"));
    let foreign = ProjectSession::create(temp.child("project-two"), "Foreign").unwrap();
    let foreign_access = foreign.documents().attach("foreign-session".into()).unwrap();
    let read = project
        .read_source_pins(foreign_access.clone(), "chapter-one".into())
        .unwrap_err();
    assert_eq!(read.code, "WrongProjectSession");
    let save = project
        .save_source_pins(SaveSourcePins {
            access: foreign_access,
            operation_id: "foreign-pin".into(),
            scope: SourcePinScope::Project,
            target_document_id: None,
            expected_version: "0".into(),
            source_document_ids: Vec::new(),
        })
        .unwrap_err();
    assert_eq!(save.code, "WrongProjectSession");
    let _ = access;
}
