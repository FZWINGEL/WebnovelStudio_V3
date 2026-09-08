use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::knowledge_history::{
    KnowledgeHistoryUncertainty, query_knowledge_history,
};
use webnovel_core::context::reviewed_knowledge::{
    ReviewedKnowledgeSet, records_hash, validate_frozen_knowledge_set,
};
use webnovel_core::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    ReviewedBasisManifest, ReviewedBasisMember, SourceDescriptor, SourceKind, SourceRef,
    StorySnapshot,
};
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::story_context::{FreezeReviewedContinuation, FrozenContext};
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, KnowledgeAttitude, KnowledgeRecord, PossessionRecord,
    PossessionTiming, StoryEntityRef, canonical_knowledge_json, validate_knowledge,
};
use webnovel_core::projects::{CreateDocument, ProjectSession, SaveCause, SaveSnapshot};
use webnovel_core::transfer::{create_backup, recover_backup};
use webnovel_core::validate_snapshot_json;

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wns-reviewed-knowledge-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create test root");
        Self(path)
    }

    fn child(&self, name: &str) -> PathBuf {
        self.0.join(name)
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

fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn knowledge(text: &str, id: &str, audience: EvidenceAudience) -> KnowledgeRecord {
    KnowledgeRecord {
        id: id.into(),
        character: StoryEntityRef {
            id: "mei".into(),
            label: "Mei".into(),
        },
        topic: StoryEntityRef {
            id: "key".into(),
            label: "The key".into(),
        },
        attitude: KnowledgeAttitude::Knows,
        statement: "The key opens the eastern gate.".into(),
        timing: PossessionTiming::AtPassage,
        audience,
        evidence: EvidenceAnchor {
            block_id: "p1".into(),
            from_utf16: 0,
            to_utf16: text.encode_utf16().count() as u32,
            quote: text.into(),
            quote_hash: hash(text),
        },
    }
}

fn possession(text: &str, id: &str) -> PossessionRecord {
    PossessionRecord {
        id: id.into(),
        object: StoryEntityRef {
            id: "object-key".into(),
            label: "The key".into(),
        },
        holder: Some(StoryEntityRef {
            id: "mei".into(),
            label: "Mei".into(),
        }),
        timing: PossessionTiming::AtPassage,
        audience: EvidenceAudience::AuthorRoom,
        evidence: EvidenceAnchor {
            block_id: "p1".into(),
            from_utf16: 0,
            to_utf16: text.encode_utf16().count() as u32,
            quote: text.into(),
            quote_hash: hash(text),
        },
    }
}

#[test]
fn knowledge_validation_is_exact_and_canonical() {
    let text = "Mei knows the key opens the eastern gate.";
    let revision_body = body(text);
    let receipt = validate_snapshot_json(&serde_json::to_string(&revision_body).unwrap()).unwrap();
    let revision = webnovel_core::projects::Revision {
        id: "revision-1".into(),
        head: webnovel_core::projects::Head {
            document_id: "chapter-1".into(),
            version: "1".into(),
            body_hash: receipt.hash,
        },
        body: revision_body,
        reason: "test".into(),
        parent_id: None,
    };
    let record = knowledge(text, "knowledge-1", EvidenceAudience::Reader);
    let records = vec![record.clone()];
    let fingerprint = validate_knowledge(&records, &revision).unwrap().unwrap();
    assert_eq!(
        canonical_knowledge_json(&records).unwrap().unwrap(),
        serde_json::to_string(&records).unwrap()
    );
    assert_eq!(fingerprint, records_hash(&records).unwrap());

    let mut altered = record;
    altered.evidence.quote = "A different quote".into();
    assert_eq!(
        validate_knowledge(&[altered], &revision).unwrap_err().code,
        "InvalidReviewedEvidence"
    );
}

#[test]
fn stage_knowledge_supports_inherit_and_explicit_clear() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Reviewed knowledge").unwrap();
    let access = project.attach("knowledge-test".into()).unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-1".into(),
            title: "Chapter One".into(),
            kind: "chapter".into(),
            body: body("Mei knows the key opens the eastern gate."),
        })
        .unwrap();
    let first = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-knowledge".into(),
            expected: document.head.clone(),
            records: None,
            promises: None,
            knowledge: Some(vec![knowledge(
                "Mei knows the key opens the eastern gate.",
                "knowledge-1",
                EvidenceAudience::Reader,
            )]),
            summary: None,
        })
        .unwrap();
    assert!(first.knowledge.is_some());
    let first_stage_id = first.id.clone();
    let bundle = project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-knowledge".into(),
            stage_id: first_stage_id.clone(),
        })
        .unwrap();
    assert!(bundle.knowledge_hash.is_some());
    let idempotent = project.mark_ready(MarkReady {
        access: access.clone(),
        operation_id: "ready-knowledge".into(),
        stage_id: first_stage_id,
    });
    assert_eq!(idempotent.unwrap().id, bundle.id);

    let inherited = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-inherited-knowledge".into(),
            expected: document.head.clone(),
            records: None,
            promises: None,
            knowledge: None,
            summary: None,
        })
        .unwrap();
    assert_eq!(inherited.knowledge, bundle.knowledge);

    let cleared = project
        .stage_author_review(StageAuthorReview {
            access,
            operation_id: "stage-cleared-knowledge".into(),
            expected: document.head,
            records: None,
            promises: None,
            knowledge: Some(Vec::new()),
            summary: None,
        })
        .unwrap();
    assert!(cleared.knowledge.is_none());
    assert!(cleared.knowledge_hash.is_none());
}

