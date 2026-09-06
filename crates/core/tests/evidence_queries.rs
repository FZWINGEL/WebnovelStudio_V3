use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
use uuid::Uuid;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::story_context::{FreezeReviewedContinuation, FreezeStory};
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionRecord, PossessionTiming, StoryEntityRef,
};
use webnovel_core::projects::{CreateDocument, DocumentRecord, ProjectAccess, ProjectSession};
use webnovel_core::transfer::{create_backup, recover_backup};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-evidence-query-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let (Ok(path), Ok(root)) = (self.0.canonicalize(), std::env::temp_dir().canonicalize())
            && path.parent() == Some(root.as_path())
            && path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("wns-evidence-query-")
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}
fn body(text: &str) -> Value {
    json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p"},"content":[{"type":"text","text":text}]}]}})
}

fn hash_text(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
fn record(
    id: &str,
    object_id: &str,
    label: &str,
    text: &str,
    audience: EvidenceAudience,
) -> PossessionRecord {
    PossessionRecord {
        id: id.into(),
        object: StoryEntityRef {
            id: object_id.into(),
            label: label.into(),
        },
        holder: None,
        timing: PossessionTiming::Unknown,
        audience,
        evidence: EvidenceAnchor {
            block_id: "p".into(),
            from_utf16: 0,
            to_utf16: text.encode_utf16().count() as u32,
            quote: text.into(),
            quote_hash: Sha256::digest(text.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        },
    }
}
fn review(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &DocumentRecord,
    records: Vec<PossessionRecord>,
) {
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: Uuid::new_v4().to_string(),
            expected: document.head.clone(),
            records: Some(records),
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: Uuid::new_v4().to_string(),
            stage_id: stage.id,
        })
        .unwrap();
}
fn policy(
    project: &ProjectSession,
    access: &ProjectAccess,
    audience: Audience,
    frontier: Option<&str>,
) -> InformationPolicy {
    InformationPolicy {
        version: project.context_epochs(access.clone()).unwrap().policy,
        audience,
        reader_frontier: frontier.map(str::to_owned),
        character_id: None,
        character_grants: vec![],
        allow_alternatives: false,
        allow_historical: false,
    }
}
#[test]
fn catalog_reuses_ids_preserves_same_names_and_does_not_write() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("story"), "Story").unwrap();
    let access = project.attach("test".into()).unwrap();
    let first = chapter(&project, &access, "One", "Mei held the key.");
    review(
        &project,
        &access,
        &first,
        vec![record(
            "r1",
            "key-a",
            "Key",
            "Mei held the key.",
            EvidenceAudience::Reader,
        )],
    );
    let second = chapter(&project, &access, "Two", "Ren held two keys.");
    review(
        &project,
        &access,
        &second,
        vec![
            record(
                "r2",
                "key-a",
                "Silver key",
                "Ren held two keys.",
                EvidenceAudience::Reader,
            ),
            record(
                "r3",
                "key-b",
                "Key",
                "Ren held two keys.",
                EvidenceAudience::AuthorRoom,
            ),
        ],
    );
    let before = project.context_epochs(access.clone()).unwrap();
    let catalog = project.reviewed_entity_catalog(access.clone()).unwrap();
    assert_eq!(catalog.entities.len(), 2);
    assert_eq!(catalog.entities[0].entity.id, "key-a");
    assert_eq!(
        catalog.entities[0].label_variants,
        vec!["Key", "Silver key"]
    );
    assert_eq!(catalog.entities[0].first_document_title, "One");
    assert_eq!(catalog.entities[1].entity.id, "key-b");
    assert_eq!(catalog.entities[1].first_document_title, "Two");
    assert_eq!(
        project.context_epochs(access).unwrap().source,
        before.source
    );
}

