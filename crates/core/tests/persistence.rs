use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::projects::*;

struct TempProject {
    path: PathBuf,
}
impl TempProject {
    fn new() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("wns-persistence-{}", Uuid::new_v4())),
        }
    }
    fn create(&self) -> ProjectSession {
        ProjectSession::create(&self.path, "The lantern keeper").unwrap()
    }
}
impl Drop for TempProject {
    fn drop(&mut self) {
        if self.path.starts_with(std::env::temp_dir()) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
fn body(text: &str) -> Value {
    let mut block = json!({"type":"paragraph","attrs":{"id":"p1"}});
    if !text.is_empty() {
        block["content"] = json!([{"type":"text","text":text}]);
    }
    json!({"schemaVersion":1,"body":{"type":"doc","content":[block]}})
}
fn setup(project: &ProjectSession) -> (ProjectAccess, DocumentRecord) {
    let access = project.documents().attach("renderer-one".into()).unwrap();
    let document = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "new-document".into(),
            document_id: "chapter-one".into(),
            title: "An empty harbour".into(),
            kind: "chapter".into(),
            body: body(""),
        })
        .unwrap();
    (access, document)
}
fn save(
    access: &ProjectAccess,
    head: &Head,
    op: &str,
    generation: &str,
    text: &str,
) -> SaveSnapshot {
    SaveSnapshot {
        access: access.clone(),
        operation_id: op.into(),
        expected: head.clone(),
        local_generation: generation.into(),
        body: body(text),
        cause: SaveCause::Typing,
    }
}
fn reconcile(project: &ProjectSession, session: &str, ids: &[&str]) -> ReconciledDocument {
    project
        .documents().reconcile(ReconcileRequest {
            project_id: project.info.project_id.clone(),
            operation_namespace: project.info.operation_namespace.clone(),
            session: session.into(),
            document_id: "chapter-one".into(),
            pending_operation_ids: ids.iter().map(|v| (*v).into()).collect(),
        })
        .unwrap()
}

#[test]
fn file_backed_save_reopen_and_durability_configuration() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, document) = setup(&project);
    let info = project.project().storage().unwrap();
    assert_eq!(
        (&*info.journal_mode, info.synchronous, info.foreign_keys),
        ("wal", 2, 1)
    );
    assert!(!info.sqlite_source_id.is_empty());
    assert!(!info.compile_options.is_empty());
    let ack = project
        .documents().save(save(
            &access,
            &document.head,
            "save-one",
            "41",
            "Mei waited. 👩‍🚀 e\u{301}",
        ))
        .unwrap();
    assert_eq!(
        (ack.head.version.as_str(), ack.saved_generation.as_str()),
        ("1", "41")
    );
    let info_before = project.info.clone();
    drop(project);
    let project = ProjectSession::open(&temp.path).unwrap();
    assert_eq!(project.info, info_before);
    let recovered = reconcile(&project, "renderer-two", &["save-one"]);
    assert_eq!(recovered.document.head, ack.head);
    assert_eq!(recovered.document.body, body("Mei waited. 👩‍🚀 e\u{301}"));
    assert_eq!(recovered.receipts.len(), 1);
}

#[test]
fn receipt_replay_is_idempotent_and_payload_and_kind_are_bound() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, doc) = setup(&project);
    let request = save(&access, &doc.head, "save-one", "1", "A lantern.");
    let first = project.documents().save(request.clone()).unwrap();
    assert_eq!(project.documents().save(request.clone()).unwrap(), first);
    let mut changed = request.clone();
    changed.body = body("Another lantern.");
    assert_eq!(
        project.documents().save(changed).unwrap_err().code,
        "OperationIdReusedWithDifferentPayload"
    );
    let mut changed = request;
    changed.local_generation = "2".into();
    assert_eq!(
        project.documents().save(changed).unwrap_err().code,
        "OperationIdReusedWithDifferentPayload"
    );
    let reused_kind = save(&access, &doc.head, "new-document", "0", "");
    assert_eq!(
        project.documents().save(reused_kind).unwrap_err().code,
        "OperationIdReusedWithDifferentPayload"
    );
    assert_eq!(
        project.documents().read(access, "chapter-one".into()).unwrap().head,
        first.head
    );
}

