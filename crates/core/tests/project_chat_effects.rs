use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{DiscussionBegin, DiscussionFinish};
use webnovel_core::projects::project_chat::{
    AdoptChatPreview, PrepareChatAdoption, ProjectChatDraftRef, ProjectComposer,
    SaveAssistantDraft, SetChatDisposition, StartProjectChat,
};
use webnovel_core::projects::project_chat_output::ChatGroupEffectsOutput;
use webnovel_core::projects::workshop::{
    PreferencePolarity, PreferenceScope, PreferenceStrength, SaveWorkshop, WorkshopPreference,
    WorkshopRelationship, WorkshopRelationshipStatus,
};
use webnovel_core::projects::{
    CheckpointReason, CheckpointRequest, CreateDocument, ProjectAccess, ProjectSession, SaveCause,
    SaveSnapshot,
};

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-chat-effects-{}", Uuid::new_v4()));
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

fn grouped_output(effect_from: &str, effect_to: &str) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I prepared two related records for review.",
        "questions": [],
        "assumptions": [],
        "drafts": [
            {
                "key": "hero-draft", "title": "The River Keeper", "kind": "character",
                "changeSummary": "Adds the keeper of the river gate.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The keeper knows every crossing."}] }]
            },
            {
                "key": "gate-world", "title": "River Gate", "kind": "world",
                "changeSummary": "Adds the gate where the story begins.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The gate opens at first light."}] }]
            }
        ],
        "groupEffects": {
            "relationships": [{
                "key": "keeper-guards-gate",
                "fromRef": effect_from,
                "toRef": effect_to,
                "type": "guards",
                "description": "The keeper is responsible for the river gate.",
                "uncertainty": "The exact reason for the duty is still open."
            }],
            "impacts": [],
            "supersessions": [],
            "placements": []
        }
    }))
    .expect("serialize grouped project-chat output")
}

fn grouped_existing_target_output(existing_handle: &str) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I prepared a revised keeper and a new gate for review.",
        "questions": [],
        "assumptions": [],
        "drafts": [
            {
                "key": "keeper-edit", "title": "Updated River Keeper", "kind": "character",
                "targetHandle": existing_handle,
                "changeSummary": "Clarifies the keeper's responsibility.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The keeper remembers every crossing."}] }]
            },
            {
                "key": "gate-world", "title": "River Gate", "kind": "world",
                "changeSummary": "Adds the gate where the story begins.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The gate opens at first light."}] }]
            }
        ],
        "groupEffects": {
            "relationships": [{
                "key": "keeper-guards-gate",
                "fromRef": existing_handle,
                "toRef": "gate-world",
                "type": "guards",
                "description": "The keeper is responsible for the river gate.",
                "uncertainty": "The exact reason for the duty is still open."
            }],
            "impacts": [],
            "supersessions": [],
            "placements": []
        }
    }))
    .expect("serialize existing-target grouped project-chat output")
}

fn invalid_effect_output(bad_reference: &str, use_as_from: bool) -> String {
    let from = if use_as_from {
        bad_reference
    } else {
        "hero-draft"
    };
    let to = if use_as_from {
        "gate-world"
    } else {
        bad_reference
    };
    grouped_output_with_question_or_assumption(from, to)
}

fn grouped_output_with_question_or_assumption(effect_from: &str, effect_to: &str) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I need to keep this relationship provisional.",
        "questions": [{"key": "open-question", "text": "Who first built the gate?"}],
        "assumptions": [{"key": "open-assumption", "text": "The gate predates the keeper."}],
        "drafts": [
            {
                "key": "hero-draft", "title": "The River Keeper", "kind": "character",
                "changeSummary": "Adds the keeper of the river gate.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The keeper knows every crossing."}] }]
            },
            {
                "key": "gate-world", "title": "River Gate", "kind": "world",
                "changeSummary": "Adds the gate where the story begins.",
                "blocks": [{"type":"paragraph","content":[{"type":"text","text":"The gate opens at first light."}] }]
            }
        ],
        "groupEffects": {
            "relationships": [{
                "key": "invalid-reference",
                "fromRef": effect_from,
                "toRef": effect_to,
                "type": "guards",
                "description": "This must never resolve through a question or assumption.",
                "uncertainty": "unknown"
            }],
            "impacts": [],
            "supersessions": [],
            "placements": []
        }
    }))
    .expect("serialize invalid project-chat output")
}

