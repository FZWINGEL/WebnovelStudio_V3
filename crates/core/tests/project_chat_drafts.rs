use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{DiscussionBegin, DiscussionFinish};
use webnovel_core::projects::project_chat::{
    ChatDispositionScope, ChatDispositionScopeKind, ChatUnknownTo, PrepareChatAdoption,
    ProjectChatDraftRef, ProjectComposer, ReadProjectConversation, SaveAssistantDraft,
    SaveProjectComposer, SetChatDisposition, StartProjectChat,
};
use webnovel_core::projects::{
    CheckpointReason, CheckpointRequest, CreateDocument, ProjectAccess, ProjectSession, SaveCause,
    SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-project-chat-drafts-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temporary directory");
        Self(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [{
            "type": "paragraph", "attrs": {"id": "p1"},
            "content": [{"type": "text", "text": text}]
        }]}
    })
}

fn setup() -> (Temp, ProjectSession, ProjectAccess) {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("project"), "Chat drafts").expect("create");
    let access = project.attach("chat-drafts-test".into()).expect("attach");
    (temp, project, access)
}

fn finish_chat(
    project: &ProjectSession,
    access: &ProjectAccess,
    output: &str,
    prefix: &str,
) -> (String, webnovel_core::projects::discussions::RunOwner) {
    finish_chat_with_composer(
        project,
        access,
        ProjectComposer {
            text: "Develop this project idea.".into(),
            ..ProjectComposer::default()
        },
        output,
        prefix,
    )
}

fn finish_chat_with_composer(
    project: &ProjectSession,
    access: &ProjectAccess,
    composer: ProjectComposer,
    output: &str,
    prefix: &str,
) -> (String, webnovel_core::projects::discussions::RunOwner) {
    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("conversation");
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: format!("{prefix}-composer"),
            conversation_id: conversation.id.clone(),
            expected_version: conversation.composer.version,
            body: composer.clone(),
        })
        .expect("save composer");
    let started = project
        .start_project_chat(StartProjectChat {
            access: access.clone(),
            operation_id: format!("{prefix}-start"),
            conversation_id: conversation.id.clone(),
            expected_composer_version: saved.version,
            composer,
            budget: MockContextBudget::new("100000", "8192", "100"),
            provider_binding: None,
        })
        .expect("start project chat");
    project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .expect("begin");
    project
        .mark_discussion_delivered(started.run.owner.clone())
        .expect("deliver");
    project
        .finish_discussion(DiscussionFinish {
            owner: started.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: format!("{prefix}-finish"),
            assistant_text: output.into(),
        })
        .expect("finish");
    (conversation.id, started.run.owner)
}

fn output_with_draft_and_question() -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I have a useful starting point.",
        "questions": [{"key": "q-stakes", "text": "Should the opening protect someone or prove the delivery?"}],
        "assumptions": [{"key": "a-register", "text": "I will use clear English with a translated webnovel rhythm."}],
        "drafts": [{
            "key": "d-world", "title": "Harbor sketch", "kind": "world",
            "changeSummary": "Adds a first setting sketch.",
            "blocks": [{"type": "paragraph", "content": [{"type": "text", "text": "The harbor keeps its secrets under blue lanterns."}]}]
        }]
    }))
    .expect("serialize output")
}

fn output_answer_only() -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I will keep this provisional and continue.",
        "questions": [],
        "assumptions": [],
        "drafts": []
    }))
    .expect("serialize answer-only output")
}

fn output_with_blank_chapter_handoff() -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "We can move from the project room into a new chapter when you are ready.",
        "questions": [],
        "assumptions": [],
        "drafts": [],
        "chapterHandoff": {
            "targetHandle": null,
            "proposedTitle": "Arrival at the river gate",
            "instruction": "Write the opening arrival and stop before the letter is opened.",
            "brief": "Keep the missing ship unexplained and preserve the courier's uncertainty."
        }
    }))
    .expect("serialize blank chapter handoff")
}

