use serde_json::{Value, json};
use std::collections::BTreeMap;
use webnovel_core::context::packet::{
    CompiledPacket, MockContextBudget, PacketError, PacketMessage, PacketOptions, PacketRequest,
    compile_packet, packet_input_hash, serialized_input,
};
use webnovel_core::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    SourceDescriptor, SourceKind, SourceRef, StorySnapshot,
};
use webnovel_core::documents::{ScopeGrant, ScopeKind, capture_scope};
use webnovel_core::projects::story_context::{FrozenContext, SourcePassage, SourceRead};
use webnovel_core::validate_snapshot_json;

const PROJECT: &str = "packet-project";

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
        display_name: format!("Private title {handle}"),
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

fn frozen(
    descriptors: Vec<SourceDescriptor>,
    purpose: ContextPurpose,
    audience: Audience,
) -> FrozenContext {
    let target = descriptors[0].source.clone();
    StorySnapshot {
        snapshot_id: "snapshot-packet".into(),
        project_id: PROJECT.into(),
        basis: BasisKind::Working,
        target,
        context_source_epoch: "1".into(),
        ordering_epoch: "1".into(),
        disclosure_policy_version: "1".into(),
        sources: descriptors,
    }
    .pipe(|snapshot| FrozenContext {
        snapshot,
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
    })
}

trait Pipe: Sized {
    fn pipe<T>(self, function: impl FnOnce(Self) -> T) -> T {
        function(self)
    }
}
impl<T> Pipe for T {}

fn request(frozen: FrozenContext, reads: Vec<SourceRead>) -> PacketRequest {
    PacketRequest {
        packet_id: "packet-1".into(),
        session_id: "session-1".into(),
        invocation_ordinal: "1".into(),
        frozen,
        instruction: "Keep the ending exactly as written.".into(),
        sources: reads,
        mandatory_handles: Vec::new(),
        scope: None,
        budget: MockContextBudget::new("100000", "100", "100"),
    }
}

fn compile(request: PacketRequest) -> CompiledPacket {
    compile_packet(&request).unwrap_or_else(|error| panic!("packet should compile: {error}"))
}

