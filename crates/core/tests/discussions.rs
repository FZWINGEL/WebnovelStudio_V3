use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::documents::{Endpoint, ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionFail, DiscussionFinish, DiscussionMessageRole,
    DiscussionOutputAppend, DiscussionRunStatus, DiscussionScopeInput, FeedbackIntent, RunOwner,
    SaveDiscussionDraft, StartDiscussion,
};
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("wns-discussions-{label}-{}", Uuid::new_v4()));
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
    let project = ProjectSession::create(root, "Discussion test").expect("create project");
    let access = project
        .attach("discussion-session".into())
        .expect("attach project");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-document".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("prefix selected suffix"),
        })
        .expect("create document");
    (project, access, document)
}

fn budget() -> MockContextBudget {
    MockContextBudget::new("100000", "100", "100")
}

fn start_request(
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    operation_id: &str,
    instruction: &str,
    scope: Option<DiscussionScopeInput>,
    pinned_document_ids: Vec<String>,
) -> StartDiscussion {
    StartDiscussion {
        access: access.clone(),
        operation_id: operation_id.into(),
        expected: document.head.clone(),
        instruction: instruction.into(),
        intent: Default::default(),
        scope,
        pinned_document_ids,
        budget: budget(),
        previous_run_id: None,
    }
}

fn start(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    operation_id: &str,
) -> webnovel_core::projects::discussions::DiscussionStart {
    project
        .start_discussion(start_request(
            access,
            document,
            operation_id,
            "What should the next beat emphasize?",
            None,
            Vec::new(),
        ))
        .expect("start discussion")
}

fn begin(
    project: &ProjectSession,
    owner: &RunOwner,
) -> webnovel_core::projects::discussions::DiscussionRun {
    project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .expect("begin discussion")
        .run
}

fn append(
    project: &ProjectSession,
    owner: &RunOwner,
    expected_sequence: &str,
    event_id: &str,
    chunk: &str,
) -> webnovel_core::projects::discussions::DiscussionRun {
    project
        .append_discussion_output(DiscussionOutputAppend {
            owner: owner.clone(),
            expected_sequence: expected_sequence.into(),
            event_id: event_id.into(),
            chunk: chunk.into(),
        })
        .expect("append discussion output")
}

fn finish(
    project: &ProjectSession,
    owner: &RunOwner,
    expected_sequence: &str,
    event_id: &str,
    assistant_text: &str,
) -> webnovel_core::projects::discussions::DiscussionRun {
    project
        .finish_discussion(DiscussionFinish {
            owner: owner.clone(),
            expected_sequence: expected_sequence.into(),
            event_id: event_id.into(),
            assistant_text: assistant_text.into(),
        })
        .expect("finish discussion")
}

fn discussion(
    project: &ProjectSession,
    access: &ProjectAccess,
) -> webnovel_core::projects::discussions::DiscussionView {
    project
        .read_discussion(access.clone(), "chapter-one".into())
        .expect("read discussion")
}

fn completed_turn(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    id: &str,
    text: &str,
) -> webnovel_core::projects::discussions::DiscussionStart {
    let result = start(project, access, document, id);
    begin(project, &result.run.owner);
    project
        .mark_discussion_delivered(result.run.owner.clone())
        .unwrap();
    append(
        project,
        &result.run.owner,
        "0",
        &format!("{id}-chunk"),
        text,
    );
    finish(
        project,
        &result.run.owner,
        "1",
        &format!("{id}-finish"),
        text,
    );
    result
}

#[test]
fn recent_complete_turns_are_exact_capped_and_frozen_across_retry_and_restart() {
    let temp = TempDir::new("recent-conversation");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    for index in 0..6 {
        completed_turn(
            &project,
            &access,
            &document,
            &format!("turn-{index}"),
            &format!("Reply {index}: an unadopted idea."),
        );
    }
    let partial = start(&project, &access, &document, "partial");
    begin(&project, &partial.run.owner);
    append(
        &project,
        &partial.run.owner,
        "0",
        "partial-chunk",
        "Incomplete reply.",
    );
    project
        .stop_discussion(access.clone(), partial.run.id)
        .unwrap();
    let source_epoch = project.context_source_epoch().unwrap();
    let result = completed_turn(
        &project,
        &access,
        &document,
        "follow-up",
        "A later completed reply.",
    );
    let frozen = project
        .story_snapshot(access.clone(), result.packet.receipt.snapshot_id.clone())
        .unwrap();
    let conversation = frozen.conversation.unwrap();
    assert_eq!(conversation.turns.len(), 4);
    assert_eq!(conversation.omitted_turns, 2);
    assert_eq!(
        conversation.turns[0].assistant.content,
        "Reply 5: an unadopted idea."
    );
    assert_eq!(
        conversation.turns[3].assistant.content,
        "Reply 2: an unadopted idea."
    );
    let envelope: Value = serde_json::from_str(&result.packet.messages[1].content).unwrap();
    assert_eq!(
        envelope["recentDiscussion"][0]["assistant"]["content"],
        "Reply 2: an unadopted idea."
    );
    assert_eq!(result.packet.receipt.conversation_message_ids.len(), 8);
    assert_eq!(result.packet.receipt.omitted_discussion_turns, 2);
    assert_eq!(
        start(&project, &access, &document, "follow-up").packet,
        result.packet
    );
    assert_eq!(project.context_source_epoch().unwrap(), source_epoch);
    assert_eq!(
        project
            .document(access, document.head.document_id.clone())
            .unwrap()
            .body,
        document.body
    );
    drop(project);
    let reopened = ProjectSession::open(path).unwrap();
    let access = reopened.attach("reopened-discussion".into()).unwrap();
    assert_eq!(
        reopened
            .prepared_context(access, result.packet.receipt.packet_id.clone())
            .unwrap(),
        result.packet
    );
}

#[test]
fn conversation_is_excluded_across_documents_policy_and_recovery() {
    let temp = TempDir::new("conversation-boundaries");
    let (project, access, document) = setup_project(&temp.child("project"));
    completed_turn(
        &project,
        &access,
        &document,
        "source",
        "A private earlier discussion.",
    );
    let second = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "other-doc".into(),
            document_id: "other-doc".into(),
            title: "Other".into(),
            kind: "chapter".into(),
            body: body("Other chapter."),
        })
        .unwrap();
    assert!(
        start(&project, &access, &second, "other-discussion")
            .packet
            .receipt
            .conversation_message_ids
            .is_empty()
    );
    let old = start(&project, &access, &document, "uses-history");
    assert_eq!(old.packet.receipt.conversation_message_ids.len(), 2);
    project
        .revoke_story_context(access.clone(), "0".into())
        .unwrap();
    let current = start(&project, &access, &document, "after-revocation");
    assert!(current.packet.receipt.conversation_message_ids.is_empty());
    assert_eq!(
        project
            .prepared_context(access, old.packet.receipt.packet_id)
            .unwrap_err()
            .code,
        "ContextPolicyChanged"
    );
    let archive = temp.child("history.wnsbackup");
    create_backup(&project, &archive).unwrap();
    let copy = recover_backup(&archive, &temp.child("copy"), "Recovered history").unwrap();
    let access = copy.attach("copy-session".into()).unwrap();
    let doc = copy
        .document(access.clone(), document.head.document_id)
        .unwrap();
    assert!(
        start(&copy, &access, &doc, "copy-request")
            .packet
            .receipt
            .conversation_message_ids
            .is_empty()
    );
    assert!(
        discussion(&copy, &access)
            .messages
            .iter()
            .any(|m| m.content == "A private earlier discussion.")
    );
}

