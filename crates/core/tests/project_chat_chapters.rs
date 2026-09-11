use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::context::{BasisKind, ProjectBriefOrigin, SafeBriefInput};
use webnovel_core::documents::{Endpoint, ScopeKind};
use webnovel_core::projects::discussions::{
    DiscussionBegin, DiscussionFinish, DiscussionScopeInput, FeedbackIntent,
};
use webnovel_core::projects::project_chat::{
    ProjectChapterComposer, ProjectChatDraftRef, ProjectComposer, ReadProjectConversation,
    SaveProjectComposer, StartProjectChapter, StartProjectChat,
};
use webnovel_core::projects::{CreateDocument, Head, ProjectAccess, ProjectSession};

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-project-chat-chapter-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [{
            "type": "paragraph", "attrs": {"id": "p1"},
            "content": [{"type": "text", "text": text}]
        }]}
    })
}

fn setup() -> (Temp, ProjectSession, ProjectAccess, webnovel_core::projects::DocumentRecord) {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("project"), "Chapter chat fixture").unwrap();
    let access = project.documents().attach("chapter-chat-test".into()).unwrap();
    let chapter = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-1".into(),
            title: "Chapter 1".into(),
            kind: "chapter".into(),
            body: body("The gate opened."),
        })
        .unwrap();
    (temp, project, access, chapter)
}

fn conversation(project: &ProjectSession, access: &ProjectAccess) -> webnovel_core::projects::project_chat::ProjectConversation {
    project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .unwrap()
}

fn chapter_composer(
    chapter: &webnovel_core::projects::DocumentRecord,
    intent: FeedbackIntent,
) -> ProjectComposer {
    ProjectComposer {
        text: "Develop the next beat while preserving the chapter's voice.".into(),
        chapter: Some(ProjectChapterComposer {
            target: chapter.head.clone(),
            intent,
            basis: (intent == FeedbackIntent::Continue)
                .then_some(BasisKind::Working),
            scope: None,
            safe_brief: None,
        }),
        ..ProjectComposer::default()
    }
}

fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes.as_ref());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn selected_scope(chapter: &webnovel_core::projects::DocumentRecord) -> DiscussionScopeInput {
    DiscussionScopeInput {
        kind: ScopeKind::Passage,
        start: Some(Endpoint {
            block_id: "p1".into(),
            utf16_offset: 0,
        }),
        end: Some(Endpoint {
            block_id: "p1".into(),
            utf16_offset: 3,
        }),
        quote: "The".into(),
        source_body_hash: chapter.head.body_hash.clone(),
    }
}

fn save_chapter(
    project: &ProjectSession,
    access: &ProjectAccess,
    composer: ProjectComposer,
    operation_id: &str,
) -> StartProjectChapter {
    let view = conversation(project, access);
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: format!("{operation_id}-save"),
            conversation_id: view.id.clone(),
            expected_version: view.composer.version,
            body: composer.clone(),
        })
        .unwrap();
    StartProjectChapter {
        access: access.clone(),
        operation_id: operation_id.into(),
        conversation_id: view.id,
        expected_composer_version: saved.version,
        composer,
        budget: MockContextBudget::new("100000", "8192", "100"),
        provider_binding: None,
    }
}

fn complete(
    project: &ProjectSession,
    start: &webnovel_core::projects::discussions::DiscussionStart,
    assistant_text: &str,
    event_id: &str,
) {
    project
        .begin_discussion_run(DiscussionBegin {
            owner: start.run.owner.clone(),
        })
        .unwrap();
    project
        .mark_discussion_delivered(start.run.owner.clone())
        .unwrap();
    project
        .finish_discussion(DiscussionFinish {
            owner: start.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: event_id.into(),
            assistant_text: assistant_text.into(),
        })
        .unwrap();
}

