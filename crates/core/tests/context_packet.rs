use serde_json::{Value, json};
use std::collections::BTreeMap;
use webnovel_core::context::packet::{
    CONTINUATION_RESPONSE_CONTRACT, CompiledPacket, MockContextBudget, PROPOSAL_RESPONSE_CONTRACT,
    PacketError, PacketMessage, PacketOptions, PacketRequest, ProviderBinding, compile_packet,
    packet_input_hash, serialized_input,
};
use webnovel_core::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    SafeBriefInput, SourceDescriptor, SourceKind, SourceRef, StorySnapshot,
};
use webnovel_core::documents::{
    Endpoint, ScopeGrant, ScopeKind, capture_append_scope, capture_scope,
};
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
        reviewed_basis: None,
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
        navigation_views: Vec::new(),
        reviewed_evidence: Vec::new(),
        reviewed_promises: Vec::new(),
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
        lookup: None,
        packet_id: "packet-1".into(),
        session_id: "session-1".into(),
        invocation_ordinal: "1".into(),
        frozen,
        instruction: "Keep the ending exactly as written.".into(),
        sources: reads,
        mandatory_handles: Vec::new(),
        scope: None,
        safe_brief: None,
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
        response_contract: None,
    }
}

fn passage_scope(body: &Value) -> ScopeGrant {
    capture_scope(
        body,
        ScopeGrant {
            kind: ScopeKind::Passage,
            start: Some(Endpoint {
                block_id: "target-1".into(),
                utf16_offset: 0,
            }),
            end: Some(Endpoint {
                block_id: "target-1".into(),
                utf16_offset: 8,
            }),
            source_hash: String::new(),
            quote: String::new(),
            quote_hash: String::new(),
            prefix: None,
            suffix: None,
        },
    )
    .expect("capture exact passage scope")
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
fn safe_brief_compiler_projects_exact_text_receipt_and_restricted_instruction() {
    use sha2::{Digest, Sha256};

    let target_body = body(&[("target-1", "A selected passage.")]);
    let target = source("target", "target-doc", &target_body);
    let mut request = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Revise,
            Audience::RestrictedWriting,
        ),
        vec![read(&target, &target_body)],
    );
    request.scope = Some(passage_scope(&target_body));
    let text = "Keep the exchange restrained and preserve the exact selected passage.";
    request.safe_brief = Some(SafeBriefInput {
        text: text.into(),
        origin_message_id: None,
        confirmed: true,
    });

    let packet = compile(request);
    let receipt = packet
        .receipt
        .safe_brief
        .as_ref()
        .expect("safe brief receipt");
    assert_eq!(receipt.text, text);
    assert_eq!(
        receipt.text_hash,
        Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    assert!(receipt.origin_message_id.is_none());
    let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
    assert_eq!(envelope["approvedWritingBrief"], text);
    assert!(envelope.get("originMessageId").is_none());
    assert!(packet.messages[0].content.contains("approvedWritingBrief"));
    assert!(packet.messages[0].content.contains("final author request"));
    assert!(
        packet.messages[0]
            .content
            .contains("exact selected passage scope")
    );
    assert_eq!(
        packet.receipt.input_hash,
        packet_input_hash(&packet.messages, &packet.options).unwrap()
    );
}

#[test]
fn safe_brief_compiler_refuses_wrong_policy_scope_empty_unconfirmed_and_oversized_input() {
    let target_body = body(&[("target-1", "A selected passage.")]);
    let target = source("target", "target-doc", &target_body);
    let mut valid = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Revise,
            Audience::RestrictedWriting,
        ),
        vec![read(&target, &target_body)],
    );
    valid.scope = Some(passage_scope(&target_body));
    valid.safe_brief = Some(SafeBriefInput {
        text: "Keep the scene quiet.".into(),
        origin_message_id: None,
        confirmed: true,
    });

    let mut wrong_audience = valid.clone();
    wrong_audience.frozen.policy.audience = Audience::AuthorRoom;
    let mut wrong_scope = valid.clone();
    wrong_scope.scope = None;
    let mut empty = valid.clone();
    empty.safe_brief.as_mut().unwrap().text.clear();
    let mut unconfirmed = valid.clone();
    unconfirmed.safe_brief.as_mut().unwrap().confirmed = false;
    let mut oversized = valid;
    oversized.safe_brief.as_mut().unwrap().text = "x".repeat(16 * 1024 + 1);

    for (label, candidate) in [
        ("wrong audience", wrong_audience),
        ("wrong scope", wrong_scope),
        ("empty", empty),
        ("unconfirmed", unconfirmed),
        ("oversized", oversized),
    ] {
        assert!(
            matches!(
                compile_packet(&candidate),
                Err(PacketError::InvalidRequest { .. })
            ),
            "safe brief case {label} must fail before packet projection"
        );
    }
}

