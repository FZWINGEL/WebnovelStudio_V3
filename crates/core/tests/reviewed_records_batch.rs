use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::story_context::FreezeStory;
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionRecord, PossessionTiming, StoryEntityRef,
};
use webnovel_core::projects::{
    CreateDocument, DocumentRecord, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-reviewed-batch-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn child(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let Ok(path) = self.0.canonicalize() else {
            return;
        };
        let Ok(temp) = std::env::temp_dir().canonicalize() else {
            return;
        };
        if path.parent() == Some(temp.as_path())
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("wns-reviewed-batch-"))
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": [{
            "type": "paragraph", "attrs": {"id": "p"},
            "content": [{"type": "text", "text": text}]
        }]}
    })
}

fn hash_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn record(id: &str, text: &str) -> PossessionRecord {
    PossessionRecord {
        id: id.into(),
        object: StoryEntityRef {
            id: format!("object-{id}"),
            label: format!("Object {id}"),
        },
        holder: None,
        timing: PossessionTiming::AtPassage,
        audience: EvidenceAudience::AuthorRoom,
        evidence: EvidenceAnchor {
            block_id: "p".into(),
            from_utf16: 0,
            to_utf16: text.encode_utf16().count() as u32,
            quote: text.into(),
            quote_hash: hash_hex(text),
        },
    }
}

fn chapter(
    project: &ProjectSession,
    access: &ProjectAccess,
    id: &str,
    text: &str,
) -> DocumentRecord {
    project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: format!("create-{id}"),
            document_id: id.into(),
            title: id.into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .unwrap()
}

fn mark_ready(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &DocumentRecord,
    operation_id: &str,
    text: &str,
) {
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: format!("stage-{operation_id}"),
            expected: document.head.clone(),
            records: Some(vec![record(operation_id, text)]),
            promises: None,
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: format!("ready-{operation_id}"),
            stage_id: stage.id,
        })
        .unwrap();
}

fn freeze(
    project: &ProjectSession,
    access: &ProjectAccess,
    target: &DocumentRecord,
    operation_id: &str,
) -> webnovel_core::projects::CoreResult<webnovel_core::projects::story_context::FrozenContext> {
    project.freeze_story(FreezeStory {
        access: access.clone(),
        operation_id: operation_id.into(),
        expected: target.head.clone(),
        basis: BasisKind::Working,
        purpose: ContextPurpose::StoryQuestion,
        policy: InformationPolicy {
            version: project.context_epochs(access.clone()).unwrap().policy,
            audience: Audience::AuthorRoom,
            reader_frontier: None,
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        },
    })
}

#[test]
fn working_freeze_batches_current_sets_and_omits_unreviewed_gap() {
    let (temp, project, access) = {
        let temp = TempProject::new();
        let project = ProjectSession::create(temp.child("story"), "Batch review").unwrap();
        let access = project.attach("batch-test".into()).unwrap();
        (temp, project, access)
    };
    let first = chapter(&project, &access, "chapter-1", "First evidence.");
    let second = chapter(&project, &access, "chapter-2", "Second evidence.");
    let target = chapter(&project, &access, "chapter-3", "Unreviewed gap.");
    mark_ready(&project, &access, &first, "first", "First evidence.");
    mark_ready(&project, &access, &second, "second", "Second evidence.");

    let frozen = freeze(&project, &access, &target, "freeze-batch").unwrap();
    assert_eq!(
        frozen
            .reviewed_evidence
            .iter()
            .map(|set| set.records[0].id.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    assert_eq!(frozen.reviewed_evidence.len(), 2);
    drop(project);
    drop(temp);
}

#[test]
fn working_freeze_drops_current_records_when_earlier_prose_is_stale() {
    let (temp, project, access) = {
        let temp = TempProject::new();
        let project = ProjectSession::create(temp.child("story"), "Batch stale").unwrap();
        let access = project.attach("batch-stale".into()).unwrap();
        (temp, project, access)
    };
    let first = chapter(&project, &access, "chapter-1", "First evidence.");
    let target = chapter(&project, &access, "chapter-2", "Second evidence.");
    mark_ready(&project, &access, &first, "first", "First evidence.");
    mark_ready(&project, &access, &target, "second", "Second evidence.");
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-first".into(),
            expected: first.head.clone(),
            local_generation: "1".into(),
            body: body("Changed first evidence."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let current_target = project
        .document(access.clone(), target.head.document_id.clone())
        .unwrap();
    let frozen = freeze(&project, &access, &current_target, "freeze-stale").unwrap();
    assert!(frozen.reviewed_evidence.is_empty());
    drop(project);
    drop(temp);
}

#[test]
fn working_freeze_propagates_malformed_selected_bundle_history() {
    let (temp, project, access) = {
        let temp = TempProject::new();
        let project = ProjectSession::create(temp.child("story"), "Batch malformed").unwrap();
        let access = project.attach("batch-malformed".into()).unwrap();
        (temp, project, access)
    };
    let first = chapter(&project, &access, "chapter-1", "First evidence.");
    mark_ready(&project, &access, &first, "first", "First evidence.");

    let connection = Connection::open(temp.child("story").join("project.sqlite3")).unwrap();
    connection
        .execute("DROP TRIGGER ready_bundles_no_update", [])
        .unwrap();
    connection
        .execute(
            "UPDATE ready_bundles SET prefix_json='not-json' WHERE document_id='chapter-1'",
            [],
        )
        .unwrap();
    drop(connection);

    let error = freeze(&project, &access, &first, "freeze-malformed").unwrap_err();
    assert_eq!(error.code, "InvalidProject");
    drop(project);
    drop(temp);
}

#[test]
fn working_freeze_respects_same_position_document_id_order() {
    let (temp, project, access) = {
        let temp = TempProject::new();
        let project = ProjectSession::create(temp.child("story"), "Batch order").unwrap();
        let access = project.attach("batch-order".into()).unwrap();
        (temp, project, access)
    };
    let later_id = chapter(&project, &access, "z", "Z evidence.");
    let earlier_id = chapter(&project, &access, "a", "A evidence.");
    mark_ready(&project, &access, &later_id, "z", "Z evidence.");
    mark_ready(&project, &access, &earlier_id, "a", "A evidence.");
    drop(project);

    let database = temp.child("story").join("project.sqlite3");
    let connection = Connection::open(database).unwrap();
    connection
        .execute("UPDATE documents SET position=0 WHERE id IN ('z','a')", [])
        .unwrap();
    drop(connection);

    let reopened = ProjectSession::open(temp.child("story")).unwrap();
    let reopened_access = reopened.attach("batch-order-reopened".into()).unwrap();
    let target = reopened
        .document(reopened_access.clone(), "z".into())
        .unwrap();
    let frozen = freeze(&reopened, &reopened_access, &target, "freeze-tied").unwrap();
    assert!(frozen.reviewed_evidence.is_empty());
    drop(reopened);
    drop(temp);
}
