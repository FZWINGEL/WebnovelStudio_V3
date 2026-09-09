//! Backup/recovery qualification for the project-chat projection.
//!
//! This file is intentionally kept separate from the first draft lifecycle
//! suite.  Register it in `tests/integration.rs` when the coordinated transfer
//! slice is enabled.

use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::BasisKind;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::projects::discussions::{DiscussionBegin, DiscussionFinish, FeedbackIntent};
use webnovel_core::projects::project_chat::{
    AdoptChatPreview, HistoricalConversationRef, PrepareChatAdoption, ProjectChapterComposer,
    ProjectChatDraftRef, ReadProjectChatHistory, SaveAssistantDraft, StartProjectChapter,
    StartProjectChat,
};
use webnovel_core::projects::project_chat::{
    ProjectComposer, ReadProjectConversation, SaveProjectComposer,
};
use webnovel_core::projects::{CreateDocument, ProjectSession, SaveCause, SaveSnapshot};
use webnovel_core::transfer::{BackupManifest, create_backup, recover_backup};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

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

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (key, canonicalize(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        other => other,
    }
}

fn logical_hash(value: &Value) -> String {
    sha256(
        serde_json::to_string(&canonicalize(value.clone()))
            .expect("serialize logical hash value")
            .as_bytes(),
    )
}

fn archive_entries(path: &std::path::Path) -> (BackupManifest, Vec<u8>) {
    let file = File::open(path).expect("open backup archive");
    let mut archive = ZipArchive::new(file).expect("read backup archive");
    let mut manifest_bytes = Vec::new();
    archive
        .by_name("manifest.json")
        .expect("manifest entry")
        .read_to_end(&mut manifest_bytes)
        .expect("read manifest");
    let manifest: BackupManifest =
        serde_json::from_slice(&manifest_bytes).expect("parse backup manifest");
    let mut database = Vec::new();
    archive
        .by_name("project.sqlite3")
        .expect("database entry")
        .read_to_end(&mut database)
        .expect("read backup database");
    (manifest, database)
}

fn write_archive(path: &std::path::Path, manifest: &BackupManifest, database: &[u8]) {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("create resealed archive");
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    archive
        .start_file("manifest.json", options)
        .expect("manifest entry");
    archive
        .write_all(&serde_json::to_vec(manifest).expect("serialize manifest"))
        .expect("write manifest");
    archive
        .start_file("project.sqlite3", options)
        .expect("database entry");
    archive.write_all(database).expect("write database");
    archive.finish().expect("finish resealed archive");
}

fn reseal_with_tampered_relationship_endpoint(
    source_archive: &std::path::Path,
    tampered_archive: &std::path::Path,
    preview: &webnovel_core::projects::project_chat::ChatAdoptionPreview,
    replacement: &webnovel_core::projects::DocumentRecord,
) {
    let (mut manifest, database) = archive_entries(source_archive);
    let database_path = source_archive.with_extension("tampered.sqlite3");
    fs::write(&database_path, database).expect("write extracted database");
    let connection = Connection::open(&database_path).expect("open extracted database");
    let payload: String = connection
        .query_row(
            "SELECT payload_json FROM conversation_items WHERE kind='adoptionPreview' AND reference_id=?",
            [preview.id.as_str()],
            |row| row.get(0),
        )
        .expect("read adoption preview payload");
    let mut stored: Value = serde_json::from_str(&payload).expect("parse adoption preview payload");
    let mut altered_preview = serde_json::to_value(preview).expect("serialize preview");
    altered_preview["effects"]["proposedRelationships"][0]["fromDocumentId"] =
        json!(replacement.head.document_id);
    altered_preview["effects"]["proposedRelationships"][0]["fromHead"] =
        serde_json::to_value(&replacement.head).expect("serialize replacement head");
    altered_preview["digest"] = Value::String(String::new());
    let altered_digest = logical_hash(&altered_preview);
    altered_preview["digest"] = json!(altered_digest);
    stored["preview"]["effects"] = altered_preview["effects"].clone();
    stored["preview"]["digest"] = altered_preview["digest"].clone();
    let tampered_payload =
        serde_json::to_string(&canonicalize(stored)).expect("serialize canonical tampered payload");
    connection
        .execute_batch("DROP TRIGGER conversation_items_immutable_update;")
        .expect("drop immutable test trigger");
    connection
        .execute(
            "UPDATE conversation_items SET payload_json=?,payload_hash=? WHERE kind='adoptionPreview' AND reference_id=?",
            rusqlite::params![
                tampered_payload,
                sha256(tampered_payload.as_bytes()),
                preview.id.as_str()
            ],
        )
        .expect("tamper adoption preview manifest");
    drop(connection);
    let tampered_database = fs::read(&database_path).expect("read tampered database");
    manifest.database_sha256 = sha256(&tampered_database);
    write_archive(tampered_archive, &manifest, &tampered_database);
}

