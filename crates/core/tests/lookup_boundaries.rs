use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::lookup::{LOOKUP_SCHEMA_VERSION, LookupAllowance};
use webnovel_core::context::packet::{MockContextBudget, serialized_input};
use webnovel_core::projects::discussion_lookup::{
    LookupAdvanceRequest, LookupHaltRequest, LookupInvocationReport,
};
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionRunStatus, DiscussionStart, FeedbackIntent, ProviderCleanup,
    ProviderOutcomeStatus, RunOwner, StartDiscussion,
};
use webnovel_core::projects::story_context::{SearchMode, SearchStory};
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("wns-lookup-boundary-{label}-{}", Uuid::new_v4()));
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
    text: &str,
) -> (
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
) {
    let project = ProjectSession::create(path, "Lookup boundary test").expect("create project");
    let access = project
        .attach("lookup-boundary-session".into())
        .expect("attach");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .expect("create chapter");
    (project, access, document)
}

fn start(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    operation_id: &str,
    lookup: LookupAllowance,
) -> DiscussionStart {
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
            lookup: Some(lookup),
        })
        .expect("start discussion")
}

fn begin(
    project: &ProjectSession,
    owner: &RunOwner,
) -> webnovel_core::projects::discussions::DiscussionDispatch {
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .expect("begin discussion")
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

fn final_json(text: &str) -> String {
    serde_json::to_string(&json!({
        "kind": "discussion",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "text": text,
    }))
    .expect("serialize lookup response")
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

#[test]
fn duplicate_claim_cannot_grant_one_invocation_twice() {
    let temp = TempDir::new("duplicate-claim");
    let (project, access, document) =
        setup_at(&temp.child("project"), "A jade pendant rests on the table.");
    let started = start(
        &project,
        &access,
        &document,
        "duplicate-claim",
        LookupAllowance::default(),
    );
    let owner = started.run.owner.clone();
    let _initial = begin(&project, &owner);
    project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("first claim");
    let error = project
        .claim_lookup_invocation(owner, "0".into())
        .expect_err("a claimed invocation cannot be granted twice");
    assert_eq!(error.code, "LookupInvocationAlreadyClaimed");
}

#[test]
fn same_event_id_with_changed_payload_is_refused() {
    let temp = TempDir::new("event-replay");
    let (project, access, document) =
        setup_at(&temp.child("project"), "A jade pendant rests on the table.");
    let started = start(
        &project,
        &access,
        &document,
        "event-replay",
        LookupAllowance::default(),
    );
    let owner = started.run.owner.clone();
    let initial = begin(&project, &owner);
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim");
    project
        .settle_lookup_invocation(completed_report(
            &owner,
            &claimed.packet,
            "0",
            "same-event",
            final_json("first answer"),
        ))
        .expect("first result");
    let error = project
        .settle_lookup_invocation(completed_report(
            &owner,
            &initial.packet,
            "0",
            "same-event",
            final_json("changed answer"),
        ))
        .expect_err("same event ID with changed content must be refused");
    assert_eq!(error.code, "LookupResultConflict");
}

#[test]
fn pretty_json_final_response_survives_reopen() {
    let temp = TempDir::new("pretty-reopen");
    let path = temp.child("project");
    let (project, access, document) = setup_at(&path, "A jade pendant rests on the table.");
    let started = start(
        &project,
        &access,
        &document,
        "pretty-reopen",
        LookupAllowance::default(),
    );
    let owner = started.run.owner.clone();
    let _initial = begin(&project, &owner);
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim");
    let pretty = serde_json::to_string_pretty(&json!({
        "kind": "discussion",
        "schemaVersion": LOOKUP_SCHEMA_VERSION,
        "text": "The pendant is still on the table.",
    }))
    .expect("serialize pretty lookup response");
    assert!(pretty.contains('\n'));
    project
        .settle_lookup_invocation(completed_report(
            &owner,
            &claimed.packet,
            "0",
            "pretty-final",
            pretty,
        ))
        .expect("settle pretty response");
    let backup = temp.child("pretty.wnsbackup");
    create_backup(&project, &backup).expect("backup pretty response");
    drop(access);
    drop(project);

    let recovered_path = temp.child("recovered");
    let reopened = recover_backup(&backup, &recovered_path, "Recovered pretty response")
        .expect("pretty response remains valid through backup recovery");
    let reopened_access = reopened
        .attach("pretty-reopen-session".into())
        .expect("reattach");
    let view = reopened
        .read_discussion(reopened_access, "chapter-one".into())
        .expect("read reopened discussion");
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Completed);
}

#[test]
fn completed_needs_context_followed_by_stop_survives_backup_recovery() {
    let temp = TempDir::new("needs-context-stop");
    let (project, access, document) =
        setup_at(&temp.child("project"), "A jade pendant rests on the table.");
    let started = start(
        &project,
        &access,
        &document,
        "needs-context-stop",
        LookupAllowance::default(),
    );
    let owner = started.run.owner.clone();
    let _initial = begin(&project, &owner);
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim");
    project
        .settle_lookup_invocation(completed_report(
            &owner,
            &claimed.packet,
            "0",
            "needs-context-stop-result",
            needs_context_json(),
        ))
        .expect("settle needs-context response");
    project
        .halt_lookup(LookupHaltRequest {
            owner: owner.clone(),
            reason: "author stopped the lookup".into(),
        })
        .expect("stop lookup");

    let backup = temp.child("needs-context-stop.wnsbackup");
    create_backup(&project, &backup).expect("backup stopped needs-context response");
    let recovered = recover_backup(
        &backup,
        &temp.child("recovered"),
        "Recovered needs-context stop",
    )
    .expect("recover stopped needs-context response");
    let recovered_access = recovered
        .attach("recovered-session".into())
        .expect("attach");
    let view = recovered
        .read_discussion(recovered_access, "chapter-one".into())
        .expect("read recovered discussion");
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Interrupted);
}

