use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use webnovel_core::context::evidence_history::{
    EvidenceHistoryUncertainty, query_evidence_history,
};
use webnovel_core::context::reviewed_evidence::{ReviewedEvidenceSet, records_hash};
use webnovel_core::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    ReviewedBasisManifest, ReviewedBasisMember, SourceDescriptor, SourceKind, SourceRef,
    StorySnapshot,
};
use webnovel_core::projects::story_context::FrozenContext;
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionRecord, PossessionTiming, StoryEntityRef,
};
use webnovel_core::validate_snapshot_json;

const PROJECT: &str = "evidence-history-project";
const NAMESPACE: &str = "evidence-history-namespace";

fn source(
    handle: &str,
    document_id: &str,
    kind: SourceKind,
    reader_position: &str,
) -> SourceDescriptor {
    let body = json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "attrs": {"id": format!("{document_id}-block")},
                "content": [{"type": "text", "text": format!("Source {document_id}.")}]
            }]
        }
    });
    let hash = validate_snapshot_json(&serde_json::to_string(&body).unwrap())
        .unwrap()
        .hash;
    SourceDescriptor {
        handle: handle.into(),
        source: SourceRef {
            project_id: PROJECT.into(),
            document_id: document_id.into(),
            revision_id: format!("revision-{document_id}"),
            body_hash: hash,
        },
        display_name: format!("Chapter {document_id}"),
        kind,
        current: true,
        coverage: CoverageLabel::Verbatim,
        disclosure: Disclosure {
            reader_position: Some(reader_position.into()),
            visible_to_characters: Vec::new(),
            author_only: false,
            future_private: false,
        },
        story_time: None,
        dependencies: Vec::new(),
    }
}