fn reseal_as_legacy_grouped_snapshot(
    source_archive: &std::path::Path,
    legacy_archive: &std::path::Path,
    request: &AdoptChatPreview,
    preview: &webnovel_core::projects::project_chat::ChatAdoptionPreview,
) -> String {
    let (mut manifest, database) = archive_entries(source_archive);
    let database_path = source_archive.with_extension("legacy.sqlite3");
    fs::write(&database_path, database).expect("write extracted legacy database");
    let connection = Connection::open(&database_path).expect("open extracted legacy database");
    let old_payload_hash = logical_hash(
        &serde_json::to_value((request, &preview.effects)).expect("serialize legacy hash input"),
    );
    let decision_id: String = connection
        .query_row(
            "SELECT reference_id FROM conversation_items WHERE kind='adoptionDecision' AND operation_id=?",
            [request.operation_id.as_str()],
            |row| row.get(0),
        )
        .expect("read adoption decision identity");
    let decision_payload: String = connection
        .query_row(
            "SELECT payload_json FROM conversation_items WHERE kind='adoptionDecision' AND reference_id=?",
            [decision_id.as_str()],
            |row| row.get(0),
        )
        .expect("read adoption decision payload");
    let mut decision: Value =
        serde_json::from_str(&decision_payload).expect("parse adoption decision payload");
    decision
        .as_object_mut()
        .expect("decision object")
        .remove("snapshotPayloadHash");
    let decision_json = serde_json::to_string(&decision).expect("serialize legacy decision");
    connection
        .execute_batch(
            "DROP TRIGGER conversation_items_immutable_update;
             DROP TRIGGER workshop_snapshots_no_update;",
        )
        .expect("drop legacy test triggers");
    connection
        .execute(
            "UPDATE conversation_items SET payload_json=?,payload_hash=? WHERE kind='adoptionDecision' AND reference_id=?",
            rusqlite::params![decision_json, sha256(decision_json.as_bytes()), decision_id],
        )
        .expect("remove new decision binding");
    connection
        .execute(
            "UPDATE workshop_snapshots SET payload_hash=? WHERE operation_namespace=? AND operation_id=?",
            rusqlite::params![
                old_payload_hash,
                request.access.operation_namespace,
                request.operation_id
            ],
        )
        .expect("restore legacy snapshot hash");
    drop(connection);
    let legacy_database = fs::read(&database_path).expect("read legacy database");
    manifest.database_sha256 = sha256(&legacy_database);
    write_archive(legacy_archive, &manifest, &legacy_database);
    old_payload_hash
}

fn reseal_with_new_snapshot_binding_tamper(
    source_archive: &std::path::Path,
    tampered_archive: &std::path::Path,
    operation_namespace: &str,
    operation_id: &str,
    tamper_command_receipt: bool,
) {
    let (mut manifest, database) = archive_entries(source_archive);
    let database_path = source_archive.with_extension(if tamper_command_receipt {
        "command-tampered.sqlite3"
    } else {
        "snapshot-tampered.sqlite3"
    });
    fs::write(&database_path, database).expect("write extracted binding database");
    let connection = Connection::open(&database_path).expect("open extracted binding database");
    connection
        .execute_batch(if tamper_command_receipt {
            "DROP TRIGGER receipts_no_update;"
        } else {
            "DROP TRIGGER workshop_snapshots_no_update;"
        })
        .expect("drop binding test trigger");
    let tampered_hash = "f".repeat(64);
    if tamper_command_receipt {
        connection
            .execute(
                "UPDATE command_receipts SET payload_hash=? WHERE operation_namespace=? AND operation_id=?",
                rusqlite::params![tampered_hash, operation_namespace, operation_id],
            )
            .expect("tamper command receipt hash");
    } else {
        connection
            .execute(
                "UPDATE workshop_snapshots SET payload_hash=? WHERE operation_namespace=? AND operation_id=?",
                rusqlite::params![tampered_hash, operation_namespace, operation_id],
            )
            .expect("tamper snapshot binding hash");
    }
    drop(connection);
    let tampered_database = fs::read(&database_path).expect("read binding tampered database");
    manifest.database_sha256 = sha256(&tampered_database);
    write_archive(tampered_archive, &manifest, &tampered_database);
}

