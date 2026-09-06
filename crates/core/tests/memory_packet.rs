use serde_json::{Value, json};
use std::collections::BTreeMap;
use webnovel_core::context::packet::{
    HTTP_MEMORY_INPUT_LIMIT_BYTES, HTTP_MEMORY_MODEL_ID, HTTP_MEMORY_OUTPUT_LIMIT_BYTES,
    HTTP_MEMORY_PROFILE_VERSION, HTTP_MEMORY_REASONING, HTTP_TOKEN_ACCOUNTING_METHOD,
    HttpProviderBinding, HttpResponseFormat, MEMORY_RESPONSE_CONTRACT, MockContextBudget,
    PacketError, PacketRequest, ProviderBinding, compile_packet, packet_input_hash,
    serialized_input,
};
use webnovel_core::context::{
    Audience, BasisKind, ContextPurpose, CoverageLabel, Disclosure, InformationPolicy,
    SafeBriefInput, SourceDescriptor, SourceKind, SourceRef, StorySnapshot,
};
use webnovel_core::projects::story_context::{FrozenContext, SourcePassage, SourceRead};
use webnovel_core::validate_snapshot_json;

const PROJECT: &str = "memory-packet-project";

fn body(text: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "body": {
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "attrs": {"id": "chapter-block"},
                "content": [{"type": "text", "text": text}]
            }]
        }
    })
}

