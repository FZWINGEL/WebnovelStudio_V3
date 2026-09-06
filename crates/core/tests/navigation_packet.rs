use serde_json::{Value, json};
use std::collections::BTreeMap;
use webnovel_core::context::memory::{
    DIGEST_SCHEMA_VERSION, DigestCandidate, DigestEvidence, DigestItem,
};
use webnovel_core::context::navigation::{
    FrozenNavigationView, NavigationOmissionReason, NavigationViewRef, navigation_content_hash,
};
use webnovel_core::context::packet::{
    MockContextBudget, PacketError, PacketRequest, compile_packet,
};
use webnovel_core::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    SourceDescriptor, SourceKind, SourceRef, StorySnapshot,
};
use webnovel_core::projects::story_context::{FrozenContext, SourcePassage, SourceRead};
use webnovel_core::validate_snapshot_json;

const PROJECT: &str = "navigation-packet-project";

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

fn long_body(handle: &str, block_count: usize, width: usize) -> Value {
    let blocks = (0..block_count)
        .map(|index| {
            let id = format!("{handle}-block-{index}");
            let text = format!(
                "{handle} detail {index}: {}",
                "x".repeat(width.saturating_sub(handle.len() + 12))
            );
            json!({
                "type": "paragraph",
                "attrs": {"id": id},
                "content": [{"type": "text", "text": text}]
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schemaVersion": 1,
        "body": {"type": "doc", "content": blocks}
    })
}

fn first_block_text(document: &Value) -> String {
    document["body"]["content"][0]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn source(handle: &str, document_id: &str, body: &Value) -> SourceDescriptor {
    let hash = validate_snapshot_json(&serde_json::to_string(body).unwrap())
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
        display_name: handle.into(),
        kind: SourceKind::CurrentDraft,
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

fn read(descriptor: &SourceDescriptor, body: &Value) -> SourceRead {
    let canonical = validate_snapshot_json(&serde_json::to_string(body).unwrap())
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

fn digest(source: &SourceDescriptor, quote: &str) -> DigestCandidate {
    DigestCandidate {
        schema_version: DIGEST_SCHEMA_VERSION.into(),
        source: source.source.clone(),
        items: vec![DigestItem {
            text: "A useful navigation marker.".into(),
            evidence: vec![DigestEvidence {
                block_id: format!("{}-block-0", source.handle),
                from_utf16: 0,
                to_utf16: quote.encode_utf16().count() as u32,
                quote: quote.into(),
            }],
            uncertainty: None,
        }],
    }
}

fn view(source: &SourceDescriptor, quote: &str) -> FrozenNavigationView {
    let candidate = digest(source, quote);
    FrozenNavigationView {
        reference: NavigationViewRef {
            view_id: format!("view-{}", source.handle.as_str()),
            project_id: PROJECT.into(),
            operation_namespace: "project-operation".into(),
            content_hash: navigation_content_hash(&candidate).unwrap(),
        },
        source_context_epoch: "1".into(),
        disclosure_policy_version: "1".into(),
        dependencies: vec![source.source.clone()],
        candidate,
    }
}

fn frozen(
    descriptors: Vec<SourceDescriptor>,
    purpose: ContextPurpose,
    audience: Audience,
    navigation_views: Vec<FrozenNavigationView>,
) -> FrozenContext {
    let target = descriptors[0].source.clone();
    FrozenContext {
        snapshot: StorySnapshot {
            snapshot_id: "navigation-snapshot".into(),
            project_id: PROJECT.into(),
            basis: BasisKind::Working,
            target,
            context_source_epoch: "1".into(),
            ordering_epoch: "1".into(),
            disclosure_policy_version: "1".into(),
            sources: descriptors,
            reviewed_basis: None,
        },
        policy: InformationPolicy {
            version: "1".into(),
            audience,
            reader_frontier: (audience == Audience::RestrictedWriting).then(|| "1".into()),
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        },
        purpose,
        aliases: BTreeMap::new(),
        excluded_source_count: 0,
        guidance: Vec::new(),
        conversation: None,
        navigation_views,
        reviewed_evidence: Vec::new(),
        reviewed_promises: Vec::new(),
    }
}

fn request(
    target_body: &Value,
    chapter_body: &Value,
    budget: usize,
    navigation_views: Vec<FrozenNavigationView>,
) -> PacketRequest {
    let target = source("target", "target-doc", target_body);
    let chapter = source("chapter", "chapter-doc", chapter_body);
    request_with_sources(
        vec![
            (target, target_body.clone()),
            (chapter, chapter_body.clone()),
        ],
        budget,
        navigation_views,
        Vec::new(),
    )
}

fn request_with_sources(
    sources: Vec<(SourceDescriptor, Value)>,
    budget: usize,
    navigation_views: Vec<FrozenNavigationView>,
    mandatory_handles: Vec<String>,
) -> PacketRequest {
    let descriptors: Vec<_> = sources
        .iter()
        .map(|(descriptor, _)| descriptor.clone())
        .collect();
    PacketRequest {
        lookup: None,
        packet_id: "packet-1".into(),
        session_id: "session-1".into(),
        invocation_ordinal: "1".into(),
        frozen: frozen(
            descriptors,
            ContextPurpose::Discuss,
            Audience::AuthorRoom,
            navigation_views,
        ),
        instruction: "Locate the useful chapter detail.".into(),
        sources: sources
            .iter()
            .map(|(descriptor, body)| read(descriptor, body))
            .collect(),
        mandatory_handles,
        scope: None,
        safe_brief: None,
        budget: MockContextBudget::new(budget.to_string(), "0", "0"),
        provider_binding: None,
        response_contract: None,
    }
}

#[test]
fn full_text_fit_keeps_packet_shape_and_records_original_text_omission() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let chapter_body = body(&[("chapter-block-0", "The earlier chapter detail.")]);
    let chapter = source("chapter", "chapter-doc", &chapter_body);
    let navigation = view(&chapter, "The earlier chapter detail.");
    let packet = compile_packet(&request(
        &target_body,
        &chapter_body,
        100_000,
        vec![navigation],
    ))
    .expect("full packet");
    let plain_packet = compile_packet(&request(&target_body, &chapter_body, 100_000, Vec::new()))
        .expect("plain full packet");
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert!(envelope.get("derivedViews").is_none());
    assert_eq!(packet.messages, plain_packet.messages);
    assert_eq!(packet.options, plain_packet.options);
    assert_eq!(packet.receipt.input_hash, plain_packet.receipt.input_hash);
    assert!(packet.receipt.navigation_views.is_empty());
    assert_eq!(packet.receipt.navigation_omissions.len(), 1);
    assert_eq!(
        packet.receipt.navigation_omissions[0].reason,
        NavigationOmissionReason::OriginalTextIncluded
    );
}

#[test]
fn closed_navigation_view_from_an_earlier_epoch_keeps_its_provenance() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let chapter_body = body(&[
        (
            "chapter-block-0",
            &"A long earlier chapter detail. ".repeat(80),
        ),
        (
            "chapter-block-1",
            &"Another long earlier chapter detail. ".repeat(80),
        ),
    ]);
    let chapter = source("chapter", "chapter-doc", &chapter_body);
    let navigation = view(&chapter, "A long earlier chapter detail.");
    let mut request = request(&target_body, &chapter_body, 3_000, vec![navigation]);
    request.frozen.snapshot.context_source_epoch = "2".into();

    let packet = compile_packet(&request).expect("an earlier closed chapter view remains usable");
    assert_eq!(packet.receipt.navigation_views.len(), 1);
    assert_eq!(
        request.frozen.navigation_views[0].source_context_epoch, "1",
        "the view epoch is provenance and must not be rewritten to the newer snapshot epoch"
    );
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert_eq!(envelope["derivedViews"]["completeCandidate"], true);
}

#[test]
fn future_or_malformed_navigation_epochs_are_refused() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let chapter_body = body(&[("chapter-block-0", "Exact evidence.")]);
    let chapter = source("chapter", "chapter-doc", &chapter_body);

    let mut future = request(
        &target_body,
        &chapter_body,
        100_000,
        vec![view(&chapter, "Exact evidence.")],
    );
    future.frozen.snapshot.context_source_epoch = "2".into();
    future.frozen.navigation_views[0].source_context_epoch = "3".into();
    let error = compile_packet(&future).expect_err("a view from a future epoch");
    assert!(matches!(error, PacketError::SourceBinding { .. }));

    let mut malformed_view = request(
        &target_body,
        &chapter_body,
        100_000,
        vec![view(&chapter, "Exact evidence.")],
    );
    malformed_view.frozen.navigation_views[0].source_context_epoch = "01".into();
    let error = compile_packet(&malformed_view).expect_err("a noncanonical view epoch");
    assert!(matches!(error, PacketError::SourceBinding { .. }));

    let mut malformed_snapshot = request(
        &target_body,
        &chapter_body,
        100_000,
        vec![view(&chapter, "Exact evidence.")],
    );
    malformed_snapshot.frozen.snapshot.context_source_epoch = "not-an-epoch".into();
    let error = compile_packet(&malformed_snapshot).expect_err("a malformed snapshot epoch");
    assert!(matches!(error, PacketError::SourceBinding { .. }));
}

#[test]
fn smaller_view_is_delivered_without_duplicate_original_source() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let chapter_body = body(&[
        (
            "chapter-block-0",
            &"A long earlier chapter detail. ".repeat(80),
        ),
        (
            "chapter-block-1",
            &"Another long earlier chapter detail. ".repeat(80),
        ),
    ]);
    let chapter = source("chapter", "chapter-doc", &chapter_body);
    let navigation = view(&chapter, "A long earlier chapter detail.");
    let packet = compile_packet(&request(
        &target_body,
        &chapter_body,
        3_000,
        vec![navigation],
    ))
    .expect("bounded packet");
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert_eq!(packet.receipt.navigation_views.len(), 1);
    assert_eq!(envelope["derivedViews"]["coverage"], "unreviewedGenerated");
    assert_eq!(envelope["derivedViews"]["representation"], "digest");
    assert_eq!(envelope["derivedViews"]["completeCandidate"], true);
    assert_eq!(packet.receipt.source_handles, vec!["target"]);
    assert!(
        packet.receipt.omissions.iter().any(|text| text
            == "handle:chapter;reason:navigation view delivered;original source omitted")
    );
    assert!(
        !packet.messages[1]
            .content
            .contains("Another long earlier chapter detail.")
    );
}

#[test]
fn tampered_view_payload_fails_before_budget_fallback() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let chapter_body = body(&[("chapter-block-0", "Exact evidence.")]);
    let chapter = source("chapter", "chapter-doc", &chapter_body);
    let mut navigation = view(&chapter, "Exact evidence.");
    navigation.dependencies.clear();
    let error = compile_packet(&request(&target_body, &chapter_body, 1, vec![navigation]))
        .expect_err("tampered dependency");
    assert!(matches!(error, PacketError::SourceBinding { .. }));
}

