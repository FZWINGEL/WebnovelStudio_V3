use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{DiscussionBegin, DiscussionFinish};
use webnovel_core::projects::project_chat::{
    AdoptChatPreview, PrepareChatAdoption, ProjectComposer, ProjectChatDraftRef,
    SaveAssistantDraft, StartProjectChat,
};
use webnovel_core::projects::{
    CheckpointReason, CheckpointRequest, CreateDocument, ProjectAccess, ProjectSession, SaveCause,
    SaveSnapshot,
};

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-chat-adoption-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create temporary directory");
        Self(path)
    }
    fn project_path(&self) -> PathBuf {
        self.0.join("project")
    }
}

impl Drop for TempProject {
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

fn output() -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I prepared a world sketch for review.",
        "questions": [],
        "assumptions": [],
        "drafts": [{
            "key": "world-sketch",
            "title": "River Gate",
            "kind": "world",
            "changeSummary": "Adds the first setting sketch.",
            "blocks": [{
                "type": "paragraph",
                "content": [{"type": "text", "text": "The river gate opens at dawn."}]
            }]
        }]
    }))
    .expect("serialize project-chat output")
}

fn output_three() -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I prepared three related records for review.",
        "questions": [],
        "assumptions": [],
        "drafts": [
            {
                "key": "world-one", "title": "World One", "kind": "world",
                "changeSummary": "Adds the first setting sketch.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The first river gate opens at dawn."}]}]
            },
            {
                "key": "character-one", "title": "Character One", "kind": "character",
                "changeSummary": "Adds the first character sketch.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The keeper remembers every visitor."}]}]
            },
            {
                "key": "theme-one", "title": "Theme One", "kind": "theme",
                "changeSummary": "Adds the first thematic thread.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"Trust is tested at the gate."}]}]
            }
        ]
    }))
    .expect("serialize grouped project-chat output")
}

fn output_target(handle: &str) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I prepared two edits to the same record.",
        "questions": [],
        "assumptions": [],
        "drafts": [
            {
                "key": "target-one", "title": "Target One", "kind": "world",
                "targetHandle": handle,
                "changeSummary": "First edit.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The first proposed edit."}]}]
            },
            {
                "key": "target-two", "title": "Target Two", "kind": "world",
                "targetHandle": handle,
                "changeSummary": "Second edit.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The second proposed edit."}]}]
            }
        ]
    }))
    .expect("serialize duplicate-target project-chat output")
}

fn has_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(object) => object
            .iter()
            .any(|(name, child)| name == key || has_key(child, key)),
        Value::Array(values) => values.iter().any(|child| has_key(child, key)),
        _ => false,
    }
}

fn start_chat(
    project: &ProjectSession,
    access: &ProjectAccess,
) -> (
    String,
    webnovel_core::projects::discussions::DiscussionStart,
) {
    start_chat_with(project, access, ProjectComposer::default(), output(), "single")
}

fn start_chat_with(
    project: &ProjectSession,
    access: &ProjectAccess,
    mut composer: ProjectComposer,
    assistant_text: String,
    prefix: &str,
) -> (
    String,
    webnovel_core::projects::discussions::DiscussionStart,
) {
    let conversation = project
        .read_project_conversation(
            webnovel_core::projects::project_chat::ReadProjectConversation {
                access: access.clone(),
                before: None,
                limit: 40,
            },
        )
        .expect("ensure project conversation");
    if composer.text.is_empty() {
        composer.text = "Develop the world around the river gate.".into();
    }
    let saved = project
        .save_project_composer(webnovel_core::projects::project_chat::SaveProjectComposer {
            access: access.clone(),
            operation_id: format!("{prefix}-save-composer"),
            conversation_id: conversation.id.clone(),
            expected_version: conversation.composer.version,
            body: composer.clone(),
        })
        .expect("save composer");
    let started = project
        .start_project_chat(StartProjectChat {
            access: access.clone(),
            operation_id: format!("{prefix}-start-chat"),
            conversation_id: conversation.id.clone(),
            expected_composer_version: saved.version,
            composer,
            budget: MockContextBudget::new("100000", "1000", "100"),
            provider_binding: None,
        })
        .expect("start project chat");
    project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .expect("begin chat run");
    project
        .mark_discussion_delivered(started.run.owner.clone())
        .expect("mark chat delivered");
    project
        .finish_discussion(DiscussionFinish {
            owner: started.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: format!("{prefix}-finish"),
            assistant_text,
        })
        .expect("finish chat run");
    project
        .materialize_chat_result(started.run.owner.clone())
        .expect("materialize chat result")
        .expect("materialization event");
    (conversation.id, started)
}

