use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::documents::{Endpoint, ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionFail, DiscussionFinish, DiscussionMessageRole,
    DiscussionOutputAppend, DiscussionRunStatus, DiscussionScopeInput, RunOwner,
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
            scope: None,
            pinned_document_ids: Vec::new(),
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
            scope: None,
            pinned_document_ids: Vec::new(),
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
            scope: None,
            pinned_document_ids: Vec::new(),
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
    begin(&project, &first.run.owner);
    finish(&project, &first.run.owner, "0", "terminal", "first answer");
    let mut request = start_request(
        &access,
        &document,
        "retry-run",
        "Try the discussion again.",
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
