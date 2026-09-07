use rusqlite::Connection;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::{fs, mem};
use uuid::Uuid;
use webnovel_core::context::memory::mock_navigation_digest;
use webnovel_core::context::packet::{
    HTTP_MEMORY_INPUT_LIMIT_BYTES, HTTP_MEMORY_MODEL_ID, HTTP_MEMORY_OUTPUT_LIMIT_BYTES,
    HTTP_MEMORY_PROFILE_VERSION, HTTP_MEMORY_REASONING, HTTP_TOKEN_ACCOUNTING_METHOD,
    HttpProviderBinding, HttpResponseFormat, MockContextBudget, ProviderBinding, serialized_input,
};
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::projects::discussions::HttpProviderUsage;
use webnovel_core::projects::discussions::{HttpDeliverySubmission, ProviderDeliveryReceipt};
use webnovel_core::projects::memory::{CompleteMemory, MemoryJobStatus, StartMemory};
use webnovel_core::projects::story_context::FreezeStory;
use webnovel_core::projects::{
    CreateDocument, DocumentRecord, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::providers::http_request::prepare_request;
use webnovel_core::transfer::create_backup;

struct TempProject {
    path: PathBuf,
}

impl TempProject {
    fn new(label: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(format!("wns-memory-{label}-{}", Uuid::new_v4())),
        }
    }

    fn create(&self) -> (ProjectSession, ProjectAccess, DocumentRecord) {
        let project = ProjectSession::create(&self.path, "Memory storage test").unwrap();
        let access = project.attach("memory-renderer".into()).unwrap();
        let document = project
            .create_document(CreateDocument {
                access: access.clone(),
                operation_id: "create-chapter".into(),
                document_id: "chapter-one".into(),
                title: "Chapter one".into(),
                kind: "chapter".into(),
                body: body("Mei opened the lantern."),
            })
            .unwrap();
        (project, access, document)
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
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

fn budget() -> MockContextBudget {
    MockContextBudget::new("100000", "100", "100")
}

fn http_memory_binding() -> ProviderBinding {
    ProviderBinding {
        provider_id: "openai-compatible:00000000-0000-0000-0000-000000000001".into(),
        model_id: HTTP_MEMORY_MODEL_ID.into(),
        reasoning: Some(HTTP_MEMORY_REASONING.into()),
        service_tier: None,
        profile_version: HTTP_MEMORY_PROFILE_VERSION.into(),
        input_limit_bytes: HTTP_MEMORY_INPUT_LIMIT_BYTES.to_string(),
        reserved_output_bytes: "0".into(),
        reserved_protocol_bytes: "0".into(),
        output_limit_bytes: HTTP_MEMORY_OUTPUT_LIMIT_BYTES.to_string(),
        accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.into(),
        runtime: None,
        http: Some(HttpProviderBinding {
            base_url: "https://example.test/v1".into(),
            config_revision: "1".into(),
            stream: true,
            response_format: HttpResponseFormat::JsonObject,
        }),
    }
}

fn start_request(
    access: &ProjectAccess,
    document: &DocumentRecord,
    operation: &str,
) -> StartMemory {
    StartMemory {
        access: access.clone(),
        operation_id: operation.into(),
        expected: document.head.clone(),
        budget: budget(),
        provider_binding: None,
    }
}

#[test]
fn author_selected_codex_binding_cannot_change_the_memory_model() {
    let temp = TempProject::new("fixed-memory-model");
    let (project, access, document) = temp.create();
    let mut request = start_request(&access, &document, "memory-author-profile");
    request.provider_binding = Some(ProviderBinding::codex_author_runtime(
        "gpt-6-astra",
        "ultra",
        Some("priority"),
        "9.1",
        &"a".repeat(64),
        &"b".repeat(64),
    ));
    assert_eq!(
        project.start_memory(request).unwrap_err().code,
        "UnsupportedProviderFeature"
    );
    assert_eq!(
        project
            .document(access, document.head.document_id.clone())
            .unwrap()
            .body,
        document.body
    );
}

#[test]
fn http_memory_completion_retains_exact_delivery_and_reopens_without_codex_fields() {
    let temp = TempProject::new("http-memory-delivery");
    let (project, access, document) = temp.create();
    let mut request = start_request(&access, &document, "memory-http");
    request.provider_binding = Some(http_memory_binding());
    let job = project.start_memory(request).unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    let raw = serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap()).unwrap();
    let prepared = prepare_request(&dispatch.packet.messages, &dispatch.packet.options).unwrap();
    let delivery = ProviderDeliveryReceipt {
        body_hash: prepared.body_hash,
        body_bytes: prepared.body_bytes,
        submission: HttpDeliverySubmission::ResponseReceived,
        usage: None,
    };
    let completion = project
        .complete_memory(CompleteMemory {
            owner: dispatch.job.owner.clone(),
            event_id: "http-memory-result".into(),
            raw_output: raw,
            outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: Some(webnovel_core::projects::discussions::ProviderCleanup::Settled),
            error: None,
            effective_identity: None,
            delivery: Some(delivery.clone()),
        })
        .unwrap();
    assert_eq!(completion.result.delivery, Some(delivery.clone()));
    assert!(completion.result.confirmed_stdin_bytes.is_none());
    assert!(completion.result.usage.is_none());

    drop(project);
    let reopened = ProjectSession::open(&temp.path).unwrap();
    let access = reopened.attach("http-memory-reopen".into()).unwrap();
    let read = reopened
        .read_memory(access, document.head.document_id)
        .unwrap();
    let result = read.jobs[0].result.as_ref().expect("retained result");
    assert_eq!(result.delivery, Some(delivery));
    assert!(result.confirmed_stdin_bytes.is_none());
    assert!(result.usage.is_none());
}

#[test]
fn http_memory_completion_requires_received_response_and_exact_body() {
    let temp = TempProject::new("http-memory-receipt");
    let (project, access, document) = temp.create();
    let mut request = start_request(&access, &document, "memory-http-receipt");
    request.provider_binding = Some(http_memory_binding());
    let job = project.start_memory(request).unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    let raw = serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap()).unwrap();
    let prepared = prepare_request(&dispatch.packet.messages, &dispatch.packet.options).unwrap();
    let mut delivery = ProviderDeliveryReceipt {
        body_hash: prepared.body_hash,
        body_bytes: prepared.body_bytes,
        submission: HttpDeliverySubmission::Uncertain,
        usage: None,
    };
    let rejected = project.complete_memory(CompleteMemory {
        owner: dispatch.job.owner.clone(),
        event_id: "http-memory-uncertain".into(),
        raw_output: raw.clone(),
        outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: None,
        usage: None,
        cleanup: Some(webnovel_core::projects::discussions::ProviderCleanup::Settled),
        error: None,
        effective_identity: None,
        delivery: Some(delivery.clone()),
    });
    assert_eq!(rejected.unwrap_err().code, "ProviderInputUnknown");

    delivery.body_bytes = "1".into();
    delivery.submission = HttpDeliverySubmission::ResponseReceived;
    let rejected = project.complete_memory(CompleteMemory {
        owner: dispatch.job.owner.clone(),
        event_id: "http-memory-mismatch".into(),
        raw_output: raw,
        outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: None,
        usage: None,
        cleanup: Some(webnovel_core::projects::discussions::ProviderCleanup::Settled),
        error: None,
        effective_identity: None,
        delivery: Some(delivery),
    });
    assert_eq!(rejected.unwrap_err().code, "ProviderInputMismatch");

    let prepared = prepare_request(&dispatch.packet.messages, &dispatch.packet.options).unwrap();
    let rejected = project.complete_memory(CompleteMemory {
        owner: dispatch.job.owner.clone(),
        event_id: "http-memory-missing-cleanup".into(),
        raw_output: serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap())
            .unwrap(),
        outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: None,
        usage: None,
        cleanup: None,
        error: None,
        effective_identity: None,
        delivery: Some(ProviderDeliveryReceipt {
            body_hash: prepared.body_hash.clone(),
            body_bytes: prepared.body_bytes.clone(),
            submission: HttpDeliverySubmission::ResponseReceived,
            usage: None,
        }),
    });
    assert_eq!(rejected.unwrap_err().code, "ProviderCleanupUnknown");

    let rejected = project.complete_memory(CompleteMemory {
        owner: dispatch.job.owner.clone(),
        event_id: "http-memory-not-sent-output".into(),
        raw_output: "unexpected provider output".into(),
        outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Failed,
        confirmed_stdin_bytes: None,
        usage: None,
        cleanup: Some(webnovel_core::projects::discussions::ProviderCleanup::Settled),
        error: Some("request was not sent".into()),
        effective_identity: None,
        delivery: Some(ProviderDeliveryReceipt {
            body_hash: prepared.body_hash.clone(),
            body_bytes: prepared.body_bytes.clone(),
            submission: HttpDeliverySubmission::NotSent,
            usage: None,
        }),
    });
    assert_eq!(rejected.unwrap_err().code, "InvalidRequest");

    let rejected = project.complete_memory(CompleteMemory {
        owner: dispatch.job.owner,
        event_id: "http-memory-uncertain-usage".into(),
        raw_output: String::new(),
        outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Failed,
        confirmed_stdin_bytes: None,
        usage: None,
        cleanup: Some(webnovel_core::projects::discussions::ProviderCleanup::Settled),
        error: Some("request outcome uncertain".into()),
        effective_identity: None,
        delivery: Some(ProviderDeliveryReceipt {
            body_hash: prepared.body_hash,
            body_bytes: prepared.body_bytes,
            submission: HttpDeliverySubmission::Uncertain,
            usage: Some(HttpProviderUsage {
                input_tokens: Some(1),
                output_tokens: Some(1),
                total_tokens: Some(2),
            }),
        }),
    });
    assert_eq!(rejected.unwrap_err().code, "InvalidRequest");
}