fn quote_hash(quote: &str) -> String {
    Sha256::digest(quote.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn record(
    id: &str,
    object_id: &str,
    object_label: &str,
    holder: Option<(&str, &str)>,
    timing: PossessionTiming,
    audience: EvidenceAudience,
    quote: &str,
) -> PossessionRecord {
    PossessionRecord {
        id: id.into(),
        object: StoryEntityRef {
            id: object_id.into(),
            label: object_label.into(),
        },
        holder: holder.map(|(id, label)| StoryEntityRef {
            id: id.into(),
            label: label.into(),
        }),
        timing,
        audience,
        evidence: EvidenceAnchor {
            block_id: format!("evidence-{id}"),
            from_utf16: 0,
            to_utf16: quote.encode_utf16().count() as u32,
            quote: quote.into(),
            quote_hash: quote_hash(quote),
        },
    }
}

fn set(
    source: &SourceDescriptor,
    bundle_id: &str,
    records: Vec<PossessionRecord>,
) -> ReviewedEvidenceSet {
    ReviewedEvidenceSet {
        project_id: PROJECT.into(),
        operation_namespace: NAMESPACE.into(),
        bundle_id: bundle_id.into(),
        records_hash: records_hash(&records).unwrap(),
        source_handle: source.handle.clone(),
        source: source.source.clone(),
        records,
    }
}

fn author_policy() -> InformationPolicy {
    InformationPolicy {
        version: "1".into(),
        audience: Audience::AuthorRoom,
        reader_frontier: None,
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    }
}

fn restricted_policy() -> InformationPolicy {
    InformationPolicy {
        version: "1".into(),
        audience: Audience::RestrictedWriting,
        reader_frontier: Some("2".into()),
        character_id: None,
        character_grants: Vec::new(),
        allow_alternatives: false,
        allow_historical: false,
    }
}

fn working_frozen(
    sources: Vec<SourceDescriptor>,
    evidence: Vec<ReviewedEvidenceSet>,
    purpose: ContextPurpose,
) -> FrozenContext {
    let target = sources[0].source.clone();
    FrozenContext {
        snapshot: StorySnapshot {
            snapshot_id: "snapshot-evidence-history".into(),
            project_id: PROJECT.into(),
            basis: BasisKind::Working,
            target,
            context_source_epoch: "1".into(),
            ordering_epoch: "1".into(),
            disclosure_policy_version: "1".into(),
            sources,
            reviewed_basis: None,
        },
        policy: author_policy(),
        purpose,
        aliases: BTreeMap::new(),
        excluded_source_count: 0,
        guidance: Vec::new(),
        conversation: None,
        navigation_views: Vec::new(),
        reviewed_evidence: evidence,
        reviewed_promises: Vec::new(),
    }
}

fn restricted_frozen(
    target: SourceDescriptor,
    evidence_source: SourceDescriptor,
    evidence: Vec<ReviewedEvidenceSet>,
) -> FrozenContext {
    let bundle_id = evidence[0].bundle_id.clone();
    FrozenContext {
        snapshot: StorySnapshot {
            snapshot_id: "snapshot-restricted-evidence-history".into(),
            project_id: PROJECT.into(),
            basis: BasisKind::Reviewed,
            target: target.source.clone(),
            context_source_epoch: "1".into(),
            ordering_epoch: "1".into(),
            disclosure_policy_version: "1".into(),
            sources: vec![target, evidence_source.clone()],
            reviewed_basis: Some(ReviewedBasisManifest {
                project_id: PROJECT.into(),
                operation_namespace: NAMESPACE.into(),
                prefix: vec![ReviewedBasisMember {
                    document_id: evidence_source.source.document_id.clone(),
                    bundle_id,
                    revision_id: evidence_source.source.revision_id.clone(),
                    version: "1".into(),
                    body_hash: evidence_source.source.body_hash.clone(),
                }],
            }),
        },
        policy: restricted_policy(),
        purpose: ContextPurpose::Continue,
        aliases: BTreeMap::new(),
        excluded_source_count: 0,
        guidance: Vec::new(),
        conversation: None,
        navigation_views: Vec::new(),
        reviewed_evidence: evidence,
        reviewed_promises: Vec::new(),
    }
}

#[test]
fn observations_follow_snapshot_source_order_and_keep_exact_refs() {
    let target = source("target", "target", SourceKind::CurrentDraft, "3");
    let earlier = source("earlier", "earlier", SourceKind::CurrentDraft, "1");
    let later = source("later", "later", SourceKind::CurrentDraft, "2");
    let earlier_set = set(
        &earlier,
        "bundle-earlier",
        vec![record(
            "record-earlier",
            "key",
            "Ancient Key",
            Some(("mei", "Mei")),
            PossessionTiming::Earlier,
            EvidenceAudience::AuthorRoom,
            "Mei held it earlier.",
        )],
    );
    let later_set = set(
        &later,
        "bundle-later",
        vec![record(
            "record-later",
            "key",
            "Key",
            Some(("ren", "Ren")),
            PossessionTiming::AtPassage,
            EvidenceAudience::AuthorRoom,
            "Ren holds the key.",
        )],
    );
    // The frozen source manifest, rather than sidecar insertion order, is the
    // stable order exposed to callers.
    let frozen = working_frozen(
        vec![target, earlier.clone(), later.clone()],
        vec![later_set, earlier_set],
        ContextPurpose::Discuss,
    );
    let history = query_evidence_history(&frozen, "key").unwrap();
    assert_eq!(history.label_variants, vec!["Ancient Key", "Key"]);
    assert_eq!(
        history
            .observations
            .iter()
            .map(|observation| observation.source_handle.as_str())
            .collect::<Vec<_>>(),
        vec!["earlier", "later"]
    );
    assert_eq!(history.observations[0].record_id, "record-earlier");
    assert_eq!(history.observations[0].source, earlier.source);
    assert_eq!(
        history.observations[0].evidence.quote,
        "Mei held it earlier."
    );
    assert_eq!(history.observations[0].holder.as_ref().unwrap().id, "mei");
    assert_eq!(history.observations[1].evidence.quote, "Ren holds the key.");
    assert!(
        history
            .uncertainty
            .contains(&EvidenceHistoryUncertainty::EarlierTiming)
    );
    assert!(
        history
            .uncertainty
            .contains(&EvidenceHistoryUncertainty::DifferingHolders)
    );
    assert!(history.incomplete);
}

#[test]
fn equal_reader_positions_use_document_id_before_manifest_order() {
    let target = source("target", "target", SourceKind::CurrentDraft, "5");
    let chapter_b = source("b", "chapter-b", SourceKind::CurrentDraft, "4");
    let chapter_a = source("a", "chapter-a", SourceKind::CurrentDraft, "4");
    let set_b = set(
        &chapter_b,
        "bundle-b",
        vec![record(
            "record-b",
            "key",
            "Key",
            None,
            PossessionTiming::AtPassage,
            EvidenceAudience::AuthorRoom,
            "Chapter B.",
        )],
    );
    let set_a = set(
        &chapter_a,
        "bundle-a",
        vec![record(
            "record-a",
            "key",
            "Key",
            None,
            PossessionTiming::AtPassage,
            EvidenceAudience::AuthorRoom,
            "Chapter A.",
        )],
    );
    let frozen = working_frozen(
        vec![target, chapter_b, chapter_a],
        vec![set_b, set_a],
        ContextPurpose::StoryQuestion,
    );
    let history = query_evidence_history(&frozen, "key").unwrap();
    assert_eq!(
        history
            .observations
            .iter()
            .map(|observation| observation.source.document_id.as_str())
            .collect::<Vec<_>>(),
        vec!["chapter-a", "chapter-b"]
    );
}

#[test]
fn restricted_query_filters_private_records_before_matching_or_serializing() {
    let target = source("target", "target", SourceKind::CurrentDraft, "3");
    let evidence_source = source("evidence", "chapter-1", SourceKind::ReviewedAuthority, "1");
    let records = vec![
        record(
            "private-record",
            "private-key",
            "Private Key Label",
            Some(("secret-holder", "Secret Holder")),
            PossessionTiming::AtPassage,
            EvidenceAudience::AuthorRoom,
            "The private record.",
        ),
        record(
            "reader-record",
            "key",
            "Reader Key",
            Some(("mei", "Mei")),
            PossessionTiming::AtPassage,
            EvidenceAudience::Reader,
            "The reader record.",
        ),
    ];
    let frozen = restricted_frozen(
        target,
        evidence_source.clone(),
        vec![set(&evidence_source, "bundle-1", records)],
    );
    let history = query_evidence_history(&frozen, "key").unwrap();
    assert_eq!(history.observations.len(), 1);
    assert_eq!(history.observations[0].record_id, "reader-record");
    assert_eq!(history.label_variants, vec!["Reader Key"]);
    assert!(
        history
            .uncertainty
            .contains(&EvidenceHistoryUncertainty::DisclosureLimited)
    );
    assert!(history.incomplete);
    let encoded = serde_json::to_string(&history).unwrap();
    assert!(!encoded.contains("private-key"));
    assert!(!encoded.contains("Private Key Label"));
    assert!(!encoded.contains("private-record"));
}

#[test]
fn same_name_entities_are_kept_separate_by_explicit_id() {
    let target = source("target", "target", SourceKind::CurrentDraft, "2");
    let evidence_source = source("evidence", "chapter-1", SourceKind::CurrentDraft, "1");
    let frozen = working_frozen(
        vec![target, evidence_source.clone()],
        vec![set(
            &evidence_source,
            "bundle-1",
            vec![
                record(
                    "record-a",
                    "key-a",
                    "Key",
                    Some(("mei", "Mei")),
                    PossessionTiming::AtPassage,
                    EvidenceAudience::AuthorRoom,
                    "A key.",
                ),
                record(
                    "record-b",
                    "key-b",
                    "Key",
                    Some(("ren", "Ren")),
                    PossessionTiming::AtPassage,
                    EvidenceAudience::AuthorRoom,
                    "Another key.",
                ),
            ],
        )],
        ContextPurpose::StoryQuestion,
    );
    let first = query_evidence_history(&frozen, "key-a").unwrap();
    assert_eq!(first.observations.len(), 1);
    assert_eq!(first.observations[0].record_id, "record-a");
    assert_eq!(first.label_variants, vec!["Key"]);
    let absent = query_evidence_history(&frozen, "key-missing").unwrap();
    assert!(absent.observations.is_empty());
    assert!(absent.label_variants.is_empty());
    assert!(absent.uncertainty.is_empty());
}

#[test]
fn unknown_holder_and_timing_are_separate_from_earlier_timing() {
    let target = source("target", "target", SourceKind::CurrentDraft, "3");
    let evidence_source = source("evidence", "chapter-1", SourceKind::CurrentDraft, "1");
    let frozen = working_frozen(
        vec![target, evidence_source.clone()],
        vec![set(
            &evidence_source,
            "bundle-1",
            vec![
                record(
                    "unknown",
                    "amulet",
                    "Amulet",
                    None,
                    PossessionTiming::Unknown,
                    EvidenceAudience::AuthorRoom,
                    "The amulet appears.",
                ),
                record(
                    "earlier",
                    "amulet",
                    "Amulet",
                    Some(("mei", "Mei")),
                    PossessionTiming::Earlier,
                    EvidenceAudience::AuthorRoom,
                    "The amulet was held before.",
                ),
            ],
        )],
        ContextPurpose::Discuss,
    );
    let history = query_evidence_history(&frozen, "amulet").unwrap();
    assert!(
        history
            .uncertainty
            .contains(&EvidenceHistoryUncertainty::UnknownHolder)
    );
    assert!(
        history
            .uncertainty
            .contains(&EvidenceHistoryUncertainty::UnknownTiming)
    );
    assert!(
        history
            .uncertainty
            .contains(&EvidenceHistoryUncertainty::EarlierTiming)
    );
    assert!(
        !history
            .uncertainty
            .contains(&EvidenceHistoryUncertainty::DifferingHolders)
    );
    assert_eq!(history.observations[0].holder, None);
    assert_eq!(history.observations[0].timing, PossessionTiming::Unknown);
}

#[test]
fn invalid_or_unsupported_frozen_sets_fail_closed() {
    let target = source("target", "target", SourceKind::CurrentDraft, "2");
    let evidence_source = source("evidence", "chapter-1", SourceKind::CurrentDraft, "1");
    let evidence = set(
        &evidence_source,
        "bundle-1",
        vec![record(
            "record-1",
            "key",
            "Key",
            None,
            PossessionTiming::Unknown,
            EvidenceAudience::AuthorRoom,
            "A key.",
        )],
    );
    let mut tampered = working_frozen(
        vec![target.clone(), evidence_source.clone()],
        vec![evidence.clone()],
        ContextPurpose::Discuss,
    );
    tampered.reviewed_evidence[0].records_hash = "0".repeat(64);
    assert!(query_evidence_history(&tampered, "key").is_err());

    let unsupported = working_frozen(
        vec![target, evidence_source],
        vec![evidence],
        ContextPurpose::MemoryAnalysis,
    );
    assert!(query_evidence_history(&unsupported, "key").is_err());
}
