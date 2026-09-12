use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::memory::mock_navigation_digest;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::projects::context_packets::{PreparationResult, PrepareContext};
use webnovel_core::projects::discussions::ProviderOutcomeStatus;
use webnovel_core::projects::memory::{CompleteMemory, StartMemory};
use webnovel_core::projects::story_context::FreezeStory;
use webnovel_core::projects::{
    CreateDocument, DocumentRecord, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct Fixture {
    root: PathBuf,
    project: ProjectSession,
    access: ProjectAccess,
    target: DocumentRecord,
    optional: DocumentRecord,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("wns-navigation-{}", Uuid::new_v4()));
        let project = ProjectSession::create(&root, "Navigation storage").unwrap();
        let access = project.documents().attach("navigation-test".into()).unwrap();
        let target = project
            .documents().create(CreateDocument {
                access: access.clone(),
                operation_id: "create-target".into(),
                document_id: "target".into(),
                title: "Target".into(),
                kind: "chapter".into(),
                body: body("The target chapter opens."),
            })
            .unwrap();
        let optional = project
            .documents().create(CreateDocument {
                access: access.clone(),
                operation_id: "create-optional".into(),
                document_id: "optional".into(),
                title: "Optional".into(),
                kind: "chapter".into(),
                body: body("The optional chapter names the hidden gate."),
            })
            .unwrap();
        Self {
            root,
            project,
            access,
            target,
            optional,
        }
    }

    fn policy(&self) -> InformationPolicy {
        InformationPolicy {
            version: self
                .project
                .context_epochs(self.access.clone())
                .unwrap()
                .policy,
            audience: Audience::AuthorRoom,
            reader_frontier: None,
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        }
    }

    fn freeze(&self, operation: &str) -> webnovel_core::projects::story_context::FrozenContext {
        self.project
            .freeze_story(FreezeStory {
                access: self.access.clone(),
                operation_id: operation.into(),
                expected: self.target.head.clone(),
                basis: BasisKind::Working,
                purpose: ContextPurpose::StoryQuestion,
                policy: self.policy(),
            })
            .unwrap()
    }

    fn install_optional_memory(&self) {
        let job = self
            .project
            .start_memory(StartMemory {
                access: self.access.clone(),
                operation_id: "memory-optional".into(),
                expected: self.optional.head.clone(),
                budget: MockContextBudget::new("100000", "100", "100"),
                provider_binding: None,
            })
            .unwrap();
        let dispatch = self.project.begin_memory(job.owner.clone()).unwrap();
        let raw =
            serde_json::to_string(&mock_navigation_digest(&dispatch.source).unwrap()).unwrap();
        self.project
            .complete_memory(CompleteMemory {
                app_server: None,
                owner: dispatch.job.owner.clone(),
                event_id: "memory-optional-result".into(),
                raw_output: raw,
                outcome: ProviderOutcomeStatus::Completed,
                confirmed_stdin_bytes: None,
                usage: None,
                cleanup: None,
                error: None,
                effective_identity: None,
                delivery: None,
            })
            .unwrap();
        self.project.install_memory(job.owner).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
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
fn freeze_persists_current_navigation_and_keeps_historical_payload_after_stale_edit() {
    let fixture = Fixture::new();
    fixture.install_optional_memory();
    let frozen = fixture.freeze("freeze-with-navigation");
    assert_eq!(frozen.navigation_views.len(), 1);
    assert_eq!(
        frozen.navigation_views[0].candidate.source.document_id,
        "optional"
    );

    let db = Connection::open(fixture.root.join("project.sqlite3")).unwrap();
    let pins: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM snapshot_navigation_views WHERE snapshot_id=?",
            [&frozen.snapshot.snapshot_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pins, 1);
    drop(db);

    let packet = match fixture
        .project
        .prepare_context(PrepareContext {
            lookup: None,
            access: fixture.access.clone(),
            operation_id: "packet-with-navigation".into(),
            snapshot_id: frozen.snapshot.snapshot_id.clone(),
            instruction: "Read the frozen story context.".into(),
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
        PreparationResult::BudgetRejected { error } => {
            panic!("unexpected packet budget: {error:?}")
        }
    };
    let packet_after_edit = fixture
        .project
        .prepared_context(fixture.access.clone(), packet.receipt.packet_id.clone())
        .unwrap();
    assert_eq!(
        packet_after_edit.receipt.snapshot_id,
        frozen.snapshot.snapshot_id
    );

    let snapshot_id = frozen.snapshot.snapshot_id.clone();
    let restored = fixture
        .project
        .story_snapshot(fixture.access.clone(), snapshot_id)
        .unwrap();
    assert_eq!(restored.navigation_views.len(), 1);
    assert_eq!(
        restored.navigation_views[0].candidate.items,
        frozen.navigation_views[0].candidate.items
    );

    let optional = fixture
        .project
        .documents().read(fixture.access.clone(), "optional".into())
        .unwrap();
    fixture
        .project
        .documents().save(SaveSnapshot {
            access: fixture.access.clone(),
            operation_id: "edit-optional-after-freeze".into(),
            expected: optional.head,
            local_generation: "1".into(),
            body: body("The optional chapter names a different gate."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let current_target = fixture
        .project
        .documents().read(fixture.access.clone(), "target".into())
        .unwrap();
    let newer = fixture
        .project
        .freeze_story(FreezeStory {
            access: fixture.access.clone(),
            operation_id: "freeze-after-stale".into(),
            expected: current_target.head,
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: InformationPolicy {
                version: fixture
                    .project
                    .context_epochs(fixture.access.clone())
                    .unwrap()
                    .policy,
                audience: Audience::AuthorRoom,
                reader_frontier: None,
                character_id: None,
                character_grants: Vec::new(),
                allow_alternatives: false,
                allow_historical: false,
            },
        })
        .unwrap();
    assert!(newer.navigation_views.is_empty());
    assert_eq!(
        fixture
            .project
            .story_snapshot(fixture.access.clone(), frozen.snapshot.snapshot_id.clone())
            .unwrap()
            .navigation_views
            .len(),
        1
    );
    assert_eq!(
        fixture
            .project
            .prepared_context(fixture.access.clone(), packet.receipt.packet_id)
            .unwrap()
            .receipt
            .snapshot_id,
        frozen.snapshot.snapshot_id
    );
}

#[test]
fn unrelated_new_evidence_epoch_reuses_old_navigation_view() {
    let fixture = Fixture::new();
    fixture.install_optional_memory();
    let frozen = fixture.freeze("freeze-before-unrelated-evidence");
    assert_eq!(frozen.navigation_views.len(), 1);

    fixture
        .project
        .documents().create(CreateDocument {
            access: fixture.access.clone(),
            operation_id: "create-unretrieved-evidence".into(),
            document_id: "unretrieved".into(),
            title: "Unretrieved evidence".into(),
            kind: "chapter".into(),
            body: body("This chapter was added after the frozen navigation view."),
        })
        .unwrap();
    let target = fixture
        .project
        .documents().read(fixture.access.clone(), "target".into())
        .unwrap();
    let newer = fixture
        .project
        .freeze_story(FreezeStory {
            access: fixture.access.clone(),
            operation_id: "freeze-after-unrelated-evidence".into(),
            expected: target.head,
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: fixture.policy(),
        })
        .unwrap();
    assert_eq!(newer.navigation_views.len(), 1);
    assert_eq!(
        fixture
            .project
            .story_snapshot(fixture.access.clone(), frozen.snapshot.snapshot_id)
            .unwrap()
            .navigation_views
            .len(),
        1
    );
}

#[test]
fn schema17_missing_navigation_pin_table_rejects_empty_backup_and_reads() {
    let no_snapshot = Fixture::new();
    let db = Connection::open(no_snapshot.root.join("project.sqlite3")).unwrap();
    db.execute_batch("DROP TABLE snapshot_navigation_views;")
        .unwrap();
    drop(db);
    let backup = no_snapshot.root.with_extension("missing-empty.wnsbackup");
    assert_eq!(
        create_backup(&no_snapshot.project, &backup)
            .unwrap_err()
            .code,
        "InvalidBackup"
    );
    assert!(!backup.exists());
    drop(no_snapshot);

    let fixture = Fixture::new();
    let frozen = fixture.freeze("empty-navigation-manifest-without-table");
    assert!(frozen.navigation_views.is_empty());
    let db = Connection::open(fixture.root.join("project.sqlite3")).unwrap();
    db.execute_batch("DROP TABLE snapshot_navigation_views;")
        .unwrap();
    drop(db);
    assert_eq!(
        fixture
            .project
            .story_snapshot(fixture.access.clone(), frozen.snapshot.snapshot_id)
            .unwrap_err()
            .code,
        "InvalidContext"
    );
    let backup = fixture.root.with_extension("missing-read.wnsbackup");
    assert_eq!(
        create_backup(&fixture.project, &backup).unwrap_err().code,
        "InvalidBackup"
    );
    assert!(!backup.exists());
}

#[test]
fn backup_rejects_navigation_pin_hash_tampering() {
    let fixture = Fixture::new();
    fixture.install_optional_memory();
    let frozen = fixture.freeze("freeze-before-tamper");
    assert_eq!(frozen.navigation_views.len(), 1);
    let db = Connection::open(fixture.root.join("project.sqlite3")).unwrap();
    db.execute(
        "UPDATE snapshot_navigation_views SET content_hash='bad' WHERE snapshot_id=?",
        [&frozen.snapshot.snapshot_id],
    )
    .unwrap_err();
    // The immutable trigger rejects ordinary tampering; simulate a damaged
    // backup database the same way transfer tests do when checking validators.
    db.execute_batch("PRAGMA foreign_keys=OFF; DROP TRIGGER snapshot_navigation_views_no_update;")
        .unwrap();
    db.execute(
        "UPDATE snapshot_navigation_views SET content_hash='bad' WHERE snapshot_id=?",
        [&frozen.snapshot.snapshot_id],
    )
    .unwrap();
    drop(db);
    let backup = fixture.root.with_extension("wnsbackup");
    assert_eq!(
        create_backup(&fixture.project, &backup).unwrap_err().code,
        "InvalidBackup"
    );
    assert!(!backup.exists());
}

#[test]
fn restricted_and_revoked_reads_exclude_frozen_navigation() {
    let fixture = Fixture::new();
    fixture.install_optional_memory();
    let restricted = fixture
        .project
        .freeze_story(FreezeStory {
            access: fixture.access.clone(),
            operation_id: "restricted-no-navigation".into(),
            expected: fixture.target.head.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: InformationPolicy {
                version: fixture.policy().version,
                audience: Audience::RestrictedWriting,
                reader_frontier: Some("0".into()),
                character_id: None,
                character_grants: Vec::new(),
                allow_alternatives: false,
                allow_historical: false,
            },
        })
        .unwrap();
    assert!(restricted.navigation_views.is_empty());

    let author_snapshot = fixture.freeze("author-navigation-before-revoke");
    fixture
        .project
        .revoke_story_context(fixture.access.clone(), "0".into())
        .unwrap();
    assert_eq!(
        fixture
            .project
            .story_snapshot(fixture.access.clone(), author_snapshot.snapshot.snapshot_id,)
            .unwrap_err()
            .code,
        "ContextPolicyChanged"
    );
}

#[test]
fn backup_rejects_missing_and_extra_navigation_pins() {
    let fixture = Fixture::new();
    fixture.install_optional_memory();
    let _frozen = fixture.freeze("missing-navigation-pin");
    let db = Connection::open(fixture.root.join("project.sqlite3")).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys=OFF;
         DROP TRIGGER snapshot_navigation_views_no_delete;
         DELETE FROM snapshot_navigation_views WHERE snapshot_id=(SELECT id FROM story_snapshots WHERE operation_id='missing-navigation-pin');",
    )
    .unwrap();
    drop(db);
    let backup = fixture.root.with_extension("missing.wnsbackup");
    assert_eq!(
        create_backup(&fixture.project, &backup).unwrap_err().code,
        "InvalidBackup"
    );
    assert!(!backup.exists());
    drop(fixture);

    let extra = Fixture::new();
    let empty = extra.freeze("empty-navigation-manifest");
    assert!(empty.navigation_views.is_empty());
    let db = Connection::open(extra.root.join("project.sqlite3")).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys=OFF;
         DROP TRIGGER snapshot_navigation_views_no_update;
         INSERT INTO snapshot_navigation_views(snapshot_id,view_id,content_hash) VALUES((SELECT id FROM story_snapshots WHERE operation_id='empty-navigation-manifest'),'extra','bad');",
    )
    .unwrap();
    drop(db);
    let backup = extra.root.with_extension("extra.wnsbackup");
    assert_eq!(
        create_backup(&extra.project, &backup).unwrap_err().code,
        "InvalidBackup"
    );
    assert!(!backup.exists());
}

#[test]
fn backup_rejects_recursive_memory_snapshot_reference() {
    let fixture = Fixture::new();
    fixture.install_optional_memory();
    let frozen = fixture.freeze("recursive-navigation-pin");
    let db = Connection::open(fixture.root.join("project.sqlite3")).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys=OFF;
         DROP TRIGGER memory_jobs_identity_no_update;
         DROP TRIGGER memory_views_no_update;",
    )
    .unwrap();
    let job_id: String = db
        .query_row(
            "SELECT job_id FROM memory_views WHERE id=?",
            [&frozen.navigation_views[0].reference.view_id],
            |row| row.get(0),
        )
        .unwrap();
    db.execute(
        "UPDATE memory_jobs SET snapshot_id=? WHERE id=?",
        params_for_test(&frozen.snapshot.snapshot_id, &job_id),
    )
    .unwrap();
    db.execute(
        "UPDATE memory_views SET snapshot_id=? WHERE id=?",
        params_for_test(
            &frozen.snapshot.snapshot_id,
            &frozen.navigation_views[0].reference.view_id,
        ),
    )
    .unwrap();
    drop(db);
    let backup = fixture.root.with_extension("recursive.wnsbackup");
    assert_eq!(
        create_backup(&fixture.project, &backup).unwrap_err().code,
        "InvalidBackup"
    );
    assert!(!backup.exists());
}

#[test]
fn recovered_copy_does_not_reuse_original_navigation_views() {
    let fixture = Fixture::new();
    fixture.install_optional_memory();
    fixture
        .project
        .documents().create(CreateDocument {
            access: fixture.access.clone(),
            operation_id: "create-copy-unrelated".into(),
            document_id: "copy-unrelated".into(),
            title: "Unrelated copy chapter".into(),
            kind: "chapter".into(),
            body: body("This advances the source epoch after memory generation."),
        })
        .unwrap();
    let frozen = fixture.freeze("original-navigation-copy");
    assert_eq!(frozen.navigation_views.len(), 1);
    assert_eq!(
        fixture
            .project
            .story_snapshot(fixture.access.clone(), frozen.snapshot.snapshot_id.clone())
            .unwrap()
            .navigation_views
            .len(),
        1
    );

    let archive = fixture.root.with_extension("copy.wnsbackup");
    create_backup(&fixture.project, &archive).unwrap();
    let recovered_root = fixture.root.with_extension("recovered");
    let recovered = recover_backup(&archive, &recovered_root, "Recovered navigation").unwrap();
    let recovered_access = recovered.documents().attach("recovered-navigation".into()).unwrap();
    assert_ne!(recovered.info.project_id, frozen.snapshot.project_id);
    let recovered_memory = recovered
        .read_memory(recovered_access.clone(), "optional".into())
        .unwrap();
    assert!(recovered_memory.views.iter().any(|view| {
        view.id == frozen.navigation_views[0].reference.view_id && view.historical && !view.current
    }));
    assert_eq!(
        recovered
            .story_snapshot(
                recovered_access.clone(),
                frozen.snapshot.snapshot_id.clone()
            )
            .unwrap_err()
            .code,
        "ContextProjectMismatch"
    );

    let recovered_target = recovered
        .documents().read(recovered_access.clone(), "target".into())
        .unwrap();
    let epochs = recovered.context_epochs(recovered_access.clone()).unwrap();
    let new_snapshot = recovered
        .freeze_story(FreezeStory {
            access: recovered_access,
            operation_id: "recovered-navigation-freeze".into(),
            expected: recovered_target.head,
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: InformationPolicy {
                version: epochs.policy,
                audience: Audience::AuthorRoom,
                reader_frontier: None,
                character_id: None,
                character_grants: Vec::new(),
                allow_alternatives: false,
                allow_historical: false,
            },
        })
        .unwrap();
    assert!(new_snapshot.navigation_views.is_empty());
}

fn params_for_test<'a>(first: &'a str, second: &'a str) -> [&'a str; 2] {
    [first, second]
}
