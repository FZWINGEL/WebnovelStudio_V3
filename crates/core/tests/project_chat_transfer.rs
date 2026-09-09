//! Backup/recovery qualification for the project-chat projection.
//!
//! This file is intentionally kept separate from the first draft lifecycle
//! suite.  Register it in `tests/integration.rs` when the coordinated transfer
//! slice is enabled.

use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::BasisKind;
use serde_json::json;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{DiscussionBegin, DiscussionFinish, FeedbackIntent};
use webnovel_core::projects::project_chat::{
    AdoptChatPreview, PrepareChatAdoption, ProjectChatDraftRef, SaveAssistantDraft,
    ProjectChapterComposer, StartProjectChapter, StartProjectChat,
};
use webnovel_core::projects::{CreateDocument, ProjectSession, SaveCause, SaveSnapshot};
use webnovel_core::projects::project_chat::{
    ProjectComposer, ReadProjectConversation, SaveProjectComposer,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("wns-project-chat-transfer-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temporary directory");
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
        "body": {"type":"doc","content":[{
            "type":"paragraph","attrs":{"id":"p1"},
            "content":[{"type":"text","text":text}]
        }]}
    })
}

fn sha256(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn materialize_and_adopt(project: &ProjectSession, access: &webnovel_core::projects::ProjectAccess) {
    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read conversation");
    let composer = ProjectComposer {
        text: "Develop a harbor mystery for the project.".into(),
        ..ProjectComposer::default()
    };
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: "chat-transfer-live-composer".into(),
            conversation_id: conversation.id.clone(),
            expected_version: conversation.composer.version,
            body: composer.clone(),
        })
        .expect("save live composer");
    let started = project
        .start_project_chat(StartProjectChat {
            access: access.clone(),
            operation_id: "chat-transfer-live-start".into(),
            conversation_id: conversation.id.clone(),
            expected_composer_version: saved.version,
            composer,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
        })
        .expect("start live chat");
    project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .expect("begin live chat");
    project
        .mark_discussion_delivered(started.run.owner.clone())
        .expect("deliver live chat");
    let output = serde_json::to_string(&json!({
        "schemaVersion":"project-assistant-output.v1",
        "answer":"I prepared a harbor setting.",
        "questions":[],"assumptions":[],
        "drafts":[{"key":"harbor","title":"Harbor","kind":"world",
        "changeSummary":"Adds the harbor setting.",
        "blocks":[{"type":"paragraph","content":[{"type":"text","text":"Fog gathers over the harbor."}]}]}]
    }))
    .expect("serialize assistant output");
    project
        .finish_discussion(DiscussionFinish {
            owner: started.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: "chat-transfer-live-finish".into(),
            assistant_text: output,
        })
        .expect("finish live chat");
    project
        .materialize_chat_result(started.run.owner)
        .expect("materialize live chat")
        .expect("materialization event");

    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read materialized conversation");
    let draft = conversation.drafts.first().expect("materialized draft");
    let edited = project
        .save_assistant_draft(SaveAssistantDraft {
            conversation_id: conversation.id.clone(),
            disposition_version: draft.disposition_version.clone(),
            snapshot: SaveSnapshot {
                access: access.clone(),
                operation_id: "chat-transfer-live-edit".into(),
                expected: draft.document.head.clone(),
                local_generation: "1".into(),
                body: body("Fog gathers over the harbor and hides the old bell."),
                cause: SaveCause::Typing,
            },
        })
        .expect("edit materialized draft");
    assert_eq!(edited.document_id, draft.document.head.document_id);
    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read edited conversation");
    let draft = conversation.drafts.first().expect("edited draft");
    let preview = project
        .prepare_chat_adoption(PrepareChatAdoption {
            access: access.clone(),
            operation_id: "chat-transfer-live-prepare".into(),
            conversation_id: conversation.id.clone(),
            drafts: vec![ProjectChatDraftRef {
                head: draft.document.head.clone(),
                disposition_version: draft.disposition_version.clone(),
            }],
        })
        .expect("prepare live adoption");
    project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: "chat-transfer-live-adopt".into(),
            conversation_id: conversation.id,
            preview_id: preview.id.clone(),
            preview_version: preview.version,
            preview_digest: preview.digest,
        })
        .expect("adopt live preview");
}