fn start_root(
    project: &ProjectSession,
    access: &ProjectAccess,
    text: &str,
    operation_id: &str,
) -> webnovel_core::projects::discussions::DiscussionStart {
    let view = conversation(project, access);
    let composer = ProjectComposer {
        text: text.into(),
        ..ProjectComposer::default()
    };
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: format!("{operation_id}-save"),
            conversation_id: view.id.clone(),
            expected_version: view.composer.version,
            body: composer.clone(),
        })
        .unwrap();
    project
        .start_project_chat(StartProjectChat {
            access: access.clone(),
            operation_id: operation_id.into(),
            conversation_id: view.id,
            expected_composer_version: saved.version,
            composer,
            budget: MockContextBudget::new("100000", "8192", "100"),
            provider_binding: None,
        })
        .unwrap()
}

fn project_origin(
    access: &ProjectAccess,
    conversation_id: &str,
    message_id: &str,
    target: &Head,
    scope: &Option<DiscussionScopeInput>,
    text: &str,
) -> ProjectBriefOrigin {
    ProjectBriefOrigin {
        version: "project-conversation-brief.v1".into(),
        project_id: access.project_id.clone(),
        operation_namespace: access.operation_namespace.clone(),
        conversation_id: conversation_id.into(),
        message_id: message_id.into(),
        target: target.clone(),
        scope_hash: sha256_hex(serde_json::to_vec(scope).unwrap()),
        text_hash: sha256_hex(text.as_bytes()),
    }
}

#[test]
fn chapter_request_links_timeline_clears_composer_and_replays_idempotently() {
    let (_temp, project, access, chapter) = setup();
    let view = conversation(&project, &access);
    let composer = chapter_composer(&chapter, FeedbackIntent::Continue);
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: "chapter-composer".into(),
            conversation_id: view.id.clone(),
            expected_version: view.composer.version,
            body: composer.clone(),
        })
        .unwrap();
    let request = StartProjectChapter {
        access: access.clone(),
        operation_id: "chapter-start".into(),
        conversation_id: view.id.clone(),
        expected_composer_version: saved.version,
        composer,
        budget: MockContextBudget::new("100000", "8192", "100"),
        provider_binding: None,
    };
    let first = project.start_project_chapter(request.clone()).unwrap();
    let replay = project.start_project_chapter(request).unwrap();
    assert_eq!(first.run.id, replay.run.id);
    assert_eq!(first.run.target.document_id, "chapter-1");
    let after = conversation(&project, &access);
    assert!(after.composer.body.chapter.is_none());
    assert_eq!(after.composer.version, "2");
    assert!(after.items.iter().any(|item| item.kind == "chapterRequest"));
}

#[test]
fn chapter_target_must_be_an_ordinary_chapter_and_exact_current_head() {
    let (_temp, project, access, chapter) = setup();
    let view = conversation(&project, &access);
    let mut composer = chapter_composer(&chapter, FeedbackIntent::Discuss);
    composer.chapter.as_mut().unwrap().target.document_id = "missing".into();
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: "bad-target-composer".into(),
            conversation_id: view.id.clone(),
            expected_version: view.composer.version,
            body: composer.clone(),
        })
        .unwrap();
    let error = project
        .start_project_chapter(StartProjectChapter {
            access,
            operation_id: "bad-target-start".into(),
            conversation_id: view.id,
            expected_composer_version: saved.version,
            composer,
            budget: MockContextBudget::new("100000", "8192", "100"),
            provider_binding: None,
        })
        .unwrap_err();
    assert!(matches!(error.code.as_str(), "DocumentNotFound" | "InvalidDocument"));
}

