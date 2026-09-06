use serde_json::json;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::lookup::{LOOKUP_SCHEMA_VERSION, LookupAllowance};
use webnovel_core::context::packet::{MockContextBudget, serialized_input};
use webnovel_core::projects::discussion_lookup::{
    LookupAdvance, LookupAdvanceRequest, LookupInvocationReport,
};
use webnovel_core::projects::discussions::{
    DiscussionBegin, FeedbackIntent, ProviderCleanup, ProviderOutcomeStatus, RunOwner,
    StartDiscussion,
};
use webnovel_core::projects::{CreateDocument, ProjectAccess, ProjectSession};
use webnovel_core::transfer::create_backup;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-lookup-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create temp directory");
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

fn body() -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "attrs": {"id": "p1"},
                "content": [{"type": "text", "text": "A jade pendant rests on the table."}]
            }]
        }
    })
}

fn setup() -> (
    TempDir,
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let temp = TempDir::new();
    let project = ProjectSession::create(temp.child("project"), "Lookup test").expect("create");
    let access = project.attach("lookup-session".into()).expect("attach");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body(),
        })
        .expect("create chapter");
    (temp, project, access, document)
}

fn initial_report(
    owner: &RunOwner,
    packet: &webnovel_core::context::packet::CompiledPacket,
    ordinal: &str,
    event_id: &str,
    raw: String,
) -> LookupInvocationReport {
    LookupInvocationReport {
        owner: owner.clone(),
        ordinal: ordinal.into(),
        event_id: event_id.into(),
        assistant_text: raw,
        binding: None,
        status: ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: serialized_input(&packet.messages, &packet.options)
            .expect("serialize packet")
            .len()
            .to_string(),
        usage: None,
        cleanup: ProviderCleanup::Settled,
        error: None,
    }
}

#[test]
fn lookup_claims_expands_exact_frozen_read_and_finishes_with_child_packet() {
    let (temp, project, access, document) = setup();
    let started = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "lookup-discussion".into(),
            expected: document.head.clone(),
            instruction: "Find the pendant detail.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: Some(LookupAllowance::default()),
        })
        .expect("start lookup discussion");
    let owner = started.run.owner.clone();
    let initial = project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .expect("begin");
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim initial lookup");
    assert_eq!(
        claimed.packet.receipt.packet_id,
        initial.packet.receipt.packet_id
    );
    let duplicate_claim = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect_err("a claimed invocation cannot be paid twice");
    assert_eq!(duplicate_claim.code, "LookupInvocationAlreadyClaimed");
    let needs_context = serde_json::to_string(&json!({
        "kind": "needsContext",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "reads": [{
            "kind": "search",
            "id": "find-pendant",
            "query": "pendant",
            "mode": "literal",
            "limit": 5
        }]
    }))
    .expect("needs-context JSON");
    let after_initial = project
        .settle_lookup_invocation(initial_report(
            &owner,
            &claimed.packet,
            "0",
            "lookup-result-0",
            needs_context,
        ))
        .expect("settle needs-context response");
    assert_eq!(
        after_initial.status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Running
    );
    assert_eq!(
        after_initial.lookup.as_ref().unwrap().invocations[0].state,
        webnovel_core::projects::discussion_lookup::LookupInvocationState::NeedsContext
    );

    let prepared = project
        .advance_lookup(LookupAdvanceRequest {
            owner: owner.clone(),
            completed_ordinal: "0".into(),
        })
        .expect("advance lookup");
    let child = match prepared {
        LookupAdvance::Prepared { dispatch } => dispatch,
        LookupAdvance::Finished { .. } => panic!("lookup should prepare a child packet"),
    };
    assert_eq!(child.ordinal, "1");
    assert_ne!(
        child.packet.receipt.packet_id,
        initial.packet.receipt.packet_id
    );
    let claimed_child = project
        .claim_lookup_invocation(owner.clone(), "1".into())
        .expect("claim child lookup");
    let final_raw = serde_json::to_string(&json!({
        "kind": "discussion",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "text": "The frozen chapter says the jade pendant rests on the table."
    }))
    .expect("final JSON");
    let final_run = project
        .settle_lookup_invocation(initial_report(
            &owner,
            &claimed_child.packet,
            "1",
            "lookup-result-1",
            final_raw,
        ))
        .expect("settle final lookup response");
    assert_eq!(
        final_run.status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Completed
    );
    let view = project
        .read_discussion(access, "chapter-one".into())
        .expect("read discussion");
    let assistant = view
        .messages
        .iter()
        .rev()
        .find(|message| {
            message.role == webnovel_core::projects::discussions::DiscussionMessageRole::Assistant
        })
        .expect("final assistant message");
    assert_eq!(
        assistant.packet_id.as_deref(),
        Some(child.packet.receipt.packet_id.as_str())
    );
    create_backup(&project, &temp.child("lookup.wnsbackup")).expect("validate lookup backup");
}