#[test]
fn oversized_recent_reply_is_retained_but_omitted_as_a_whole_turn() {
    let temp = TempDir::new("conversation-limit");
    let (project, access, document) = setup_project(&temp.child("project"));
    completed_turn(&project, &access, &document, "small", "Small older reply.");
    let large = "A long unadopted alternative. ".repeat(800);
    completed_turn(&project, &access, &document, "large", &large);
    let current = start(&project, &access, &document, "after-large");
    assert!(current.packet.receipt.conversation_message_ids.is_empty());
    assert_eq!(current.packet.receipt.omitted_discussion_turns, 2);
    assert!(
        discussion(&project, &access)
            .messages
            .iter()
            .any(|m| m.content == large)
    );
    let envelope: Value = serde_json::from_str(&current.packet.messages[1].content).unwrap();
    assert!(envelope.get("recentDiscussion").is_none());
    assert_eq!(envelope["omittedDiscussionTurns"], 2);
}

#[test]
fn frozen_conversation_rejects_changed_retained_message_text() {
    let temp = TempDir::new("conversation-integrity");
    let (project, access, document) = setup_project(&temp.child("project"));
    completed_turn(
        &project,
        &access,
        &document,
        "source-turn",
        "The exact original reply.",
    );
    let result = start(&project, &access, &document, "frozen-turn");
    let source_id = result.packet.receipt.conversation_message_ids[1].clone();
    let db = Connection::open(project.path.join("project.sqlite3")).unwrap();
    db.execute_batch("DROP TRIGGER discussion_messages_no_update;")
        .unwrap();
    db.execute(
        "UPDATE discussion_messages SET content='Changed reply' WHERE id=?",
        [source_id],
    )
    .unwrap();
    assert_eq!(
        project
            .prepared_context(access, result.packet.receipt.packet_id)
            .unwrap_err()
            .code,
        "InvalidConversationContext"
    );
    assert_eq!(
        create_backup(&project, &temp.child("invalid.wnsbackup"))
            .unwrap_err()
            .code,
        "InvalidBackup"
    );
}

#[test]
fn an_empty_completed_answer_does_not_block_later_discussion() {
    let temp = TempDir::new("conversation-empty-answer");
    let (project, access, document) = setup_project(&temp.child("project"));
    completed_turn(
        &project,
        &access,
        &document,
        "useful",
        "A useful older reply.",
    );
    completed_turn(&project, &access, &document, "blank", " \n\t");
    let current = start(&project, &access, &document, "after-blank");
    assert_eq!(current.packet.receipt.conversation_message_ids.len(), 2);
    assert_eq!(current.packet.receipt.omitted_discussion_turns, 1);
}

fn save_draft(
    project: &ProjectSession,
    access: &ProjectAccess,
    expected_version: &str,
    operation_id: &str,
    text: &str,
) -> webnovel_core::projects::discussions::DiscussionDraft {
    project
        .save_discussion_draft(SaveDiscussionDraft {
            access: access.clone(),
            operation_id: operation_id.into(),
            document_id: "chapter-one".into(),
            expected_version: expected_version.into(),
            text: text.into(),
            intent: Default::default(),
            scope: None,
            pinned_document_ids: Vec::new(),
            previous_run_id: None,
        })
        .expect("save discussion draft")
}

fn counts(project: &ProjectSession) -> [i64; 8] {
    let connection = Connection::open(project.path.join("project.sqlite3")).expect("open db");
    [
        "revisions",
        "story_snapshots",
        "snapshot_sources",
        "context_packets",
        "discussion_threads",
        "discussion_runs",
        "discussion_messages",
        "discussion_output_events",
    ]
    .map(|table| {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count discussion rows")
    })
}