#[test]
fn selected_chapter_feedback_keeps_exact_scope_and_does_not_widen_discuss() {
    let (_temp, project, access, chapter) = setup();
    let view = conversation(&project, &access);
    let mut composer = chapter_composer(&chapter, FeedbackIntent::ProposeEdits);
    composer.chapter.as_mut().unwrap().scope = Some(webnovel_core::projects::discussions::DiscussionScopeInput {
        kind: ScopeKind::Passage,
        start: Some(Endpoint { block_id: "p1".into(), utf16_offset: 0 }),
        end: Some(Endpoint { block_id: "p1".into(), utf16_offset: 3 }),
        quote: "The".into(),
        source_body_hash: chapter.head.body_hash.clone(),
    });
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: "scope-composer".into(),
            conversation_id: view.id.clone(),
            expected_version: view.composer.version,
            body: composer.clone(),
        })
        .unwrap();
    let start = project
        .start_project_chapter(StartProjectChapter {
            access,
            operation_id: "scope-start".into(),
            conversation_id: view.id,
            expected_composer_version: saved.version,
            composer,
            budget: MockContextBudget::new("100000", "8192", "100"),
            provider_binding: None,
        })
        .unwrap();
    assert_eq!(start.run.target.document_id, chapter.head.document_id);
    let envelope: serde_json::Value =
        serde_json::from_str(&start.packet.messages[1].content).unwrap();
    assert_eq!(envelope["scope"]["start"]["blockId"], "p1");
}

#[test]
fn unscoped_chapter_discuss_projects_one_source_bound_range_without_granting_edit_scope() {
    let (_temp, project, access, chapter) = setup();
    let start = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            chapter_composer(&chapter, FeedbackIntent::Discuss),
            "unscoped-range-start",
        ))
        .unwrap();
    assert!(start.packet.messages[0]
        .content
        .contains("chapter-discussion-output.v1"));
    let response = serde_json::json!({
        "schemaVersion": "chapter-discussion-output.v1",
        "answer": "The opening has a clear pressure point.",
        "rangeProposal": {
            "sourceHead": chapter.head,
            "firstBlockId": "p1",
            "lastBlockId": "p1",
            "quote": "The gate opened."
        }
    });
    complete(
        &project,
        &start,
        &response.to_string(),
        "unscoped-range-finish",
    );
    let feedback = project
        .read_project_chapter_feedback(access.clone(), start.run.id.clone())
        .unwrap()
        .expect("new chapter Discuss output has a read projection");
    assert_eq!(feedback.run_id, start.run.id);
    assert_eq!(feedback.answer, "The opening has a clear pressure point.");
    let range = feedback.range_proposal.expect("valid range proposal");
    assert_eq!(range.first_block_id, "p1");
    assert_eq!(range.last_block_id, "p1");
    assert_eq!(range.quote, "The gate opened.");
    assert!(feedback.range_error.is_none());
}

#[test]
fn stale_chapter_range_keeps_feedback_but_never_returns_a_selectable_range() {
    let (_temp, project, access, chapter) = setup();
    let start = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            chapter_composer(&chapter, FeedbackIntent::Discuss),
            "stale-range-start",
        ))
        .unwrap();
    let response = serde_json::json!({
        "schemaVersion": "chapter-discussion-output.v1",
        "answer": "The answer remains readable even when the hint is stale.",
        "rangeProposal": {
            "sourceHead": chapter.head,
            "firstBlockId": "p1",
            "lastBlockId": "p1",
            "quote": "A different paragraph."
        }
    });
    complete(
        &project,
        &start,
        &response.to_string(),
        "stale-range-finish",
    );
    let feedback = project
        .read_project_chapter_feedback(access, start.run.id)
        .unwrap()
        .expect("answer projection survives stale range");
    assert_eq!(
        feedback.answer,
        "The answer remains readable even when the hint is stale."
    );
    assert!(feedback.range_proposal.is_none());
    assert!(feedback.range_error.is_some());
}

#[test]
fn malformed_chapter_range_is_ignored_without_replacing_the_raw_feedback() {
    let (_temp, project, access, chapter) = setup();
    let start = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            chapter_composer(&chapter, FeedbackIntent::Discuss),
            "malformed-range-start",
        ))
        .unwrap();
    let response = serde_json::json!({
        "schemaVersion": "chapter-discussion-output.v1",
        "answer": "Keep this feedback visible even though the range is unusable.",
        "rangeProposal": {
            "sourceHead": chapter.head,
            "firstBlockId": "",
            "lastBlockId": "p1",
            "quote": "The gate opened."
        }
    });
    complete(
        &project,
        &start,
        &response.to_string(),
        "malformed-range-finish",
    );
    let feedback = project
        .read_project_chapter_feedback(access.clone(), start.run.id.clone())
        .unwrap()
        .expect("malformed range keeps the readable answer projection");
    assert_eq!(
        feedback.answer,
        "Keep this feedback visible even though the range is unusable."
    );
    assert!(feedback.range_proposal.is_none());
    assert!(feedback.range_error.is_some());
    let view = conversation(&project, &access);
    let run = view
        .items
        .iter()
        .find(|item| item.reference_id.as_deref() == Some(start.run.id.as_str()))
        .and_then(|item| item.payload.get("run"))
        .expect("raw run remains readable in the chapter timeline");
    assert_eq!(run["outputText"], response.to_string());
}

