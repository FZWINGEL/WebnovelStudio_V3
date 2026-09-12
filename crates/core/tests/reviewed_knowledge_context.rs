use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::context::packet::{PacketRequest, compile_packet};
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::documents::{ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::context_packets::{PreparationResult, PrepareContext};
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::story_context::{
    FreezeReviewedContinuation, FreezeStory, FrozenContext,
};
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, KnowledgeAttitude, KnowledgeRecord, PossessionTiming,
    StoryEntityRef,
};
use webnovel_core::projects::{
    CreateDocument, DocumentRecord, ProjectAccess, ProjectSession, SaveCause, SaveSnapshot,
};

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

fn setup(name: &str) -> (Cleanup, ProjectSession, ProjectAccess) {
    let root =
        std::env::temp_dir().join(format!("wns-reviewed-knowledge-{name}-{}", Uuid::new_v4()));
    fs::create_dir(&root).expect("create test root");
    let project = ProjectSession::create(root.join("story"), "Reviewed knowledge context test")
        .expect("create project");
    let access = project
        .documents().attach(format!("reviewed-knowledge-{name}"))
        .expect("attach project");
    (Cleanup(root), project, access)
}

fn chapter(
    project: &ProjectSession,
    access: &ProjectAccess,
    id: &str,
    title: &str,
    text: &str,
) -> DocumentRecord {
    project
        .documents().create(CreateDocument {
            access: access.clone(),
            operation_id: format!("create-{id}"),
            document_id: id.into(),
            title: title.into(),
            kind: "chapter".into(),
            body: body(text),
        })
        .expect("create chapter")
}

fn ready(
    project: &ProjectSession,
    access: &ProjectAccess,
    document: &DocumentRecord,
    operation: &str,
    knowledge: Vec<KnowledgeRecord>,
) -> webnovel_core::projects::reviewed_story::ReadyBundle {
    let stage = project
        .stage_author_review(StageAuthorReview {
            access: access.clone(),
            operation_id: format!("stage-{operation}"),
            expected: document.head.clone(),
            records: None,
            promises: None,
            summary: None,
            knowledge: Some(knowledge),
        })
        .expect("stage review");
    project
        .mark_ready(MarkReady {
            access: access.clone(),
            operation_id: format!("ready-{operation}"),
            stage_id: stage.id,
        })
        .expect("mark review ready")
}

fn working_policy(project: &ProjectSession, access: &ProjectAccess) -> InformationPolicy {
    InformationPolicy {
        version: project
            .context_epochs(access.clone())
            .expect("read epochs")
            .policy,
        audience: Audience::AuthorRoom,
        reader_frontier: None,
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    }
}

fn freeze_working(
    project: &ProjectSession,
    access: &ProjectAccess,
    target: &DocumentRecord,
    operation: &str,
) -> FrozenContext {
    project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: operation.into(),
            expected: target.head.clone(),
            basis: BasisKind::Working,
            purpose: ContextPurpose::Discuss,
            policy: working_policy(project, access),
        })
        .expect("freeze working context")
}

fn freeze_reviewed(
    project: &ProjectSession,
    access: &ProjectAccess,
    target: &DocumentRecord,
    operation: &str,
    frontier: &str,
) -> FrozenContext {
    let mut policy = working_policy(project, access);
    policy.audience = Audience::RestrictedWriting;
    policy.reader_frontier = Some(frontier.into());
    project
        .freeze_reviewed_continuation(FreezeReviewedContinuation {
            access: access.clone(),
            operation_id: operation.into(),
            expected: target.head.clone(),
            policy,
        })
        .expect("freeze reviewed continuation")
}

