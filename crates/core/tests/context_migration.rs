use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use webnovel_core::context::memory::mock_navigation_digest;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::documents::Endpoint;
use webnovel_core::projects::context_packets::{PreparationResult, PrepareContext};
use webnovel_core::projects::discussions::ProviderOutcomeStatus;
use webnovel_core::projects::discussions::SaveDiscussionDraft;
use webnovel_core::projects::memory::{CompleteMemory, StartMemory};
use webnovel_core::projects::reviewed_story::{MarkReady, ReviewState, StageAuthorReview};
use webnovel_core::projects::story_context::FreezeStory;
use webnovel_core::projects::{
    CreateDocument, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{BackupManifest, create_backup, recover_backup};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

#[path = "support/schema.rs"]
mod legacy_schema;
use legacy_schema::remove_schema24_features;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("wns-context-migration-{label}-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create temporary test directory");
        Self(path)
    }

    fn child(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "attrs": {"id": "p1"},
                "content": [{"type": "text", "text": text}]
            }]
        }
    })
}

#[test]
fn schema24_upgrade_keeps_exact_historical_codex_packet_and_backup() {
    use webnovel_core::context::packet::{ProviderBinding, serialized_input};
    let temp = TempDir::new("codex-runtime-identity");
    let path = temp.child("project");
    let (project, access, _, saved) = setup_project(&path);
    let current = project
        .document(access.clone(), saved.head.document_id)
        .unwrap();
    let snapshot_id = freeze_one_snapshot(&project, &access, &current);
    let packet = match project
        .prepare_context(PrepareContext {
            lookup: None,
            access: access.clone(),
            operation_id: "historical-codex-packet".into(),
            snapshot_id,
            instruction: "Explain the saved passage.".into(),
            mandatory_handles: Vec::new(),
            transient_mandatory_handles: None,
            safe_brief: None,
            scope: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: Some(ProviderBinding::codex_luna_historical()),
            response_contract: None,
        })
        .unwrap()
    {
        PreparationResult::Prepared { packet, .. } => *packet,
        _ => panic!("historical packet fits"),
    };
    let original = serde_json::to_vec(&packet).unwrap();
    let input = serialized_input(&packet.messages, &packet.options).unwrap();
    assert!(!input.contains("\"runtime\""));
    drop(project);
    let database = Connection::open(path.join("project.sqlite3")).unwrap();
    database.pragma_update(None, "user_version", 24).unwrap();
    drop(database);
    let reopened = ProjectSession::open(&path).unwrap();
    assert_eq!(schema_version(&path.join("project.sqlite3")), 25);
    let access = reopened.attach("new-runtime-reader".into()).unwrap();
    let restored = reopened
        .prepared_context(access, packet.receipt.packet_id)
        .unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), original);
    assert_eq!(
        serialized_input(&restored.messages, &restored.options).unwrap(),
        input
    );
    let archive = temp.child("retained-history.wnsbackup");
    create_backup(&reopened, &archive).unwrap();
    let recovered = recover_backup(&archive, &temp.child("recovered"), "Historical Codex").unwrap();
    drop(recovered);
    let backups = fs::read_dir(path.join("migrations"))
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    assert_eq!(schema_version(&backups[0].as_ref().unwrap().path()), 24);
}