fn guidance(text: &str) -> webnovel_core::context::guidance::FrozenGuidance {
    use sha2::{Digest, Sha256};
    use webnovel_core::context::guidance::{FrozenGuidance, GuidanceScope, GuidanceVersion};
    FrozenGuidance {
        handle: "guidance-guidance-version-1".into(),
        project_id: PROJECT.into(),
        version: GuidanceVersion {
            guidance_id: "author-intention".into(),
            version_id: "guidance-version-1".into(),
            version: "1".into(),
            scope: GuidanceScope::Document,
            document_id: Some("target-doc".into()),
            text: text.into(),
            text_hash: Sha256::digest(text.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            active: true,
            origin_message_id: None,
            created_at: "2026-09-05T00:00:00Z".into(),
        },
    }
}

fn conversation_request() -> PacketRequest {
    use webnovel_core::context::conversation::{
        ConversationMessage, ConversationTurn, FrozenConversation,
    };
    let prose = body(&[("target-1", "Keep this exact ending.")]);
    let target = source("target", "target-doc", &prose);
    let optional_body = body(&[("old-1", &"An older promise. ".repeat(180))]);
    let optional = source("earlier", "old-doc", &optional_body);
    let mut frozen = frozen(
        vec![target.clone(), optional.clone()],
        ContextPurpose::Discuss,
        Audience::AuthorRoom,
    );
    frozen
        .guidance
        .push(guidance("The ending must remain intact."));
    frozen.conversation = Some(FrozenConversation {
        project_id: PROJECT.into(),
        operation_namespace: "operations".into(),
        document_id: "target-doc".into(),
        thread_id: "discussion".into(),
        omitted_turns: 3,
        turns: (0..2)
            .map(|index| ConversationTurn {
                run_id: format!("run-{index}"),
                packet_id: format!("old-packet-{index}"),
                source_snapshot_id: format!("old-source-{index}"),
                policy_version: "1".into(),
                user: ConversationMessage {
                    id: format!("user-{index}"),
                    content: format!("Question {index}: {}", "A prior question. ".repeat(20)),
                    scope: None,
                },
                assistant: ConversationMessage {
                    id: format!("assistant-{index}"),
                    content: format!(
                        "Unadopted reply {index}: {}",
                        "A possible idea. ".repeat(35)
                    ),
                    scope: None,
                },
            })
            .collect(),
    });
    request(
        frozen,
        vec![read(&target, &prose), read(&optional, &optional_body)],
    )
}

#[test]
fn discussion_packet_delivers_exact_whole_turns_in_chronological_order() {
    let request = conversation_request();
    let packet = compile(request.clone());
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert_eq!(
        envelope["recentDiscussion"],
        serde_json::to_value(
            request
                .frozen
                .conversation
                .unwrap()
                .turns
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
        )
        .unwrap()
    );
    assert_eq!(
        packet.receipt.conversation_message_ids,
        ["user-1", "assistant-1", "user-0", "assistant-0"]
    );
    assert_eq!(packet.receipt.omitted_discussion_turns, 3);
    assert!(
        packet.messages[0]
            .content
            .contains("does not adopt earlier suggestions")
    );
    assert_eq!(packet.messages[2].content, request.instruction);
}

#[test]
fn discussion_budget_extends_a_stable_prefix_without_clipping_turns_or_guidance() {
    let mut request = conversation_request();
    let mut prior_messages = std::collections::HashSet::new();
    let mut prior_sources = std::collections::HashSet::new();
    let mut saw_omitted = false;
    let mut saw_full = false;
    for window in (2000..16000).step_by(250) {
        request.budget.context_window_tokens = window.to_string();
        let Ok(packet) = compile_packet(&request) else {
            continue;
        };
        let messages: std::collections::HashSet<_> = packet
            .receipt
            .conversation_message_ids
            .iter()
            .cloned()
            .collect();
        let sources: std::collections::HashSet<_> =
            packet.receipt.source_handles.iter().cloned().collect();
        assert!(prior_messages.is_subset(&messages));
        assert!(prior_sources.is_subset(&sources));
        assert_eq!(messages.len() % 2, 0);
        assert_eq!(packet.messages[2].content, request.instruction);
        assert_eq!(packet.receipt.guidance_handles.len(), 1);
        let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
        assert_eq!(
            envelope["authorGuidance"],
            serde_json::to_value(&request.frozen.guidance).unwrap()
        );
        if let Some(turns) = envelope["recentDiscussion"].as_array() {
            for turn in turns {
                assert!(
                    request
                        .frozen
                        .conversation
                        .as_ref()
                        .unwrap()
                        .turns
                        .iter()
                        .any(|original| serde_json::to_value(original).unwrap() == *turn)
                );
            }
        }
        saw_omitted |= messages.len() < 4;
        saw_full |= messages.len() == 4 && sources.len() == 2;
        prior_messages = messages;
        prior_sources = sources;
    }
    assert!(saw_omitted && saw_full);
}

#[test]
fn conversation_context_rejects_foreign_private_incomplete_and_oversized_turns() {
    let valid = conversation_request();
    for change in 0..7 {
        let mut request = valid.clone();
        let conversation = request.frozen.conversation.as_mut().unwrap();
        match change {
            0 => conversation.project_id = "other-project".into(),
            1 => conversation.document_id = "other-document".into(),
            2 => conversation.turns[0].policy_version = "0".into(),
            3 => conversation.turns[0].assistant.content.clear(),
            4 => conversation.turns[0].assistant.id = conversation.turns[0].user.id.clone(),
            5 => conversation.turns[0].assistant.content = "x".repeat(17000),
            _ => request.frozen.policy.audience = Audience::RestrictedWriting,
        }
        assert!(
            matches!(compile_packet(&request), Err(PacketError::SourceBinding { code, .. }) if code == "InvalidConversationContext"),
            "case {change}"
        );
    }
}

#[test]
fn adopted_guidance_is_exact_mandatory_context_separate_from_story_sources() {
    let prose = body(&[("target-1", "A quiet reunion.")]);
    let target = source("target", "target-doc", &prose);
    let mut frozen = frozen(
        vec![target.clone()],
        ContextPurpose::Discuss,
        Audience::AuthorRoom,
    );
    frozen
        .guidance
        .push(guidance("  Keep the ending.\nHer sister must survive.  "));
    let original = frozen.guidance.clone();
    let packet = compile(request(frozen, vec![read(&target, &prose)]));
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert_eq!(
        envelope["authorGuidance"],
        serde_json::to_value(&original).unwrap()
    );
    assert_eq!(
        packet.receipt.guidance_handles,
        vec![original[0].handle.clone()]
    );
    assert_eq!(packet.receipt.source_handles, vec!["target"]);
    assert!(
        packet.messages[0]
            .content
            .contains("not established story facts")
    );
    assert_eq!(
        packet.messages[2].content,
        "Keep the ending exactly as written."
    );
    assert_eq!(
        packet.receipt.input_hash,
        packet_input_hash(&packet.messages, &packet.options).unwrap()
    );
}

#[test]
fn guidance_overflow_is_reported_instead_of_silently_shortening_the_instruction() {
    let prose = body(&[("target-1", "The ending.")]);
    let target = source("target", "target-doc", &prose);
    let mut request = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Discuss,
            Audience::AuthorRoom,
        ),
        vec![read(&target, &prose)],
    );
    let baseline = compile_packet(&request)
        .unwrap()
        .receipt
        .input_tokens
        .parse::<usize>()
        .unwrap();
    request
        .frozen
        .guidance
        .push(guidance(&"Keep the ending. ".repeat(400)));
    request.budget = MockContextBudget::new((baseline + 1000).to_string(), "0", "0");
    match compile_packet(&request).unwrap_err() {
        PacketError::Budget(error) => {
            assert_eq!(
                error.code,
                webnovel_core::context::BudgetErrorCode::MandatoryContextTooLarge
            );
            assert!(
                error
                    .mandatory_handles
                    .contains(&request.frozen.guidance[0].handle)
            );
        }
        error => panic!("expected guidance budget error: {error}"),
    }
}