#[test]
fn inherited_knowledge_with_stale_source_is_rejected_before_persistence() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Stale knowledge").unwrap();
    let access = project.attach("knowledge-stale".into()).unwrap();
    let text = "Mei knows the key opens the eastern gate.";
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-1".into(),
            title: "Chapter One".into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .unwrap();
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-knowledge".into(),
            expected: document.head.clone(),
            records: None,
            promises: None,
            knowledge: Some(vec![knowledge(
                text,
                "knowledge-1",
                EvidenceAudience::Reader,
            )]),
            summary: None,
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-knowledge".into(),
            stage_id: stage.id,
        })
        .unwrap();
    let edited = project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-source".into(),
            expected: document.head,
            local_generation: "1".into(),
            body: body("Mei has never seen the key."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    let error = project
        .stage_author_review(StageAuthorReview {
            access,
            operation_id: "stage-stale-inherit".into(),
            expected: edited.head,
            records: None,
            promises: None,
            knowledge: None,
            summary: None,
        })
        .unwrap_err();
    assert_eq!(error.code, "InvalidReviewedKnowledge");
}

#[test]
fn schema_33_migrates_legacy_rows_and_keeps_knowledge_absent_bytes_compatible() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Legacy knowledge").unwrap();
    let access = project.attach("knowledge-legacy".into()).unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-1".into(),
            title: "Chapter One".into(),
            kind: "chapter".into(),
            body: body("An unreviewed scene."),
        })
        .unwrap();
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-legacy".into(),
            expected: document.head.clone(),
            records: None,
            promises: None,
            knowledge: None,
            summary: None,
        })
        .unwrap();
    assert!(!serde_json::to_string(&stage).unwrap().contains("knowledge"));
    let bundle = project
        .mark_ready(MarkReady {
            access,
            operation_id: "ready-legacy".into(),
            stage_id: stage.id,
        })
        .unwrap();
    assert!(
        !serde_json::to_string(&bundle)
            .unwrap()
            .contains("knowledge")
    );
    drop(project);

    let database = temp.child("story").join("project.sqlite3");
    let connection = Connection::open(&database).unwrap();
    let nulls: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT knowledge_json,knowledge_hash FROM ready_bundles WHERE id=?",
            [&bundle.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(nulls, (None, None));
    crate::legacy_schema::remove_schema38_features(&connection).unwrap();
    connection
        .execute_batch(
            "ALTER TABLE review_stages DROP COLUMN knowledge_json;
             ALTER TABLE review_stages DROP COLUMN knowledge_hash;
             ALTER TABLE ready_bundles DROP COLUMN knowledge_json;
             ALTER TABLE ready_bundles DROP COLUMN knowledge_hash;
             PRAGMA user_version=32;",
        )
        .unwrap();
    drop(connection);

    let reopened = ProjectSession::open(temp.child("story")).unwrap();
    drop(reopened);
    let migrated = Connection::open(&database).unwrap();
    let version: i64 = migrated
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 38);
    for (table, column) in [
        ("review_stages", "knowledge_json"),
        ("review_stages", "knowledge_hash"),
        ("ready_bundles", "knowledge_json"),
        ("ready_bundles", "knowledge_hash"),
    ] {
        let present: bool = migrated
            .query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name=?)"),
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert!(present, "missing {table}.{column}");
    }
}