#[test]
fn safe_brief_counts_as_mandatory_context_and_never_truncates() {
    let target_body = body(&[("target-1", "A selected passage.")]);
    let target = source("target", "target-doc", &target_body);
    let mut baseline = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Revise,
            Audience::RestrictedWriting,
        ),
        vec![read(&target, &target_body)],
    );
    baseline.scope = Some(passage_scope(&target_body));
    baseline.budget = MockContextBudget::new("100000", "0", "0");
    let baseline_packet = compile(baseline.clone());
    let baseline_tokens = baseline_packet
        .receipt
        .input_tokens
        .parse::<usize>()
        .unwrap();

    baseline.safe_brief = Some(SafeBriefInput {
        text: "A".repeat(512),
        origin_message_id: None,
        confirmed: true,
    });
    baseline.budget = MockContextBudget::new(baseline_tokens.to_string(), "0", "0");
    match compile_packet(&baseline).expect_err("brief must not be silently truncated") {
        PacketError::Budget(error) => {
            assert_eq!(
                error.code,
                webnovel_core::context::BudgetErrorCode::MandatoryContextTooLarge
            );
            assert!(error.mandatory_handles.contains(&"target".into()));
            assert!(error.required_input_tokens.parse::<usize>().unwrap() > baseline_tokens);
        }
        error => panic!("expected mandatory brief budget error, got {error}"),
    }
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
fn codex_binding_uses_exact_utf8_byte_packet_and_keeps_model_limit_unknown() {
    let target_body = body(&[("target-1", "A short scene for the bounded live packet.")]);
    let target = source("target", "target-doc", &target_body);
    let mut request = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Discuss,
            Audience::AuthorRoom,
        ),
        vec![read(&target, &target_body)],
    );
    request.provider_binding = Some(ProviderBinding::codex_luna());
    let packet = compile(request).clone();
    let input = serialized_input(&packet.messages, &packet.options).unwrap();
    assert_eq!(packet.receipt.input_tokens, input.len().to_string());
    assert_eq!(packet.options.model_id, "gpt-5.6-luna");
    assert_eq!(packet.options.max_output_tokens, "");
    assert_eq!(
        packet
            .options
            .provider_binding
            .as_ref()
            .unwrap()
            .output_limit_bytes,
        "65536"
    );
    let serialized = serde_json::to_string(&packet.options).unwrap();
    assert!(!serialized.contains("maxOutputTokens"));
}

#[test]
fn current_and_historical_codex_bindings_are_exact_and_history_round_trips() {
    let current = ProviderBinding::codex_luna();
    let historical = ProviderBinding::codex_luna_historical();
    assert_eq!(current.profile_version, "codex-stdin.v1");
    assert_eq!(current.reasoning.as_deref(), Some("xhigh"));
    assert_eq!(historical.reasoning.as_deref(), Some("max"));
    assert_eq!(historical.profile_version, "0.153.3");
    assert_ne!(current, historical);
    assert!(current.validate().is_ok());
    assert!(historical.validate().is_ok());

    let encoded = serde_json::to_string(&historical).unwrap();
    assert!(!encoded.contains("runtime"));
    let decoded: ProviderBinding = serde_json::from_str(&encoded).unwrap();
    assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);

    let mut forged = historical.clone();
    forged.model_id = "gpt-5.6-astra".into();
    assert!(forged.validate().is_err());
    forged = historical.clone();
    forged.profile_version = "0.153.5".into();
    assert!(forged.validate().is_err());
}

