use rusqlite::Connection;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::documents::{Endpoint, ScopeKind};
use webnovel_core::projects::discussions::*;
use webnovel_core::projects::proposals::*;
use webnovel_core::projects::*;
use webnovel_core::transfer::{create_backup, recover_backup};

struct Fixture {
    root: PathBuf,
    project: Option<ProjectSession>,
    access: ProjectAccess,
    document: DocumentRecord,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("wns-proposal-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project = ProjectSession::create(root.join("project"), "Proposal tests").unwrap();
        let access = project.attach("renderer".into()).unwrap();
        let document = project
            .create_document(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: body("selected"),
            })
            .unwrap();
        Self {
            root,
            project: Some(project),
            access,
            document,
        }
    }
    fn project(&self) -> &ProjectSession {
        self.project.as_ref().unwrap()
    }
    fn start(&self, operation: &str) -> DiscussionStart {
        self.project()
            .start_discussion(StartDiscussion {
                access: self.access.clone(),
                operation_id: operation.into(),
                expected: self.document.head.clone(),
                instruction: "Make the selected words clearer; keep the ending.".into(),
                scope: Some(DiscussionScopeInput {
                    kind: ScopeKind::Passage,
                    start: Some(Endpoint {
                        block_id: "p".into(),
                        utf16_offset: 7,
                    }),
                    end: Some(Endpoint {
                        block_id: "p".into(),
                        utf16_offset: 15,
                    }),
                    quote: "selected".into(),
                    source_body_hash: self.document.head.body_hash.clone(),
                }),
                pinned_document_ids: vec![],
                safe_brief: None,
                budget: MockContextBudget::new("100000", "1000", "100"),
                provider_binding: None,
                previous_run_id: None,
                lookup: None,
                intent: FeedbackIntent::ProposeEdits,
                basis: None,
            })
            .unwrap()
    }
    fn complete(&self, start: &DiscussionStart, text: &str) -> DiscussionFinish {
        self.project()
            .begin_discussion_run(DiscussionBegin {
                owner: start.run.owner.clone(),
            })
            .unwrap();
        self.project()
            .mark_discussion_delivered(start.run.owner.clone())
            .unwrap();
        let finish = DiscussionFinish {
            owner: start.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: format!("finish-{}", start.run.id),
            assistant_text: text.into(),
        };
        self.project().finish_discussion(finish.clone()).unwrap();
        finish
    }
    fn candidates(&self) -> Vec<Proposal> {
        let started = self.start("request");
        self.complete(&started, &output());
        self.list()
    }
    fn list(&self) -> Vec<Proposal> {
        self.project()
            .proposals(self.access.clone(), "chapter".into())
            .unwrap()
    }
    fn prepare_request(
        &self,
        proposal: &Proposal,
        text: &str,
        expected: &str,
        operation: &str,
    ) -> PrepareProposal {
        PrepareProposal {
            access: self.access.clone(),
            operation_id: operation.into(),
            proposal_id: proposal.id.clone(),
            expected_prepared_version: expected.into(),
            replacement_text: text.into(),
            body: body(text),
        }
    }
    fn apply_request(&self, proposal: &Proposal, prepared: &PreparedProposal) -> ApplyProposal {
        ApplyProposal {
            access: self.access.clone(),
            operation_id: "apply".into(),
            proposal_id: proposal.id.clone(),
            prepared_id: prepared.id.clone(),
            expected: self.document.head.clone(),
            result_hash: prepared.body_hash.clone(),
            local_generation: "1".into(),
        }
    }
    fn current(&self) -> DocumentRecord {
        self.project()
            .document(self.access.clone(), "chapter".into())
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.project.take());
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn body(text: &str) -> Value {
    json!({"schemaVersion":1,"body":{"type":"doc","content":[
        {"type":"paragraph","attrs":{"id":"p"},"content":[{"type":"text","text":format!("prefix {text} suffix")}]},
        {"type":"paragraph","attrs":{"id":"end"},"content":[{"type":"text","marks":[{"type":"italic"}],"text":"Protected ending."}]}
    ]}})
}
fn output() -> String {
    serde_json::to_string(&ProposalOutput {
        suggestions: ["clearer", "firmer", "quieter"]
            .map(|text| ProposalCandidate {
                title: text.into(),
                replacement_text: text.into(),
                explanation: "A test alternative.".into(),
            })
            .to_vec(),
    })
    .unwrap()
}