fn conversation(
    project: &ProjectSession,
    access: &ProjectAccess,
) -> webnovel_core::projects::project_chat::ProjectConversation {
    project
        .read_project_conversation(
            webnovel_core::projects::project_chat::ReadProjectConversation {
                access: access.clone(),
                before: None,
                limit: 100,
            },
        )
        .expect("read project conversation")
}

fn draft_refs(
    project: &ProjectSession,
    access: &ProjectAccess,
) -> Vec<ProjectChatDraftRef> {
    conversation(project, access)
        .drafts
        .into_iter()
        .map(|draft| ProjectChatDraftRef {
            head: draft.document.head,
            disposition_version: draft.disposition_version,
        })
        .collect()
}

fn prepare(
    project: &ProjectSession,
    access: &ProjectAccess,
    conversation_id: String,
    operation_id: &str,
    drafts: Vec<ProjectChatDraftRef>,
) -> webnovel_core::projects::project_chat::ChatAdoptionPreview {
    project
        .prepare_chat_adoption(PrepareChatAdoption {
            access: access.clone(),
            operation_id: operation_id.into(),
            conversation_id,
            drafts,
            group_effects: None,
        })
        .expect("prepare adoption")
}

fn adopt(
    project: &ProjectSession,
    access: &ProjectAccess,
    conversation_id: String,
    operation_id: &str,
    preview: &webnovel_core::projects::project_chat::ChatAdoptionPreview,
) -> webnovel_core::projects::project_chat::ChatAdoptionAck {
    project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: operation_id.into(),
            conversation_id,
            preview_id: preview.id.clone(),
            preview_version: preview.version.clone(),
            preview_digest: preview.digest.clone(),
        })
        .expect("adopt preview")
}

fn create_source(
    project: &ProjectSession,
    access: &ProjectAccess,
    id: &str,
    title: &str,
) -> webnovel_core::projects::DocumentRecord {
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: format!("create-{id}"),
            document_id: id.into(),
            title: title.into(),
            kind: "world".into(),
            body: body("A source that anchors the chat context."),
        })
        .expect("create source document")
}

#[test]
fn explicit_chat_adoption_is_atomic_and_preview_payload_is_ref_only() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Chat adoption").expect("create");
    let access = project.attach("chat-adoption-test".into()).expect("attach");
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-world".into(),
            document_id: "world".into(),
            title: "Existing World".into(),
            kind: "world".into(),
            body: body("A river surrounds the city."),
        })
        .expect("create source document");

    let (conversation_id, _started) = start_chat(&project, &access);
    let conversation = project
        .read_project_conversation(
            webnovel_core::projects::project_chat::ReadProjectConversation {
                access: access.clone(),
                before: None,
                limit: 40,
            },
        )
        .expect("read conversation");
    let draft = conversation.drafts.first().expect("draft");
    let reference = serde_json::from_value(json!({
        "head": draft.document.head.clone(),
        "dispositionVersion": draft.disposition_version,
    }))
    .expect("draft reference");
    let preview = project
        .prepare_chat_adoption(PrepareChatAdoption {
            access: access.clone(),
            operation_id: "prepare-adoption".into(),
            conversation_id: conversation_id.clone(),
            drafts: vec![reference],
            group_effects: None,
        })
        .expect("prepare adoption");
    let ack = project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: "adopt-preview".into(),
            conversation_id,
            preview_id: preview.id.clone(),
            preview_version: preview.version.clone(),
            preview_digest: preview.digest.clone(),
        })
        .expect("adopt preview");
    assert_eq!(ack.documents.len(), 1);
    assert_eq!(ack.documents[0].kind, "world");

    let adopted = project
        .document(access.clone(), ack.documents[0].head.document_id.clone())
        .expect("read adopted ordinary document");
    assert_eq!(
        adopted.role,
        webnovel_core::projects::DocumentRole::Ordinary
    );
    let after = project
        .read_project_conversation(
            webnovel_core::projects::project_chat::ReadProjectConversation {
                access: access.clone(),
                before: None,
                limit: 40,
            },
        )
        .expect("read adopted conversation");
    assert_eq!(after.drafts[0].disposition, "adopted");
    drop(project);

    let db = Connection::open(temp.project_path().join("project.sqlite3")).expect("open db");
    let payload: String = db
        .query_row(
            "SELECT payload_json FROM conversation_items WHERE kind='adoptionPreview'",
            [],
            |row| row.get(0),
        )
        .expect("read preview payload");
    let value: Value = serde_json::from_str(&payload).expect("parse preview payload");
    assert!(
        !has_key(&value, "body"),
        "preview ledger must not duplicate document bodies"
    );
    assert!(
        !has_key(&value, "content"),
        "preview ledger must reference revisions rather than blocks"
    );
}