#[test]
fn codex_runtime_versions_are_recorded_without_a_release_allowlist() {
    for version in ["0.153.4", "0.154.0", "1.4.0-beta.2"] {
        let binding = ProviderBinding::codex_luna_runtime(version, &"a".repeat(64));
        assert!(binding.validate().is_ok());
        assert!(binding.is_current_codex_profile());
        let encoded = serde_json::to_string(&binding).unwrap();
        assert!(encoded.contains(version));
        assert_eq!(
            serde_json::from_str::<ProviderBinding>(&encoded).unwrap(),
            binding
        );
    }
    assert!(
        ProviderBinding::codex_luna_runtime("0.154.0\nignored", &"a".repeat(64))
            .validate()
            .is_err()
    );
    assert!(
        ProviderBinding::codex_luna_runtime("0.154.0", "not-a-fingerprint")
            .validate()
            .is_err()
    );
    let mut forged = ProviderBinding::codex_luna_runtime("0.154.0", &"a".repeat(64));
    forged.profile_version = "0.153.3".into();
    forged.reasoning = Some("max".into());
    assert!(forged.validate().is_err());
}

#[test]
fn historical_codex_packet_input_and_hash_survive_reopen_serialization() {
    let target_body = body(&[("target-1", "A historical packet must remain readable.")]);
    let target = source("target", "target-doc", &target_body);
    let mut request = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Discuss,
            Audience::AuthorRoom,
        ),
        vec![read(&target, &target_body)],
    );
    request.provider_binding = Some(ProviderBinding::codex_luna_historical());
    let packet = compile(request);
    let original_input = serialized_input(&packet.messages, &packet.options).unwrap();
    let original_hash = packet_input_hash(&packet.messages, &packet.options).unwrap();

    let reopened_messages: Vec<PacketMessage> =
        serde_json::from_str(&serde_json::to_string(&packet.messages).unwrap()).unwrap();
    let reopened_options: PacketOptions =
        serde_json::from_str(&serde_json::to_string(&packet.options).unwrap()).unwrap();
    assert_eq!(
        serialized_input(&reopened_messages, &reopened_options).unwrap(),
        original_input
    );
    assert_eq!(
        packet_input_hash(&reopened_messages, &reopened_options).unwrap(),
        original_hash
    );
    assert_eq!(
        reopened_options.provider_binding,
        Some(ProviderBinding::codex_luna_historical())
    );
}

#[test]
fn codex_binding_rejects_mandatory_context_over_application_input_cap() {
    let target_body = body(&[("target-1", &"A".repeat(40_000))]);
    let target = source("target", "target-doc", &target_body);
    let mut request = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Discuss,
            Audience::AuthorRoom,
        ),
        vec![read(&target, &target_body)],
    );
    request.provider_binding = Some(ProviderBinding::codex_luna());
    let error = compile_packet(&request).expect_err("oversized mandatory target");
    assert!(
        matches!(error, PacketError::Budget(error) if error.code == webnovel_core::context::BudgetErrorCode::MandatoryContextTooLarge)
    );
}

#[test]
fn live_proposal_contract_is_frozen_into_system_message_and_preserves_author_bytes() {
    let target_body = body(&[("target-1", "A selected passage for revision.")]);
    let target = source("target", "target-doc", &target_body);
    let instruction = "Keep the author's exact wording constraints: use a quieter ending.";
    let mut request = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Revise,
            Audience::RestrictedWriting,
        ),
        vec![read(&target, &target_body)],
    );
    request.instruction = instruction.into();
    request.scope = Some(passage_scope(&target_body));
    request.provider_binding = Some(ProviderBinding::codex_luna());
    request.response_contract = Some(PROPOSAL_RESPONSE_CONTRACT.into());

    let with_contract = compile(request.clone());
    assert!(
        with_contract.messages[0]
            .content
            .contains(PROPOSAL_RESPONSE_CONTRACT)
    );
    assert!(
        with_contract.messages[0]
            .content
            .contains("replacementText")
    );
    assert_eq!(with_contract.messages[2].content, instruction);

    request.response_contract = None;
    let without_contract = compile(request);
    assert!(
        !without_contract.messages[0]
            .content
            .contains(PROPOSAL_RESPONSE_CONTRACT)
    );
    assert_ne!(
        with_contract.receipt.input_hash,
        without_contract.receipt.input_hash
    );
}

#[test]
fn proposal_contract_rejects_mock_or_unscoped_requests() {
    let target_body = body(&[("target-1", "A selected passage.")]);
    let target = source("target", "target-doc", &target_body);
    let mut request = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Revise,
            Audience::RestrictedWriting,
        ),
        vec![read(&target, &target_body)],
    );
    request.response_contract = Some(PROPOSAL_RESPONSE_CONTRACT.into());
    assert!(matches!(
        compile_packet(&request),
        Err(PacketError::InvalidRequest { .. })
    ));

    request.provider_binding = Some(ProviderBinding::codex_luna());
    request.scope = None;
    assert!(matches!(
        compile_packet(&request),
        Err(PacketError::InvalidRequest { .. })
    ));
}