fn reseal_with_copied_draft_reference(
    source_archive: &std::path::Path,
    tampered_archive: &std::path::Path,
    current_preview_id: &str,
    copied_preview_id: &str,
) {
    let (mut manifest, database) = archive_entries(source_archive);
    let database_path = source_archive.with_extension("copied-draft.sqlite3");
    fs::write(&database_path, database).expect("write extracted copied-draft database");
    let connection = Connection::open(&database_path).expect("open extracted copied-draft database");
    let current_payload: String = connection
        .query_row(
            "SELECT payload_json FROM conversation_items WHERE kind='adoptionPreview' AND reference_id=?",
            [current_preview_id],
            |row| row.get(0),
        )
        .expect("read current adoption preview");
    let copied_payload: String = connection
        .query_row(
            "SELECT payload_json FROM conversation_items WHERE kind='adoptionPreview' AND reference_id=?",
            [copied_preview_id],
            |row| row.get(0),
        )
        .expect("read copied adoption preview");
    let mut current: Value = serde_json::from_str(&current_payload).expect("parse current preview");
    let copied: Value = serde_json::from_str(&copied_payload).expect("parse copied preview");
    let copied_target = copied["preview"]["targets"][0].clone();
    let current_target = current["preview"]["targets"]
        .as_array_mut()
        .and_then(|targets| targets.first_mut())
        .expect("current preview target");
    for field in ["draft", "draftDocumentId", "draftRevisionId", "dispositionVersion"] {
        current_target[field] = copied_target[field].clone();
    }
    current["preview"]["digest"] = Value::String(String::new());
    let digest = logical_hash(&current["preview"]);
    current["preview"]["digest"] = Value::String(digest);
    let tampered_payload =
        serde_json::to_string(&canonicalize(current)).expect("serialize copied-draft preview");
    connection
        .execute_batch("DROP TRIGGER conversation_items_immutable_update;")
        .expect("drop copied-draft test trigger");
    connection
        .execute(
            "UPDATE conversation_items SET payload_json=?,payload_hash=? WHERE kind='adoptionPreview' AND reference_id=?",
            rusqlite::params![
                tampered_payload,
                sha256(tampered_payload.as_bytes()),
                current_preview_id
            ],
        )
        .expect("tamper copied-draft preview reference");
    drop(connection);
    let tampered_database = fs::read(&database_path).expect("read copied-draft database");
    manifest.database_sha256 = sha256(&tampered_database);
    write_archive(tampered_archive, &manifest, &tampered_database);
}

fn historical_ref(project: &ProjectSession, conversation_id: String) -> HistoricalConversationRef {
    HistoricalConversationRef {
        project_id: project.info.project_id.clone(),
        operation_namespace: project.info.operation_namespace.clone(),
        conversation_id,
    }
}

fn read_history(
    project: &ProjectSession,
    access: &webnovel_core::projects::ProjectAccess,
    conversation: HistoricalConversationRef,
) -> webnovel_core::projects::CoreResult<
    webnovel_core::projects::project_chat::HistoricalConversation,
> {
    project.read_project_chat_history(ReadProjectChatHistory {
        access: access.clone(),
        conversation,
        before: None,
        limit: 40,
    })
}

