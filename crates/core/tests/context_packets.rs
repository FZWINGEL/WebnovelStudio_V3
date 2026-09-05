use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::packet::{MockContextBudget, PROPOSAL_RESPONSE_CONTRACT};
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::documents::{Endpoint, ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::context_packets::{PreparationResult, PrepareContext};
use webnovel_core::projects::discussions::{
    DiscussionScopeInput, FeedbackIntent, SafeBriefInput, StartDiscussion,
};
use webnovel_core::projects::story_context::FreezeStory;
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("wns-context-packets-{label}-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create temporary test directory");
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

fn setup_project(
    root: &Path,
) -> (
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let project = ProjectSession::create(root, "Packet test").expect("create project");
    let access = project
        .attach("packet-session".into())
        .expect("attach project");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-document".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("The packet target is stable."),
        })
        .expect("create document");
    (project, access, document)
}

fn passage_scope(document: &webnovel_core::projects::DocumentRecord) -> DiscussionScopeInput {
    let grant = capture_scope(
        &document.body,
        ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "p1".into(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: "p1".into(),
                utf16_offset: 8,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .expect("capture discussion passage");
    DiscussionScopeInput {
        kind: grant.kind,
        start: grant.start,
        end: grant.end,
        quote: grant.quote,
        source_body_hash: grant.source_hash,
    }
}

fn policy(project: &ProjectSession, access: &ProjectAccess) -> InformationPolicy {
    InformationPolicy {
        version: project
            .context_epochs(access.clone())
            .expect("read context policy")
            .policy,
        audience: Audience::AuthorRoom,
        reader_frontier: None,
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    }
}

fn freeze(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
) -> webnovel_core::projects::story_context::FrozenContext {
    project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: format!("freeze-{}", Uuid::new_v4()),
            expected: document.head.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: policy(project, access),
        })
        .expect("freeze story snapshot")
}

fn prepare_request(
    access: &ProjectAccess,
    snapshot_id: &str,
    operation_id: &str,
    budget: MockContextBudget,
) -> PrepareContext {
    PrepareContext {
        access: access.clone(),
        operation_id: operation_id.into(),
        snapshot_id: snapshot_id.into(),
        instruction: "Keep the selected target grounded in the supplied evidence.".into(),
        mandatory_handles: Vec::new(),
        transient_mandatory_handles: None,
        safe_brief: None,
        scope: None,
        budget,
        provider_binding: None,
        response_contract: None,
    }
}

fn budget() -> MockContextBudget {
    MockContextBudget::new("100000", "100", "100")
}

fn prepared(result: PreparationResult) -> webnovel_core::context::packet::CompiledPacket {
    prepared_with_current(result).0
}

fn prepared_with_current(
    result: PreparationResult,
) -> (webnovel_core::context::packet::CompiledPacket, bool) {
    match result {
        PreparationResult::Prepared { packet, current } => (*packet, current),
        PreparationResult::BudgetRejected { error } => {
            panic!("packet unexpectedly rejected by budget: {error:?}")
        }
    }
}

