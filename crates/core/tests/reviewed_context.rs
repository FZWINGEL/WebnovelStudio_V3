use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy, SourceKind};
use webnovel_core::documents::{ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::DocumentRecord;
use webnovel_core::projects::context_packets::{PreparationResult, PrepareContext};
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::story_context::{
    FreezeReviewedContinuation, FreezeStory, FrozenContext,
};
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionRecord, PossessionTiming, StoryEntityRef,
};
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

#[path = "support/schema.rs"]
mod legacy_schema;

struct Cleanup(PathBuf);

impl Drop for Cleanup {
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

fn hash_json(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn archive_entries(path: &PathBuf) -> (Vec<u8>, Vec<u8>) {
    let file = File::open(path).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let mut manifest = Vec::new();
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_end(&mut manifest)
        .unwrap();
    let mut database = Vec::new();
    archive
        .by_name("project.sqlite3")
        .unwrap()
        .read_to_end(&mut database)
        .unwrap();
    (manifest, database)
}

fn write_archive(path: &PathBuf, manifest: &[u8], database: &[u8]) {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    archive.start_file("manifest.json", options).unwrap();
    archive.write_all(manifest).unwrap();
    archive.start_file("project.sqlite3", options).unwrap();
    archive.write_all(database).unwrap();
    archive.finish().unwrap();
}

fn setup() -> (Cleanup, ProjectSession, ProjectAccess) {
    let root = std::env::temp_dir().join(format!("wns-reviewed-context-{}", Uuid::new_v4()));
    fs::create_dir(&root).expect("create test root");
    let project = ProjectSession::create(root.join("story"), "Reviewed context test").unwrap();
    let access = project.attach("reviewed-context".into()).unwrap();
    (Cleanup(root), project, access)
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
    prefix: &str,
) {
    let stage = project
        .stage_author_review(StageAuthorReview {
            knowledge: None,
            access: access.clone(),
            operation_id: format!("stage-{prefix}-{}", document.head.document_id),
            expected: document.head.clone(),
            records: None,
            promises: None,
            summary: None,
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: format!("ready-{prefix}-{}", document.head.document_id),
            stage_id: stage.id,
        })
        .unwrap();
}

fn policy(
    project: &ProjectSession,
    access: &ProjectAccess,
    target_position: &str,
) -> InformationPolicy {
    InformationPolicy {
        version: project.context_epochs(access.clone()).unwrap().policy,
        audience: Audience::RestrictedWriting,
        reader_frontier: Some(target_position.into()),
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    }
}

fn request(
    project: &ProjectSession,
    access: &ProjectAccess,
    operation_id: &str,
    target: &DocumentRecord,
    target_position: &str,
) -> FreezeReviewedContinuation {
    FreezeReviewedContinuation {
        access: access.clone(),
        operation_id: operation_id.into(),
        expected: target.head.clone(),
        policy: policy(project, access, target_position),
    }
}

fn review_prefix(
    project: &ProjectSession,
    access: &ProjectAccess,
    target: &DocumentRecord,
) -> FrozenContext {
    project
        .freeze_reviewed_continuation(request(project, access, "reviewed-freeze", target, "2"))
        .unwrap()
}

#[test]
fn reviewed_continuation_freezes_exact_prefix_and_replays_immutable_basis() {
    let (_cleanup, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "The first promise.");
    let second = chapter(&project, &access, "chapter-2", "The second promise.");
    let target = chapter(&project, &access, "chapter-3", "The current draft.");
    mark_ready(&project, &access, &first, "first");
    mark_ready(&project, &access, &second, "second");

    let freeze_request = request(&project, &access, "reviewed-freeze", &target, "2");
    let frozen = project
        .freeze_reviewed_continuation(freeze_request.clone())
        .unwrap();
    assert_eq!(frozen.snapshot.basis, BasisKind::Reviewed);
    assert_eq!(frozen.purpose, ContextPurpose::Continue);
    assert_eq!(
        frozen
            .snapshot
            .reviewed_basis
            .as_ref()
            .unwrap()
            .prefix
            .len(),
        2
    );
    assert_eq!(frozen.snapshot.sources[0].kind, SourceKind::CurrentDraft);
    assert!(frozen.snapshot.sources[0].dependencies.is_empty());
    assert!(
        frozen
            .snapshot
            .sources
            .iter()
            .skip(1)
            .all(|source| source.kind == SourceKind::ReviewedAuthority)
    );

    let replay = project
        .freeze_reviewed_continuation(freeze_request.clone())
        .unwrap();
    assert_eq!(replay.snapshot.snapshot_id, frozen.snapshot.snapshot_id);

    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-unretrieved-earlier".into(),
            expected: first.head,
            local_generation: "1".into(),
            body: body("The first promise was changed elsewhere."),
            cause: SaveCause::Typing,
        })
        .unwrap();

