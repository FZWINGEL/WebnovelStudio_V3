//! Read-only access to retained project-chat conversations.

use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{DiscussionBegin, DiscussionFinish};
use webnovel_core::projects::project_chat::{
    HistoricalConversationRef, ProjectComposer, ReadProjectChatHistory,
    ReadProjectConversation, SaveProjectComposer, StartProjectChat,
};
use webnovel_core::projects::{ProjectAccess, ProjectSession};
use webnovel_core::transfer::{create_backup, recover_backup};

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-project-chat-history-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn historical_ref(project: &ProjectSession, conversation_id: String) -> HistoricalConversationRef {
    HistoricalConversationRef {
        project_id: project.info.project_id.clone(),
        operation_namespace: project.info.operation_namespace.clone(),
        conversation_id,
    }
}

fn finished_project_chat(
    project: &ProjectSession,
    access: &ProjectAccess,
    operation_id: &str,
) -> HistoricalConversationRef {
    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .unwrap();
    let composer = ProjectComposer {
        text: "Remember the lantern keeper's unanswered question.".into(),
        ..ProjectComposer::default()
    };
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: format!("{operation_id}-save"),
            conversation_id: conversation.id.clone(),
            expected_version: conversation.composer.version,
            body: composer.clone(),
        })
        .unwrap();
    let started = project
        .start_project_chat(StartProjectChat {
            access: access.clone(),
            operation_id: operation_id.into(),
            conversation_id: conversation.id.clone(),
            expected_composer_version: saved.version,
            composer,
            budget: MockContextBudget::new("100000", "8192", "100"),
            provider_binding: None,
        })
        .unwrap();
    project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .unwrap();
    project
        .mark_discussion_delivered(started.run.owner.clone())
        .unwrap();
    project
        .finish_discussion(DiscussionFinish {
            owner: started.run.owner,
            expected_sequence: "0".into(),
            event_id: format!("{operation_id}-finish"),
            assistant_text: "The unanswered question remains the story's hook.".into(),
        })
        .unwrap();
    historical_ref(project, conversation.id)
}

fn read(
    project: &ProjectSession,
    access: &ProjectAccess,
    conversation: HistoricalConversationRef,
) -> webnovel_core::projects::CoreResult<webnovel_core::projects::project_chat::HistoricalConversation> {
    project.read_project_chat_history(ReadProjectChatHistory {
        access: access.clone(),
        conversation,
        before: None,
        limit: 40,
    })
}

#[test]
fn current_history_requires_exact_project_namespace_and_conversation_identity() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("project"), "History access").unwrap();
    let access = project.documents().attach("history-session".into()).unwrap();
    let reference = finished_project_chat(&project, &access, "history-current");

    let history = read(&project, &access, reference.clone()).unwrap();
    assert_eq!(history.conversation, reference);
    let request = history
        .items
        .iter()
        .find(|item| item.item.kind == "request")
        .expect("retained request item");
    assert_eq!(request.run.as_ref().unwrap().id, request.item.reference_id.clone().unwrap());
    assert!(request
        .messages
        .iter()
        .any(|message| message.content.contains("lantern keeper")));
    assert!(request
        .messages
        .iter()
        .any(|message| message.content.contains("unanswered question")));

    let mut wrong_project = reference.clone();
    wrong_project.project_id = "another-project".into();
    assert_eq!(read(&project, &access, wrong_project).unwrap_err().code, "HistoricalConversationNotFound");

    let mut wrong_namespace = reference.clone();
    wrong_namespace.operation_namespace = "another-namespace".into();
    assert_eq!(read(&project, &access, wrong_namespace).unwrap_err().code, "HistoricalConversationNotFound");

    let mut arbitrary = reference;
    arbitrary.conversation_id = "missing-conversation".into();
    assert_eq!(read(&project, &access, arbitrary).unwrap_err().code, "HistoricalConversationNotFound");
}

#[test]
fn recovered_project_can_read_original_history_but_current_identity_is_separate() {
    let temp = Temp::new();
    let source = ProjectSession::create(temp.0.join("source"), "History recovery").unwrap();
    let source_access = source.documents().attach("history-source".into()).unwrap();
    let reference = finished_project_chat(&source, &source_access, "history-recovered");
    let backup = temp.0.join("history.wnsbackup");
    create_backup(&source, &backup).unwrap();

    let recovered_path = temp.0.join("recovered");
    let recovered = recover_backup(&backup, &recovered_path, "Recovered history").unwrap();
    let recovered_access = recovered.documents().attach("history-recovered".into()).unwrap();
    let current = recovered
        .read_project_conversation(ReadProjectConversation {
            access: recovered_access.clone(),
            before: None,
            limit: 40,
        })
        .unwrap();
    assert!(current.items.is_empty(), "recovery starts a new current conversation");
    assert_ne!(recovered.info.project_id, reference.project_id);
    assert_ne!(recovered.info.operation_namespace, reference.operation_namespace);

    let index = recovered
        .list_project_chat_history(recovered_access.clone())
        .unwrap();
    assert_eq!(index.len(), 2);
    assert!(index.iter().any(|entry| entry.current));
    assert!(index.iter().any(|entry| !entry.current && entry.conversation == reference));

    let history = read(
        &recovered,
        &recovered_access,
        index.into_iter().find(|entry| !entry.current).unwrap().conversation,
    )
    .unwrap();
    let request = history
        .items
        .iter()
        .find(|item| item.item.kind == "request")
        .expect("recovered request item");
    assert!(request.messages.iter().any(|message| {
        message.content.contains("lantern keeper") || message.content.contains("unanswered question")
    }));
}