fn packet(
    project: &ProjectSession,
    access: &ProjectAccess,
    frozen: &FrozenContext,
    operation: &str,
    budget: MockContextBudget,
    scope: Option<ScopeGrant>,
) -> webnovel_core::context::packet::CompiledPacket {
    match project
        .prepare_context(PrepareContext {
            access: access.clone(),
            operation_id: operation.into(),
            snapshot_id: frozen.snapshot.snapshot_id.clone(),
            instruction: "Use the accepted story context for this discussion.".into(),
            mandatory_handles: Vec::new(),
            transient_mandatory_handles: None,
            safe_brief: None,
            scope,
            budget,
            provider_binding: None,
            response_contract: None,
            lookup: None,
        })
        .expect("prepare context packet")
    {
        PreparationResult::Prepared { packet, .. } => *packet,
        PreparationResult::BudgetRejected { error } => {
            panic!("packet unexpectedly rejected: {error:?}")
        }
    }
}

fn envelope(packet: &webnovel_core::context::packet::CompiledPacket) -> Value {
    serde_json::from_str(&packet.messages[1].content).expect("decode context envelope")
}

fn knowledge(id: &str, quote: &str, audience: EvidenceAudience) -> KnowledgeRecord {
    KnowledgeRecord {
        id: id.into(),
        character: StoryEntityRef {
            id: "character-mei".into(),
            label: "Mei".into(),
        },
        topic: StoryEntityRef {
            id: "topic-key".into(),
            label: "The key's location".into(),
        },
        attitude: KnowledgeAttitude::Believes,
        statement: "The key is hidden in the tower.".into(),
        timing: PossessionTiming::AtPassage,
        audience,
        evidence: EvidenceAnchor {
            block_id: "p1".into(),
            from_utf16: 0,
            to_utf16: quote.encode_utf16().count() as u32,
            quote: quote.into(),
            quote_hash: hash_json(quote),
        },
    }
}

fn whole_scope(target: &DocumentRecord) -> ScopeGrant {
    capture_scope(
        &target.body,
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
    .unwrap()
}

#[test]
fn working_packet_carries_exact_knowledge_and_interpretation_without_changing_prose() {
    let (_cleanup, project, access) = setup("working");
    let text = "Mei believed the key was hidden in the tower.";
    let source = chapter(&project, &access, "chapter-1", "An uncertain belief", text);
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "The search",
        "Mei began to search.",
    );
    let record = knowledge("belief", text, EvidenceAudience::AuthorRoom);
    let bundle = ready(&project, &access, &source, "belief", vec![record.clone()]);
    let frozen = freeze_working(&project, &access, &target, "freeze");
    assert_eq!(frozen.reviewed_knowledge[0].records, vec![record]);
    let compiled = packet(
        &project,
        &access,
        &frozen,
        "packet",
        MockContextBudget::new("100000", "100", "100"),
        None,
    );
    let payload = envelope(&compiled);
    assert_eq!(
        payload["reviewedKnowledge"]["sets"][0]["bundleId"],
        bundle.id
    );
    assert_eq!(
        payload["reviewedKnowledge"]["sets"][0]["records"][0]["attitude"],
        "believes"
    );
    assert!(
        payload["reviewedKnowledge"]["interpretation"]
            .as_str()
            .unwrap()
            .contains("world truth")
    );
    assert_eq!(
        compiled.receipt.reviewed_knowledge[0].record_ids,
        vec!["belief"]
    );
    assert!(compiled.receipt.reviewed_knowledge[0].complete_record_set);
    assert!(compiled.receipt.reviewed_knowledge_omissions.is_empty());
    let read = project
        .read_story_source(
            access,
            frozen.snapshot.snapshot_id,
            frozen.reviewed_knowledge[0].source_handle.clone(),
        )
        .unwrap();
    assert_eq!(read.passages[0].text, text);
}