fn unsupported_effect_output(category: &str) -> String {
    let mut output: Value = serde_json::from_str(&grouped_output("hero-draft", "gate-world"))
        .expect("parse grouped output");
    let effects = output
        .get_mut("groupEffects")
        .and_then(Value::as_object_mut)
        .expect("group effects object");
    let proposal = match category {
        "impacts" => json!([{
            "targetRef": "hero-draft",
            "kind": "possibleTension",
            "reason": "The new relationship may alter the opening conflict."
        }]),
        "supersessions" => json!([{
            "targetRef": "hero-draft",
            "supersededRef": "gate-world",
            "reason": "This provisional record would replace an earlier sketch."
        }]),
        "placements" => json!([{
            "targetRef": "hero-draft",
            "beforeRef": "gate-world"
        }]),
        other => panic!("unsupported test category {other}"),
    };
    effects.insert(category.to_owned(), proposal);
    serde_json::to_string(&output).expect("serialize unsupported effect output")
}

fn start_chat(
    project: &ProjectSession,
    access: &ProjectAccess,
    assistant_text: String,
    prefix: &str,
) -> (
    String,
    webnovel_core::projects::project_chat::ChatMaterialization,
) {
    start_chat_with_composer(
        project,
        access,
        ProjectComposer {
            text: "Develop the keeper and the river gate together.".into(),
            ..ProjectComposer::default()
        },
        assistant_text,
        prefix,
    )
}

fn start_chat_with_composer(
    project: &ProjectSession,
    access: &ProjectAccess,
    composer: ProjectComposer,
    assistant_text: String,
    prefix: &str,
) -> (
    String,
    webnovel_core::projects::project_chat::ChatMaterialization,
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
    let materialization = project
        .materialize_chat_result(started.run.owner)
        .expect("materialize chat result")
        .expect("materialization event");
    (conversation.id, materialization)
}

fn drafts(project: &ProjectSession, access: &ProjectAccess) -> Vec<ProjectChatDraftRef> {
    project
        .read_project_conversation(
            webnovel_core::projects::project_chat::ReadProjectConversation {
                access: access.clone(),
                before: None,
                limit: 100,
            },
        )
        .expect("read project conversation")
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
    conversation_id: &str,
    operation_id: &str,
    draft_refs: Vec<ProjectChatDraftRef>,
    group_effects: Option<ChatGroupEffectsOutput>,
) -> webnovel_core::projects::project_chat::ChatAdoptionPreview {
    project
        .prepare_chat_adoption(PrepareChatAdoption {
            access: access.clone(),
            operation_id: operation_id.into(),
            conversation_id: conversation_id.into(),
            drafts: draft_refs,
            group_effects,
        })
        .expect("prepare chat adoption")
}

fn adopt(
    project: &ProjectSession,
    access: &ProjectAccess,
    conversation_id: &str,
    operation_id: &str,
    preview: &webnovel_core::projects::project_chat::ChatAdoptionPreview,
) -> webnovel_core::projects::project_chat::ChatAdoptionAck {
    project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: operation_id.into(),
            conversation_id: conversation_id.into(),
            preview_id: preview.id.clone(),
            preview_version: preview.version.clone(),
            preview_digest: preview.digest.clone(),
        })
        .expect("adopt chat preview")
}

fn assert_no_material_targets(project: &ProjectSession, access: &ProjectAccess) {
    assert!(project
        .documents().list(access.clone())
        .expect("list documents")
        .iter()
        .all(|document| document.role != webnovel_core::projects::DocumentRole::Ordinary));
}