#[test]
fn continuation_contract_is_frozen_into_mock_packet_instruction() {
    let target_body = body(&[("target-1", "The exact chapter ending.")]);
    let target = source("target", "target-doc", &target_body);
    let mut request = request(
        frozen(
            vec![target.clone()],
            ContextPurpose::Continue,
            Audience::RestrictedWriting,
        ),
        vec![read(&target, &target_body)],
    );
    request.scope = Some(capture_append_scope(&target_body).unwrap());
    request.response_contract = Some(CONTINUATION_RESPONSE_CONTRACT.into());
    request.instruction = "Continue the chapter with a quiet reveal.".into();

    let packet = compile(request.clone());
    assert!(
        packet.messages[0]
            .content
            .contains(CONTINUATION_RESPONSE_CONTRACT)
    );
    assert!(packet.messages[0].content.contains("paragraphs"));
    assert_eq!(packet.messages[2].content, request.instruction);

    let mut wrong_scope = request.clone();
    wrong_scope.scope = Some(passage_scope(&target_body));
    assert!(matches!(
        compile_packet(&wrong_scope),
        Err(PacketError::InvalidRequest { .. })
    ));
    let mut wrong_purpose = request;
    wrong_purpose.frozen.purpose = ContextPurpose::Revise;
    assert!(matches!(
        compile_packet(&wrong_purpose),
        Err(PacketError::InvalidRequest { .. })
    ));
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
        provider_binding: None,
    };
    let first = serialized_input(&messages, &options).unwrap();
    let second = serialized_input(&messages, &options).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        packet_input_hash(&messages, &options).unwrap(),
        packet_input_hash(&messages, &options).unwrap()
    );
}

mod lookup_packets {
    use super::*;
    use webnovel_core::context::lookup::{
        LOOKUP_SOURCE_PROJECTION_SCHEMA, LookupAllowance, LookupExchange, LookupPacketInput,
        LookupRead, LookupReadResult, LookupSourceProjection,
    };
    use webnovel_core::context::packet::LOOKUP_RESPONSE_CONTRACT;
    use webnovel_core::projects::story_context::{SearchHit, SearchMode, SearchResult};

    fn initial() -> PacketRequest {
        let target_body = body(&[("target-1", "Keep this ending.")]);
        let old_body = body(&[
            ("old-1", "Mei promised to return the brass key."),
            ("old-2", "Her brother waited."),
        ]);
        let target = source("target", "target-doc", &target_body);
        let old = source("old", "old-doc", &old_body);
        let mut req = request(
            frozen(
                vec![target.clone(), old.clone()],
                ContextPurpose::Discuss,
                Audience::AuthorRoom,
            ),
            vec![read(&target, &target_body), read(&old, &old_body)],
        );
        req.invocation_ordinal = "0".into();
        req.response_contract = Some(LOOKUP_RESPONSE_CONTRACT.into());
        req.lookup = Some(LookupPacketInput {
            allowance: LookupAllowance::default(),
            completed_invocations: 0,
            exchanges: vec![],
            source_projection: None,
        });
        req
    }

    fn expanded() -> PacketRequest {
        let mut req = initial();
        req.invocation_ordinal = "1".into();
        let old = &req.sources[1];
        req.lookup.as_mut().unwrap().completed_invocations = 1;
        req.lookup.as_mut().unwrap().exchanges.push(LookupExchange {
            request: LookupRead::Read {
                id: "read-1".into(),
                handle: "old".into(),
                block_ids: Some(vec!["old-1".into()]),
            },
            result: LookupReadResult::Read {
                handle: "old".into(),
                source: old.descriptor.source.clone(),
                passages: vec![old.passages[0].clone()],
                complete: false,
            },
        });
        req
    }

    fn projected(req: &PacketRequest) -> LookupSourceProjection {
        LookupSourceProjection::from_exchanges(
            &req.frozen,
            &req.lookup.as_ref().expect("lookup input").exchanges,
        )
        .expect("frozen lookup evidence projects")
    }