#[test]
fn grouped_adoption_is_one_epoch_and_replay_returns_the_same_result() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Grouped adoption").expect("create");
    let access = project.attach("grouped-adoption-test".into()).expect("attach");
    let (conversation_id, _) = start_chat_with(
        &project,
        &access,
        ProjectComposer::default(),
        output_three(),
        "grouped",
    );
    let drafts = draft_refs(&project, &access);
    assert_eq!(drafts.len(), 3);
    let preview = prepare(
        &project,
        &access,
        conversation_id.clone(),
        "grouped-prepare",
        drafts.clone(),
    );
    let replayed_preview = prepare(
        &project,
        &access,
        conversation_id.clone(),
        "grouped-prepare",
        drafts,
    );
    assert_eq!(
        serde_json::to_value(&preview).expect("preview json"),
        serde_json::to_value(&replayed_preview).expect("replayed preview json")
    );

    let epoch_before = project.context().source_epoch().expect("read source epoch");
    let ack = adopt(
        &project,
        &access,
        conversation_id.clone(),
        "grouped-adopt",
        &preview,
    );
    assert_eq!(ack.documents.len(), 3);
    assert_eq!(
        project.context().source_epoch().expect("read source epoch"),
        (epoch_before.parse::<u64>().expect("epoch") + 1).to_string()
    );
    let replayed_ack = adopt(
        &project,
        &access,
        conversation_id.clone(),
        "grouped-adopt",
        &preview,
    );
    assert_eq!(
        serde_json::to_value(&ack).expect("ack json"),
        serde_json::to_value(&replayed_ack).expect("replayed ack json")
    );
    let after = conversation(&project, &access);
    assert_eq!(
        after
            .drafts
            .iter()
            .filter(|draft| draft.disposition == "adopted")
            .count(),
        3
    );
    assert_eq!(
        project
            .documents(access)
            .expect("list ordinary documents")
            .iter()
            .filter(|document| ["World One", "Character One", "Theme One"].contains(&document.title.as_str()))
            .count(),
        3
    );
}