#[test]
fn guidance_rejects_cross_project_wrong_scope_tampering_and_restricted_disclosure() {
    let prose = body(&[("target-1", "The chapter.")]);
    let target = source("target", "target-doc", &prose);
    let mut base = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Discuss,
            Audience::AuthorRoom,
        ),
        vec![read(&target, &prose)],
    );
    base.frozen
        .guidance
        .push(guidance("The mentor knows the secret."));
    for case in 0..6 {
        let mut changed = base.clone();
        match case {
            0 => changed.frozen.guidance[0].project_id = "another-project".into(),
            1 => changed.frozen.guidance[0].version.document_id = Some("another-document".into()),
            2 => changed.frozen.guidance[0].version.text.push_str(" Changed"),
            3 => changed.frozen.policy.audience = Audience::RestrictedWriting,
            4 => changed.frozen.guidance[0].version.active = false,
            _ => changed
                .frozen
                .guidance
                .push(changed.frozen.guidance[0].clone()),
        }
        assert!(
            matches!(compile_packet(&changed), Err(PacketError::SourceBinding { code, .. }) if code == "InvalidGuidance"),
            "case {case}"
        );
    }
}

#[test]
fn old_snapshot_and_receipt_json_keep_their_shape_when_guidance_is_absent() {
    let prose = body(&[("target-1", "An older saved chapter.")]);
    let target = source("target", "target-doc", &prose);
    let frozen = frozen(
        vec![target.clone()],
        ContextPurpose::Discuss,
        Audience::AuthorRoom,
    );
    let frozen_json = serde_json::to_value(&frozen).unwrap();
    assert!(frozen_json.get("guidance").is_none());
    assert!(frozen_json.get("conversation").is_none());
    let reloaded: FrozenContext = serde_json::from_value(frozen_json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&reloaded).unwrap(), frozen_json);
    let packet = compile(request(reloaded, vec![read(&target, &prose)]));
    let packet_json = serde_json::to_value(&packet).unwrap();
    assert!(packet_json["receipt"].get("guidanceHandles").is_none());
    assert!(
        packet_json["receipt"]
            .get("conversationMessageIds")
            .is_none()
    );
    assert!(
        packet_json["receipt"]
            .get("omittedDiscussionTurns")
            .is_none()
    );
    let reloaded: CompiledPacket = serde_json::from_value(packet_json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&reloaded).unwrap(), packet_json);
    let envelope: Value = serde_json::from_str(&reloaded.messages[1].content).unwrap();
    assert!(envelope.get("authorGuidance").is_none());
    assert!(envelope.get("recentDiscussion").is_none());
    assert!(envelope.get("omittedDiscussionTurns").is_none());
}