fn packet_count(project: &ProjectSession) -> i64 {
    Connection::open(project.path.join("project.sqlite3"))
        .expect("open packet database")
        .query_row("SELECT COUNT(*) FROM context_packets", [], |row| row.get(0))
        .expect("count packet rows")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn generic_prepare_rejects_safe_brief_before_snapshot_lookup() {
    let temp = TempDir::new("safe-brief-generic");
    let (project, access, _document) = setup_project(&temp.child("project"));
    let mut request = prepare_request(&access, "missing-snapshot", "generic-safe-brief", budget());
    request.safe_brief = Some(SafeBriefInput {
        text: "A caller supplied direction".into(),
        origin_message_id: None,
        confirmed: true,
    });
    let error = project
        .prepare_context(request)
        .expect_err("generic packet preparation must not authorize a safe brief");
    assert_eq!(error.code, "SafeBriefRequiresDiscussion");
}

#[test]
fn generic_prepare_rejects_renderer_supplied_response_contract() {
    let temp = TempDir::new("response-contract-generic");
    let (project, access, _document) = setup_project(&temp.child("project"));
    let mut request = prepare_request(
        &access,
        "missing-snapshot",
        "generic-response-contract",
        budget(),
    );
    request.response_contract = Some(PROPOSAL_RESPONSE_CONTRACT.into());
    let error = project
        .prepare_context(request)
        .expect_err("generic preparation must not choose a live response contract");
    assert_eq!(error.code, "ResponseContractRequiresDiscussion");
}

fn rewrite_packet(project: &ProjectSession, packet_id: &str, mutate: impl FnOnce(&mut Value)) {
    let connection = Connection::open(project.path.join("project.sqlite3"))
        .expect("open packet database for packet tampering");
    let packet_json: String = connection
        .query_row(
            "SELECT packet_json FROM context_packets WHERE id=?",
            [packet_id],
            |row| row.get(0),
        )
        .expect("read packet JSON for packet tampering");
    let mut packet: Value = serde_json::from_str(&packet_json).expect("decode packet JSON");
    mutate(&mut packet);
    let compiled: webnovel_core::context::packet::CompiledPacket =
        serde_json::from_value(packet.clone()).expect("decode tampered packet");
    let input =
        webnovel_core::context::packet::serialized_input(&compiled.messages, &compiled.options)
            .expect("serialize tampered packet input");
    packet["receipt"]["inputHash"] = json!(sha256_hex(input.as_bytes()));
    packet["receipt"]["inputTokens"] = json!(input.len().to_string());
    let packet_json = serde_json::to_string(&packet).expect("serialize tampered packet");
    let packet_hash = sha256_hex(packet_json.as_bytes());
    connection
        .execute_batch("DROP TRIGGER immutable_context_packet_update;")
        .expect("disable packet immutability for tampering fixture");
    connection
        .execute(
            "UPDATE context_packets SET packet_json=?,packet_hash=?,input_hash=? WHERE id=?",
            params![
                packet_json,
                packet_hash,
                sha256_hex(input.as_bytes()),
                packet_id
            ],
        )
        .expect("rewrite packet receipt coherently");
}

#[test]
fn legacy_packet_without_mandatory_annotation_retains_exact_input_and_replays() {
    let temp = TempDir::new("legacy-mandatory-annotation");
    let (project, access, document) = setup_project(&temp.child("project"));
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "legacy-source-create".into(),
            document_id: "legacy-source".into(),
            title: "Original evidence".into(),
            kind: "note".into(),
            body: body("The pendant belonged to her mother."),
        })
        .unwrap();
    let frozen = freeze(&project, &access, &document);
    let mandatory = frozen
        .snapshot
        .sources
        .iter()
        .find(|source| source.source.document_id == "legacy-source")
        .unwrap()
        .handle
        .clone();
    let mut request = prepare_request(
        &access,
        &frozen.snapshot.snapshot_id,
        "legacy-packet",
        budget(),
    );
    request.mandatory_handles = vec![mandatory.clone()];
    let original = prepared(project.prepare_context(request.clone()).unwrap());
    assert_eq!(original.receipt.mandatory_source_handles, vec![mandatory]);
    let stored = Connection::open(project.path.join("project.sqlite3")).unwrap();
    let request_json: String = stored
        .query_row(
            "SELECT request_json FROM context_packets WHERE id=?",
            [&original.receipt.packet_id],
            |row| row.get(0),
        )
        .unwrap();
    let packet_json: String = stored
        .query_row(
            "SELECT packet_json FROM context_packets WHERE id=?",
            [&original.receipt.packet_id],
            |row| row.get(0),
        )
        .unwrap();
    let stored_request: Value = serde_json::from_str(&request_json).unwrap();
    let stored_packet: Value = serde_json::from_str(&packet_json).unwrap();
    assert!(stored_request.get("safeBrief").is_none());
    assert!(stored_packet["receipt"].get("safeBrief").is_none());
    drop(stored);
    rewrite_packet(&project, &original.receipt.packet_id, |packet| {
        packet["receipt"]
            .as_object_mut()
            .unwrap()
            .remove("mandatorySourceHandles");
    });
    let historical = project
        .prepared_context(access, original.receipt.packet_id.clone())
        .unwrap();
    assert_eq!(historical.messages, original.messages);
    assert_eq!(historical.receipt.input_hash, original.receipt.input_hash);
    assert!(historical.receipt.mandatory_source_handles.is_empty());
    assert_eq!(
        prepared(project.prepare_context(request).unwrap()),
        historical
    );
    create_backup(&project, &temp.child("legacy.wnsbackup")).unwrap();
}