#[test]
fn grouped_adoption_rolls_back_when_the_second_material_write_fails() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Grouped rollback").expect("create");
    let access = project.attach("grouped-rollback-test".into()).expect("attach");
    let (conversation_id, _) = start_chat_with(
        &project,
        &access,
        ProjectComposer::default(),
        output_three(),
        "rollback",
    );
    let preview = prepare(
        &project,
        &access,
        conversation_id.clone(),
        "rollback-prepare",
        draft_refs(&project, &access),
    );
    let epoch_before = project.context().source_epoch().expect("read source epoch");
    let db = Connection::open(temp.project_path().join("project.sqlite3")).expect("open db");
    db.execute_batch(
        "CREATE TRIGGER fail_second_chat_material_write
         BEFORE INSERT ON documents
         WHEN NEW.title='Character One'
         BEGIN SELECT RAISE(ABORT,'injected second material write failure'); END;",
    )
    .expect("install fault trigger");
    let error = project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: "rollback-adopt".into(),
            conversation_id,
            preview_id: preview.id.clone(),
            preview_version: preview.version.clone(),
            preview_digest: preview.digest.clone(),
        })
        .expect_err("second target failure must abort the whole adoption");
    assert!(!error.code.is_empty());
    drop(db);
    drop(project);

    let db = Connection::open(temp.project_path().join("project.sqlite3")).expect("reopen db");
    let ordinary_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM documents WHERE role='ordinary' AND title IN ('World One','Character One','Theme One')",
            [],
            |row| row.get(0),
        )
        .expect("count material targets");
    assert_eq!(ordinary_count, 0, "the first target must roll back too");
    let epoch: i64 = db
        .query_row("SELECT context_source_epoch FROM project WHERE singleton=1", [], |row| row.get(0))
        .expect("read source epoch");
    assert_eq!(epoch.to_string(), epoch_before);
    let pending: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM assistant_drafts WHERE disposition='pending'",
            [],
            |row| row.get(0),
        )
        .expect("count pending drafts");
    assert_eq!(pending, 3);
    let decisions: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM conversation_items WHERE kind='adoptionDecision'",
            [],
            |row| row.get(0),
        )
        .expect("count decisions");
    assert_eq!(decisions, 0);
    let receipts: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM command_receipts WHERE operation_id='rollback-adopt'",
            [],
            |row| row.get(0),
        )
        .expect("count adoption receipt");
    assert_eq!(receipts, 0);
}

#[test]
fn changed_draft_invalidates_preview_without_partial_adoption() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Changed draft").expect("create");
    let access = project.attach("changed-draft-test".into()).expect("attach");
    let (conversation_id, _) = start_chat(&project, &access);
    let draft = conversation(&project, &access).drafts.into_iter().next().expect("draft");
    let preview = prepare(
        &project,
        &access,
        conversation_id.clone(),
        "changed-draft-prepare",
        vec![ProjectChatDraftRef {
            head: draft.document.head.clone(),
            disposition_version: draft.disposition_version.clone(),
        }],
    );
    project
        .save_assistant_draft(SaveAssistantDraft {
            conversation_id: conversation_id.clone(),
            disposition_version: draft.disposition_version,
            snapshot: SaveSnapshot {
                access: access.clone(),
                operation_id: "changed-draft-save".into(),
                expected: draft.document.head,
                local_generation: "1".into(),
                body: body("The author changed this draft after preview."),
                cause: SaveCause::Typing,
            },
        })
        .expect("edit assistant draft");
    let historical = project.read_chat_adoption_preview(access.clone(), conversation_id.clone(), preview.id.clone())
        .expect("original preview remains readable after a later draft edit");
    assert_eq!(serde_json::to_value(&historical).unwrap(), serde_json::to_value(&preview).unwrap());
    assert!(project.read_chat_adoption_preview(access.clone(), "other-conversation".into(), preview.id.clone()).is_err());
    let error = project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: "changed-draft-adopt".into(),
            conversation_id,
            preview_id: preview.id,
            preview_version: preview.version,
            preview_digest: preview.digest,
        })
        .expect_err("changed draft must invalidate preview");
    assert!(matches!(error.code.as_str(), "DraftChanged" | "ContextChanged"));
    assert!(project
        .documents(access)
        .expect("list ordinary documents")
        .iter()
        .all(|document| document.title != "River Gate"));
}