    #[test]
    fn explicit_lookup_contract_preserves_instruction_and_ordinary_wire_shape() {
        let req = initial();
        let packet = compile_packet(&req).unwrap();
        assert_eq!(packet.messages.last().unwrap().content, req.instruction);
        assert_eq!(packet.receipt.lookup, req.lookup);
        assert!(packet.messages[0].content.contains("story-lookup.v1"));
        let envelope: Value = serde_json::from_str(&packet.messages[1].content).unwrap();
        assert_eq!(envelope["lookup"]["completedInvocations"], 0);
        assert!(envelope["lookup"].get("sourceProjection").is_none());
        let mut ordinary = req;
        ordinary.lookup = None;
        ordinary.response_contract = None;
        let ordinary = compile_packet(&ordinary).unwrap();
        assert!(
            !serde_json::to_value(&ordinary.receipt)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("lookup")
        );
        assert!(!ordinary.messages[1].content.contains("\"lookup\""));
        assert!(!ordinary.messages[0].content.contains("story-lookup.v1"));
    }

    #[test]
    fn old_lookup_evidence_is_mandatory_even_when_optional_prefix_cannot_fit() {
        let mut req = expanded();
        let filler = "Unrelated scenery. ".repeat(1800);
        let filler_body = body(&[("filler-1", &filler)]);
        let descriptor = source("filler", "filler-doc", &filler_body);
        req.frozen.snapshot.sources.insert(1, descriptor.clone());
        req.sources.insert(1, read(&descriptor, &filler_body));
        let packet = compile_packet(&req).unwrap();
        assert_eq!(packet.receipt.lookup, req.lookup);
        assert!(
            packet.messages[1]
                .content
                .contains("Mei promised to return the brass key.")
        );
        assert!(!packet.messages[1].content.contains(&filler));
        assert!(
            packet
                .receipt
                .source_handles
                .iter()
                .any(|handle| handle == "target")
        );
        assert_eq!(packet.messages.last().unwrap().content, req.instruction);

        req.lookup.as_mut().unwrap().allowance.total_input_bytes = "100".into();
        assert!(matches!(compile_packet(&req), Err(PacketError::Budget(_))));
    }

    #[test]
    fn altered_passages_identity_order_and_completeness_are_refused() {
        let req = expanded();
        compile_packet(&req).unwrap();
        for mutation in 0..7 {
            let mut changed = req.clone();
            let LookupReadResult::Read {
                handle,
                source,
                passages,
                complete,
            } = &mut changed.lookup.as_mut().unwrap().exchanges[0].result
            else {
                unreachable!()
            };
            match mutation {
                0 => passages[0].text.push_str(" The promise was fulfilled."),
                1 => source.revision_id = "other-revision".into(),
                2 => *handle = "other-project-source".into(),
                3 => passages[0].block_order += 1,
                4 => *complete = true,
                5 => passages.clear(),
                _ => passages.push(passages[0].clone()),
            }
            assert!(
                matches!(compile_packet(&changed), Err(PacketError::SourceBinding { code, .. }) if code == "InvalidLookupEvidence"),
                "mutation {mutation}"
            );
        }
    }

    #[test]
    fn new_lookup_projection_carries_exact_read_and_search_titles() {
        let mut read_request = expanded();
        let read_projection_input = projected(&read_request);
        read_request.lookup.as_mut().unwrap().source_projection = Some(read_projection_input);
        let read_packet = compile_packet(&read_request).expect("read projection compiles");
        let read_projection = read_packet
            .receipt
            .lookup
            .as_ref()
            .and_then(|lookup| lookup.source_projection.as_ref())
            .expect("read projection receipt");
        assert_eq!(
            read_projection.schema_version,
            LOOKUP_SOURCE_PROJECTION_SCHEMA
        );
        assert_eq!(read_projection.sources.len(), 1);
        assert_eq!(read_projection.sources[0].handle, "old");
        assert_eq!(
            read_projection.sources[0].source,
            read_request.sources[1].descriptor.source
        );
        assert_eq!(read_projection.sources[0].display_name, "Private title old");
        let read_envelope: Value = serde_json::from_str(&read_packet.messages[1].content).unwrap();
        assert_eq!(
            read_envelope["lookup"]["sourceProjection"]["sources"][0]["displayName"],
            "Private title old"
        );

        let mut search_request = expanded();
        let old_passage = search_request.sources[1].passages[0].clone();
        let start = old_passage.text.find("brass key").expect("search phrase");
        search_request.lookup.as_mut().unwrap().exchanges = vec![LookupExchange {
            request: LookupRead::Search {
                id: "search-1".into(),
                query: "brass key".into(),
                mode: SearchMode::Literal,
                limit: 3,
            },
            result: LookupReadResult::Search {
                result: SearchResult {
                    snapshot_id: search_request.frozen.snapshot.snapshot_id.clone(),
                    hits: vec![SearchHit {
                        passage: old_passage,
                        start_utf16: start as u32,
                        end_utf16: (start + "brass key".len()) as u32,
                    }],
                    source_matches: vec![],
                    searched_sources: 2,
                    has_more: false,
                    coverage: "Exact frozen source match.".into(),
                },
            },
        }];
        let search_projection_input = projected(&search_request);
        search_request.lookup.as_mut().unwrap().source_projection = Some(search_projection_input);
        let search_packet = compile_packet(&search_request).expect("search projection compiles");
        assert_eq!(
            search_packet
                .receipt
                .lookup
                .as_ref()
                .unwrap()
                .source_projection
                .as_ref()
                .unwrap()
                .sources[0]
                .display_name,
            "Private title old"
        );
    }