#[test]
fn reviewed_knowledge_rejects_an_operation_namespace_mismatch() {
    let descriptor = source("chapter-1", "chapter-1", SourceKind::ReviewedAuthority);
    let record = knowledge(
        "Mei knows the key opens the eastern gate.",
        "knowledge-1",
        EvidenceAudience::Reader,
    );
    let set = ReviewedKnowledgeSet {
        project_id: "knowledge-history-project".into(),
        operation_namespace: "wrong-namespace".into(),
        bundle_id: "bundle-1".into(),
        records_hash: records_hash(std::slice::from_ref(&record)).unwrap(),
        source_handle: descriptor.handle.clone(),
        source: descriptor.source.clone(),
        records: vec![record],
    };
    let snapshot = StorySnapshot {
        snapshot_id: "snapshot-reviewed".into(),
        project_id: "knowledge-history-project".into(),
        basis: BasisKind::Reviewed,
        target: descriptor.source.clone(),
        context_source_epoch: "1".into(),
        ordering_epoch: "1".into(),
        disclosure_policy_version: "1".into(),
        sources: vec![descriptor.clone()],
        reviewed_basis: Some(ReviewedBasisManifest {
            project_id: "knowledge-history-project".into(),
            operation_namespace: "right-namespace".into(),
            prefix: vec![ReviewedBasisMember {
                document_id: descriptor.source.document_id.clone(),
                bundle_id: "bundle-1".into(),
                revision_id: descriptor.source.revision_id.clone(),
                version: "1".into(),
                body_hash: descriptor.source.body_hash.clone(),
            }],
        }),
    };
    let policy = InformationPolicy {
        version: "1".into(),
        audience: Audience::RestrictedWriting,
        reader_frontier: Some("1".into()),
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    };
    let error = validate_frozen_knowledge_set(&set, &snapshot, &policy, ContextPurpose::Continue)
        .unwrap_err();
    assert_eq!(error.code, "InvalidReviewedKnowledge");
    assert!(error.detail.contains("operation namespace"));
}