fn complete_mock(
    project: &ProjectSession,
    dispatch: &webnovel_core::projects::memory::MemoryDispatch,
    event_id: &str,
    raw: Option<String>,
) -> webnovel_core::projects::memory::MemoryCompletion {
    let raw = raw.unwrap_or_else(|| {
        serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap()).unwrap()
    });
    project
        .complete_memory(CompleteMemory {
            owner: dispatch.job.owner.clone(),
            event_id: event_id.into(),
            raw_output: raw,
            outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: None,
            error: None,
            effective_identity: None,
            delivery: None,
        })
        .unwrap()
}

#[test]
fn start_replay_ignores_lease_rotation_and_begin_is_idempotent_without_redispatch() {
    let temp = TempProject::new("replay");
    let (project, access, document) = temp.create();
    let request = start_request(&access, &document, "memory-one");
    let first = project.start_memory(request.clone()).unwrap();
    let replacement = project.attach("memory-renderer-two".into()).unwrap();
    let mut replay = request;
    replay.access = replacement.clone();
    assert_eq!(project.start_memory(replay).unwrap().id, first.id);

    let dispatch = project.begin_memory(first.owner.clone()).unwrap();
    assert!(dispatch.newly_dispatched);
    assert_eq!(dispatch.source.descriptor.source, first.source);
    let replayed = project.begin_memory(first.owner.clone()).unwrap();
    assert!(!replayed.newly_dispatched);
    assert_eq!(replayed.packet, dispatch.packet);

    let mut changed = start_request(&replacement, &document, "memory-one");
    changed.budget = MockContextBudget::new("90000", "100", "100");
    assert_eq!(
        project.start_memory(changed).unwrap_err().code,
        "OperationIdReusedWithDifferentPayload"
    );
}