#[test]
fn confirmed_project_brief_is_bound_to_project_message_and_restricted_packet() {
    let (_temp, project, access, chapter) = setup();
    let root = start_root(
        &project,
        &access,
        "Plan the confrontation without revealing the private cause.",
        "root-private-plan",
    );
    complete(
        &project,
        &root,
        "PRIVATE PLANNING SECRET: the mentor caused the massacre.",
        "root-private-finish",
    );

    let project_view = conversation(&project, &access);
    let conversation_id = project_view.id;
    let origin_message_id = project_view
        .items
        .iter()
        .find(|item| item.reference_id.as_deref() == Some(root.run.id.as_str()))
        .and_then(|item| item.payload["assistantMessageId"].as_str())
        .expect("completed project request exposes its assistant message")
        .to_owned();
    let scope = Some(selected_scope(&chapter));
    let brief_text = "Preserve the protagonist's uncertainty about the mentor.";
    let origin = project_origin(
        &access,
        &conversation_id,
        &origin_message_id,
        &chapter.head,
        &scope,
        brief_text,
    );
    let mut composer = chapter_composer(&chapter, FeedbackIntent::ProposeEdits);
    composer.chapter.as_mut().unwrap().scope = scope;
    composer.chapter.as_mut().unwrap().safe_brief = Some(SafeBriefInput {
        text: brief_text.into(),
        // The UI preserves the legacy message ID alongside the versioned
        // project origin. Both IDs must identify the same retained message.
        origin_message_id: Some(origin_message_id),
        confirmed: true,
        project_origin: Some(origin.clone()),
    });
    let start = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            composer,
            "chapter-private-brief",
        ))
        .unwrap();
    let packet_json = start.packet.messages[1].content.as_str();
    let envelope: serde_json::Value = serde_json::from_str(packet_json).unwrap();
    assert_eq!(envelope["approvedWritingBrief"], brief_text);
    assert!(!packet_json.contains("PRIVATE PLANNING SECRET"));
    assert!(!packet_json.contains("the mentor caused the massacre"));
    assert_eq!(
        start
            .packet
            .receipt
            .safe_brief
            .as_ref()
            .and_then(|receipt| receipt.project_origin.as_ref()),
        Some(&origin)
    );
}

#[test]
fn confirmed_project_brief_for_continue_keeps_null_ui_scope_and_uses_derived_append_scope() {
    let (_temp, project, access, chapter) = setup();
    let root = start_root(
        &project,
        &access,
        "Prepare a safe continuation direction without exposing the private plan.",
        "continue-brief-root",
    );
    complete(
        &project,
        &root,
        "PRIVATE AUTHOR ROOM DETAIL: the mentor caused the massacre.",
        "continue-brief-root-finish",
    );

    let project_view = conversation(&project, &access);
    let assistant_message_id = project_view
        .items
        .iter()
        .find(|item| item.reference_id.as_deref() == Some(root.run.id.as_str()))
        .and_then(|item| item.payload["assistantMessageId"].as_str())
        .expect("completed project request exposes its assistant message")
        .to_owned();
    let brief_text = "Continue with restrained tension while preserving the chapter's voice.";
    // Continue deliberately has no renderer-selected scope.  The project
    // origin therefore hashes JSON null; packet compilation separately
    // derives and validates the exact append grant from the frozen chapter.
    let scope = None;
    let origin = project_origin(
        &access,
        &project_view.id,
        &assistant_message_id,
        &chapter.head,
        &scope,
        brief_text,
    );
    let mut composer = chapter_composer(&chapter, FeedbackIntent::Continue);
    composer.chapter.as_mut().unwrap().safe_brief = Some(SafeBriefInput {
        text: brief_text.into(),
        origin_message_id: Some(assistant_message_id),
        confirmed: true,
        project_origin: Some(origin.clone()),
    });

    let start = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            composer,
            "continue-brief-chapter",
        ))
        .expect("a project-approved continuation brief should survive packet compilation");

    let packet_json = start.packet.messages[1].content.as_str();
    let envelope: serde_json::Value = serde_json::from_str(packet_json).unwrap();
    assert_eq!(envelope["approvedWritingBrief"], brief_text);
    assert_eq!(envelope["scope"]["kind"], "append");
    assert!(!packet_json.contains("PRIVATE AUTHOR ROOM DETAIL"));
    assert_eq!(
        start
            .packet
            .receipt
            .safe_brief
            .as_ref()
            .and_then(|receipt| receipt.project_origin.as_ref()),
        Some(&origin)
    );
}

