use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use webnovel_core::context::packet::{MockContextBudget, PacketRequest, compile_packet};
use webnovel_core::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    ReviewedBasisManifest, ReviewedBasisMember, ReviewedEvidenceSet, SourceDescriptor, SourceKind,
    SourceRef, StorySnapshot,
};
use webnovel_core::documents::capture_append_scope;
use webnovel_core::projects::story_context::{FrozenContext, SourcePassage, SourceRead};
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionRecord, PossessionTiming, StoryEntityRef,
};
use webnovel_core::validate_snapshot_json;

const PROJECT: &str = "evidence-packet-project";

fn body(blocks: &[(&str, &str)]) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": blocks.iter().map(|(id, text)| json!({
                "type": "paragraph",
                "attrs": {"id": id},
                "content": [{"type": "text", "text": text}]
            })).collect::<Vec<_>>()
        }
    })
}

fn source(
    handle: &str,
    document_id: &str,
    value: &Value,
    kind: SourceKind,
    reader_position: &str,
) -> SourceDescriptor {
    let hash = validate_snapshot_json(&serde_json::to_string(value).unwrap())
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
        display_name: document_id.into(),
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

fn read(descriptor: &SourceDescriptor, value: &Value) -> SourceRead {
    let canonical = validate_snapshot_json(&serde_json::to_string(value).unwrap())
        .unwrap()
        .snapshot;
    let passages = canonical["body"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(order, block)| SourcePassage {
            handle: descriptor.handle.clone(),
            source: descriptor.source.clone(),
            block_id: block["attrs"]["id"].as_str().unwrap().into(),
            block_order: order as u32,
            text: block["content"][0]["text"].as_str().unwrap().into(),
        })
        .collect();
    SourceRead {
        descriptor: descriptor.clone(),
        passages,
        body: canonical,
        used_validated_projection: true,
    }
}

fn record(id: &str, audience: EvidenceAudience, quote: &str) -> PossessionRecord {
    let quote_hash = Sha256::digest(quote.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    PossessionRecord {
        id: id.into(),
        object: StoryEntityRef {
            id: "key".into(),
            label: "Key".into(),
        },
        holder: Some(StoryEntityRef {
            id: "mei".into(),
            label: "Mei".into(),
        }),
        timing: PossessionTiming::AtPassage,
        audience,
        evidence: EvidenceAnchor {
            block_id: "evidence-block".into(),
            from_utf16: 0,
            to_utf16: quote.encode_utf16().count() as u32,
            quote: quote.into(),
            quote_hash,
        },
    }
}

fn set(
    source: &SourceDescriptor,
    bundle_id: &str,
    records: Vec<PossessionRecord>,
) -> ReviewedEvidenceSet {
    let records_hash = webnovel_core::context::reviewed_evidence::records_hash(&records).unwrap();
    ReviewedEvidenceSet {
        project_id: PROJECT.into(),
        operation_namespace: "namespace".into(),
        bundle_id: bundle_id.into(),
        records_hash,
        source_handle: source.handle.clone(),
        source: source.source.clone(),
        records,
    }
}

fn request(
    snapshot: StorySnapshot,
    policy: InformationPolicy,
    purpose: ContextPurpose,
    evidence: Vec<ReviewedEvidenceSet>,
    reads: Vec<SourceRead>,
) -> PacketRequest {
    PacketRequest {
        packet_id: "packet-evidence".into(),
        session_id: "session-evidence".into(),
        invocation_ordinal: "1".into(),
        frozen: FrozenContext {
            snapshot,
            policy,
            purpose,
            aliases: BTreeMap::new(),
            excluded_source_count: 0,
            guidance: Vec::new(),
            conversation: None,
            navigation_views: Vec::new(),
            reviewed_evidence: evidence,
        },
        instruction: "Answer from reviewed evidence and exact prose.".into(),
        sources: reads,
        mandatory_handles: Vec::new(),
        scope: None,
        safe_brief: None,
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
        response_contract: None,
    }
}

#[test]
fn author_room_delivers_complete_record_set_with_bundle_identity() {
    let target_body = body(&[("target", "Ending.")]);
    let evidence_body = body(&[("evidence-block", "Mei holds the key.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let evidence = source(
        "evidence",
        "chapter-evidence",
        &evidence_body,
        SourceKind::CurrentDraft,
        "0",
    );
    let records = vec![record(
        "r1",
        EvidenceAudience::AuthorRoom,
        "Mei holds the key.",
    )];
    let evidence_set = set(&evidence, "bundle-1", records);
    let snapshot = StorySnapshot {
        snapshot_id: "snapshot-evidence".into(),
        project_id: PROJECT.into(),
        basis: BasisKind::Working,
        target: target.source.clone(),
        context_source_epoch: "1".into(),
        ordering_epoch: "1".into(),
        disclosure_policy_version: "1".into(),
        sources: vec![target.clone(), evidence.clone()],
        reviewed_basis: None,
    };
    let packet = compile_packet(&request(
        snapshot,
        InformationPolicy {
            version: "1".into(),
            audience: Audience::AuthorRoom,
            reader_frontier: None,
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        },
        ContextPurpose::Discuss,
        vec![evidence_set],
        vec![read(&target, &target_body), read(&evidence, &evidence_body)],
    ))
    .unwrap();
    assert_eq!(packet.receipt.reviewed_evidence.len(), 1);
    assert!(packet.receipt.reviewed_evidence[0].complete_record_set);
    assert_eq!(
        packet.receipt.reviewed_evidence[0].projection_hash,
        packet.receipt.reviewed_evidence[0].records_hash
    );
    assert_eq!(packet.receipt.reviewed_evidence[0].record_ids, vec!["r1"]);
    assert!(packet.receipt.reviewed_evidence_omissions.is_empty());
}

#[test]
fn tampered_quote_with_recomputed_set_hash_is_refused_before_budget() {
    let target_body = body(&[("target", "Ending.")]);
    let evidence_body = body(&[("evidence-block", "Mei holds the key.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let evidence = source(
        "evidence",
        "chapter-evidence",
        &evidence_body,
        SourceKind::CurrentDraft,
        "0",
    );
    let mut tampered = record("r1", EvidenceAudience::AuthorRoom, "Mei holds the key.");
    tampered.evidence.quote = "A different claim.".into();
    let evidence_set = set(&evidence, "bundle-1", vec![tampered]);
    let snapshot = StorySnapshot {
        snapshot_id: "snapshot-evidence".into(),
        project_id: PROJECT.into(),
        basis: BasisKind::Working,
        target: target.source.clone(),
        context_source_epoch: "1".into(),
        ordering_epoch: "1".into(),
        disclosure_policy_version: "1".into(),
        sources: vec![target.clone(), evidence.clone()],
        reviewed_basis: None,
    };
    let error = compile_packet(&request(
        snapshot,
        InformationPolicy {
            version: "1".into(),
            audience: Audience::AuthorRoom,
            reader_frontier: None,
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        },
        ContextPurpose::Discuss,
        vec![evidence_set],
        vec![read(&target, &target_body), read(&evidence, &evidence_body)],
    ))
    .unwrap_err();
    assert!(error.to_string().contains("quotation"));
}

#[test]
fn restricted_reviewed_packet_filters_private_records_without_rewriting_hash() {
    let target_body = body(&[("target", "Ending.")]);
    let evidence_body = body(&[("evidence-block", "Mei holds the key.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let evidence = source(
        "reviewed-evidence",
        "chapter-evidence",
        &evidence_body,
        SourceKind::ReviewedAuthority,
        "0",
    );
    let mut private = record(
        "private",
        EvidenceAudience::AuthorRoom,
        "Mei holds the key.",
    );
    private.object.label = "Hidden heirloom".into();
    private.holder.as_mut().unwrap().label = "Unrevealed identity".into();
    let records = vec![
        private,
        record("reader", EvidenceAudience::Reader, "Mei holds the key."),
    ];
    let evidence_set = set(&evidence, "bundle-1", records);
    let snapshot = StorySnapshot {
        snapshot_id: "snapshot-reviewed-evidence".into(),
        project_id: PROJECT.into(),
        basis: BasisKind::Reviewed,
        target: target.source.clone(),
        context_source_epoch: "1".into(),
        ordering_epoch: "1".into(),
        disclosure_policy_version: "1".into(),
        sources: vec![target.clone(), evidence.clone()],
        reviewed_basis: Some(ReviewedBasisManifest {
            project_id: PROJECT.into(),
            operation_namespace: "namespace".into(),
            prefix: vec![ReviewedBasisMember {
                document_id: evidence.source.document_id.clone(),
                bundle_id: "bundle-1".into(),
                revision_id: evidence.source.revision_id.clone(),
                version: "1".into(),
                body_hash: evidence.source.body_hash.clone(),
            }],
        }),
    };
    let mut request = request(
        snapshot,
        InformationPolicy {
            version: "1".into(),
            audience: Audience::RestrictedWriting,
            reader_frontier: Some("1".into()),
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        },
        ContextPurpose::Continue,
        vec![evidence_set],
        vec![read(&target, &target_body), read(&evidence, &evidence_body)],
    );
    request.scope = Some(capture_append_scope(&request.sources[0].body).unwrap());
    let packet = compile_packet(&request).unwrap();
    let coverage = &packet.receipt.reviewed_evidence[0];
    assert!(!coverage.complete_record_set);
    assert_eq!(coverage.record_ids, vec!["reader"]);
    assert_eq!(packet.receipt.reviewed_evidence_omissions.len(), 1);
    assert_eq!(packet.receipt.reviewed_evidence_omissions[0].count, 1);
    assert_ne!(coverage.projection_hash, coverage.records_hash);
    let serialized = serde_json::to_string(&packet).unwrap();
    assert!(!serialized.contains("private"));
    assert!(!serialized.contains("Hidden heirloom"));
    assert!(!serialized.contains("Unrevealed identity"));
    assert_eq!(
        packet.receipt.reviewed_evidence_omissions[0].reason,
        webnovel_core::context::ReviewedEvidenceOmissionReason::Disclosure
    );
}

#[test]
fn reviewed_evidence_budget_uses_stable_prefix_without_duplicate_records() {
    let target_body = body(&[("target", "Ending.")]);
    let evidence_body = body(&[("evidence-block", "Mei holds the key.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let evidence = source(
        "evidence",
        "chapter-evidence",
        &evidence_body,
        SourceKind::CurrentDraft,
        "0",
    );
    let records = (0..8)
        .map(|index| {
            record(
                &format!("record-{index}"),
                EvidenceAudience::AuthorRoom,
                "Mei holds the key.",
            )
        })
        .collect::<Vec<_>>();
    let evidence_set = set(&evidence, "bundle-1", records.clone());
    let snapshot = StorySnapshot {
        snapshot_id: "snapshot-evidence-budget".into(),
        project_id: PROJECT.into(),
        basis: BasisKind::Working,
        target: target.source.clone(),
        context_source_epoch: "1".into(),
        ordering_epoch: "1".into(),
        disclosure_policy_version: "1".into(),
        sources: vec![target.clone(), evidence.clone()],
        reviewed_basis: None,
    };
    let mut broad_request = request(
        snapshot,
        InformationPolicy {
            version: "1".into(),
            audience: Audience::AuthorRoom,
            reader_frontier: None,
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        },
        ContextPurpose::Discuss,
        vec![evidence_set],
        vec![read(&target, &target_body), read(&evidence, &evidence_body)],
    );
    let broad = compile_packet(&broad_request).unwrap();
    let broad_ids = broad.receipt.reviewed_evidence[0].record_ids.clone();
    assert_eq!(broad_ids.len(), records.len());

    let broad_tokens = broad.receipt.input_tokens.parse::<usize>().unwrap();
    broad_request.budget = MockContextBudget::new((broad_tokens * 3 / 5).to_string(), "0", "0");
    let constrained = compile_packet(&broad_request).unwrap();
    let constrained_ids = constrained.receipt.reviewed_evidence[0].record_ids.clone();
    assert!(!constrained_ids.is_empty());
    assert!(constrained_ids.len() < broad_ids.len());
    assert_eq!(constrained_ids, broad_ids[..constrained_ids.len()].to_vec());
    assert_eq!(
        constrained.receipt.reviewed_evidence[0].projection_hash,
        webnovel_core::context::reviewed_evidence::records_hash(&records).unwrap()
    );
    let mut unique = constrained_ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), constrained_ids.len());
    assert!(
        constrained
            .receipt
            .reviewed_evidence_omissions
            .iter()
            .any(|omission| omission.reason
                == webnovel_core::context::ReviewedEvidenceOmissionReason::Budget
                && omission.count == records.len() - constrained_ids.len())
    );
    assert!(
        constrained.receipt.input_tokens.parse::<usize>().unwrap()
            <= broad_request
                .budget
                .context_window_tokens
                .parse::<usize>()
                .unwrap()
    );
}
