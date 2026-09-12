use webnovel_core::context::packet::{
    HTTP_INPUT_LIMIT_BYTES, HTTP_MEMORY_LEGACY_MODEL_ID, HTTP_MEMORY_LEGACY_PROFILE_VERSION,
    HTTP_MEMORY_LEGACY_REASONING, HTTP_MEMORY_MODEL_ID, HTTP_MEMORY_OUTPUT_LIMIT_BYTES,
    HTTP_MEMORY_PROFILE_VERSION, HTTP_MEMORY_REASONING, HTTP_OUTPUT_LIMIT_BYTES,
    HTTP_PROFILE_VERSION, HTTP_TOKEN_ACCOUNTING_METHOD, HttpProviderBinding, HttpResponseFormat,
    PacketMessage, PacketOptions, ProviderBinding,
};
use webnovel_core::providers::http_request::prepare_request;

fn http_binding(response_format: HttpResponseFormat) -> ProviderBinding {
    ProviderBinding {
        provider_id: "openai-compatible:00000000-0000-0000-0000-000000000001".into(),
        model_id: "gpt-5.6-luna".into(),
        reasoning: Some("xhigh".into()),
        service_tier: Some("priority".into()),
        profile_version: HTTP_PROFILE_VERSION.into(),
        input_limit_bytes: HTTP_INPUT_LIMIT_BYTES.to_string(),
        reserved_output_bytes: "0".into(),
        reserved_protocol_bytes: "0".into(),
        output_limit_bytes: HTTP_OUTPUT_LIMIT_BYTES.to_string(),
        accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.into(),
        runtime: None,
        http: Some(HttpProviderBinding {
            base_url: "https://example.test/v1".into(),
            config_revision: "1".into(),
            stream: true,
            response_format,
        }),
    }
}

fn packet_options(binding: ProviderBinding) -> PacketOptions {
    PacketOptions {
        model_id: binding.model_id.clone(),
        max_output_tokens: String::new(),
        token_accounting_method: HTTP_TOKEN_ACCOUNTING_METHOD.into(),
        provider_binding: Some(binding),
    }
}

#[test]
fn http_binding_is_secret_free_and_body_is_deterministic() {
    let binding = http_binding(HttpResponseFormat::JsonObject);
    binding.validate().unwrap();
    let options = packet_options(binding);
    let messages = vec![
        PacketMessage {
            role: "system".into(),
            content: "You are an editor.".into(),
        },
        PacketMessage {
            role: "user".into(),
            content: "Suggest a scoped revision.".into(),
        },
    ];
    let first = prepare_request(&messages, &options).unwrap();
    let second = prepare_request(&messages, &options).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.body_hash.len(), 64);
    assert_eq!(first.body_bytes, first.body.len().to_string());
    let body: serde_json::Value = serde_json::from_slice(&first.body).unwrap();
    assert_eq!(body["model"], "gpt-5.6-luna");
    assert_eq!(body["stream"], true);
    assert_eq!(body["response_format"]["type"], "json_object");
    assert_eq!(body["reasoning_effort"], "xhigh");
    assert_eq!(body["service_tier"], "priority");
}

#[test]
fn historical_codex_binding_shape_does_not_gain_http_fields() {
    let binding = ProviderBinding::codex_luna_historical();
    let encoded = serde_json::to_string(&binding).unwrap();
    assert!(!encoded.contains("http"));
    let restored: ProviderBinding = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, binding);
    restored.validate().unwrap();
}

#[test]
fn http_binding_rejects_secrets_and_non_normalized_urls() {
    let mut binding = http_binding(HttpResponseFormat::Text);
    binding.http.as_mut().unwrap().base_url = "https://key@example.test/v1".into();
    assert!(binding.validate().is_err());
    binding.http.as_mut().unwrap().base_url = "https://example.test/v1/".into();
    assert!(binding.validate().is_err());
}

fn http_memory_binding() -> ProviderBinding {
    ProviderBinding::http_memory(
        "openai-compatible:00000000-0000-0000-0000-000000000001",
        "https://example.test/v1",
        "1",
        true,
        HttpResponseFormat::Text,
    )
}

#[test]
fn memory_http_profile_is_fixed_and_secret_free() {
    let binding = http_memory_binding();
    binding.validate().unwrap();
    assert!(binding.is_http());
    assert!(binding.is_http_memory());
    assert_eq!(binding.input_limit().unwrap(), HTTP_INPUT_LIMIT_BYTES);
    assert_eq!(
        binding.output_limit().unwrap(),
        HTTP_MEMORY_OUTPUT_LIMIT_BYTES
    );

    for (mutated, message) in [
        (
            {
                let mut value = binding.clone();
                value.model_id = "gpt-5.6-luna".into();
                value
            },
            "model",
        ),
        (
            {
                let mut value = binding.clone();
                value.reasoning = Some("medium".into());
                value
            },
            "reasoning",
        ),
        (
            {
                let mut value = binding.clone();
                value.service_tier = Some("priority".into());
                value
            },
            "tier",
        ),
        (
            {
                let mut value = binding.clone();
                value.profile_version = HTTP_PROFILE_VERSION.into();
                value
            },
            "profile",
        ),
    ] {
        assert!(mutated.validate().is_err(), "invalid {message} accepted");
    }
    let encoded = serde_json::to_string(&binding).unwrap();
    assert!(!encoded.contains("apiKey"));
    assert!(!encoded.contains("secret"));
}

#[test]
fn legacy_luna_memory_profile_remains_valid_and_distinct_from_current_astra() {
    let mut binding = http_memory_binding();
    binding.profile_version = HTTP_MEMORY_LEGACY_PROFILE_VERSION.into();
    binding.model_id = HTTP_MEMORY_LEGACY_MODEL_ID.into();
    binding.reasoning = Some(HTTP_MEMORY_LEGACY_REASONING.into());
    binding.validate().unwrap();
    assert!(binding.is_http_memory());
    assert_ne!(binding.profile_version, HTTP_MEMORY_PROFILE_VERSION);
    assert_ne!(binding.model_id, HTTP_MEMORY_MODEL_ID);
    assert_eq!(
        serde_json::to_string(&binding).unwrap(),
        r#"{"providerId":"openai-compatible:00000000-0000-0000-0000-000000000001","modelId":"gpt-5.6-luna","reasoning":"xhigh","serviceTier":null,"profileVersion":"openai-chat-completions.memory.v1","inputLimitBytes":"2097152","reservedOutputBytes":"0","reservedProtocolBytes":"0","outputLimitBytes":"65536","accountingMethod":"utf8-byte-count/openai-compatible-http-application-cap-v1","http":{"baseUrl":"https://example.test/v1","configRevision":"1","stream":true,"responseFormat":"text"}}"#
    );

    let mut wrong = binding.clone();
    wrong.model_id = HTTP_MEMORY_MODEL_ID.into();
    assert!(wrong.validate().is_err());
    wrong = binding;
    wrong.reasoning = Some(HTTP_MEMORY_REASONING.into());
    assert!(wrong.validate().is_err());
}