#[test]
fn safe_brief_receipt_tampering_is_rejected_by_read_and_backup_validation() {
    let temp = TempDir::new("safe-brief-tamper");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "safe-brief-packet".into(),
            expected: document.head.clone(),
            instruction: "Revise the selected passage.".into(),
            intent: FeedbackIntent::ProposeEdits,
            scope: Some(passage_scope(&document)),
            pinned_document_ids: Vec::new(),
            safe_brief: Some(SafeBriefInput {
                text: "Keep the selected exchange restrained.".into(),
                origin_message_id: None,
                confirmed: true,
            }),
            budget: budget(),
            provider_binding: None,
            previous_run_id: None,
        })
        .expect("prepare safe brief packet");
    rewrite_packet(&project, &started.packet.receipt.packet_id, |packet| {
        packet["receipt"]["safeBrief"]["text"] = json!("Tampered direction.");
    });

    let read_error = project
        .prepared_context(access, started.packet.receipt.packet_id.clone())
        .expect_err("tampered safe brief receipt must fail packet read");
    assert_eq!(read_error.code, "InvalidContextPacket");
    let backup_error = create_backup(&project, &temp.child("tampered.wnsbackup"))
        .expect_err("tampered safe brief receipt must fail backup validation");
    assert_eq!(backup_error.code, "InvalidBackup");
}

#[test]
fn preparation_is_idempotent_and_operation_payload_is_bound() {
    let temp = TempDir::new("idempotent");
    let (project, access, document) = setup_project(&temp.child("project"));
    let snapshot = freeze(&project, &access, &document);
    let request = prepare_request(
        &access,
        &snapshot.snapshot.snapshot_id,
        "prepare-once",
        budget(),
    );
    let (first, first_current) = prepared_with_current(
        project
            .prepare_context(request.clone())
            .expect("prepare context"),
    );
    assert!(first_current);
    let retry = prepared(
        project
            .prepare_context(request.clone())
            .expect("retry same preparation"),
    );
    assert_eq!(retry, first);
    assert_eq!(packet_count(&project), 1);

    let mut changed = request;
    changed.instruction = "A different instruction must not reuse the operation.".into();
    let error = project
        .prepare_context(changed)
        .expect_err("changed payload must be rejected");
    assert_eq!(error.code, "OperationIdReusedWithDifferentPayload");
    assert_eq!(packet_count(&project), 1);
}

#[test]
fn prepared_packet_round_trips_exactly_after_project_restart() {
    let temp = TempDir::new("restart");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let snapshot = freeze(&project, &access, &document);
    let expected = prepared(
        project
            .prepare_context(prepare_request(
                &access,
                &snapshot.snapshot.snapshot_id,
                "restart-preparation",
                budget(),
            ))
            .expect("prepare packet"),
    );
    drop(project);

    let reopened = ProjectSession::open(&path).expect("reopen project");
    let reopened_access = reopened
        .attach("packet-session-after-restart".into())
        .expect("attach after restart");
    let restored = reopened
        .prepared_context(reopened_access, expected.receipt.packet_id.clone())
        .expect("read exact prepared packet");
    assert_eq!(restored.messages, expected.messages);
    assert_eq!(restored.options, expected.options);
    assert_eq!(restored.receipt, expected.receipt);
}

#[test]
fn stale_story_blocks_new_preparation_but_old_packet_remains_inspectable() {
    let temp = TempDir::new("stale");
    let (project, access, document) = setup_project(&temp.child("project"));
    let snapshot = freeze(&project, &access, &document);
    let request = prepare_request(
        &access,
        &snapshot.snapshot.snapshot_id,
        "old-preparation",
        budget(),
    );
    let old = prepared(
        project
            .prepare_context(request.clone())
            .expect("prepare old packet"),
    );
    let current = project
        .document(access.clone(), document.head.document_id.clone())
        .expect("read current document");
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "change-after-packet".into(),
            expected: current.head,
            local_generation: "1".into(),
            body: body("The story changed after preparation."),
            cause: SaveCause::Typing,
        })
        .expect("save changed story");

    assert!(
        !project
            .prepared_context_is_current(access.clone(), old.receipt.packet_id.clone())
            .expect("check packet freshness")
    );
    let old_read = project
        .prepared_context(access.clone(), old.receipt.packet_id.clone())
        .expect("old packet remains inspectable");
    assert_eq!(old_read.receipt.input_hash, old.receipt.input_hash);
    let retry = project
        .prepare_context(request)
        .expect("same operation remains an inspectable stale receipt");
    match retry {
        PreparationResult::Prepared { packet, current } => {
            assert!(!current);
            assert_eq!(*packet, old);
        }
        PreparationResult::BudgetRejected { .. } => {
            panic!("an idempotent stale retry must return its prepared packet")
        }
    }
    let error = project
        .prepare_context(prepare_request(
            &access,
            &snapshot.snapshot.snapshot_id,
            "new-after-story-change",
            budget(),
        ))
        .expect_err("new packet from stale snapshot must fail");
    assert_eq!(error.code, "ContextChanged");
    assert_eq!(packet_count(&project), 1);
}