#[test]
fn draft_only_effects_resolve_to_ordinary_targets_and_replay_once() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Grouped effects").expect("create");
    let access = project.documents().attach("effects-success".into()).expect("attach");
    let (conversation_id, materialization) = start_chat(
        &project,
        &access,
        grouped_output("hero-draft", "gate-world"),
        "effects-success",
    );
    assert!(materialization.output_valid);
    assert_eq!(materialization.draft_ids.len(), 2);
    assert!(materialization.group_effects.is_some());
    let draft_ids = drafts(&project, &access)
        .into_iter()
        .map(|draft| draft.head.document_id)
        .collect::<Vec<_>>();
    let preview = prepare(
        &project,
        &access,
        &conversation_id,
        "effects-prepare",
        drafts(&project, &access),
        None,
    );
    assert_eq!(preview.targets.len(), 2);
    assert_eq!(
        preview
            .effects
            .as_ref()
            .unwrap()
            .proposed_relationships
            .len(),
        1
    );
    assert!(preview
        .targets
        .iter()
        .all(|target| !draft_ids.contains(&target.document_id)));
    let proposed = &preview.effects.as_ref().unwrap().proposed_relationships[0];
    assert!(!draft_ids.contains(&proposed.from_document_id));
    assert!(!draft_ids.contains(&proposed.to_document_id));
    assert!(preview.targets.iter().all(|target| target.before.is_none()));

    let epoch_before = project.context().source_epoch().expect("read source epoch");
    let ack = adopt(
        &project,
        &access,
        &conversation_id,
        "effects-adopt",
        &preview,
    );
    assert_eq!(ack.documents.len(), 2);
    let ordinary_by_id = ack
        .documents
        .iter()
        .map(|document| (document.head.document_id.clone(), document))
        .collect::<HashMap<_, _>>();
    let workshop = project
        .workshop().read(access.clone())
        .expect("read workshop");
    assert_eq!(workshop.version, "1");
    assert_eq!(workshop.state.relationships.len(), 1);
    let relationship = &workshop.state.relationships[0];
    assert_eq!(relationship.from_document_id, proposed.from_document_id);
    assert_eq!(relationship.to_document_id, proposed.to_document_id);
    assert_eq!(relationship.source_heads.len(), 2);
    assert_eq!(
        relationship.source_heads[0],
        ordinary_by_id[&relationship.from_document_id].head
    );
    assert_eq!(
        relationship.source_heads[1],
        ordinary_by_id[&relationship.to_document_id].head
    );
    assert!(ack
        .documents
        .iter()
        .all(|document| { document.role == webnovel_core::projects::DocumentRole::Ordinary }));
    assert_eq!(
        project.context().source_epoch().expect("read source epoch"),
        (epoch_before.parse::<u64>().expect("epoch") + 1).to_string()
    );

    let replay = adopt(
        &project,
        &access,
        &conversation_id,
        "effects-adopt",
        &preview,
    );
    assert_eq!(
        serde_json::to_value(&replay).expect("replay json"),
        serde_json::to_value(&ack).expect("ack json")
    );
    assert_eq!(
        project.context().source_epoch().expect("read source epoch"),
        "1"
    );
    assert_eq!(
        project
            .workshop().read(access)
            .expect("read workshop")
            .state
            .relationships
            .len(),
        1
    );
}

#[test]
fn excluding_an_effect_endpoint_refuses_before_any_write() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Excluded effect").expect("create");
    let access = project.documents().attach("effects-excluded".into()).expect("attach");
    let (conversation_id, materialization) = start_chat(
        &project,
        &access,
        grouped_output("hero-draft", "gate-world"),
        "effects-excluded",
    );
    assert!(materialization.group_effects.is_some());
    let all_drafts = drafts(&project, &access);
    assert_eq!(all_drafts.len(), 2);
    let error = project
        .prepare_chat_adoption(PrepareChatAdoption {
            access: access.clone(),
            operation_id: "effects-excluded-prepare".into(),
            conversation_id,
            drafts: vec![all_drafts[0].clone()],
            group_effects: None,
        })
        .expect_err("an effect cannot name an unselected response draft");
    assert_eq!(error.code, "InvalidRequest");
    assert_no_material_targets(&project, &access);
    assert_eq!(
        project.context().source_epoch().expect("read source epoch"),
        "0"
    );
    let db = Connection::open(temp.project_path().join("project.sqlite3")).expect("open db");
    let previews: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM conversation_items WHERE kind='adoptionPreview'",
            [],
            |row| row.get(0),
        )
        .expect("count previews");
    assert_eq!(previews, 0);
}