fn output_with_three_drafts(label: &str) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": format!("Three retained candidates for {label}."),
        "questions": [],
        "assumptions": [],
        "drafts": [
            {
                "key": format!("{label}-world"),
                "title": format!("World {label}"),
                "kind": "world",
                "changeSummary": "Adds a retained worldbuilding candidate.",
                "blocks": [{"type": "paragraph", "content": [{"type": "text", "text": format!("The setting for {label} waits beyond the harbor gate.")}]}]
            },
            {
                "key": format!("{label}-character"),
                "title": format!("Character {label}"),
                "kind": "character",
                "changeSummary": "Adds a retained character candidate.",
                "blocks": [{"type": "paragraph", "content": [{"type": "text", "text": format!("The protagonist of {label} keeps one promise hidden.")}]}]
            },
            {
                "key": format!("{label}-theme"),
                "title": format!("Theme {label}"),
                "kind": "theme",
                "changeSummary": "Adds a retained thematic candidate.",
                "blocks": [{"type": "paragraph", "content": [{"type": "text", "text": format!("Trust is tested throughout {label}.")}]}]
            }
        ]
    }))
    .expect("serialize three-draft output")
}

fn output_with_predecessor(handle: &str) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I revised the candidate without replacing the original.",
        "questions": [],
        "assumptions": [],
        "drafts": [{
            "key": "d-revised", "title": "Revised harbor sketch", "kind": "world",
            "changeSummary": "Clarifies the earlier harbor candidate.",
            "targetHandle": null, "predecessorHandle": handle,
            "blocks": [{"type": "paragraph", "content": [{
                "type": "text", "text": "The harbor keeps its secrets beneath green lanterns."
            }]}]
        }]
    }))
    .expect("serialize predecessor output")
}

fn manifest_for_run(project: &ProjectSession, run_id: &str) -> Value {
    let db = Connection::open(project.path.join("project.sqlite3")).expect("open project db");
    let manifest: String = db
        .query_row(
            "SELECT s.manifest_json FROM discussion_runs r JOIN context_packets p ON p.id=r.packet_id JOIN story_snapshots s ON s.id=p.snapshot_id WHERE r.id=?",
            [run_id],
            |row| row.get(0),
        )
        .expect("read frozen manifest");
    serde_json::from_str(&manifest).expect("parse frozen manifest")
}

fn packet_id_for_run(project: &ProjectSession, run_id: &str) -> String {
    let db = Connection::open(project.path.join("project.sqlite3")).expect("open project db");
    db.query_row(
        "SELECT packet_id FROM discussion_runs WHERE id=?",
        [run_id],
        |row| row.get(0),
    )
    .expect("read run packet")
}

#[test]
fn materialization_is_local_idempotent_and_draft_save_does_not_advance_story_epoch() {
    let (_temp, project, access) = setup();
    let before_epoch = project.context_source_epoch().expect("epoch");
    let (conversation_id, owner) = finish_chat(
        &project,
        &access,
        &output_with_draft_and_question(),
        "draft-save",
    );
    let first = project
        .materialize_chat_result(owner.clone())
        .expect("materialize")
        .expect("durable result");
    let second = project
        .materialize_chat_result(owner)
        .expect("idempotent materialize")
        .expect("replayed materialization");
    assert_eq!(first, second);
    assert!(first.output_valid);
    assert_eq!(first.draft_ids.len(), 1);
    assert_eq!(project.context_source_epoch().expect("epoch"), before_epoch);

    let view = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("conversation");
    let draft = view.drafts.first().expect("isolated draft");
    let ack = project
        .save_assistant_draft(SaveAssistantDraft {
            conversation_id,
            disposition_version: draft.disposition_version.clone(),
            snapshot: SaveSnapshot {
                access: access.clone(),
                operation_id: "draft-edit".into(),
                expected: draft.document.head.clone(),
                local_generation: "1".into(),
                body: body("The harbor keeps its secrets beneath green lanterns."),
                cause: SaveCause::Typing,
            },
        })
        .expect("save isolated draft");
    assert_eq!(ack.head.version, "1");
    assert_eq!(project.context_source_epoch().expect("epoch"), before_epoch);
}