#[test]
fn unsupported_scope_rejects_frozen_views() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let chapter_body = body(&[("chapter-block-0", "Exact evidence.")]);
    let chapter = source("chapter", "chapter-doc", &chapter_body);
    let navigation = view(&chapter, "Exact evidence.");
    let mut request = request(&target_body, &chapter_body, 100_000, vec![navigation]);
    request.frozen.purpose = ContextPurpose::Revise;
    request.frozen.policy.audience = Audience::RestrictedWriting;
    let error = compile_packet(&request).expect_err("unsupported view scope");
    assert!(matches!(error, PacketError::SourceBinding { .. }));
}

#[test]
fn mandatory_pinned_source_keeps_exact_original_over_navigation_view() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let chapter_body = long_body("chapter", 4, 900);
    let optional_body = long_body("optional", 10, 1_200);
    let chapter = source("chapter", "chapter-doc", &chapter_body);
    let optional = source("optional", "optional-doc", &optional_body);
    let navigation = view(&chapter, &first_block_text(&chapter_body));
    let packet_request = request_with_sources(
        vec![
            (
                source("target", "target-doc", &target_body),
                target_body.clone(),
            ),
            (chapter.clone(), chapter_body.clone()),
            (optional, optional_body),
        ],
        6_500,
        vec![navigation],
        vec!["chapter".into()],
    );
    let packet = compile_packet(&packet_request).expect("mandatory original should fit");
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert_eq!(envelope["packingMethod"], "layeredExcerpt");
    assert!(envelope.get("derivedViews").is_none());
    assert!(packet.receipt.navigation_views.is_empty());
    assert!(
        packet
            .receipt
            .source_handles
            .iter()
            .any(|handle| handle == "chapter")
    );
    assert_eq!(
        packet.receipt.navigation_omissions,
        vec![webnovel_core::context::navigation::NavigationViewOmission {
            view_id: "view-chapter".into(),
            reason: NavigationOmissionReason::OriginalTextIncluded,
        }]
    );
    assert!(
        packet.messages[1]
            .content
            .contains(&first_block_text(&chapter_body))
    );
}