#[test]
fn full_text_packet_preserves_instruction_target_and_receipt_hash() {
    let target_body = body(&[("target-1", "The jade pendant glinted.")]);
    let extra_body = body(&[("extra-1", "A storm gathered beyond the gate.")]);
    let target = source("target", "target-doc", &target_body);
    let extra = source("extra", "extra-doc", &extra_body);
    let frozen = frozen(
        vec![target.clone(), extra.clone()],
        ContextPurpose::StoryQuestion,
        Audience::AuthorRoom,
    );
    let packet = compile(request(
        frozen,
        vec![read(&target, &target_body), read(&extra, &extra_body)],
    ));
    assert_eq!(
        packet.messages[2].content,
        "Keep the ending exactly as written."
    );
    assert!(
        packet.messages[1]
            .content
            .contains("The jade pendant glinted.")
    );
    assert!(
        packet.messages[1]
            .content
            .contains("A storm gathered beyond the gate.")
    );
    assert_eq!(packet.receipt.source_handles, vec!["target", "extra"]);
    assert!(packet.receipt.omissions.is_empty());
    assert!(
        packet
            .receipt
            .coverage
            .iter()
            .all(|entry| entry.label == "fullText")
    );
    assert_eq!(
        packet.receipt.input_hash,
        packet_input_hash(&packet.messages, &packet.options).unwrap()
    );
    assert_eq!(
        packet.receipt.input_tokens,
        serialized_input(&packet.messages, &packet.options)
            .unwrap()
            .len()
            .to_string()
    );
}

#[test]
fn mandatory_target_and_pin_reject_tiny_budget_without_truncation() {
    let target_body = body(&[("target-1", "A target that must remain whole.")]);
    let pin_body = body(&[("pin-1", "A pinned rule that must remain whole.")]);
    let target = source("target", "target-doc", &target_body);
    let pin = source("pin", "pin-doc", &pin_body);
    let frozen = frozen(
        vec![target.clone(), pin.clone()],
        ContextPurpose::StoryQuestion,
        Audience::AuthorRoom,
    );
    let mut request = request(
        frozen,
        vec![read(&target, &target_body), read(&pin, &pin_body)],
    );
    request.mandatory_handles = vec!["pin".into()];
    request.budget = MockContextBudget::new("1", "0", "0");
    let error = compile_packet(&request).expect_err("mandatory context must fail closed");
    match error {
        PacketError::Budget(error) => {
            assert_eq!(
                error.code,
                webnovel_core::context::BudgetErrorCode::MandatoryContextTooLarge
            );
            assert_eq!(error.mandatory_handles, vec!["target", "pin"]);
            assert!(error.required_input_tokens.parse::<usize>().unwrap() > 1);
        }
        other => panic!("expected structured budget error, got {other:?}"),
    }
}

#[test]
fn prose_requests_require_an_explicit_scope() {
    let target_body = body(&[("target-1", "The selected passage.")]);
    let target = source("target", "target-doc", &target_body);
    for purpose in [ContextPurpose::Revise, ContextPurpose::Continue] {
        let request = request(
            frozen(vec![target.clone()], purpose, Audience::RestrictedWriting),
            vec![read(&target, &target_body)],
        );
        assert!(matches!(
            compile_packet(&request),
            Err(PacketError::ScopeValidation { .. })
        ));
    }
}