#[test]
fn exact_version_and_hash_cas_and_canonical_decimal_versions() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, doc) = setup(&project);
    let first = project
        .documents().save(save(&access, &doc.head, "save-one", "1", "A lantern."))
        .unwrap();
    let error = project
        .documents().save(save(&access, &doc.head, "late-save", "2", "Old body"))
        .unwrap_err();
    assert_eq!(error.code, "VersionConflict");
    assert_eq!(error.current_head, Some(first.head.clone()));
    let mut wrong = first.head.clone();
    wrong.body_hash = doc.head.body_hash;
    assert_eq!(
        project
            .documents().save(save(&access, &wrong, "wrong-hash", "2", "Bad"))
            .unwrap_err()
            .code,
        "VersionConflict"
    );
    for version in ["01", "-1", "1.0", "9223372036854775808", ""] {
        let mut wrong = first.head.clone();
        wrong.version = version.into();
        assert_eq!(
            project
                .documents().save(save(&access, &wrong, "wrong-version", "2", "Bad"))
                .unwrap_err()
                .code,
            "InvalidRequest"
        );
    }
}

#[test]
fn a_noop_save_has_a_receipt_without_bumping_version() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, doc) = setup(&project);
    let ack = project
        .documents().save(save(&access, &doc.head, "noop", "77", ""))
        .unwrap();
    assert_eq!(ack.head, doc.head);
    assert_eq!(ack.saved_generation, "77");
    assert!(
        project
            .documents().history(access, "chapter-one".into())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        reconcile(&project, "renderer-one", &["noop"])
            .receipts
            .len(),
        1
    );
}

#[test]
fn reconciliation_fences_old_writes_and_returns_latest_not_receipt_head() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, doc) = setup(&project);
    let original = save(&access, &doc.head, "first", "1", "First");
    let first = project.documents().save(original.clone()).unwrap();
    let second = project
        .documents().save(save(&access, &first.head, "second", "2", "Newer"))
        .unwrap();
    let recovered = reconcile(&project, "renderer-two", &["first", "missing"]);
    assert_eq!(recovered.document.head, second.head);
    assert_eq!(recovered.receipts[0].result.head, first.head);
    assert_eq!(
        project
            .documents().save(save(&access, &second.head, "late", "3", "Late"))
            .unwrap_err()
            .code,
        "WriterLeaseExpired"
    );
    let mut replay = original;
    replay.access = recovered.access.clone();
    let ack = project.documents().save(replay).unwrap();
    assert_eq!(ack.head, first.head);
    assert_eq!(ack.session, "renderer-two");
    assert_eq!(
        project
            .documents().read(recovered.access, "chapter-one".into())
            .unwrap()
            .head,
        second.head
    );
}

#[test]
fn retired_renderer_cannot_reacquire_a_lease_with_a_late_reconciliation() {
    let temp = TempProject::new();
    let project = temp.create();
    let (old, doc) = setup(&project);
    let current = reconcile(&project, "new-renderer", &[]);
    assert_eq!(
        project.documents().attach(old.session.clone()).unwrap_err().code,
        "WriterLeaseExpired"
    );
    let error = project
        .documents().reconcile(ReconcileRequest {
            project_id: old.project_id,
            operation_namespace: old.operation_namespace,
            session: old.session,
            document_id: "chapter-one".into(),
            pending_operation_ids: vec![],
        })
        .unwrap_err();
    assert_eq!(error.code, "WriterLeaseExpired");
    project
        .documents().save(save(
            &current.access,
            &doc.head,
            "current-save",
            "1",
            "The current renderer still owns this chapter.",
        ))
        .unwrap();
}