#[test]
fn stale_before_begin_is_refused_but_late_completion_is_retained_as_historical() {
    let temp = TempProject::new("stale");
    let (project, access, document) = temp.create();
    let queued = project
        .start_memory(start_request(&access, &document, "memory-before-stale"))
        .unwrap();
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "save-after-memory-start".into(),
            expected: document.head.clone(),
            local_generation: "1".into(),
            body: body("Mei lifted the lantern."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    assert_eq!(
        project.begin_memory(queued.owner.clone()).unwrap_err().code,
        "MemoryBasisChanged"
    );

    let current = project
        .document(access.clone(), "chapter-one".into())
        .unwrap();
    let second = project
        .start_memory(start_request(&access, &current, "memory-during-stale"))
        .unwrap();
    let dispatch = project.begin_memory(second.owner.clone()).unwrap();
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "save-during-memory".into(),
            expected: current.head,
            local_generation: "2".into(),
            body: body("Mei extinguished the lantern."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let completion = complete_mock(&project, &dispatch, "memory-finish", None);
    assert_eq!(completion.job.status, MemoryJobStatus::Completed);
    let view = project.install_memory(second.owner).unwrap();
    assert!(!view.current);
    assert!(view.source_changed);
    assert!(view.candidate.is_some());
}

#[test]
fn unrelated_source_epoch_keeps_exact_memory_view_current_without_rewriting_history() {
    let temp = TempProject::new("unrelated-epoch");
    let (project, access, document) = temp.create();
    let discussion = project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: "discussion-before-unrelated-epoch".into(),
            expected: document.head.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::Discuss,
            policy: InformationPolicy {
                version: project.context_epochs(access.clone()).unwrap().policy,
                audience: Audience::AuthorRoom,
                reader_frontier: None,
                character_id: None,
                character_grants: Vec::new(),
                allow_alternatives: false,
                allow_historical: false,
            },
        })
        .unwrap();
    let job = project
        .start_memory(start_request(&access, &document, "memory-unrelated-epoch"))
        .unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-unrelated-chapter".into(),
            document_id: "unrelated-chapter".into(),
            title: "Unrelated chapter".into(),
            kind: "chapter".into(),
            body: body("A later chapter changes the collection epoch."),
        })
        .unwrap();
    let completion = complete_mock(&project, &dispatch, "memory-unrelated-result", None);
    let view = project.install_memory(job.owner).unwrap();
    assert_eq!(completion.job.status, MemoryJobStatus::Completed);
    assert!(view.current);
    assert!(!view.source_changed);

    let db = Connection::open(temp.path.join("project.sqlite3")).unwrap();
    let installed_current: i64 = db
        .query_row(
            "SELECT installed_current FROM memory_views WHERE id=?",
            [&view.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(installed_current, 0);
    drop(db);

    let read = project
        .read_memory(access.clone(), document.head.document_id.clone())
        .unwrap();
    assert!(
        read.views
            .iter()
            .any(|item| item.id == view.id && item.current)
    );
    assert!(
        !project
            .story_snapshot_is_current(access.clone(), discussion.snapshot.snapshot_id)
            .unwrap()
    );

    mem::drop(project);
    let reopened = ProjectSession::open(&temp.path).unwrap();
    let reopened_access = reopened.attach("memory-unrelated-reopen".into()).unwrap();
    let unrelated = reopened
        .document(reopened_access.clone(), "unrelated-chapter".into())
        .unwrap();
    let epochs = reopened.context_epochs(reopened_access.clone()).unwrap();
    let frozen = reopened
        .freeze_story(FreezeStory {
            access: reopened_access,
            operation_id: "discussion-after-unrelated-epoch".into(),
            expected: unrelated.head,
            basis: BasisKind::Working,
            purpose: ContextPurpose::Discuss,
            policy: InformationPolicy {
                version: epochs.policy,
                audience: Audience::AuthorRoom,
                reader_frontier: None,
                character_id: None,
                character_grants: Vec::new(),
                allow_alternatives: false,
                allow_historical: false,
            },
        })
        .unwrap();
    assert_eq!(frozen.navigation_views.len(), 1);
    assert_eq!(frozen.navigation_views[0].reference.view_id, view.id);
}

#[test]
fn malformed_terminal_output_is_retained_and_cannot_install() {
    let temp = TempProject::new("malformed");
    let (project, access, document) = temp.create();
    let job = project
        .start_memory(start_request(&access, &document, "memory-malformed"))
        .unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    let completion = complete_mock(
        &project,
        &dispatch,
        "memory-malformed-result",
        Some("{}".into()),
    );
    assert_eq!(completion.job.status, MemoryJobStatus::Failed);
    assert_eq!(completion.result.raw_output.as_deref(), Some("{}"));
    assert!(completion.result.validation_error.is_some());
    assert_eq!(
        project.install_memory(job.owner).unwrap_err().code,
        "MemoryInstallBlocked"
    );
}

#[test]
fn stop_before_dispatch_and_recovery_prevent_dispatch_or_install() {
    let temp = TempProject::new("stop-recover");
    let (project, access, document) = temp.create();
    let queued = project
        .start_memory(start_request(&access, &document, "memory-stop"))
        .unwrap();
    let stopped = project
        .stop_memory(access.clone(), queued.id.clone())
        .unwrap();
    assert_eq!(stopped.status, MemoryJobStatus::Stopped);
    assert_eq!(
        project.begin_memory(queued.owner.clone()).unwrap_err().code,
        "MemoryJobSealed"
    );

    let running = project
        .start_memory(start_request(&access, &document, "memory-recover"))
        .unwrap();
    let dispatch = project.begin_memory(running.owner.clone()).unwrap();
    mem::drop(project);
    let reopened = ProjectSession::open(&temp.path).unwrap();
    let access = reopened.attach("memory-reopened".into()).unwrap();
    let read = reopened.read_memory(access, "chapter-one".into()).unwrap();
    let recovered = read
        .jobs
        .into_iter()
        .find(|job| job.id == dispatch.job.id)
        .unwrap();
    assert_eq!(recovered.status, MemoryJobStatus::Interrupted);
    assert_eq!(
        reopened.begin_memory(dispatch.job.owner).unwrap_err().code,
        "MemoryJobSealed"
    );
}

#[test]
fn queued_stop_without_terminal_result_is_valid_backup_history() {
    let temp = TempProject::new("stopped-backup");
    let (project, access, document) = temp.create();
    let queued = project
        .start_memory(start_request(&access, &document, "memory-stopped-backup"))
        .unwrap();
    let stopped = project.stop_memory(access, queued.id.clone()).unwrap();
    assert_eq!(stopped.status, MemoryJobStatus::Stopped);
    let backup = temp.path.with_extension("wnsbackup");
    let manifest = create_backup(&project, &backup).unwrap();
    assert_eq!(manifest.database_schema_version, 37);
    let _ = fs::remove_file(backup);
}

#[test]
fn unresolved_cleanup_stays_interrupted_and_retains_result_without_installation() {
    let temp = TempProject::new("cleanup-unresolved");
    let (project, access, document) = temp.create();
    let job = project
        .start_memory(start_request(
            &access,
            &document,
            "memory-cleanup-unresolved",
        ))
        .unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    project.stop_memory(access, job.id.clone()).unwrap();
    let completion = project
        .complete_memory(CompleteMemory {
            owner: dispatch.job.owner.clone(),
            event_id: "cleanup-unresolved-result".into(),
            raw_output: "partial provider output".into(),
            outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Failed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: Some(webnovel_core::projects::discussions::ProviderCleanup::Unresolved),
            error: Some("cleanup could not be confirmed".into()),
            effective_identity: None,
            delivery: None,
        })
        .unwrap();
    assert_eq!(completion.job.status, MemoryJobStatus::Interrupted);
    assert_eq!(
        completion.job.stop_reason.as_deref(),
        Some("author_stopped")
    );
    assert_eq!(
        completion.result.cleanup,
        Some(webnovel_core::projects::discussions::ProviderCleanup::Unresolved)
    );
    assert_eq!(
        completion.result.raw_output.as_deref(),
        Some("partial provider output")
    );
    assert_eq!(
        project.install_memory(job.owner).unwrap_err().code,
        "MemoryInstallBlocked"
    );
    let backup = temp.path.with_extension("wnsbackup");
    create_backup(&project, &backup).unwrap();
    let _ = fs::remove_file(backup);
}

#[test]
fn uncertain_dispatch_claim_interrupts_without_generation_and_replays_terminal() {
    let temp = TempProject::new("interrupt-claim");
    let (project, access, document) = temp.create();
    let queued = project
        .start_memory(start_request(&access, &document, "memory-uncertain-queued"))
        .unwrap();
    let interrupted = project
        .interrupt_memory_claim(queued.owner.clone())
        .unwrap();
    assert_eq!(interrupted.status, MemoryJobStatus::Interrupted);
    assert_eq!(
        interrupted.stop_reason.as_deref(),
        Some("dispatch_outcome_unknown")
    );
    assert!(interrupted.result.is_none());
    let replay = project
        .interrupt_memory_claim(queued.owner.clone())
        .unwrap();
    assert_eq!(replay.status, MemoryJobStatus::Interrupted);
    assert_eq!(
        project.begin_memory(queued.owner).unwrap_err().code,
        "MemoryJobSealed"
    );

    let running = project
        .start_memory(start_request(
            &access,
            &document,
            "memory-uncertain-running",
        ))
        .unwrap();
    let dispatch = project.begin_memory(running.owner.clone()).unwrap();
    let running_interrupted = project
        .interrupt_memory_claim(dispatch.job.owner.clone())
        .unwrap();
    assert_eq!(running_interrupted.status, MemoryJobStatus::Interrupted);
    assert!(running_interrupted.result.is_none());

    let terminal = project
        .start_memory(start_request(
            &access,
            &document,
            "memory-uncertain-terminal",
        ))
        .unwrap();
    let terminal_dispatch = project.begin_memory(terminal.owner.clone()).unwrap();
    complete_mock(&project, &terminal_dispatch, "terminal-result", None);
    let terminal_replay = project.interrupt_memory_claim(terminal.owner).unwrap();
    assert_eq!(terminal_replay.status, MemoryJobStatus::Completed);
}

#[test]
fn reopened_dispatched_claim_can_settle_historical_result_without_installation() {
    let temp = TempProject::new("reopen-settle");
    let (project, _access, document) = temp.create();
    let queued = project
        .start_memory({
            let access = project.attach("reopen-settle-renderer".into()).unwrap();
            start_request(&access, &document, "memory-reopen-settle")
        })
        .unwrap();
    let dispatch = project.begin_memory(queued.owner.clone()).unwrap();
    let raw = serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap()).unwrap();
    mem::drop(project);

    let reopened = ProjectSession::open(&temp.path).unwrap();
    let access = reopened.attach("reopen-settle-after-open".into()).unwrap();
    let completion = reopened
        .complete_memory(CompleteMemory {
            owner: dispatch.job.owner.clone(),
            event_id: "reopen-settle-result".into(),
            raw_output: raw,
            outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: None,
            error: None,
            effective_identity: None,
            delivery: None,
        })
        .unwrap();
    assert_eq!(completion.job.status, MemoryJobStatus::Interrupted);
    assert_eq!(
        completion.job.stop_reason.as_deref(),
        Some("recovered_unknown_external_outcome")
    );
    assert!(completion.job.view.is_none());
    assert!(completion.result.raw_output.is_some());
    assert_eq!(
        reopened
            .install_memory(dispatch.job.owner.clone())
            .unwrap_err()
            .code,
        "MemoryInstallBlocked"
    );
    let read = reopened
        .read_memory(access, document.head.document_id)
        .unwrap();
    assert_eq!(read.jobs[0].status, MemoryJobStatus::Interrupted);
    assert!(read.jobs[0].result.is_some());
    let backup = temp.path.with_extension("wnsbackup");
    create_backup(&reopened, &backup).unwrap();
    let _ = fs::remove_file(backup);
}