#[test]
fn prepare_edit_apply_one_and_reject_others_keeps_decision_and_freshness_separate() {
    let f = Fixture::new();
    let proposals = f.candidates();
    assert_eq!(proposals.len(), 3);
    assert!(
        proposals
            .iter()
            .all(|proposal| proposal.kind == ProposalKind::Passage)
    );
    assert!(proposals.iter().all(|p| p.current && p.decision.is_none()));
    assert_eq!(f.current().head, f.document.head);
    let prepare = f.prepare_request(&proposals[1], "firmer", "0", "prepare-1");
    let v1 = f.project().prepare_proposal(prepare.clone()).unwrap();
    assert!(v1.paragraphs.is_none());
    assert_eq!(f.project().prepare_proposal(prepare).unwrap().id, v1.id);
    let v2 = f
        .project()
        .prepare_proposal(f.prepare_request(&proposals[1], "author wording", "1", "prepare-2"))
        .unwrap();
    assert_eq!(v2.version, "2");
    let old = f.apply_request(&proposals[1], &v1);
    assert_eq!(
        f.project().apply_proposal(old).unwrap_err().code,
        "PreparedVersionConflict"
    );
    let request = f.apply_request(&proposals[1], &v2);
    let applied = f.project().apply_proposal(request.clone()).unwrap();
    assert!(!applied.already_applied);
    assert_eq!(applied.document.body, body("author wording"));
    assert_eq!(applied.result.head.version, "1");
    assert!(applied.result.applied.is_some());
    let again = f.project().apply_proposal(request.clone()).unwrap();
    assert!(again.already_applied);
    assert_eq!(again.result, applied.result);
    let mut different = request;
    different.operation_id = "double-click".into();
    assert_eq!(
        f.project().apply_proposal(different).unwrap_err().code,
        "SuggestionAlreadyDecided"
    );
    let after = f.list();
    assert!(after.iter().all(|p| !p.current));
    assert!(after[0].decision.is_none() && after[2].decision.is_none());
    let reject = RejectProposal {
        access: f.access.clone(),
        operation_id: "reject".into(),
        proposal_id: after[0].id.clone(),
    };
    let decision = f.project().reject_proposal(reject.clone()).unwrap();
    assert_eq!(f.project().reject_proposal(reject).unwrap().id, decision.id);
    assert_eq!(f.current().head, applied.document.head);
    let history = f
        .project()
        .history(f.access.clone(), "chapter".into())
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].body, body("author wording"));
    assert_eq!(history[1].body, f.document.body);
    create_backup(f.project(), &f.root.join("with-decisions.wnsbackup")).unwrap();
}

#[test]
fn exact_text_scope_and_cas_are_checked_before_any_prepared_version_is_written() {
    let f = Fixture::new();
    let proposals = f.candidates();
    let p = &proposals[0];
    let good = f.prepare_request(p, "clearer", "0", "prepare");
    let mut wrong = good.clone();
    wrong.body = body("secretly different");
    assert_eq!(
        f.project().prepare_proposal(wrong).unwrap_err().code,
        "ScopeViolation"
    );
    let mut neighbor = good.clone();
    neighbor.body["body"]["content"][1]["content"][0]["text"] = json!("Changed ending.");
    assert_eq!(
        f.project().prepare_proposal(neighbor).unwrap_err().code,
        "ScopeViolation"
    );
    assert!(f.list()[0].prepared.is_none());
    let prepared = f.project().prepare_proposal(good.clone()).unwrap();
    let mut changed = good;
    changed.replacement_text = "other".into();
    changed.body = body("other");
    assert_eq!(
        f.project().prepare_proposal(changed).unwrap_err().code,
        "OperationIdReusedWithDifferentPayload"
    );
    assert_eq!(
        f.project()
            .prepare_proposal(f.prepare_request(p, "new", "0", "stale-prepare"))
            .unwrap_err()
            .code,
        "PreparedVersionConflict"
    );
    assert_eq!(f.list()[0].prepared.as_ref().unwrap().id, prepared.id);
    assert_eq!(f.current().head, f.document.head);
}