#[test]
fn blank_chapter_handoff_is_a_valid_proposal_without_creating_story_documents() {
    let (_temp, project, access) = setup();
    let (_conversation_id, owner) = finish_chat(
        &project,
        &access,
        &output_with_blank_chapter_handoff(),
        "chapter-handoff-blank",
    );

    let result = project
        .materialize_chat_result(owner.clone())
        .expect("materialize chapter handoff")
        .expect("durable chapter handoff result");
    assert!(result.output_valid);
    assert!(result.draft_ids.is_empty());

    let db = Connection::open(project.path.join("project.sqlite3")).expect("open project db");
    let assistant_drafts: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM documents WHERE role='assistantDraft'",
            [],
            |row| row.get(0),
        )
        .expect("count assistant drafts");
    assert_eq!(assistant_drafts, 0);
    let ordinary_documents: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM documents WHERE role='ordinary'",
            [],
            |row| row.get(0),
        )
        .expect("count ordinary documents");
    assert_eq!(ordinary_documents, 0);

    let retained_output: String = db
        .query_row(
            "SELECT output_text FROM discussion_runs WHERE id=?",
            [&owner.run_id],
            |row| row.get(0),
        )
        .expect("read retained output");
    let retained: Value = serde_json::from_str(&retained_output).expect("parse retained output");
    assert_eq!(retained["chapterHandoff"]["targetHandle"], Value::Null);
    assert_eq!(
        retained["chapterHandoff"]["proposedTitle"],
        "Arrival at the river gate"
    );
}

#[test]
fn question_disposition_is_versioned_and_does_not_block_a_new_disposition() {
    let (_temp, project, access) = setup();
    let (conversation_id, owner) = finish_chat(
        &project,
        &access,
        &output_with_draft_and_question(),
        "question-disposition",
    );
    project
        .materialize_chat_result(owner)
        .expect("materialize")
        .expect("durable result");
    let reference = format!(
        "{}:q-stakes",
        project
            .read_project_conversation(ReadProjectConversation {
                access: access.clone(),
                before: None,
                limit: 40
            })
            .expect("conversation")
            .items
            .iter()
            .find(|item| item.kind == "request")
            .and_then(|item| item.reference_id.clone())
            .expect("request run")
    );
    let first = project
        .set_chat_disposition(SetChatDisposition {
            access: access.clone(),
            operation_id: "question-not-now".into(),
            conversation_id: conversation_id.clone(),
            reference_id: reference.clone(),
            expected_version: "0".into(),
            disposition: "notNow".into(),
            rationale: "I want to decide after the first scene.".into(),
            scope: None,
            unknown_to: None,
        })
        .expect("not now");
    assert_eq!(first.payload["version"], "1");
    let second = project
        .set_chat_disposition(SetChatDisposition {
            access,
            operation_id: "question-reconsider".into(),
            conversation_id,
            reference_id: reference,
            expected_version: "1".into(),
            disposition: "reconsider".into(),
            rationale: String::new(),
            scope: None,
            unknown_to: None,
        })
        .expect("reconsider");
    assert_eq!(second.payload["version"], "2");
}

#[test]
fn explicit_predecessor_creates_a_new_candidate_without_mutating_the_predecessor() {
    let (_temp, project, access) = setup();
    let (_conversation_id, first_owner) = finish_chat(
        &project,
        &access,
        &output_with_draft_and_question(),
        "predecessor-first",
    );
    project
        .materialize_chat_result(first_owner)
        .expect("materialize first candidate")
        .expect("first materialization");
    let first_view = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read first candidate");
    let predecessor = first_view
        .drafts
        .first()
        .expect("predecessor draft")
        .clone();
    let predecessor_head = predecessor.document.head.clone();
    let predecessor_disposition = predecessor.disposition.clone();
    let composer = ProjectComposer {
        text: "Revise the harbor sketch while preserving the original candidate.".into(),
        task_draft_refs: vec![ProjectChatDraftRef {
            head: predecessor.document.head.clone(),
            disposition_version: predecessor.disposition_version.clone(),
        }],
        ..ProjectComposer::default()
    };
    let (_, successor_owner) = finish_chat_with_composer(
        &project,
        &access,
        composer,
        &output_with_predecessor(predecessor.initial_revision_id.as_str()),
        "predecessor-successor",
    );
    let materialized = project
        .materialize_chat_result(successor_owner)
        .expect("materialize successor")
        .expect("successor materialization");
    assert!(materialized.output_valid);
    assert_eq!(materialized.draft_ids.len(), 1);

    let view = project
        .read_project_conversation(ReadProjectConversation {
            access,
            before: None,
            limit: 40,
        })
        .expect("read successor candidates");
    assert_eq!(view.drafts.len(), 2);
    let original = view
        .drafts
        .iter()
        .find(|draft| draft.document.head.document_id == predecessor_head.document_id)
        .expect("original remains present");
    let successor = view
        .drafts
        .iter()
        .find(|draft| draft.document.head.document_id != predecessor_head.document_id)
        .expect("fresh successor document");
    assert_eq!(original.document.head, predecessor_head);
    assert_eq!(original.disposition, predecessor_disposition);
    assert_eq!(
        successor.predecessor_document_id.as_deref(),
        Some(predecessor_head.document_id.as_str())
    );
    assert_ne!(
        successor.document.head.document_id,
        predecessor_head.document_id
    );
}