#[test]
fn revoked_policy_blocks_reading_an_old_prepared_packet() {
    let temp = TempDir::new("revoked");
    let (project, access, document) = setup_project(&temp.child("project"));
    let snapshot = freeze(&project, &access, &document);
    let packet = prepared(
        project
            .prepare_context(prepare_request(
                &access,
                &snapshot.snapshot.snapshot_id,
                "revoked-preparation",
                budget(),
            ))
            .expect("prepare packet"),
    );
    project
        .revoke_story_context(access.clone(), "0".into())
        .expect("revoke old policy");
    let error = project
        .prepared_context(access, packet.receipt.packet_id.clone())
        .expect_err("revoked policy must block packet read");
    assert_eq!(error.code, "ContextPolicyChanged");
    let archive = temp.child("revoked-history.wnsbackup");
    create_backup(&project, &archive)
        .expect("revocation must not prevent backing up retained history");
    let recovered = recover_backup(&archive, &temp.child("recovered"), "Recovered history")
        .expect("revoked historical packets remain valid stored evidence");
    let recovered_access = recovered
        .attach("recovered-history-session".into())
        .unwrap();
    assert_eq!(
        recovered
            .prepared_context(recovered_access.clone(), packet.receipt.packet_id)
            .expect_err("backup retention must not authorize copied packets")
            .code,
        "ContextProjectMismatch"
    );
    let current = recovered
        .document(recovered_access.clone(), "chapter-one".into())
        .unwrap();
    let frozen = freeze(&recovered, &recovered_access, &current);
    let fresh = prepared(
        recovered
            .prepare_context(prepare_request(
                &recovered_access,
                &frozen.snapshot.snapshot_id,
                "current-copy-packet",
                budget(),
            ))
            .expect("the recovered copy can prepare under its current policy"),
    );
    assert_eq!(fresh.receipt.snapshot_id, frozen.snapshot.snapshot_id);
}

#[test]
fn recovered_copy_rejects_original_packet_identity_and_accepts_new_snapshot_packet() {
    let temp = TempDir::new("recovery");
    let source_path = temp.child("source");
    let (source, source_access, source_document) = setup_project(&source_path);
    let source_snapshot = freeze(&source, &source_access, &source_document);
    let original = prepared(
        source
            .prepare_context(prepare_request(
                &source_access,
                &source_snapshot.snapshot.snapshot_id,
                "original-preparation",
                budget(),
            ))
            .expect("prepare source packet"),
    );
    let archive = temp.child("source.wnsbackup");
    create_backup(&source, &archive).expect("backup source packet");
    drop(source);

    let target = temp.child("recovered");
    let recovered = recover_backup(&archive, &target, "Recovered packet project")
        .expect("recover copied project");
    let recovered_access = recovered
        .attach("recovered-packet-session".into())
        .expect("attach recovered project");
    let error = recovered
        .prepared_context(recovered_access.clone(), original.receipt.packet_id.clone())
        .expect_err("original packet identity must not cross project recovery");
    assert_eq!(error.code, "ContextProjectMismatch");

    let recovered_document = recovered
        .document(recovered_access.clone(), "chapter-one".into())
        .expect("read recovered target");
    let recovered_snapshot = freeze(&recovered, &recovered_access, &recovered_document);
    let new_packet = prepared(
        recovered
            .prepare_context(prepare_request(
                &recovered_access,
                &recovered_snapshot.snapshot.snapshot_id,
                "recovered-preparation",
                budget(),
            ))
            .expect("prepare new packet in recovered project"),
    );
    assert_ne!(new_packet.receipt.packet_id, original.receipt.packet_id);
    assert_eq!(packet_count(&recovered), 2);
}