#[test]
fn valid_project_chat_backup_recovers_with_a_new_current_identity() {
    let temp = Temp::new();
    let source_path = temp.0.join("source");
    let project = ProjectSession::create(&source_path, "Chat transfer").expect("create project");
    let access = project.attach("chat-transfer-test".into()).expect("attach");
    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("create conversation");
    let composer = ProjectComposer {
        text: "Keep the harbor mystery unresolved for now.".into(),
        ..ProjectComposer::default()
    };
    project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: "chat-transfer-composer".into(),
            conversation_id: conversation.id.clone(),
            expected_version: conversation.composer.version,
            body: composer,
        })
        .expect("save composer");
    materialize_and_adopt(&project, &access);

    let backup = temp.0.join("chat.wnsbackup");
    create_backup(&project, &backup).expect("create chat backup");
    let recovered = recover_backup(&backup, &temp.0.join("recovered"), "Recovered chat")
        .expect("recover chat backup");
    assert_ne!(recovered.info.project_id, project.info.project_id);
    assert_ne!(
        recovered.info.operation_namespace,
        project.info.operation_namespace
    );

    let recovered_access = recovered
        .attach("recovered-chat".into())
        .expect("attach recovered");
    let current = recovered
        .read_project_conversation(ReadProjectConversation {
            access: recovered_access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read recovered conversation");
    assert_ne!(current.id, conversation.id);
    assert!(current.items.is_empty());

    let db = Connection::open(recovered.path.join("project.sqlite3")).expect("open recovered db");
    let historical: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM project_conversations WHERE project_id=? AND operation_namespace=?",
            rusqlite::params![project.info.project_id, project.info.operation_namespace],
            |row| row.get(0),
        )
        .expect("count historical conversation");
    assert_eq!(historical, 1);
    let historical_drafts: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM documents WHERE role='assistantDraft'",
            [],
            |row| row.get(0),
        )
        .expect("count recovered drafts");
    assert_eq!(historical_drafts, 1);
    let historical_adoptions: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM conversation_items WHERE kind='adoptionDecision'",
            [],
            |row| row.get(0),
        )
        .expect("count recovered adoption decisions");
    assert_eq!(historical_adoptions, 1);
    let root_role: String = db
        .query_row(
            "SELECT role FROM documents WHERE id=(SELECT anchor_document_id FROM project_conversations WHERE project_id=? AND operation_namespace=?)",
            rusqlite::params![project.info.project_id, project.info.operation_namespace],
            |row| row.get(0),
        )
        .expect("read historical conversation anchor role");
    assert_eq!(root_role, "conversationAnchor");
}

#[test]
fn changed_conversation_event_fingerprint_blocks_backup() {
    let temp = Temp::new();
    let project =
        ProjectSession::create(temp.0.join("tampered"), "Tampered chat").expect("create project");
    let access = project
        .attach("chat-transfer-tamper".into())
        .expect("attach");
    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("create conversation");
    project
        .save_project_composer(SaveProjectComposer {
            access,
            operation_id: "tampered-composer".into(),
            conversation_id: conversation.id,
            expected_version: "0".into(),
            body: ProjectComposer {
                text: "An idea to fingerprint.".into(),
                ..ProjectComposer::default()
            },
        })
        .expect("save composer");
    let db_path = project.path.join("project.sqlite3");
    let db = Connection::open(&db_path).expect("open project db");
    db.execute_batch("DROP TRIGGER conversation_items_immutable_update;")
        .expect("drop test trigger");
    db.execute(
        "UPDATE conversation_items SET payload_hash='0000000000000000000000000000000000000000000000000000000000000000'",
        [],
    )
    .expect("tamper event");
    drop(db);
    let error = create_backup(&project, &temp.0.join("tampered.wnsbackup"))
        .expect_err("tampered chat must block backup");
    assert_eq!(error.code, "InvalidBackup");
}

