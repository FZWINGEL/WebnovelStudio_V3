use webnovel_core::context::packet::{
    HTTP_INPUT_LIMIT_BYTES, HTTP_OUTPUT_LIMIT_BYTES, HTTP_PROFILE_VERSION,
    HTTP_TOKEN_ACCOUNTING_METHOD, HttpProviderBinding, HttpResponseFormat, PacketMessage,
    PacketOptions, ProviderBinding,
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
