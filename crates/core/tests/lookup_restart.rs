use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::lookup::{LOOKUP_SCHEMA_VERSION, LookupAllowance};
use webnovel_core::context::packet::{MockContextBudget, serialized_input};
use webnovel_core::projects::discussion_lookup::{
    LookupAdvance, LookupAdvanceRequest, LookupInvocationReport, LookupInvocationState,
};
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionRunStatus, FeedbackIntent, ProviderCleanup, ProviderOutcomeStatus,
    RunOwner, StartDiscussion,
};
use webnovel_core::projects::{CreateDocument, ProjectAccess, ProjectSession};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("wns-lookup-restart-{label}-{}", Uuid::new_v4()));
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

fn body(text: &str) -> serde_json::Value {
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

fn setup_at(
    path: &Path,
) -> (
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let project = ProjectSession::create(path, "Lookup restart test").expect("create project");
    let access = project
        .documents().attach("lookup-restart-session".into())
        .expect("attach");
    let document = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("A jade pendant rests on the table."),
        })
        .expect("create chapter");
    (project, access, document)
}

fn start(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    operation_id: &str,
) -> webnovel_core::projects::discussions::DiscussionStart {
    project
        .start_discussion(StartDiscussion {
            access: access.clone(),
            operation_id: operation_id.into(),
            expected: document.head.clone(),
            instruction: "Find an old story detail.".into(),
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
        .expect("start discussion")
}

fn begin(project: &ProjectSession, owner: &RunOwner) {
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .expect("begin discussion");
}

fn needs_context_json() -> String {
    serde_json::to_string(&json!({
        "kind": "needsContext",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "reads": [{
            "kind": "search",
            "id": "find-detail",
            "query": "pendant",
            "mode": "literal",
            "limit": 5,
        }],
    }))
    .expect("serialize needs-context response")
}

fn completed_report(
    owner: &RunOwner,
    packet: &webnovel_core::context::packet::CompiledPacket,
    ordinal: &str,
    event_id: &str,
    assistant_text: String,
) -> LookupInvocationReport {
    LookupInvocationReport {
        owner: owner.clone(),
        ordinal: ordinal.into(),
        event_id: event_id.into(),
        assistant_text,
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

fn assert_interrupted(view: &webnovel_core::projects::discussions::DiscussionView) {
    assert_eq!(view.runs.len(), 1);
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Interrupted);
}

#[test]
fn reopening_claimed_lookup_marks_it_unknown_and_blocks_dispatch_replay() {
    let temp = TempDir::new("claimed");
    let path = temp.child("project");
    let (project, access, document) = setup_at(&path);
    let started = start(&project, &access, &document, "claimed-restart");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim initial lookup");

    // Dropping the session leaves the claimed provider call unresolved, just
    // as a process crash would. ProjectSession::open performs recovery before
    // accepting a new writer session.
    drop(access);
    drop(project);

    let reopened = ProjectSession::open(&path).expect("reopen project");
    let reopened_access = reopened.documents().attach("reopened-claimed".into()).expect("attach");
    let view = reopened
        .read_discussion(reopened_access, "chapter-one".into())
        .expect("read recovered discussion");
    assert_interrupted(&view);
    let lookup = view.runs[0].lookup.as_ref().expect("lookup summary");
    assert_eq!(lookup.invocations.len(), 1);
    assert_eq!(lookup.invocations[0].state, LookupInvocationState::Unknown);
    assert!(lookup.invocations[0].response.is_none());
    let replay = reopened
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect_err("an interrupted lookup must never be redispatched");
    assert_eq!(replay.code, "RunSealed");

    let backup = temp.child("claimed-restart.wnsbackup");
    create_backup(&reopened, &backup).expect("backup interrupted lookup history");
    drop(reopened);
    let recovered = recover_backup(
        &backup,
        &temp.child("recovered"),
        "Recovered claimed lookup",
    )
    .expect("recover interrupted lookup history");
    let recovered_access = recovered
        .documents().attach("recovered-claimed".into())
        .expect("attach recovered");
    let recovered_view = recovered
        .read_discussion(recovered_access, "chapter-one".into())
        .expect("read recovered lookup history");
    assert_interrupted(&recovered_view);
    assert_eq!(
        recovered_view.runs[0].lookup.as_ref().unwrap().invocations[0].state,
        LookupInvocationState::Unknown
    );
}

#[test]
fn reopening_prepared_lookup_child_stops_child_and_keeps_predecessor_result() {
    let temp = TempDir::new("prepared-child");
    let path = temp.child("project");
    let (project, access, document) = setup_at(&path);
    let started = start(&project, &access, &document, "prepared-child-restart");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim initial lookup");
    project
        .settle_lookup_invocation(completed_report(
            &owner,
            &claimed.packet,
            "0",
            "needs-context-before-crash",
            needs_context_json(),
        ))
        .expect("settle needs-context response");
    let child = match project
        .advance_lookup(LookupAdvanceRequest {
            owner: owner.clone(),
            completed_ordinal: "0".into(),
        })
        .expect("prepare child lookup")
    {
        LookupAdvance::Prepared { dispatch } => dispatch,
        LookupAdvance::Finished { .. } => panic!("needs-context response should prepare a child"),
    };
    assert_eq!(child.ordinal, "1");

    drop(access);
    drop(project);

    let reopened = ProjectSession::open(&path).expect("reopen project");
    let reopened_access = reopened.documents().attach("reopened-child".into()).expect("attach");
    let view = reopened
        .read_discussion(reopened_access, "chapter-one".into())
        .expect("read recovered discussion");
    assert_interrupted(&view);
    let invocations = &view.runs[0]
        .lookup
        .as_ref()
        .expect("lookup summary")
        .invocations;
    assert_eq!(invocations.len(), 2);
    assert_eq!(invocations[0].state, LookupInvocationState::NeedsContext);
    assert!(
        invocations[0].response.is_some(),
        "predecessor response must remain durable"
    );
    assert_eq!(invocations[1].state, LookupInvocationState::Stopped);
    assert!(invocations[1].response.is_none());
    let replay = reopened
        .claim_lookup_invocation(owner.clone(), "1".into())
        .expect_err("a stopped prepared child must never be redispatched");
    assert_eq!(replay.code, "RunSealed");

    let backup = temp.child("prepared-child-restart.wnsbackup");
    create_backup(&reopened, &backup).expect("backup interrupted child history");
    drop(reopened);
    let recovered = recover_backup(
        &backup,
        &temp.child("recovered"),
        "Recovered prepared child",
    )
    .expect("recover interrupted child history");
    let recovered_access = recovered
        .documents().attach("recovered-child".into())
        .expect("attach recovered");
    let recovered_view = recovered
        .read_discussion(recovered_access, "chapter-one".into())
        .expect("read recovered child history");
    assert_interrupted(&recovered_view);
    let recovered_invocations = &recovered_view.runs[0]
        .lookup
        .as_ref()
        .expect("recovered lookup summary")
        .invocations;
    assert_eq!(
        recovered_invocations[0].state,
        LookupInvocationState::NeedsContext
    );
    assert!(recovered_invocations[0].response.is_some());
    assert_eq!(
        recovered_invocations[1].state,
        LookupInvocationState::Stopped
    );
}