#[test]
fn project_brief_rejects_mismatched_legacy_and_project_origin_ids() {
    let (_temp, project, access, chapter) = setup();
    let root = start_root(&project, &access, "Prepare a chapter brief.", "brief-id-root");
    complete(&project, &root, "An author-room answer.", "brief-id-finish");
    let view = conversation(&project, &access);
    let assistant_message_id = view
        .items
        .iter()
        .find(|item| item.reference_id.as_deref() == Some(root.run.id.as_str()))
        .and_then(|item| item.payload["assistantMessageId"].as_str())
        .expect("completed project request exposes its assistant message")
        .to_owned();
    let scope = Some(selected_scope(&chapter));
    let brief_text = "Keep the reveal restrained.";
    let origin = project_origin(
        &access,
        &view.id,
        &assistant_message_id,
        &chapter.head,
        &scope,
        brief_text,
    );
    let mut composer = chapter_composer(&chapter, FeedbackIntent::ProposeEdits);
    composer.chapter.as_mut().unwrap().scope = scope;
    composer.chapter.as_mut().unwrap().safe_brief = Some(SafeBriefInput {
        text: brief_text.into(),
        origin_message_id: Some(root.user_message.id),
        confirmed: true,
        project_origin: Some(origin),
    });
    let request = save_chapter(&project, &access, composer, "brief-id-mismatch");
    let error = project.start_project_chapter(request).unwrap_err();
    assert_eq!(error.code, "InvalidSafeBrief");
}

#[test]
fn restricted_chapter_response_cannot_be_project_brief_origin() {
    let (_temp, project, access, chapter) = setup();
    let scope = Some(selected_scope(&chapter));
    let mut restricted = chapter_composer(&chapter, FeedbackIntent::ProposeEdits);
    restricted.chapter.as_mut().unwrap().scope = scope.clone();
    let restricted_start = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            restricted,
            "restricted-origin-source",
        ))
        .unwrap();
    complete(
        &project,
        &restricted_start,
        "RESTRICTED RESPONSE MUST NOT BECOME AUTHOR GUIDANCE",
        "restricted-origin-source-finish",
    );

    let view = conversation(&project, &access);
    let source_item = view
        .items
        .iter()
        .find(|item| item.reference_id.as_deref() == Some(restricted_start.run.id.as_str()))
        .expect("completed chapter request remains in the project timeline");
    let source_message_id = source_item.payload["assistantMessageId"]
        .as_str()
        .expect("completed chapter request exposes its assistant message")
        .to_owned();
    let brief_text = "Keep the confrontation restrained.";
    let origin = project_origin(
        &access,
        &view.id,
        &source_message_id,
        &chapter.head,
        &scope,
        brief_text,
    );
    let mut composer = chapter_composer(&chapter, FeedbackIntent::ProposeEdits);
    composer.chapter.as_mut().unwrap().scope = scope;
    composer.chapter.as_mut().unwrap().safe_brief = Some(SafeBriefInput {
        text: brief_text.into(),
        origin_message_id: None,
        confirmed: true,
        project_origin: Some(origin),
    });
    let error = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            composer,
            "restricted-origin-consumer",
        ))
        .unwrap_err();
    assert_eq!(error.code, "InvalidSafeBrief");
}

