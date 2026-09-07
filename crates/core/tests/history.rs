use rusqlite::Connection;
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::projects::history::RestoreRevision;
use webnovel_core::projects::{
    CheckpointReason, CheckpointRequest, CreateDocument, DocumentRecord, Head, ProjectAccess,
    ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempProject {
    path: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("wns-history-{}", Uuid::new_v4())),
        }
    }

    fn create(&self) -> ProjectSession {
        ProjectSession::create(&self.path, "History test").expect("create project")
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        if self.path.starts_with(std::env::temp_dir()) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn body(text: &str, id: &str) -> Value {
    let mut block = json!({"type":"paragraph","attrs":{"id":id}});
    if !text.is_empty() {
        block["content"] = json!([{"type":"text","text":text}]);
    }
    json!({"schemaVersion":1,"body":{"type":"doc","content":[block]}})
}

fn setup(project: &ProjectSession) -> (ProjectAccess, DocumentRecord) {
    let access = project.attach("history-renderer".into()).expect("attach");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "history-create".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("first", "p1"),
        })
        .expect("create document");
    (access, document)
}

fn checkpoint(
    project: &ProjectSession,
    access: &ProjectAccess,
    head: &Head,
) -> webnovel_core::projects::Revision {
    project
        .checkpoint(CheckpointRequest {
            access: access.clone(),
            expected: head.clone(),
            reason: CheckpointReason::Manual,
        })
        .expect("checkpoint")
}

fn save(
    project: &ProjectSession,
    access: &ProjectAccess,
    head: &Head,
    operation_id: &str,
    text: &str,
) -> webnovel_core::projects::SaveAck {
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: operation_id.into(),
            expected: head.clone(),
            local_generation: "1".into(),
            body: body(text, "p1"),
            cause: SaveCause::Typing,
        })
        .expect("save")
}

#[test]
fn history_is_bounded_and_reads_one_owned_body() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, initial) = setup(&project);
    let first = checkpoint(&project, &access, &initial.head);
    let second = save(&project, &access, &initial.head, "history-save-1", "second");
    let _second_revision = checkpoint(&project, &access, &second.head);
    let third = save(&project, &access, &second.head, "history-save-2", "third");
    let third_revision = checkpoint(&project, &access, &third.head);

    let page = project
        .list_document_history(access.clone(), "chapter-one".into(), None, 1)
        .expect("first page");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, third_revision.id);
    assert_eq!(page.items[0].head.version, "2");
    assert_eq!(page.next_before_version.as_deref(), Some("2"));
    let next = project
        .list_document_history(
            access.clone(),
            "chapter-one".into(),
            page.next_before_version,
            1,
        )
        .expect("second page");
    assert_eq!(next.items[0].id, _second_revision.id);
    assert_eq!(next.items[0].head.version, "1");
    assert_eq!(next.next_before_version.as_deref(), Some("1"));
    let last = project
        .list_document_history(
            access.clone(),
            "chapter-one".into(),
            next.next_before_version,
            1,
        )
        .expect("last page");
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].id, first.id);
    assert_eq!(last.items[0].head.version, "0");
    assert_eq!(last.next_before_version, None);

    let selected = project
        .read_document_revision(access.clone(), "chapter-one".into(), first.id.clone())
        .expect("selected revision");
    assert_eq!(selected.body, body("first", "p1"));
    assert_eq!(selected.head.body_hash, initial.head.body_hash);
    assert!(
        project
            .read_document_revision(access, "other-document".into(), first.id)
            .is_err()
    );
}