    // The old operation still reads its immutable source revision. A new
    // operation must refuse to continue from the stale selected prefix.
    let replay_after_edit = project
        .freeze_reviewed_continuation(freeze_request)
        .unwrap();
    assert_eq!(
        replay_after_edit.snapshot.snapshot_id,
        frozen.snapshot.snapshot_id
    );
    let current_target = project
        .document(access.clone(), target.head.document_id.clone())
        .unwrap();
    let error = project
        .freeze_reviewed_continuation(request(
            &project,
            &access,
            "reviewed-freeze-after-edit",
            &current_target,
            "2",
        ))
        .unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "ReviewBasisUnavailable" | "BasisUnavailable"
    ));
}

#[test]
fn reviewed_prefix_is_optional_packet_material_when_target_has_no_dependencies() {
    let (_cleanup, project, access) = setup();
    let first = chapter(
        &project,
        &access,
        "chapter-1",
        &"Earlier reviewed prose. ".repeat(600),
    );
    let second = chapter(
        &project,
        &access,
        "chapter-2",
        &"More earlier reviewed prose. ".repeat(600),
    );
    let target = chapter(&project, &access, "chapter-3", "Current draft.");
    mark_ready(&project, &access, &first, "first");
    mark_ready(&project, &access, &second, "second");
    let frozen = review_prefix(&project, &access, &target);
    let target_handle = frozen.snapshot.sources[0].handle.clone();
    let target_read = project
        .read_story_source(
            access.clone(),
            frozen.snapshot.snapshot_id.clone(),
            target_handle.clone(),
        )
        .unwrap();
    let scope = capture_scope(
        &target_read.body,
        ScopeGrant {
            kind: ScopeKind::WholeDocument,
            start: None,
            end: None,
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .unwrap();
    let prepared = project
        .prepare_context(PrepareContext {
            lookup: None,
            access: access.clone(),
            operation_id: "reviewed-packet".into(),
            snapshot_id: frozen.snapshot.snapshot_id,
            instruction: "Continue this chapter while preserving the current draft.".into(),
            mandatory_handles: Vec::new(),
            transient_mandatory_handles: None,
            safe_brief: None,
            scope: Some(scope),
            budget: webnovel_core::context::packet::MockContextBudget::new("2500", "50", "50"),
            provider_binding: None,
            response_contract: None,
        })
        .unwrap();
    let packet = match prepared {
        PreparationResult::Prepared { packet, .. } => packet,
        PreparationResult::BudgetRejected { error } => panic!("target should fit: {error:?}"),
    };
    assert!(
        packet
            .receipt
            .source_handles
            .iter()
            .any(|handle| handle == &target_handle)
    );
    assert!(
        packet
            .receipt
            .omissions
            .iter()
            .any(|omission| omission.contains("optional source omitted by input budget"))
    );
}

#[test]
fn reviewed_evidence_freezes_from_marked_bundle_and_reaches_restricted_packet() {
    let (_cleanup, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "The first promise.");
    let target = chapter(&project, &access, "chapter-2", "The current draft.");
    let quote = "The first promise.";
    let stage = project
        .stage_author_review(StageAuthorReview {
            knowledge: None,
            access: access.clone(),
            operation_id: "stage-evidence".into(),
            expected: first.head.clone(),
            records: Some(vec![PossessionRecord {
                id: "record-first".into(),
                object: StoryEntityRef {
                    id: "promise".into(),
                    label: "First promise".into(),
                },
                holder: None,
                timing: PossessionTiming::AtPassage,
                audience: EvidenceAudience::Reader,
                evidence: EvidenceAnchor {
                    block_id: "p1".into(),
                    from_utf16: 0,
                    to_utf16: quote.encode_utf16().count() as u32,
                    quote: quote.into(),
                    quote_hash: hash_json(quote),
                },
            }]),
            promises: None,
            summary: None,
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-evidence".into(),
            stage_id: stage.id,
        })
        .unwrap();

    let frozen = project
        .freeze_reviewed_continuation(request(
            &project,
            &access,
            "reviewed-evidence-freeze",
            &target,
            "1",
        ))
        .unwrap();
    assert_eq!(frozen.reviewed_evidence.len(), 1);
    assert_eq!(frozen.reviewed_evidence[0].records.len(), 1);
    assert_eq!(frozen.reviewed_evidence[0].records[0].id, "record-first");

    let target_handle = frozen.snapshot.sources[0].handle.clone();
    let target_read = project
        .read_story_source(
            access.clone(),
            frozen.snapshot.snapshot_id.clone(),
            target_handle,
        )
        .unwrap();
    let scope = capture_scope(
        &target_read.body,
        ScopeGrant {
            kind: ScopeKind::WholeDocument,
            start: None,
            end: None,
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .unwrap();
    let prepared = project
        .prepare_context(PrepareContext {
            lookup: None,
            access,
            operation_id: "reviewed-evidence-packet".into(),
            snapshot_id: frozen.snapshot.snapshot_id,
            instruction: "Continue this chapter from the reviewed evidence.".into(),
            mandatory_handles: Vec::new(),
            transient_mandatory_handles: None,
            safe_brief: None,
            scope: Some(scope),
            budget: webnovel_core::context::packet::MockContextBudget::new("100000", "50", "50"),
            provider_binding: None,
            response_contract: None,
        })
        .unwrap();
    let packet = match prepared {
        PreparationResult::Prepared { packet, .. } => packet,
        PreparationResult::BudgetRejected { error } => panic!("target should fit: {error:?}"),
    };
    assert_eq!(packet.receipt.reviewed_evidence.len(), 1);
    assert_eq!(
        packet.receipt.reviewed_evidence[0].record_ids,
        vec!["record-first"]
    );
    assert!(packet.receipt.reviewed_evidence[0].complete_record_set);
}

#[test]
fn reviewed_continuation_requires_restricted_writing_at_the_target_frontier() {
    let (_cleanup, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "First.");
    let target = chapter(&project, &access, "chapter-2", "Target.");
    mark_ready(&project, &access, &first, "first");

    let mut author_room = request(&project, &access, "author-room", &target, "1");
    author_room.policy.audience = Audience::AuthorRoom;
    author_room.policy.reader_frontier = None;
    assert_eq!(
        project
            .freeze_reviewed_continuation(author_room)
            .unwrap_err()
            .code,
        "BoundaryConflict"
    );

    let mut future_frontier = request(&project, &access, "future-frontier", &target, "1");
    future_frontier.policy.reader_frontier = Some("99".into());
    assert_eq!(
        project
            .freeze_reviewed_continuation(future_frontier)
            .unwrap_err()
            .code,
        "BoundaryConflict"
    );
}

#[test]
fn reviewed_continuation_uses_document_id_to_resolve_tied_positions() {
    let (cleanup, project, access) = setup();
    let first = chapter(&project, &access, "a", "A.");
    let tied_prefix = chapter(&project, &access, "b", "B.");
    let target = chapter(&project, &access, "c", "C.");
    mark_ready(&project, &access, &first, "first");
    mark_ready(&project, &access, &tied_prefix, "tied");
    let path = cleanup.0.join("story");
    drop(project);
    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    connection
        .execute("UPDATE documents SET position=2 WHERE id='b'", [])
        .unwrap();
    drop(connection);
    let project = ProjectSession::open(path).unwrap();
    let access = project.attach("reviewed-context-tied".into()).unwrap();
    let target = project
        .document(access.clone(), target.head.document_id)
        .unwrap();
    let frozen = project
        .freeze_reviewed_continuation(request(&project, &access, "tied-position", &target, "2"))
        .unwrap();
    let manifest = frozen.snapshot.reviewed_basis.unwrap();
    assert_eq!(manifest.prefix.last().unwrap().document_id, "b");
}

#[test]
fn reviewed_reader_position_mutation_is_rejected_but_reordered_history_remains_readable() {
    let (cleanup, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "First.");
    let second = chapter(&project, &access, "chapter-2", "Second.");
    let target = chapter(&project, &access, "chapter-3", "Target.");
    mark_ready(&project, &access, &first, "first");
    mark_ready(&project, &access, &second, "second");
    let frozen = review_prefix(&project, &access, &target);
    let snapshot_id = frozen.snapshot.snapshot_id.clone();
    let path = cleanup.0.join("story");
    drop(project);

    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    connection
        .execute_batch("DROP TRIGGER immutable_story_snapshot_update")
        .unwrap();
    let json: String = connection
        .query_row(
            "SELECT manifest_json FROM story_snapshots WHERE id=?",
            [&snapshot_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut manifest: Value = serde_json::from_str(&json).unwrap();
    manifest["policy"]["readerFrontier"] = Value::String("99".into());
    manifest["snapshot"]["sources"][0]["disclosure"]["readerPosition"] = Value::String("99".into());
    let json = serde_json::to_string(&manifest).unwrap();
    connection
        .execute(
            "UPDATE story_snapshots SET manifest_json=?,manifest_hash=? WHERE id=?",
            rusqlite::params![json, hash_json(&json), snapshot_id],
        )
        .unwrap();
    drop(connection);

    let reopened = ProjectSession::open(path).unwrap();
    let reopened_access = reopened.attach("reader-position-check".into()).unwrap();
    let error = reopened
        .story_snapshot(reopened_access, snapshot_id)
        .unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "InvalidContext" | "ContextSourceDisallowed"
    ));

    let (cleanup, project, access) = setup();
    let first = chapter(&project, &access, "chapter-1", "First.");
    let second = chapter(&project, &access, "chapter-2", "Second.");
    let target = chapter(&project, &access, "chapter-3", "Target.");
    mark_ready(&project, &access, &first, "first");
    mark_ready(&project, &access, &second, "second");
    let frozen = review_prefix(&project, &access, &target);
    let snapshot_id = frozen.snapshot.snapshot_id.clone();
    let path = cleanup.0.join("story");
    drop(project);
    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    connection
        .execute("UPDATE documents SET position=20 WHERE id='chapter-1'", [])
        .unwrap();
    connection
        .execute("UPDATE documents SET position=21 WHERE id='chapter-2'", [])
        .unwrap();
    connection
        .execute("UPDATE documents SET position=22 WHERE id='chapter-3'", [])
        .unwrap();
    drop(connection);
    let reopened = ProjectSession::open(path).unwrap();
    let reopened_access = reopened.attach("historical-reorder-check".into()).unwrap();
    assert_eq!(
        reopened
            .story_snapshot(reopened_access, snapshot_id)
            .unwrap()
            .snapshot
            .basis,
        BasisKind::Reviewed
    );
}

#[test]
fn current_schema_missing_reader_position_column_fails_backup_validation() {
    let (cleanup, project, access) = setup();
    let document = chapter(&project, &access, "chapter-1", "Working draft.");
    project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: "working-snapshot".into(),
            expected: document.head,
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
        .unwrap();
    let path = cleanup.0.join("story");
    drop(project);
    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    connection
        .execute(
            "ALTER TABLE snapshot_sources DROP COLUMN reader_position",
            [],
        )
        .unwrap();
    drop(connection);
    let reopened = ProjectSession::open(path).unwrap();
    let backup = cleanup.0.join("missing-reader-position.wnsbackup");
    let error = create_backup(&reopened, &backup).unwrap_err();
    assert_eq!(error.code, "InvalidBackup");
}

#[test]
fn schema14_archived_working_snapshot_recovers_after_reader_pin_migration() {
    let (cleanup, project, access) = setup();
    let document = chapter(&project, &access, "chapter-1", "Working draft.");
    let frozen = project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: "archived-working-snapshot".into(),
            expected: document.head.clone(),
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
        .unwrap();
    let current_archive = cleanup.0.join("current.wnsbackup");
    create_backup(&project, &current_archive).unwrap();
    let (manifest, database) = archive_entries(&current_archive);
    let legacy_database_path = cleanup.0.join("schema14.sqlite3");
    fs::write(&legacy_database_path, database).unwrap();
    drop(project);

    let connection = Connection::open(&legacy_database_path).unwrap();
    legacy_schema::remove_schema24_features(&connection).unwrap();
    legacy_schema::remove_schema19_features(&connection).unwrap();
    connection
        .execute_batch(
            "DROP TABLE snapshot_navigation_views; DROP TABLE memory_view_sources; DROP TABLE memory_views; DROP TABLE memory_results; DROP TABLE memory_jobs; ALTER TABLE snapshot_sources DROP COLUMN reader_position",
        )
        .unwrap();
    connection.pragma_update(None, "user_version", 14).unwrap();
    drop(connection);
    let legacy_database = fs::read(&legacy_database_path).unwrap();
    let mut manifest_value: Value = serde_json::from_slice(&manifest).unwrap();
    manifest_value["databaseSha256"] = Value::String(
        Sha256::digest(&legacy_database)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    );
    let legacy_archive = cleanup.0.join("schema14.wnsbackup");
    write_archive(
        &legacy_archive,
        &serde_json::to_vec(&manifest_value).unwrap(),
        &legacy_database,
    );
    let archive_before_recovery = fs::read(&legacy_archive).unwrap();

    let recovered_path = cleanup.0.join("recovered");
    let recovered =
        recover_backup(&legacy_archive, &recovered_path, "Recovered working story").unwrap();
    let recovered_access = recovered.attach("schema14-reader".into()).unwrap();
    let restored = recovered
        .document(recovered_access.clone(), document.head.document_id)
        .unwrap();
    assert_eq!(restored.body, document.body);
    assert_eq!(
        recovered.context_epochs(recovered_access).unwrap().source,
        "1"
    );
    assert_eq!(fs::read(&legacy_archive).unwrap(), archive_before_recovery);
    let connection = Connection::open(recovered_path.join("project.sqlite3")).unwrap();
    let schema: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(schema, 33);
    let retained_json: String = connection
        .query_row(
            "SELECT manifest_json FROM story_snapshots WHERE id=?",
            [&frozen.snapshot.snapshot_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained_json, serde_json::to_string(&frozen).unwrap());
    let position: Option<i64> = connection
        .query_row(
            "SELECT reader_position FROM snapshot_sources WHERE snapshot_id=?",
            [&frozen.snapshot.snapshot_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(position, None);
}