#[test]
fn live_completion_requires_exact_delivery_and_foreign_owner_is_refused() {
    let temp = TempProject::new("live-proof");
    let (project, access, document) = temp.create();
    let mut request = start_request(&access, &document, "memory-live");
    request.provider_binding = Some(ProviderBinding::codex_luna());
    let job = project.start_memory(request).unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    let raw = serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap()).unwrap();
    let input_bytes = serialized_input(&dispatch.packet.messages, &dispatch.packet.options)
        .unwrap()
        .len()
        .to_string();
    let missing_cleanup = CompleteMemory {
        owner: dispatch.job.owner.clone(),
        event_id: "live-cleanup-unknown".into(),
        raw_output: raw.clone(),
        outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: Some(input_bytes.clone()),
        usage: None,
        cleanup: None,
        error: None,
        effective_identity: None,
        delivery: None,
    };
    assert_eq!(
        project.complete_memory(missing_cleanup).unwrap_err().code,
        "ProviderCleanupUnknown"
    );
    let missing_delivery = CompleteMemory {
        owner: dispatch.job.owner.clone(),
        event_id: "live-result".into(),
        raw_output: raw.clone(),
        outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: None,
        usage: None,
        cleanup: Some(webnovel_core::projects::discussions::ProviderCleanup::Settled),
        error: None,
        effective_identity: None,
        delivery: None,
    };
    assert_eq!(
        project.complete_memory(missing_delivery).unwrap_err().code,
        "ProviderInputUnknown"
    );
    let mut complete = CompleteMemory {
        owner: dispatch.job.owner.clone(),
        event_id: "live-result".into(),
        raw_output: raw,
        outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: None,
        usage: None,
        cleanup: Some(webnovel_core::projects::discussions::ProviderCleanup::Settled),
        error: None,
        effective_identity: None,
        delivery: None,
    };
    complete.confirmed_stdin_bytes = Some(input_bytes);
    let completion = project.complete_memory(complete).unwrap();
    assert_eq!(completion.job.status, MemoryJobStatus::Completed);

    let second = TempProject::new("foreign-owner");
    let (foreign, _, _) = second.create();
    let foreign_owner = webnovel_core::projects::memory::MemoryOwner {
        project_id: foreign.info.project_id.clone(),
        operation_namespace: foreign.info.operation_namespace.clone(),
        job_id: job.id,
    };
    assert_eq!(
        foreign.begin_memory(foreign_owner).unwrap_err().code,
        "MemoryJobNotFound"
    );
}