#[test]
fn schema14_reader_floor_upgrade_preserves_exact_reviews_and_working_snapshots() {
    let temp = TempDir::new("reviewed-reader-floor");
    let path = temp.child("project");
    let (project, access, document, saved) = setup_project(&path);
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "legacy-stage".into(),
            expected: saved.head.clone(),
            records: None,
            promises: None,
        })
        .unwrap();
    let bundle = project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "legacy-review".into(),
            stage_id: stage.id,
        })
        .unwrap();
    let frozen = project
        .freeze_story(FreezeStory {
            access,
            operation_id: "legacy-context".into(),
            expected: saved.head.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: InformationPolicy {
                version: "0".into(),
                audience: Audience::AuthorRoom,
                reader_frontier: None,
                character_id: None,
                character_grants: vec![],
                allow_alternatives: false,
                allow_historical: false,
            },
        })
        .unwrap();
    let original_json = serde_json::to_string(&frozen).unwrap();
    assert!(!original_json.contains("reviewedBasis"));
    drop(project);
    let db = Connection::open(path.join("project.sqlite3")).unwrap();
    remove_schema24_features(&db).unwrap();
    legacy_schema::remove_schema19_features(&db).unwrap();
    db.execute_batch("DROP TABLE snapshot_navigation_views; DROP TABLE memory_view_sources; DROP TABLE memory_views; DROP TABLE memory_results; DROP TABLE memory_jobs; ALTER TABLE snapshot_sources DROP COLUMN reader_position")
        .unwrap();
    db.pragma_update(None, "user_version", 14).unwrap();
    drop(db);

    let reopened = ProjectSession::open(&path).unwrap();
    let access = reopened.attach("new-reader".into()).unwrap();
    let restored = reopened
        .story_snapshot(access.clone(), frozen.snapshot.snapshot_id)
        .unwrap();
    assert_eq!(serde_json::to_string(&restored).unwrap(), original_json);
    let status = reopened
        .chapter_review_status(access.clone(), document.head.document_id.clone())
        .unwrap();
    assert_eq!(status.state, ReviewState::Ready);
    assert_eq!(status.active_bundle_id, Some(bundle.id));
    assert_eq!(
        reopened
            .document(access, document.head.document_id)
            .unwrap()
            .head,
        saved.head
    );
    assert_eq!(schema_version(&path.join("project.sqlite3")), 25);
    let backups: Vec<_> = fs::read_dir(path.join("migrations"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1);
    assert!(
        backups[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("schema14-before-schema25-")
    );
    assert_eq!(schema_version(&backups[0]), 14);
}

#[test]
fn schema16_upgrade_preserves_original_snapshot_and_packet_bytes() {
    let temp = TempDir::new("schema16-navigation");
    let path = temp.child("legacy");
    let (project, access, _, saved) = setup_project(&path);
    let current = project
        .document(access.clone(), saved.head.document_id.clone())
        .unwrap();
    let snapshot_id = freeze_one_snapshot(&project, &access, &current);
    let frozen = project
        .story_snapshot(access.clone(), snapshot_id.clone())
        .unwrap();
    assert!(frozen.navigation_views.is_empty());
    let frozen_bytes = serde_json::to_vec(&frozen).unwrap();
    assert!(!String::from_utf8_lossy(&frozen_bytes).contains("navigationViews"));
    let packet = match project
        .prepare_context(PrepareContext {
            lookup: None,
            access: access.clone(),
            operation_id: "schema16-packet".into(),
            snapshot_id: snapshot_id.clone(),
            instruction: "What promise was made?".into(),
            mandatory_handles: Vec::new(),
            transient_mandatory_handles: None,
            safe_brief: None,
            scope: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            response_contract: None,
        })
        .unwrap()
    {
        PreparationResult::Prepared { packet, .. } => *packet,
        _ => panic!("legacy packet must fit"),
    };
    let packet_bytes = serde_json::to_vec(&packet).unwrap();
    assert!(!String::from_utf8_lossy(&packet_bytes).contains("navigationViews"));
    assert!(!String::from_utf8_lossy(&packet_bytes).contains("navigationOmissions"));
    drop(project);
    let db = Connection::open(path.join("project.sqlite3")).unwrap();
    remove_schema24_features(&db).unwrap();
    legacy_schema::remove_schema19_features(&db).unwrap();
    let request_before: String = db
        .query_row(
            "SELECT request_json FROM context_packets WHERE id=?",
            [&packet.receipt.packet_id],
            |row| row.get(0),
        )
        .unwrap();
    db.execute_batch("DROP TABLE snapshot_navigation_views; PRAGMA user_version=16;")
        .unwrap();
    drop(db);

    let reopened = ProjectSession::open(&path).unwrap();
    let access = reopened.attach("schema19-reader".into()).unwrap();
    assert_eq!(schema_version(&path.join("project.sqlite3")), 25);
    assert_eq!(
        serde_json::to_vec(
            &reopened
                .story_snapshot(access.clone(), snapshot_id)
                .unwrap()
        )
        .unwrap(),
        frozen_bytes
    );
    assert_eq!(
        serde_json::to_vec(
            &reopened
                .prepared_context(access.clone(), packet.receipt.packet_id.clone())
                .unwrap()
        )
        .unwrap(),
        packet_bytes
    );
    assert_eq!(
        reopened
            .document(access, saved.head.document_id.clone())
            .unwrap()
            .head,
        saved.head
    );
    let db = Connection::open(path.join("project.sqlite3")).unwrap();
    let request_after: String = db
        .query_row(
            "SELECT request_json FROM context_packets WHERE id=?",
            [&packet.receipt.packet_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(request_before, request_after);
    let pins: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM snapshot_navigation_views",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pins, 0);
    let backups: Vec<_> = fs::read_dir(path.join("migrations"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(schema_version(&backups[0]), 16);
}

#[test]
fn schema17_reader_upgrade_preserves_generated_views_pins_and_packet_bytes() {
    let temp = TempDir::new("schema17-chapter-freshness");
    let path = temp.child("legacy");
    let (project, access, _, saved) = setup_project(&path);
    let target = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-discussion-target".into(),
            document_id: "later-chapter".into(),
            title: "Later chapter".into(),
            kind: "chapter".into(),
            body: body("The old promise still matters."),
        })
        .unwrap();
    let job = project
        .start_memory(StartMemory {
            access: access.clone(),
            operation_id: "legacy-memory".into(),
            expected: saved.head.clone(),
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
        })
        .unwrap();
    let dispatch = project.begin_memory(job.owner.clone()).unwrap();
    project
        .complete_memory(CompleteMemory {
            owner: job.owner.clone(),
            event_id: "legacy-memory-result".into(),
            raw_output: serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap())
                .unwrap(),
            outcome: ProviderOutcomeStatus::Completed,
            confirmed_stdin_bytes: None,
            usage: None,
            cleanup: None,
            error: None,
            effective_identity: None,
        })
        .unwrap();
    let view = project.install_memory(job.owner).unwrap();
    let snapshot_id = freeze_one_snapshot(&project, &access, &target);
    let frozen = project
        .story_snapshot(access.clone(), snapshot_id.clone())
        .unwrap();
    assert_eq!(frozen.navigation_views.len(), 1);
    let frozen_bytes = serde_json::to_vec(&frozen).unwrap();
    let packet = match project
        .prepare_context(PrepareContext {
            lookup: None,
            access: access.clone(),
            operation_id: "legacy-navigation-packet".into(),
            snapshot_id: snapshot_id.clone(),
            instruction: "What promise was made?".into(),
            mandatory_handles: Vec::new(),
            transient_mandatory_handles: None,
            safe_brief: None,
            scope: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            response_contract: None,
        })
        .unwrap()
    {
        PreparationResult::Prepared { packet, .. } => *packet,
        _ => panic!("legacy navigation packet must fit"),
    };
    let packet_bytes = serde_json::to_vec(&packet).unwrap();
    // Capture immutable persisted rows, not just their public projections.
    fn retained_rows(db: &Connection) -> Vec<String> {
        [
            "SELECT request_json FROM memory_jobs ORDER BY id",
            "SELECT candidate_json FROM memory_results ORDER BY job_id",
            "SELECT candidate_json || ':' || installed_current FROM memory_views ORDER BY id",
            "SELECT snapshot_id || ':' || view_id || ':' || content_hash FROM snapshot_navigation_views ORDER BY snapshot_id,view_id",
            "SELECT manifest_json FROM story_snapshots ORDER BY id",
            "SELECT request_json FROM context_packets ORDER BY id",
        ].iter().flat_map(|sql| {
            db.prepare(sql).unwrap().query_map([], |row| row.get::<_, String>(0))
                .unwrap().map(Result::unwrap).collect::<Vec<_>>()
        }).collect()
    }
    drop(project);
    let db = Connection::open(path.join("project.sqlite3")).unwrap();
    remove_schema24_features(&db).unwrap();
    legacy_schema::remove_schema19_features(&db).unwrap();
    let rows_before = retained_rows(&db);
    db.pragma_update(None, "user_version", 17).unwrap();
    drop(db);

    let reopened = ProjectSession::open(&path).unwrap();
    let access = reopened
        .attach("schema19-navigation-reader".into())
        .unwrap();
    assert_eq!(schema_version(&path.join("project.sqlite3")), 25);
    assert_eq!(
        serde_json::to_vec(
            &reopened
                .story_snapshot(access.clone(), snapshot_id)
                .unwrap()
        )
        .unwrap(),
        frozen_bytes
    );
    assert_eq!(
        serde_json::to_vec(
            &reopened
                .prepared_context(access.clone(), packet.receipt.packet_id)
                .unwrap()
        )
        .unwrap(),
        packet_bytes
    );
    assert_eq!(
        reopened
            .document(access.clone(), saved.head.document_id.clone())
            .unwrap()
            .head,
        saved.head
    );
    let memory = reopened.read_memory(access, view.document_id).unwrap();
    assert!(
        memory
            .views
            .iter()
            .any(|retained| retained.id == view.id && retained.current)
    );
    let db = Connection::open(path.join("project.sqlite3")).unwrap();
    assert_eq!(retained_rows(&db), rows_before);
    let backups: Vec<_> = fs::read_dir(path.join("migrations"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(schema_version(&backups[0]), 17);
    let old = Connection::open(&backups[0]).unwrap();
    assert_eq!(retained_rows(&old), rows_before);
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_archive(path: &Path, manifest: &BackupManifest, database: &[u8]) {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("create test backup archive");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer
        .start_file("manifest.json", options)
        .expect("write manifest entry");
    writer
        .write_all(&serde_json::to_vec(manifest).expect("serialize manifest"))
        .expect("write manifest");
    writer
        .start_file("project.sqlite3", options)
        .expect("write database entry");
    writer.write_all(database).expect("write database");
    writer.finish().expect("finish backup archive");
}

fn archive_entries(path: &Path) -> (BackupManifest, Vec<u8>) {
    let file = File::open(path).expect("open backup archive");
    let mut archive = ZipArchive::new(file).expect("read backup archive");
    let mut manifest_bytes = Vec::new();
    archive
        .by_name("manifest.json")
        .expect("manifest entry")
        .read_to_end(&mut manifest_bytes)
        .expect("read manifest");
    let manifest = serde_json::from_slice(&manifest_bytes).expect("parse manifest");
    let mut database = Vec::new();
    archive
        .by_name("project.sqlite3")
        .expect("database entry")
        .read_to_end(&mut database)
        .expect("read database");
    (manifest, database)
}

fn setup_project(
    root: &Path,
) -> (
    ProjectSession,
    ProjectAccess,
    webnovel_core::projects::DocumentRecord,
    webnovel_core::projects::SaveAck,
) {
    let project = ProjectSession::create(root, "Migration source").expect("create project");
    let access = project
        .attach("migration-session".into())
        .expect("attach project");
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-document".into(),
            document_id: "chapter-one".into(),
            title: "Chapter one".into(),
            kind: "chapter".into(),
            body: body("before schema migration"),
        })
        .expect("create document");
    let saved = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "save-document".into(),
            expected: document.head.clone(),
            local_generation: "1".into(),
            body: body("saved before schema migration"),
            cause: SaveCause::Typing,
        })
        .expect("save document");
    let endpoint = Endpoint {
        block_id: "p1".into(),
        utf16_offset: 5,
    };
    project
        .save_view_state(
            access.clone(),
            saved.head.clone(),
            endpoint.clone(),
            Endpoint {
                block_id: "p1".into(),
                utf16_offset: 17,
            },
        )
        .expect("save view state");
    (project, access, document, saved)
}

/// Convert the current database into the previous schema2 shape. This is a
/// synthetic legacy database used only to exercise the upgrade boundary.
fn downgrade_to_schema2(path: &Path) {
    let database = path.join("project.sqlite3");
    let connection = Connection::open(&database).expect("open current database");
    remove_schema24_features(&connection).unwrap();
    legacy_schema::remove_schema19_features(&connection)
        .expect("remove schema19-only fixture features");
    remove_post_schema14_tables(&connection);
    connection
        .execute_batch(
            "DROP TRIGGER source_pin_receipts_no_update;
             DROP TRIGGER source_pin_receipts_no_delete;
             DROP TABLE source_pin_receipts;
             DROP TABLE source_pin_sets;
             DROP TABLE export_records;
             DROP TRIGGER command_receipts_no_proposal_collision;
             DROP TABLE proposal_receipts; DROP TABLE proposal_decisions; DROP TABLE proposal_versions; DROP TABLE proposals;
             DROP TABLE guidance_request_uses;
             DROP TABLE snapshot_guidance;
             DROP TABLE author_guidance_receipts;
             DROP TABLE author_guidance_heads;
             DROP TABLE author_guidance_versions;
             DROP TABLE discussion_output_events;
             DROP TABLE discussion_messages;
             DROP TABLE discussion_draft_receipts;
             DROP TABLE discussion_runs;
             DROP TABLE discussion_drafts;
             DROP TABLE discussion_threads;
             DROP TABLE context_packets;
             DROP TABLE snapshot_sources;
             DROP TABLE story_snapshots;
             DROP TABLE passage_projections;
             DROP TABLE document_aliases;
             ALTER TABLE project DROP COLUMN disclosure_policy_epoch;
             PRAGMA user_version=2;",
        )
        .expect("downgrade synthetic database to schema2");
    drop(connection);
}

/// Convert the current database into the previous schema3 shape while keeping
/// all frozen story snapshots. This is a synthetic legacy database used only
/// to exercise the context-packet migration boundary.
fn downgrade_to_schema3(path: &Path) {
    let database = path.join("project.sqlite3");
    let connection = Connection::open(&database).expect("open current database");
    remove_schema24_features(&connection).unwrap();
    legacy_schema::remove_schema19_features(&connection)
        .expect("remove schema19-only fixture features");
    remove_post_schema14_tables(&connection);
    connection
        .execute_batch(
            "DROP TRIGGER source_pin_receipts_no_update;
             DROP TRIGGER source_pin_receipts_no_delete;
             DROP TABLE source_pin_receipts;
             DROP TABLE source_pin_sets;
             DROP TABLE export_records;
             DROP TRIGGER command_receipts_no_proposal_collision;
             DROP TABLE proposal_receipts; DROP TABLE proposal_decisions; DROP TABLE proposal_versions; DROP TABLE proposals;
             DROP TABLE guidance_request_uses;
             DROP TABLE snapshot_guidance;
             DROP TABLE author_guidance_receipts;
             DROP TABLE author_guidance_heads;
             DROP TABLE author_guidance_versions;
             DROP TABLE discussion_output_events;
             DROP TABLE discussion_messages;
             DROP TABLE discussion_draft_receipts;
             DROP TABLE discussion_runs;
             DROP TABLE discussion_drafts;
             DROP TABLE discussion_threads;
             DROP TABLE context_packets;
             PRAGMA user_version=3;",
        )
        .expect("downgrade synthetic database to schema3");
    drop(connection);
}

fn schema_version(path: &Path) -> i64 {
    Connection::open(path)
        .expect("open database")
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read schema version")
}

fn remove_post_schema14_tables(connection: &Connection) {
    connection
        .execute_batch(
            "DROP TABLE snapshot_navigation_views; DROP TABLE memory_view_sources; DROP TABLE memory_views; DROP TABLE memory_results; DROP TABLE memory_jobs; ALTER TABLE snapshot_sources DROP COLUMN reader_position;
             DROP TRIGGER review_stages_no_update;
             DROP TRIGGER review_stages_no_delete;
             DROP TRIGGER ready_bundles_no_update;
             DROP TRIGGER ready_bundles_no_delete;
             DROP TRIGGER review_fences_no_update;
             DROP TRIGGER review_fences_no_delete;
             DROP TABLE review_fences;
             DROP TABLE ready_heads;
             DROP TABLE ready_bundles;
             DROP TABLE review_stages;
             DROP TABLE provider_results;
             DROP TABLE import_manifest; DROP TABLE import_id_map;
             DROP TABLE import_body_decisions; DROP TABLE import_legacy_records;",
        )
        .expect("remove later tables from synthetic legacy fixture");
}

fn source_policy(project: &ProjectSession, access: &ProjectAccess) -> InformationPolicy {
    InformationPolicy {
        version: project
            .context_epochs(access.clone())
            .expect("read policy version")
            .policy,
        audience: Audience::AuthorRoom,
        reader_frontier: None,
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    }
}

fn freeze_one_snapshot(
    project: &ProjectSession,
    access: &ProjectAccess,
    target: &webnovel_core::projects::DocumentRecord,
) -> String {
    project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: "context-migration-freeze".into(),
            expected: target.head.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: source_policy(project, access),
        })
        .expect("freeze story context")
        .snapshot
        .snapshot_id
}

