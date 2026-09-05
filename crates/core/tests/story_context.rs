use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::projects::story_context::{FreezeStory, FrozenContext, SearchMode, SearchStory};
use webnovel_core::projects::{
    CreateDocument, DocumentRecord, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};
use webnovel_core::transfer::{create_backup, recover_backup};

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Fixture {
    project: ProjectSession,
    access: ProjectAccess,
    root: PathBuf,
    _cleanup: Cleanup,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("wns-context-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project = ProjectSession::create(root.join("story"), "Test story").unwrap();
        let access = project.attach("context-test".into()).unwrap();
        Self {
            _cleanup: Cleanup(root.clone()),
            root,
            project,
            access,
        }
    }
    fn document(&self, id: &str, kind: &str, text: &str) -> DocumentRecord {
        self.project
            .create_document(CreateDocument {
                access: self.access.clone(),
                operation_id: Uuid::new_v4().to_string(),
                document_id: id.into(),
                title: id.into(),
                kind: kind.into(),
                body: body(text),
            })
            .unwrap()
    }
    fn request(&self, target: &DocumentRecord) -> FreezeStory {
        FreezeStory {
            access: self.access.clone(),
            operation_id: Uuid::new_v4().to_string(),
            expected: target.head.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::StoryQuestion,
            policy: InformationPolicy {
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
            },
        }
    }
    fn freeze(&self, target: &DocumentRecord) -> FrozenContext {
        self.project.freeze_story(self.request(target)).unwrap()
    }
    fn search(
        &self,
        snapshot: &FrozenContext,
        query: &str,
    ) -> webnovel_core::projects::story_context::SearchResult {
        self.project
            .search_story(SearchStory {
                access: self.access.clone(),
                snapshot_id: snapshot.snapshot.snapshot_id.clone(),
                query: query.into(),
                mode: SearchMode::Literal,
                limit: 100,
            })
            .unwrap()
    }
    fn save(&self, document: &DocumentRecord, text: &str) {
        self.project
            .save(SaveSnapshot {
                access: self.access.clone(),
                operation_id: Uuid::new_v4().to_string(),
                expected: document.head.clone(),
                local_generation: "1".into(),
                body: body(text),
                cause: SaveCause::Typing,
            })
            .unwrap();
    }
}

fn body(text: &str) -> Value {
    json!({"schemaVersion":1,"body":{"type":"doc","content":[{"type":"paragraph","attrs":{"id":"p1"},"content":[{"type":"text","text":text}]}]}})
}

#[test]
fn frozen_sources_survive_new_edits_and_unretrieved_evidence_stales_proposals() {
    let f = Fixture::new();
    let early = f.document(
        "chapter-1",
        "chapter",
        "Mei promised to return the pendant.",
    );
    let current = f.document("chapter-120", "chapter", "The confrontation begins.");
    let before = f.freeze(&current);
    let epoch = f.project.context_epochs(f.access.clone()).unwrap().source;
    f.project.rebuild_story_index(f.access.clone()).unwrap();
    assert_eq!(
        f.project.context_epochs(f.access.clone()).unwrap().source,
        epoch
    );
    f.save(&early, "Mei promised to return the key.");
    assert!(
        !f.project
            .story_snapshot_is_current(f.access.clone(), before.snapshot.snapshot_id.clone())
            .unwrap()
    );
    assert_eq!(f.search(&before, "pendant").hits.len(), 1);
    assert!(f.search(&before, "key").hits.is_empty());
    let after = f.freeze(&current);
    assert_eq!(f.search(&after, "key").hits.len(), 1);
    assert!(f.search(&after, "pendant").hits.is_empty());
    let source = after
        .snapshot
        .sources
        .iter()
        .find(|source| source.source.document_id == "chapter-1")
        .unwrap();
    assert!(
        !f.project
            .read_story_source(
                f.access.clone(),
                after.snapshot.snapshot_id.clone(),
                source.handle.clone()
            )
            .unwrap()
            .used_validated_projection
    );
    f.document(
        "unretrieved-new-chapter",
        "chapter",
        "Her brother later gave the key to Lian.",
    );
    assert!(
        !f.project
            .story_snapshot_is_current(f.access.clone(), after.snapshot.snapshot_id.clone())
            .unwrap()
    );
}