#[test]
fn backup_accepts_a_valid_root_chat_and_chapter_request_together() {
    let temp = Temp::new();
    let source_path = temp.0.join("mixed");
    let project = ProjectSession::create(&source_path, "Mixed chat transfer").expect("create");
    let access = project.attach("mixed-chat-transfer".into()).expect("attach");
    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("create conversation");
    let chapter = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "mixed-create-chapter".into(),
            document_id: "chapter".into(),
            title: "Chapter One".into(),
            kind: "chapter".into(),
            body: body("The harbor bell rang once."),
        })
        .expect("create chapter");
    let composer = ProjectComposer {
        text: "Continue this chapter carefully.".into(),
        chapter: Some(ProjectChapterComposer {
            target: chapter.head.clone(),
            intent: FeedbackIntent::Continue,
            basis: Some(BasisKind::Working),
            scope: None,
            safe_brief: None,
        }),
        ..ProjectComposer::default()
    };
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: "mixed-save-chapter-composer".into(),
            conversation_id: conversation.id.clone(),
            expected_version: conversation.composer.version,
            body: composer.clone(),
        })
        .expect("save chapter composer");
    project
        .start_project_chapter(StartProjectChapter {
            access: access.clone(),
            operation_id: "mixed-start-chapter".into(),
            conversation_id: conversation.id,
            expected_composer_version: saved.version,
            composer,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
        })
        .expect("start chapter request");
    let backup = temp.0.join("mixed.wnsbackup");
    create_backup(&project, &backup).expect("create mixed backup");
    let recovered = recover_backup(&backup, &temp.0.join("mixed-recovered"), "Recovered mixed")
        .expect("recover mixed backup");
    let recovered_access = recovered.attach("mixed-recovered-session".into()).expect("attach recovered");
    let view = recovered
        .read_project_conversation(ReadProjectConversation {
            access: recovered_access,
            before: None,
            limit: 40,
        })
        .expect("read recovered current conversation");
    assert!(view.items.is_empty(), "recovery starts with a fresh current conversation");
}

#[test]
fn backup_rejects_a_chapter_request_forged_for_another_target() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("cross-target"), "Cross target").expect("create");
    let access = project.attach("cross-target-session".into()).expect("attach");
    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("create conversation");
    let chapter_a = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "cross-create-a".into(),
            document_id: "chapter-a".into(),
            title: "Chapter A".into(),
            kind: "chapter".into(),
            body: body("A"),
        })
        .expect("create chapter A");
    let chapter_b = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "cross-create-b".into(),
            document_id: "chapter-b".into(),
            title: "Chapter B".into(),
            kind: "chapter".into(),
            body: body("B"),
        })
        .expect("create chapter B");
    let composer = ProjectComposer {
        text: "Continue chapter A.".into(),
        chapter: Some(ProjectChapterComposer {
            target: chapter_a.head,
            intent: FeedbackIntent::Continue,
            basis: Some(BasisKind::Working),
            scope: None,
            safe_brief: None,
        }),
        ..ProjectComposer::default()
    };
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: "cross-save".into(),
            conversation_id: conversation.id.clone(),
            expected_version: conversation.composer.version,
            body: composer.clone(),
        })
        .expect("save chapter composer");
    project
        .start_project_chapter(StartProjectChapter {
            access: access.clone(),
            operation_id: "cross-start".into(),
            conversation_id: conversation.id.clone(),
            expected_composer_version: saved.version,
            composer,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
        })
        .expect("start chapter request");

    let db_path = project.path.join("project.sqlite3");
    let db = Connection::open(&db_path).expect("open project db");
    let payload: String = db
        .query_row(
            "SELECT payload_json FROM conversation_items WHERE kind='chapterRequest'",
            [],
            |row| row.get(0),
        )
        .expect("read chapter request payload");
    let mut forged: serde_json::Value = serde_json::from_str(&payload).expect("parse payload");
    forged["target"] = serde_json::to_value(chapter_b.head).expect("serialize chapter B head");
    let forged_payload = serde_json::to_string(&forged).expect("serialize forged payload");
    db.execute_batch("DROP TRIGGER conversation_items_immutable_update;")
        .expect("drop immutable test trigger");
    let forged_hash = sha256(forged_payload.as_bytes());
    db.execute(
        "UPDATE conversation_items SET payload_json=?,payload_hash=? WHERE kind='chapterRequest'",
        rusqlite::params![forged_payload, forged_hash],
    )
    .expect("forge chapter request target");
    drop(db);
    let error = create_backup(&project, &temp.0.join("cross-target.wnsbackup"))
        .expect_err("cross-target chapter request must block backup");
    assert_eq!(error.code, "InvalidBackup");
}
