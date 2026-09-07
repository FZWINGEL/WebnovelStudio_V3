use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::packet::{
    HTTP_INPUT_LIMIT_BYTES, HTTP_OUTPUT_LIMIT_BYTES, HTTP_PROFILE_VERSION,
    HTTP_TOKEN_ACCOUNTING_METHOD, HttpProviderBinding, HttpResponseFormat, MockContextBudget,
    PROPOSAL_RESPONSE_CONTRACT, ProviderBinding, serialized_input,
};
use webnovel_core::documents::{Endpoint, ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionFail, DiscussionFinish, DiscussionMessageRole,
    DiscussionOutputAppend, DiscussionRunStatus, DiscussionScopeInput, DiscussionStopCleanup,
    DiscussionStopSettled, FeedbackIntent, HttpDeliverySubmission, ProviderCleanup,
    ProviderDeliveryReceipt, ProviderOutcomeStatus, ProviderTerminalReport, ProviderUsage,
    RunOwner, SafeBriefInput, SaveDiscussionDraft, StartDiscussion,
};
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::providers::http_request::prepare_request;
use webnovel_core::transfer::{create_backup, recover_backup};

use crate::legacy_schema;

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

fn http_binding(response_format: HttpResponseFormat) -> ProviderBinding {
    ProviderBinding {
        provider_id: "openai-compatible:00000000-0000-0000-0000-000000000001".into(),
        model_id: "gpt-5.6-luna".into(),
        reasoning: Some("xhigh".into()),
        service_tier: Some("priority".into()),
        profile_version: HTTP_PROFILE_VERSION.into(),
        input_limit_bytes: HTTP_INPUT_LIMIT_BYTES.to_string(),
        reserved_output_bytes: "0".into(),
        reserved_protocol_bytes: "0".into(),
        output_limit_bytes: HTTP_OUTPUT_LIMIT_BYTES.to_string(),
        accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.into(),
        runtime: None,
        http: Some(HttpProviderBinding {
            base_url: "https://example.test/v1".into(),
            config_revision: "1".into(),
            stream: true,
            response_format,
        }),
    }
}