#[test]
fn uncertain_commit_in_save_create_or_checkpoint_fences_and_reopens() {
    for kind in ["save", "create", "checkpoint"] {
        let temp = TempProject::new();
        let project = temp.create();
        let (access, doc) = setup(&project);
        let connection = Connection::open(temp.path.join("project.sqlite3")).unwrap();
        connection.execute_batch("CREATE TABLE deferred_fault(id TEXT REFERENCES documents(id) DEFERRABLE INITIALLY DEFERRED);").unwrap();
        let table = if kind == "checkpoint" {
            "revisions"
        } else {
            "command_receipts"
        };
        connection.execute_batch(&format!("CREATE TRIGGER inject_commit_failure AFTER INSERT ON {table} BEGIN INSERT INTO deferred_fault VALUES('missing-document'); END;")).unwrap();
        let error = match kind {
            "save" => project
                .documents().save(save(&access, &doc.head, "fault", "1", "Uncommitted"))
                .unwrap_err(),
            "create" => project
                .documents().create(CreateDocument {
                    access: access.clone(),
                    operation_id: "fault".into(),
                    document_id: "new".into(),
                    title: "New".into(),
                    kind: "note".into(),
                    body: blank_document(),
                })
                .unwrap_err(),
            _ => project
                .documents().checkpoint(CheckpointRequest {
                    access: access.clone(),
                    expected: doc.head.clone(),
                    reason: CheckpointReason::Manual,
                })
                .unwrap_err(),
        };
        assert_eq!(error.code, "UncertainOutcome", "{kind}");
        assert_eq!(
            project.documents().attach(access.session.clone()).unwrap_err().code,
            "UncertainOutcome"
        );
        assert!(
            project
                .documents().save(save(&access, &doc.head, "must-not-write", "2", "Blocked"))
                .is_err()
        );
        connection
            .execute_batch("DROP TRIGGER inject_commit_failure")
            .unwrap();
        drop(connection);
        let recovered = reconcile(&project, "new-renderer", &["fault"]);
        assert_eq!(recovered.document.head, doc.head);
        assert!(recovered.receipts.is_empty());
        assert!(
            project
                .documents().history(recovered.access.clone(), "chapter-one".into())
                .unwrap()
                .is_empty()
        );
        assert_eq!(project.project().storage().unwrap().synchronous, 2);
        project
            .documents().save(save(
                &recovered.access,
                &doc.head,
                "after-recovery",
                "3",
                "Recovered safely",
            ))
            .unwrap();
    }
}

#[test]
fn project_identity_and_namespace_are_checked_on_every_write() {
    let a = TempProject::new();
    let pa = a.create();
    let (aa, da) = setup(&pa);
    let b = TempProject::new();
    let pb = b.create();
    let (ab, db) = setup(&pb);
    assert_eq!(
        pa.documents().save(save(&ab, &da.head, "wrong-project", "1", "No"))
            .unwrap_err()
            .code,
        "WrongProjectSession"
    );
    let mut wrong = aa.clone();
    wrong.operation_namespace = ab.operation_namespace.clone();
    assert_eq!(
        pa.documents().save(save(&wrong, &da.head, "wrong-namespace", "1", "No"))
            .unwrap_err()
            .code,
        "WrongProjectSession"
    );
    assert_eq!(
        pb.documents().read(ab.clone(), "chapter-one".into()).unwrap().head,
        db.head
    );
    assert!(
        pa.documents().reconcile(ReconcileRequest {
            project_id: pa.info.project_id.clone(),
            operation_namespace: "old-namespace".into(),
            session: "a".into(),
            document_id: "chapter-one".into(),
            pending_operation_ids: vec![]
        })
        .is_err()
    );
}

#[test]
fn checkpoints_are_immutable_reused_and_owned_by_the_document() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, doc) = setup(&project);
    let request = CheckpointRequest {
        access: access.clone(),
        expected: doc.head.clone(),
        reason: CheckpointReason::Manual,
    };
    let before = project.documents().checkpoint(request.clone()).unwrap();
    assert_eq!(project.documents().checkpoint(request).unwrap().id, before.id);
    let mut change = save(&access, &doc.head, "undo", "1", "Reverted prose");
    change.cause = SaveCause::Undo;
    let after = project.documents().save(change).unwrap();
    let history = project
        .documents().history(access.clone(), "chapter-one".into())
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].head, after.head);
    assert_eq!(history[0].parent_id, Some(before.id.clone()));
    assert_eq!(
        project
            .documents().checkpoint(CheckpointRequest {
                access,
                expected: doc.head,
                reason: CheckpointReason::Source
            })
            .unwrap_err()
            .code,
        "VersionConflict"
    );
    let connection = Connection::open(temp.path.join("project.sqlite3")).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    assert!(
        connection
            .execute(
                "UPDATE revisions SET reason='changed' WHERE id=?",
                [&before.id]
            )
            .is_err()
    );
    assert!(
        connection
            .execute("DELETE FROM revisions WHERE id=?", [&before.id])
            .is_err()
    );
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
}