#[test]
fn review_replacement_changes_catalog_while_history_keeps_exact_frozen_evidence() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("story"), "Story").unwrap();
    let access = project.attach("test".into()).unwrap();
    let first = chapter(&project, &access, "One", "Mei held the key.");
    review(
        &project,
        &access,
        &first,
        vec![record(
            "r1",
            "key",
            "Key",
            "Mei held the key.",
            EvidenceAudience::Reader,
        )],
    );
    let second = chapter(&project, &access, "Two", "Ren held the key.");
    review(
        &project,
        &access,
        &second,
        vec![record(
            "r2",
            "key",
            "Key",
            "Ren held the key.",
            EvidenceAudience::Reader,
        )],
    );
    let frozen = project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: "freeze".into(),
            expected: second.head.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: policy(&project, &access, Audience::AuthorRoom, None),
        })
        .unwrap();
    let before = project
        .reviewed_evidence_history(
            access.clone(),
            frozen.snapshot.snapshot_id.clone(),
            "key".into(),
        )
        .unwrap();
    assert!(before.current);
    assert_eq!(before.history.observations.len(), 2);
    review(&project, &access, &first, vec![]);
    assert!(
        project
            .reviewed_entity_catalog(access.clone())
            .unwrap()
            .entities
            .is_empty()
    );
    let history = project
        .reviewed_evidence_history(
            access.clone(),
            frozen.snapshot.snapshot_id.clone(),
            "key".into(),
        )
        .unwrap();
    assert!(!history.current);
    assert_eq!(
        serde_json::to_value(history.history).unwrap(),
        serde_json::to_value(before.history).unwrap()
    );
    let archive = temp.0.join("copy.wnsbackup");
    create_backup(&project, &archive).unwrap();
    let recovered = recover_backup(&archive, &temp.0.join("copy"), "Copy").unwrap();
    let recovered_access = recovered.attach("test".into()).unwrap();
    assert!(
        recovered
            .reviewed_entity_catalog(recovered_access.clone())
            .unwrap()
            .entities
            .is_empty()
    );
    assert!(
        recovered
            .reviewed_evidence_history(
                recovered_access,
                frozen.snapshot.snapshot_id.clone(),
                "key".into()
            )
            .is_err()
    );
    let epoch = project.context_epochs(access.clone()).unwrap().policy;
    project.revoke_story_context(access.clone(), epoch).unwrap();
    assert_eq!(
        project
            .reviewed_evidence_history(access, frozen.snapshot.snapshot_id, "key".into())
            .unwrap_err()
            .code,
        "ContextPolicyChanged"
    );
}

#[test]
fn historical_evidence_rejects_decodable_tampered_prefix_rows() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("story"), "Story").unwrap();
    let access = project.attach("tamper-test".into()).unwrap();
    let first = chapter(&project, &access, "One", "Mei held the key.");
    review(
        &project,
        &access,
        &first,
        vec![record(
            "r1",
            "key",
            "Key",
            "Mei held the key.",
            EvidenceAudience::Reader,
        )],
    );
    let second = chapter(&project, &access, "Two", "Ren held the key.");
    review(
        &project,
        &access,
        &second,
        vec![record(
            "r2",
            "key",
            "Key",
            "Ren held the key.",
            EvidenceAudience::Reader,
        )],
    );
    let target = chapter(&project, &access, "Three", "The key was hidden.");
    let frozen = project
        .freeze_reviewed_continuation(FreezeReviewedContinuation {
            access: access.clone(),
            operation_id: "freeze-tamper".into(),
            expected: target.head,
            policy: policy(&project, &access, Audience::RestrictedWriting, Some("2")),
        })
        .unwrap();
    let tampered_bundle_id = frozen
        .snapshot
        .reviewed_basis
        .as_ref()
        .expect("reviewed basis")
        .prefix[1]
        .bundle_id
        .clone();

    let database = temp.0.join("story").join("project.sqlite3");
    let connection = Connection::open(database).unwrap();
    connection
        .execute("DROP TRIGGER ready_bundles_no_update", [])
        .unwrap();
    let changed = connection
        .execute(
            "UPDATE ready_bundles SET prefix_json='[]', prefix_hash=? WHERE id=?",
            (hash_text("[]"), tampered_bundle_id),
        )
        .unwrap();
    assert_eq!(changed, 1);
    drop(connection);

    let error = project
        .reviewed_evidence_history(access, frozen.snapshot.snapshot_id, "key".into())
        .unwrap_err();
    assert_eq!(error.code, "InvalidContext");
    assert!(error.detail.contains("exact source"));
}

#[test]
fn restricted_history_filters_private_observations_before_labels_and_results() {
    let temp = Temp::new();
    let project = ProjectSession::create(temp.0.join("story"), "Story").unwrap();
    let access = project.attach("test".into()).unwrap();
    let first = chapter(&project, &access, "One", "Mei held the key.");
    review(
        &project,
        &access,
        &first,
        vec![
            record(
                "public",
                "key",
                "Key",
                "Mei held the key.",
                EvidenceAudience::Reader,
            ),
            record(
                "secret-record",
                "key",
                "Secret identity",
                "Mei held the key.",
                EvidenceAudience::AuthorRoom,
            ),
        ],
    );
    let second = chapter(&project, &access, "Two", "Ren waited.");
    let frozen = project
        .freeze_reviewed_continuation(FreezeReviewedContinuation {
            access: access.clone(),
            operation_id: "freeze".into(),
            expected: second.head,
            policy: policy(&project, &access, Audience::RestrictedWriting, Some("1")),
        })
        .unwrap();
    let history = project
        .reviewed_evidence_history(access, frozen.snapshot.snapshot_id, "key".into())
        .unwrap();
    assert_eq!(history.history.observations.len(), 1);
    let json = serde_json::to_string(&history).unwrap();
    assert!(!json.contains("Secret identity"));
    assert!(!json.contains("secret-record"));
}