fn source(body: &Value, kind: SourceKind) -> SourceDescriptor {
    let hash = validate_snapshot_json(&serde_json::to_string(body).unwrap())
        .unwrap()
        .hash;
    SourceDescriptor {
        handle: "chapter".into(),
        source: SourceRef {
            project_id: PROJECT.into(),
            document_id: "chapter-1".into(),
            revision_id: "revision-1".into(),
            body_hash: hash,
        },
        display_name: "Chapter 1".into(),
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

fn read(descriptor: &SourceDescriptor, body: &Value) -> SourceRead {
    let canonical = validate_snapshot_json(&serde_json::to_string(body).unwrap())
        .unwrap()
        .snapshot;
    SourceRead {
        descriptor: descriptor.clone(),
        passages: vec![SourcePassage {
            handle: descriptor.handle.clone(),
            source: descriptor.source.clone(),
            block_id: "chapter-block".into(),
            block_order: 0,
            text: canonical["body"]["content"][0]["content"][0]["text"]
                .as_str()
                .unwrap()
                .into(),
        }],
        body: canonical,
        used_validated_projection: true,
    }
}

fn frozen(descriptors: Vec<SourceDescriptor>) -> FrozenContext {
    let target = descriptors[0].source.clone();
    FrozenContext {
        snapshot: StorySnapshot {
            snapshot_id: "memory-snapshot".into(),
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
            audience: Audience::AuthorRoom,
            reader_frontier: None,
            character_id: None,
            character_grants: Vec::new(),
            allow_alternatives: false,
            allow_historical: false,
        },
        purpose: ContextPurpose::MemoryAnalysis,
        aliases: BTreeMap::new(),
        excluded_source_count: 0,
        guidance: Vec::new(),
        conversation: None,
        navigation_views: Vec::new(),
        reviewed_evidence: Vec::new(),
        reviewed_promises: Vec::new(),
    }
}

fn valid_request(text: &str) -> PacketRequest {
    let body = body(text);
    let descriptor = source(&body, SourceKind::CurrentDraft);
    PacketRequest {
        lookup: None,
        packet_id: "memory-packet-1".into(),
        session_id: "memory-session-1".into(),
        invocation_ordinal: "1".into(),
        frozen: frozen(vec![descriptor.clone()]),
        instruction: "Build navigation only from this saved chapter.".into(),
        sources: vec![read(&descriptor, &body)],
        mandatory_handles: Vec::new(),
        scope: None,
        safe_brief: None,
        budget: MockContextBudget::new("100000", "100", "100"),
        provider_binding: None,
        response_contract: Some(MEMORY_RESPONSE_CONTRACT.into()),
    }
}

fn invalid_request(request: PacketRequest) {
    assert!(matches!(
        compile_packet(&request),
        Err(PacketError::InvalidRequest { .. })
    ));
}

fn http_memory_binding() -> ProviderBinding {
    ProviderBinding {
        provider_id: "openai-compatible:00000000-0000-0000-0000-000000000001".into(),
        model_id: HTTP_MEMORY_MODEL_ID.into(),
        reasoning: Some(HTTP_MEMORY_REASONING.into()),
        service_tier: None,
        profile_version: HTTP_MEMORY_PROFILE_VERSION.into(),
        input_limit_bytes: HTTP_MEMORY_INPUT_LIMIT_BYTES.to_string(),
        reserved_output_bytes: "0".into(),
        reserved_protocol_bytes: "0".into(),
        output_limit_bytes: HTTP_MEMORY_OUTPUT_LIMIT_BYTES.to_string(),
        accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.into(),
        runtime: None,
        http: Some(HttpProviderBinding {
            base_url: "https://example.test/v1".into(),
            config_revision: "1".into(),
            stream: true,
            response_format: HttpResponseFormat::Text,
        }),
    }
}

#[test]
fn memory_packet_is_exactly_one_chapter_and_stable() {
    let packet = compile_packet(&valid_request("An exact saved chapter.")).expect("compile");
    assert_eq!(packet.messages.len(), 3);
    assert_eq!(
        packet.messages[2].content,
        "Build navigation only from this saved chapter."
    );
    assert!(packet.messages[0].content.contains("navigation-digest.v1"));
    assert!(
        packet.messages[0]
            .content
            .contains("unreviewed generated navigation aid")
    );
    assert_eq!(packet.receipt.source_handles, vec!["chapter"]);
    assert!(packet.receipt.mandatory_source_handles.is_empty());
    let serialized = serialized_input(&packet.messages, &packet.options).expect("serialize");
    assert_eq!(
        packet.receipt.input_hash,
        packet_input_hash(&packet.messages, &packet.options).expect("hash")
    );
    assert_eq!(packet.receipt.input_tokens, serialized.len().to_string());
    assert!(!serialized.contains("proposal-output.v1"));
}

#[test]
fn extra_private_or_derived_sources_are_refused() {
    let chapter_body = body("Only the current chapter is allowed.");
    let chapter = source(&chapter_body, SourceKind::CurrentDraft);
    let mut request = valid_request("Only the current chapter is allowed.");
    let private_body = body("Private future note.");
    let private = source(&private_body, SourceKind::PrivateFuture);
    request.frozen = frozen(vec![chapter.clone(), private.clone()]);
    request.sources = vec![read(&chapter, &chapter_body), read(&private, &private_body)];
    assert!(matches!(
        compile_packet(&request),
        Err(PacketError::Eligibility(_))
    ));

    let derived_body = body("Derived observation.");
    let derived = source(&derived_body, SourceKind::GeneratedDigest);
    let mut request = valid_request("Derived observation.");
    request.frozen = frozen(vec![derived.clone()]);
    request.sources = vec![read(&derived, &derived_body)];
    assert!(matches!(
        compile_packet(&request),
        Err(PacketError::Eligibility(_))
    ));
}

#[test]
fn memory_recipe_rejects_wrong_contract_and_authority_inputs() {
    let mut request = valid_request("No extra authority.");
    request.response_contract = None;
    invalid_request(request);

    let mut request = valid_request("No extra authority.");
    request.mandatory_handles.push("chapter".into());
    invalid_request(request);

    let mut request = valid_request("No extra authority.");
    request.safe_brief = Some(SafeBriefInput {
        text: "Rewrite it".into(),
        origin_message_id: None,
        confirmed: true,
    });
    invalid_request(request);

    let mut request = valid_request("No extra authority.");
    request
        .frozen
        .aliases
        .insert("hero".into(), vec!["H".into()]);
    invalid_request(request);

    let mut request = valid_request("No extra authority.");
    request
        .frozen
        .guidance
        .push(webnovel_core::context::guidance::FrozenGuidance {
            handle: "g".into(),
            project_id: PROJECT.into(),
            version: webnovel_core::context::guidance::GuidanceVersion {
                guidance_id: "g".into(),
                version_id: "g1".into(),
                version: "1".into(),
                scope: webnovel_core::context::guidance::GuidanceScope::Project,
                document_id: None,
                text: "Do not rewrite.".into(),
                text_hash: "0".repeat(64),
                active: true,
                origin_message_id: None,
                created_at: "2026-09-06T00:00:00Z".into(),
            },
        });
    invalid_request(request);
}

#[test]
fn memory_recipe_rejects_conversation_context() {
    let mut request = valid_request("No chat context.");
    request.frozen.conversation = Some(webnovel_core::context::conversation::FrozenConversation {
        project_id: PROJECT.into(),
        operation_namespace: "memory".into(),
        document_id: "chapter-1".into(),
        thread_id: "chat".into(),
        omitted_turns: 0,
        turns: Vec::new(),
    });
    invalid_request(request);
}

#[test]
fn memory_http_profile_cannot_cross_into_author_discussion_packets() {
    let mut request = valid_request("The memory profile is chapter scoped.");
    request.provider_binding = Some(http_memory_binding());
    request.frozen.purpose = ContextPurpose::Discuss;
    request.response_contract = None;
    invalid_request(request);
}

#[test]
fn memory_recipe_rejects_claude_author_profile() {
    let mut request = valid_request("Claude author runs cannot create memory records.");
    request.provider_binding = Some(ProviderBinding::claude_author_runtime(
        "claude-sonnet-5",
        "high",
        "2.1.220",
        &"a".repeat(64),
    ));
    invalid_request(request);
}

#[test]
fn whole_chapter_overflow_is_a_mandatory_error_without_truncation() {
    let text = "A".repeat(800);
    let mut request = valid_request(&text);
    request.budget = MockContextBudget::new("100", "10", "10");
    let error = compile_packet(&request).expect_err("whole target must not be shortened");
    match error {
        PacketError::Budget(error) => {
            assert_eq!(
                error.code,
                webnovel_core::context::BudgetErrorCode::MandatoryContextTooLarge
            );
            assert!(
                error
                    .mandatory_handles
                    .iter()
                    .any(|handle| handle == "chapter")
            );
        }
        other => panic!("expected mandatory overflow, got {other:?}"),
    }
}