#[test]
fn project_brief_rejects_wrong_project_message_target_scope_and_text() {
    let (_temp, project, access, chapter) = setup();
    let root = start_root(&project, &access, "Collect a safe writing brief.", "origin-root");
    complete(&project, &root, "PRIVATE AUTHOR ROOM DETAIL", "origin-finish");
    let conversation_id = conversation(&project, &access).id;
    let scope = Some(selected_scope(&chapter));
    let brief_text = "Keep the scene emotionally restrained.";
    let valid_origin = project_origin(
        &access,
        &conversation_id,
        &root.user_message.id,
        &chapter.head,
        &scope,
        brief_text,
    );

    let mut cases = Vec::new();
    let mut wrong_project = valid_origin.clone();
    wrong_project.project_id = "another-project".into();
    cases.push(("wrong-project", wrong_project));
    let mut wrong_message = valid_origin.clone();
    wrong_message.message_id = "missing-message".into();
    cases.push(("wrong-message", wrong_message));
    let mut wrong_target = valid_origin.clone();
    wrong_target.target.document_id = "another-chapter".into();
    cases.push(("wrong-target", wrong_target));
    let mut wrong_scope = valid_origin.clone();
    wrong_scope.scope_hash = "0".repeat(64);
    cases.push(("wrong-scope", wrong_scope));
    let mut wrong_text = valid_origin.clone();
    wrong_text.text_hash = "0".repeat(64);
    cases.push(("wrong-text", wrong_text));

    for (index, (label, origin)) in cases.into_iter().enumerate() {
        let mut composer = chapter_composer(&chapter, FeedbackIntent::ProposeEdits);
        composer.chapter.as_mut().unwrap().scope = scope.clone();
        composer.chapter.as_mut().unwrap().safe_brief = Some(SafeBriefInput {
            text: brief_text.into(),
            origin_message_id: None,
            confirmed: true,
            project_origin: Some(origin),
        });
        let error = project
            .start_project_chapter(save_chapter(
                &project,
                &access,
                composer,
                &format!("brief-invalid-{label}-{index}"),
            ))
            .unwrap_err();
        assert_eq!(error.code, "InvalidSafeBrief", "{label}");
    }
}

#[test]
fn chapter_continuation_keeps_working_first_and_rejects_unavailable_reviewed_basis() {
    let (_temp, project, access, chapter) = setup();
    let mut working = chapter_composer(&chapter, FeedbackIntent::Continue);
    working.chapter.as_mut().unwrap().basis = Some(BasisKind::Working);
    let working_start = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            working,
            "continue-working",
        ))
        .unwrap();
    complete(
        &project,
        &working_start,
        "A working continuation candidate.",
        "continue-working-finish",
    );

    let mut reviewed = chapter_composer(&chapter, FeedbackIntent::Continue);
    reviewed.chapter.as_mut().unwrap().basis = Some(BasisKind::Reviewed);
    let error = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            reviewed,
            "continue-reviewed-without-prefix",
        ))
        .unwrap_err();
    assert_eq!(error.code, "BasisUnavailable");
}