#[test]
fn question_and_assumption_keys_cannot_be_used_as_effect_endpoints() {
    for (suffix, bad_key, from_side) in [
        ("question", "open-question", true),
        ("assumption", "open-assumption", false),
    ] {
        let temp = TempProject::new();
        let project =
            ProjectSession::create(temp.project_path(), "Invalid effect refs").expect("create");
        let access = project
            .documents().attach(format!("effects-invalid-{suffix}"))
            .expect("attach");
        let (conversation_id, materialization) = start_chat(
            &project,
            &access,
            invalid_effect_output(bad_key, from_side),
            suffix,
        );
        assert!(!materialization.output_valid);
        assert!(materialization.draft_ids.is_empty());
        assert!(materialization
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("outside the frozen sources")));
        assert!(project
            .read_project_conversation(
                webnovel_core::projects::project_chat::ReadProjectConversation {
                    access,
                    before: None,
                    limit: 40,
                }
            )
            .expect("read invalid response conversation")
            .drafts
            .is_empty());
        assert!(!conversation_id.is_empty());
    }
}

#[test]
fn changed_draft_source_and_workshop_state_fence_effect_adoption() {
    // The assistant draft is the target-side fence: changing it after preview
    // must leave both ordinary material and Workshop untouched.
    {
        let temp = TempProject::new();
        let project =
            ProjectSession::create(temp.project_path(), "Changed draft effect").expect("create");
        let access = project
            .documents().attach("effects-draft-fence".into())
            .expect("attach");
        let (conversation_id, materialization) = start_chat(
            &project,
            &access,
            grouped_output("hero-draft", "gate-world"),
            "effects-draft-fence",
        );
        assert!(materialization.output_valid);
        let preview = prepare(
            &project,
            &access,
            &conversation_id,
            "effects-draft-prepare",
            drafts(&project, &access),
            None,
        );
        let draft = project
            .read_project_conversation(
                webnovel_core::projects::project_chat::ReadProjectConversation {
                    access: access.clone(),
                    before: None,
                    limit: 40,
                },
            )
            .expect("read draft")
            .drafts
            .into_iter()
            .next()
            .expect("draft");
        project
            .save_assistant_draft(SaveAssistantDraft {
                conversation_id: conversation_id.clone(),
                disposition_version: draft.disposition_version,
                snapshot: SaveSnapshot {
                    access: access.clone(),
                    operation_id: "effects-draft-edit".into(),
                    expected: draft.document.head,
                    local_generation: "1".into(),
                    body: body("The author changed the assistant draft after preview."),
                    cause: SaveCause::Typing,
                },
            })
            .expect("edit assistant draft");
        let error = project
            .adopt_chat_preview(AdoptChatPreview {
                access: access.clone(),
                operation_id: "effects-draft-adopt".into(),
                conversation_id,
                preview_id: preview.id,
                preview_version: preview.version,
                preview_digest: preview.digest,
            })
            .expect_err("changed draft must fence effect adoption");
        assert_eq!(error.code, "DraftChanged");
        assert_no_material_targets(&project, &access);
        assert!(project
            .workshop().read(access)
            .expect("read workshop")
            .state
            .relationships
            .is_empty());
    }

    // A source epoch change is global and must invalidate the frozen effect
    // context before any adoption document is written.
    {
        let temp = TempProject::new();
        let project =
            ProjectSession::create(temp.project_path(), "Changed source effect").expect("create");
        let access = project
            .documents().attach("effects-source-fence".into())
            .expect("attach");
        let (conversation_id, materialization) = start_chat(
            &project,
            &access,
            grouped_output("hero-draft", "gate-world"),
            "effects-source-fence",
        );
        assert!(materialization.output_valid);
        let preview = prepare(
            &project,
            &access,
            &conversation_id,
            "effects-source-prepare",
            drafts(&project, &access),
            None,
        );
        project
            .documents().create(CreateDocument {
                access: access.clone(),
                operation_id: "effects-source-change".into(),
                document_id: "new-source".into(),
                title: "A New Source".into(),
                kind: "world".into(),
                body: body("This source appeared after the preview."),
            })
            .expect("change source epoch");
        let error = project
            .adopt_chat_preview(AdoptChatPreview {
                access: access.clone(),
                operation_id: "effects-source-adopt".into(),
                conversation_id,
                preview_id: preview.id,
                preview_version: preview.version,
                preview_digest: preview.digest,
            })
            .expect_err("changed source epoch must fence effect adoption");
        assert_eq!(error.code, "ContextChanged");
        assert_eq!(
            project
                .documents().list(access.clone())
                .expect("list documents")
                .iter()
                .filter(|document| document.title == "The River Keeper"
                    || document.title == "River Gate")
                .count(),
            0
        );
        assert!(project
            .workshop().read(access)
            .expect("read workshop")
            .state
            .relationships
            .is_empty());
    }

    // Workshop state changes are a separate fence from the story source
    // epoch, so an unrelated saved preference still invalidates the preview.
    {
        let temp = TempProject::new();
        let project =
            ProjectSession::create(temp.project_path(), "Changed workshop effect").expect("create");
        let access = project
            .documents().attach("effects-workshop-fence".into())
            .expect("attach");
        let (conversation_id, materialization) = start_chat(
            &project,
            &access,
            grouped_output("hero-draft", "gate-world"),
            "effects-workshop-fence",
        );
        assert!(materialization.output_valid);
        let preview = prepare(
            &project,
            &access,
            &conversation_id,
            "effects-workshop-prepare",
            drafts(&project, &access),
            None,
        );
        let mut workshop = project
            .workshop().read(access.clone())
            .expect("read workshop");
        workshop.state.preferences.push(WorkshopPreference {
            id: "effects-preference".into(),
            label: "Keep the gate mysterious".into(),
            family: "mystery".into(),
            meaning: "Do not resolve the gate's origin yet".into(),
            examples: "The first chapter leaves the question open".into(),
            timing: "early".into(),
            polarity: PreferencePolarity::Want,
            strength: PreferenceStrength::Soft,
            scope: PreferenceScope::Project,
            target_id: None,
            confirmed: true,
        });
        project
            .workshop().save(SaveWorkshop {
                access: access.clone(),
                operation_id: "effects-workshop-change".into(),
                expected_version: workshop.version,
                state: workshop.state,
            })
            .expect("change workshop state");
        let error = project
            .adopt_chat_preview(AdoptChatPreview {
                access: access.clone(),
                operation_id: "effects-workshop-adopt".into(),
                conversation_id,
                preview_id: preview.id,
                preview_version: preview.version,
                preview_digest: preview.digest,
            })
            .expect_err("changed Workshop state must fence effect adoption");
        assert_eq!(error.code, "WorkshopChanged");
        assert_no_material_targets(&project, &access);
        assert_eq!(
            project
                .workshop().read(access)
                .expect("read workshop")
                .state
                .relationships
                .len(),
            0
        );
    }
}