#[test]
fn budget_rejection_creates_no_packet_and_does_not_mutate_body() {
    let temp = TempDir::new("budget");
    let (project, access, document) = setup_project(&temp.child("project"));
    let snapshot = freeze(&project, &access, &document);
    let before = project
        .document(access.clone(), "chapter-one".into())
        .expect("read body before budget rejection");
    let result = project
        .prepare_context(prepare_request(
            &access,
            &snapshot.snapshot.snapshot_id,
            "budget-rejection",
            MockContextBudget::new("1", "0", "0"),
        ))
        .expect("budget rejection is a structured result");
    match result {
        PreparationResult::BudgetRejected { error } => {
            assert_eq!(
                error.code,
                webnovel_core::context::BudgetErrorCode::MandatoryContextTooLarge
            );
            assert!(!error.mandatory_handles.is_empty());
        }
        PreparationResult::Prepared { .. } => panic!("tiny budget must not prepare a packet"),
    }
    assert_eq!(packet_count(&project), 0);
    let after = project
        .document(access, "chapter-one".into())
        .expect("read body after budget rejection");
    assert_eq!(after.body, before.body);
    assert_eq!(after.head, before.head);
}

#[test]
fn packet_insert_failure_rolls_back_without_a_durable_packet_row() {
    let temp = TempDir::new("transaction-failure");
    let (project, access, document) = setup_project(&temp.child("project"));
    let snapshot = freeze(&project, &access, &document);
    let connection = Connection::open(project.path.join("project.sqlite3"))
        .expect("open packet database for fault injection");
    connection
        .execute_batch(
            "CREATE TRIGGER fail_context_packet BEFORE INSERT ON context_packets
             WHEN NEW.operation_id='packet-trigger-failure'
             BEGIN SELECT RAISE(ABORT,'injected packet failure'); END;",
        )
        .expect("install packet insertion fault");
    drop(connection);

    let error = project
        .prepare_context(prepare_request(
            &access,
            &snapshot.snapshot.snapshot_id,
            "packet-trigger-failure",
            budget(),
        ))
        .expect_err("packet insertion fault must fail");
    assert_eq!(error.code, "PersistenceUnavailable");
    assert_eq!(packet_count(&project), 0);
    let connection =
        Connection::open(project.path.join("project.sqlite3")).expect("reopen packet database");
    connection
        .execute_batch("DROP TRIGGER fail_context_packet;")
        .expect("remove packet insertion fault");
}