fn scope_input(document: &webnovel_core::projects::DocumentRecord) -> DiscussionScopeInput {
    let grant = capture_scope(
        &document.body,
        ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "p1".into(),
                utf16_offset: 7,
            }),
            end: Some(Endpoint {
                block_id: "p1".into(),
                utf16_offset: 15,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .expect("capture selected passage");
    DiscussionScopeInput {
        kind: grant.kind,
        start: grant.start,
        end: grant.end,
        quote: grant.quote,
        source_body_hash: grant.source_hash,
    }
}

fn adopt_guidance(
    project: &ProjectSession,
    access: &ProjectAccess,
    id: &str,
    scope: webnovel_core::context::guidance::GuidanceScope,
    text: &str,
) -> webnovel_core::context::guidance::GuidanceVersion {
    use webnovel_core::context::guidance::GuidanceScope;
    project
        .save_guidance(webnovel_core::projects::guidance::SaveGuidance {
            access: access.clone(),
            operation_id: format!("adopt-{id}"),
            guidance_id: id.into(),
            expected_version: "0".into(),
            text: text.into(),
            scope,
            document_id: (scope != GuidanceScope::Project).then(|| "chapter-one".into()),
            active: true,
            origin_message_id: None,
        })
        .unwrap()
}

#[test]
fn guidance_versions_remain_exact_after_edits_and_restart_while_old_requests_become_stale() {
    use webnovel_core::context::guidance::GuidanceScope;
    let temp = TempDir::new("guidance-history");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let initial = adopt_guidance(
        &project,
        &access,
        "voice",
        GuidanceScope::Project,
        "Keep the ending. Avoid sarcasm.",
    );
    let started = start(&project, &access, &document, "guided-run");
    let original_packet = project
        .prepared_context(access.clone(), started.run.packet_id.clone())
        .unwrap();
    let envelope: Value = serde_json::from_str(&original_packet.messages[1].content).unwrap();
    assert_eq!(
        envelope["authorGuidance"][0]["version"]["text"],
        initial.text
    );
    let updated = project
        .save_guidance(webnovel_core::projects::guidance::SaveGuidance {
            access: access.clone(),
            operation_id: "edit-voice".into(),
            guidance_id: initial.guidance_id.clone(),
            expected_version: "1".into(),
            text: "Keep the ending. A little dry humor is fine.".into(),
            scope: GuidanceScope::Project,
            document_id: None,
            active: true,
            origin_message_id: None,
        })
        .unwrap();
    assert_eq!(updated.version, "2");
    assert!(
        !project
            .prepared_context_is_current(access.clone(), started.run.packet_id.clone())
            .unwrap()
    );
    assert_eq!(
        project
            .prepared_context(access.clone(), started.run.packet_id.clone())
            .unwrap(),
        original_packet
    );
    assert_eq!(
        project
            .begin_discussion_run(DiscussionBegin {
                owner: started.run.owner.clone()
            })
            .unwrap_err()
            .code,
        "ContextChanged"
    );
    drop(project);
    let reopened = ProjectSession::open(&path).unwrap();
    let fresh = reopened.attach("guidance-reopen".into()).unwrap();
    assert_eq!(
        reopened
            .guidance(fresh.clone(), "chapter-one".into())
            .unwrap()[0],
        updated
    );
    assert_eq!(
        reopened
            .prepared_context(fresh, started.run.packet_id)
            .unwrap(),
        original_packet
    );
}

#[test]
fn next_request_guidance_is_consumed_once_only_after_a_successful_atomic_start() {
    use webnovel_core::context::guidance::GuidanceScope;
    let temp = TempDir::new("guidance-once");
    let (project, access, document) = setup_project(&temp.child("project"));
    adopt_guidance(
        &project,
        &access,
        "one-request",
        GuidanceScope::Request,
        "Discuss the pendant for this request.",
    );
    let before = counts(&project);
    let mut too_small = start_request(
        &access,
        &document,
        "too-small",
        "Discuss this passage.",
        None,
        vec![],
    );
    too_small.budget = MockContextBudget::new("1", "0", "0");
    assert!(project.start_discussion(too_small).is_err());
    assert_eq!(counts(&project), before);
    assert_eq!(
        project
            .guidance(access.clone(), "chapter-one".into())
            .unwrap()
            .len(),
        1
    );
    let injection = Connection::open(project.path.join("project.sqlite3")).unwrap();
    injection.execute_batch("CREATE TRIGGER fail_guided_message BEFORE INSERT ON discussion_messages BEGIN SELECT RAISE(ABORT,'injected guided message failure'); END;").unwrap();
    assert!(
        project
            .start_discussion(start_request(
                &access,
                &document,
                "guided-message-failure",
                "Keep the instruction.",
                None,
                vec![]
            ))
            .is_err()
    );
    assert_eq!(counts(&project), before);
    assert_eq!(
        project
            .guidance(access.clone(), "chapter-one".into())
            .unwrap()
            .len(),
        1
    );
    injection
        .execute_batch("DROP TRIGGER fail_guided_message;")
        .unwrap();
    let first = start(&project, &access, &document, "guidance-first");
    assert_eq!(first.packet.receipt.guidance_handles.len(), 1);
    assert!(
        project
            .guidance(access.clone(), "chapter-one".into())
            .unwrap()
            .is_empty()
    );
    let epoch = project.context_source_epoch().unwrap();
    let retry = start(&project, &access, &document, "guidance-first");
    assert_eq!(retry.packet, first.packet);
    assert_eq!(project.context_source_epoch().unwrap(), epoch);
    let next = start(&project, &access, &document, "guidance-second");
    assert!(next.packet.receipt.guidance_handles.is_empty());
    assert_eq!(
        project.document(access, "chapter-one".into()).unwrap().body,
        document.body
    );
}

#[test]
fn guidance_scope_is_local_and_author_room_instructions_do_not_enter_restricted_writing() {
    use webnovel_core::context::guidance::GuidanceScope;
    use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
    use webnovel_core::projects::story_context::FreezeStory;
    let temp = TempDir::new("guidance-policy");
    let (project, access, document) = setup_project(&temp.child("project"));
    let other = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-other-chapter".into(),
            document_id: "chapter-two".into(),
            title: "Other chapter".into(),
            kind: "chapter".into(),
            body: body("Another scene."),
        })
        .unwrap();
    adopt_guidance(
        &project,
        &access,
        "local",
        GuidanceScope::Document,
        "The mentor is the hidden antagonist.",
    );
    adopt_guidance(
        &project,
        &access,
        "project",
        GuidanceScope::Project,
        "Keep the voice understated.",
    );
    let packet = start(&project, &access, &other, "other-guided-run").packet;
    assert_eq!(packet.receipt.guidance_handles.len(), 1);
    assert!(!packet.messages[1].content.contains("hidden antagonist"));
    let restricted = project
        .freeze_story(FreezeStory {
            access,
            operation_id: "restricted-guidance".into(),
            expected: document.head,
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: InformationPolicy {
                version: "0".into(),
                audience: Audience::RestrictedWriting,
                reader_frontier: Some("100".into()),
                character_id: None,
                character_grants: vec![],
                allow_alternatives: false,
                allow_historical: false,
            },
        })
        .unwrap();
    assert!(restricted.guidance.is_empty());
    assert!(
        !serde_json::to_string(&restricted)
            .unwrap()
            .contains("hidden antagonist")
    );
}

#[test]
fn recovered_copy_keeps_author_guidance_but_cannot_reuse_original_packet_authority() {
    use webnovel_core::context::guidance::GuidanceScope;
    let temp = TempDir::new("guidance-recovery");
    let (source, access, document) = setup_project(&temp.child("source"));
    adopt_guidance(
        &source,
        &access,
        "ending",
        GuidanceScope::Project,
        "Preserve the final reunion.",
    );
    let original = start(&source, &access, &document, "original-guidance-run");
    let backup = temp.child("guidance.wnsbackup");
    create_backup(&source, &backup).unwrap();
    let copy = recover_backup(&backup, &temp.child("copy"), "Recovered guidance").unwrap();
    let copy_access = copy.attach("copy-guidance".into()).unwrap();
    assert_eq!(
        copy.guidance(copy_access.clone(), "chapter-one".into())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        copy.prepared_context(copy_access.clone(), original.run.packet_id)
            .unwrap_err()
            .code,
        "ContextProjectMismatch"
    );
    let copy_document = copy
        .document(copy_access.clone(), "chapter-one".into())
        .unwrap();
    let copied = start(&copy, &copy_access, &copy_document, "copy-guidance-run");
    let frozen = copy
        .story_snapshot(copy_access.clone(), copied.packet.receipt.snapshot_id)
        .unwrap();
    assert_eq!(frozen.guidance[0].project_id, copy_access.project_id);
    assert_ne!(frozen.guidance[0].project_id, access.project_id);
    assert_eq!(
        frozen.guidance[0].version.text,
        "Preserve the final reunion."
    );
}

#[test]
fn start_is_atomic_and_idempotent_with_payload_binding() {
    let temp = TempDir::new("start");
    let (project, access, document) = setup_project(&temp.child("project"));
    let first = start(&project, &access, &document, "start-once");
    let retry = project
        .start_discussion(start_request(
            &access,
            &document,
            "start-once",
            "What should the next beat emphasize?",
            None,
            Vec::new(),
        ))
        .expect("retry same discussion");
    assert_eq!(retry.thread_id, first.thread_id);
    assert_eq!(retry.run.id, first.run.id);
    assert_eq!(retry.user_message.id, first.user_message.id);
    assert_eq!(retry.packet, first.packet);
    assert_eq!(counts(&project)[4..7], [1, 1, 1]);

    let mut changed = start_request(
        &access,
        &document,
        "start-once",
        "A changed instruction must not reuse the run.",
        None,
        Vec::new(),
    );
    changed.budget = MockContextBudget::new("90000", "100", "100");
    let error = project
        .start_discussion(changed)
        .expect_err("changed payload must be rejected");
    assert_eq!(error.code, "OperationIdReusedWithDifferentPayload");
    assert_eq!(counts(&project)[4..7], [1, 1, 1]);
}