#[test]
fn relationship_write_failure_rolls_back_materialization_and_allows_same_operation_retry() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.project_path(), "Effects transaction rollback")
        .expect("create");
    let access = project
        .documents().attach("effects-transaction".into())
        .expect("attach");
    let (conversation_id, materialization) = start_chat(
        &project,
        &access,
        grouped_output("hero-draft", "gate-world"),
        "effects-transaction",
    );
    let preview = prepare(
        &project,
        &access,
        &conversation_id,
        "effects-transaction-prepare",
        drafts(&project, &access),
        None,
    );
    assert!(materialization.output_valid);
    let epoch_before = project.context().source_epoch().expect("read source epoch");
    let db = Connection::open(temp.project_path().join("project.sqlite3")).expect("open db");
    db.execute_batch(
        "CREATE TRIGGER fail_chat_relationship_state_insert
         BEFORE INSERT ON workshop_state
         BEGIN SELECT RAISE(ABORT,'injected relationship persistence failure'); END;
         CREATE TRIGGER fail_chat_relationship_state_update
         BEFORE UPDATE ON workshop_state
         WHEN NEW.version <> OLD.version
         BEGIN SELECT RAISE(ABORT,'injected relationship persistence failure'); END;",
    )
    .expect("install relationship write fault");

    let error = project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: "effects-transaction-adopt".into(),
            conversation_id: conversation_id.clone(),
            preview_id: preview.id.clone(),
            preview_version: preview.version.clone(),
            preview_digest: preview.digest.clone(),
        })
        .expect_err("Workshop persistence fault must abort adoption");
    assert!(!error.code.is_empty());

    let ordinary_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM documents WHERE role='ordinary' AND title IN ('The River Keeper','River Gate')",
            [],
            |row| row.get(0),
        )
        .expect("count rolled-back ordinary documents");
    assert_eq!(ordinary_count, 0);
    let relationship_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM workshop_snapshots WHERE operation_id='effects-transaction-adopt'",
            [],
            |row| row.get(0),
        )
        .expect("count rolled-back Workshop snapshot");
    assert_eq!(relationship_count, 0);
    let decision_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM conversation_items WHERE kind='adoptionDecision' AND operation_id='effects-transaction-adopt'",
            [],
            |row| row.get(0),
        )
        .expect("count rolled-back adoption decision");
    assert_eq!(decision_count, 0);
    let receipt_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM command_receipts WHERE operation_id='effects-transaction-adopt'",
            [],
            |row| row.get(0),
        )
        .expect("count rolled-back adoption receipt");
    assert_eq!(receipt_count, 0);
    let epoch_after_failure: i64 = db
        .query_row(
            "SELECT context_source_epoch FROM project WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .expect("read rolled-back source epoch");
    assert_eq!(epoch_after_failure.to_string(), epoch_before);
    assert!(
        project
            .read_chat_adoption_preview(access.clone(), conversation_id.clone(), preview.id.clone())
            .is_ok(),
        "the immutable preview remains available for an explicit retry"
    );

    db.execute_batch(
        "DROP TRIGGER fail_chat_relationship_state_insert;
         DROP TRIGGER fail_chat_relationship_state_update",
    )
    .expect("remove relationship write fault");
    drop(db);
    let ack = adopt(
        &project,
        &access,
        &conversation_id,
        "effects-transaction-adopt",
        &preview,
    );
    assert_eq!(ack.documents.len(), 2);
    assert_eq!(
        project
            .workshop().read(access)
            .expect("read committed Workshop")
            .state
            .relationships
            .len(),
        1
    );
}