#[test]
fn coherent_packet_tampering_is_rejected_by_reads_and_transfer() {
    for (label, mutate) in [
        (
            "mandatory-annotation-change",
            Box::new(|packet: &mut Value| {
                packet["receipt"]["mandatorySourceHandles"] = json!([]);
            }) as Box<dyn FnOnce(&mut Value)>,
        ),
        (
            "mandatory-source-omission",
            Box::new(|packet: &mut Value| {
                let envelope: Value = serde_json::from_str(
                    packet["messages"][1]["content"]
                        .as_str()
                        .expect("packet evidence envelope"),
                )
                .expect("decode packet evidence envelope");
                let mandatory = packet["receipt"]["sourceHandles"]
                    .as_array()
                    .expect("packet source handles")
                    .last()
                    .and_then(Value::as_str)
                    .expect("mandatory source handle")
                    .to_owned();
                let mut envelope = envelope;
                envelope["sources"] = envelope["sources"]
                    .as_array()
                    .expect("packet source envelope")
                    .iter()
                    .filter(|source| source["handle"].as_str() != Some(mandatory.as_str()))
                    .cloned()
                    .collect();
                packet["messages"][1]["content"] =
                    json!(serde_json::to_string(&envelope).expect("serialize packet envelope"));
                packet["receipt"]["sourceHandles"] = packet["receipt"]["sourceHandles"]
                    .as_array()
                    .expect("packet source handles")
                    .iter()
                    .filter(|handle| handle.as_str() != Some(mandatory.as_str()))
                    .cloned()
                    .collect();
                packet["receipt"]["coverage"] = packet["receipt"]["coverage"]
                    .as_array()
                    .expect("packet coverage")
                    .iter()
                    .filter(|entry| entry["handle"].as_str() != Some(mandatory.as_str()))
                    .cloned()
                    .collect();
            }) as Box<dyn FnOnce(&mut Value)>,
        ),
        (
            "source-metadata-change",
            Box::new(|packet: &mut Value| {
                let mut envelope: Value = serde_json::from_str(
                    packet["messages"][1]["content"]
                        .as_str()
                        .expect("packet evidence envelope"),
                )
                .expect("decode packet evidence envelope");
                envelope["target"]["source"]["bodyHash"] = json!("0".repeat(64));
                packet["messages"][1]["content"] =
                    json!(serde_json::to_string(&envelope).expect("serialize packet envelope"));
            }) as Box<dyn FnOnce(&mut Value)>,
        ),
        (
            "final-instruction-change",
            Box::new(|packet: &mut Value| {
                packet["messages"][2]["content"] = json!("A different final instruction.");
            }) as Box<dyn FnOnce(&mut Value)>,
        ),
        (
            "options-change",
            Box::new(|packet: &mut Value| {
                packet["options"]["maxOutputTokens"] = json!("17");
            }) as Box<dyn FnOnce(&mut Value)>,
        ),
    ] {
        let temp = TempDir::new(label);
        let (project, access, document) = setup_project(&temp.child("project"));
        let second = project
            .create_document(CreateDocument {
                access: access.clone(),
                operation_id: format!("create-second-{label}"),
                document_id: "chapter-two".into(),
                title: "Chapter two".into(),
                kind: "chapter".into(),
                body: body("The mandatory evidence is here."),
            })
            .expect("create mandatory source document");
        let snapshot = freeze(&project, &access, &document);
        let mandatory_handle = snapshot
            .snapshot
            .sources
            .iter()
            .find(|source| source.source.document_id == second.head.document_id)
            .expect("find mandatory source")
            .handle
            .clone();
        let mut request = prepare_request(
            &access,
            &snapshot.snapshot.snapshot_id,
            &format!("tampered-{label}"),
            budget(),
        );
        request.mandatory_handles = vec![mandatory_handle];
        let packet = prepared(project.prepare_context(request).expect("prepare packet"));
        rewrite_packet(&project, &packet.receipt.packet_id, mutate);

        let read_error = project
            .prepared_context(access, packet.receipt.packet_id.clone())
            .expect_err("coherently tampered packet must fail validated read");
        assert_eq!(read_error.code, "InvalidContextPacket", "{label}");
        let transfer_error = create_backup(&project, &temp.child("tampered.wnsbackup"))
            .expect_err("coherently tampered packet must fail transfer validation");
        assert_eq!(transfer_error.code, "InvalidBackup", "{label}");
    }
}

#[test]
fn generic_preparation_rejects_consumed_discussion_request_guidance() {
    use webnovel_core::context::guidance::GuidanceScope;

    let temp = TempDir::new("request-guidance-replay");
    let (project, access, document) = setup_project(&temp.child("project"));
    project
        .save_guidance(webnovel_core::projects::guidance::SaveGuidance {
            access: access.clone(),
            operation_id: "adopt-request-guidance".into(),
            guidance_id: "request-guidance".into(),
            expected_version: "0".into(),
            text: "Keep the answer anchored to the pendant.".into(),
            scope: GuidanceScope::Request,
            document_id: Some(document.head.document_id.clone()),
            active: true,
            origin_message_id: None,
        })
        .expect("adopt request guidance");

    let discussion_request = StartDiscussion {
        access: access.clone(),
        operation_id: "discussion-consumes-request-guidance".into(),
        expected: document.head.clone(),
        instruction: "Discuss the selected passage.".into(),
        intent: Default::default(),
        scope: None,
        pinned_document_ids: Vec::new(),
        safe_brief: None,
        budget: budget(),
        provider_binding: None,
        previous_run_id: None,
    };
    let original = project
        .start_discussion(discussion_request.clone())
        .expect("start discussion with request guidance");

    let error = project
        .prepare_context(prepare_request(
            &access,
            &original.packet.receipt.snapshot_id,
            "generic-request-guidance-replay",
            budget(),
        ))
        .expect_err("generic preparation must reject discussion-only guidance");
    assert_eq!(error.code, "RequestGuidanceRequiresDiscussion");

    let replay = project
        .start_discussion(discussion_request)
        .expect("original discussion operation remains idempotent");
    assert_eq!(replay.packet, original.packet);
    assert_eq!(
        project
            .prepared_context(access, original.packet.receipt.packet_id.clone())
            .expect("original discussion packet remains readable"),
        original.packet
    );
}