#[test]
fn ordinary_or_foreign_predecessors_are_rejected_without_creating_drafts() {
    let (_temp, project, access) = setup();
    let ordinary = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "predecessor-ordinary-create".into(),
            document_id: "ordinary-predecessor-source".into(),
            title: "Ordinary source".into(),
            kind: "note".into(),
            body: body("An ordinary source cannot be predecessor lineage."),
        })
        .expect("create ordinary source");
    let ordinary_checkpoint = project
        .checkpoint(CheckpointRequest {
            access: access.clone(),
            expected: ordinary.head.clone(),
            reason: CheckpointReason::Source,
        })
        .expect("checkpoint ordinary source");
    let (conversation_id, _) = finish_chat_with_composer(
        &project,
        &access,
        ProjectComposer {
            text: "Start from the ordinary source.".into(),
            source_refs: vec![ordinary.head.clone()],
            ..ProjectComposer::default()
        },
        &output_answer_only(),
        "predecessor-invalid-ordinary",
    );
    let ordinary_handle = ordinary_checkpoint.id;
    let (_, ordinary_owner) = finish_chat_with_composer(
        &project,
        &access,
        ProjectComposer {
            text: "Try to misuse the ordinary source as a predecessor.".into(),
            source_refs: vec![ordinary.head],
            ..ProjectComposer::default()
        },
        &output_with_predecessor(&ordinary_handle),
        "predecessor-invalid-ordinary-result",
    );
    let ordinary_result = project
        .materialize_chat_result(ordinary_owner)
        .expect("materialize invalid ordinary predecessor")
        .expect("invalid ordinary terminal result");
    assert!(!ordinary_result.output_valid);
    assert!(ordinary_result.draft_ids.is_empty());

    let (_, foreign_owner) = finish_chat(
        &project,
        &access,
        &output_with_predecessor("foreign-draft-revision"),
        "predecessor-invalid-foreign",
    );
    let foreign_result = project
        .materialize_chat_result(foreign_owner)
        .expect("materialize invalid foreign predecessor")
        .expect("invalid foreign terminal result");
    assert!(!foreign_result.output_valid);
    assert!(foreign_result.draft_ids.is_empty());
    let view = project
        .read_project_conversation(ReadProjectConversation {
            access,
            before: None,
            limit: 40,
        })
        .expect("read invalid predecessor conversation");
    assert!(
        view.drafts.is_empty(),
        "invalid predecessors create no artifacts"
    );
    assert!(!conversation_id.is_empty());
}