#[test]
fn transaction_failure_after_document_update_rolls_back_body_checkpoint_and_receipt() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, doc) = setup(&project);
    // Failure injection lives only in this synthetic test database, never in a shipping command.
    let connection = Connection::open(temp.path.join("project.sqlite3")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON command_receipts WHEN NEW.operation_id='fault' BEGIN SELECT RAISE(ABORT,'test disk failure'); END;").unwrap();
    let mut request = save(&access, &doc.head, "fault", "1", "Should roll back");
    request.cause = SaveCause::Undo;
    assert_eq!(
        project.documents().save(request).unwrap_err().code,
        "PersistenceUnavailable"
    );
    assert_eq!(
        project
            .documents().read(access.clone(), "chapter-one".into())
            .unwrap()
            .head,
        doc.head
    );
    assert!(
        project
            .documents().history(access, "chapter-one".into())
            .unwrap()
            .is_empty()
    );
    assert!(
        reconcile(&project, "renderer-one", &["fault"])
            .receipts
            .is_empty()
    );
}

#[test]
fn os_lock_survives_handle_clone_and_releases_when_owner_ends() {
    let temp = TempProject::new();
    let project = temp.create();
    let clone = project.clone();
    drop(project);
    assert_eq!(
        ProjectSession::open(&temp.path).err().unwrap().code,
        "ProjectAlreadyOpen"
    );
    let alias = temp.path.join(".");
    assert_eq!(
        ProjectSession::open(alias).err().unwrap().code,
        "ProjectAlreadyOpen"
    );
    drop(clone);
    assert!(ProjectSession::open(&temp.path).is_ok());
}

#[test]
fn refuses_existing_creation_mismatched_marker_and_newer_database() {
    let temp = TempProject::new();
    let project = temp.create();
    let info = project.info.clone();
    drop(project);
    assert!(ProjectSession::create(&temp.path, "Replacement").is_err());
    let marker_path = temp.path.join("project.wns.json");
    let mut wrong = info.clone();
    wrong.project_id = "wrong".into();
    std::fs::write(&marker_path, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert_eq!(
        ProjectSession::open(&temp.path).err().unwrap().code,
        "InvalidProject"
    );
    std::fs::write(&marker_path, serde_json::to_vec(&info).unwrap()).unwrap();
    let connection = Connection::open(temp.path.join("project.sqlite3")).unwrap();
    connection.pragma_update(None, "user_version", 99).unwrap();
    drop(connection);
    assert_eq!(
        ProjectSession::open(&temp.path).err().unwrap().code,
        "UnsupportedSchema"
    );
}

#[test]
fn malformed_snapshot_leaves_working_body_and_receipts_untouched() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, doc) = setup(&project);
    let mut request = save(&access, &doc.head, "malformed", "1", "No");
    request.body["body"]["content"][0]["attrs"]["unexpected"] = json!(true);
    assert_eq!(project.documents().save(request).unwrap_err().code, "InvalidDocument");
    assert_eq!(
        project.documents().read(access, "chapter-one".into()).unwrap().head,
        doc.head
    );
    assert!(reconcile(&project, "r", &["malformed"]).receipts.is_empty());
}

#[test]
fn checkpoint_parent_cannot_cross_documents_even_through_sql() {
    let temp = TempProject::new();
    let project = temp.create();
    let (access, doc) = setup(&project);
    let revision = project
        .documents().checkpoint(CheckpointRequest {
            access: access.clone(),
            expected: doc.head,
            reason: CheckpointReason::Manual,
        })
        .unwrap();
    let other = project
        .documents().create(CreateDocument {
            access,
            operation_id: "another".into(),
            document_id: "other".into(),
            title: "A character".into(),
            kind: "character".into(),
            body: blank_document(),
        })
        .unwrap();
    let connection = Connection::open(temp.path.join("project.sqlite3")).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    assert!(connection.execute("INSERT INTO revisions(id,document_id,source_working_version,schema_version,body_json,body_hash,parent_id,reason) VALUES('bad-parent','other',0,1,?,?,?,'manual')",params![serde_json::to_string(&other.body).unwrap(),other.head.body_hash,revision.id]).is_err());
    assert!(
        connection
            .execute(
                "UPDATE documents SET last_checkpoint_id=? WHERE id='other'",
                [revision.id]
            )
            .is_err()
    );
}