#[test]
fn restricted_packet_filters_private_interpretations_and_later_learning() {
    let (_cleanup, project, access) = setup("disclosure");
    let text = "Mei studied the tower.";
    let first = chapter(&project, &access, "chapter-1", "Earlier", text);
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "She approached the door.",
    );
    let future = chapter(
        &project,
        &access,
        "chapter-3",
        "Later discovery",
        "She discovered the key had been destroyed.",
    );
    let public = knowledge("public-belief", text, EvidenceAudience::Reader);
    let mut private = knowledge("secret-record", text, EvidenceAudience::AuthorRoom);
    private.statement = "PRIVATE_KNOWLEDGE_SENTINEL".into();
    private.topic.label = "PRIVATE_TOPIC_SENTINEL".into();
    private.character.label = "PRIVATE_CHARACTER_SENTINEL".into();
    ready(&project, &access, &first, "earlier", vec![public, private]);
    ready(&project, &access, &target, "target", vec![]);
    let mut learned = knowledge(
        "future-learning",
        "She discovered the key had been destroyed.",
        EvidenceAudience::Reader,
    );
    learned.attitude = KnowledgeAttitude::Knows;
    learned.timing = PossessionTiming::Earlier;
    learned.statement = "FUTURE_LEARNING_SENTINEL".into();
    ready(&project, &access, &future, "future", vec![learned]);
    let frozen = freeze_reviewed(&project, &access, &target, "freeze", "1");
    assert_eq!(frozen.reviewed_knowledge.len(), 1);
    assert_eq!(frozen.reviewed_knowledge[0].records.len(), 2);
    let history = project
        .reviewed_knowledge_history(
            access.clone(),
            frozen.snapshot.snapshot_id.clone(),
            "character-mei".into(),
            None,
        )
        .unwrap();
    assert_eq!(history.history.observations.len(), 1);
    assert_eq!(history.history.observations[0].record_id, "public-belief");
    assert!(history.history.incomplete);
    let unknown = project
        .reviewed_knowledge_history(
            access.clone(),
            frozen.snapshot.snapshot_id.clone(),
            "character-never-observed".into(),
            None,
        )
        .unwrap();
    assert!(unknown.history.observations.is_empty());
    assert!(unknown.history.incomplete);
    assert!(
        serde_json::to_string(&unknown)
            .unwrap()
            .contains("noEligibleObservations")
    );
    let scope = whole_scope(&target);
    let compiled = packet(
        &project,
        &access,
        &frozen,
        "packet",
        MockContextBudget::new("100000", "100", "100"),
        Some(scope),
    );
    let serialized = serde_json::to_string(&compiled).unwrap();
    for secret in [
        "PRIVATE_KNOWLEDGE_SENTINEL",
        "PRIVATE_TOPIC_SENTINEL",
        "PRIVATE_CHARACTER_SENTINEL",
        "secret-record",
        "FUTURE_LEARNING_SENTINEL",
        "future-learning",
    ] {
        assert!(
            !serialized.contains(secret),
            "restricted packet leaked {secret}"
        );
        assert!(
            !serde_json::to_string(&history).unwrap().contains(secret),
            "restricted knowledge history leaked {secret}"
        );
    }
    assert_eq!(
        compiled.receipt.reviewed_knowledge[0].record_ids,
        vec!["public-belief"]
    );
    assert_eq!(compiled.receipt.reviewed_knowledge_omissions[0].count, 1);
    assert_eq!(
        serde_json::to_value(&compiled.receipt.reviewed_knowledge_omissions[0]).unwrap()["reason"],
        "disclosure"
    );
}

#[test]
fn layered_packet_delivers_knowledge_when_original_chapter_does_not_fit() {
    let (_cleanup, project, access) = setup("layered");
    let quote = "Mei believed the key was in the tower.";
    let text = format!(
        "{quote} {}",
        "An older scene fills the chapter. ".repeat(1600)
    );
    let source = chapter(&project, &access, "chapter-1", "Long chapter", &text);
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "She began her search.",
    );
    ready(
        &project,
        &access,
        &source,
        "knowledge",
        vec![knowledge("belief", quote, EvidenceAudience::Reader)],
    );
    let frozen = freeze_working(&project, &access, &target, "freeze");
    let compiled = packet(
        &project,
        &access,
        &frozen,
        "packet",
        MockContextBudget::new("12000", "100", "100"),
        None,
    );
    let payload = envelope(&compiled);
    assert_eq!(
        compiled.receipt.reviewed_knowledge[0].record_ids,
        vec!["belief"]
    );
    assert_eq!(
        payload["reviewedKnowledge"]["sets"][0]["records"][0]["evidence"]["quote"],
        quote
    );
    assert!(
        !serde_json::to_string(&compiled.messages)
            .unwrap()
            .contains(&"An older scene fills the chapter. ".repeat(3))
    );
    assert!(
        compiled
            .receipt
            .omissions
            .iter()
            .any(|omission| omission.contains(&frozen.reviewed_knowledge[0].source_handle))
    );
}