    #[test]
    fn lookup_projection_rejects_tampered_missing_extra_duplicate_and_restricted_labels() {
        let valid = expanded();
        let expected = projected(&valid);
        let mut candidates = Vec::new();

        let mut missing = valid.clone();
        missing.lookup.as_mut().unwrap().source_projection = Some(LookupSourceProjection {
            schema_version: LOOKUP_SOURCE_PROJECTION_SCHEMA.into(),
            sources: Vec::new(),
        });
        candidates.push(missing);

        let mut extra = valid.clone();
        let mut extra_projection = expected.clone();
        let mut extra_source = extra_projection.sources[0].clone();
        extra_source.handle = "target".into();
        extra_source.source = extra.frozen.snapshot.target.clone();
        extra_source.display_name = "Private title target".into();
        extra_projection.sources.push(extra_source);
        extra.lookup.as_mut().unwrap().source_projection = Some(extra_projection);
        candidates.push(extra);

        let duplicate = valid.clone();
        let mut duplicate_projection = expected.clone();
        duplicate_projection
            .sources
            .push(duplicate_projection.sources[0].clone());
        let mut duplicate = duplicate;
        duplicate.lookup.as_mut().unwrap().source_projection = Some(duplicate_projection);
        candidates.push(duplicate);

        let mut renamed = valid.clone();
        let mut renamed_projection = expected.clone();
        renamed_projection.sources[0].display_name = "Invented title".into();
        renamed.lookup.as_mut().unwrap().source_projection = Some(renamed_projection);
        candidates.push(renamed);

        let mut wrong_source = valid.clone();
        let mut wrong_source_projection = expected.clone();
        wrong_source_projection.sources[0].source.revision_id = "other-revision".into();
        wrong_source.lookup.as_mut().unwrap().source_projection = Some(wrong_source_projection);
        candidates.push(wrong_source);

        for candidate in candidates {
            assert!(
                compile_packet(&candidate).is_err(),
                "tampered source projection accepted"
            );
        }

        let mut restricted = valid;
        restricted.frozen.policy.audience = Audience::RestrictedWriting;
        restricted.lookup.as_mut().unwrap().source_projection = Some(expected);
        assert!(compile_packet(&restricted).is_err());
    }

    #[test]
    fn historical_lookup_without_projection_round_trips_exact_bytes() {
        let request = expanded();
        let packet = compile_packet(&request).expect("historical lookup packet compiles");
        let packet_bytes = serde_json::to_vec(&packet).expect("serialize packet");
        let round_tripped: CompiledPacket =
            serde_json::from_slice(&packet_bytes).expect("reopen packet");
        assert_eq!(serde_json::to_vec(&round_tripped).unwrap(), packet_bytes);
        assert!(
            round_tripped
                .receipt
                .lookup
                .as_ref()
                .unwrap()
                .source_projection
                .is_none()
        );
        assert!(!String::from_utf8_lossy(&packet_bytes).contains("sourceProjection"));
    }