#[test]
fn schema2_upgrade_preserves_documents_view_state_epoch_and_durable_pre_upgrade_backup() {
    let temp = TempDir::new("upgrade");
    let path = temp.child("legacy");
    let (project, access, document, saved) = setup_project(&path);
    let view = project
        .view_state(access.clone())
        .expect("read source view state")
        .expect("view state exists");
    let epoch = project.context_source_epoch().expect("read source epoch");
    drop(project);
    downgrade_to_schema2(&path);
    assert_eq!(schema_version(&path.join("project.sqlite3")), 2);

    let upgraded = ProjectSession::open(&path).expect("upgrade schema2 project");
    assert_eq!(schema_version(&path.join("project.sqlite3")), 25);
    assert_eq!(
        upgraded
            .context_source_epoch()
            .expect("read upgraded epoch"),
        epoch
    );
    let attached = upgraded
        .attach_snapshot("after-upgrade".into())
        .expect("attach upgraded project");
    assert_eq!(attached.documents.len(), 1);
    assert_eq!(attached.documents[0].head, saved.head);
    assert_eq!(
        attached.documents[0].body,
        body("saved before schema migration")
    );
    assert_eq!(attached.view_state, Some(view.clone()));
    assert_eq!(view.head, saved.head);
    assert_eq!(document.head.version, "0");
    drop(upgraded);

    let backups = fs::read_dir(path.join("migrations"))
        .expect("read pre-upgrade backups")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    assert_eq!(schema_version(&backups[0]), 2);
    let backup = Connection::open(&backups[0]).expect("open durable schema2 backup");
    let integrity: String = backup
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("check pre-upgrade backup");
    assert_eq!(integrity, "ok");
    let backed_body: String = backup
        .query_row(
            "SELECT body_json FROM documents WHERE id='chapter-one'",
            [],
            |row| row.get(0),
        )
        .expect("read backed document");
    assert!(backed_body.contains("saved before schema migration"));
    let backed_view: (String, i64, i64) = backup
        .query_row(
            "SELECT document_id,head_version,anchor_utf16_offset FROM view_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read backed view state");
    assert_eq!(backed_view, ("chapter-one".into(), 1, 5));
}