#[test]
fn unsupported_grouped_effect_categories_refuse_before_any_write() {
    for category in ["impacts", "supersessions", "placements"] {
        let temp = TempProject::new();
        let project =
            ProjectSession::create(temp.project_path(), "Unsupported effects").expect("create");
        let access = project
            .documents().attach(format!("effects-unsupported-{category}"))
            .expect("attach");
        let (conversation_id, materialization) = start_chat(
            &project,
            &access,
            unsupported_effect_output(category),
            category,
        );
        assert!(materialization.output_valid);
        assert!(materialization.group_effects.is_some());
        let error = project
            .prepare_chat_adoption(PrepareChatAdoption {
                access: access.clone(),
                operation_id: format!("effects-unsupported-{category}-prepare"),
                conversation_id,
                drafts: drafts(&project, &access),
                group_effects: None,
            })
            .expect_err("unsupported grouped effects must remain review-only");
        assert_eq!(error.code, "InvalidRequest");
        assert_no_material_targets(&project, &access);
        assert_eq!(
            project.context().source_epoch().expect("read source epoch"),
            "0"
        );
        let db = Connection::open(temp.project_path().join("project.sqlite3")).expect("open db");
        let previews: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM conversation_items WHERE kind='adoptionPreview'",
                [],
                |row| row.get(0),
            )
            .expect("count previews");
        assert_eq!(previews, 0);
    }
}

