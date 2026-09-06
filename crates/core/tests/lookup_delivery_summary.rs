use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::lookup::LookupAllowance;
use webnovel_core::context::packet::{MockContextBudget, serialized_input};
use webnovel_core::projects::discussion_lookup::LookupInvocationReport;
use webnovel_core::projects::discussions::{
    DiscussionBegin, FeedbackIntent, ProviderCleanup, ProviderOutcomeStatus, StartDiscussion,
};
use webnovel_core::projects::{CreateDocument, ProjectAccess, ProjectSession};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-lookup-delivery-{}", Uuid::new_v4()));
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

fn body() -> Value {
    json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "attrs": {"id": "p1"},
                "content": [{"type": "text", "text": "An old promise remains."}]
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
    let project = ProjectSession::create(temp.child("project"), "Lookup delivery test")
        .expect("create project");
    let access = project
        .attach("lookup-delivery-session".into())
        .expect("attach");
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

fn report(
    start: &webnovel_core::projects::discussions::DiscussionStart,
    packet: &webnovel_core::context::packet::CompiledPacket,
    ordinal: &str,
    event_id: &str,
    status: ProviderOutcomeStatus,
    assistant_text: &str,
) -> LookupInvocationReport {
    LookupInvocationReport {
        owner: start.run.owner.clone(),
        ordinal: ordinal.into(),
        event_id: event_id.into(),
        assistant_text: assistant_text.into(),
        binding: None,
        status,
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
fn delivery_summary_requires_the_complete_saved_packet_and_ignores_outcome() {
    let (_temp, project, access, document) = setup();

    let failed = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "lookup-failed".into(),
            expected: document.head.clone(),
            instruction: "Find the promise.".into(),
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
        .expect("start failed lookup");
    let failed_dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: failed.run.owner.clone(),
        })
        .expect("begin failed lookup");
    let failed_claim = project
        .claim_lookup_invocation(failed.run.owner.clone(), "0".into())
        .expect("claim failed lookup");
    project
        .settle_lookup_invocation(report(
            &failed,
            &failed_claim.packet,
            "0",
            "failed-delivery-result",
            ProviderOutcomeStatus::Completed,
            "not valid lookup JSON",
        ))
        .expect("save malformed lookup response");
    let failed_view = project
        .read_discussion(access.clone(), document.head.document_id.clone())
        .expect("read failed lookup");
    let failed_summary = failed_view
        .runs
        .iter()
        .find(|run| run.id == failed.run.id)
        .and_then(|run| run.lookup.as_ref())
        .expect("failed lookup summary");
    assert_eq!(
        failed_view
            .runs
            .iter()
            .find(|run| run.id == failed.run.id)
            .unwrap()
            .packet_id,
        failed_dispatch.packet.receipt.packet_id
    );
    assert!(failed_summary.invocations[0].input_delivered);

    let stopped = project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: "lookup-stopped".into(),
            expected: document.head.clone(),
            instruction: "Find the promise again.".into(),
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
        .expect("start stopped lookup");
    project
        .begin_discussion_run(DiscussionBegin {
            owner: stopped.run.owner.clone(),
        })
        .expect("begin stopped lookup");
    let stopped_claim = project
        .claim_lookup_invocation(stopped.run.owner.clone(), "0".into())
        .expect("claim stopped lookup");
    project
        .stop_discussion(access.clone(), stopped.run.id.clone())
        .expect("request stop");
    project
        .settle_lookup_invocation(report(
            &stopped,
            &stopped_claim.packet,
            "0",
            "stopped-delivery-result",
            ProviderOutcomeStatus::Stopped,
            "",
        ))
        .expect("save stopped lookup");
    let stopped_view = project
        .read_discussion(access.clone(), "chapter-one".into())
        .expect("read stopped lookup");
    let stopped_summary = stopped_view
        .runs
        .iter()
        .find(|run| run.id == stopped.run.id)
        .and_then(|run| run.lookup.as_ref())
        .expect("stopped lookup summary");
    assert!(stopped_summary.invocations[0].input_delivered);

    let prepared = project
        .start_discussion(StartDiscussion {
            access,
            operation_id: "lookup-prepared".into(),
            expected: document.head.clone(),
            instruction: "Wait before sending.".into(),
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
        .expect("start prepared lookup");
    assert!(!prepared.run.lookup.unwrap().invocations[0].input_delivered);
}