#[test]
fn policy_revocation_redacts_terminal_candidate_and_raw_output() {
    let temp = TempProject::new("policy");
    let (project, access, document) = temp.create();
    let job = project
        .start_memory(start_request(&access, &document, "memory-policy"))
        .unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    let raw = serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap()).unwrap();
    project
        .complete_memory(CompleteMemory {
            owner: dispatch.job.owner.clone(),
            event_id: "policy-result".into(),
            raw_output: raw,
            outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: None,
            error: Some("diagnostic copied chapter prose".into()),
            effective_identity: Some("provider-identity".into()),
            delivery: None,
        })
        .unwrap();
    project
        .revoke_story_context(access.clone(), "0".into())
        .unwrap();
    let read = project.read_memory(access, "chapter-one".into()).unwrap();
    let retained = read
        .jobs
        .into_iter()
        .find(|item| item.id == job.id)
        .unwrap();
    assert!(retained.result.as_ref().unwrap().raw_output.is_none());
    assert!(retained.result.as_ref().unwrap().candidate.is_none());
    assert!(retained.result.as_ref().unwrap().error.is_none());
    assert!(retained.result.as_ref().unwrap().validation_error.is_none());
    assert!(
        retained
            .result
            .as_ref()
            .unwrap()
            .effective_identity
            .is_none()
    );
}