#[test]
fn start_failure_at_run_or_message_insert_rolls_back_every_side_effect() {
    for (label, trigger) in [
        (
            "run",
            "CREATE TRIGGER fail_discussion_run BEFORE INSERT ON discussion_runs
             BEGIN SELECT RAISE(ABORT,'injected run failure'); END;",
        ),
        (
            "message",
            "CREATE TRIGGER fail_discussion_message BEFORE INSERT ON discussion_messages
             BEGIN SELECT RAISE(ABORT,'injected message failure'); END;",
        ),
    ] {
        let temp = TempDir::new(label);
        let (project, access, document) = setup_project(&temp.child("project"));
        let before = counts(&project);
        Connection::open(project.path.join("project.sqlite3"))
            .expect("open db for fault injection")
            .execute_batch(trigger)
            .expect("install discussion fault");
        let error = project
            .start_discussion(start_request(
                &access,
                &document,
                &format!("atomic-{label}"),
                "This request must roll back.",
                None,
                Vec::new(),
            ))
            .expect_err("injected start failure must be reported");
        assert_eq!(error.code, "PersistenceUnavailable");
        assert_eq!(counts(&project), before);
        let trigger_name = if label == "run" {
            "fail_discussion_run"
        } else {
            "fail_discussion_message"
        };
        Connection::open(project.path.join("project.sqlite3"))
            .expect("reopen db")
            .execute_batch(&format!("DROP TRIGGER {trigger_name};"))
            .expect("remove discussion fault");
    }
}

#[test]
fn exact_scope_quote_hash_and_target_head_are_required() {
    let temp = TempDir::new("scope");
    let (project, access, document) = setup_project(&temp.child("project"));
    let scope = scope_input(&document);
    let valid = project
        .start_discussion(start_request(
            &access,
            &document,
            "scope-valid",
            "Discuss this sentence.",
            Some(scope.clone()),
            Vec::new(),
        ))
        .expect("exact selected passage is accepted");
    assert_eq!(valid.user_message.scope.as_ref().unwrap().quote, "selected");

    let mut wrong_quote = scope.clone();
    wrong_quote.quote = "not selected".into();
    let error = project
        .start_discussion(start_request(
            &access,
            &document,
            "scope-wrong-quote",
            "Discuss this sentence.",
            Some(wrong_quote),
            Vec::new(),
        ))
        .expect_err("wrong quote must be rejected");
    assert_eq!(error.code, "InvalidScope");

    let mut wrong_hash = scope;
    wrong_hash.source_body_hash = "0".repeat(64);
    let error = project
        .start_discussion(start_request(
            &access,
            &document,
            "scope-wrong-hash",
            "Discuss this sentence.",
            Some(wrong_hash),
            Vec::new(),
        ))
        .expect_err("wrong source hash must be rejected");
    assert_eq!(error.code, "InvalidScope");

    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "scope-target-change".into(),
            expected: document.head.clone(),
            local_generation: "1".into(),
            body: body("the selected sentence changed"),
            cause: SaveCause::Typing,
        })
        .expect("change target after captured request");
    let error = project
        .start_discussion(start_request(
            &access,
            &document,
            "scope-stale-head",
            "Discuss this sentence.",
            None,
            Vec::new(),
        ))
        .expect_err("stale target head must be rejected");
    assert_eq!(error.code, "VersionConflict");
}

#[test]
fn begin_is_single_claim_and_output_events_use_sequence_and_event_cas() {
    let temp = TempDir::new("output");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = start(&project, &access, &document, "output-run");
    let owner = started.run.owner.clone();
    let claimed = begin(&project, &owner);
    assert_eq!(claimed.status, DiscussionRunStatus::Running);
    assert_eq!(claimed.dispatch_state, "claimed");

    let error = project
        .begin_discussion_run(DiscussionBegin {
            owner: owner.clone(),
        })
        .expect_err("a run can only be claimed once");
    assert_eq!(error.code, "RunAlreadyStarted");

    let first = append(&project, &owner, "0", "chunk-1", "alpha");
    assert_eq!(first.sequence, "1");
    assert_eq!(first.output_text, "alpha");
    let duplicate = project
        .append_discussion_output(DiscussionOutputAppend {
            owner: owner.clone(),
            expected_sequence: "0".into(),
            event_id: "chunk-1".into(),
            chunk: "alpha".into(),
        })
        .expect("same output event is idempotent");
    assert_eq!(duplicate.sequence, "1");

    let error = project
        .append_discussion_output(DiscussionOutputAppend {
            owner: owner.clone(),
            expected_sequence: "1".into(),
            event_id: "chunk-1".into(),
            chunk: "alpha".into(),
        })
        .expect_err("same chunk with a changed sequence is not the original event");
    assert_eq!(error.code, "EventIdReused");

    let error = project
        .append_discussion_output(DiscussionOutputAppend {
            owner: owner.clone(),
            expected_sequence: "0".into(),
            event_id: "chunk-gap".into(),
            chunk: "gap".into(),
        })
        .expect_err("a gap must not be accepted");
    assert_eq!(error.code, "SequenceConflict");
    let error = project
        .append_discussion_output(DiscussionOutputAppend {
            owner: owner.clone(),
            expected_sequence: "1".into(),
            event_id: "chunk-1".into(),
            chunk: "different".into(),
        })
        .expect_err("event IDs bind their original chunk");
    assert_eq!(error.code, "EventIdReused");

    append(&project, &owner, "1", "chunk-2", " beta");
    Connection::open(project.path.join("project.sqlite3"))
        .expect("open db for terminal fault injection")
        .execute_batch(
            "CREATE TRIGGER fail_assistant_message BEFORE INSERT ON discussion_messages
             WHEN NEW.role='assistant'
             BEGIN SELECT RAISE(ABORT,'injected assistant message failure'); END;",
        )
        .expect("install terminal message fault");
    let error = project
        .finish_discussion(DiscussionFinish {
            owner: owner.clone(),
            expected_sequence: "2".into(),
            event_id: "terminal-rollback".into(),
            assistant_text: "must roll back".into(),
        })
        .expect_err("terminal event and message must commit atomically");
    assert_eq!(error.code, "PersistenceUnavailable");
    let after_failed_finish = discussion(&project, &access);
    assert_eq!(
        after_failed_finish.runs[0].status,
        DiscussionRunStatus::Running
    );
    assert_eq!(after_failed_finish.runs[0].sequence, "2");
    assert_eq!(after_failed_finish.runs[0].output_text, "alpha beta");
    assert_eq!(after_failed_finish.messages.len(), 1);
    Connection::open(project.path.join("project.sqlite3"))
        .expect("reopen db after terminal fault")
        .execute_batch("DROP TRIGGER fail_assistant_message;")
        .expect("remove terminal message fault");
    let completed = finish(&project, &owner, "2", "terminal-1", "final answer");
    assert_eq!(completed.status, DiscussionRunStatus::Completed);
    assert_eq!(completed.sequence, "3");
    assert_eq!(completed.output_text, "final answer");
    let view = discussion(&project, &access);
    assert_eq!(view.runs.len(), 1);
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Completed);
    assert_eq!(view.messages.len(), 2);
    let assistant = view
        .messages
        .iter()
        .find(|message| message.role == DiscussionMessageRole::Assistant)
        .expect("terminal output has an assistant message");
    assert_eq!(assistant.content, "final answer");

    let duplicate_terminal = project
        .finish_discussion(DiscussionFinish {
            owner: owner.clone(),
            expected_sequence: "2".into(),
            event_id: "terminal-1".into(),
            assistant_text: "final answer".into(),
        })
        .expect("same terminal event is idempotent");
    assert_eq!(duplicate_terminal.id, completed.id);
    assert_eq!(discussion(&project, &access).messages.len(), 2);
    let error = project
        .finish_discussion(DiscussionFinish {
            owner,
            expected_sequence: "0".into(),
            event_id: "terminal-1".into(),
            assistant_text: "final answer".into(),
        })
        .expect_err("a terminal retry must retain its original sequence");
    assert_eq!(error.code, "EventIdReused");
}

