    use super::*;
    use serde_json::json;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    // Compiled only into the Rust unit-test binary, never the desktop/core library.
    pub(super) fn hold_after_commit_before_ack(operation_id: &str) {
        if matches!(
            operation_id,
            "crash-save" | "crash-apply" | "crash-restore" | "crash-context"
        ) && let Some(root) = std::env::var_os("WNS_UNIT_CRASH_ROOT")
        {
            let mut marker = File::create_new(PathBuf::from(root).join("committed")).unwrap();
            marker
                .write_all(
                    b"COMMIT returned successfully; no document acknowledgment has been sent",
                )
                .unwrap();
            marker.sync_all().unwrap();
            loop {
                std::thread::park();
            }
        }
    }

    #[test]
    #[ignore = "Subprocess target for kill_after_commit_before_ack_recovers_once"]
    fn crash_child() {
        let root = PathBuf::from(std::env::var_os("WNS_UNIT_CRASH_ROOT").unwrap());
        let project = ProjectSession::open(root.join("project")).unwrap();
        let access = project.documents().attach("crash-child".into()).unwrap();
        let json = std::fs::read(root.join("request.json")).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        if value.get("preparedId").is_some() {
            let mut request: proposals::ApplyProposal = serde_json::from_slice(&json).unwrap();
            request.access = access;
            project.apply_proposal(request).unwrap();
        } else if value.get("revisionId").is_some() {
            let mut request: history::RestoreRevision = serde_json::from_slice(&json).unwrap();
            request.access = access;
            project.restore_revision(request).unwrap();
        } else if value.get("snapshotId").is_some() {
            let mut request: context_packets::PrepareContext =
                serde_json::from_slice(&json).unwrap();
            request.access = access;
            project.prepare_context(request).unwrap();
        } else {
            let mut request: SaveSnapshot = serde_json::from_slice(&json).unwrap();
            request.access = access;
            project.documents().save(request).unwrap();
        }
        panic!("The deterministic after-commit barrier did not hold");
    }

    #[test]
    fn kill_after_commit_before_ack_recovers_once() {
        let root = std::env::temp_dir().join(format!("wns-crash-{}", new_id()));
        std::fs::create_dir(&root).unwrap();
        let project =
            ProjectSession::create(root.join("project"), "Crash recovery fixture").unwrap();
        let access = project.documents().attach("parent".into()).unwrap();
        let document = project
            .documents()
            .create(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "A chapter".into(),
                kind: "chapter".into(),
                body: blank_document(),
            })
            .unwrap();
        let mut changed = document.body.clone();
        changed["body"]["content"][0]["content"] =
            json!([{"type":"text","text":"This prose survived the lost acknowledgment. 👩‍🚀"}]);
        let mut request = SaveSnapshot {
            access,
            operation_id: "crash-save".into(),
            expected: document.head,
            local_generation: "9".into(),
            body: changed,
            cause: SaveCause::Typing,
        };
        std::fs::write(
            root.join("request.json"),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        drop(project);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "projects::tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("WNS_UNIT_CRASH_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !root.join("committed").exists() && started.elapsed() < Duration::from_secs(15) {
            assert!(
                child.try_wait().unwrap().is_none(),
                "Child stopped before COMMIT"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        let committed = root.join("committed").exists();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(committed, "Child did not reach the commit barrier");
        let recovered = ProjectSession::open(root.join("project")).unwrap();
        let snapshot = recovered
            .documents()
            .reconcile(ReconcileRequest {
                project_id: recovered.info.project_id.clone(),
                operation_namespace: recovered.info.operation_namespace.clone(),
                session: "new-renderer".into(),
                document_id: "chapter".into(),
                pending_operation_ids: vec!["crash-save".into()],
            })
            .unwrap();
        assert_eq!(snapshot.document.head.version, "1");
        assert_eq!(snapshot.document.body, request.body);
        assert_eq!(snapshot.receipts.len(), 1);
        request.access = snapshot.access.clone();
        let replay = recovered.documents().save(request).unwrap();
        assert_eq!(replay.head, snapshot.document.head);
        assert_eq!(replay.saved_generation, "9");
        assert_eq!(replay.session, "new-renderer");
        drop(recovered);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn killed_context_prepare_after_commit_retries_the_same_packet_once() {
        use crate::context::packet::MockContextBudget;
        use crate::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
        use crate::projects::context_packets::{PreparationResult, PrepareContext};
        use crate::projects::story_context::FreezeStory;

        let root = std::env::temp_dir().join(format!("wns-context-crash-{}", new_id()));
        std::fs::create_dir(&root).unwrap();
        let project =
            ProjectSession::create(root.join("project"), "Context crash fixture").unwrap();
        let access = project.documents().attach("parent".into()).unwrap();
        let document = project
            .documents()
            .create(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "A chapter".into(),
                kind: "chapter".into(),
                body: blank_document(),
            })
            .unwrap();
        let policy = InformationPolicy {
            version: project.context_epochs(access.clone()).unwrap().policy,
            audience: Audience::AuthorRoom,
            reader_frontier: None,
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        };
        let frozen = project
            .freeze_story(FreezeStory {
                access: access.clone(),
                operation_id: "freeze".into(),
                expected: document.head.clone(),
                basis: BasisKind::Working,
                purpose: ContextPurpose::StoryQuestion,
                policy,
            })
            .unwrap();
        let request = PrepareContext {
            access,
            operation_id: "crash-context".into(),
            snapshot_id: frozen.snapshot.snapshot_id,
            instruction: "What detail survives the lost packet acknowledgment?".into(),
            mandatory_handles: Vec::new(),
            transient_mandatory_handles: None,
            safe_brief: None,
            scope: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            response_contract: None,
            lookup: None,
        };
        std::fs::write(
            root.join("request.json"),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        drop(project);

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "projects::tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("WNS_UNIT_CRASH_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !root.join("committed").exists() && started.elapsed() < Duration::from_secs(15) {
            assert!(
                child.try_wait().unwrap().is_none(),
                "Child stopped before context COMMIT"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        let committed = root.join("committed").exists();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(committed, "Child did not reach the context commit barrier");

        let recovered = ProjectSession::open(root.join("project")).unwrap();
        let reconciled = recovered
            .documents()
            .reconcile(ReconcileRequest {
                project_id: recovered.info.project_id.clone(),
                operation_namespace: recovered.info.operation_namespace.clone(),
                session: "new-renderer".into(),
                document_id: "chapter".into(),
                pending_operation_ids: Vec::new(),
            })
            .unwrap();
        let before: (String, String, String, String, String, String, String, String) =
            Connection::open(recovered.path.join("project.sqlite3"))
                .unwrap()
                .query_row(
                    "SELECT id,payload_hash,request_json,snapshot_id,session_id,packet_json,packet_hash,input_hash FROM context_packets",
                    [],
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
        let before_packet = recovered
            .prepared_context(reconciled.access.clone(), before.0.clone())
            .unwrap();
        assert_eq!(serde_json::to_string(&before_packet).unwrap(), before.5);
        let mut retry = request;
        retry.access = reconciled.access.clone();
        let packet = match recovered.prepare_context(retry).unwrap() {
            PreparationResult::Prepared { packet, current } => {
                assert!(current);
                *packet
            }
            PreparationResult::BudgetRejected { .. } => {
                panic!("the committed packet must be returned on an exact retry")
            }
        };
        assert_eq!(
            Connection::open(recovered.path.join("project.sqlite3"))
                .unwrap()
                .query_row("SELECT COUNT(*) FROM context_packets", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        let after: (String, String, String, String, String, String, String, String) =
            Connection::open(recovered.path.join("project.sqlite3"))
                .unwrap()
                .query_row(
                    "SELECT id,payload_hash,request_json,snapshot_id,session_id,packet_json,packet_hash,input_hash FROM context_packets",
                    [],
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
        assert_eq!(after, before);
        assert_eq!(serde_json::to_string(&packet).unwrap(), before.5);
        drop(recovered);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn killed_apply_after_commit_recovers_decision_and_checkpoint_once() {
        use crate::context::packet::MockContextBudget;
        use crate::documents::{Endpoint, ScopeKind};
        use discussions::*;
        use proposals::*;
        let root = std::env::temp_dir().join(format!("wns-apply-crash-{}", new_id()));
        std::fs::create_dir(&root).unwrap();
        let project = ProjectSession::create(root.join("project"), "Apply crash fixture").unwrap();
        let access = project.documents().attach("parent".into()).unwrap();
        let body = |text: &str| json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p"},"content":[{"type":"text","text":text}]}]}});
        let document = project
            .documents()
            .create(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: body("original"),
            })
            .unwrap();
        let started = project
            .start_discussion(StartDiscussion {
                access: access.clone(),
                operation_id: "feedback".into(),
                expected: document.head.clone(),
                instruction: "Revise this passage.".into(),
                intent: FeedbackIntent::ProposeEdits,
                basis: None,
                scope: Some(DiscussionScopeInput {
                    kind: ScopeKind::Passage,
                    start: Some(Endpoint {
                        block_id: "p".into(),
                        utf16_offset: 0,
                    }),
                    end: Some(Endpoint {
                        block_id: "p".into(),
                        utf16_offset: 8,
                    }),
                    quote: "original".into(),
                    source_body_hash: document.head.body_hash.clone(),
                }),
                pinned_document_ids: vec![],
                safe_brief: None,
                budget: MockContextBudget::new("100000", "1000", "100"),
                provider_binding: None,
                previous_run_id: None,
                lookup: None,
            })
            .unwrap();
        project
            .begin_discussion_run(DiscussionBegin {
                owner: started.run.owner.clone(),
            })
            .unwrap();
        project
            .mark_discussion_delivered(started.run.owner.clone())
            .unwrap();
        project
            .finish_discussion(DiscussionFinish {
                owner: started.run.owner,
                expected_sequence: "0".into(),
                event_id: "terminal".into(),
                assistant_text: serde_json::to_string(&ProposalOutput {
                    suggestions: vec![ProposalCandidate {
                        title: "Alternative".into(),
                        replacement_text: "survived".into(),
                        explanation: "Crash test.".into(),
                    }],
                })
                .unwrap(),
            })
            .unwrap();
        let proposal = project
            .proposals(access.clone(), "chapter".into())
            .unwrap()
            .remove(0);
        let prepared = project
            .prepare_proposal(PrepareProposal {
                access: access.clone(),
                operation_id: "prepare".into(),
                proposal_id: proposal.id.clone(),
                expected_prepared_version: "0".into(),
                replacement_text: "survived".into(),
                body: body("survived"),
            })
            .unwrap();
        let mut request = ApplyProposal {
            access,
            operation_id: "crash-apply".into(),
            proposal_id: proposal.id,
            prepared_id: prepared.id,
            expected: document.head,
            result_hash: prepared.body_hash,
            local_generation: "1".into(),
        };
        std::fs::write(
            root.join("request.json"),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        drop(project);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "projects::tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("WNS_UNIT_CRASH_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !root.join("committed").exists() && started.elapsed() < Duration::from_secs(15) {
            assert!(
                child.try_wait().unwrap().is_none(),
                "Child stopped before Apply COMMIT"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        let committed = root.join("committed").exists();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(committed, "Child did not reach the Apply commit barrier");
        let recovered = ProjectSession::open(root.join("project")).unwrap();
        let snapshot = recovered
            .documents()
            .reconcile(ReconcileRequest {
                project_id: recovered.info.project_id.clone(),
                operation_namespace: recovered.info.operation_namespace.clone(),
                session: "new-renderer".into(),
                document_id: "chapter".into(),
                pending_operation_ids: vec!["crash-apply".into()],
            })
            .unwrap();
        assert_eq!(snapshot.document.body, body("survived"));
        assert_eq!(snapshot.document.head.version, "1");
        assert_eq!(snapshot.receipts.len(), 1);
        assert!(snapshot.receipts[0].result.applied.is_some());
        request.access = snapshot.access.clone();
        let repeated = recovered.apply_proposal(request).unwrap();
        assert!(repeated.already_applied);
        assert_eq!(repeated.document.head, snapshot.document.head);
        assert_eq!(
            recovered
                .documents()
                .history(snapshot.access.clone(), "chapter".into())
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            recovered
                .proposals(snapshot.access, "chapter".into())
                .unwrap()[0]
                .decision
                .as_ref()
                .unwrap()
                .kind,
            "apply"
        );
        drop(recovered);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn killed_restore_after_commit_recovers_historical_result_once() {
        let root = std::env::temp_dir().join(format!("wns-restore-crash-{}", new_id()));
        std::fs::create_dir(&root).unwrap();
        let project =
            ProjectSession::create(root.join("project"), "Restore crash fixture").unwrap();
        let access = project.documents().attach("parent".into()).unwrap();
        let initial_body = blank_document();
        let document = project
            .documents()
            .create(CreateDocument {
                access: access.clone(),
                operation_id: "create".into(),
                document_id: "chapter".into(),
                title: "Chapter".into(),
                kind: "chapter".into(),
                body: initial_body.clone(),
            })
            .unwrap();
        let source = project
            .documents()
            .checkpoint(CheckpointRequest {
                access: access.clone(),
                expected: document.head.clone(),
                reason: CheckpointReason::Manual,
            })
            .unwrap();
        let current = project
            .documents()
            .save(SaveSnapshot {
                access: access.clone(),
                operation_id: "restore-current".into(),
                expected: document.head,
                local_generation: "11".into(),
                body: json!({
                    "schemaVersion": 1,
                    "body": {"type": "doc", "content": [{
                        "type": "paragraph",
                        "attrs": {"id": "p"},
                        "content": [{"type": "text", "text": "later edit"}]
                    }]}
                }),
                cause: SaveCause::Typing,
            })
            .unwrap();
        let mut request = history::RestoreRevision {
            access,
            operation_id: "crash-restore".into(),
            expected: current.head,
            revision_id: source.id,
            revision_hash: source.head.body_hash,
            local_generation: "12".into(),
        };
        std::fs::write(
            root.join("request.json"),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        drop(project);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "projects::tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("WNS_UNIT_CRASH_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !root.join("committed").exists() && started.elapsed() < Duration::from_secs(15) {
            assert!(
                child.try_wait().unwrap().is_none(),
                "Child stopped before Restore COMMIT"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        let committed = root.join("committed").exists();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(committed, "Child did not reach the Restore commit barrier");
        let recovered = ProjectSession::open(root.join("project")).unwrap();
        let snapshot = recovered
            .documents()
            .reconcile(ReconcileRequest {
                project_id: recovered.info.project_id.clone(),
                operation_namespace: recovered.info.operation_namespace.clone(),
                session: "new-renderer".into(),
                document_id: "chapter".into(),
                pending_operation_ids: vec!["crash-restore".into()],
            })
            .unwrap();
        assert_eq!(snapshot.document.body, initial_body);
        assert_eq!(snapshot.document.head.version, "2");
        assert_eq!(snapshot.receipts.len(), 1);
        assert_eq!(snapshot.receipts[0].operation_kind, "restore");
        assert!(snapshot.receipts[0].result.restored.is_some());
        request.access = snapshot.access.clone();
        let replay = recovered.restore_revision(request).unwrap();
        assert!(replay.already_applied);
        assert_eq!(replay.result, snapshot.receipts[0].result);
        assert_eq!(replay.document.head, snapshot.document.head);
        drop(recovered);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_marker_write_leaves_only_unregistered_staging() {
        let root = std::env::temp_dir().join(format!("wns-staging-{}", new_id()));
        std::fs::create_dir(&root).unwrap();
        let destination = root.join("project");
        assert!(ProjectSession::create(&destination, "unit-fail-project-marker").is_err());
        assert!(!destination.exists());
        let entries = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].starts_with(".wns-create-"));
        let project = ProjectSession::create(&destination, "Successful retry").unwrap();
        assert_eq!(project.info.title, "Successful retry");
        drop(project);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }
