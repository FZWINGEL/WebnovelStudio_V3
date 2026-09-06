use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use webnovel_core::context::packet::{MockContextBudget, PacketRequest, compile_packet};
use webnovel_core::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    ReviewedBasisManifest, ReviewedBasisMember, ReviewedPromiseSet, SourceDescriptor, SourceKind,
    SourceRef, StorySnapshot, query_promise_history,
};
use webnovel_core::documents::capture_append_scope;
use webnovel_core::projects::story_context::{FrozenContext, SourcePassage, SourceRead};
use webnovel_core::projects::story_records::{
    EvidenceAnchor, EvidenceAudience, PossessionTiming, PromisePhase, PromiseRecord, StoryEntityRef,
};
use webnovel_core::validate_snapshot_json;

const PROJECT: &str = "promise-context-project";

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

fn quote_hash(quote: &str) -> String {
    Sha256::digest(quote.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// Keep every independently varied evidence field visible at each fixture site.
#[allow(clippy::too_many_arguments)]
fn promise(
    id: &str,
    promise_id: &str,
    label: &str,
    phase: PromisePhase,
    timing: PossessionTiming,
    audience: EvidenceAudience,
    block_id: &str,
    quote: &str,
) -> PromiseRecord {
    PromiseRecord {
        id: id.into(),
        promise: StoryEntityRef {
            id: promise_id.into(),
            label: label.into(),
        },
        phase,
        timing,
        note: format!("{phase:?} observation"),
        audience,
        evidence: EvidenceAnchor {
            block_id: block_id.into(),
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
    records: Vec<PromiseRecord>,
) -> ReviewedPromiseSet {
    let records_hash = webnovel_core::context::reviewed_promises::records_hash(&records).unwrap();
    ReviewedPromiseSet {
        project_id: PROJECT.into(),
        operation_namespace: "namespace".into(),
        bundle_id: bundle_id.into(),
        records_hash,
        source_handle: source.handle.clone(),
        source: source.source.clone(),
        records,
    }
}

fn working_frozen(
    target: &SourceDescriptor,
    sources: &[SourceDescriptor],
    promises: Vec<ReviewedPromiseSet>,
) -> FrozenContext {
    FrozenContext {
        reviewed_knowledge: Vec::new(),
        snapshot: StorySnapshot {
            snapshot_id: "snapshot-promises".into(),
            project_id: PROJECT.into(),
            basis: BasisKind::Working,
            target: target.source.clone(),
            context_source_epoch: "1".into(),
            ordering_epoch: "1".into(),
            disclosure_policy_version: "1".into(),
            sources: sources.to_vec(),
            reviewed_basis: None,
        },
        policy: InformationPolicy {
            version: "1".into(),
            audience: Audience::AuthorRoom,
            reader_frontier: None,
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
        reviewed_summaries: Vec::new(),
        reviewed_promises: promises,
    }
}

#[test]
fn history_preserves_exact_identity_labels_and_record_order() {
    let target_body = body(&[("target", "The ending.")]);
    let setup_body = body(&[("setup", "The bell will ring again.")]);
    let payoff_body = body(&[("payoff", "The bell rang again.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "2",
    );
    let setup_source = source(
        "setup",
        "chapter-setup",
        &setup_body,
        SourceKind::CurrentDraft,
        "0",
    );
    let payoff_source = source(
        "payoff",
        "chapter-payoff",
        &payoff_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let frozen = working_frozen(
        &target,
        &[target.clone(), setup_source.clone(), payoff_source.clone()],
        vec![
            set(
                &payoff_source,
                "bundle-payoff",
                vec![
                    promise(
                        "payoff-record",
                        "promise-bell",
                        "The bell vow",
                        PromisePhase::Payoff,
                        PossessionTiming::Unknown,
                        EvidenceAudience::AuthorRoom,
                        "payoff",
                        "The bell rang again.",
                    ),
                    promise(
                        "other-record",
                        "other-promise",
                        "The bell vow",
                        PromisePhase::Setup,
                        PossessionTiming::AtPassage,
                        EvidenceAudience::AuthorRoom,
                        "payoff",
                        "The bell rang again.",
                    ),
                ],
            ),
            set(
                &setup_source,
                "bundle-setup",
                vec![promise(
                    "setup-record",
                    "promise-bell",
                    "The bell promise",
                    PromisePhase::Setup,
                    PossessionTiming::AtPassage,
                    EvidenceAudience::AuthorRoom,
                    "setup",
                    "The bell will ring again.",
                )],
            ),
        ],
    );

    let history = query_promise_history(&frozen, "promise-bell").unwrap();
    assert_eq!(
        history.label_variants,
        vec!["The bell promise", "The bell vow"]
    );
    assert_eq!(
        history
            .observations
            .iter()
            .map(|observation| observation.record_id.as_str())
            .collect::<Vec<_>>(),
        vec!["setup-record", "payoff-record"]
    );
    assert_eq!(history.observations[0].source_order, 0);
    assert_eq!(history.observations[1].source_order, 1);
    assert!(history.has_recorded_payoff);
    assert!(history.incomplete);
    assert!(
        history
            .uncertainty
            .contains(&webnovel_core::context::PromiseHistoryUncertainty::UnknownTiming)
    );
    let absent = query_promise_history(&frozen, "never-recorded").unwrap();
    assert!(absent.observations.is_empty());
    assert!(!absent.has_recorded_payoff);
    assert!(absent.incomplete);
}

#[test]
fn restricted_history_filters_private_observations_without_private_identifiers() {
    let target_body = body(&[("target", "The ending.")]);
    let source_body = body(&[("chapter", "The lantern waits.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let reviewed = source(
        "reviewed",
        "chapter-reviewed",
        &source_body,
        SourceKind::ReviewedAuthority,
        "0",
    );
    let private = promise(
        "private-record-id",
        "private-promise-id",
        "Private future promise",
        PromisePhase::Payoff,
        PossessionTiming::AtPassage,
        EvidenceAudience::AuthorRoom,
        "chapter",
        "The lantern waits.",
    );
    let reader = promise(
        "reader-record",
        "reader-promise",
        "Reader promise",
        PromisePhase::Setup,
        PossessionTiming::AtPassage,
        EvidenceAudience::Reader,
        "chapter",
        "The lantern waits.",
    );
    let set = set(&reviewed, "bundle-reviewed", vec![private, reader]);
    let frozen = FrozenContext {
        reviewed_knowledge: Vec::new(),
        snapshot: StorySnapshot {
            snapshot_id: "snapshot-reviewed".into(),
            project_id: PROJECT.into(),
            basis: BasisKind::Reviewed,
            target: target.source.clone(),
            context_source_epoch: "1".into(),
            ordering_epoch: "1".into(),
            disclosure_policy_version: "1".into(),
            sources: vec![target.clone(), reviewed.clone()],
            reviewed_basis: Some(ReviewedBasisManifest {
                project_id: PROJECT.into(),
                operation_namespace: "namespace".into(),
                prefix: vec![ReviewedBasisMember {
                    document_id: reviewed.source.document_id.clone(),
                    bundle_id: "bundle-reviewed".into(),
                    revision_id: reviewed.source.revision_id.clone(),
                    version: "1".into(),
                    body_hash: reviewed.source.body_hash.clone(),
                }],
            }),
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
        purpose: ContextPurpose::Continue,
        aliases: BTreeMap::new(),
        excluded_source_count: 1,
        guidance: Vec::new(),
        conversation: None,
        navigation_views: Vec::new(),
        reviewed_evidence: Vec::new(),
        reviewed_summaries: Vec::new(),
        reviewed_promises: vec![set],
    };
    let history = query_promise_history(&frozen, "reader-promise").unwrap();
    assert_eq!(history.observations.len(), 1);
    assert_eq!(history.observations[0].record_id, "reader-record");
    assert!(!history.has_recorded_payoff);
    assert!(
        history
            .uncertainty
            .contains(&webnovel_core::context::PromiseHistoryUncertainty::DisclosureLimited)
    );
    assert!(
        history
            .uncertainty
            .contains(&webnovel_core::context::PromiseHistoryUncertainty::ExcludedSources)
    );
    let serialized = serde_json::to_string(&history).unwrap();
    assert!(!serialized.contains("private-record-id"));
    assert!(!serialized.contains("private-promise-id"));
    assert!(!serialized.contains("Private future promise"));
}

#[test]
fn malformed_unrelated_set_is_rejected_before_query_result() {
    let target_body = body(&[("target", "The ending.")]);
    let source_body = body(&[("source", "A promise.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let source = source(
        "source",
        "chapter-source",
        &source_body,
        SourceKind::CurrentDraft,
        "0",
    );
    let valid = set(
        &source,
        "bundle-valid",
        vec![promise(
            "record-valid",
            "requested",
            "Requested",
            PromisePhase::Setup,
            PossessionTiming::AtPassage,
            EvidenceAudience::AuthorRoom,
            "source",
            "A promise.",
        )],
    );
    let mut malformed = set(
        &source,
        "bundle-malformed",
        vec![promise(
            "record-malformed",
            "unrelated",
            "Unrelated",
            PromisePhase::Unclear,
            PossessionTiming::AtPassage,
            EvidenceAudience::AuthorRoom,
            "source",
            "A promise.",
        )],
    );
    malformed.records_hash = "0".repeat(64);
    let frozen = working_frozen(&target, &[target.clone(), source], vec![valid, malformed]);
    let error = query_promise_history(&frozen, "requested").unwrap_err();
    assert_eq!(error.code, "InvalidReviewedPromises");
}

#[test]
fn promise_absence_keeps_legacy_packet_and_context_bytes() {
    let target_body = body(&[("target", "The ending.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "0",
    );
    let frozen = working_frozen(&target, std::slice::from_ref(&target), Vec::new());
    let serialized_context = serde_json::to_string(&frozen).unwrap();
    assert!(!serialized_context.contains("reviewedPromises"));
    let request = PacketRequest {
        lookup: None,
        packet_id: "packet-legacy".into(),
        session_id: "session-legacy".into(),
        invocation_ordinal: "1".into(),
        frozen,
        instruction: "Discuss the ending.".into(),
        sources: vec![read(&target, &target_body)],
        mandatory_handles: Vec::new(),
        scope: None,
        safe_brief: None,
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
        response_contract: None,
    };
    let packet = compile_packet(&request).unwrap();
    let serialized_packet = serde_json::to_string(&packet).unwrap();
    assert!(!serialized_packet.contains("reviewedPromises"));
    assert!(!serialized_packet.contains("reviewedPromiseOmissions"));
}

#[test]
fn author_packet_delivers_promise_envelope_and_receipt_identity() {
    let target_body = body(&[("target", "The ending.")]);
    let promise_body = body(&[("promise", "The lantern waits.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let promise_source = source(
        "promise",
        "chapter-promise",
        &promise_body,
        SourceKind::CurrentDraft,
        "0",
    );
    let promise_set = set(
        &promise_source,
        "bundle-promise",
        vec![promise(
            "promise-record",
            "lantern-promise",
            "The lantern promise",
            PromisePhase::Setup,
            PossessionTiming::AtPassage,
            EvidenceAudience::AuthorRoom,
            "promise",
            "The lantern waits.",
        )],
    );
    let frozen = working_frozen(
        &target,
        &[target.clone(), promise_source.clone()],
        vec![promise_set],
    );
    let request = PacketRequest {
        lookup: None,
        packet_id: "packet-promises".into(),
        session_id: "session-promises".into(),
        invocation_ordinal: "1".into(),
        frozen,
        instruction: "Discuss the promise.".into(),
        sources: vec![
            read(&target, &target_body),
            read(&promise_source, &promise_body),
        ],
        mandatory_handles: Vec::new(),
        scope: None,
        safe_brief: None,
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
        response_contract: None,
    };
    let packet = compile_packet(&request).unwrap();
    assert_eq!(packet.receipt.reviewed_promises.len(), 1);
    assert!(packet.receipt.reviewed_promises[0].complete_record_set);
    assert_eq!(
        packet.receipt.reviewed_promises[0].record_ids,
        vec!["promise-record"]
    );
    assert!(packet.receipt.reviewed_promise_omissions.is_empty());
    let serialized = serde_json::to_string(&packet).unwrap();
    assert!(serialized.contains("reviewedPromises"));
    assert!(serialized.contains("lantern-promise"));
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert_eq!(envelope["target"]["displayName"], "chapter-target");
    assert_eq!(envelope["sources"][0]["displayName"], "chapter-promise");
    assert_eq!(
        envelope["reviewedPromises"]["sets"][0]["sourceDisplayName"],
        "chapter-promise"
    );
}

#[test]
fn layered_author_packet_keeps_frozen_names_for_promise_evidence() {
    let target_body = body(&[("target", "The ending.")]);
    let long_text = format!("The lantern waits. {}", "A".repeat(20_000));
    let promise_body = body(&[("promise", long_text.as_str())]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let promise_source = source(
        "promise",
        "chapter-promise",
        &promise_body,
        SourceKind::CurrentDraft,
        "0",
    );
    let promise_set = set(
        &promise_source,
        "bundle-promise",
        vec![promise(
            "promise-record",
            "lantern-promise",
            "The lantern promise",
            PromisePhase::Setup,
            PossessionTiming::AtPassage,
            EvidenceAudience::AuthorRoom,
            "promise",
            "The lantern waits.",
        )],
    );
    let frozen = working_frozen(
        &target,
        &[target.clone(), promise_source.clone()],
        vec![promise_set],
    );
    let request = PacketRequest {
        lookup: None,
        packet_id: "packet-layered-promises".into(),
        session_id: "session-layered-promises".into(),
        invocation_ordinal: "1".into(),
        frozen,
        instruction: "Discuss the promise.".into(),
        sources: vec![
            read(&target, &target_body),
            read(&promise_source, &promise_body),
        ],
        mandatory_handles: Vec::new(),
        scope: None,
        safe_brief: None,
        budget: MockContextBudget::new("8000", "100", "100"),
        provider_binding: None,
        response_contract: None,
    };
    let packet = compile_packet(&request).expect("layered packet should compile");
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert_eq!(envelope["packingMethod"], "layeredExcerpt");
    assert_eq!(envelope["target"]["displayName"], "chapter-target");
    assert_eq!(
        envelope["reviewedPromises"]["sets"][0]["sourceDisplayName"],
        "chapter-promise"
    );
    assert!(
        envelope["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| source["handle"] != "promise")
    );
}

#[test]
fn restricted_packet_omits_frozen_source_titles() {
    let target_body = body(&[("target", "The ending.")]);
    let reviewed_body = body(&[("reviewed", "The lantern waits.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "1",
    );
    let mut reviewed = source(
        "reviewed",
        "chapter-reviewed",
        &reviewed_body,
        SourceKind::ReviewedAuthority,
        "0",
    );
    reviewed.kind = SourceKind::ReviewedAuthority;
    let promise_set = set(
        &reviewed,
        "bundle-reviewed",
        vec![promise(
            "reader-record",
            "lantern-promise",
            "The lantern promise",
            PromisePhase::Setup,
            PossessionTiming::AtPassage,
            EvidenceAudience::Reader,
            "reviewed",
            "The lantern waits.",
        )],
    );
    let frozen = FrozenContext {
        reviewed_knowledge: Vec::new(),
        snapshot: StorySnapshot {
            snapshot_id: "snapshot-restricted-promise".into(),
            project_id: PROJECT.into(),
            basis: BasisKind::Reviewed,
            target: target.source.clone(),
            context_source_epoch: "1".into(),
            ordering_epoch: "1".into(),
            disclosure_policy_version: "1".into(),
            sources: vec![target.clone(), reviewed.clone()],
            reviewed_basis: Some(ReviewedBasisManifest {
                project_id: PROJECT.into(),
                operation_namespace: "namespace".into(),
                prefix: vec![ReviewedBasisMember {
                    document_id: reviewed.source.document_id.clone(),
                    bundle_id: "bundle-reviewed".into(),
                    revision_id: reviewed.source.revision_id.clone(),
                    version: "1".into(),
                    body_hash: reviewed.source.body_hash.clone(),
                }],
            }),
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
        purpose: ContextPurpose::Continue,
        aliases: BTreeMap::new(),
        excluded_source_count: 0,
        guidance: Vec::new(),
        conversation: None,
        navigation_views: Vec::new(),
        reviewed_evidence: Vec::new(),
        reviewed_summaries: Vec::new(),
        reviewed_promises: vec![promise_set],
    };
    let request = PacketRequest {
        lookup: None,
        packet_id: "packet-restricted-promises".into(),
        session_id: "session-restricted-promises".into(),
        invocation_ordinal: "1".into(),
        frozen,
        instruction: "Continue the chapter.".into(),
        sources: vec![read(&target, &target_body), read(&reviewed, &reviewed_body)],
        mandatory_handles: Vec::new(),
        scope: Some(capture_append_scope(&target_body).unwrap()),
        safe_brief: None,
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
        response_contract: None,
    };
    let packet = compile_packet(&request).expect("restricted packet should compile");
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert!(envelope["target"].get("displayName").is_none());
    assert!(
        envelope["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| source.get("displayName").is_none())
    );
    assert!(
        envelope["reviewedPromises"]["sets"][0]
            .get("sourceDisplayName")
            .is_none()
    );
}

#[test]
fn frozen_source_label_tampering_is_rejected_before_packet_packing() {
    let target_body = body(&[("target", "The ending.")]);
    let target = source(
        "target",
        "chapter-target",
        &target_body,
        SourceKind::CurrentDraft,
        "0",
    );
    let frozen = working_frozen(&target, std::slice::from_ref(&target), Vec::new());
    let mut request = PacketRequest {
        lookup: None,
        packet_id: "packet-label-tamper".into(),
        session_id: "session-label-tamper".into(),
        invocation_ordinal: "1".into(),
        frozen,
        instruction: "Discuss the ending.".into(),
        sources: vec![read(&target, &target_body)],
        mandatory_handles: Vec::new(),
        scope: None,
        safe_brief: None,
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
        response_contract: None,
    };
    request.frozen.snapshot.sources[0].display_name = "Mutable current title".into();
    let error = compile_packet(&request).expect_err("frozen label tampering must fail closed");
    assert!(matches!(
        error,
        webnovel_core::context::packet::PacketError::SourceBinding { code, .. }
            if code == "SourceDescriptorMismatch"
    ));
}