#[test]
fn stop_linearizes_late_output_and_terminal_delivery() {
    let temp = TempDir::new("stop");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = start(&project, &access, &document, "stop-run");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    append(&project, &owner, "0", "partial", "partial output");
    let stopped = project
        .stop_discussion(access.clone(), owner.run_id.clone())
        .expect("stop exact run");
    assert_eq!(stopped.run.status, DiscussionRunStatus::Stopped);
    assert_eq!(stopped.run.stop_reason.as_deref(), Some("author_stopped"));
    assert_eq!(stopped.run.sequence, "2");
    let stopped_view = discussion(&project, &access);
    assert_eq!(stopped_view.messages.len(), 2);
    let stop_message = stopped_view
        .messages
        .iter()
        .find(|message| message.role == DiscussionMessageRole::Assistant)
        .expect("stop records an immutable terminal assistant message");
    assert!(stop_message.content.contains("partial output"));
    assert!(stop_message.content.to_ascii_lowercase().contains("stopp"));
    let error = project
        .append_discussion_output(DiscussionOutputAppend {
            owner: owner.clone(),
            expected_sequence: "2".into(),
            event_id: "late".into(),
            chunk: "late output".into(),
        })
        .expect_err("late output after stop must be fenced");
    assert_eq!(error.code, "RunSealed");
    let error = project
        .finish_discussion(DiscussionFinish {
            owner: owner.clone(),
            expected_sequence: "2".into(),
            event_id: "late-terminal".into(),
            assistant_text: "late terminal".into(),
        })
        .expect_err("late terminal after stop must be fenced");
    assert_eq!(error.code, "RunSealed");
    drop(project);
    let reopened = ProjectSession::open(temp.child("project")).expect("reopen stopped project");
    let reopened_access = reopened
        .attach("stop-reopened".into())
        .expect("attach stopped project");
    let reopened_view = discussion(&reopened, &reopened_access);
    assert_eq!(reopened_view.messages.len(), 2);
    assert_eq!(reopened_view.runs[0].status, DiscussionRunStatus::Stopped);
    assert_eq!(reopened_view.runs[0].sequence, "2");
    assert!(reopened_view.messages[1].content.contains("partial output"));
    let repeated = reopened
        .stop_discussion(reopened_access, owner.run_id)
        .expect("repeated stop is idempotent");
    assert_eq!(repeated.run.status, DiscussionRunStatus::Stopped);
    assert_eq!(repeated.run.sequence, "2");
    assert_eq!(
        discussion(&reopened, &reopened.attach("stop-read".into()).unwrap())
            .messages
            .len(),
        2
    );
}

#[test]
fn source_or_policy_change_between_queue_and_begin_fails_closed() {
    for change in ["source", "policy"] {
        let label = change;
        let temp = TempDir::new(label);
        let (project, access, document) = setup_project(&temp.child("project"));
        let started = start(&project, &access, &document, &format!("queued-{label}"));
        if change == "source" {
            project
                .save(SaveSnapshot {
                    access: access.clone(),
                    operation_id: "queued-source-change".into(),
                    expected: document.head.clone(),
                    local_generation: "1".into(),
                    body: body("source changed while queued"),
                    cause: SaveCause::Typing,
                })
                .expect("change source while queued");
        } else {
            project
                .revoke_story_context(access.clone(), "0".into())
                .expect("revoke policy while queued");
        }
        let error = project
            .begin_discussion_run(DiscussionBegin {
                owner: started.run.owner.clone(),
            })
            .expect_err("stale queued context must not dispatch");
        assert_eq!(error.code, "ContextChanged");
        let view = discussion(&project, &access);
        assert_eq!(view.runs[0].status, DiscussionRunStatus::Failed);
        assert_eq!(view.runs[0].stop_reason.as_deref(), Some("context_stale"));
        if change == "policy" {
            let retry_error = project
                .start_discussion(start_request(
                    &access,
                    &document,
                    &format!("queued-{label}"),
                    "What should the next beat emphasize?",
                    None,
                    Vec::new(),
                ))
                .expect_err("a start retry cannot bypass revoked packet reads");
            assert_eq!(retry_error.code, "ContextPolicyChanged");
            assert_eq!(discussion(&project, &access).runs.len(), 1);
        }
    }
}

#[test]
fn queued_failure_seals_once_and_retains_a_terminal_message() {
    let temp = TempDir::new("queued-failure");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = start(&project, &access, &document, "queued-failure-run");
    let failure = project
        .fail_discussion_run(DiscussionFail {
            owner: started.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: "failure-event".into(),
            reason: "the local worker could not start".into(),
        })
        .expect("a queued worker failure is terminalized durably");
    assert_eq!(failure.status, DiscussionRunStatus::Failed);
    assert_eq!(failure.sequence, "1");
    assert_eq!(
        failure.stop_reason.as_deref(),
        Some("the local worker could not start")
    );
    let view = discussion(&project, &access);
    assert_eq!(view.messages.len(), 2);
    assert!(view.messages.iter().any(|message| {
        message.role == DiscussionMessageRole::Assistant
            && message.content.contains("local worker could not start")
    }));
    let retry = project
        .fail_discussion_run(DiscussionFail {
            owner: started.run.owner,
            expected_sequence: "0".into(),
            event_id: "failure-event".into(),
            reason: "the local worker could not start".into(),
        })
        .expect("same failure event is idempotent");
    assert_eq!(retry.id, failure.id);
    assert_eq!(discussion(&project, &access).messages.len(), 2);
    let error = project
        .finish_discussion(DiscussionFinish {
            owner: retry.owner,
            expected_sequence: "0".into(),
            event_id: "failure-event".into(),
            assistant_text: "Discussion failed: the local worker could not start".into(),
        })
        .expect_err("a failure receipt cannot acknowledge completion");
    assert_eq!(error.code, "EventIdReused");
}

#[test]
fn restart_marks_active_runs_interrupted_without_replaying_them() {
    let temp = TempDir::new("restart");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let started = start(&project, &access, &document, "restart-run");
    drop(project);

    let reopened = ProjectSession::open(&path).expect("reopen project");
    let reopened_access = reopened
        .attach("restart-session".into())
        .expect("attach reopened");
    let view = discussion(&reopened, &reopened_access);
    assert_eq!(view.runs.len(), 1);
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Interrupted);
    assert_eq!(view.runs[0].sequence, "1");
    assert_eq!(view.messages.len(), 2);
    let retry = reopened
        .start_discussion(start_request(
            &reopened_access,
            &reopened
                .document(reopened_access.clone(), "chapter-one".into())
                .unwrap(),
            "restart-run",
            "What should the next beat emphasize?",
            None,
            Vec::new(),
        ))
        .expect("replaying the same operation reads historical run");
    assert_eq!(retry.run.id, started.run.id);
    assert_eq!(retry.run.status, DiscussionRunStatus::Interrupted);
}