#[test]
fn backup_validation_accepts_retained_memory_history() {
    let temp = TempProject::new("backup");
    let (project, access, document) = temp.create();
    let job = project
        .start_memory(start_request(&access, &document, "memory-backup"))
        .unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    complete_mock(&project, &dispatch, "backup-result", None);
    project.install_memory(job.owner).unwrap();
    let backup = temp.path.with_extension("wnsbackup");
    let manifest = create_backup(&project, &backup).unwrap();
    assert_eq!(manifest.database_schema_version, 37);
    let _ = fs::remove_file(backup);
}

#[test]
fn backup_rejects_an_unusable_view_identifier() {
    let temp = TempProject::new("invalid-view-id");
    let (project, access, document) = temp.create();
    let job = project
        .start_memory(start_request(&access, &document, "memory-invalid-id"))
        .unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    complete_mock(&project, &dispatch, "invalid-id-result", None);
    let view = project.install_memory(job.owner).unwrap();
    let db = rusqlite::Connection::open(temp.path.join("project.sqlite3")).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys=OFF; DROP TRIGGER memory_views_no_update; DROP TRIGGER memory_view_sources_no_update;",
    )
    .unwrap();
    db.execute("UPDATE memory_views SET id='' WHERE id=?", [&view.id])
        .unwrap();
    db.execute(
        "UPDATE memory_view_sources SET view_id='' WHERE view_id=?",
        [&view.id],
    )
    .unwrap();
    let backup = temp.path.with_extension("wnsbackup");
    assert_eq!(
        create_backup(&project, &backup).unwrap_err().code,
        "InvalidBackup"
    );
    assert!(!backup.exists());
}