#[test]
fn root_and_chapter_requests_share_one_project_busy_lock() {
    let (_temp, project, access, chapter) = setup();
    let _root = start_root(&project, &access, "Keep this project request active.", "busy-root");
    let chapter_request = save_chapter(
        &project,
        &access,
        chapter_composer(&chapter, FeedbackIntent::Discuss),
        "busy-chapter",
    );
    assert_eq!(
        project
            .start_project_chapter(chapter_request)
            .unwrap_err()
            .code,
        "ProjectChatBusy"
    );

    let (_temp, project, access, chapter) = setup();
    let chapter_request = save_chapter(
        &project,
        &access,
        chapter_composer(&chapter, FeedbackIntent::Discuss),
        "busy-chapter-first",
    );
    let _chapter = project.start_project_chapter(chapter_request).unwrap();
    let view = conversation(&project, &access);
    let composer = ProjectComposer {
        text: "The root request must wait.".into(),
        ..ProjectComposer::default()
    };
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: "busy-root-save".into(),
            conversation_id: view.id.clone(),
            expected_version: view.composer.version,
            body: composer.clone(),
        })
        .unwrap();
    assert_eq!(
        project
            .start_project_chat(StartProjectChat {
                access,
                operation_id: "busy-root-second".into(),
                conversation_id: view.id,
                expected_composer_version: saved.version,
                composer,
                budget: MockContextBudget::new("100000", "8192", "100"),
                provider_binding: None,
            })
            .unwrap_err()
            .code,
        "ProjectChatBusy"
    );
}

#[test]
fn project_history_includes_only_completed_author_room_discuss_chapters() {
    let (_temp, project, access, chapter) = setup();
    let discuss = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            chapter_composer(&chapter, FeedbackIntent::Discuss),
            "history-discuss",
        ))
        .unwrap();
    complete(
        &project,
        &discuss,
        "ELIGIBLE DISCUSS HISTORY",
        "history-discuss-finish",
    );

    let mut restricted = chapter_composer(&chapter, FeedbackIntent::ProposeEdits);
    restricted.chapter.as_mut().unwrap().scope = Some(selected_scope(&chapter));
    restricted.text = "RESTRICTED PROSE INSTRUCTION".into();
    let restricted = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            restricted,
            "history-restricted",
        ))
        .unwrap();
    complete(
        &project,
        &restricted,
        "RESTRICTED PROSE OUTPUT",
        "history-restricted-finish",
    );

    let root = start_root(&project, &access, "Summarize what we discussed.", "history-root");
    let packet_json = root.packet.messages[1].content.as_str();
    assert!(packet_json.contains("ELIGIBLE DISCUSS HISTORY"));
    assert!(!packet_json.contains("RESTRICTED PROSE OUTPUT"));
    assert!(!packet_json.contains("RESTRICTED PROSE INSTRUCTION"));
}

#[test]
fn workshop_intent_stays_in_root_project_chat_and_cannot_use_chapter_route() {
    let (_temp, project, access, chapter) = setup();
    let composer = chapter_composer(&chapter, FeedbackIntent::WorkshopExplore);
    let view = conversation(&project, &access);
    let _saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: "unsupported-save".into(),
            conversation_id: view.id.clone(),
            expected_version: view.composer.version,
            body: composer.clone(),
        })
        .unwrap();
    let chapter_request = save_chapter(
        &project,
        &access,
        composer.clone(),
        "unsupported-chapter",
    );
    assert_eq!(
        project
            .start_project_chapter(chapter_request)
            .unwrap_err()
            .code,
        "InvalidChapterRequest"
    );
    let current = conversation(&project, &access);
    assert_eq!(
        project
            .start_project_chat(StartProjectChat {
                access,
                operation_id: "unsupported-root".into(),
                conversation_id: current.id,
                expected_composer_version: current.composer.version,
                composer,
                budget: MockContextBudget::new("100000", "8192", "100"),
                provider_binding: None,
            })
            .unwrap_err()
            .code,
        "InvalidProjectChat"
    );
}

#[test]
fn restricted_chapter_requests_reject_unadopted_project_chat_drafts() {
    let (_temp, project, access, chapter) = setup();
    let mut composer = chapter_composer(&chapter, FeedbackIntent::ProposeEdits);
    composer.chapter.as_mut().unwrap().scope = Some(selected_scope(&chapter));
    composer.task_draft_refs.push(ProjectChatDraftRef {
        head: chapter.head.clone(),
        disposition_version: "1".into(),
    });
    let error = project
        .start_project_chapter(save_chapter(
            &project,
            &access,
            composer,
            "reject-private-draft",
        ))
        .unwrap_err();
    assert_eq!(error.code, "InvalidChapterRequest");
}