#[test]
fn recovered_and_separate_projects_cannot_control_each_others_jobs() {
    let temp = TempDir::new("ownership");
    let source_path = temp.child("source");
    let (source, source_access, source_document) = setup_project(&source_path);
    let source_start = start(&source, &source_access, &source_document, "source-run");
    let archive = temp.child("source.wnsbackup");
    create_backup(&source, &archive).expect("backup active discussion");
    let recovered_path = temp.child("recovered");
    let recovered = recover_backup(&archive, &recovered_path, "Recovered discussion")
        .expect("recover discussion copy");
    let recovered_access = recovered
        .attach("recovered-session".into())
        .expect("attach copy");
    let recovered_view = discussion(&recovered, &recovered_access);
    assert_eq!(recovered_view.runs.len(), 1);
    assert_eq!(
        recovered_view.runs[0].status,
        DiscussionRunStatus::Interrupted
    );
    let error = recovered
        .begin_discussion_run(DiscussionBegin {
            owner: source_start.run.owner.clone(),
        })
        .expect_err("copied historical run cannot be dispatched");
    assert!(matches!(
        error.code.as_str(),
        "RunSealed" | "DiscussionProjectMismatch"
    ));
    let recovered_document = recovered
        .document(recovered_access.clone(), "chapter-one".into())
        .expect("read recovered chapter");
    let new_start = start(
        &recovered,
        &recovered_access,
        &recovered_document,
        "recovered-run",
    );
    assert_ne!(
        new_start.run.owner.project_id,
        source_start.run.owner.project_id
    );
    assert_ne!(
        new_start.run.owner.operation_namespace,
        source_start.run.owner.operation_namespace
    );

    let other_path = temp.child("other");
    let (other, other_access, other_document) = setup_project(&other_path);
    let other_start = start(&other, &other_access, &other_document, "other-run");
    let error = other
        .stop_discussion(other_access.clone(), source_start.run.id.clone())
        .expect_err("a separate project must not resolve another job");
    assert_eq!(error.code, "DiscussionRunNotFound");
    assert_eq!(
        discussion(&other, &other_access).runs[0].id,
        other_start.run.id
    );
}

#[test]
fn composer_draft_is_idempotent_cas_safe_and_retained_when_target_becomes_stale() {
    let temp = TempDir::new("draft");
    let (project, access, document) = setup_project(&temp.child("project"));
    let first = save_draft(&project, &access, "0", "draft-one", "unsent question");
    assert_eq!(first.version, "1");
    let retry = save_draft(&project, &access, "0", "draft-one", "unsent question");
    assert_eq!(retry.version, first.version);
    assert_eq!(retry.text, first.text);
    let error = project
        .save_discussion_draft(SaveDiscussionDraft {
            access: access.clone(),
            operation_id: "draft-one".into(),
            document_id: "chapter-one".into(),
            expected_version: "0".into(),
            text: "changed payload".into(),
            intent: Default::default(),
            scope: None,
            pinned_document_ids: Vec::new(),
            previous_run_id: None,
        })
        .expect_err("draft operation payload is immutable");
    assert_eq!(error.code, "OperationIdReusedWithDifferentPayload");
    let error = project
        .save_discussion_draft(SaveDiscussionDraft {
            access: access.clone(),
            operation_id: "draft-two".into(),
            document_id: "chapter-one".into(),
            expected_version: "0".into(),
            text: "stale version".into(),
            intent: Default::default(),
            scope: None,
            pinned_document_ids: Vec::new(),
            previous_run_id: None,
        })
        .expect_err("draft version is a CAS boundary");
    assert_eq!(error.code, "DraftVersionConflict");

    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "draft-target-change".into(),
            expected: document.head.clone(),
            local_generation: "1".into(),
            body: body("the manuscript moved on"),
            cause: SaveCause::Typing,
        })
        .expect("change target while draft remains unsent");
    let view = discussion(&project, &access);
    assert_eq!(view.draft.as_ref().unwrap().text, "unsent question");
    let error = project
        .start_discussion(start_request(
            &access,
            &document,
            "draft-stale-start",
            "Send the retained draft.",
            None,
            Vec::new(),
        ))
        .expect_err("starting from stale target must be rejected");
    assert_eq!(error.code, "VersionConflict");
}

#[test]
fn retry_after_terminal_run_links_a_new_run_to_the_same_project_document() {
    let temp = TempDir::new("retry");
    let (project, access, document) = setup_project(&temp.child("project"));
    let first = start(&project, &access, &document, "terminal-run");
    project
        .stop_discussion(access.clone(), first.run.id.clone())
        .unwrap();
    let mut request = start_request(
        &access,
        &document,
        "retry-run",
        "What should the next beat emphasize?",
        None,
        Vec::new(),
    );
    request.previous_run_id = Some(first.run.id.clone());
    let retry = project
        .start_discussion(request)
        .expect("new retry creates a linked run");
    assert_ne!(retry.run.id, first.run.id);
    assert_eq!(
        retry.run.previous_run_id.as_deref(),
        Some(first.run.id.as_str())
    );
    assert_eq!(retry.run.owner.project_id, first.run.owner.project_id);
    assert_eq!(
        retry.run.owner.operation_namespace,
        first.run.owner.operation_namespace
    );
    assert_eq!(retry.run.target.document_id, first.run.target.document_id);

    let other_path = temp.child("other-project");
    let (other, other_access, other_document) = setup_project(&other_path);
    let mut cross_project = start_request(
        &other_access,
        &other_document,
        "cross-project-retry",
        "A foreign terminal run must not become context.",
        None,
        Vec::new(),
    );
    cross_project.previous_run_id = Some(first.run.id);
    let error = other
        .start_discussion(cross_project)
        .expect_err("retry linkage must stay within one project and document");
    assert_eq!(error.code, "PreviousRunNotFound");
}

fn retry_request(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &webnovel_core::projects::DocumentRecord,
    run_id: &str,
    operation_id: &str,
) -> StartDiscussion {
    let draft = project
        .discussion_retry(access.clone(), run_id.to_owned())
        .unwrap();
    let mut request = start_request(
        access,
        document,
        operation_id,
        &draft.text,
        draft.scope,
        draft.pinned_document_ids,
    );
    request.intent = draft.intent;
    request.previous_run_id = Some(draft.previous_run_id);
    request
}