#[test]
fn needs_context_with_prepared_child_followed_by_stop_survives_backup_recovery() {
    let temp = TempDir::new("prepared-child-stop");
    let (project, access, document) =
        setup_at(&temp.child("project"), "A jade pendant rests on the table.");
    let started = start(
        &project,
        &access,
        &document,
        "prepared-child-stop",
        LookupAllowance::default(),
    );
    let owner = started.run.owner.clone();
    let _initial = begin(&project, &owner);
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim initial lookup");
    project
        .settle_lookup_invocation(completed_report(
            &owner,
            &claimed.packet,
            "0",
            "prepared-child-stop-result",
            needs_context_json(),
        ))
        .expect("settle needs-context response");
    let prepared = project
        .advance_lookup(LookupAdvanceRequest {
            owner: owner.clone(),
            completed_ordinal: "0".into(),
        })
        .expect("prepare child lookup");
    assert!(matches!(
        prepared,
        webnovel_core::projects::discussion_lookup::LookupAdvance::Prepared { .. }
    ));
    project
        .halt_lookup(LookupHaltRequest {
            owner: owner.clone(),
            reason: "author stopped after preparing the next lookup".into(),
        })
        .expect("stop lookup");

    let backup = temp.child("prepared-child-stop.wnsbackup");
    create_backup(&project, &backup).expect("backup stopped prepared child");
    let recovered = recover_backup(
        &backup,
        &temp.child("recovered"),
        "Recovered prepared child stop",
    )
    .expect("recover stopped prepared child");
    let recovered_access = recovered
        .attach("recovered-session".into())
        .expect("attach");
    let view = recovered
        .read_discussion(recovered_access, "chapter-one".into())
        .expect("read recovered discussion");
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Interrupted);
}

#[test]
fn malformed_completed_envelope_is_a_failed_result_that_survives_backup_recovery() {
    let temp = TempDir::new("malformed-envelope");
    let (project, access, document) =
        setup_at(&temp.child("project"), "A jade pendant rests on the table.");
    let started = start(
        &project,
        &access,
        &document,
        "malformed-envelope",
        LookupAllowance::default(),
    );
    let owner = started.run.owner.clone();
    let _initial = begin(&project, &owner);
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("claim");
    let failed = project
        .settle_lookup_invocation(completed_report(
            &owner,
            &claimed.packet,
            "0",
            "malformed-envelope-result",
            "{not valid lookup JSON".into(),
        ))
        .expect("malformed completed output is recorded as failed");
    assert_eq!(failed.status, DiscussionRunStatus::Failed);

    let backup = temp.child("malformed-envelope.wnsbackup");
    create_backup(&project, &backup).expect("backup malformed result");
    let recovered = recover_backup(
        &backup,
        &temp.child("recovered"),
        "Recovered malformed envelope",
    )
    .expect("recover malformed result");
    let recovered_access = recovered
        .attach("recovered-session".into())
        .expect("attach");
    let view = recovered
        .read_discussion(recovered_access, "chapter-one".into())
        .expect("read recovered discussion");
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Failed);
}