#[test]
fn exact_revision_read_survives_trashed_source_but_live_paths_do_not() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, initial) = setup(&project);
    let source = checkpoint(&project, &access, &initial.head);
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "history-create-other".into(),
            document_id: "other-document".into(),
            title: "Other document".into(),
            kind: "world".into(),
            body: body("Other", "other"),
        })
        .expect("create second document");
    drop(project);

    let database = temp.path.join("project.sqlite3");
    let connection = Connection::open(&database).expect("open test db");
    connection
        .execute("UPDATE documents SET trashed=1 WHERE id=?", ["chapter-one"])
        .expect("hide source document");
    drop(connection);

    let project = ProjectSession::open(&temp.path).expect("reopen hidden source project");
    let access = project.attach("history-hidden-renderer".into()).unwrap();
    let retained = project
        .read_document_revision(access.clone(), "chapter-one".into(), source.id.clone())
        .expect("read exact retained revision");
    assert_eq!(retained.body, initial.body);
    assert_eq!(retained.head, source.head);

    let history_error = project
        .list_document_history(access.clone(), "chapter-one".into(), None, 50)
        .expect_err("active history must refuse a hidden source");
    assert_eq!(history_error.code, "DocumentNotFound");
    let foreign_document_error = project
        .read_document_revision(access.clone(), "other-document".into(), source.id.clone())
        .expect_err("a revision must remain owned by its document");
    assert_eq!(foreign_document_error.code, "RevisionDocumentMismatch");

    let other_temp = TempProject::new();
    let other_project = other_temp.create();
    let other_access = other_project
        .attach("other-history-renderer".into())
        .unwrap();
    let wrong_project_error = project
        .read_document_revision(other_access, "chapter-one".into(), source.id.clone())
        .expect_err("a revision read must remain project-bound");
    assert_eq!(wrong_project_error.code, "WrongProjectSession");

    let restore_error = project
        .restore_revision(RestoreRevision {
            access,
            operation_id: "restore-hidden-source".into(),
            expected: initial.head,
            revision_id: source.id,
            revision_hash: source.head.body_hash,
            local_generation: "1".into(),
        })
        .expect_err("restore must refuse a hidden source");
    assert_eq!(restore_error.code, "DocumentNotFound");
}

#[test]
fn restore_advances_head_records_receipt_and_replays_latest_document() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, initial) = setup(&project);
    let source = checkpoint(&project, &access, &initial.head);
    let current = save(&project, &access, &initial.head, "restore-save", "newer");
    let request = RestoreRevision {
        access: access.clone(),
        operation_id: "restore-one".into(),
        expected: current.head.clone(),
        revision_id: source.id.clone(),
        revision_hash: source.head.body_hash.clone(),
        local_generation: "17".into(),
    };
    let restored = project.restore_revision(request.clone()).expect("restore");
    assert!(!restored.already_applied);
    assert_eq!(restored.document.body, initial.body);
    assert_eq!(restored.document.head.version, "2");
    assert_eq!(restored.result.head, restored.document.head);
    let decision = restored.result.restored.clone().expect("restore decision");
    assert_eq!(decision.revision_id, source.id);
    assert_ne!(decision.before_revision_id, decision.after_revision_id);

    let later = save(
        &project,
        &access,
        &restored.document.head,
        "restore-later-save",
        "later",
    );
    let replay = project.restore_revision(request).expect("replay restore");
    assert!(replay.already_applied);
    assert_eq!(replay.result.head.version, "2");
    assert_eq!(replay.document.head, later.head);
    assert_eq!(replay.document.body, body("later", "p1"));
    let reconciled = project
        .reconcile(webnovel_core::projects::ReconcileRequest {
            project_id: project.info.project_id.clone(),
            operation_namespace: project.info.operation_namespace.clone(),
            session: access.session,
            document_id: "chapter-one".into(),
            pending_operation_ids: vec!["restore-one".into()],
        })
        .expect("reconcile");
    assert_eq!(reconciled.receipts.len(), 1);
    assert_eq!(
        reconciled.receipts[0].result.restored,
        replay.result.restored
    );
}