fn http_delivery(
    dispatch: &webnovel_core::projects::discussions::DiscussionDispatch,
    submission: HttpDeliverySubmission,
) -> ProviderDeliveryReceipt {
    let prepared = prepare_request(&dispatch.packet.messages, &dispatch.packet.options).unwrap();
    ProviderDeliveryReceipt {
        body_hash: prepared.body_hash,
        body_bytes: prepared.body_bytes,
        submission,
        usage: None,
    }
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
        basis: None,
        scope,
        pinned_document_ids,
        safe_brief: None,
        budget: budget(),
        provider_binding: None,
        previous_run_id: None,
        lookup: None,
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

fn settle_stop(
    project: &ProjectSession,
    owner: &RunOwner,
    expected_sequence: &str,
    event_id: &str,
    assistant_text: &str,
    cleanup: DiscussionStopCleanup,
) -> webnovel_core::projects::discussions::DiscussionRun {
    project
        .settle_discussion_stop(DiscussionStopSettled {
            owner: owner.clone(),
            expected_sequence: expected_sequence.into(),
            event_id: event_id.into(),
            assistant_text: assistant_text.into(),
            cleanup,
        })
        .expect("settle stopped discussion")
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
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            previous_run_id: None,
            lookup: None,
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

fn safe_brief(text: &str, origin_message_id: Option<String>, confirmed: bool) -> SafeBriefInput {
    SafeBriefInput {
        text: text.into(),
        origin_message_id,
        confirmed,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
            assistant_text: "alpha beta final".into(),
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
    let completed = finish(&project, &owner, "2", "terminal-1", "alpha beta final");
    assert_eq!(completed.status, DiscussionRunStatus::Completed);
    assert_eq!(completed.sequence, "3");
    assert_eq!(completed.output_text, "alpha beta final");
    let view = discussion(&project, &access);
    assert_eq!(view.runs.len(), 1);
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Completed);
    assert_eq!(view.messages.len(), 2);
    let assistant = view
        .messages
        .iter()
        .find(|message| message.role == DiscussionMessageRole::Assistant)
        .expect("terminal output has an assistant message");
    assert_eq!(assistant.content, "alpha beta final");

    let duplicate_terminal = project
        .finish_discussion(DiscussionFinish {
            owner: owner.clone(),
            expected_sequence: "2".into(),
            event_id: "terminal-1".into(),
            assistant_text: "alpha beta final".into(),
        })
        .expect("same terminal event is idempotent");
    assert_eq!(duplicate_terminal.id, completed.id);
    assert_eq!(discussion(&project, &access).messages.len(), 2);
    let error = project
        .finish_discussion(DiscussionFinish {
            owner,
            expected_sequence: "0".into(),
            event_id: "terminal-1".into(),
            assistant_text: "alpha beta final".into(),
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
    assert_eq!(stopped.run.status, DiscussionRunStatus::Stopping);
    assert_eq!(stopped.run.stop_reason.as_deref(), Some("author_stopped"));
    assert_eq!(stopped.run.sequence, "1");
    let stopping_view = discussion(&project, &access);
    assert_eq!(stopping_view.messages.len(), 1);
    let settled = settle_stop(
        &project,
        &owner,
        "1",
        "stop-settle",
        "partial output",
        DiscussionStopCleanup::Settled,
    );
    assert_eq!(settled.status, DiscussionRunStatus::Stopped);
    assert_eq!(settled.sequence, "2");
    assert_eq!(settled.output_text, "partial output");
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
    assert!(
        reopened_view.messages[1]
            .content
            .to_ascii_lowercase()
            .contains("stopp")
    );
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
fn queued_stop_seals_with_terminal_message_and_repeats() {
    let temp = TempDir::new("queued-stop");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = start(&project, &access, &document, "queued-stop-run");
    let first = project
        .stop_discussion(access.clone(), started.run.id.clone())
        .expect("queued stop is durable");
    assert_eq!(first.run.status, DiscussionRunStatus::Stopped);
    assert_eq!(first.run.sequence, "1");
    assert_eq!(discussion(&project, &access).messages.len(), 2);
    let repeated = project
        .stop_discussion(access.clone(), started.run.id.clone())
        .expect("terminal stop is idempotent");
    assert_eq!(repeated.run.status, DiscussionRunStatus::Stopped);
    assert_eq!(repeated.run.sequence, "1");
    let error = project
        .settle_discussion_stop(DiscussionStopSettled {
            owner: started.run.owner,
            expected_sequence: "1".into(),
            event_id: "queued-settle".into(),
            assistant_text: String::new(),
            cleanup: DiscussionStopCleanup::Settled,
        })
        .expect_err("a queued stop has no worker settlement to acknowledge");
    assert_eq!(error.code, "RunSealed");
}

#[test]
fn stopping_blocks_retry_and_settlement_replay_is_idempotent() {
    let temp = TempDir::new("stopping-replay");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = start(&project, &access, &document, "stopping-replay-run");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    append(&project, &owner, "0", "partial", "partial");
    let stopping = project
        .stop_discussion(access.clone(), owner.run_id.clone())
        .expect("stop intent");
    assert_eq!(stopping.run.status, DiscussionRunStatus::Stopping);
    assert_eq!(stopping.run.sequence, "1");
    let error = project
        .mark_discussion_delivered(owner.clone())
        .expect_err("stop intent prevents delivery acknowledgement");
    assert_eq!(error.code, "RunStopping");
    let error = project
        .discussion_retry(access.clone(), owner.run_id.clone())
        .expect_err("retry waits for worker settlement");
    assert_eq!(error.code, "PreviousRunActive");
    let settled = settle_stop(
        &project,
        &owner,
        "1",
        "settlement-replay",
        "partial plus cleanup",
        DiscussionStopCleanup::Settled,
    );
    assert_eq!(settled.status, DiscussionRunStatus::Stopped);
    assert_eq!(settled.sequence, "2");
    assert_eq!(settled.output_text, "partial plus cleanup");
    let replay = project
        .settle_discussion_stop(DiscussionStopSettled {
            owner: owner.clone(),
            expected_sequence: "1".into(),
            event_id: "settlement-replay".into(),
            assistant_text: "partial plus cleanup".into(),
            cleanup: DiscussionStopCleanup::Settled,
        })
        .expect("identical settlement replay");
    assert_eq!(replay.id, settled.id);
    assert_eq!(replay.sequence, "2");
    let error = project
        .settle_discussion_stop(DiscussionStopSettled {
            owner,
            expected_sequence: "1".into(),
            event_id: "settlement-replay".into(),
            assistant_text: "partial changed".into(),
            cleanup: DiscussionStopCleanup::Settled,
        })
        .expect_err("changed settlement payload reuses the event ID");
    assert_eq!(error.code, "EventIdReused");
}

#[test]
fn unresolved_stop_settlement_interrupts_without_proposals() {
    let temp = TempDir::new("stopping-unresolved");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = start(&project, &access, &document, "stopping-unresolved-run");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    append(&project, &owner, "0", "partial", "partial");
    project
        .stop_discussion(access.clone(), owner.run_id.clone())
        .expect("stop intent");
    let interrupted = settle_stop(
        &project,
        &owner,
        "1",
        "unresolved-settlement",
        "partial cleanup tail",
        DiscussionStopCleanup::Unresolved,
    );
    assert_eq!(interrupted.status, DiscussionRunStatus::Interrupted);
    assert_eq!(
        interrupted.stop_reason.as_deref(),
        Some("stop_cleanup_unresolved")
    );
    assert_eq!(interrupted.output_text, "partial cleanup tail");
    let interrupted_message = discussion(&project, &access)
        .messages
        .into_iter()
        .find(|message| message.role == DiscussionMessageRole::Assistant)
        .expect("unresolved stop keeps a terminal explanation");
    assert!(interrupted_message.content.contains("partial cleanup tail"));
    assert!(
        interrupted_message
            .content
            .to_ascii_lowercase()
            .contains("interrupted")
    );
    assert!(
        project
            .proposals(access, "chapter-one".into())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn empty_stop_settlement_keeps_terminal_explanation() {
    let temp = TempDir::new("stopping-empty");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = start(&project, &access, &document, "stopping-empty-run");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    project
        .stop_discussion(access.clone(), owner.run_id.clone())
        .expect("stop intent");
    let stopped = settle_stop(
        &project,
        &owner,
        "0",
        "empty-settlement",
        "",
        DiscussionStopCleanup::Settled,
    );
    assert_eq!(stopped.status, DiscussionRunStatus::Stopped);
    assert_eq!(stopped.sequence, "1");
    assert!(stopped.output_text.is_empty());
    let message = discussion(&project, &access)
        .messages
        .into_iter()
        .find(|message| message.role == DiscussionMessageRole::Assistant)
        .expect("empty settlement keeps a terminal explanation");
    assert!(message.content.to_ascii_lowercase().contains("stopp"));
}

#[test]
fn completion_wins_when_stop_arrives_after_terminal_commit() {
    let temp = TempDir::new("stopping-complete");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = start(&project, &access, &document, "stopping-complete-run");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    let completed = finish(&project, &owner, "0", "complete", "complete answer");
    assert_eq!(completed.status, DiscussionRunStatus::Completed);
    assert_eq!(completed.sequence, "1");
    let before = discussion(&project, &access);
    let stopped = project
        .stop_discussion(access.clone(), owner.run_id.clone())
        .expect("stop after completion is idempotent")
        .run;
    assert_eq!(stopped.status, DiscussionRunStatus::Completed);
    assert_eq!(stopped.sequence, "1");
    let after = discussion(&project, &access);
    assert_eq!(after.messages.len(), before.messages.len());
    for (after_message, before_message) in after.messages.iter().zip(before.messages.iter()) {
        assert_eq!(after_message.id, before_message.id);
        assert_eq!(after_message.content, before_message.content);
    }
}

#[test]
fn failed_stop_settlement_retains_stopping_prefix_and_finish_rejects_replacement() {
    let temp = TempDir::new("stopping-rollback");
    let (project, access, document) = setup_project(&temp.child("project"));
    let started = start(&project, &access, &document, "stopping-rollback-run");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    append(&project, &owner, "0", "partial", "prefix");
    let error = project
        .finish_discussion(DiscussionFinish {
            owner: owner.clone(),
            expected_sequence: "1".into(),
            event_id: "replace".into(),
            assistant_text: "replacement".into(),
        })
        .expect_err("terminal completion cannot replace persisted output");
    assert_eq!(error.code, "OutputConflict");
    project
        .stop_discussion(access.clone(), owner.run_id.clone())
        .expect("stop intent");
    Connection::open(project.path.join("project.sqlite3"))
        .expect("open rollback database")
        .execute_batch(
            "CREATE TRIGGER fail_stopped_message BEFORE INSERT ON discussion_messages
             WHEN NEW.role='assistant'
             BEGIN SELECT RAISE(ABORT,'injected stopped message failure'); END;",
        )
        .expect("install stopped message fault");
    let error = project
        .settle_discussion_stop(DiscussionStopSettled {
            owner: owner.clone(),
            expected_sequence: "1".into(),
            event_id: "rollback-settlement".into(),
            assistant_text: "prefix tail".into(),
            cleanup: DiscussionStopCleanup::Settled,
        })
        .expect_err("settlement message and run must commit atomically");
    assert_eq!(error.code, "PersistenceUnavailable");
    let after = discussion(&project, &access);
    assert_eq!(after.runs[0].status, DiscussionRunStatus::Stopping);
    assert_eq!(after.runs[0].sequence, "1");
    assert_eq!(after.runs[0].output_text, "prefix");
    Connection::open(project.path.join("project.sqlite3"))
        .expect("reopen rollback database")
        .execute_batch("DROP TRIGGER fail_stopped_message;")
        .expect("remove stopped message fault");
    let settled = settle_stop(
        &project,
        &owner,
        "1",
        "rollback-settlement",
        "prefix tail",
        DiscussionStopCleanup::Settled,
    );
    assert_eq!(settled.status, DiscussionRunStatus::Stopped);
}

#[test]
fn stopping_run_becomes_interrupted_on_reopen() {
    let temp = TempDir::new("stopping-recovery");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let started = start(&project, &access, &document, "stopping-recovery-run");
    let owner = started.run.owner.clone();
    begin(&project, &owner);
    append(&project, &owner, "0", "partial", "prefix");
    project
        .stop_discussion(access, owner.run_id.clone())
        .expect("stop intent");
    let archive = temp.child("stopping-recovery.wnsbackup");
    create_backup(&project, &archive).expect("backup stopping project");
    let recovered = recover_backup(&archive, &temp.child("stopping-copy"), "Copy")
        .expect("recover stopping project");
    let recovered_access = recovered.attach("stopping-copy-session".into()).unwrap();
    let error = recovered
        .settle_discussion_stop(DiscussionStopSettled {
            owner: owner.clone(),
            expected_sequence: "1".into(),
            event_id: "foreign-settlement".into(),
            assistant_text: "prefix".into(),
            cleanup: DiscussionStopCleanup::Settled,
        })
        .expect_err("a recovered copy cannot acknowledge the original stop");
    assert_eq!(error.code, "DiscussionProjectMismatch");
    drop(recovered_access);
    drop(recovered);
    drop(project);
    let reopened = ProjectSession::open(&path).expect("reopen stopping project");
    let reopened_access = reopened.attach("stopping-recovery-session".into()).unwrap();
    let view = discussion(&reopened, &reopened_access);
    assert_eq!(view.runs[0].status, DiscussionRunStatus::Interrupted);
    assert_eq!(view.runs[0].output_text, "prefix");
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
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            previous_run_id: None,
            lookup: None,
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
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            previous_run_id: None,
            lookup: None,
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
    request.safe_brief = draft.safe_brief;
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
        basis: None,
        scope: retry.scope,
        pinned_document_ids: retry.pinned_document_ids,
        safe_brief: None,
        previous_run_id: Some(first.run.id.clone()),
        lookup: None,
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
    legacy_schema::remove_schema19_features(&connection).unwrap();
    connection
        .execute_batch(
            "DROP TABLE snapshot_navigation_views; DROP TABLE memory_view_sources; DROP TABLE memory_views; DROP TABLE memory_results; DROP TABLE memory_jobs; ALTER TABLE snapshot_sources DROP COLUMN reader_position;
             DROP TRIGGER review_stages_no_update;
             DROP TRIGGER review_stages_no_delete;
             DROP TRIGGER ready_bundles_no_update;
             DROP TRIGGER ready_bundles_no_delete;
             DROP TRIGGER review_fences_no_update;
             DROP TRIGGER review_fences_no_delete;
             DROP TABLE review_fences;
             DROP TABLE ready_heads;
             DROP TABLE ready_bundles;
             DROP TABLE review_stages;
             DROP TRIGGER source_pin_receipts_no_update;
             DROP TRIGGER source_pin_receipts_no_delete;
             DROP TABLE source_pin_receipts;
             DROP TABLE source_pin_sets;
             DROP TABLE export_records;
             DROP TABLE provider_results;
             DROP TABLE import_manifest;
             DROP TABLE import_id_map;
             DROP TABLE import_body_decisions;
             DROP TABLE import_legacy_records;
             DROP TRIGGER discussion_lookup_results_no_update;
             DROP TRIGGER discussion_lookup_results_no_delete;
             DROP TRIGGER discussion_lookup_reads_no_update;
             DROP TRIGGER discussion_lookup_reads_no_delete;
             DROP TABLE discussion_lookup_reads;
             DROP TABLE discussion_lookup_results;
             DROP TABLE discussion_lookup_invocations;
             ALTER TABLE discussion_drafts DROP COLUMN safe_brief_json;
             DROP TRIGGER command_receipts_no_proposal_collision;
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
            .starts_with("schema6-before-schema37-")
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
            basis: None,
            scope: retry.scope.clone(),
            pinned_document_ids: retry.pinned_document_ids.clone(),
            safe_brief: None,
            previous_run_id: Some(first.run.id.clone()),
            lookup: None,
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
        basis: None,
        scope: retry.scope.clone(),
        pinned_document_ids: retry.pinned_document_ids.clone(),
        safe_brief: None,
        previous_run_id: Some(first.run.id.clone()),
        lookup: None,
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

#[test]
fn safe_brief_is_exact_restricted_packet_direction_without_origin_metadata() {
    let temp = TempDir::new("safe-brief-packet");
    let (project, access, document) = setup_project(&temp.child("project"));
    completed_turn(
        &project,
        &access,
        &document,
        "safe-brief-origin",
        "An earlier answer from the author room.",
    );
    let origin_message = discussion(&project, &access)
        .messages
        .into_iter()
        .find(|message| matches!(message.role, DiscussionMessageRole::Assistant))
        .expect("completed origin has an assistant message");
    let text = "Keep the selected exchange restrained and end on the unanswered threat.";
    let mut request = start_request(
        &access,
        &document,
        "safe-brief-start",
        "Revise the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    request.intent = FeedbackIntent::ProposeEdits;
    request.safe_brief = Some(safe_brief(text, Some(origin_message.id.clone()), true));

    let result = project.start_discussion(request).expect("safe brief start");
    let receipt = result.packet.receipt.safe_brief.as_ref().unwrap();
    assert_eq!(receipt.text, text);
    assert_eq!(receipt.text_hash, sha256_hex(text.as_bytes()));
    assert_eq!(
        receipt.origin_message_id.as_deref(),
        Some(origin_message.id.as_str())
    );
    assert!(
        result.packet.messages[0]
            .content
            .contains("author direction")
    );
    let envelope: Value = serde_json::from_str(&result.packet.messages[1].content).unwrap();
    assert_eq!(envelope["approvedWritingBrief"], text);
    assert!(envelope.get("originMessageId").is_none());
    assert!(
        !result.packet.messages[1]
            .content
            .contains(&origin_message.id)
    );
}

#[test]
fn safe_brief_rejects_unconfirmed_wrong_mode_and_foreign_origin_without_writes() {
    let temp = TempDir::new("safe-brief-rejections");
    let (project, access, document) = setup_project(&temp.child("project"));
    let before = counts(&project);

    let mut unconfirmed = start_request(
        &access,
        &document,
        "safe-brief-unconfirmed",
        "Revise the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    unconfirmed.intent = FeedbackIntent::ProposeEdits;
    unconfirmed.safe_brief = Some(safe_brief("Keep the ending quiet.", None, false));
    assert_eq!(
        project.start_discussion(unconfirmed).unwrap_err().code,
        "InvalidSafeBrief"
    );
    assert_eq!(counts(&project), before);

    let mut wrong_mode = start_request(
        &access,
        &document,
        "safe-brief-wrong-mode",
        "Discuss the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    wrong_mode.safe_brief = Some(safe_brief("Keep the ending quiet.", None, true));
    assert_eq!(
        project.start_discussion(wrong_mode).unwrap_err().code,
        "InvalidSafeBrief"
    );
    assert_eq!(counts(&project), before);

    let other_temp = TempDir::new("safe-brief-foreign");
    let (other, other_access, other_document) = setup_project(&other_temp.child("project"));
    let foreign_origin = start(&other, &other_access, &other_document, "foreign-origin");
    let mut foreign = start_request(
        &access,
        &document,
        "safe-brief-foreign-origin",
        "Revise the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    foreign.intent = FeedbackIntent::ProposeEdits;
    foreign.safe_brief = Some(safe_brief(
        "Keep the ending quiet.",
        Some(foreign_origin.user_message.id),
        true,
    ));
    assert_eq!(
        project.start_discussion(foreign).unwrap_err().code,
        "InvalidSafeBrief"
    );
    assert_eq!(counts(&project), before);
}

#[test]
fn safe_brief_draft_retains_unconfirmed_text_across_reopen() {
    let temp = TempDir::new("safe-brief-draft");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let draft_brief = safe_brief("", None, false);
    let saved = project
        .save_discussion_draft(SaveDiscussionDraft {
            access: access.clone(),
            operation_id: "safe-brief-draft".into(),
            document_id: document.head.document_id.clone(),
            expected_version: "0".into(),
            text: "An unsubmitted revision note".into(),
            intent: FeedbackIntent::ProposeEdits,
            basis: None,
            scope: Some(scope_input(&document)),
            pinned_document_ids: Vec::new(),
            safe_brief: Some(draft_brief.clone()),
            previous_run_id: None,
            lookup: None,
        })
        .expect("save editable safe brief draft");
    assert_eq!(saved.safe_brief, Some(draft_brief.clone()));

    drop(project);
    let reopened = ProjectSession::open(path).expect("reopen draft project");
    let reopened_access = reopened.attach("safe-brief-draft-reopen".into()).unwrap();
    let view = reopened
        .read_discussion(reopened_access, document.head.document_id)
        .unwrap();
    assert_eq!(view.draft.unwrap().safe_brief, Some(draft_brief));
}

#[test]
fn safe_brief_retry_preserves_exact_approval_and_rejects_changed_brief() {
    let temp = TempDir::new("safe-brief-retry");
    let (project, access, document) = setup_project(&temp.child("project"));
    let origin = start(&project, &access, &document, "safe-brief-retry-origin");
    let brief = safe_brief(
        "Keep the protagonist's answer indirect.",
        Some(origin.user_message.id),
        true,
    );
    let mut request = start_request(
        &access,
        &document,
        "safe-brief-retry-first",
        "Revise the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    request.intent = FeedbackIntent::ProposeEdits;
    request.safe_brief = Some(brief.clone());
    let first = project.start_discussion(request).unwrap();
    project
        .stop_discussion(access.clone(), first.run.id.clone())
        .unwrap();

    let retry = project
        .discussion_retry(access.clone(), first.run.id.clone())
        .unwrap();
    assert_eq!(retry.safe_brief, Some(brief.clone()));
    let mut changed = retry_request(
        &project,
        &access,
        &document,
        &first.run.id,
        "safe-brief-retry-changed",
    );
    changed.safe_brief = Some(safe_brief(
        "Use a direct answer instead.",
        brief.origin_message_id.clone(),
        true,
    ));
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
            "safe-brief-retry",
        ))
        .unwrap();
    assert_eq!(
        retried.packet.receipt.safe_brief,
        first.packet.receipt.safe_brief
    );
}

#[test]
fn safe_brief_replay_survives_policy_bump_but_new_origin_use_does_not() {
    let temp = TempDir::new("safe-brief-policy-replay");
    let (project, access, document) = setup_project(&temp.child("project"));
    let origin = start(&project, &access, &document, "safe-brief-policy-origin");
    let mut request = start_request(
        &access,
        &document,
        "safe-brief-policy-start",
        "Revise the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    request.intent = FeedbackIntent::ProposeEdits;
    request.safe_brief = Some(safe_brief(
        "Keep the unanswered threat at the end.",
        Some(origin.user_message.id.clone()),
        true,
    ));
    let first = project.start_discussion(request.clone()).unwrap();
    project
        .revoke_story_context(access.clone(), "0".into())
        .unwrap();

    let replay = project
        .start_discussion(request)
        .expect("same operation receipt must remain replayable");
    assert_eq!(replay.run.id, first.run.id);
    assert_eq!(replay.packet, first.packet);

    let mut fresh = start_request(
        &access,
        &document,
        "safe-brief-policy-fresh",
        "Revise the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    fresh.intent = FeedbackIntent::ProposeEdits;
    fresh.safe_brief = Some(safe_brief(
        "Keep the unanswered threat at the end.",
        Some(origin.user_message.id),
        true,
    ));
    assert_eq!(
        project.start_discussion(fresh).unwrap_err().code,
        "ContextPolicyChanged"
    );
}

#[test]
fn safe_brief_backup_retains_history_but_old_origin_cannot_authorize_recovery() {
    let temp = TempDir::new("safe-brief-recovery");
    let source_path = temp.child("source");
    let (source, access, document) = setup_project(&source_path);
    let origin = start(&source, &access, &document, "safe-brief-recovery-origin");
    let mut request = start_request(
        &access,
        &document,
        "safe-brief-recovery-start",
        "Revise the selected passage.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    request.intent = FeedbackIntent::ProposeEdits;
    request.safe_brief = Some(safe_brief(
        "Keep the final image unresolved.",
        Some(origin.user_message.id.clone()),
        true,
    ));
    let started = source.start_discussion(request).unwrap();
    let archive = temp.child("safe-brief.wnsbackup");
    create_backup(&source, &archive).unwrap();

    let recovered = recover_backup(&archive, &temp.child("recovered"), "Recovered brief")
        .expect("recover safe brief history");
    let recovered_access = recovered.attach("safe-brief-recovered".into()).unwrap();
    let packet_json: String = Connection::open(recovered.path.join("project.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT packet_json FROM context_packets WHERE id=?",
            [&started.packet.receipt.packet_id],
            |row| row.get(0),
        )
        .unwrap();
    let packet: Value = serde_json::from_str(&packet_json).unwrap();
    assert_eq!(
        packet["receipt"]["safeBrief"]["text"],
        "Keep the final image unresolved."
    );
    let recovered_document = recovered
        .document(recovered_access.clone(), document.head.document_id.clone())
        .unwrap();
    let mut copied_request = start_request(
        &recovered_access,
        &recovered_document,
        "safe-brief-recovery-new",
        "Revise the selected passage.",
        Some(scope_input(&recovered_document)),
        Vec::new(),
    );
    copied_request.intent = FeedbackIntent::ProposeEdits;
    copied_request.safe_brief = Some(safe_brief(
        "Keep the final image unresolved.",
        Some(origin.user_message.id),
        true,
    ));
    assert_eq!(
        recovered.start_discussion(copied_request).unwrap_err().code,
        "InvalidSafeBrief"
    );
}

#[test]
fn bounded_provider_completion_persists_binding_usage_and_replays_after_restart() {
    let temp = TempDir::new("provider-complete");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let binding = ProviderBinding::codex_luna();
    let mut request = start_request(
        &access,
        &document,
        "provider-complete-start",
        "Answer about the selected chapter.",
        None,
        Vec::new(),
    );
    request.provider_binding = Some(binding.clone());
    let started = project
        .start_discussion(request)
        .expect("start live packet");
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .expect("claim live packet");
    let delivered_error = project
        .mark_discussion_delivered(started.run.owner.clone())
        .expect_err("live delivery must require a typed provider result");
    assert_eq!(delivered_error.code, "ProviderResultRequired");
    let finish_error = project
        .finish_discussion(DiscussionFinish {
            owner: started.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: "provider-finish-forbidden".into(),
            assistant_text: "A complete bounded answer.".into(),
        })
        .expect_err("live completion must require a typed provider result");
    assert_eq!(finish_error.code, "ProviderResultRequired");
    let still_running = project
        .read_discussion_run(started.run.owner.clone())
        .expect("read live run after rejected legacy methods");
    assert_eq!(still_running.status, DiscussionRunStatus::Running);
    assert_eq!(still_running.sequence, "0");
    let stdin_bytes = serialized_input(&dispatch.packet.messages, &dispatch.packet.options)
        .unwrap()
        .len()
        .to_string();
    let report = ProviderTerminalReport {
        owner: started.run.owner.clone(),
        expected_sequence: "0".into(),
        event_id: "provider-terminal-complete".into(),
        assistant_text: "A complete bounded answer.".into(),
        binding: binding.clone(),
        status: ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: stdin_bytes,
        usage: Some(ProviderUsage {
            input_tokens: 12,
            cached_input_tokens: 2,
            cache_write_input_tokens: 0,
            output_tokens: 7,
            reasoning_output_tokens: 3,
        }),
        cleanup: ProviderCleanup::Settled,
        error: None,
        effective_identity: None,
        reported_model: None,
        delivery: None,
    };
    let settled = project
        .settle_provider_discussion(report.clone())
        .expect("settle provider completion");
    let replay = project
        .settle_provider_discussion(report)
        .expect("replay provider completion after a lost acknowledgment");
    assert_eq!(replay.run.id, settled.run.id);
    assert_eq!(replay.provider_result, settled.provider_result);
    assert_eq!(settled.run.status, DiscussionRunStatus::Completed);
    assert_eq!(settled.run.provider_binding, Some(binding.clone()));
    assert_eq!(
        settled
            .run
            .provider_result
            .as_ref()
            .unwrap()
            .usage
            .as_ref()
            .unwrap()
            .output_tokens,
        7
    );
    drop(project);
    let reopened = ProjectSession::open(&path).expect("reopen provider project");
    let reopened_access = reopened.attach("provider-reopen".into()).unwrap();
    let view = reopened
        .read_discussion(reopened_access, "chapter-one".into())
        .unwrap();
    let run = view.runs.last().unwrap();
    assert_eq!(run.status, DiscussionRunStatus::Completed);
    assert_eq!(run.provider_binding, Some(binding));
    assert_eq!(run.output_text, "A complete bounded answer.");
}

#[test]
fn claude_completion_requires_and_persists_the_exact_reported_model() {
    let temp = TempDir::new("claude-provider-complete");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let binding = ProviderBinding::claude_author_runtime(
        "claude-sonnet-5",
        "high",
        "2.1.220",
        &"a".repeat(64),
    );
    let mut request = start_request(
        &access,
        &document,
        "claude-provider-complete-start",
        "Answer about the selected chapter.",
        None,
        Vec::new(),
    );
    request.provider_binding = Some(binding.clone());
    let started = project.start_discussion(request).unwrap();
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .unwrap();
    let stdin_bytes = serialized_input(&dispatch.packet.messages, &dispatch.packet.options)
        .unwrap()
        .len()
        .to_string();
    let settled = project
        .settle_provider_discussion(ProviderTerminalReport {
            owner: started.run.owner,
            expected_sequence: "0".into(),
            event_id: "claude-provider-terminal-complete".into(),
            assistant_text: "A complete Claude answer.".into(),
            binding,
            status: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: stdin_bytes,
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: None,
            effective_identity: None,
            reported_model: Some("claude-sonnet-5".into()),
            delivery: None,
        })
        .expect("Claude completion with exact model");
    assert_eq!(
        settled.provider_result.reported_model.as_deref(),
        Some("claude-sonnet-5")
    );
    drop(project);
    let reopened = ProjectSession::open(&path).unwrap();
    let reopened_access = reopened.attach("claude-provider-reopen".into()).unwrap();
    let reopened_view = reopened
        .read_discussion(reopened_access, "chapter-one".into())
        .unwrap();
    let run = reopened_view.runs.last().unwrap();
    assert_eq!(
        run.provider_result
            .as_ref()
            .and_then(|result| result.reported_model.as_deref()),
        Some("claude-sonnet-5")
    );
}

#[test]
fn claude_completion_rejects_missing_or_mismatched_reported_model() {
    for (label, reported_model) in [("missing", None), ("mismatched", Some("claude-opus-5"))] {
        let temp = TempDir::new(&format!("claude-provider-{label}"));
        let (project, access, document) = setup_project(&temp.child("project"));
        let binding = ProviderBinding::claude_author_runtime(
            "claude-sonnet-5",
            "high",
            "2.1.220",
            &"b".repeat(64),
        );
        let mut request = start_request(
            &access,
            &document,
            &format!("claude-provider-{label}-start"),
            "Answer about the selected chapter.",
            None,
            Vec::new(),
        );
        request.provider_binding = Some(binding.clone());
        let started = project.start_discussion(request).unwrap();
        let dispatch = project
            .begin_discussion_run(DiscussionBegin {
                owner: started.run.owner.clone(),
            })
            .unwrap();
        let stdin_bytes = serialized_input(&dispatch.packet.messages, &dispatch.packet.options)
            .unwrap()
            .len()
            .to_string();
        let error = project
            .settle_provider_discussion(ProviderTerminalReport {
                owner: started.run.owner,
                expected_sequence: "0".into(),
                event_id: format!("claude-provider-terminal-{label}"),
                assistant_text: "A complete Claude answer.".into(),
                binding,
                status: ProviderOutcomeStatus::Completed,
                confirmed_stdin_bytes: stdin_bytes,
                usage: None,
                cleanup: ProviderCleanup::Settled,
                error: None,
                effective_identity: None,
                reported_model: reported_model.map(str::to_owned),
                delivery: None,
            })
            .expect_err("invalid Claude completion identity");
        assert_eq!(error.code, "InvalidRequest");
    }
}

#[test]
fn failed_claude_result_retains_requested_and_reported_models() {
    let temp = TempDir::new("claude-provider-failed-model");
    let (project, access, document) = setup_project(&temp.child("project"));
    let binding = ProviderBinding::claude_author_runtime(
        "claude-sonnet-5",
        "high",
        "2.1.220",
        &"c".repeat(64),
    );
    let mut request = start_request(
        &access,
        &document,
        "claude-provider-failed-model-start",
        "Answer about the selected chapter.",
        None,
        Vec::new(),
    );
    request.provider_binding = Some(binding.clone());
    let started = project.start_discussion(request).unwrap();
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .unwrap();
    let stdin_bytes = serialized_input(&dispatch.packet.messages, &dispatch.packet.options)
        .unwrap()
        .len()
        .to_string();
    let settled = project
        .settle_provider_discussion(ProviderTerminalReport {
            owner: started.run.owner,
            expected_sequence: "0".into(),
            event_id: "claude-provider-terminal-failed-model".into(),
            assistant_text: "Partial Claude answer.".into(),
            binding: binding.clone(),
            status: ProviderOutcomeStatus::Failed,
            confirmed_stdin_bytes: stdin_bytes,
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: Some("The CLI reported a different model before failing.".into()),
            effective_identity: None,
            reported_model: Some("claude-next-5.1".into()),
            delivery: None,
        })
        .expect("failed Claude result may retain diagnosis identity");
    assert_eq!(settled.provider_result.binding.model_id, "claude-sonnet-5");
    assert_eq!(
        settled.provider_result.reported_model.as_deref(),
        Some("claude-next-5.1")
    );
    drop(project);
    let reopened = ProjectSession::open(temp.child("project")).unwrap();
    let reopened_access = reopened
        .attach("claude-provider-failed-model-reopen".into())
        .unwrap();
    let reopened_view = reopened
        .read_discussion(reopened_access, "chapter-one".into())
        .unwrap();
    let reopened_result = reopened_view
        .runs
        .last()
        .and_then(|run| run.provider_result.as_ref())
        .expect("failed Claude identity remains inspectable after reopen");
    assert_eq!(reopened_result.binding.model_id, "claude-sonnet-5");
    assert_eq!(
        reopened_result.reported_model.as_deref(),
        Some("claude-next-5.1")
    );
}

#[test]
fn claude_binding_is_allowed_for_scoped_proposals_before_apply() {
    let temp = TempDir::new("claude-provider-proposal");
    let (project, access, document) = setup_project(&temp.child("project"));
    let mut request = start_request(
        &access,
        &document,
        "claude-provider-proposal-start",
        "Make only this passage quieter while preserving the ending.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    request.intent = FeedbackIntent::ProposeEdits;
    request.provider_binding = Some(ProviderBinding::claude_author_runtime(
        "claude-sonnet-5",
        "high",
        "2.1.220",
        &"d".repeat(64),
    ));
    let started = project
        .start_discussion(request)
        .expect("Claude may prepare a scoped proposal");
    assert_eq!(started.run.intent, FeedbackIntent::ProposeEdits);
    assert_eq!(
        started.packet.options.provider_binding,
        started.run.provider_binding
    );
    let envelope: Value = serde_json::from_str(&started.packet.messages[1].content).unwrap();
    assert!(envelope.get("scope").is_some_and(|scope| !scope.is_null()));
}

#[test]
fn http_provider_completion_persists_delivery_body_and_reopens_without_stdin_claim() {
    let temp = TempDir::new("http-provider-complete");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let binding = http_binding(HttpResponseFormat::Text);
    let mut request = start_request(
        &access,
        &document,
        "http-provider-complete-start",
        "Answer about the selected chapter.",
        None,
        Vec::new(),
    );
    request.provider_binding = Some(binding.clone());
    let started = project.start_discussion(request).unwrap();
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .unwrap();
    let report = ProviderTerminalReport {
        owner: started.run.owner.clone(),
        expected_sequence: "0".into(),
        event_id: "http-provider-terminal-complete".into(),
        assistant_text: "A complete HTTP answer.".into(),
        binding: binding.clone(),
        status: ProviderOutcomeStatus::Completed,
        confirmed_stdin_bytes: "0".into(),
        usage: None,
        cleanup: ProviderCleanup::Settled,
        error: None,
        effective_identity: None,
        reported_model: None,
        delivery: Some(http_delivery(
            &dispatch,
            HttpDeliverySubmission::ResponseReceived,
        )),
    };
    let settled = project
        .settle_provider_discussion(report.clone())
        .expect("settle HTTP provider completion");
    let replay = project
        .settle_provider_discussion(report)
        .expect("replay HTTP provider completion");
    assert_eq!(replay.provider_result, settled.provider_result);
    assert_eq!(settled.provider_result.confirmed_stdin_bytes, "0");
    assert_eq!(
        settled
            .provider_result
            .delivery
            .as_ref()
            .unwrap()
            .body_bytes,
        prepare_request(&dispatch.packet.messages, &dispatch.packet.options)
            .unwrap()
            .body
            .len()
            .to_string()
    );
    drop(project);
    let reopened = ProjectSession::open(&path).expect("reopen HTTP provider project");
    let reopened_access = reopened.attach("http-provider-reopen".into()).unwrap();
    let run = reopened
        .read_discussion(reopened_access, "chapter-one".into())
        .unwrap()
        .runs
        .last()
        .unwrap()
        .clone();
    assert_eq!(run.status, DiscussionRunStatus::Completed);
    assert_eq!(run.output_text, "A complete HTTP answer.");
}

#[test]
fn http_provider_rejects_tampered_body_and_seals_uncertain_delivery_history() {
    let temp = TempDir::new("http-provider-uncertain");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let binding = http_binding(HttpResponseFormat::JsonObject);
    let mut complete_request = start_request(
        &access,
        &document,
        "http-provider-tamper-start",
        "Return a structured proposal.",
        Some(scope_input(&document)),
        Vec::new(),
    );
    complete_request.intent = FeedbackIntent::ProposeEdits;
    complete_request.provider_binding = Some(binding.clone());
    let started = project.start_discussion(complete_request).unwrap();
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .unwrap();
    let mut tampered = http_delivery(&dispatch, HttpDeliverySubmission::ResponseReceived);
    tampered.body_hash = "00".repeat(32);
    let error = project
        .settle_provider_discussion(ProviderTerminalReport {
            owner: started.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: "http-provider-tampered".into(),
            assistant_text: "{}".into(),
            binding: binding.clone(),
            status: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: "0".into(),
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: None,
            effective_identity: None,
            reported_model: None,
            delivery: Some(tampered),
        })
        .unwrap_err();
    assert_eq!(error.code, "ProviderInputMismatch");

    let mut uncertain_request = start_request(
        &access,
        &document,
        "http-provider-uncertain-start",
        "Discuss the selected chapter.",
        None,
        Vec::new(),
    );
    uncertain_request.provider_binding = Some(binding.clone());
    let uncertain_started = project.start_discussion(uncertain_request).unwrap();
    let uncertain_dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: uncertain_started.run.owner.clone(),
        })
        .unwrap();
    let settled = project
        .settle_provider_discussion(ProviderTerminalReport {
            owner: uncertain_started.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: "http-provider-uncertain".into(),
            assistant_text: "Partial answer".into(),
            binding,
            status: ProviderOutcomeStatus::Failed,
            confirmed_stdin_bytes: "0".into(),
            usage: None,
            cleanup: ProviderCleanup::Unresolved,
            error: Some("The response became unreachable after submission.".into()),
            effective_identity: None,
            reported_model: None,
            delivery: Some(http_delivery(
                &uncertain_dispatch,
                HttpDeliverySubmission::Uncertain,
            )),
        })
        .unwrap();
    assert_eq!(settled.run.status, DiscussionRunStatus::Interrupted);
    drop(project);
    let reopened = ProjectSession::open(&path).expect("reopen uncertain HTTP project");
    let reopened_access = reopened
        .attach("http-provider-uncertain-reopen".into())
        .unwrap();
    let reopened_view = reopened
        .read_discussion(reopened_access, "chapter-one".into())
        .unwrap();
    let run = reopened_view.runs.last().unwrap();
    assert_eq!(run.status, DiscussionRunStatus::Interrupted);
    assert_eq!(
        run.provider_result.as_ref().unwrap().confirmed_stdin_bytes,
        "0"
    );
    assert_eq!(
        run.provider_result
            .as_ref()
            .unwrap()
            .delivery
            .as_ref()
            .unwrap()
            .submission,
        HttpDeliverySubmission::Uncertain
    );
}

#[test]
fn unresolved_provider_cleanup_interrupts_and_accepts_partial_stdin_without_proposals() {
    let temp = TempDir::new("provider-unresolved");
    let (project, access, document) = setup_project(&temp.child("project"));
    let binding = ProviderBinding::codex_luna();
    let mut request = start_request(
        &access,
        &document,
        "provider-unresolved-start",
        "Answer about the selected chapter.",
        None,
        Vec::new(),
    );
    request.provider_binding = Some(binding.clone());
    let started = project.start_discussion(request).unwrap();
    project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .unwrap();
    let prefix = append(
        &project,
        &started.run.owner,
        "0",
        "provider-prefix",
        "Partial ",
    );
    let settled = project
        .settle_provider_discussion(ProviderTerminalReport {
            owner: started.run.owner.clone(),
            expected_sequence: prefix.sequence,
            event_id: "provider-terminal-unresolved".into(),
            assistant_text: "Partial ".into(),
            binding,
            status: ProviderOutcomeStatus::TimedOut,
            confirmed_stdin_bytes: "0".into(),
            usage: None,
            cleanup: ProviderCleanup::Unresolved,
            error: Some("cleanup could not be confirmed".into()),
            effective_identity: None,
            reported_model: None,
            delivery: None,
        })
        .expect("settle unresolved provider result");
    assert_eq!(settled.run.status, DiscussionRunStatus::Interrupted);
    assert_eq!(settled.run.output_text, "Partial ");
    assert!(settled.run.provider_result.is_some());
    assert!(
        project
            .proposals(access, "chapter-one".into())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn live_proposal_discussion_freezes_strict_response_contract_in_packet() {
    let temp = TempDir::new("provider-proposal-contract");
    let (project, access, document) = setup_project(&temp.child("project"));
    let instruction = "Make only this passage quieter while preserving the author's exact ending.";
    let mut request = start_request(
        &access,
        &document,
        "provider-proposal-contract-start",
        instruction,
        Some(scope_input(&document)),
        Vec::new(),
    );
    request.intent = FeedbackIntent::ProposeEdits;
    request.provider_binding = Some(ProviderBinding::codex_luna());

    let started = project
        .start_discussion(request)
        .expect("start live proposal discussion");
    assert!(
        started.packet.messages[0]
            .content
            .contains(PROPOSAL_RESPONSE_CONTRACT)
    );
    assert!(
        started.packet.messages[0]
            .content
            .contains("replacementText")
    );
    assert_eq!(started.packet.messages[2].content, instruction);

    let reopened_packet = project
        .prepared_context(access, started.packet.receipt.packet_id.clone())
        .expect("reopen frozen proposal packet");
    assert_eq!(reopened_packet, started.packet);
    assert_eq!(
        reopened_packet.options.provider_binding,
        Some(ProviderBinding::codex_luna())
    );
}

#[test]
fn tampered_provider_result_is_rejected_by_backup_validation() {
    let temp = TempDir::new("provider-result-backup-tamper");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let binding = ProviderBinding::codex_luna();
    let mut request = start_request(
        &access,
        &document,
        "provider-result-backup-tamper-start",
        "Answer about the selected chapter.",
        None,
        Vec::new(),
    );
    request.provider_binding = Some(binding.clone());
    let started = project.start_discussion(request).unwrap();
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .unwrap();
    let stdin_bytes = serialized_input(&dispatch.packet.messages, &dispatch.packet.options)
        .unwrap()
        .len()
        .to_string();
    project
        .settle_provider_discussion(ProviderTerminalReport {
            owner: started.run.owner,
            expected_sequence: "0".into(),
            event_id: "provider-result-backup-tamper-terminal".into(),
            assistant_text: "A durable answer.".into(),
            binding,
            status: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: stdin_bytes,
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: None,
            effective_identity: None,
            reported_model: None,
            delivery: None,
        })
        .unwrap();
    drop(project);

    let database = path.join("project.sqlite3");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER provider_results_no_update;
             UPDATE provider_results SET assistant_text='tampered';",
        )
        .unwrap();
    drop(connection);

    let reopened = ProjectSession::open(&path).unwrap();
    let error = create_backup(&reopened, &temp.child("tampered.wnsbackup"))
        .expect_err("tampered provider result must reject backup");
    assert_eq!(error.code, "InvalidBackup");
}

#[test]
fn completed_live_run_without_provider_result_is_rejected_by_backup_validation() {
    let temp = TempDir::new("provider-result-backup-missing");
    let path = temp.child("project");
    let (project, access, document) = setup_project(&path);
    let binding = ProviderBinding::codex_luna();
    let mut request = start_request(
        &access,
        &document,
        "provider-result-backup-missing-start",
        "Answer about the selected chapter.",
        None,
        Vec::new(),
    );
    request.provider_binding = Some(binding.clone());
    let started = project.start_discussion(request).unwrap();
    let dispatch = project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .unwrap();
    let stdin_bytes = serialized_input(&dispatch.packet.messages, &dispatch.packet.options)
        .unwrap()
        .len()
        .to_string();
    project
        .settle_provider_discussion(ProviderTerminalReport {
            owner: started.run.owner,
            expected_sequence: "0".into(),
            event_id: "provider-result-backup-missing-terminal".into(),
            assistant_text: "A durable answer.".into(),
            binding,
            status: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: stdin_bytes,
            usage: None,
            cleanup: ProviderCleanup::Settled,
            error: None,
            effective_identity: None,
            reported_model: None,
            delivery: None,
        })
        .unwrap();
    drop(project);

    let database = path.join("project.sqlite3");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER provider_results_no_delete;
             DELETE FROM provider_results;",
        )
        .unwrap();
    drop(connection);

    let reopened = ProjectSession::open(&path).unwrap();
    let error = create_backup(&reopened, &temp.child("missing.wnsbackup"))
        .expect_err("completed live run without provider result must reject backup");
    assert_eq!(error.code, "InvalidBackup");
}