    #[test]
    fn author_room_discussion_authorization_cannot_expand_to_writing_or_extra_calls() {
        let req = expanded();
        for mutation in 0..7 {
            let mut changed = req.clone();
            match mutation {
                0 => changed.response_contract = None,
                1 => changed.frozen.policy.audience = Audience::RestrictedWriting,
                2 => changed.frozen.purpose = ContextPurpose::Continue,
                3 => changed.invocation_ordinal = "2".into(),
                4 => {
                    changed
                        .lookup
                        .as_mut()
                        .unwrap()
                        .allowance
                        .max_additional_invocations = 0
                }
                5 => {
                    let duplicate = changed.lookup.as_ref().unwrap().exchanges[0].clone();
                    changed.lookup.as_mut().unwrap().exchanges.push(duplicate);
                }
                _ => changed.lookup.as_mut().unwrap().exchanges.clear(),
            }
            assert!(compile_packet(&changed).is_err(), "mutation {mutation}");
        }
    }

    #[test]
    fn search_hits_are_bound_to_frozen_text_and_utf16_boundaries() {
        let mut req = expanded();
        let unicode_body = body(&[("unicode-1", "🔑 Mei kept the key.")]);
        let unicode = source("unicode", "unicode-doc", &unicode_body);
        let unicode_read = read(&unicode, &unicode_body);
        req.frozen.snapshot.sources.push(unicode);
        req.sources.push(unicode_read.clone());
        req.lookup.as_mut().unwrap().exchanges = vec![LookupExchange {
            request: LookupRead::Search { id: "search-1".into(), query: "🔑".into(), mode: SearchMode::Literal, limit: 3 },
            result: LookupReadResult::Search { result: SearchResult {
                snapshot_id: req.frozen.snapshot.snapshot_id.clone(),
                hits: vec![SearchHit { passage: unicode_read.passages[0].clone(), start_utf16: 0, end_utf16: 2 }],
                source_matches: vec![], searched_sources: 3, has_more: false,
                coverage: "Searched the frozen sources; absence does not prove an event never occurred.".into(),
            } },
        }];
        compile_packet(&req).unwrap();
        for mutation in 0..9 {
            let mut changed = req.clone();
            let LookupReadResult::Search { result } =
                &mut changed.lookup.as_mut().unwrap().exchanges[0].result
            else {
                unreachable!()
            };
            match mutation {
                0 => result.hits[0].end_utf16 = 1,
                1 => result.hits[0].passage.text = "invented".into(),
                2 => result.snapshot_id = "other-snapshot".into(),
                3 => result.searched_sources = 2,
                4 => result.hits.push(result.hits[0].clone()),
                5 => {
                    result.hits[0].start_utf16 = 3;
                    result.hits[0].end_utf16 = 6;
                }
                6 => result.has_more = true,
                7 => result.hits.clear(),
                _ => {
                    let LookupRead::Search { query, .. } =
                        &mut changed.lookup.as_mut().unwrap().exchanges[0].request
                    else {
                        unreachable!()
                    };
                    *query = "missing pendant".into();
                }
            }
            assert!(compile_packet(&changed).is_err(), "mutation {mutation}");
        }
    }

    #[test]
    fn source_alias_lookup_rejects_unmatched_names_and_wrong_result_kinds() {
        let mut req = expanded();
        let old = req.sources[1].descriptor.clone();
        req.frozen
            .aliases
            .insert(old.handle.clone(), vec!["The key vow".into()]);
        req.lookup.as_mut().unwrap().exchanges = vec![LookupExchange {
            request: LookupRead::Search {
                id: "alias-1".into(),
                query: "the KEY vow".into(),
                mode: SearchMode::ExactAlias,
                limit: 1,
            },
            result: LookupReadResult::Search {
                result: SearchResult {
                    snapshot_id: req.frozen.snapshot.snapshot_id.clone(),
                    hits: vec![],
                    source_matches: vec![old],
                    searched_sources: 2,
                    has_more: false,
                    coverage: "Exact source aliases.".into(),
                },
            },
        }];
        compile_packet(&req).unwrap();
        for mutation in 0..3 {
            let mut changed = req.clone();
            match mutation {
                0 => changed.frozen.aliases.clear(),
                1 => {
                    let LookupRead::Search { mode, .. } =
                        &mut changed.lookup.as_mut().unwrap().exchanges[0].request
                    else {
                        unreachable!()
                    };
                    *mode = SearchMode::Literal;
                }
                _ => {
                    let LookupReadResult::Search { result } =
                        &mut changed.lookup.as_mut().unwrap().exchanges[0].result
                    else {
                        unreachable!()
                    };
                    result.source_matches.clear();
                }
            }
            assert!(compile_packet(&changed).is_err(), "mutation {mutation}");
        }
    }
}