#[test]
fn restore_refuses_stale_heads_foreign_revisions_hashes_and_noops() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, initial) = setup(&project);
    let source = checkpoint(&project, &access, &initial.head);
    let current = save(
        &project,
        &access,
        &initial.head,
        "restore-stale-save",
        "newer",
    );
    let mut stale = current.head.clone();
    stale.version = "0".into();
    assert_eq!(
        project
            .restore_revision(RestoreRevision {
                access: access.clone(),
                operation_id: "restore-stale".into(),
                expected: stale,
                revision_id: source.id.clone(),
                revision_hash: source.head.body_hash.clone(),
                local_generation: "1".into(),
            })
            .expect_err("stale restore")
            .code,
        "VersionConflict"
    );
    assert_eq!(
        project
            .restore_revision(RestoreRevision {
                access: access.clone(),
                operation_id: "restore-hash".into(),
                expected: current.head.clone(),
                revision_id: source.id.clone(),
                revision_hash: "0".repeat(64),
                local_generation: "1".into(),
            })
            .expect_err("hash mismatch")
            .code,
        "RevisionHashMismatch"
    );
    let other = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "history-other".into(),
            document_id: "other-document".into(),
            title: "Other".into(),
            kind: "note".into(),
            body: body("other", "p2"),
        })
        .expect("other document");
    assert_eq!(
        project
            .restore_revision(RestoreRevision {
                access: access.clone(),
                operation_id: "restore-foreign".into(),
                expected: current.head.clone(),
                revision_id: checkpoint(&project, &access, &other.head).id,
                revision_hash: other.head.body_hash,
                local_generation: "1".into(),
            })
            .expect_err("foreign revision")
            .code,
        "RevisionDocumentMismatch"
    );
    let back = save(&project, &access, &current.head, "restore-back", "first");
    let no_op = project
        .restore_revision(RestoreRevision {
            access,
            operation_id: "restore-noop".into(),
            expected: back.head,
            revision_id: source.id,
            revision_hash: source.head.body_hash,
            local_generation: "1".into(),
        })
        .expect_err("same body restore");
    assert_eq!(no_op.code, "NoChanges");
}

#[test]
fn restore_failures_roll_back_every_mutation_stage() {
    let failure_points = [
        "BEFORE UPDATE OF working_version ON documents",
        "BEFORE UPDATE OF context_source_epoch ON project",
        "BEFORE INSERT ON revisions WHEN NEW.reason='beforeRestore'",
        "BEFORE UPDATE OF last_checkpoint_id ON documents",
        "BEFORE INSERT ON revisions WHEN NEW.reason='afterRestore'",
        "BEFORE INSERT ON command_receipts WHEN NEW.operation_id='restore-fault'",
    ];
    for point in failure_points {
        let temp = TempProject::new();
        let project = temp.create();
        let (access, initial) = setup(&project);
        let source = checkpoint(&project, &access, &initial.head);
        let current = save(&project, &access, &initial.head, "fault-current", "newer");
        let epoch = project.context_source_epoch().expect("source epoch");
        let request = RestoreRevision {
            access: access.clone(),
            operation_id: "restore-fault".into(),
            expected: current.head.clone(),
            revision_id: source.id,
            revision_hash: source.head.body_hash,
            local_generation: "3".into(),
        };
        let db = Connection::open(temp.path.join("project.sqlite3")).expect("open test db");
        db.execute_batch(&format!(
            "CREATE TRIGGER restore_failure {point} BEGIN SELECT RAISE(ABORT,'injected restore failure'); END;"
        ))
        .expect("install restore failure trigger");

        let error = project
            .restore_revision(request.clone())
            .expect_err("restore must fail at injected stage");
        assert_eq!(error.code, "PersistenceUnavailable", "{point}");
        let unchanged = project
            .document(access.clone(), "chapter-one".into())
            .expect("unchanged document");
        assert_eq!(unchanged.head, current.head, "{point}");
        assert_eq!(unchanged.body, body("newer", "p1"), "{point}");
        assert_eq!(
            project.context_source_epoch().expect("source epoch"),
            epoch,
            "{point}"
        );
        assert_eq!(
            project
                .history(access.clone(), "chapter-one".into())
                .unwrap()
                .len(),
            1,
            "{point}"
        );
        let receipts: i64 = db
            .query_row(
                "SELECT count(*) FROM command_receipts WHERE operation_id='restore-fault'",
                [],
                |row| row.get(0),
            )
            .expect("receipt count");
        assert_eq!(receipts, 0, "{point}");

        db.execute_batch("DROP TRIGGER restore_failure")
            .expect("remove restore failure trigger");
        let committed = project
            .restore_revision(request)
            .expect("restore after injected failure");
        assert!(!committed.already_applied, "{point}");
        assert_eq!(committed.document.body, initial.body, "{point}");
        assert_eq!(committed.document.head.version, "2", "{point}");
        assert!(committed.result.restored.is_some(), "{point}");
    }
}