#[test]
fn schema3_upgrade_preserves_frozen_snapshot_and_useful_backup() {
    let temp = TempDir::new("schema3-upgrade");
    let path = temp.child("legacy");
    let (project, access, _document, _saved) = setup_project(&path);
    let target = project
        .document(access.clone(), "chapter-one".into())
        .expect("read current target");
    let snapshot_id = freeze_one_snapshot(&project, &access, &target);
    drop(project);
    downgrade_to_schema3(&path);
    assert_eq!(schema_version(&path.join("project.sqlite3")), 3);

    let upgraded = ProjectSession::open(&path).expect("upgrade schema3 project");
    assert_eq!(schema_version(&path.join("project.sqlite3")), 25);
    let connection =
        Connection::open(path.join("project.sqlite3")).expect("open migrated schema19 database");
    let discussion_tables: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='discussion_runs'",
            [],
            |row| row.get(0),
        )
        .expect("check migrated discussion tables");
    assert_eq!(discussion_tables, 1);
    drop(connection);
    let attached = upgraded
        .attach_snapshot("schema3-upgrade-reader".into())
        .expect("attach upgraded project");
    let restored = upgraded
        .story_snapshot(attached.access.clone(), snapshot_id.clone())
        .expect("read preserved frozen snapshot");
    assert_eq!(restored.snapshot.snapshot_id, snapshot_id);
    assert_eq!(restored.snapshot.sources.len(), 1);
    assert_eq!(
        restored.snapshot.sources[0].source.document_id,
        "chapter-one"
    );
    assert!(!restored.snapshot.sources[0].source.revision_id.is_empty());
    drop(upgraded);

    let backups = fs::read_dir(path.join("migrations"))
        .expect("read schema3 pre-upgrade backups")
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let backup = Connection::open(backups[0].path()).expect("open schema3 backup");
    let backup_schema: i64 = backup
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read schema3 backup version");
    assert_eq!(backup_schema, 3);
    let backup_snapshot: (i64, String) = backup
        .query_row("SELECT COUNT(*),MIN(id) FROM story_snapshots", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("read backed frozen snapshot");
    assert_eq!(backup_snapshot, (1, snapshot_id));
    let backup_pins: i64 = backup
        .query_row("SELECT COUNT(*) FROM snapshot_sources", [], |row| {
            row.get(0)
        })
        .expect("read backed snapshot pins");
    assert_eq!(backup_pins, 1);
}

