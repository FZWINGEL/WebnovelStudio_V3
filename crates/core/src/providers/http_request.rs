//! Deterministic OpenAI-compatible request bodies.
//!
//! This module has no network or credential access.  It exists so the native
//! worker and the core settlement path can agree on the exact HTTP bytes that
//! belong to an immutable packet without calling those bytes Codex stdin.

use crate::context::packet::{HttpResponseFormat, PacketMessage, PacketOptions};
use crate::projects::{CoreError, CoreResult};
use crate::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedHttpRequest {
    pub body: Vec<u8>,
    pub body_hash: String,
    pub body_bytes: String,
    pub stream: bool,
}

/// Build the canonical body sent to an OpenAI-compatible chat-completions
/// endpoint.  The ordering and field names intentionally mirror the native
/// adapter's request serializer.
pub fn prepare_request(
    messages: &[PacketMessage],
    options: &PacketOptions,
) -> CoreResult<PreparedHttpRequest> {
    let binding = options.provider_binding.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidProviderBinding",
            "An OpenAI-compatible request needs an immutable provider binding.",
        )
    })?;
    binding
        .validate()
        .map_err(|message| CoreError::new("InvalidProviderBinding", &message))?;
    let http = binding.http.as_ref().ok_or_else(|| {
        CoreError::new(
            "InvalidProviderBinding",
            "The OpenAI-compatible provider binding has no HTTP contract.",
        )
    })?;
    if !binding.is_http() {
        return Err(CoreError::new(
            "InvalidProviderBinding",
            "The deterministic HTTP body helper accepts only an OpenAI-compatible binding.",
        ));
    }
    if messages.is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "An OpenAI-compatible request must contain at least one message.",
        ));
    }
    if options.model_id != binding.model_id {
        return Err(CoreError::new(
            "ProviderBindingMismatch",
            "The HTTP request model does not match its immutable provider binding.",
        ));
    }

    let mut body = Map::new();
    body.insert("model".to_owned(), Value::String(options.model_id.clone()));
    body.insert(
        "messages".to_owned(),
        Value::Array(
            messages
                .iter()
                .map(|message| json!({"role": message.role, "content": message.content}))
                .collect(),
        ),
    );
    body.insert("stream".to_owned(), Value::Bool(http.stream));
    if let Some(value) = &binding.reasoning {
        body.insert("reasoning_effort".to_owned(), Value::String(value.clone()));
    }
    if let Some(value) = &binding.service_tier {
        body.insert("service_tier".to_owned(), Value::String(value.clone()));
    }
    if !options.max_output_tokens.is_empty() {
        let value = options.max_output_tokens.parse::<u32>().map_err(|_| {
            CoreError::new(
                "InvalidRequest",
                "The HTTP request output allowance is not a canonical u32.",
            )
        })?;
        body.insert("max_completion_tokens".to_owned(), Value::from(value));
    }
    if http.response_format == HttpResponseFormat::JsonObject {
        body.insert("response_format".to_owned(), json!({"type": "json_object"}));
    }
    let body = serde_json::to_vec(&body).map_err(|error| {
        CoreError::new(
            "InvalidRequest",
            &format!("The HTTP request body could not be encoded: {error}"),
        )
    })?;
    if body.len() > crate::context::packet::HTTP_INPUT_LIMIT_BYTES {
        return Err(CoreError::new(
            "InputTooLarge",
            "The OpenAI-compatible request body exceeds the application limit.",
        ));
    }
    Ok(PreparedHttpRequest {
        body_hash: sha256_hex(&body),
        body_bytes: body.len().to_string(),
        stream: http.stream,
        body,
    })
}