fn materialize_grouped_and_adopt(
    project: &ProjectSession,
    access: &webnovel_core::projects::ProjectAccess,
    prefix: &str,
) -> (
    String,
    webnovel_core::projects::project_chat::ChatAdoptionPreview,
    webnovel_core::projects::project_chat::ChatAdoptionAck,
) {
    let conversation = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read project conversation");
    let composer = ProjectComposer {
        text: "Develop the river keeper and gate together.".into(),
        ..ProjectComposer::default()
    };
    let saved = project
        .save_project_composer(SaveProjectComposer {
            access: access.clone(),
            operation_id: format!("{prefix}-save-composer"),
            conversation_id: conversation.id.clone(),
            expected_version: conversation.composer.version,
            body: composer.clone(),
        })
        .expect("save grouped composer");
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
        .expect("start grouped chat");
    project
        .begin_discussion_run(DiscussionBegin {
            owner: started.run.owner.clone(),
        })
        .expect("begin grouped chat");
    project
        .mark_discussion_delivered(started.run.owner.clone())
        .expect("deliver grouped chat");
    let output = serde_json::to_string(&json!({
        "schemaVersion": "project-assistant-output.v1",
        "answer": "I prepared two related records for review.",
        "questions": [],
        "assumptions": [],
        "drafts": [
            {"key":"hero-draft","title":"The River Keeper","kind":"character",
             "changeSummary":"Adds the keeper of the river gate.",
             "blocks":[{"type":"paragraph","content":[{"type":"text","text":"The keeper knows every crossing."}]}]},
            {"key":"gate-world","title":"River Gate","kind":"world",
             "changeSummary":"Adds the gate where the story begins.",
             "blocks":[{"type":"paragraph","content":[{"type":"text","text":"The gate opens at first light."}]}]}
        ],
        "groupEffects": {
            "relationships": [{
                "key":"keeper-guards-gate", "fromRef":"hero-draft", "toRef":"gate-world",
                "type":"guards", "description":"The keeper is responsible for the river gate.",
                "uncertainty":"The exact reason for the duty is still open."
            }],
            "impacts": [], "supersessions": [], "placements": []
        }
    }))
    .expect("serialize grouped response");
    project
        .finish_discussion(DiscussionFinish {
            owner: started.run.owner.clone(),
            expected_sequence: "0".into(),
            event_id: format!("{prefix}-finish"),
            assistant_text: output,
        })
        .expect("finish grouped chat");
    project
        .materialize_chat_result(started.run.owner)
        .expect("materialize grouped chat")
        .expect("grouped materialization event");

    let conversation_after = project
        .read_project_conversation(ReadProjectConversation {
            access: access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read grouped drafts");
    assert_eq!(conversation_after.drafts.len(), 2);
    let draft_refs = conversation_after
        .drafts
        .into_iter()
        .map(|draft| ProjectChatDraftRef {
            head: draft.document.head,
            disposition_version: draft.disposition_version,
        })
        .collect();
    let preview = project
        .prepare_chat_adoption(PrepareChatAdoption {
            access: access.clone(),
            operation_id: format!("{prefix}-prepare"),
            conversation_id: conversation.id.clone(),
            drafts: draft_refs,
            group_effects: None,
        })
        .expect("prepare grouped adoption");
    assert_eq!(
        preview
            .effects
            .as_ref()
            .unwrap()
            .proposed_relationships
            .len(),
        1
    );
    let ack = project
        .adopt_chat_preview(AdoptChatPreview {
            access: access.clone(),
            operation_id: format!("{prefix}-adopt"),
            conversation_id: conversation.id.clone(),
            preview_id: preview.id.clone(),
            preview_version: preview.version.clone(),
            preview_digest: preview.digest.clone(),
        })
        .expect("adopt grouped preview");
    (conversation.id, preview, ack)
}

fn materialize_and_adopt(
    project: &ProjectSession,
    access: &webnovel_core::projects::ProjectAccess,
) {
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
            group_effects: None,
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
    let access = project
        .attach("mixed-chat-transfer".into())
        .expect("attach");
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
    let recovered_access = recovered
        .attach("mixed-recovered-session".into())
        .expect("attach recovered");
    let view = recovered
        .read_project_conversation(ReadProjectConversation {
            access: recovered_access,
            before: None,
            limit: 40,
        })
        .expect("read recovered current conversation");
    assert!(
        view.items.is_empty(),
        "recovery starts with a fresh current conversation"
    );
}

#[test]
fn backup_rejects_a_chapter_request_forged_for_another_target() {
    let temp = Temp::new();
    let project =
        ProjectSession::create(temp.0.join("cross-target"), "Cross target").expect("create");
    let access = project
        .attach("cross-target-session".into())
        .expect("attach");
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

#[test]
fn grouped_effects_backup_recovery_preserves_historical_material_and_blocks_old_adoption() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("grouped-source"), "Grouped transfer")
        .expect("create grouped project");
    let access = project
        .attach("grouped-source-session".into())
        .expect("attach source");
    let (conversation_id, preview, ack) =
        materialize_grouped_and_adopt(&project, &access, "grouped-transfer");
    let source_ref = historical_ref(&project, conversation_id.clone());
    let source_history = read_history(&project, &access, source_ref.clone())
        .expect("read source historical conversation");
    let mut source_documents = {
        let mut documents = project
            .documents(access.clone())
            .expect("read source documents")
            .into_iter()
            .filter(|document| {
                document.role != webnovel_core::projects::DocumentRole::ConversationAnchor
            })
            .collect::<Vec<_>>();
        documents.sort_by(|left, right| left.head.document_id.cmp(&right.head.document_id));
        documents
    };
    let source_relationships = project
        .read_workshop(access.clone())
        .expect("read source workshop")
        .state
        .relationships;
    let endpoint = source_documents
        .first()
        .expect("adopted relationship endpoint")
        .clone();
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "grouped-transfer-edit-endpoint".into(),
            expected: endpoint.head,
            local_generation: "1".into(),
            body: body("The endpoint changed after the relationship was adopted."),
            cause: SaveCause::Typing,
        })
        .expect("edit relationship endpoint after adoption");
    source_documents = project
        .documents(access.clone())
        .expect("read edited source documents")
        .into_iter()
        .filter(|document| {
            document.role != webnovel_core::projects::DocumentRole::ConversationAnchor
        })
        .collect::<Vec<_>>();
    source_documents.sort_by(|left, right| left.head.document_id.cmp(&right.head.document_id));
    let historical_workshop = project
        .workshop_history(access.clone())
        .expect("read historical workshop after endpoint edit");
    assert_eq!(historical_workshop.len(), 1);
    assert_eq!(
        source_documents.len(),
        2,
        "two adopted ordinary records remain"
    );
    assert_eq!(source_relationships.len(), 1);
    assert_eq!(source_relationships[0].source_heads.len(), 2);
    assert_eq!(ack.documents.len(), 2);

    let backup = temp.0.join("grouped.wnsbackup");
    create_backup(&project, &backup).expect("create grouped backup");
    let recovered = recover_backup(
        &backup,
        &temp.0.join("grouped-recovered"),
        "Recovered grouped transfer",
    )
    .expect("recover grouped backup");
    let recovered_access = recovered
        .attach("grouped-recovered-session".into())
        .expect("attach recovered project");
    let current = recovered
        .read_project_conversation(ReadProjectConversation {
            access: recovered_access.clone(),
            before: None,
            limit: 40,
        })
        .expect("read recovered current conversation");
    assert!(
        current.items.is_empty(),
        "recovery starts a fresh conversation"
    );
    assert_ne!(recovered.info.project_id, source_ref.project_id);
    assert_ne!(
        recovered.info.operation_namespace,
        source_ref.operation_namespace
    );

    let historical = read_history(&recovered, &recovered_access, source_ref.clone())
        .expect("read recovered historical conversation");
    for kind in [
        "materializeChatResult",
        "adoptionPreview",
        "adoptionDecision",
    ] {
        let source_item = source_history
            .items
            .iter()
            .find(|item| item.item.kind == kind)
            .unwrap_or_else(|| panic!("source history has {kind}"));
        let recovered_item = historical
            .items
            .iter()
            .find(|item| item.item.kind == kind)
            .unwrap_or_else(|| panic!("recovered history has {kind}"));
        assert_eq!(
            recovered_item.item.payload, source_item.item.payload,
            "retained {kind}"
        );
    }
    let source_drafts = source_history
        .items
        .iter()
        .flat_map(|item| item.draft_revisions.iter())
        .map(|draft| {
            json!({
                "documentId": draft.document_id,
                "initial": draft.initial,
                "revision": serde_json::to_value(&draft.revision).expect("serialize source revision")
            })
        })
        .collect::<Vec<_>>();
    let recovered_drafts = historical
        .items
        .iter()
        .flat_map(|item| item.draft_revisions.iter())
        .map(|draft| {
            json!({
                "documentId": draft.document_id,
                "initial": draft.initial,
                "revision": serde_json::to_value(&draft.revision).expect("serialize recovered revision")
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        recovered_drafts, source_drafts,
        "retained assistant draft revisions"
    );

    let recovered_documents = {
        let mut documents = recovered
            .documents(recovered_access.clone())
            .expect("read recovered documents")
            .into_iter()
            .filter(|document| {
                document.role != webnovel_core::projects::DocumentRole::ConversationAnchor
            })
            .collect::<Vec<_>>();
        documents.sort_by(|left, right| left.head.document_id.cmp(&right.head.document_id));
        documents
    };
    assert_eq!(
        serde_json::to_value(recovered_documents).expect("serialize recovered documents"),
        serde_json::to_value(source_documents).expect("serialize source documents")
    );
    let recovered_relationships = recovered
        .read_workshop(recovered_access.clone())
        .expect("read recovered workshop")
        .state
        .relationships;
    assert_eq!(recovered_relationships, source_relationships);
    assert_eq!(
        recovered_relationships[0].source_heads,
        source_relationships[0].source_heads
    );

    let source_db = Connection::open(project.path.join("project.sqlite3")).expect("open source db");
    let source_receipt: String = source_db
        .query_row(
            "SELECT result_json FROM command_receipts WHERE operation_namespace=? AND operation_id=?",
            rusqlite::params![source_ref.operation_namespace, "grouped-transfer-adopt"],
            |row| row.get(0),
        )
        .expect("read source adoption receipt");
    drop(source_db);
    let db = Connection::open(recovered.path.join("project.sqlite3")).expect("open recovered db");
    let recovered_receipt: String = db
        .query_row(
            "SELECT result_json FROM command_receipts WHERE operation_namespace=? AND operation_id=?",
            rusqlite::params![source_ref.operation_namespace, "grouped-transfer-adopt"],
            |row| row.get(0),
        )
        .expect("read retained adoption receipt");
    assert_eq!(source_receipt, recovered_receipt,);
    drop(db);

    let error = recovered
        .adopt_chat_preview(AdoptChatPreview {
            access: recovered_access,
            operation_id: "recovered-old-preview".into(),
            conversation_id,
            preview_id: preview.id,
            preview_version: preview.version,
            preview_digest: preview.digest,
        })
        .expect_err("old preview must not adopt under recovered identity");
    assert_eq!(error.code, "WrongProjectSession");
}

#[test]
fn legacy_grouped_snapshot_tuple_hash_remains_recoverable_without_new_marker() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("legacy-grouped"), "Legacy grouped")
        .expect("create legacy grouped project");
    let access = project
        .attach("legacy-grouped-session".into())
        .expect("attach source");
    let (conversation_id, preview, _) =
        materialize_grouped_and_adopt(&project, &access, "legacy-grouped");
    let request = AdoptChatPreview {
        access: access.clone(),
        operation_id: "legacy-grouped-adopt".into(),
        conversation_id: conversation_id.clone(),
        preview_id: preview.id.clone(),
        preview_version: preview.version.clone(),
        preview_digest: preview.digest.clone(),
    };
    let backup = temp.0.join("legacy-grouped-current.wnsbackup");
    create_backup(&project, &backup).expect("create current grouped backup");
    let source_snapshot: (i64, String, String, String) = {
        let db = Connection::open(project.path.join("project.sqlite3")).expect("open source db");
        db.query_row(
            "SELECT version,payload_hash,state_json,state_hash FROM workshop_snapshots WHERE operation_namespace=? AND operation_id=?",
            rusqlite::params![access.operation_namespace, request.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read source grouped snapshot")
    };
    let legacy_backup = temp.0.join("legacy-grouped.wnsbackup");
    let old_payload_hash = reseal_as_legacy_grouped_snapshot(
        &backup,
        &legacy_backup,
        &request,
        &preview,
    );
    assert_ne!(source_snapshot.1, old_payload_hash);
    let recovered = recover_backup(
        &legacy_backup,
        &temp.0.join("legacy-grouped-recovered"),
        "Recovered legacy grouped",
    )
    .expect("recover legacy grouped backup");
    let recovered_access = recovered
        .attach("legacy-grouped-recovered-session".into())
        .expect("attach recovered legacy project");
    assert_eq!(
        recovered
            .workshop_history(recovered_access.clone())
            .expect("read recovered legacy workshop history")
            .len(),
        1
    );
    let recovered_db = Connection::open(recovered.path.join("project.sqlite3"))
        .expect("open recovered legacy db");
    let recovered_snapshot: (i64, String, String, String) = recovered_db
        .query_row(
            "SELECT version,payload_hash,state_json,state_hash FROM workshop_snapshots WHERE operation_namespace=? AND operation_id=?",
            rusqlite::params![access.operation_namespace, request.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read recovered legacy snapshot");
    assert_eq!(recovered_snapshot.0, source_snapshot.0);
    assert_eq!(recovered_snapshot.1, old_payload_hash);
    assert_eq!(recovered_snapshot.2, source_snapshot.2);
    assert_eq!(recovered_snapshot.3, source_snapshot.3);
    let decision_payload: String = recovered_db
        .query_row(
            "SELECT payload_json FROM conversation_items WHERE kind='adoptionDecision' AND operation_id=?",
            [request.operation_id.as_str()],
            |row| row.get(0),
        )
        .expect("read recovered legacy decision");
    let decision: Value = serde_json::from_str(&decision_payload).expect("parse legacy decision");
    assert!(decision.get("snapshotPayloadHash").is_none());
}

#[test]
fn new_grouped_snapshot_binding_tampering_blocks_recovery() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("binding-grouped"), "Binding grouped")
        .expect("create binding grouped project");
    let access = project
        .attach("binding-grouped-session".into())
        .expect("attach source");
    let (conversation_id, preview, _) =
        materialize_grouped_and_adopt(&project, &access, "binding-grouped");
    let backup = temp.0.join("binding-grouped.wnsbackup");
    create_backup(&project, &backup).expect("create binding grouped backup");
    let operation_id = "binding-grouped-adopt";
    let _ = conversation_id;
    let _ = preview;
    for (suffix, tamper_command_receipt) in [("snapshot", false), ("command", true)] {
        let tampered = temp.0.join(format!("binding-{suffix}-tampered.wnsbackup"));
        reseal_with_new_snapshot_binding_tamper(
            &backup,
            &tampered,
            &access.operation_namespace,
            operation_id,
            tamper_command_receipt,
        );
        let result = recover_backup(
            &tampered,
            &temp.0.join(format!("binding-{suffix}-recovered")),
            &format!("Tampered {suffix} binding"),
        );
        let error = match result {
            Ok(_) => panic!("new grouped binding tampering must block recovery"),
            Err(error) => error,
        };
        assert_eq!(error.code, "InvalidBackup", "tamper mode: {suffix}");
    }
}

#[test]
fn copied_assistant_draft_identity_is_rejected_by_preview_provenance() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("copied-draft-source"), "Copied draft source")
        .expect("create copied-draft source project");
    let access = project
        .attach("copied-draft-source-session".into())
        .expect("attach source");
    let (_source_conversation_id, source_preview, _) =
        materialize_grouped_and_adopt(&project, &access, "copied-draft-source");
    let source_backup = temp.0.join("copied-draft-source.wnsbackup");
    create_backup(&project, &source_backup).expect("create source backup");
    let recovered = recover_backup(
        &source_backup,
        &temp.0.join("copied-draft-recovered"),
        "Recover copied-draft source",
    )
    .expect("recover copied-draft source");
    let recovered_access = recovered
        .attach("copied-draft-recovered-session".into())
        .expect("attach recovered current identity");
    let (_, current_preview, _) = materialize_grouped_and_adopt(
        &recovered,
        &recovered_access,
        "copied-draft-current",
    );
    let current_backup = temp.0.join("copied-draft-current.wnsbackup");
    create_backup(&recovered, &current_backup).expect("create current copied-draft backup");
    let tampered_backup = temp.0.join("copied-draft-tampered.wnsbackup");
    reseal_with_copied_draft_reference(
        &current_backup,
        &tampered_backup,
        &current_preview.id,
        &source_preview.id,
    );
    let result = recover_backup(
        &tampered_backup,
        &temp.0.join("copied-draft-tampered-recovered"),
        "Reject copied assistant draft",
    );
    let error = match result {
        Ok(_) => panic!("copied assistant draft identity must block recovery"),
        Err(error) => error,
    };
    assert_eq!(error.code, "InvalidBackup");
    assert!(
        error.detail.contains("assistant draft")
            || error.detail.contains("conversation identity"),
        "unexpected copied-draft refusal: {}",
        error.detail
    );
}