#[test]
fn frozen_snapshot_and_retry_survive_restart_without_repinning_newer_text() {
    let f = Fixture::new();
    let document = f.document("chapter", "chapter", "The old promise.");
    let request = f.request(&document);
    let frozen = f.project.freeze_story(request.clone()).unwrap();
    f.save(&document, "The changed promise.");
    let retry = f.project.freeze_story(request.clone()).unwrap();
    assert_eq!(retry.snapshot.snapshot_id, frozen.snapshot.snapshot_id);
    let mut changed = request.clone();
    changed.purpose = ContextPurpose::Discuss;
    assert_eq!(
        f.project.freeze_story(changed).unwrap_err().code,
        "OperationIdReusedWithDifferentPayload"
    );
    let path = f.project.path.clone();
    let Fixture {
        root,
        project,
        _cleanup,
        ..
    } = f;
    drop(project);
    let reopened = ProjectSession::open(path).unwrap();
    let access = reopened.attach("restarted".into()).unwrap();
    let restored = reopened
        .story_snapshot(access.clone(), frozen.snapshot.snapshot_id.clone())
        .unwrap();
    let read = reopened
        .read_story_source(
            access,
            restored.snapshot.snapshot_id,
            restored.snapshot.sources[0].handle.clone(),
        )
        .unwrap();
    assert_eq!(read.passages[0].text, "The old promise.");
    drop(reopened);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restricted_snapshot_excludes_private_titles_aliases_and_later_chapters() {
    let f = Fixture::new();
    let early = f.document("early", "chapter", "A pendant glinted.");
    f.project
        .set_document_aliases(
            f.access.clone(),
            early.head.document_id.clone(),
            f.project.context_epochs(f.access.clone()).unwrap().source,
            vec!["Hidden culprit".into()],
        )
        .unwrap();
    f.document(
        "future-secret-title",
        "note",
        "The mentor killed her father.",
    );
    f.document(
        "future-chapter",
        "chapter",
        "Mei finally discovers the murderer.",
    );
    f.project
        .set_document_aliases(
            f.access.clone(),
            "future-secret-title".into(),
            f.project.context_epochs(f.access.clone()).unwrap().source,
            vec!["murderer".into()],
        )
        .unwrap();
    let mut request = f.request(&early);
    request.policy.audience = Audience::RestrictedWriting;
    request.policy.reader_frontier = Some("0".into());
    let frozen = f.project.freeze_story(request).unwrap();
    assert_eq!(frozen.snapshot.sources.len(), 1);
    assert_eq!(frozen.excluded_source_count, 2);
    let serialized = serde_json::to_string(&frozen).unwrap();
    assert!(!serialized.contains("future-secret-title"));
    assert!(!serialized.contains("murderer"));
    assert!(!serialized.contains("Hidden culprit"));
    assert!(f.search(&frozen, "mentor").hits.is_empty());
    let private = f.freeze(&early);
    let handle = private
        .snapshot
        .sources
        .iter()
        .find(|s| s.source.document_id == "future-secret-title")
        .unwrap()
        .handle
        .clone();
    assert_eq!(
        f.project
            .read_story_source(f.access.clone(), frozen.snapshot.snapshot_id, handle)
            .unwrap_err()
            .code,
        "ContextSourceDisallowed"
    );
}

#[test]
fn policy_revocation_blocks_old_reads_search_retry_and_submission_basis() {
    let f = Fixture::new();
    let document = f.document("chapter", "chapter", "A secret.");
    let request = f.request(&document);
    let frozen = f.project.freeze_story(request.clone()).unwrap();
    let new_epochs = f
        .project
        .revoke_story_context(f.access.clone(), "0".into())
        .unwrap();
    assert_eq!(new_epochs.policy, "1");
    assert_eq!(
        f.project
            .story_snapshot(f.access.clone(), frozen.snapshot.snapshot_id.clone())
            .unwrap_err()
            .code,
        "ContextPolicyChanged"
    );
    assert_eq!(
        f.project
            .read_story_source(
                f.access.clone(),
                frozen.snapshot.snapshot_id.clone(),
                frozen.snapshot.sources[0].handle.clone()
            )
            .unwrap_err()
            .code,
        "ContextPolicyChanged"
    );
    assert_eq!(
        f.project.freeze_story(request).unwrap_err().code,
        "ContextPolicyChanged"
    );
    assert_eq!(
        f.project
            .revoke_story_context(f.access.clone(), "0".into())
            .unwrap_err()
            .code,
        "ContextPolicyChanged"
    );
    assert!(
        f.project
            .story_snapshot_is_current(f.access.clone(), f.freeze(&document).snapshot.snapshot_id)
            .unwrap()
    );
}

#[test]
fn repeated_unicode_quotes_keep_distinct_utf16_anchors_and_aliases_are_snapshot_bound() {
    let f = Fixture::new();
    let document = f.document("elodie", "chapter", "🌙 Mei vows. Mei vows. Li waits.");
    f.project
        .rename_document(
            f.access.clone(),
            document.head.document_id.clone(),
            "0".into(),
            "Élodie".into(),
        )
        .unwrap();
    f.project
        .set_document_aliases(
            f.access.clone(),
            document.head.document_id.clone(),
            f.project.context_epochs(f.access.clone()).unwrap().source,
            vec!["Mei-Lin".into(), "Méi".into()],
        )
        .unwrap();
    let frozen = f.freeze(&document);
    let hits = f.search(&frozen, "Mei").hits;
    assert_eq!(hits.len(), 2);
    assert_eq!((hits[0].start_utf16, hits[0].end_utf16), (3, 6));
    assert_eq!((hits[1].start_utf16, hits[1].end_utf16), (13, 16));
    assert_eq!(f.search(&frozen, "Li").hits.len(), 1);
    let alias = f
        .project
        .search_story(SearchStory {
            access: f.access.clone(),
            snapshot_id: frozen.snapshot.snapshot_id.clone(),
            query: "méi".into(),
            mode: SearchMode::ExactAlias,
            limit: 10,
        })
        .unwrap();
    assert!(alias.hits.is_empty());
    assert_eq!(alias.source_matches.len(), 1);
    f.project
        .set_document_aliases(
            f.access.clone(),
            document.head.document_id.clone(),
            f.project.context_epochs(f.access.clone()).unwrap().source,
            vec!["Another name".into()],
        )
        .unwrap();
    assert_eq!(
        f.project
            .story_snapshot(f.access.clone(), frozen.snapshot.snapshot_id.clone())
            .unwrap()
            .aliases,
        frozen.aliases
    );
}

#[test]
fn deleting_or_corrupting_projection_rows_cannot_hide_saved_evidence() {
    let f = Fixture::new();
    let document = f.document("chapter", "chapter", "An old promise remains.");
    let frozen = f.freeze(&document);
    let handle = frozen.snapshot.sources[0].handle.clone();
    f.project.rebuild_story_index(f.access.clone()).unwrap();
    assert!(
        f.project
            .read_story_source(
                f.access.clone(),
                frozen.snapshot.snapshot_id.clone(),
                handle.clone()
            )
            .unwrap()
            .used_validated_projection
    );
    // Synthetic index corruption, never author data. The projection is not an
    // authority and must be revalidated even if its claimed body hash matches.
    let db = Connection::open(f.project.path.join("project.sqlite3")).unwrap();
    db.execute("UPDATE passage_projections SET text='wrong text'", [])
        .unwrap();
    drop(db);
    assert!(
        !f.project
            .read_story_source(
                f.access.clone(),
                frozen.snapshot.snapshot_id.clone(),
                handle.clone()
            )
            .unwrap()
            .used_validated_projection
    );
    assert_eq!(f.search(&frozen, "promise").hits.len(), 1);
    f.project.clear_story_index(f.access.clone()).unwrap();
    assert_eq!(f.search(&frozen, "promise").hits.len(), 1);
    f.project.rebuild_story_index(f.access.clone()).unwrap();
    assert_eq!(f.search(&frozen, "promise").hits.len(), 1);
}

#[test]
fn project_switch_and_recovery_cannot_redirect_old_snapshot_handles() {
    let f = Fixture::new();
    let document = f.document("chapter", "chapter", "Only in this project.");
    let frozen = f.freeze(&document);
    let other = ProjectSession::create(f.root.join("other"), "Other").unwrap();
    let other_access = other.attach("other".into()).unwrap();
    assert_eq!(
        other
            .story_snapshot(other_access, frozen.snapshot.snapshot_id.clone())
            .unwrap_err()
            .code,
        "ContextNotFound"
    );
    let archive = f.root.join("backup.wnsbackup");
    create_backup(&f.project, &archive).unwrap();
    let recovered = recover_backup(&archive, &f.root.join("recovered"), "Recovered").unwrap();
    let access = recovered.attach("recovered".into()).unwrap();
    assert_eq!(
        recovered
            .story_snapshot(access.clone(), frozen.snapshot.snapshot_id.clone())
            .unwrap_err()
            .code,
        "ContextProjectMismatch"
    );
    let mut request = f.request(&document);
    request.access = access;
    let new = recovered.freeze_story(request).unwrap();
    assert_ne!(new.snapshot.project_id, frozen.snapshot.project_id);
}

#[test]
fn unsupported_authority_and_character_policy_fail_closed() {
    let f = Fixture::new();
    let document = f.document("chapter", "chapter", "A draft, not reviewed canon.");
    for basis in [BasisKind::Reviewed, BasisKind::ExplicitHistory] {
        let mut request = f.request(&document);
        request.basis = basis;
        assert_eq!(
            f.project.freeze_story(request).unwrap_err().code,
            "BasisUnavailable"
        );
    }
    for purpose in [ContextPurpose::Revise, ContextPurpose::Continue] {
        let mut request = f.request(&document);
        request.purpose = purpose;
        assert_eq!(
            f.project.freeze_story(request).unwrap_err().code,
            "BoundaryConflict"
        );
    }
    let mut request = f.request(&document);
    request.policy.character_id = Some("Mei".into());
    assert_eq!(
        f.project.freeze_story(request).unwrap_err().code,
        "CharacterPolicyUnavailable"
    );
}

#[test]
fn failed_snapshot_pin_insert_rolls_back_all_new_checkpoints() {
    let f = Fixture::new();
    let document = f.document("chapter", "chapter", "Uncheckpointed source.");
    let db = Connection::open(f.project.path.join("project.sqlite3")).unwrap();
    let before: i64 = db
        .query_row("SELECT COUNT(*) FROM revisions", [], |r| r.get(0))
        .unwrap();
    db.execute_batch("CREATE TRIGGER fail_pin BEFORE INSERT ON snapshot_sources BEGIN SELECT RAISE(ABORT,'injected pin failure'); END;").unwrap();
    assert!(f.project.freeze_story(f.request(&document)).is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM revisions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        before
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_snapshots", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_pin").unwrap();
    assert!(f.project.freeze_story(f.request(&document)).is_ok());
}

#[test]
fn cold_snapshot_cost_is_measured_on_large_synthetic_story() {
    let f = Fixture::new();
    let prose = "The old road wound between the terraces. Mei remembered her promise. ".repeat(120);
    // Populate a synthetic 1,000-chapter DB in one transaction so the timing
    // measures cold snapshot construction, not fixture creation/IPC overhead.
    let mut db = Connection::open(f.project.path.join("project.sqlite3")).unwrap();
    let tx = db.transaction().unwrap();
    let validated =
        webnovel_core::validate_snapshot_json(&serde_json::to_string(&body(&prose)).unwrap())
            .unwrap();
    for i in 0..1000 {
        tx.execute("INSERT INTO documents(id,kind,title,position,working_version,schema_version,body_json,body_hash) VALUES(?,'chapter',?,?,0,1,?,?)", params![format!("chapter-{i}"),format!("Chapter {i}"),i,validated.canonical_json,validated.hash]).unwrap();
    }
    tx.execute("UPDATE project SET context_source_epoch=1000", [])
        .unwrap();
    tx.commit().unwrap();
    drop(db);
    let target = f
        .project
        .document(f.access.clone(), "chapter-999".into())
        .unwrap();
    let start = Instant::now();
    let frozen = f.freeze(&target);
    let elapsed = start.elapsed();
    assert_eq!(frozen.snapshot.sources.len(), 1000);
    eprintln!(
        "C1 cold snapshot: 1000 chapters, {} UTF-8 prose bytes, {} ms (debug test build)",
        prose.len() * 1000,
        elapsed.as_millis()
    );
}