#[test]
fn backup_rejects_a_failed_job_with_a_valid_completed_result() {
    let temp = TempProject::new("invalid-terminal-pair");
    let (project, access, document) = temp.create();
    let job = project
        .start_memory(start_request(&access, &document, "memory-invalid-pair"))
        .unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    complete_mock(&project, &dispatch, "invalid-pair-result", None);
    let db = rusqlite::Connection::open(temp.path.join("project.sqlite3")).unwrap();
    db.execute(
        "UPDATE memory_jobs SET status='failed' WHERE id=?",
        [&job.id],
    )
    .unwrap();
    let backup = temp.path.with_extension("wnsbackup");
    assert_eq!(
        create_backup(&project, &backup).unwrap_err().code,
        "InvalidBackup"
    );
    assert!(!backup.exists());
}

#[test]
fn recovered_memory_keeps_original_evidence_without_operation_authority() {
    let temp = TempProject::new("recover-source");
    let recovered_temp = TempProject::new("recovered-source");
    let (project, access, document) = temp.create();
    let job = project
        .start_memory(start_request(&access, &document, "memory-recovery"))
        .unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    complete_mock(&project, &dispatch, "recovery-result", None);
    let view = project.install_memory(job.owner.clone()).unwrap();
    assert!(!view.historical);
    let original = project.read_memory_source(access, view.id.clone()).unwrap();
    let backup = temp.path.with_extension("wnsbackup");
    create_backup(&project, &backup).unwrap();
    let recovered =
        webnovel_core::transfer::recover_backup(&backup, &recovered_temp.path, "Recovered memory")
            .unwrap();
    let recovered_access = recovered
        .attach("recovered-memory-renderer".into())
        .unwrap();
    let read = recovered
        .read_memory(recovered_access.clone(), document.head.document_id)
        .unwrap();
    assert_eq!(read.jobs.len(), 1);
    assert!(read.jobs[0].historical);
    assert_eq!(read.jobs[0].owner, job.owner);
    assert_eq!(read.views.len(), 1);
    assert!(read.views[0].historical);
    assert!(!read.views[0].current);
    assert_eq!(read.views[0].source, view.source);
    assert_eq!(read.views[0].candidate, view.candidate);
    let inspected = recovered
        .read_memory_source(recovered_access.clone(), view.id.clone())
        .unwrap();
    assert_eq!(inspected.descriptor, original.descriptor);
    assert_eq!(inspected.body, original.body);
    assert_eq!(inspected.passages, original.passages);
    assert_eq!(
        recovered.begin_memory(job.owner.clone()).unwrap_err().code,
        "MemoryProjectMismatch"
    );
    assert_eq!(
        recovered.install_memory(job.owner).unwrap_err().code,
        "MemoryProjectMismatch"
    );
    let policy = recovered.context_epochs(recovered_access.clone()).unwrap();
    recovered
        .revoke_story_context(recovered_access.clone(), policy.policy)
        .unwrap();
    assert_eq!(
        recovered
            .read_memory_source(recovered_access.clone(), view.id)
            .unwrap_err()
            .code,
        "ContextPolicyChanged"
    );
    let revoked = recovered
        .read_memory(recovered_access, read.document_id)
        .unwrap();
    assert!(revoked.views[0].historical);
    assert!(!revoked.views[0].policy_available);
    assert!(revoked.views[0].candidate.is_none());
    let _ = fs::remove_file(backup);
}