#[test]
fn existing_relationship_endpoint_drift_refuses_without_partial_adoption() {
    let temp = TempProject::new();
    let project =
        ProjectSession::create(temp.project_path(), "Relationship endpoint drift").expect("create");
    let access = project
        .documents().attach("effects-relationship-drift".into())
        .expect("attach");

    let character = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "relationship-character-create".into(),
            document_id: "keeper-character".into(),
            title: "River Keeper".into(),
            kind: "character".into(),
            body: body("The keeper guards every crossing."),
        })
        .expect("create character source");
    let witness = project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: "relationship-witness-create".into(),
            document_id: "keeper-witness".into(),
            title: "Keeper Witness".into(),
            kind: "world".into(),
            body: body("The witness remembers the keeper's oath."),
        })
        .expect("create relationship endpoint source");
    let character_checkpoint = project
        .documents().checkpoint(CheckpointRequest {
            access: access.clone(),
            expected: character.head.clone(),
            reason: CheckpointReason::Source,
        })
        .expect("checkpoint character source");
    let _witness_checkpoint = project
        .documents().checkpoint(CheckpointRequest {
            access: access.clone(),
            expected: witness.head.clone(),
            reason: CheckpointReason::Source,
        })
        .expect("checkpoint relationship endpoint source");

    let mut workshop = project
        .workshop().read(access.clone())
        .expect("read initial workshop");
    let existing_relationship = WorkshopRelationship {
        id: "keeper-remembers-witness".into(),
        from_document_id: character.head.document_id.clone(),
        to_document_id: witness.head.document_id.clone(),
        relationship_type: "remembers".into(),
        description: "The keeper remembers what the witness saw.".into(),
        uncertainty: "The witness may be mistaken about the final crossing.".into(),
        status: WorkshopRelationshipStatus::Chosen,
        source_heads: vec![character.head.clone(), witness.head.clone()],
    };
    workshop
        .state
        .relationships
        .push(existing_relationship.clone());
    project
        .workshop().save(SaveWorkshop {
            access: access.clone(),
            operation_id: "relationship-save-existing".into(),
            expected_version: workshop.version,
            state: workshop.state,
        })
        .expect("save existing relationship");

    let composer = ProjectComposer {
        text: "Revise the keeper and add the river gate.".into(),
        source_refs: vec![character.head.clone(), witness.head.clone()],
        ..ProjectComposer::default()
    };
    let (conversation_id, materialization) = start_chat_with_composer(
        &project,
        &access,
        composer,
        grouped_existing_target_output(&character_checkpoint.id),
        "effects-relationship-drift",
    );
    assert!(materialization.output_valid);
    let preview = prepare(
        &project,
        &access,
        &conversation_id,
        "effects-relationship-drift-prepare",
        drafts(&project, &access),
        None,
    );
    let effects = preview.effects.as_ref().expect("effect manifest");
    assert_eq!(effects.relationship_dependencies.len(), 1);
    assert_eq!(
        effects.relationship_dependencies[0].relationship_id,
        existing_relationship.id
    );
    assert_eq!(
        effects.relationship_dependencies[0].to_document_id,
        witness.head.document_id
    );

    // This endpoint is not an adoption target, so its changed head must be
    // caught by the existing relationship dependency fence before any new
    // draft target or Workshop relationship is materialized.
    let changed_witness = project
        .documents().save(SaveSnapshot {
            access: access.clone(),
            operation_id: "relationship-endpoint-drift".into(),
            expected: witness.head.clone(),
            local_generation: "1".into(),
            body: body("The witness changed their account after the preview."),
            cause: SaveCause::Typing,
        })
        .expect("change unselected relationship endpoint");
    let epoch_after_drift = project.context().source_epoch().expect("read drift epoch");
    let workshop_after_drift = project
        .workshop().read(access.clone())
        .expect("read workshop after endpoint drift");
    let character_before_adoption = project
        .documents().read(access.clone(), character.head.document_id.clone())
        .expect("read existing adoption target");
    let error = project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: "effects-relationship-drift-adopt".into(),
            conversation_id,
            preview_id: preview.id,
            preview_version: preview.version,
            preview_digest: preview.digest,
        })
        .expect_err("relationship endpoint drift must refuse adoption");
    assert_eq!(error.code, "ContextChanged");

    let documents = project.documents().list(access.clone()).expect("list documents");
    assert!(documents
        .iter()
        .all(|document| document.title != "River Gate"));
    assert_eq!(
        project
            .documents().read(access.clone(), character.head.document_id.clone())
            .expect("read target after rejected adoption")
            .head,
        character_before_adoption.head
    );
    assert_eq!(
        project
            .documents().read(access.clone(), witness.head.document_id.clone())
            .expect("read changed endpoint")
            .head,
        changed_witness.head
    );
    assert_eq!(
        project
            .context().source_epoch()
            .expect("read epoch after rejection"),
        epoch_after_drift
    );
    assert_eq!(
        project
            .workshop().read(access.clone())
            .expect("read workshop after rejected adoption")
            .state
            .relationships,
        workshop_after_drift.state.relationships
    );

    let db = Connection::open(temp.project_path().join("project.sqlite3")).expect("open db");
    let decisions: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM conversation_items WHERE kind='adoptionDecision' AND operation_id='effects-relationship-drift-adopt'",
            [],
            |row| row.get(0),
        )
        .expect("count adoption decisions");
    let receipts: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM command_receipts WHERE operation_id='effects-relationship-drift-adopt'",
            [],
            |row| row.get(0),
        )
        .expect("count adoption receipts");
    assert_eq!(decisions, 0);
    assert_eq!(receipts, 0);
}

