use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use webnovel_core::context::packet::MockContextBudget;
use webnovel_core::context::{Audience, BasisKind, ContextPurpose, InformationPolicy};
use webnovel_core::documents::{ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::context_packets::{PreparationResult, PrepareContext};
use webnovel_core::projects::reviewed_story::{MarkReady, StageAuthorReview};
use webnovel_core::projects::reviewed_summary::{SummaryAudience, SummaryChange, SummaryRevision};
use webnovel_core::projects::story_context::{
    FreezeReviewedContinuation, FreezeStory, FrozenContext,
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
    let root = std::env::temp_dir().join(format!("wns-reviewed-summary-{name}-{}", Uuid::new_v4()));
    fs::create_dir(&root).expect("create test root");
    let project = ProjectSession::create(root.join("story"), "Reviewed summary context test")
        .expect("create project");
    let access = project
        .attach(format!("reviewed-summary-{name}"))
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
        .create_document(CreateDocument {
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
    summary: Option<SummaryChange>,
) -> webnovel_core::projects::reviewed_story::ReadyBundle {
    let stage = project
        .stage_author_review(StageAuthorReview {
            knowledge: None,
            access: access.clone(),
            operation_id: format!("stage-{operation}"),
            expected: document.head.clone(),
            records: None,
            promises: None,
            summary,
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

fn tamper_bundle(
    project: &ProjectSession,
    bundle_id: &str,
    mutate: impl FnOnce(&mut Value),
    replace_hash: bool,
) {
    let connection =
        Connection::open(project.path.join("project.sqlite3")).expect("open test database");
    connection
        .execute_batch("DROP TRIGGER ready_bundles_no_update;")
        .expect("disable immutable test trigger");
    if replace_hash {
        let summary_json: String = connection
            .query_row(
                "SELECT summary_json FROM ready_bundles WHERE id=?",
                [bundle_id],
                |row| row.get(0),
            )
            .expect("read summary JSON");
        let mut summary: Value = serde_json::from_str(&summary_json).expect("decode summary JSON");
        mutate(&mut summary);
        let typed: SummaryRevision = serde_json::from_value(summary).expect("decode typed summary");
        let canonical = serde_json::to_string(&typed).expect("encode canonical summary JSON");
        connection
            .execute(
                "UPDATE ready_bundles SET summary_json=?,summary_hash=? WHERE id=?",
                params![canonical, hash_json(&canonical), bundle_id],
            )
            .expect("replace summary fixture");
    } else {
        connection
            .execute(
                "UPDATE ready_bundles SET summary_hash=? WHERE id=?",
                params!["0".repeat(64), bundle_id],
            )
            .expect("replace summary hash fixture");
    }
}

#[test]
fn working_freeze_includes_accepted_summary_as_separate_story_memory() {
    let (_cleanup, project, access) = setup("working-summary");
    let source = chapter(
        &project,
        &access,
        "chapter-1",
        "The opening",
        "The pendant changed hands at dusk.",
    );
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "The current scene",
        "The current scene begins.",
    );
    let bundle = ready(
        &project,
        &access,
        &source,
        "opening-summary",
        Some(SummaryChange::Set {
            text: "The opening establishes the pendant's transfer at dusk.".into(),
            audience: SummaryAudience::Reader,
        }),
    );

    let frozen = freeze_working(&project, &access, &target, "working-summary-freeze");
    assert_eq!(frozen.reviewed_summaries.len(), 1);
    assert_eq!(frozen.reviewed_summaries[0].bundle_id, bundle.id);
    assert_eq!(
        frozen.reviewed_summaries[0].summary.source.document_id,
        source.head.document_id
    );
    assert_eq!(
        frozen.reviewed_summaries[0].summary.dependencies,
        Vec::new()
    );
    assert_eq!(
        frozen.reviewed_summaries[0].summary.audience,
        SummaryAudience::Reader
    );
}

#[test]
fn reviewed_continuation_includes_prefix_summary_but_excludes_target_summary() {
    let (_cleanup, project, access) = setup("continuation-summary");
    let first = chapter(
        &project,
        &access,
        "chapter-1",
        "Earlier",
        "The earlier reviewed chapter.",
    );
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Target",
        "The current continuation target.",
    );
    let first_bundle = ready(
        &project,
        &access,
        &first,
        "first-summary",
        Some(SummaryChange::Set {
            text: "The earlier chapter establishes the separation.".into(),
            audience: SummaryAudience::Reader,
        }),
    );
    let target_bundle = ready(
        &project,
        &access,
        &target,
        "target-summary",
        Some(SummaryChange::Set {
            text: "The target chapter has not been written yet.".into(),
            audience: SummaryAudience::Reader,
        }),
    );

    let frozen = freeze_reviewed(&project, &access, &target, "reviewed-summary-freeze", "1");
    assert_eq!(frozen.reviewed_summaries.len(), 1);
    assert_eq!(frozen.reviewed_summaries[0].bundle_id, first_bundle.id);
    assert_ne!(frozen.reviewed_summaries[0].bundle_id, target_bundle.id);
    assert_eq!(
        frozen.reviewed_summaries[0].summary.source.document_id,
        first.head.document_id
    );
    assert!(
        frozen
            .reviewed_summaries
            .iter()
            .all(|item| item.summary.source.document_id != target.head.document_id)
    );
}

#[test]
fn layered_packet_delivers_complete_summary_without_duplicate_original_prose() {
    let (_cleanup, project, access) = setup("layered-summary");
    let source = chapter(
        &project,
        &access,
        "chapter-1",
        "A long reviewed chapter",
        &"The pendant's transfer remains the central promise. ".repeat(900),
    );
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "The current chapter.",
    );
    let bundle = ready(
        &project,
        &access,
        &source,
        "layered-summary",
        Some(SummaryChange::Set {
            text: "The chapter establishes the pendant transfer and its unresolved promise.".into(),
            audience: SummaryAudience::Reader,
        }),
    );
    let frozen = freeze_working(&project, &access, &target, "layered-summary-freeze");
    let compiled = packet(
        &project,
        &access,
        &frozen,
        "layered-summary-packet",
        MockContextBudget::new("12000", "100", "100"),
        None,
    );
    let envelope = envelope(&compiled);
    let source_handle = frozen
        .reviewed_summaries
        .iter()
        .find(|item| item.bundle_id == bundle.id)
        .expect("frozen summary")
        .source_handle
        .clone();
    assert_eq!(compiled.receipt.reviewed_summaries.len(), 1);
    assert_eq!(
        compiled.receipt.reviewed_summaries[0].source_handle,
        source_handle
    );
    assert_eq!(
        envelope["acceptedSummaries"]["coverage"],
        "reviewedAccepted"
    );
    assert_eq!(
        envelope["acceptedSummaries"]["representation"],
        "narrativeSummary"
    );
    assert_eq!(envelope["acceptedSummaries"]["completeSummary"], true);
    assert_eq!(
        envelope["acceptedSummaries"]["summaries"][0]["bundleId"],
        bundle.id
    );
    assert!(
        envelope["acceptedSummaries"]["summaries"][0]["text"]
            .as_str()
            .expect("summary text")
            .contains("pendant transfer")
    );
    assert!(
        envelope["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| source["handle"] != source_handle)
    );
    assert!(
        compiled
            .receipt
            .omissions
            .iter()
            .any(|omission| omission.contains(&format!("handle:{source_handle}")))
    );
}

#[test]
fn full_fit_delivers_original_prose_and_marks_summary_original_text_included() {
    let (_cleanup, project, access) = setup("full-summary");
    let source = chapter(
        &project,
        &access,
        "chapter-1",
        "Reviewed",
        "The short reviewed chapter.",
    );
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "The current chapter.",
    );
    ready(
        &project,
        &access,
        &source,
        "full-summary",
        Some(SummaryChange::Set {
            text: "A compact accepted account of the reviewed chapter.".into(),
            audience: SummaryAudience::Reader,
        }),
    );
    let frozen = freeze_working(&project, &access, &target, "full-summary-freeze");
    let source_handle = frozen.reviewed_summaries[0].source_handle.clone();
    let compiled = packet(
        &project,
        &access,
        &frozen,
        "full-summary-packet",
        MockContextBudget::new("100000", "100", "100"),
        None,
    );
    let envelope = envelope(&compiled);
    assert!(compiled.receipt.reviewed_summaries.is_empty());
    assert!(envelope.get("acceptedSummaries").is_none());
    assert!(
        envelope["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["handle"] == source_handle && source["body"].is_object())
    );
    assert!(compiled.receipt.reviewed_summary_omissions.iter().any(|omission| {
        omission.source_handle == source_handle
            && matches!(omission.reason, webnovel_core::context::reviewed_summaries::ReviewedSummaryOmissionReason::OriginalTextIncluded)
    }));
}

#[test]
fn restricted_continuation_excludes_private_summary_text_and_dependency_titles() {
    let (_cleanup, project, access) = setup("restricted-summary");
    let dependency = chapter(
        &project,
        &access,
        "chapter-1",
        "PRIVATE dependency title",
        "The first reviewed prose.",
    );
    let summarized = chapter(
        &project,
        &access,
        "chapter-2",
        "Summarized",
        "The second reviewed prose.",
    );
    let target = chapter(
        &project,
        &access,
        "chapter-3",
        "Target",
        "The continuation target.",
    );
    ready(&project, &access, &dependency, "dependency", None);
    ready(
        &project,
        &access,
        &summarized,
        "private-summary",
        Some(SummaryChange::Set {
            text: "PRIVATE accepted author-room summary that must not leak.".into(),
            audience: SummaryAudience::AuthorRoom,
        }),
    );
    let frozen = freeze_reviewed(&project, &access, &target, "restricted-summary-freeze", "2");
    assert!(
        frozen
            .reviewed_summaries
            .iter()
            .any(|item| item.summary.source.document_id == summarized.head.document_id)
    );
    let compiled = packet(
        &project,
        &access,
        &frozen,
        "restricted-summary-packet",
        MockContextBudget::new("100000", "100", "100"),
        Some(
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
            .expect("capture whole continuation scope"),
        ),
    );
    let envelope = envelope(&compiled);
    let serialized = serde_json::to_string(&envelope).unwrap();
    assert!(!serialized.contains("PRIVATE accepted author-room summary"));
    assert!(!serialized.contains("PRIVATE dependency title"));
    assert!(compiled.receipt.reviewed_summaries.is_empty());
    assert!(compiled.receipt.reviewed_summary_omissions.iter().any(|omission| {
        omission.source_handle == frozen.reviewed_summaries.iter().find(|item| item.summary.source.document_id == summarized.head.document_id).unwrap().source_handle
            && matches!(omission.reason, webnovel_core::context::reviewed_summaries::ReviewedSummaryOmissionReason::Disclosure)
    }));
}

#[test]
fn accepted_summary_remains_readable_in_historical_snapshot_after_source_edit() {
    let (_cleanup, project, access) = setup("historical-summary");
    let source = chapter(
        &project,
        &access,
        "chapter-1",
        "Reviewed",
        "The original reviewed prose.",
    );
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "The current prose.",
    );
    ready(
        &project,
        &access,
        &source,
        "historical-summary",
        Some(SummaryChange::Set {
            text: "The original chapter introduces the silver key.".into(),
            audience: SummaryAudience::Reader,
        }),
    );
    let frozen = freeze_working(&project, &access, &target, "historical-summary-freeze");
    let summary_text = frozen.reviewed_summaries[0].summary.text.clone();
    let source_handle = frozen.reviewed_summaries[0].source_handle.clone();
    project
        .save(SaveSnapshot {
            access: access.clone(),
            operation_id: "edit-reviewed-source".into(),
            expected: source.head.clone(),
            local_generation: "1".into(),
            body: body("The edited prose no longer uses the original wording."),
            cause: SaveCause::Typing,
        })
        .expect("edit source");
    assert!(
        !project
            .story_snapshot_is_current(access.clone(), frozen.snapshot.snapshot_id.clone())
            .expect("check historical freshness")
    );
    let historical = project
        .story_snapshot(access.clone(), frozen.snapshot.snapshot_id.clone())
        .expect("read historical snapshot");
    assert_eq!(historical.reviewed_summaries[0].summary.text, summary_text);
    let read = project
        .read_story_source(access, frozen.snapshot.snapshot_id, source_handle)
        .expect("read historical source");
    assert_eq!(read.passages[0].text, "The original reviewed prose.");
}

#[test]
fn tampered_summary_hash_is_rejected_before_context_freeze() {
    let (_cleanup, project, access) = setup("tampered-hash");
    let source = chapter(
        &project,
        &access,
        "chapter-1",
        "Reviewed",
        "The reviewed prose.",
    );
    let target = chapter(
        &project,
        &access,
        "chapter-2",
        "Current",
        "The current prose.",
    );
    let bundle = ready(
        &project,
        &access,
        &source,
        "tampered-hash",
        Some(SummaryChange::Set {
            text: "The reviewed prose is summarized here.".into(),
            audience: SummaryAudience::Reader,
        }),
    );
    tamper_bundle(&project, &bundle.id, |_| {}, false);
    let error = project
        .freeze_story(FreezeStory {
            access: access.clone(),
            operation_id: "tampered-hash-freeze".into(),
            expected: target.head,
            basis: BasisKind::Working,
            purpose: ContextPurpose::Discuss,
            policy: working_policy(&project, &access),
        })
        .expect_err("tampered summary hash must reject freeze");
    assert_eq!(error.code, "InvalidProject");
}

#[test]
fn tampered_summary_source_and_dependency_are_rejected() {
    for (name, tamper_dependency) in [("source", false), ("dependency", true)] {
        let (_cleanup, project, access) = setup(name);
        let first = chapter(
            &project,
            &access,
            "chapter-1",
            "First",
            "The first reviewed prose.",
        );
        let source = chapter(
            &project,
            &access,
            "chapter-2",
            "Reviewed",
            "The reviewed prose.",
        );
        let target = chapter(
            &project,
            &access,
            "chapter-3",
            "Current",
            "The current prose.",
        );
        ready(&project, &access, &first, "first", None);
        let bundle = ready(
            &project,
            &access,
            &source,
            "tampered-source-or-dependency",
            Some(SummaryChange::Set {
                text: "The reviewed prose is summarized here.".into(),
                audience: SummaryAudience::Reader,
            }),
        );
        tamper_bundle(
            &project,
            &bundle.id,
            |summary| {
                if tamper_dependency {
                    summary["dependencies"] = json!([]);
                } else {
                    summary["source"]["bodyHash"] = json!("f".repeat(64));
                }
            },
            true,
        );
        let error = project
            .freeze_story(FreezeStory {
                access: access.clone(),
                operation_id: format!("tampered-{name}-freeze"),
                expected: target.head,
                basis: BasisKind::Working,
                purpose: ContextPurpose::Discuss,
                policy: working_policy(&project, &access),
            })
            .expect_err("tampered summary binding must reject freeze");
        assert_eq!(
            error.code, "InvalidReviewedSummary",
            "unexpected error for {name}: {error}"
        );
    }
}