#[test]
fn unicode_search_preserves_exact_utf16_spans_for_expanding_case_and_emoji() {
    let temp = TempDir::new("unicode-search");
    let (project, access, document) = setup_at(&temp.child("project"), "İ 🔑");
    let started = start(
        &project,
        &access,
        &document,
        "unicode-search",
        LookupAllowance::default(),
    );
    let owner = started.run.owner.clone();
    let begun = begin(&project, &owner);
    let snapshot_id = begun.packet.receipt.snapshot_id.clone();

    let dotted_i = project
        .search_story(SearchStory {
            access: access.clone(),
            snapshot_id: snapshot_id.clone(),
            query: "İ".into(),
            mode: SearchMode::Literal,
            limit: 10,
        })
        .expect("search dotted I");
    assert_eq!(dotted_i.hits.len(), 1);
    assert_eq!(
        (dotted_i.hits[0].start_utf16, dotted_i.hits[0].end_utf16),
        (0, 1)
    );

    let key = project
        .search_story(SearchStory {
            access,
            snapshot_id,
            query: "🔑".into(),
            mode: SearchMode::Literal,
            limit: 10,
        })
        .expect("search emoji");
    assert_eq!(key.hits.len(), 1);
    assert_eq!((key.hits[0].start_utf16, key.hits[0].end_utf16), (2, 4));
}

#[test]
fn aggregate_custom_allowance_refuses_next_dispatch_before_provider_call() {
    let temp = TempDir::new("aggregate-allowance");
    let (project, access, document) =
        setup_at(&temp.child("project"), "A jade pendant rests on the table.");
    let allowance = LookupAllowance::new(1, "7000", "196608").expect("custom allowance");
    let started = start(
        &project,
        &access,
        &document,
        "aggregate-allowance",
        allowance,
    );
    let owner = started.run.owner.clone();
    let _initial = begin(&project, &owner);
    let claimed = project
        .claim_lookup_invocation(owner.clone(), "0".into())
        .expect("initial call fits the custom allowance");
    project
        .settle_lookup_invocation(completed_report(
            &owner,
            &claimed.packet,
            "0",
            "allowance-needs-context",
            needs_context_json(),
        ))
        .expect("settle initial lookup");
    let prepared = project
        .advance_lookup(LookupAdvanceRequest {
            owner: owner.clone(),
            completed_ordinal: "0".into(),
        })
        .expect("prepare child lookup");
    let child = match prepared {
        webnovel_core::projects::discussion_lookup::LookupAdvance::Prepared { dispatch } => {
            dispatch
        }
        webnovel_core::projects::discussion_lookup::LookupAdvance::Finished { .. } => {
            panic!("custom allowance should prepare, then reject, the next dispatch")
        }
    };
    let error = project
        .claim_lookup_invocation(owner, child.ordinal)
        .expect_err("aggregate allowance must reject before the second provider call");
    assert_eq!(error.code, "LookupAllowanceExceeded");
}

#[test]
fn story_edit_between_prepare_and_claim_stales_lookup_before_dispatch() {
    let temp = TempDir::new("edit-before-claim");
    let (project, access, document) =
        setup_at(&temp.child("project"), "A jade pendant rests on the table.");
    let started = start(
        &project,
        &access,
        &document,
        "edit-before-claim",
        LookupAllowance::default(),
    );
    let owner = started.run.owner.clone();
    let _initial = begin(&project, &owner);
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-before-claim-save".into(),
            expected: document.head,
            local_generation: "1".into(),
            body: body("The pendant is gone from the table."),
            cause: SaveCause::Typing,
        })
        .expect("edit story after packet preparation");
    let error = match project.claim_lookup_invocation(owner.clone(), "0".into()) {
        Err(error) => error,
        Ok(_) => panic!("a lookup packet must stale before provider dispatch"),
    };
    assert_eq!(error.code, "ContextChanged");
    let view = project
        .read_discussion(access, "chapter-one".into())
        .expect("read stale run");
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Failed);
    assert_eq!(view.runs[0].stop_reason.as_deref(), Some("context_stale"));
}