#[test]
fn dependency_and_quote_tampering_fails_even_with_recomputed_hash() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let chapter_body = body(&[("chapter-block-0", "Exact evidence.")]);
    let chapter = source("chapter", "chapter-doc", &chapter_body);

    let mut extra_dependency = view(&chapter, "Exact evidence.");
    extra_dependency.dependencies.push(SourceRef {
        project_id: PROJECT.into(),
        document_id: "other-doc".into(),
        revision_id: "revision-other-doc".into(),
        body_hash: "other-body-hash".into(),
    });
    let error = compile_packet(&request(
        &target_body,
        &chapter_body,
        100_000,
        vec![extra_dependency],
    ))
    .expect_err("unknown dependency");
    assert!(matches!(error, PacketError::SourceBinding { .. }));

    let mut missing_dependency = view(&chapter, "Exact evidence.");
    missing_dependency.dependencies.clear();
    let error = compile_packet(&request(
        &target_body,
        &chapter_body,
        100_000,
        vec![missing_dependency],
    ))
    .expect_err("missing dependency");
    assert!(matches!(error, PacketError::SourceBinding { .. }));

    let mut edited_quote = view(&chapter, "Exact evidence.");
    edited_quote.candidate.items[0].evidence[0].quote = "Edited evidence.".into();
    edited_quote.candidate.items[0].evidence[0].to_utf16 =
        "Edited evidence.".encode_utf16().count() as u32;
    edited_quote.reference.content_hash = navigation_content_hash(&edited_quote.candidate).unwrap();
    let error = compile_packet(&request(
        &target_body,
        &chapter_body,
        100_000,
        vec![edited_quote],
    ))
    .expect_err("edited quote with a recomputed candidate hash");
    assert!(matches!(error, PacketError::SourceBinding { .. }));

    let mut unknown_source = view(&chapter, "Exact evidence.");
    unknown_source.candidate.source.revision_id = "revision-edited".into();
    unknown_source.dependencies = vec![unknown_source.candidate.source.clone()];
    unknown_source.reference.content_hash =
        navigation_content_hash(&unknown_source.candidate).unwrap();
    let error = compile_packet(&request(
        &target_body,
        &chapter_body,
        100_000,
        vec![unknown_source],
    ))
    .expect_err("a candidate bound to an exact but unknown source revision");
    assert!(matches!(error, PacketError::SourceBinding { .. }));
}

#[test]
fn not_smaller_view_is_omitted_with_an_explicit_reason() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let chapter_body = body(&[("chapter-block-0", "Tiny source.")]);
    let optional_body = long_body("optional", 12, 1_000);
    let chapter = source("chapter", "chapter-doc", &chapter_body);
    let navigation = view(&chapter, "Tiny source.");
    let packet = compile_packet(&request_with_sources(
        vec![
            (source("target", "target-doc", &target_body), target_body),
            (chapter, chapter_body),
            (
                source("optional", "optional-doc", &optional_body),
                optional_body,
            ),
        ],
        2_600,
        vec![navigation],
        Vec::new(),
    ))
    .expect("bounded packet");
    assert!(packet.receipt.navigation_views.is_empty());
    assert_eq!(
        packet.receipt.navigation_omissions,
        vec![webnovel_core::context::navigation::NavigationViewOmission {
            view_id: "view-chapter".into(),
            reason: NavigationOmissionReason::NotSmaller,
        }]
    );
}

#[test]
fn views_grow_as_a_stable_whole_prefix_without_partial_candidates() {
    let target_body = body(&[("target-block", "The current scene.")]);
    let first_body = long_body("chapter-a", 5, 1_000);
    let second_body = long_body("chapter-b", 5, 1_000);
    let first = source("chapter-a", "chapter-a-doc", &first_body);
    let second = source("chapter-b", "chapter-b-doc", &second_body);
    let first_view = view(&first, &first_block_text(&first_body));
    let second_view = view(&second, &first_block_text(&second_body));
    let base_request = request_with_sources(
        vec![
            (source("target", "target-doc", &target_body), target_body),
            (first, first_body),
            (second, second_body),
        ],
        1_000,
        vec![first_view.clone(), second_view.clone()],
        Vec::new(),
    );

    let mut layered_counts = Vec::new();
    for budget in (1_000..=12_000).step_by(100) {
        let mut request = base_request.clone();
        request.budget = MockContextBudget::new(budget.to_string(), "0", "0");
        let Ok(packet) = compile_packet(&request) else {
            continue;
        };
        let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
        if envelope["packingMethod"] != "layeredExcerpt" {
            continue;
        }
        let delivered = packet.receipt.navigation_views.len();
        layered_counts.push((budget, delivered));
        if let Some(views) = envelope["derivedViews"]["views"].as_array() {
            for view in views {
                let view_id = view["reference"]["viewId"].as_str().unwrap();
                let expected = match view_id {
                    "view-chapter-a" => &first_view,
                    "view-chapter-b" => &second_view,
                    other => panic!("unexpected delivered navigation view {other}"),
                };
                assert_eq!(
                    view["candidate"],
                    serde_json::to_value(&expected.candidate).unwrap()
                );
                assert_eq!(
                    view["dependencies"],
                    serde_json::to_value(&expected.dependencies).unwrap()
                );
                assert_eq!(
                    view["reference"]["contentHash"],
                    expected.reference.content_hash
                );
            }
        }
    }
    assert!(layered_counts.iter().any(|(_, count)| *count == 1));
    assert!(layered_counts.iter().any(|(_, count)| *count == 2));
    let mut previous = 0;
    for (budget, count) in layered_counts {
        assert!(
            count >= previous,
            "navigation coverage regressed at budget {budget}: {count} after {previous}"
        );
        previous = count;
    }
}