#[test]
fn restored_history_survives_backup_and_new_namespace_can_restore_locally() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, initial) = setup(&project);
    let source = checkpoint(&project, &access, &initial.head);
    let current = save(&project, &access, &initial.head, "backup-save", "newer");
    let restored = project
        .restore_revision(RestoreRevision {
            access: access.clone(),
            operation_id: "backup-restore".into(),
            expected: current.head,
            revision_id: source.id.clone(),
            revision_hash: source.head.body_hash.clone(),
            local_generation: "5".into(),
        })
        .expect("restore before backup");
    let backup_current = save(
        &project,
        &access,
        &restored.document.head,
        "backup-later-save",
        "later",
    );
    let archive = temp.path.parent().expect("temp parent").join(format!(
        "{}-history.wnsbackup",
        temp.path.file_name().unwrap().to_string_lossy()
    ));
    create_backup(&project, &archive).expect("backup");
    let recovered_path = temp.path.with_file_name(format!(
        "{}-recovered",
        temp.path.file_name().unwrap().to_string_lossy()
    ));
    let recovered = recover_backup(&archive, &recovered_path, "Recovered").expect("recover");
    let recovered_access = recovered
        .attach("recovered-renderer".into())
        .expect("attach copy");
    let document = recovered
        .document(recovered_access.clone(), "chapter-one".into())
        .expect("copy document");
    assert_eq!(document.head, backup_current.head);
    let historical = recovered
        .list_document_history(recovered_access.clone(), "chapter-one".into(), None, 100)
        .expect("copy history");
    assert!(historical.items.iter().any(|item| item.id == source.id));
    let local = recovered
        .restore_revision(RestoreRevision {
            access: recovered_access,
            operation_id: "copy-restore".into(),
            expected: document.head,
            revision_id: source.id,
            revision_hash: source.head.body_hash,
            local_generation: "6".into(),
        })
        .expect("restore retained copy revision");
    assert_eq!(local.document.body, initial.body);
    assert_ne!(
        local.access.operation_namespace,
        project.info.operation_namespace
    );
}

#[test]
fn backup_rejects_tampered_restore_decision_links() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, initial) = setup(&project);
    let source = checkpoint(&project, &access, &initial.head);
    let current = save(&project, &access, &initial.head, "tamper-current", "newer");
    project
        .restore_revision(RestoreRevision {
            access: access.clone(),
            operation_id: "tamper-restore".into(),
            expected: current.head,
            revision_id: source.id,
            revision_hash: source.head.body_hash,
            local_generation: "8".into(),
        })
        .expect("restore before tamper");

    let db = Connection::open(temp.path.join("project.sqlite3")).expect("open test db");
    db.execute_batch("DROP TRIGGER receipts_no_update")
        .expect("allow synthetic tamper");
    let result_json: String = db
        .query_row(
            "SELECT result_json FROM command_receipts WHERE operation_id='tamper-restore'",
            [],
            |row| row.get(0),
        )
        .expect("restore receipt");
    let mut result: Value = serde_json::from_str(&result_json).expect("receipt JSON");
    let before = result["restored"]["beforeRevisionId"].clone();
    result["restored"]["afterRevisionId"] = before;
    db.execute(
        "UPDATE command_receipts SET result_json=? WHERE operation_id='tamper-restore'",
        [serde_json::to_string(&result).expect("tampered receipt JSON")],
    )
    .expect("tamper receipt");

    let archive = temp
        .path
        .parent()
        .expect("temp parent")
        .join(format!("{}-tampered.wnsbackup", Uuid::new_v4()));
    let error = create_backup(&project, &archive).expect_err("tampered restore must reject");
    assert_eq!(error.code, "InvalidBackup");
    assert!(!archive.exists());
}