#[test]
fn recovered_copy_keeps_knowledge_history_but_clears_active_authority() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Recovered knowledge").unwrap();
    let access = project.attach("knowledge-recovery".into()).unwrap();
    let first_text = "Mei knows the key opens the eastern gate.";
    let first = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-recovery-first".into(),
            document_id: "chapter-1".into(),
            title: "Chapter One".into(),
            kind: "chapter".into(),
            body: body(first_text),
        })
        .unwrap();
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-recovery-knowledge".into(),
            expected: first.head.clone(),
            records: None,
            promises: None,
            knowledge: Some(vec![knowledge(
                first_text,
                "recovery-knowledge",
                EvidenceAudience::Reader,
            )]),
            summary: None,
        })
        .unwrap();
    let bundle = project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-recovery-knowledge".into(),
            stage_id: stage.id,
        })
        .unwrap();
    let second = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-recovery-second".into(),
            document_id: "chapter-2".into(),
            title: "Chapter Two".into(),
            kind: "chapter".into(),
            body: body("The gate waits in silence."),
        })
        .unwrap();
    let policy = InformationPolicy {
        version: project.context_epochs(access.clone()).unwrap().policy,
        audience: Audience::RestrictedWriting,
        reader_frontier: Some("1".into()),
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    };
    let frozen = project
        .freeze_reviewed_continuation(FreezeReviewedContinuation {
            access: access.clone(),
            operation_id: "freeze-recovery-knowledge".into(),
            expected: second.head,
            policy,
        })
        .unwrap();
    let snapshot_id = frozen.snapshot.snapshot_id;
    let history = project
        .reviewed_knowledge_history(
            access.clone(),
            snapshot_id.clone(),
            "mei".into(),
            Some("key".into()),
        )
        .unwrap();
    assert_eq!(history.history.observations.len(), 1);
    assert_eq!(
        history.history.observations[0].record_id,
        "recovery-knowledge"
    );

    let archive = temp.child("knowledge.wnsbackup");
    create_backup(&project, &archive).unwrap();
    drop(project);

    let recovered =
        recover_backup(&archive, &temp.child("recovered"), "Recovered knowledge").unwrap();
    let recovered_access = recovered.attach("recovered-reader".into()).unwrap();
    assert!(
        recovered
            .read_reviewed_record_set(recovered_access.clone(), "chapter-1".into())
            .unwrap()
            .is_none()
    );
    drop(recovered);

    let database = Connection::open(temp.child("recovered").join("project.sqlite3")).unwrap();
    let active_heads: i64 = database
        .query_row("SELECT COUNT(*) FROM ready_heads", [], |row| row.get(0))
        .unwrap();
    assert_eq!(active_heads, 0);
    let retained_knowledge: Option<String> = database
        .query_row(
            "SELECT knowledge_json FROM ready_bundles WHERE id=?",
            [&bundle.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(retained_knowledge.is_some());
    let retained_snapshot: i64 = database
        .query_row(
            "SELECT COUNT(*) FROM story_snapshots WHERE id=?",
            [&snapshot_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained_snapshot, 1);
}

#[test]
fn ready_knowledge_transaction_rolls_back_bundle_head_and_epoch_on_insert_failure() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Rollback knowledge").unwrap();
    let access = project.attach("knowledge-rollback".into()).unwrap();
    let text = "Mei knows the key opens the eastern gate.";
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-rollback-chapter".into(),
            document_id: "chapter-1".into(),
            title: "Chapter One".into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .unwrap();
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-rollback-knowledge".into(),
            expected: document.head,
            records: None,
            promises: None,
            knowledge: Some(vec![knowledge(
                text,
                "rollback-knowledge",
                EvidenceAudience::Reader,
            )]),
            summary: None,
        })
        .unwrap();
    let before_epoch = project.context_epochs(access.clone()).unwrap().source;
    let database = Connection::open(project.path.join("project.sqlite3")).unwrap();
    database
        .execute_batch(
            "CREATE TRIGGER fail_ready_knowledge BEFORE INSERT ON ready_bundles
             WHEN NEW.operation_id='ready-rollback-knowledge'
             BEGIN SELECT RAISE(ABORT,'injected ready knowledge failure'); END;",
        )
        .unwrap();
    let error = project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-rollback-knowledge".into(),
            stage_id: stage.id.clone(),
        })
        .unwrap_err();
    assert_eq!(error.code, "PersistenceUnavailable");
    let bundle_count: i64 = database
        .query_row("SELECT COUNT(*) FROM ready_bundles", [], |row| row.get(0))
        .unwrap();
    let head_count: i64 = database
        .query_row("SELECT COUNT(*) FROM ready_heads", [], |row| row.get(0))
        .unwrap();
    let knowledge_count: i64 = database
        .query_row(
            "SELECT COUNT(*) FROM ready_bundles WHERE knowledge_json IS NOT NULL OR knowledge_hash IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((bundle_count, head_count, knowledge_count), (0, 0, 0));
    assert_eq!(
        project.context_epochs(access.clone()).unwrap().source,
        before_epoch
    );
    database
        .execute_batch("DROP TRIGGER fail_ready_knowledge;")
        .unwrap();
    let retried = project
        .mark_ready(MarkReady {
            access,
            operation_id: "ready-rollback-knowledge".into(),
            stage_id: stage.id,
        })
        .unwrap();
    assert!(retried.knowledge_hash.is_some());
}

#[test]
fn character_catalog_reuses_existing_possession_holders_before_knowledge() {
    let temp = TempProject::new();
    let project = ProjectSession::create(temp.child("story"), "Knowledge catalog").unwrap();
    let access = project.attach("knowledge-catalog".into()).unwrap();
    let document = project
        .create_document(CreateDocument {
            access: access.clone(),
            operation_id: "create-chapter".into(),
            document_id: "chapter-1".into(),
            title: "Chapter One".into(),
            kind: "chapter".into(),
            body: body("Mei carries the key."),
        })
        .unwrap();
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: "stage-possession-only".into(),
            expected: document.head.clone(),
            records: Some(vec![possession("Mei carries the key.", "possession-1")]),
            promises: None,
            knowledge: Some(Vec::new()),
            summary: None,
        })
        .unwrap();
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: "ready-possession-only".into(),
            stage_id: stage.id,
        })
        .unwrap();
    let catalog = project
        .reviewed_knowledge_character_catalog(access)
        .unwrap();
    assert!(
        catalog
            .entities
            .iter()
            .any(|choice| choice.entity.id == "mei")
    );
}