#[test]
fn invalid_private_knowledge_fails_before_budget_rejection_even_with_recomputed_hash() {
    let (_cleanup, project, access) = setup("invalid");
    let source = chapter(
        &project,
        &access,
        "chapter-1",
        "Earlier",
        "Mei studied the tower.",
    );
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "The search began.",
    );
    ready(
        &project,
        &access,
        &source,
        "knowledge",
        vec![knowledge(
            "private-belief",
            "Mei studied the tower.",
            EvidenceAudience::AuthorRoom,
        )],
    );
    let mut frozen = freeze_reviewed(&project, &access, &target, "freeze", "1");
    let reads = frozen
        .snapshot
        .sources
        .iter()
        .map(|s| {
            project
                .read_story_source(
                    access.clone(),
                    frozen.snapshot.snapshot_id.clone(),
                    s.handle.clone(),
                )
                .unwrap()
        })
        .collect();
    let set = &mut frozen.reviewed_knowledge[0];
    set.records[0].evidence.quote = "A forged source passage.".into();
    set.records[0].evidence.quote_hash = hash_json(&set.records[0].evidence.quote);
    set.records_hash =
        webnovel_core::context::reviewed_knowledge::records_hash(&set.records).unwrap();
    let request = PacketRequest {
        packet_id: "packet".into(),
        session_id: "session".into(),
        invocation_ordinal: "1".into(),
        frozen,
        instruction: "Continue the scene.".into(),
        sources: reads,
        mandatory_handles: vec![],
        scope: Some(whole_scope(&target)),
        safe_brief: None,
        budget: MockContextBudget::new("1", "1", "1"),
        provider_binding: None,
        response_contract: None,
        workshop_metadata: None,
        lookup: None,
    };
    let error = compile_packet(&request).unwrap_err();
    assert!(
        serde_json::to_string(&error)
            .unwrap()
            .contains("InvalidReviewedKnowledge"),
        "wrong validation order: {error:?}"
    );
}

#[test]
fn frozen_knowledge_remains_exact_after_source_edit_but_is_not_current() {
    let (_cleanup, project, access) = setup("historical");
    let text = "Mei believed the key was in the tower.";
    let source = chapter(&project, &access, "chapter-1", "Earlier", text);
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "The search began.",
    );
    ready(
        &project,
        &access,
        &source,
        "knowledge",
        vec![knowledge("belief", text, EvidenceAudience::Reader)],
    );
    let frozen = freeze_working(&project, &access, &target, "freeze");
    let packet_before = packet(
        &project,
        &access,
        &frozen,
        "packet",
        MockContextBudget::new("100000", "100", "100"),
        None,
    );
    project
        .documents().save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit".into(),
            expected: source.head.clone(),
            local_generation: "1".into(),
            body: body("Mei knew nothing about the key."),
            cause: SaveCause::Typing,
        })
        .unwrap();
    assert!(
        !project
            .story_snapshot_is_current(access.clone(), frozen.snapshot.snapshot_id.clone())
            .unwrap()
    );
    let historical = project
        .story_snapshot(access.clone(), frozen.snapshot.snapshot_id.clone())
        .unwrap();
    assert_eq!(historical.reviewed_knowledge, frozen.reviewed_knowledge);
    assert_eq!(
        envelope(&packet_before)["reviewedKnowledge"]["sets"][0]["records"][0]["evidence"]["quote"],
        text
    );
    let new = freeze_working(&project, &access, &target, "freeze-after-edit");
    assert!(new.reviewed_knowledge.is_empty());
}