#[test]
fn schema10_upgrade_adds_safe_brief_storage_and_preserves_old_packet_and_draft() {
    let temp = TempDir::new("schema10-safe-brief");
    let path = temp.child("legacy");
    let (project, access, document, _saved) = setup_project(&path);
    let current = project
        .document(access.clone(), document.head.document_id.clone())
        .expect("read current migration target");
    let snapshot_id = freeze_one_snapshot(&project, &access, &current);
    let packet = match project
        .prepare_context(PrepareContext {
            lookup: None,
            access: access.clone(),
            operation_id: "schema10-old-packet".into(),
            snapshot_id,
            instruction: "Keep the selected target grounded.".into(),
            mandatory_handles: Vec::new(),
            transient_mandatory_handles: None,
            safe_brief: None,
            scope: None,
            budget: MockContextBudget::new("100000", "100", "100"),
            provider_binding: None,
            response_contract: None,
        })
        .expect("prepare old packet")
    {
        PreparationResult::Prepared { packet, .. } => *packet,
        PreparationResult::BudgetRejected { .. } => panic!("old packet must fit budget"),
    };
    let draft = project
        .save_discussion_draft(SaveDiscussionDraft {
            access: access.clone(),
            operation_id: "schema10-old-draft".into(),
            document_id: document.head.document_id.clone(),
            expected_version: "0".into(),
            text: "A draft retained before migration.".into(),
            intent: Default::default(),
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            previous_run_id: None,
            lookup: None,
        })
        .expect("save old draft");
    drop(project);

    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    remove_schema24_features(&connection).unwrap();
    legacy_schema::remove_schema19_features(&connection).unwrap();
    remove_post_schema14_tables(&connection);
    connection
        .execute_batch(
            "ALTER TABLE discussion_drafts DROP COLUMN safe_brief_json;
             PRAGMA user_version=10;",
        )
        .expect("downgrade synthetic schema10 database");
    drop(connection);

    let upgraded = ProjectSession::open(&path).expect("upgrade schema10 project");
    assert_eq!(schema_version(&path.join("project.sqlite3")), 25);
    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    let safe_brief_column: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('discussion_drafts') WHERE name='safe_brief_json'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(safe_brief_column, 1);
    let stored_request: String = connection
        .query_row(
            "SELECT request_json FROM context_packets WHERE id=?",
            [&packet.receipt.packet_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        serde_json::from_str::<Value>(&stored_request)
            .unwrap()
            .get("safeBrief")
            .is_none()
    );
    drop(connection);

    let reopened = upgraded.attach("schema10-reader".into()).unwrap();
    let restored = upgraded
        .prepared_context(reopened.clone(), packet.receipt.packet_id)
        .expect("old packet remains readable after latest migration");
    assert_eq!(restored.receipt.input_hash, packet.receipt.input_hash);
    let restored_draft = upgraded
        .read_discussion(reopened, document.head.document_id)
        .unwrap()
        .draft
        .expect("old draft remains readable after latest migration");
    assert_eq!(restored_draft.text, draft.text);
    assert!(restored_draft.safe_brief.is_none());
}

#[test]
fn schema23_lookup_migration_preserves_legacy_composer_draft_and_starts_empty_lookup_tables() {
    let temp = TempDir::new("schema23-lookup");
    let path = temp.child("legacy");
    let (project, access, document, _) = setup_project(&path);
    let draft = project
        .save_discussion_draft(SaveDiscussionDraft {
            access: access.clone(),
            operation_id: "schema23-composer-draft".into(),
            document_id: document.head.document_id.clone(),
            expected_version: "0".into(),
            text: "A retained legacy composer draft with exact Unicode: 你好，旧稿。".into(),
            intent: Default::default(),
            basis: None,
            scope: None,
            pinned_document_ids: Vec::new(),
            safe_brief: None,
            previous_run_id: None,
            lookup: None,
        })
        .expect("save legacy composer draft");
    drop(project);

    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    remove_schema24_features(&connection).unwrap();
    connection.pragma_update(None, "user_version", 23).unwrap();
    drop(connection);

    let upgraded = ProjectSession::open(&path).expect("upgrade schema23 project");
    assert_eq!(schema_version(&path.join("project.sqlite3")), 25);
    let reader = upgraded.attach("schema23-lookup-reader".into()).unwrap();
    let restored = upgraded
        .read_discussion(reader, document.head.document_id.clone())
        .unwrap()
        .draft
        .expect("legacy composer draft remains readable");
    assert_eq!(restored.version, draft.version);
    assert_eq!(restored.text, draft.text);
    assert_eq!(restored.intent, draft.intent);
    assert_eq!(restored.pinned_document_ids, draft.pinned_document_ids);
    assert!(restored.lookup.is_none());

    let connection = Connection::open(path.join("project.sqlite3")).unwrap();
    for table in [
        "discussion_lookup_invocations",
        "discussion_lookup_results",
        "discussion_lookup_reads",
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "schema23 migration must start {table} empty");
    }
    let lookup_column: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('discussion_drafts') WHERE name='lookup_json'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lookup_column, 1);
    let lookup_value: Option<String> = connection
        .query_row(
            "SELECT lookup_json FROM discussion_drafts WHERE document_id=?",
            [&document.head.document_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(lookup_value.is_none());
}

#[test]
fn schema2_upgrade_failure_rolls_back_and_retains_durable_backup() {
    let temp = TempDir::new("upgrade-failure");
    let path = temp.child("legacy");
    let (project, _access, _document, saved) = setup_project(&path);
    let epoch = project.context_source_epoch().expect("read source epoch");
    drop(project);
    downgrade_to_schema2(&path);
    let connection = Connection::open(path.join("project.sqlite3")).expect("open schema2 database");
    connection
        .execute_batch(
            "CREATE TABLE story_snapshots (id TEXT PRIMARY KEY);
             PRAGMA user_version=2;",
        )
        .expect("install migration conflict");
    drop(connection);

    let error = ProjectSession::open(&path)
        .err()
        .expect("conflicting later migration must fail");
    assert_eq!(error.code, "PersistenceUnavailable");
    assert_eq!(schema_version(&path.join("project.sqlite3")), 2);
    let connection = Connection::open(path.join("project.sqlite3")).expect("reopen rolled back db");
    let has_disclosure_epoch: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('project') WHERE name='disclosure_policy_epoch'",
            [],
            |row| row.get(0),
        )
        .expect("inspect rolled back schema");
    assert_eq!(has_disclosure_epoch, 0);
    let (version, body_json): (i64, String) = connection
        .query_row(
            "SELECT working_version,body_json FROM documents WHERE id='chapter-one'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read rolled back document");
    assert_eq!(version, 1);
    assert!(body_json.contains("saved before schema migration"));
    let rolled_epoch: i64 = connection
        .query_row(
            "SELECT context_source_epoch FROM project WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .expect("read rolled back epoch");
    assert_eq!(rolled_epoch.to_string(), epoch);
    let view_head: i64 = connection
        .query_row(
            "SELECT head_version FROM view_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .expect("read rolled back view");
    assert_eq!(
        view_head,
        saved.head.version.parse::<i64>().expect("numeric head")
    );
    let backup = path
        .join("migrations")
        .read_dir()
        .expect("read migration dir")
        .next()
        .expect("backup entry")
        .expect("backup path");
    assert_eq!(schema_version(&backup.path()), 2);
}

#[test]
fn schema2_backup_recovers_forward_with_document_view_and_epoch() {
    let temp = TempDir::new("schema2-recovery");
    let source_path = temp.child("source");
    let (project, access, _document, saved) = setup_project(&source_path);
    let view = project
        .view_state(access.clone())
        .expect("read source view")
        .expect("source view exists");
    let epoch = project.context_source_epoch().expect("read source epoch");
    let current_archive = temp.child("current.wnsbackup");
    let current_manifest =
        create_backup(&project, &current_archive).expect("capture current manifest");
    drop(project);
    downgrade_to_schema2(&source_path);
    let database = fs::read(source_path.join("project.sqlite3")).expect("read schema2 database");
    let mut manifest = current_manifest;
    manifest.database_schema_version = 2;
    manifest.database_sha256 = sha256(&database);
    let archive = temp.child("schema2.wnsbackup");
    write_archive(&archive, &manifest, &database);

    let target = temp.child("recovered");
    let recovered =
        recover_backup(&archive, &target, "Recovered schema2").expect("recover schema2 backup");
    assert_eq!(schema_version(&target.join("project.sqlite3")), 25);
    assert_eq!(
        recovered
            .context_source_epoch()
            .expect("read recovered epoch"),
        epoch
    );
    let attached = recovered
        .attach_snapshot("schema2-recovery-reader".into())
        .expect("attach recovered project");
    assert_eq!(attached.documents[0].head, saved.head);
    assert_eq!(
        attached.documents[0].body,
        body("saved before schema migration")
    );
    assert_eq!(attached.view_state, Some(view));
    let migration_backups = fs::read_dir(target.join("migrations"))
        .expect("read recovered migration backups")
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    assert_eq!(migration_backups.len(), 1);
    assert_eq!(schema_version(&migration_backups[0].path()), 2);
}

#[test]
fn create_backup_rejects_malformed_persisted_context_manifest() {
    let temp = TempDir::new("malformed-manifest");
    let path = temp.child("source");
    let (project, access, _document, _saved) = setup_project(&path);
    let target = project
        .document(access.clone(), "chapter-one".into())
        .expect("read current target");
    let snapshot_id = freeze_one_snapshot(&project, &access, &target);
    drop(project);
    let connection = Connection::open(path.join("project.sqlite3")).expect("open source db");
    connection
        .execute_batch("DROP TRIGGER immutable_story_snapshot_update;")
        .expect("disable immutable manifest trigger for synthetic sabotage");
    connection
        .execute(
            "UPDATE story_snapshots SET manifest_json='{}' WHERE id=?",
            params![snapshot_id],
        )
        .expect("corrupt persisted context manifest");
    drop(connection);
    let reopened = ProjectSession::open(&path).expect("open source with persisted corruption");
    let target = temp.child("should-not-exist.wnsbackup");
    let error = create_backup(&reopened, &target)
        .expect_err("malformed persisted context must block backup");
    assert_eq!(error.code, "InvalidBackup");
    assert!(!target.exists());
}

#[test]
fn recovery_rejects_malformed_persisted_context_pin_without_installing_target() {
    let temp = TempDir::new("malformed-pin");
    let source_path = temp.child("source");
    let (project, access, _document, _saved) = setup_project(&source_path);
    let target_document = project
        .document(access.clone(), "chapter-one".into())
        .expect("read current target");
    freeze_one_snapshot(&project, &access, &target_document);
    let valid_archive = temp.child("valid.wnsbackup");
    create_backup(&project, &valid_archive).expect("create valid context backup");
    drop(project);

    let (mut manifest, mut database) = archive_entries(&valid_archive);
    let file_database = temp.child("pin-sabotaged.sqlite3");
    fs::write(&file_database, &database).expect("write scratch database");
    let connection = Connection::open(&file_database).expect("open scratch database");
    connection
        .execute_batch("DROP TRIGGER immutable_snapshot_source_update;")
        .expect("disable immutable pin trigger for synthetic sabotage");
    connection
        .execute("UPDATE snapshot_sources SET body_hash=?", ["0".repeat(64)])
        .expect("sabotage persisted source pin");
    drop(connection);
    database = fs::read(&file_database).expect("read sabotaged database");
    manifest.database_sha256 = sha256(&database);
    let bad_archive = temp.child("bad-pin.wnsbackup");
    write_archive(&bad_archive, &manifest, &database);

    let target = temp.child("should-not-install");
    let error = recover_backup(&bad_archive, &target, "Bad pin")
        .err()
        .expect("malformed persisted pin must block recovery");
    assert_eq!(error.code, "InvalidBackup");
    assert!(!target.exists());
}