#[test]
fn resealed_grouped_effect_manifest_with_wrong_role_is_rejected_on_recovery() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("grouped-tamper"), "Grouped tamper")
        .expect("create grouped tamper project");
    let access = project
        .attach("grouped-tamper-session".into())
        .expect("attach source");
    let replacement = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "grouped-tamper-note".into(),
            document_id: "unrelated-note".into(),
            title: "Unrelated note".into(),
            kind: "note".into(),
            body: body("This is not a relationship endpoint."),
        })
        .expect("create wrong-kind replacement");
    let (_, preview, _) = materialize_grouped_and_adopt(&project, &access, "grouped-tamper");
    let backup = temp.0.join("grouped-valid.wnsbackup");
    create_backup(&project, &backup).expect("create valid grouped backup");
    let tampered = temp.0.join("grouped-tampered.wnsbackup");
    reseal_with_tampered_relationship_endpoint(&backup, &tampered, &preview, &replacement);

    let result = recover_backup(
        &tampered,
        &temp.0.join("grouped-tampered-recovered"),
        "Tampered grouped recovery",
    );
    let error = match result {
        Ok(_) => panic!("tampered relationship endpoint must block recovery"),
        Err(error) => error,
    };
    assert_eq!(error.code, "InvalidBackup");
    assert!(
        error
            .detail
            .contains("outside the frozen ordinary material")
            || error.detail.contains("digest"),
        "unexpected tamper refusal: {}",
        error.detail
    );
}