#[test]
fn backup_rejects_mismatched_delivery_and_invalid_lifecycle() {
    let temp = TempProject::new("backup-corruption");
    let (project, access, document) = temp.create();
    let mut request = start_request(&access, &document, "memory-corrupt-delivery");
    request.provider_binding = Some(ProviderBinding::codex_luna());
    let job = project.start_memory(request).unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    let raw = serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap()).unwrap();
    let input_bytes = serialized_input(&dispatch.packet.messages, &dispatch.packet.options)
        .unwrap()
        .len()
        .to_string();
    project
        .complete_memory(CompleteMemory {
            owner: dispatch.job.owner,
            event_id: "corrupt-delivery".into(),
            raw_output: raw,
            outcome: webnovel_core::projects::discussions::ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: Some(input_bytes.clone()),
            usage: None,
            cleanup: Some(webnovel_core::projects::discussions::ProviderCleanup::Settled),
            error: None,
            effective_identity: None,
            delivery: None,
        })
        .unwrap();

    // The production trigger prevents this mutation.  A detached backup can
    // still be tampered with, so validate the immutable result against the
    // packet's exact delivery proof.
    let db = Connection::open(temp.path.join("project.sqlite3")).unwrap();
    db.execute_batch("DROP TRIGGER memory_results_no_update;")
        .unwrap();
    db.execute(
        "UPDATE memory_results SET confirmed_stdin_bytes='0' WHERE job_id=?",
        [&job.id],
    )
    .unwrap();
    let backup = temp.path.with_extension("wnsbackup");
    assert!(create_backup(&project, &backup).is_err());

    db.execute(
        "UPDATE memory_results SET confirmed_stdin_bytes=? WHERE job_id=?",
        (&input_bytes, &job.id),
    )
    .unwrap();
    db.execute(
        "UPDATE memory_jobs SET dispatch_state='pending' WHERE id=?",
        [&job.id],
    )
    .unwrap();
    assert!(create_backup(&project, &backup).is_err());
}