#[test]
fn scoped_dispositions_only_enter_matching_context_and_task_scope_does_not_leak() {
    let (_temp, project, access) = setup();
    let document_a = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "scope-document-a".into(),
            document_id: "scope-document-a".into(),
            title: "Scope A".into(),
            kind: "note".into(),
            body: body("Scope A source."),
        })
        .expect("create scope A");
    let (conversation_id, first_owner) = finish_chat(
        &project,
        &access,
        &output_with_draft_and_question(),
        "scope-first",
    );
    project
        .materialize_chat_result(first_owner.clone())
        .expect("materialize scope question")
        .expect("scope materialization");
    let reference = format!("{}:q-stakes", first_owner.run_id);
    project
        .set_chat_disposition(SetChatDisposition {
            access: access.clone(),
            operation_id: "scope-task-disposition".into(),
            conversation_id: conversation_id.clone(),
            reference_id: reference.clone(),
            expected_version: "0".into(),
            disposition: "notNow".into(),
            rationale: "Only this task should defer the question.".into(),
            scope: Some(ChatDispositionScope {
                kind: ChatDispositionScopeKind::Task,
                reference_id: Some(first_owner.run_id.clone()),
            }),
            unknown_to: None,
        })
        .expect("task-scoped disposition");
    let (_, unrelated_owner) =
        finish_chat(&project, &access, &output_answer_only(), "scope-unrelated");
    let unrelated_manifest = manifest_for_run(&project, &unrelated_owner.run_id);
    assert!(
        unrelated_manifest["projectChat"]["dispositions"]
            .as_array()
            .is_none_or(|items| items.is_empty())
    );

    project
        .set_chat_disposition(SetChatDisposition {
            access: access.clone(),
            operation_id: "scope-document-disposition".into(),
            conversation_id,
            reference_id: reference,
            expected_version: "1".into(),
            disposition: "reconsider".into(),
            rationale: "Reopen this only while reviewing Scope A.".into(),
            scope: Some(ChatDispositionScope {
                kind: ChatDispositionScopeKind::Document,
                reference_id: Some(document_a.head.document_id.clone()),
            }),
            unknown_to: None,
        })
        .expect("document-scoped disposition");
    let (_, without_document_owner) = finish_chat(
        &project,
        &access,
        &output_answer_only(),
        "scope-without-document",
    );
    let without_document_manifest = manifest_for_run(&project, &without_document_owner.run_id);
    assert!(
        without_document_manifest["projectChat"]["dispositions"]
            .as_array()
            .is_none_or(|items| items.is_empty())
    );

    let (_, with_document_owner) = finish_chat_with_composer(
        &project,
        &access,
        ProjectComposer {
            text: "Review the question with Scope A in view.".into(),
            source_refs: vec![document_a.head],
            ..ProjectComposer::default()
        },
        &output_answer_only(),
        "scope-with-document",
    );
    let with_document_manifest = manifest_for_run(&project, &with_document_owner.run_id);
    assert_eq!(
        with_document_manifest["projectChat"]["dispositions"]
            .as_array()
            .expect("document disposition projection")
            .len(),
        1
    );
}

#[test]
fn unknown_to_keep_mysterious_survives_project_chat_backup_recovery() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("source"), "Mystery backup").expect("create");
    let access = project.attach("mystery-source".into()).expect("attach");
    let (conversation_id, owner) = finish_chat(
        &project,
        &access,
        &output_with_draft_and_question(),
        "mystery-backup",
    );
    project
        .materialize_chat_result(owner.clone())
        .expect("materialize mystery question")
        .expect("mystery materialization");
    project
        .set_chat_disposition(SetChatDisposition {
            access: access.clone(),
            operation_id: "mystery-disposition".into(),
            conversation_id,
            reference_id: format!("{}:q-stakes", owner.run_id),
            expected_version: "0".into(),
            disposition: "keepMysterious".into(),
            rationale: "The author and reader should both remain uncertain.".into(),
            scope: Some(ChatDispositionScope::default()),
            unknown_to: Some(ChatUnknownTo::Both),
        })
        .expect("keep mystery");
    let backup = temp.0.join("mystery.wnsbackup");
    create_backup(&project, &backup).expect("create mystery backup");
    let recovered = recover_backup(&backup, &temp.0.join("recovered"), "Recovered mystery")
        .expect("recover mystery backup");
    let db = Connection::open(recovered.path.join("project.sqlite3")).expect("open recovered db");
    let payload: String = db
        .query_row(
            "SELECT payload_json FROM conversation_items WHERE kind='chatDisposition'",
            [],
            |row| row.get(0),
        )
        .expect("recovered disposition event");
    let payload: Value = serde_json::from_str(&payload).expect("parse recovered disposition");
    assert_eq!(payload["disposition"], "keepMysterious");
    assert_eq!(payload["unknownTo"], "both");
    assert_eq!(payload["scope"]["kind"], "project");
}

#[test]
fn malformed_output_is_retained_without_creating_a_draft() {
    let (_temp, project, access) = setup();
    let (_conversation_id, owner) =
        finish_chat(&project, &access, "not project-chat json", "malformed");
    let materialized = project
        .materialize_chat_result(owner)
        .expect("materialize malformed result")
        .expect("terminal result");
    assert!(!materialized.output_valid);
    assert!(materialized.draft_ids.is_empty());
}