#[test]
fn a_mandatory_pin_retains_its_complete_evidence_dependencies() {
    let target_body = body(&[("target-1", "The confrontation begins.")]);
    let pin_body = body(&[("pin-1", "An overview of the promise.")]);
    let evidence_body = body(&[("evidence-1", &"The original promise. ".repeat(1000))]);
    let target = source("target", "target-doc", &target_body);
    let evidence = source("evidence", "evidence-doc", &evidence_body);
    let mut pin = source("pin", "pin-doc", &pin_body);
    pin.kind = SourceKind::GeneratedDigest;
    pin.coverage = CoverageLabel::Digest;
    pin.dependencies = vec![evidence.source.clone()];
    let mut frozen = frozen(
        vec![target.clone(), pin.clone(), evidence.clone()],
        ContextPurpose::StoryQuestion,
        Audience::AuthorRoom,
    );
    frozen.excluded_source_count = 3;
    let mut request = request(
        frozen,
        vec![
            read(&target, &target_body),
            read(&pin, &pin_body),
            read(&evidence, &evidence_body),
        ],
    );
    request.mandatory_handles = vec!["pin".into()];
    request.budget = MockContextBudget::new("6000", "100", "100");
    match compile_packet(&request).unwrap_err() {
        PacketError::Budget(error) => assert!(error.mandatory_handles.contains(&"evidence".into())),
        other => panic!("expected dependency budget refusal, got {other}"),
    }
    request.budget = MockContextBudget::new("100000", "100", "100");
    let packet = compile(request);
    assert_eq!(packet.receipt.source_handles.len(), 3);
    assert!(packet.receipt.omissions.is_empty());
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert_eq!(envelope["policyExcludedSourceCount"], 3);
    assert!(
        envelope["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| source["mandatory"] == true)
    );
}

#[test]
fn a_small_budget_does_not_materialize_one_full_chapter_per_optional_block() {
    let target_body = body(&[("target-1", "A short current scene.")]);
    let blocks: Vec<_> = (0..10_000)
        .map(|i| {
            (
                format!("block-{i}"),
                "An older detail preserved in its original paragraph.".to_owned(),
            )
        })
        .collect();
    let refs: Vec<_> = blocks
        .iter()
        .map(|(id, text)| (id.as_str(), text.as_str()))
        .collect();
    let large_body = body(&refs);
    let target = source("target", "target-doc", &target_body);
    let earlier = source("earlier", "earlier-doc", &large_body);
    let mut request = request(
        frozen(
            vec![target.clone(), earlier.clone()],
            ContextPurpose::Discuss,
            Audience::AuthorRoom,
        ),
        vec![read(&target, &target_body), read(&earlier, &large_body)],
    );
    request.budget = MockContextBudget::new("6000", "100", "100");
    let packet = compile(request);
    assert!(packet.receipt.input_tokens.parse::<usize>().unwrap() <= 5800);
    assert!(
        packet
            .receipt
            .omissions
            .iter()
            .any(|text| text.contains("earlier"))
    );
    assert_eq!(
        packet
            .receipt
            .source_handles
            .iter()
            .filter(|handle| *handle == "target")
            .count(),
        1
    );
}

#[test]
fn scope_is_revalidated_against_the_same_target_source() {
    let target_body = body(&[("target-1", "The selected passage.")]);
    let target = source("target", "target-doc", &target_body);
    let frozen = frozen(
        vec![target.clone()],
        ContextPurpose::Revise,
        Audience::RestrictedWriting,
    );
    let grant = capture_scope(
        &target_body,
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
    .unwrap();
    let mut request = request(frozen, vec![read(&target, &target_body)]);
    request.scope = Some(grant.clone());
    let packet = compile(request);
    assert!(packet.messages[1].content.contains(&grant.source_hash));
    assert!(packet.messages[1].content.contains("The selected passage."));
}

#[test]
fn alias_or_unknown_source_read_is_rejected_before_eligibility() {
    let target_body = body(&[("target-1", "Target text.")]);
    let target = source("target", "target-doc", &target_body);
    let mut alias = read(&target, &target_body);
    alias.descriptor.handle = "alias-name".into();
    let frozen = frozen(
        vec![target],
        ContextPurpose::StoryQuestion,
        Audience::AuthorRoom,
    );
    let error = compile_packet(&request(frozen, vec![alias])).err().unwrap();
    assert!(
        matches!(error, PacketError::SourceBinding { code, .. } if code == "SourceOutsideFrozenManifest")
    );
}

#[test]
fn duplicate_source_reads_are_rejected() {
    let target_body = body(&[("target-1", "Target text.")]);
    let target = source("target", "target-doc", &target_body);
    let frozen = frozen(
        vec![target.clone()],
        ContextPurpose::StoryQuestion,
        Audience::AuthorRoom,
    );
    let error = compile_packet(&request(
        frozen,
        vec![read(&target, &target_body), read(&target, &target_body)],
    ))
    .expect_err("duplicate source records must not be merged");
    assert!(
        matches!(error, PacketError::SourceBinding { code, .. } if code == "DuplicateSourceRead")
    );
}

#[test]
fn subset_of_frozen_manifest_is_rejected_instead_of_claiming_full_context() {
    let target_body = body(&[("target-1", "Target text.")]);
    let extra_body = body(&[("extra-1", "Extra text.")]);
    let target = source("target", "target-doc", &target_body);
    let extra = source("extra", "extra-doc", &extra_body);
    let frozen = frozen(
        vec![target.clone(), extra],
        ContextPurpose::StoryQuestion,
        Audience::AuthorRoom,
    );
    let error = compile_packet(&request(frozen, vec![read(&target, &target_body)]))
        .expect_err("every frozen source read is required");
    assert!(
        matches!(error, PacketError::SourceBinding { code, handle: Some(handle), .. } if code == "FrozenManifestReadMissing" && handle == "extra")
    );
}

#[test]
fn extreme_decimal_reservations_return_invalid_budget_without_underflow() {
    let target_body = body(&[("target-1", "Target text.")]);
    let target = source("target", "target-doc", &target_body);
    let frozen = frozen(
        vec![target.clone()],
        ContextPurpose::StoryQuestion,
        Audience::AuthorRoom,
    );
    let mut request = request(frozen, vec![read(&target, &target_body)]);
    request.budget = MockContextBudget::new(
        u128::MAX.to_string(),
        u128::MAX.to_string(),
        u128::MAX.to_string(),
    );
    let error = compile_packet(&request).expect_err("overflowing reservations must fail closed");
    assert!(
        matches!(error, PacketError::Budget(error) if error.code == webnovel_core::context::BudgetErrorCode::InvalidBudget)
    );
}

#[test]
fn useful_block_coverage_is_monotonic_as_budget_grows() {
    let target_body = body(&[("target-1", "Target.")]);
    let extra_body = body(&[
        ("extra-1", "First useful detail."),
        ("extra-2", "Second useful detail."),
        ("extra-3", "Third useful detail."),
    ]);
    let target = source("target", "target-doc", &target_body);
    let extra = source("extra", "extra-doc", &extra_body);
    let frozen = frozen(
        vec![target.clone(), extra.clone()],
        ContextPurpose::StoryQuestion,
        Audience::AuthorRoom,
    );
    let reads = vec![read(&target, &target_body), read(&extra, &extra_body)];
    let mut low = request(frozen.clone(), reads.clone());
    low.budget = MockContextBudget::new("2000", "0", "0");
    let mut high = request(frozen, reads);
    high.budget = MockContextBudget::new("4000", "0", "0");
    let low = compile(low);
    let high = compile(high);
    let low_extra_blocks = low.messages[1].content.matches("useful detail").count();
    let high_extra_blocks = high.messages[1].content.matches("useful detail").count();
    assert!(high_extra_blocks >= low_extra_blocks);
    assert!(high.receipt.source_handles.len() >= low.receipt.source_handles.len());
}

#[test]
fn invalid_decimal_budget_is_structured_and_author_room_revise_is_forbidden() {
    let target_body = body(&[("target-1", "Target text.")]);
    let target = source("target", "target-doc", &target_body);
    let frozen = frozen(
        vec![target.clone()],
        ContextPurpose::Revise,
        Audience::AuthorRoom,
    );
    let mut request = request(frozen, vec![read(&target, &target_body)]);
    request.budget.context_window_tokens = "100000".into();
    let error = compile_packet(&request).expect_err("author-room revise must be rejected");
    assert!(
        matches!(error, PacketError::Eligibility(error) if error.code == webnovel_core::context::EligibilityErrorCode::InvalidPolicy)
    );
}

#[test]
fn packet_message_and_options_serialization_is_stable() {
    let messages = vec![PacketMessage {
        role: "system".into(),
        content: "x".into(),
    }];
    let options = PacketOptions {
        model_id: "mock-story-context".into(),
        max_output_tokens: "10".into(),
        token_accounting_method: "utf8-byte-count/mock-story-context-v1".into(),
    };
    let first = serialized_input(&messages, &options).unwrap();
    let second = serialized_input(&messages, &options).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        packet_input_hash(&messages, &options).unwrap(),
        packet_input_hash(&messages, &options).unwrap()
    );
}