#[test]
fn rejected_selected_group_member_refuses_without_partial_adoption() {
    let temp = TempProject::new();
    let project =
        ProjectSession::create(temp.project_path(), "Rejected grouped member").expect("create");
    let access = project
        .documents().attach("effects-rejected-member".into())
        .expect("attach");
    let (conversation_id, materialization) = start_chat(
        &project,
        &access,
        grouped_output("hero-draft", "gate-world"),
        "effects-rejected-member",
    );
    assert!(materialization.output_valid);
    let draft_refs = drafts(&project, &access);
    assert_eq!(draft_refs.len(), 2);
    let preview = prepare(
        &project,
        &access,
        &conversation_id,
        "effects-rejected-member-prepare",
        draft_refs.clone(),
        None,
    );

    let rejected = &draft_refs[0];
    project
        .set_chat_disposition(SetChatDisposition {
            access: access.clone(),
            operation_id: "effects-rejected-member-disposition".into(),
            conversation_id: conversation_id.clone(),
            reference_id: rejected.head.document_id.clone(),
            expected_version: rejected.disposition_version.clone(),
            disposition: "rejected".into(),
            rationale: "Keep this member out of the grouped adoption.".into(),
            scope: None,
            unknown_to: None,
        })
        .expect("reject selected grouped member");
    let epoch_before_adoption = project.context().source_epoch().expect("read epoch");
    let workshop_before_adoption = project
        .workshop().read(access.clone())
        .expect("read workshop");
    let error = project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: "effects-rejected-member-adopt".into(),
            conversation_id: conversation_id.clone(),
            preview_id: preview.id,
            preview_version: preview.version,
            preview_digest: preview.digest,
        })
        .expect_err("a rejected grouped member must refuse the whole adoption");
    assert_eq!(error.code, "DraftNotAdoptable");
    assert_no_material_targets(&project, &access);
    assert_eq!(
        project
            .context().source_epoch()
            .expect("read epoch after rejection"),
        epoch_before_adoption
    );
    assert_eq!(
        project
            .workshop().read(access.clone())
            .expect("read workshop after rejection")
            .state,
        workshop_before_adoption.state
    );
    let conversation = project
        .read_project_conversation(
            webnovel_core::projects::project_chat::ReadProjectConversation {
                access: access.clone(),
                before: None,
                limit: 100,
            },
        )
        .expect("read grouped conversation after rejection");
    assert_eq!(
        conversation
            .drafts
            .iter()
            .find(|draft| draft.document.head.document_id == rejected.head.document_id)
            .expect("rejected draft")
            .disposition,
        "rejected"
    );
    assert!(conversation
        .drafts
        .iter()
        .filter(|draft| draft.document.head.document_id != rejected.head.document_id)
        .all(|draft| draft.disposition == "pending"));

    let db = Connection::open(temp.project_path().join("project.sqlite3")).expect("open db");
    let decisions: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM conversation_items WHERE kind='adoptionDecision' AND operation_id='effects-rejected-member-adopt'",
            [],
            |row| row.get(0),
        )
        .expect("count rejected-member adoption decisions");
    let receipts: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM command_receipts WHERE operation_id='effects-rejected-member-adopt'",
            [],
            |row| row.get(0),
        )
        .expect("count rejected-member adoption receipts");
    assert_eq!(decisions, 0);
    assert_eq!(receipts, 0);
}