#[test]
fn stopping_lookup_settles_the_claimed_invocation_without_legacy_provider_receipt() {
    let (_temp, project, access, document) = setup();
    let started = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "lookup-stop".into(),
            expected: document.head,
            instruction: "Look up one detail.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: Some(LookupAllowance::default()),
        })
        .expect("start");
    let owner = started.run.owner.clone();
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .expect("begin");
    project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim");
    let stopping = project
        .stop_discussion(access.clone(), owner.run_id.clone())
        .expect("request stop")
        .run;
    assert_eq!(
        stopping.status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Stopping
    );
    let stopped = project
        .settle_lookup_invocation(LookupInvocationReport {
            owner: owner.clone(),
            ordinal: "0".into(),
            event_id: "lookup-stop-result".into(),
            assistant_text: String::new(),
            binding: None,
            status: ProviderOutcomeStatus::Stopped,
            confirmed_stdin_bytes: "0".into(),
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: None,
        })
        .expect("settle stop");
    assert_eq!(
        stopped.status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Stopped
    );
    assert!(stopped.provider_result.is_none());
    assert_eq!(stopped.packet_id, dispatch.packet.receipt.packet_id);
}

#[test]
fn malformed_completed_envelope_is_a_durable_failed_result_and_replays_by_exact_body() {
    let (_temp, project, access, document) = setup();
    let started = project
        .start_discussion(StartDiscussion {
            access,
            operation_id: "lookup-invalid-response".into(),
            expected: document.head,
            instruction: "Find the detail.".into(),
            intent: FeedbackIntent::Discuss,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: Some(LookupAllowance::default()),
        })
        .expect("start");
    let owner = started.run.owner.clone();
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .expect("begin");
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim");
    let report = initial_report(
        &owner,
        &claimed.packet,
        "0",
        "lookup-invalid-result",
        "not JSON".into(),
    );
    let failed = project
        .settle_lookup_invocation(report.clone())
        .expect("persist malformed response as failure");
    assert_eq!(
        failed.status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Failed
    );
    let replay = project
        .settle_lookup_invocation(report)
        .expect("reconcile exact failed report");
    assert_eq!(
        replay.status,
        webnovel_core::projects::discussions::DiscussionRunStatus::Failed
    );
    assert_eq!(failed.packet_id, dispatch.packet.receipt.packet_id);
}

#[test]
fn lookup_is_refused_for_non_discuss_intents() {
    let (_temp, project, access, document) = setup();
    let error = project
        .start_discussion(StartDiscussion {
            access,
            operation_id: "invalid-lookup-intent".into(),
            expected: document.head,
            instruction: "Rewrite it.".into(),
            intent: FeedbackIntent::ProposeEdits,
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
            previous_run_id: None,
            lookup: Some(LookupAllowance::default()),
        })
        .expect_err("lookup is restricted to ordinary discussion");
    assert_eq!(error.code, "UnsupportedContextTools");
}