fn source(handle: &str, document_id: &str, kind: SourceKind) -> SourceDescriptor {
    let source_body = body("Mei knows the key opens the eastern gate.");
    let body_hash = validate_snapshot_json(&serde_json::to_string(&source_body).unwrap())
        .unwrap()
        .hash;
    SourceDescriptor {
        handle: handle.into(),
        source: SourceRef {
            project_id: "knowledge-history-project".into(),
            document_id: document_id.into(),
            revision_id: format!("revision-{document_id}"),
            body_hash,
        },
        display_name: format!("Chapter {document_id}"),
        kind,
        current: true,
        coverage: CoverageLabel::Verbatim,
        disclosure: Disclosure {
            reader_position: Some("1".into()),
            visible_to_characters: Vec::new(),
            author_only: false,
            future_private: false,
        },
        story_time: None,
        dependencies: Vec::new(),
    }
}

#[test]
fn history_keeps_attitudes_and_marks_disclosure_uncertainty() {
    let descriptor = source("chapter-1", "chapter-1", SourceKind::CurrentDraft);
    let record = knowledge(
        "Mei knows the key opens the eastern gate.",
        "knowledge-reader",
        EvidenceAudience::Reader,
    );
    let mut private = knowledge(
        "Mei knows the key opens the eastern gate.",
        "knowledge-private",
        EvidenceAudience::AuthorRoom,
    );
    private.attitude = KnowledgeAttitude::Suspects;
    let set = ReviewedKnowledgeSet {
        project_id: "knowledge-history-project".into(),
        operation_namespace: "knowledge-history-namespace".into(),
        bundle_id: "bundle-1".into(),
        records_hash: records_hash(&[record.clone(), private.clone()]).unwrap(),
        source_handle: descriptor.handle.clone(),
        source: descriptor.source.clone(),
        records: vec![record, private],
    };
    let frozen = FrozenContext {
        snapshot: StorySnapshot {
            snapshot_id: "snapshot-1".into(),
            project_id: "knowledge-history-project".into(),
            basis: BasisKind::Working,
            target: descriptor.source.clone(),
            context_source_epoch: "1".into(),
            ordering_epoch: "1".into(),
            disclosure_policy_version: "1".into(),
            sources: vec![descriptor],
            reviewed_basis: None,
        },
        policy: InformationPolicy {
            version: "1".into(),
            audience: Audience::RestrictedWriting,
            reader_frontier: Some("1".into()),
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        },
        purpose: ContextPurpose::Discuss,
        aliases: BTreeMap::new(),
        excluded_source_count: 0,
        guidance: Vec::new(),
        conversation: None,
        navigation_views: Vec::new(),
        reviewed_evidence: Vec::new(),
        reviewed_promises: Vec::new(),
        reviewed_knowledge: vec![set],
        reviewed_summaries: Vec::new(),
    };
    // The restricted route is intentionally rejected for Working snapshots;
    // this guards the same route boundary used by promises/evidence.
    assert_eq!(
        validate_frozen_knowledge_set(
            &frozen.reviewed_knowledge[0],
            &frozen.snapshot,
            &frozen.policy,
            frozen.purpose,
        )
        .unwrap_err()
        .code,
        "InvalidReviewedKnowledge"
    );

    let mut author_frozen = frozen;
    author_frozen.policy.audience = Audience::AuthorRoom;
    author_frozen.policy.reader_frontier = None;
    let history = query_knowledge_history(&author_frozen, "mei", Some("key")).unwrap();
    assert_eq!(history.observations.len(), 2);
    assert!(
        history
            .uncertainty
            .contains(&KnowledgeHistoryUncertainty::MultipleRecordedAttitudes)
    );
    author_frozen.reviewed_knowledge[0].records[1].timing = PossessionTiming::Earlier;
    author_frozen.reviewed_knowledge[0].records_hash =
        records_hash(&author_frozen.reviewed_knowledge[0].records).unwrap();
    let history = query_knowledge_history(&author_frozen, "mei", Some("key")).unwrap();
    assert!(
        history
            .uncertainty
            .contains(&KnowledgeHistoryUncertainty::EarlierOrUnknownTiming)
    );
}