#[test]
fn linked_retries_keep_exact_guidance_without_consuming_the_next_new_requests_instruction() {
    use webnovel_core::context::guidance::GuidanceScope;
    let temp = TempDir::new("retry-guidance");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let original = adopt_guidance(
        &project,
        &access,
        "once",
        GuidanceScope::Request,
        "Keep the ending.",
    );
    let first = start(&project, &access, &document, "first");
    project
        .stop_discussion(access.clone(), first.run.id.clone())
        .unwrap();
    let next = adopt_guidance(
        &project,
        &access,
        "next",
        GuidanceScope::Request,
        "Discuss the next chapter's hook.",
    );
    let request = retry_request(&project, &access, &document, &first.run.id, "retry");
    let before = counts(&project);
    let mut too_small = request.clone();
    too_small.budget = MockContextBudget::new("1", "0", "0");
    assert!(project.start_discussion(too_small).is_err());
    assert_eq!(counts(&project), before);
    let retried = project.start_discussion(request.clone()).unwrap();
    assert_eq!(
        retried.packet.receipt.guidance_handles,
        vec![format!("guidance-{}", original.version_id)]
    );
    assert!(!retried.packet.messages[1].content.contains(&next.text));
    assert_eq!(
        project.start_discussion(request).unwrap().packet,
        retried.packet
    );
    assert_eq!(
        project
            .guidance(access.clone(), "chapter-one".into())
            .unwrap(),
        vec![next.clone()]
    );
    drop(project);
    let project = ProjectSession::open(&path).unwrap();
    let access = project.attach("reopened".into()).unwrap();
    let chained = project
        .start_discussion(retry_request(
            &project,
            &access,
            &document,
            &retried.run.id,
            "chained",
        ))
        .unwrap();
    assert_eq!(
        chained.packet.receipt.guidance_handles,
        retried.packet.receipt.guidance_handles
    );
    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    let uses: i64 = connection
        .query_row("SELECT COUNT(*) FROM guidance_request_uses", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(uses, 1);
    let unrelated = start(&project, &access, &document, "new-request");
    assert_eq!(
        unrelated.packet.receipt.guidance_handles,
        vec![format!("guidance-{}", next.version_id)]
    );
    assert_eq!(
        project
            .document(access.clone(), "chapter-one".into())
            .unwrap()
            .body,
        document.body
    );
    let archive = temp.child("backup.wnsbackup");
    create_backup(&project, &archive).unwrap();
    let recovered = recover_backup(&archive, &temp.child("recovered"), "Recovered").unwrap();
    let recovered_access = recovered.attach("recovered".into()).unwrap();
    assert_eq!(
        recovered
            .discussion_retry(recovered_access, first.run.id)
            .unwrap_err()
            .code,
        "PreviousRunMismatch"
    );
}

#[test]
fn linked_retry_preserves_exact_feedback_scope_and_pins_and_refuses_completed_runs() {
    let temp = TempDir::new("retry-shape");
    let (project, access, document) = setup_project(&temp.child("project"));
    let other = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "other".into(),
            document_id: "other".into(),
            title: "Old promise".into(),
            kind: "note".into(),
            body: body("Keep the key safe."),
        })
        .unwrap();
    let first = project
        .start_discussion(start_request(
            &access,
            &document,
            "first",
            "Make this warmer.",
            Some(scope_input(&document)),
            vec![other.head.document_id.clone()],
        ))
        .unwrap();
    assert_eq!(
        project
            .discussion_retry(access.clone(), first.run.id.clone())
            .unwrap_err()
            .code,
        "PreviousRunActive"
    );
    project
        .stop_discussion(access.clone(), first.run.id.clone())
        .unwrap();
    let original = retry_request(&project, &access, &document, &first.run.id, "retry");
    assert_eq!(original.pinned_document_ids, vec![other.head.document_id]);
    assert_eq!(original.scope, Some(scope_input(&document)));
    let before = counts(&project);
    for change in 0..3 {
        let mut request = original.clone();
        match change {
            0 => request.instruction.push('!'),
            1 => request.scope = None,
            _ => request.pinned_document_ids.clear(),
        }
        assert_eq!(
            project.start_discussion(request).unwrap_err().code,
            "RetryRequestChanged"
        );
        assert_eq!(counts(&project), before);
    }
    let mut foreign_target = original.clone();
    foreign_target.expected.document_id = "other".into();
    assert_eq!(
        project.start_discussion(foreign_target).unwrap_err().code,
        "PreviousRunMismatch"
    );
    let done = completed_turn(&project, &access, &document, "done", "A complete answer.");
    assert_eq!(
        project
            .discussion_retry(access.clone(), done.run.id)
            .unwrap_err()
            .code,
        "RetryAlreadyCompleted"
    );
    let retried = project.start_discussion(original).unwrap();
    assert_eq!(retried.user_message.scope, first.user_message.scope);
}

#[test]
fn retry_rebuilds_current_sources_but_never_revives_revoked_or_changed_guidance() {
    use webnovel_core::context::guidance::GuidanceScope;
    for mode in ["source", "edited", "retired", "policy"] {
        let temp = TempDir::new(mode);
        let (project, access, document) = setup_project(&temp.child("project"));
        let guidance = adopt_guidance(
            &project,
            &access,
            "once",
            GuidanceScope::Request,
            "Preserve the ending.",
        );
        let first = start(&project, &access, &document, "first");
        project
            .stop_discussion(access.clone(), first.run.id.clone())
            .unwrap();
        let mut request = retry_request(&project, &access, &document, &first.run.id, "retry");
        match mode {
            "source" => {
                let save = project
                    .save(SaveSnapshot {
                        access: access.clone(),
                        operation_id: "new-prose".into(),
                        expected: document.head.clone(),
                        local_generation: "1".into(),
                        body: body("New whole-chapter prose."),
                        cause: SaveCause::Typing,
                    })
                    .unwrap();
                request.expected = save.head;
                let result = project.start_discussion(request).unwrap();
                assert!(
                    result.packet.messages[1]
                        .content
                        .contains("New whole-chapter prose.")
                );
                assert!(result.packet.messages[1].content.contains(&guidance.text));
                assert_ne!(
                    result.packet.receipt.snapshot_id,
                    first.packet.receipt.snapshot_id
                );
            }
            "policy" => {
                project
                    .revoke_story_context(access.clone(), "0".into())
                    .unwrap();
                assert_eq!(
                    project.start_discussion(request).unwrap_err().code,
                    "ContextPolicyChanged"
                );
            }
            _ => {
                project
                    .save_guidance(webnovel_core::projects::guidance::SaveGuidance {
                        access: access.clone(),
                        operation_id: "change-guidance".into(),
                        guidance_id: guidance.guidance_id,
                        expected_version: "1".into(),
                        text: "Use this changed direction.".into(),
                        scope: GuidanceScope::Request,
                        document_id: Some("chapter-one".into()),
                        active: mode != "retired",
                        origin_message_id: None,
                    })
                    .unwrap();
                assert_eq!(
                    project.start_discussion(request).unwrap_err().code,
                    "RetryGuidanceChanged"
                );
            }
        }
    }
}

#[test]
fn retry_composer_link_is_durable_payload_bound_and_fenced_in_recovered_copies() {
    let temp = TempDir::new("retry-composer");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let first = start(&project, &access, &document, "first");
    project
        .stop_discussion(access.clone(), first.run.id.clone())
        .unwrap();
    let retry = project
        .discussion_retry(access.clone(), first.run.id.clone())
        .unwrap();
    let request = SaveDiscussionDraft {
        access: access.clone(),
        operation_id: "save-retry".into(),
        document_id: "chapter-one".into(),
        expected_version: "0".into(),
        text: retry.text,
        intent: retry.intent,
        scope: retry.scope,
        pinned_document_ids: retry.pinned_document_ids,
        previous_run_id: Some(first.run.id.clone()),
    };
    let saved = project.save_discussion_draft(request.clone()).unwrap();
    assert_eq!(
        saved.previous_run_id.as_deref(),
        Some(first.run.id.as_str())
    );
    let mut changed = request.clone();
    changed.previous_run_id = None;
    assert_eq!(
        project.save_discussion_draft(changed).unwrap_err().code,
        "OperationIdReusedWithDifferentPayload"
    );
    let archive = temp.child("backup.wnsbackup");
    create_backup(&project, &archive).unwrap();
    let recovered = recover_backup(&archive, &temp.child("copy"), "Copy").unwrap();
    let copy_access = recovered.attach("copy".into()).unwrap();
    assert!(
        recovered
            .read_discussion(copy_access.clone(), "chapter-one".into())
            .unwrap()
            .draft
            .is_none()
    );
    assert_eq!(
        recovered
            .save_discussion_draft(SaveDiscussionDraft {
                access: copy_access,
                operation_id: "foreign-retry".into(),
                ..request.clone()
            })
            .unwrap_err()
            .code,
        "PreviousRunMismatch"
    );
    drop(project);
    let project = ProjectSession::open(&path).unwrap();
    let fresh = project.attach("restart".into()).unwrap();
    assert_eq!(
        project
            .read_discussion(fresh.clone(), "chapter-one".into())
            .unwrap()
            .draft
            .unwrap()
            .previous_run_id,
        saved.previous_run_id
    );
    assert_eq!(
        project
            .save_discussion_draft(SaveDiscussionDraft {
                access: fresh,
                ..request
            })
            .unwrap()
            .version,
        saved.version
    );
}