#[test]
fn backup_rejects_recomputed_current_workshop_state_that_drifts_from_snapshot() {
    let temp = Temp::new();
    let project = ProjectSession::create(
        temp.0.join("grouped-state-tamper"),
        "Grouped state tamper",
    )
    .expect("create grouped state tamper project");
    let access = project
        .attach("grouped-state-tamper-session".into())
        .expect("attach source");
    let _ = materialize_grouped_and_adopt(&project, &access, "grouped-state-tamper");

    let db_path = project.path.join("project.sqlite3");
    let db = Connection::open(&db_path).expect("open project db");
    let state_json: String = db
        .query_row(
            "SELECT state_json FROM workshop_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .expect("read current workshop state");
    let mut state: Value = serde_json::from_str(&state_json).expect("parse workshop state");
    state["relationships"][0]["description"] = json!("tampered after adoption");
    let tampered_json = serde_json::to_string(&canonicalize(state)).expect("canonical state");
    db.execute(
        "UPDATE workshop_state SET state_json=?,state_hash=? WHERE singleton=1",
        rusqlite::params![tampered_json, sha256(tampered_json.as_bytes())],
    )
    .expect("tamper current workshop state");
    drop(db);

    let error = create_backup(&project, &temp.0.join("grouped-state-tamper.wnsbackup"))
        .expect_err("recomputed current state drift must block backup");
    assert_eq!(error.code, "InvalidBackup");
    assert!(
        error.detail.contains("current Workshop state")
            || error.detail.contains("immutable snapshot"),
        "unexpected state drift refusal: {}",
        error.detail
    );
}