#[test]
fn unseen_source_edits_and_policy_revocation_stale_apply_without_erasing_review_history() {
    for change in ["other-document", "target", "policy"] {
        let f = Fixture::new();
        let p = f.candidates().remove(0);
        let prepared = f
            .project()
            .prepare_proposal(f.prepare_request(&p, "clearer", "0", "prepare"))
            .unwrap();
        match change {
            "other-document" => {
                f.project()
                    .create_document(CreateDocument {
                        access: f.access.clone(),
                        operation_id: "new-source".into(),
                        document_id: "new-note".into(),
                        title: "A newly discovered fact".into(),
                        kind: "note".into(),
                        body: body("Private note"),
                    })
                    .unwrap();
            }
            "target" => {
                f.project()
                    .save(SaveSnapshot {
                        access: f.access.clone(),
                        operation_id: "manual".into(),
                        expected: f.document.head.clone(),
                        local_generation: "1".into(),
                        body: body("manual edit"),
                        cause: SaveCause::Typing,
                    })
                    .unwrap();
            }
            _ => {
                f.project()
                    .revoke_story_context(f.access.clone(), "0".into())
                    .unwrap();
            }
        }
        let current = f.current();
        let error = f
            .project()
            .apply_proposal(f.apply_request(&p, &prepared))
            .unwrap_err();
        assert!(
            matches!(error.code.as_str(), "SuggestionStale" | "VersionConflict"),
            "{change}: {error}"
        );
        assert!(!f.list()[0].current);
        assert!(f.list()[0].decision.is_none());
        assert_eq!(f.current().head, current.head);
        create_backup(f.project(), &f.root.join("stale-history.wnsbackup")).unwrap();
    }
}

#[test]
fn lost_ack_reconciliation_returns_latest_body_and_old_decision_without_replaying() {
    let mut f = Fixture::new();
    let p = f.candidates().remove(0);
    let prepared = f
        .project()
        .prepare_proposal(f.prepare_request(&p, "clearer", "0", "prepare"))
        .unwrap();
    let request = f.apply_request(&p, &prepared);
    let committed = f.project().apply_proposal(request.clone()).unwrap(); // Simulate lost transport ACK.
    let saved = f
        .project()
        .save(SaveSnapshot {
            access: f.access.clone(),
            operation_id: "later-manual".into(),
            expected: committed.document.head,
            local_generation: "2".into(),
            body: body("later author edit"),
            cause: SaveCause::Typing,
        })
        .unwrap();
    drop(f.project.take());
    f.project = Some(ProjectSession::open(f.root.join("project")).unwrap());
    let reconciled = f
        .project()
        .reconcile(ReconcileRequest {
            project_id: f.access.project_id.clone(),
            operation_namespace: f.access.operation_namespace.clone(),
            session: "recovered-renderer".into(),
            document_id: "chapter".into(),
            pending_operation_ids: vec![request.operation_id.clone()],
        })
        .unwrap();
    assert_eq!(reconciled.document.head, saved.head);
    assert_eq!(reconciled.document.body, body("later author edit"));
    assert_eq!(reconciled.receipts.len(), 1);
    assert_eq!(reconciled.receipts[0].operation_kind, "apply");
    assert_eq!(reconciled.receipts[0].result.head, committed.result.head);
    let mut replay = request;
    replay.access = reconciled.access.clone();
    let replay = f.project().apply_proposal(replay).unwrap();
    assert!(replay.already_applied);
    assert_eq!(replay.document.head, saved.head);
    assert_eq!(replay.result, committed.result);
    f.access = reconciled.access;
    assert_eq!(f.list()[0].decision.as_ref().unwrap().kind, "apply");
}