#[test]
fn schema_six_upgrade_preserves_old_draft_receipts_and_takes_a_backup() {
    let temp = TempDir::new("schema-six");
    let path = temp.child("project");
    let (project, access, _) = setup_project(&path);
    let saved = save_draft(&project, &access, "0", "old-save", "A retained thought.");
    drop(project);
    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER command_receipts_no_proposal_collision;
             DROP TABLE proposal_receipts; DROP TABLE proposal_decisions; DROP TABLE proposal_versions; DROP TABLE proposals;
             ALTER TABLE discussion_drafts DROP COLUMN intent;
             ALTER TABLE discussion_drafts DROP COLUMN previous_run_id; PRAGMA user_version=6;",
        )
        .unwrap();
    drop(connection);
    let project = ProjectSession::open(&path).unwrap();
    let access = project.attach("upgraded".into()).unwrap();
    let replayed = save_draft(&project, &access, "0", "old-save", "A retained thought.");
    assert_eq!(replayed.version, saved.version);
    assert!(replayed.previous_run_id.is_none());
    assert_eq!(replayed.text, saved.text);
    assert!(fs::read_dir(path.join("migrations")).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("schema6-before-schema8-")
    }));
}

#[test]
fn propose_edits_uses_restricted_chapter_context_without_author_room_material() {
    let temp = TempDir::new("propose-policy");
    let (project, access, document) = setup_project(&temp.child("project"));
    let future = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "future-chapter".into(),
            document_id: "chapter-two".into(),
            title: "Chapter two".into(),
            kind: "chapter".into(),
            body: body("A future revelation."),
        })
        .unwrap();
    let private = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "private-note".into(),
            document_id: "private-note".into(),
            title: "Private planning".into(),
            kind: "note".into(),
            body: body("The mentor's secret."),
        })
        .unwrap();
    adopt_guidance(
        &project,
        &access,
        "author-room-guide",
        webnovel_core::context::guidance::GuidanceScope::Project,
        "Keep the ending private.",
    );
    completed_turn(
        &project,
        &access,
        &document,
        "author-room-chat",
        "A private planning answer.",
    );

    let mut request = start_request(
        &access,
        &document,
        "restricted-proposal",
        "Make this selected passage more vivid.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    request.intent = FeedbackIntent::ProposeEdits;
    let started = project.start_discussion(request).unwrap();
    assert_eq!(started.run.intent, FeedbackIntent::ProposeEdits);
    let frozen = project
        .story_snapshot(access.clone(), started.packet.receipt.snapshot_id)
        .unwrap();
    assert_eq!(
        frozen.purpose,
        webnovel_core::context::ContextPurpose::Revise
    );
    assert_eq!(
        frozen.policy.audience,
        webnovel_core::context::Audience::RestrictedWriting
    );
    assert_eq!(frozen.policy.reader_frontier.as_deref(), Some("0"));
    assert!(frozen.guidance.is_empty());
    assert!(frozen.conversation.is_none());
    assert_eq!(frozen.snapshot.sources.len(), 1);
    assert_eq!(
        frozen.snapshot.sources[0].source.document_id,
        document.head.document_id
    );
    assert_ne!(
        frozen.snapshot.sources[0].source.document_id,
        future.head.document_id
    );
    assert_ne!(
        frozen.snapshot.sources[0].source.document_id,
        private.head.document_id
    );
    assert!(
        !started.packet.messages[1]
            .content
            .contains("Keep the ending private")
    );
}

#[test]
fn proposal_retry_exposes_and_preserves_intent() {
    let temp = TempDir::new("propose-retry");
    let (project, access, document) = setup_project(&temp.child("project"));
    let mut request = start_request(
        &access,
        &document,
        "proposal-first",
        "Revise the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    request.intent = FeedbackIntent::ProposeEdits;
    let first = project.start_discussion(request).unwrap();
    project
        .stop_discussion(access.clone(), first.run.id.clone())
        .unwrap();
    let retry = project
        .discussion_retry(access.clone(), first.run.id.clone())
        .unwrap();
    assert_eq!(retry.intent, FeedbackIntent::ProposeEdits);
    let saved = project
        .save_discussion_draft(SaveDiscussionDraft {
            access: access.clone(),
            operation_id: "proposal-retry-draft".into(),
            document_id: document.head.document_id.clone(),
            expected_version: "0".into(),
            text: retry.text.clone(),
            intent: retry.intent,
            scope: retry.scope.clone(),
            pinned_document_ids: retry.pinned_document_ids.clone(),
            previous_run_id: Some(first.run.id.clone()),
        })
        .unwrap();
    assert_eq!(saved.intent, FeedbackIntent::ProposeEdits);
    let mut changed_draft = SaveDiscussionDraft {
        access: access.clone(),
        operation_id: "proposal-retry-draft".into(),
        document_id: document.head.document_id.clone(),
        expected_version: "0".into(),
        text: retry.text.clone(),
        intent: FeedbackIntent::Discuss,
        scope: retry.scope.clone(),
        pinned_document_ids: retry.pinned_document_ids.clone(),
        previous_run_id: Some(first.run.id.clone()),
    };
    assert_eq!(
        project
            .save_discussion_draft(changed_draft.clone())
            .unwrap_err()
            .code,
        "OperationIdReusedWithDifferentPayload"
    );
    changed_draft.operation_id = "proposal-retry-draft-2".into();
    assert_eq!(
        project
            .save_discussion_draft(changed_draft)
            .unwrap_err()
            .code,
        "RetryRequestChanged"
    );
    let mut changed = retry_request(
        &project,
        &access,
        &document,
        &first.run.id,
        "proposal-retry-changed",
    );
    changed.intent = FeedbackIntent::Discuss;
    assert_eq!(
        project.start_discussion(changed).unwrap_err().code,
        "RetryRequestChanged"
    );
    let retried = project
        .start_discussion(retry_request(
            &project,
            &access,
            &document,
            &first.run.id,
            "proposal-retry",
        ))
        .unwrap();
    assert_eq!(retried.run.intent, FeedbackIntent::ProposeEdits);
}

#[test]
fn completed_prose_run_is_excluded_from_later_discussion_context() {
    let temp = TempDir::new("propose-conversation");
    let (project, access, document) = setup_project(&temp.child("project"));
    let mut request = start_request(
        &access,
        &document,
        "proposal-complete",
        "Revise the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    request.intent = FeedbackIntent::ProposeEdits;
    let proposal = project.start_discussion(request).unwrap();
    begin(&project, &proposal.run.owner);
    project
        .mark_discussion_delivered(proposal.run.owner.clone())
        .unwrap();
    finish(
        &project,
        &proposal.run.owner,
        "0",
        "proposal-finish",
        "A completed prose response.",
    );

    let discussion = start(&project, &access, &document, "discussion-after-proposal");
    assert!(
        discussion
            .packet
            .receipt
            .conversation_message_ids
            .is_empty()
    );
}