#[test]
fn rehashed_frozen_interpretation_cannot_impersonate_the_immutable_review_bundle() {
    let (_cleanup, project, access) = setup("tampered-snapshot");
    let source = chapter(
        &project,
        &access,
        "chapter-1",
        "Earlier",
        "Mei studied the tower.",
    );
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "The search began.",
    );
    ready(
        &project,
        &access,
        &source,
        "knowledge",
        vec![knowledge(
            "belief",
            "Mei studied the tower.",
            EvidenceAudience::Reader,
        )],
    );
    let mut frozen = freeze_working(&project, &access, &target, "freeze");
    frozen.reviewed_knowledge[0].records[0].statement =
        "The author never accepted this interpretation.".into();
    frozen.reviewed_knowledge[0].records_hash =
        webnovel_core::context::reviewed_knowledge::records_hash(
            &frozen.reviewed_knowledge[0].records,
        )
        .unwrap();
    let json = serde_json::to_string(&frozen).unwrap();
    let db = Connection::open(project.path.join("project.sqlite3")).unwrap();
    db.execute_batch("DROP TRIGGER immutable_story_snapshot_update;")
        .unwrap();
    db.execute(
        "UPDATE story_snapshots SET manifest_json=?,manifest_hash=? WHERE id=?",
        params![json, hash_json(&json), frozen.snapshot.snapshot_id],
    )
    .unwrap();
    let result = project.story_snapshot(access, frozen.snapshot.snapshot_id);
    assert!(
        result.is_err(),
        "a self-consistent sidecar cannot replace accepted review evidence"
    );
}

#[test]
fn legacy_absence_roundtrips_without_adding_knowledge_fields() {
    let (_cleanup, project, access) = setup("legacy");
    let target = chapter(
        &project,
        &access,
        "chapter-1",
        "Current",
        "An unreviewed scene.",
    );
    let frozen = freeze_working(&project, &access, &target, "freeze");
    let original = serde_json::to_string(&frozen).unwrap();
    assert!(!original.contains("reviewedKnowledge"));
    let decoded: FrozenContext = serde_json::from_str(&original).unwrap();
    assert_eq!(serde_json::to_string(&decoded).unwrap(), original);
    let compiled = packet(
        &project,
        &access,
        &decoded,
        "packet",
        MockContextBudget::new("100000", "100", "100"),
        None,
    );
    assert!(
        !serde_json::to_string(&compiled)
            .unwrap()
            .contains("reviewedKnowledge")
    );
}

#[test]
fn foreign_sidecar_namespace_and_duplicate_sources_are_rejected_after_rehashing() {
    let (_cleanup, project, access) = setup("namespace");
    let source = chapter(
        &project,
        &access,
        "chapter-1",
        "Earlier",
        "Mei studied the tower.",
    );
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "The search began.",
    );
    ready(
        &project,
        &access,
        &source,
        "knowledge",
        vec![knowledge(
            "belief",
            "Mei studied the tower.",
            EvidenceAudience::Reader,
        )],
    );
    let original = freeze_working(&project, &access, &target, "freeze");
    let db = Connection::open(project.path.join("project.sqlite3")).unwrap();
    db.execute_batch("DROP TRIGGER immutable_story_snapshot_update;")
        .unwrap();
    for duplicate in [false, true] {
        let mut frozen = original.clone();
        if duplicate {
            frozen
                .reviewed_knowledge
                .push(frozen.reviewed_knowledge[0].clone());
        } else {
            frozen.reviewed_knowledge[0].operation_namespace = "foreign-namespace".into();
        }
        let json = serde_json::to_string(&frozen).unwrap();
        db.execute(
            "UPDATE story_snapshots SET manifest_json=?,manifest_hash=? WHERE id=?",
            params![json, hash_json(&json), frozen.snapshot.snapshot_id],
        )
        .unwrap();
        let error = project
            .story_snapshot(access.clone(), frozen.snapshot.snapshot_id)
            .unwrap_err();
        assert_eq!(error.code, "InvalidReviewedKnowledge");
    }
}