#[test]
fn source_metadata_change_invalidates_target_preview() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Metadata change").expect("create");
    let access = project.attach("metadata-change-test".into()).expect("attach");
    let source = create_source(&project, &access, "source", "Original Source");
    let checkpoint = project
        .checkpoint(CheckpointRequest {
            access: access.clone(),
            expected: source.head.clone(),
            reason: CheckpointReason::Source,
        })
        .expect("checkpoint source");
    let composer = ProjectComposer {
        text: "Revise the anchored source.".into(),
        source_refs: vec![source.head.clone()],
        ..ProjectComposer::default()
    };
    let mut target_output: Value = serde_json::from_str(&output_target(&checkpoint.id)).unwrap();
    target_output["drafts"][1]["targetHandle"] = Value::Null;
    let (conversation_id, _) = start_chat_with(
        &project,
        &access,
        composer,
        serde_json::to_string(&target_output).unwrap(),
        "metadata",
    );
    let refs = draft_refs(&project, &access);
    assert_eq!(refs.len(), 2);
    let preview = prepare(
        &project,
        &access,
        conversation_id.clone(),
        "metadata-prepare",
        refs,
    );
    project
        .rename_document(
            access.clone(),
            source.head.document_id.clone(),
            source.metadata_version.clone(),
            "Renamed Source".into(),
        )
        .expect("rename source metadata");
    let error = project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: "metadata-adopt".into(),
            conversation_id,
            preview_id: preview.id,
            preview_version: preview.version,
            preview_digest: preview.digest,
        })
        .expect_err("changed source metadata must invalidate preview");
    assert_eq!(error.code, "ContextChanged");
    assert_eq!(
        project
            .documents(access)
            .expect("list ordinary documents")
            .iter()
            .filter(|document| document.title == "Target One" || document.title == "Target Two")
            .count(),
        0
    );
}

#[test]
fn duplicate_draft_or_chapter_target_is_rejected_before_preview_write() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Structural adoption").expect("create");
    let access = project.attach("structural-adoption-test".into()).expect("attach");
    let (conversation_id, _) = start_chat_with(
        &project,
        &access,
        ProjectComposer::default(),
        output_three(),
        "structural",
    );
    let refs = draft_refs(&project, &access);
    let duplicate_error = project
        .prepare_chat_adoption(PrepareChatAdoption {
            access: access.clone(),
            operation_id: "duplicate-prepare".into(),
            conversation_id: conversation_id.clone(),
            drafts: vec![refs[0].clone(), refs[0].clone()],
            group_effects: None,
        })
        .expect_err("duplicate draft must be rejected");
    assert_eq!(duplicate_error.code, "InvalidRequest");

    let db = Connection::open(temp.project_path().join("project.sqlite3")).expect("open db");
    db.execute(
        "UPDATE documents SET kind='chapter' WHERE id=?",
        [&refs[1].head.document_id],
    )
    .expect("forge chapter kind for structural test");
    drop(db);
    let chapter_error = project
        .prepare_chat_adoption(PrepareChatAdoption {
            access: access.clone(),
            operation_id: "chapter-mixed-prepare".into(),
            conversation_id,
            drafts: refs,
            group_effects: None,
        })
        .expect_err("chapter/mixed adoption must be rejected");
    assert_eq!(chapter_error.code, "InvalidDocument");
}

#[test]
fn duplicate_target_handles_are_rejected_before_preview_write() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Duplicate target").expect("create");
    let access = project.attach("duplicate-target-test".into()).expect("attach");
    let source = create_source(&project, &access, "target", "Target Source");
    let checkpoint = project
        .checkpoint(CheckpointRequest {
            access: access.clone(),
            expected: source.head.clone(),
            reason: CheckpointReason::Source,
        })
        .expect("checkpoint target source");
    let composer = ProjectComposer {
        text: "Offer alternatives for this source.".into(),
        source_refs: vec![source.head],
        ..ProjectComposer::default()
    };
    let (conversation_id, _) = start_chat_with(
        &project,
        &access,
        composer,
        output_target(&checkpoint.id),
        "duplicate-target",
    );
    let adoption_access = access.clone();
    let error = project
        .prepare_chat_adoption(PrepareChatAdoption {
            access: adoption_access.clone(),
            operation_id: "duplicate-target-prepare".into(),
            conversation_id,
            drafts: draft_refs(&project, &adoption_access),
            group_effects: None,
        })
        .expect_err("duplicate target handles must be rejected");
    assert_eq!(error.code, "InvalidRequest");
}