#[test]
fn injected_write_failures_roll_back_body_epoch_history_decision_and_receipt_together() {
    let points = [
        "BEFORE UPDATE OF working_version ON documents",
        "BEFORE UPDATE OF context_source_epoch ON project",
        "BEFORE INSERT ON revisions",
        "BEFORE UPDATE OF last_checkpoint_id ON documents",
        "BEFORE INSERT ON proposal_decisions",
        "BEFORE INSERT ON command_receipts",
    ];
    for point in points {
        let f = Fixture::new();
        let p = f.candidates().remove(0);
        let prepared = f
            .project()
            .prepare_proposal(f.prepare_request(&p, "clearer", "0", "prepare"))
            .unwrap();
        let epoch = f.project().context_epochs(f.access.clone()).unwrap().source;
        let db = Connection::open(f.root.join("project/project.sqlite3")).unwrap();
        db.execute_batch(&format!("CREATE TRIGGER injected {point} BEGIN SELECT RAISE(ABORT,'injected write failure'); END;")).unwrap();
        let request = f.apply_request(&p, &prepared);
        assert!(
            f.project().apply_proposal(request.clone()).is_err(),
            "{point}"
        );
        assert_eq!(f.current().head, f.document.head, "{point}");
        assert_eq!(
            f.project().context_epochs(f.access.clone()).unwrap().source,
            epoch,
            "{point}"
        );
        assert!(f.list()[0].decision.is_none(), "{point}");
        assert_eq!(
            f.project()
                .history(f.access.clone(), "chapter".into())
                .unwrap()
                .len(),
            1,
            "{point}"
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM command_receipts WHERE operation_kind='apply'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        db.execute_batch("DROP TRIGGER injected;").unwrap();
        assert!(!f.project().apply_proposal(request).unwrap().already_applied);
    }
}

#[test]
fn malformed_output_stop_and_duplicate_terminal_cannot_create_executable_extra_candidates() {
    let f = Fixture::new();
    let start = f.start("malformed");
    let finish = f.complete(
        &start,
        "{\"suggestions\":[{\"replacementText\":\"unsafe\"}]}",
    );
    assert!(f.list().is_empty());
    assert_eq!(
        f.project().finish_discussion(finish).unwrap().output_text,
        "{\"suggestions\":[{\"replacementText\":\"unsafe\"}]}"
    );
    let start = f.start("valid");
    let finish = f.complete(&start, &output());
    f.project().finish_discussion(finish).unwrap();
    assert_eq!(f.list().len(), 3);
    let start = f.start("stopped");
    f.project()
        .begin_discussion_run(DiscussionBegin {
            owner: start.run.owner.clone(),
        })
        .unwrap();
    f.project()
        .stop_discussion(f.access.clone(), start.run.id.clone())
        .unwrap();
    assert!(
        f.project()
            .finish_discussion(DiscussionFinish {
                owner: start.run.owner,
                expected_sequence: "1".into(),
                event_id: "late".into(),
                assistant_text: output()
            })
            .is_err()
    );
    assert_eq!(f.list().len(), 3);
    assert_eq!(f.current().head, f.document.head);
}

#[test]
fn recovery_retains_read_only_suggestions_and_never_transfers_apply_authority() {
    let f = Fixture::new();
    let p = f.candidates().remove(0);
    let prepared = f
        .project()
        .prepare_proposal(f.prepare_request(&p, "clearer", "0", "prepare"))
        .unwrap();
    let archive = f.root.join("recovery.wnsbackup");
    create_backup(f.project(), &archive).unwrap();
    let recovered_path = f.root.join("recovered");
    let recovered = recover_backup(&archive, &recovered_path, "Recovered test").unwrap();
    let access = recovered.attach("recovered".into()).unwrap();
    let copied = recovered
        .proposals(access.clone(), "chapter".into())
        .unwrap();
    assert!(copied.iter().all(|p| p.historical_copy && !p.current));
    let mut apply = f.apply_request(&p, &prepared);
    apply.access = access.clone();
    assert_eq!(
        recovered.apply_proposal(apply).unwrap_err().code,
        "ContextProjectMismatch"
    );
    let mut prepare = f.prepare_request(&p, "new", "1", "reprepare");
    prepare.access = access.clone();
    assert_eq!(
        recovered.prepare_proposal(prepare).unwrap_err().code,
        "ContextProjectMismatch"
    );
    assert_eq!(
        recovered
            .reject_proposal(RejectProposal {
                access,
                operation_id: "reject".into(),
                proposal_id: p.id
            })
            .unwrap_err()
            .code,
        "ContextProjectMismatch"
    );
    create_backup(&recovered, &f.root.join("copied-history.wnsbackup")).unwrap();
}

#[test]
fn operation_identity_cannot_cross_document_and_proposal_receipt_domains() {
    let f = Fixture::new();
    let p = f.candidates().remove(0);
    let prepared = f
        .project()
        .prepare_proposal(f.prepare_request(&p, "clearer", "0", "shared"))
        .unwrap();
    let mut apply = f.apply_request(&p, &prepared);
    apply.operation_id = "shared".into();
    let error = f.project().apply_proposal(apply).unwrap_err();
    assert_eq!(error.code, "OperationIdReusedWithDifferentPayload");
    assert_eq!(f.current().head, f.document.head);

    let f = Fixture::new();
    let p = f.candidates().remove(0);
    let prepared = f
        .project()
        .prepare_proposal(f.prepare_request(&p, "clearer", "0", "prepare"))
        .unwrap();
    let mut apply = f.apply_request(&p, &prepared);
    apply.operation_id = "shared".into();
    f.project().apply_proposal(apply).unwrap();
    let second = f
        .list()
        .into_iter()
        .find(|candidate| candidate.id != p.id)
        .unwrap();
    let error = f
        .project()
        .prepare_proposal(f.prepare_request(&second, "firmer", "0", "shared"))
        .unwrap_err();
    assert_eq!(error.code, "OperationIdReusedWithDifferentPayload");
}

#[test]
fn tampered_proposal_receipt_hash_rejects_backup_validation() {
    let f = Fixture::new();
    let p = f.candidates().remove(0);
    f.project()
        .prepare_proposal(f.prepare_request(&p, "clearer", "0", "prepare"))
        .unwrap();
    let db = Connection::open(f.root.join("project/project.sqlite3")).unwrap();
    db.execute_batch("DROP TRIGGER proposal_receipts_immutable_update;")
        .unwrap();
    db.execute(
        "UPDATE proposal_receipts SET payload_hash='tampered' WHERE operation_id='prepare'",
        [],
    )
    .unwrap();
    drop(db);
    let error = create_backup(f.project(), &f.root.join("tampered.wnsbackup")).unwrap_err();
    assert_eq!(error.code, "InvalidBackup");
}

#[test]
fn tampered_apply_decision_link_rejects_backup_validation() {
    let f = Fixture::new();
    let p = f.candidates().remove(0);
    let prepared = f
        .project()
        .prepare_proposal(f.prepare_request(&p, "clearer", "0", "prepare"))
        .unwrap();
    f.project()
        .apply_proposal(f.apply_request(&p, &prepared))
        .unwrap();
    let db = Connection::open(f.root.join("project/project.sqlite3")).unwrap();
    let json: String = db
        .query_row(
            "SELECT result_json FROM command_receipts WHERE operation_kind='apply'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut result: Value = serde_json::from_str(&json).unwrap();
    result["applied"]["decisionId"] = json!("tampered-decision");
    db.execute_batch("DROP TRIGGER receipts_no_update;")
        .unwrap();
    db.execute(
        "UPDATE command_receipts SET result_json=? WHERE operation_kind='apply'",
        [&serde_json::to_string(&result).unwrap()],
    )
    .unwrap();
    drop(db);
    let error = create_backup(f.project(), &f.root.join("tampered-apply.wnsbackup")).unwrap_err();
    assert_eq!(error.code, "InvalidBackup");
}