#[test]
fn retained_pending_drafts_remain_complete_across_timeline_pages() {
    let (_temp, project, access) = setup();
    // Each response creates three pending candidates.  This exceeds the old
    // implicit 100-row review inventory while keeping the timeline itself
    // deliberately paged.
    for index in 0..34 {
        let label = format!("inventory-{index}");
        let (_conversation_id, owner) =
            finish_chat(&project, &access, &output_with_three_drafts(&label), &label);
        let materialized = project
            .materialize_chat_result(owner)
            .expect("materialize inventory response")
            .expect("inventory response");
        assert!(materialized.output_valid);
        assert_eq!(materialized.draft_ids.len(), 3);
    }

    let newest = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 1,
        })
        .expect("read newest timeline page");
    assert_eq!(newest.drafts.len(), 102);
    assert!(
        newest
            .drafts
            .iter()
            .any(|draft| draft.document.title == "World inventory-0")
    );
    let older_before = newest.older_before.clone().expect("older timeline page");

    let older = project
        .read_project_conversation(ReadProjectConversation {
            access,
            before: Some(older_before),
            limit: 1,
        })
        .expect("read older timeline page");
    assert_eq!(older.drafts.len(), 102);
    assert!(
        older
            .drafts
            .iter()
            .any(|draft| draft.document.title == "World inventory-0")
    );
}

#[test]
fn stale_task_draft_can_seed_a_fresh_request_but_cannot_be_adopted() {
    let (_temp, project, access) = setup();
    let (_conversation_id, first_owner) = finish_chat(
        &project,
        &access,
        &output_with_draft_and_question(),
        "stale-draft-first",
    );
    let first_run_id = first_owner.run_id.clone();
    project
        .materialize_chat_result(first_owner)
        .expect("materialize initial draft")
        .expect("initial materialization");
    let initial = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read initial draft");
    let draft = initial
        .drafts
        .first()
        .expect("initial pending draft")
        .clone();
    let draft_ref = ProjectChatDraftRef {
        head: draft.document.head.clone(),
        disposition_version: draft.disposition_version.clone(),
    };

    let ordinary = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "stale-source-create".into(),
            document_id: "stale-source".into(),
            title: "Source that moves the story forward".into(),
            kind: "note".into(),
            body: body("The first source version."),
        })
        .expect("create ordinary source");
    let ordinary = project
        .checkpoint(CheckpointRequest {
            access: access.clone(),
            expected: ordinary.head.clone(),
            reason: CheckpointReason::Source,
        })
        .expect("checkpoint ordinary source");
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "stale-source-edit".into(),
            expected: ordinary.head,
            local_generation: "1".into(),
            body: body("The edited source version."),
            cause: SaveCause::Typing,
        })
        .expect("edit ordinary source");

    let composer = ProjectComposer {
        text: "Use this older draft as a starting point, then prepare a fresh response against the edited story.".into(),
        task_draft_refs: vec![draft_ref.clone()],
        ..ProjectComposer::default()
    };
    let (conversation_id, fresh_owner) = finish_chat_with_composer(
        &project,
        &access,
        composer,
        &output_answer_only(),
        "stale-draft-fresh-request",
    );
    let fresh_manifest = manifest_for_run(&project, &fresh_owner.run_id);
    assert_ne!(
        packet_id_for_run(&project, &first_run_id),
        packet_id_for_run(&project, &fresh_owner.run_id),
        "the explicit revise request must freeze a fresh packet"
    );
    assert_eq!(
        fresh_manifest["projectChat"]["taskDraftRefs"][0]["head"]["documentId"],
        draft_ref.head.document_id
    );
    assert_ne!(fresh_owner.run_id, draft.origin_run_id);

    let refreshed = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read stale draft after fresh request");
    let stale = refreshed
        .drafts
        .iter()
        .find(|candidate| candidate.document.head.document_id == draft_ref.head.document_id)
        .expect("stale draft remains reviewable");
    assert!(stale.stale);

    let error = project
        .prepare_chat_adoption(PrepareChatAdoption {
            access,
            operation_id: "stale-draft-adoption".into(),
            conversation_id,
            drafts: vec![draft_ref],
        })
        .expect_err("a stale draft cannot enter an adoption preview");
    assert_eq!(error.code, "ContextChanged");
}
