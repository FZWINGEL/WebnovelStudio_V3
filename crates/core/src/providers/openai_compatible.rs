//! Direct HTTP transport for OpenAI-compatible chat-completion endpoints.
//!
//! This module intentionally stops at provider I/O.  It does not know about
//! chapters, edits, canon, summaries, or persistence.  Callers choose a
//! model and explicitly opt into reasoning, service tier, JSON mode, and
//! bounded output; unsupported options are never guessed or substituted.

use super::adapter::{
    CancellationToken, ChatAdapter, ChatRequest, ChatResponse, MessageRole, ProviderError,
    ProviderErrorKind, ResponseFormat, StreamEvent, Usage,
};
use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, RequestBuilder, Response};
use serde_json::{Map, Value, json};
use std::fmt;
use std::time::Duration;
use url::Url;

pub const DEFAULT_MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
pub const DEFAULT_MAX_SSE_LINE_BYTES: usize = 1024 * 1024;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// User-configured endpoint and limits.  The key is kept private so serde or
/// accidental debug formatting cannot expose it.
pub struct OpenAiCompatibleConfig {
    base_url: Url,
    api_key: Option<String>,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_sse_line_bytes: usize,
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl fmt::Debug for OpenAiCompatibleConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("max_request_bytes", &self.max_request_bytes)
            .field("max_response_bytes", &self.max_response_bytes)
            .field("max_sse_line_bytes", &self.max_sse_line_bytes)
            .field("timeout", &self.timeout)
            .field("connect_timeout", &self.connect_timeout)
            .finish()
    }
}

impl OpenAiCompatibleConfig {
    pub fn new(base_url: &str, api_key: Option<String>) -> Result<Self, ProviderError> {
        if api_key
            .as_deref()
            .is_some_and(|key| key.chars().any(|character| character.is_control()))
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Configuration,
                "API key contains control characters",
            ));
        }
        Ok(Self {
            base_url: normalize_base_url(base_url)?,
            api_key,
            max_request_bytes: DEFAULT_MAX_REQUEST_BYTES,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_sse_line_bytes: DEFAULT_MAX_SSE_LINE_BYTES,
            timeout: DEFAULT_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }
}

pub fn normalize_base_url(input: &str) -> Result<Url, ProviderError> {
    if input.is_empty() || input.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(ProviderError::new(
            ProviderErrorKind::Configuration,
            "endpoint contains whitespace or controls",
        ));
    }
    let mut url = Url::parse(input).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::Configuration,
            "endpoint must be a valid URL",
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ProviderError::new(
            ProviderErrorKind::Configuration,
            "endpoint must use http or https",
        ));
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(ProviderError::new(
            ProviderErrorKind::Configuration,
            "endpoint must include a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ProviderError::new(
            ProviderErrorKind::Configuration,
            "endpoint credentials are not allowed",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ProviderError::new(
            ProviderErrorKind::Configuration,
            "endpoint query and fragment are not allowed",
        ));
    }
    let path = url.path().trim_end_matches('/');
    let path = if path.is_empty() {
        "/v1".to_owned()
    } else if path == "/v1" || path.ends_with("/v1") {
        path.to_owned()
    } else {
        format!("{path}/v1")
    };
    url.set_path(&path);
    Ok(url)
}

pub struct OpenAiCompatibleAdapter {
    config: OpenAiCompatibleConfig,
    client: Client,
}

impl fmt::Debug for OpenAiCompatibleAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleAdapter")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl OpenAiCompatibleAdapter {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(config.connect_timeout)
            .timeout(config.timeout)
            .build()
            .map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Configuration,
                    "failed to configure HTTP client",
                )
            })?;
        Ok(Self { config, client })
    }

    pub fn config(&self) -> &OpenAiCompatibleConfig {
        &self.config
    }

    fn endpoint(&self, suffix: &str) -> String {
        let mut url = self.config.base_url.clone();
        let path = format!(
            "{}/{}",
            url.path().trim_end_matches('/'),
            suffix.trim_start_matches('/')
        );
        url.set_path(&path);
        url.to_string()
    }

    fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        match &self.config.api_key {
            Some(key) if !key.is_empty() => request.header(AUTHORIZATION, format!("Bearer {key}")),
            _ => request,
        }
    }

    fn request_body(&self, request: &ChatRequest, stream: bool) -> Result<Vec<u8>, ProviderError> {
        if request.model.trim().is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::Configuration,
                "model must not be empty",
            ));
        }
        if request.messages.is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::Configuration,
                "messages must not be empty",
            ));
        }
        let messages: Vec<Value> = request
            .messages
            .iter()
            .map(|message| json!({"role": message.role.as_str(), "content": message.content}))
            .collect();
        let mut body = Map::new();
        body.insert("model".into(), Value::String(request.model.clone()));
        body.insert("messages".into(), Value::Array(messages));
        body.insert("stream".into(), Value::Bool(stream));
        if let Some(value) = &request.reasoning_effort {
            body.insert("reasoning_effort".into(), Value::String(value.clone()));
        }
        if let Some(value) = &request.service_tier {
            body.insert("service_tier".into(), Value::String(value.clone()));
        }
        if let Some(value) = request.max_output_tokens {
            body.insert("max_completion_tokens".into(), Value::from(value));
        }
        if request.response_format == ResponseFormat::JsonObject {
            body.insert("response_format".into(), json!({"type":"json_object"}));
        }
        let bytes = serde_json::to_vec(&body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "failed to encode request",
            )
        })?;
        if bytes.len() > self.config.max_request_bytes {
            return Err(ProviderError::new(
                ProviderErrorKind::LimitExceeded,
                "request exceeds configured body limit",
            ));
        }
        Ok(bytes)
    }

    fn check_cancel(cancel: &CancellationToken) -> Result<(), ProviderError> {
        if cancel.is_cancelled() {
            Err(ProviderError::new(
                ProviderErrorKind::Cancelled,
                "request cancelled",
            ))
        } else {
            Ok(())
        }
    }

    fn status_error(&self, response: &Response) -> ProviderError {
        let status = response.status().as_u16();
        let kind = if status == 401 || status == 403 {
            ProviderErrorKind::Authentication
        } else {
            ProviderErrorKind::Http
        };
        ProviderError::new(kind, "provider rejected the request").with_status(status)
    }

    async fn wait_for_cancel(cancel: &CancellationToken) {
        while !cancel.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn send_with_cancel(
        &self,
        request: RequestBuilder,
        cancel: &CancellationToken,
    ) -> Result<Response, ProviderError> {
        tokio::select! {
            result = request.send() => result.map_err(|_| ProviderError::new(ProviderErrorKind::Network, "provider request failed")),
            _ = Self::wait_for_cancel(cancel) => Err(ProviderError::new(ProviderErrorKind::Cancelled, "request cancelled")),
        }
    }

    async fn read_bounded(
        response: Response,
        max: usize,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>, ProviderError> {
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        loop {
            let next = tokio::select! {
                chunk = stream.next() => chunk,
                _ = Self::wait_for_cancel(cancel) => {
                    return Err(ProviderError::new(ProviderErrorKind::Cancelled, "request cancelled"));
                }
            };
            let Some(chunk) = next else {
                break;
            };
            let chunk = chunk.map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Network,
                    "provider response could not be read",
                )
            })?;
            if chunk.len() > max.saturating_sub(bytes.len()) {
                return Err(ProviderError::new(
                    ProviderErrorKind::LimitExceeded,
                    "provider response exceeds configured body limit",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    fn parse_response(
        &self,
        bytes: &[u8],
        response_format: ResponseFormat,
    ) -> Result<ChatResponse, ProviderError> {
        let value: Value = serde_json::from_slice(bytes).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider returned invalid JSON",
            )
        })?;
        let response = parse_chat_response(&value)?;
        if response_format == ResponseFormat::JsonObject {
            let parsed: Value = serde_json::from_str(&response.text).map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::InvalidResponse,
                    "provider JSON response content was not valid JSON",
                )
                .with_partial(response.text.clone())
            })?;
            if !parsed.is_object() {
                return Err(ProviderError::new(
                    ProviderErrorKind::InvalidResponse,
                    "provider JSON response content was not an object",
                )
                .with_partial(response.text));
            }
        }
        Ok(response)
    }

    /// Async entry point for Tauri commands.  It lets cancellation drop the
    /// in-flight request instead of waiting for a blocking socket read.
    pub async fn complete_async(
        &self,
        request: &ChatRequest,
        cancel: &CancellationToken,
    ) -> Result<ChatResponse, ProviderError> {
        Self::check_cancel(cancel)?;
        let body = self.request_body(request, false)?;
        let response = self
            .send_with_cancel(
                self.authorized(self.client.post(self.endpoint("chat/completions")))
                    .header(CONTENT_TYPE, "application/json")
                    .body(body),
                cancel,
            )
            .await?;
        Self::check_cancel(cancel)?;
        if !response.status().is_success() {
            return Err(self.status_error(&response));
        }
        let bytes = Self::read_bounded(response, self.config.max_response_bytes, cancel).await?;
        Self::check_cancel(cancel)?;
        self.parse_response(&bytes, request.response_format)
    }

    /// Async streaming entry point with cooperative cancellation while
    /// waiting for headers and each response chunk.
    pub async fn stream_async(
        &self,
        request: &ChatRequest,
        cancel: &CancellationToken,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<ChatResponse, ProviderError> {
        Self::check_cancel(cancel)?;
        let body = self.request_body(request, true)?;
        let response = self
            .send_with_cancel(
                self.authorized(self.client.post(self.endpoint("chat/completions")))
                    .header(CONTENT_TYPE, "application/json")
                    .body(body),
                cancel,
            )
            .await?;
        Self::check_cancel(cancel).map_err(|error| error.with_partial(String::new()))?;
        if !response.status().is_success() {
            return Err(self.status_error(&response));
        }

        let mut stream = response.bytes_stream();
        let mut parser = SseParser::new(self.config.max_sse_line_bytes);
        while !parser.done {
            Self::check_cancel(cancel).map_err(|error| error.with_partial(parser.text.clone()))?;
            let next = tokio::select! {
                chunk = stream.next() => chunk,
                _ = Self::wait_for_cancel(cancel) => {
                    return Err(ProviderError::new(ProviderErrorKind::Cancelled, "request cancelled")
                        .with_partial(parser.text.clone()));
                }
            };
            let Some(chunk) = next else {
                break;
            };
            let chunk = chunk.map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Network,
                    "provider stream could not be read",
                )
                .with_partial(parser.text.clone())
            })?;
            parser.total_bytes = parser.total_bytes.saturating_add(chunk.len());
            if parser.total_bytes > self.config.max_response_bytes {
                return Err(ProviderError::new(
                    ProviderErrorKind::LimitExceeded,
                    "provider stream exceeds configured body limit",
                )
                .with_partial(parser.text));
            }
            if let Err(error) = parser.feed(&chunk, on_event) {
                return Err(error.with_partial(parser.text.clone()));
            }
        }
        if !parser.done {
            parser
                .finish(on_event)
                .map_err(|error| error.with_partial(parser.text.clone()))?;
        }
        if !parser.done {
            return Err(ProviderError::new(
                ProviderErrorKind::Truncated,
                "provider stream ended before [DONE]",
            )
            .with_partial(parser.text));
        }
        if !parser.saw_choice || parser.text.is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider stream completed without assistant text",
            )
            .with_partial(parser.text));
        }
        if request.response_format == ResponseFormat::JsonObject {
            let parsed: Value = serde_json::from_str(&parser.text).map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::InvalidResponse,
                    "provider JSON response content was not valid JSON",
                )
                .with_partial(parser.text.clone())
            })?;
            if !parsed.is_object() {
                return Err(ProviderError::new(
                    ProviderErrorKind::InvalidResponse,
                    "provider JSON response content was not an object",
                )
                .with_partial(parser.text.clone()));
            }
        }
        Ok(ChatResponse {
            text: parser.text,
            model: parser.model,
            finish_reason: parser.finish_reason,
            usage: parser.usage.unwrap_or_default(),
        })
    }

    pub async fn list_models_async(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<String>, ProviderError> {
        Self::check_cancel(cancel)?;
        let response = self
            .send_with_cancel(
                self.authorized(self.client.get(self.endpoint("models"))),
                cancel,
            )
            .await?;
        Self::check_cancel(cancel)?;
        if !response.status().is_success() {
            return Err(self.status_error(&response));
        }
        let bytes = Self::read_bounded(response, self.config.max_response_bytes, cancel).await?;
        Self::check_cancel(cancel)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider returned invalid model JSON",
            )
        })?;
        let data = value.get("data").and_then(Value::as_array).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider model response has no data array",
            )
        })?;
        let models: Result<Vec<_>, _> = data
            .iter()
            .map(|item| {
                item.get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        ProviderError::new(
                            ProviderErrorKind::InvalidResponse,
                            "provider model entry has no id",
                        )
                    })
            })
            .collect();
        Self::check_cancel(cancel)?;
        models
    }
}

impl ChatAdapter for OpenAiCompatibleAdapter {
    async fn complete(
        &self,
        request: &ChatRequest,
        cancel: &CancellationToken,
    ) -> Result<ChatResponse, ProviderError> {
        self.complete_async(request, cancel).await
    }

    async fn stream(
        &self,
        request: &ChatRequest,
        cancel: &CancellationToken,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<ChatResponse, ProviderError> {
        self.stream_async(request, cancel, on_event).await
    }

    async fn list_models(&self, cancel: &CancellationToken) -> Result<Vec<String>, ProviderError> {
        self.list_models_async(cancel).await
    }
}

fn parse_chat_response(value: &Value) -> Result<ChatResponse, ProviderError> {
    if value.get("error").is_some() {
        return Err(ProviderError::new(
            ProviderErrorKind::Http,
            "provider returned an error envelope",
        ));
    }
    let choices = value
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider response has no choices",
            )
        })?;
    let choice = choices.first().ok_or_else(|| {
        ProviderError::new(
            ProviderErrorKind::InvalidResponse,
            "provider returned no choices",
        )
    })?;
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider choice has no message",
            )
        })?;
    if has_refusal(message.get("refusal"))? {
        return Err(ProviderError::new(
            ProviderErrorKind::Refused,
            "provider refused the request",
        ));
    }
    if has_tool_call(message.get("tool_calls"))? || has_tool_call(message.get("function_call"))? {
        return Err(ProviderError::new(
            ProviderErrorKind::ToolCall,
            "provider returned a tool call instead of text",
        ));
    }
    let text = message
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider message content is not text",
            )
        })?;
    let finish_reason = validate_finish_reason(choice.get("finish_reason"))?;
    if let Some(error) = finish_reason_error(finish_reason.as_deref(), text) {
        return Err(error);
    }
    if text.is_empty() {
        return Err(ProviderError::new(
            ProviderErrorKind::InvalidResponse,
            "provider returned an empty assistant message",
        ));
    }
    Ok(ChatResponse {
        text: text.to_owned(),
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
        finish_reason,
        usage: parse_usage(value.get("usage"))?,
    })
}

fn parse_usage(value: Option<&Value>) -> Result<Usage, ProviderError> {
    let Some(value) = value else {
        return Ok(Usage::default());
    };
    let object = value.as_object().ok_or_else(|| {
        ProviderError::new(
            ProviderErrorKind::InvalidResponse,
            "provider usage is not an object",
        )
    })?;
    Ok(Usage {
        input_tokens: integer(object, "prompt_tokens")?,
        output_tokens: integer(object, "completion_tokens")?,
        total_tokens: integer(object, "total_tokens")?,
    })
}

fn integer(object: &Map<String, Value>, key: &str) -> Result<Option<u64>, ProviderError> {
    match object.get(key) {
        None => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider usage contains a non-integer counter",
            )
        }),
    }
}

fn validate_finish_reason(value: Option<&Value>) -> Result<Option<String>, ProviderError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(reason)) => Ok(Some(reason.clone())),
        Some(_) => Err(ProviderError::new(
            ProviderErrorKind::Protocol,
            "provider finish reason was not a string",
        )),
    }
}

fn finish_reason_error(reason: Option<&str>, partial: &str) -> Option<ProviderError> {
    let error = match reason {
        None | Some("stop") => return None,
        Some("length") | Some("content_filter") => ProviderError::new(
            ProviderErrorKind::Incomplete,
            "provider stopped before completing a text response",
        ),
        Some("tool_calls") | Some("function_call") => {
            ProviderError::new(ProviderErrorKind::ToolCall, "provider returned a tool call")
        }
        Some("refusal") => {
            ProviderError::new(ProviderErrorKind::Refused, "provider refused the request")
        }
        Some(_) => ProviderError::new(
            ProviderErrorKind::Protocol,
            "provider returned an unsupported finish reason",
        ),
    };
    Some(error.with_partial(partial.to_owned()))
}

fn has_refusal(value: Option<&Value>) -> Result<bool, ProviderError> {
    match value {
        None | Some(Value::Null) => Ok(false),
        Some(Value::String(value)) => Ok(!value.is_empty()),
        Some(_) => Err(ProviderError::new(
            ProviderErrorKind::Protocol,
            "provider refusal was not a string",
        )),
    }
}

fn has_tool_call(value: Option<&Value>) -> Result<bool, ProviderError> {
    match value {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Array(values)) => Ok(!values.is_empty()),
        Some(Value::Object(value)) => Ok(!value.is_empty()),
        Some(_) => Err(ProviderError::new(
            ProviderErrorKind::Protocol,
            "provider tool call was not an object or array",
        )),
    }
}

struct SseParser {
    buffer: Vec<u8>,
    event_data: Vec<String>,
    total_bytes: usize,
    max_line_bytes: usize,
    done: bool,
    text: String,
    model: Option<String>,
    finish_reason: Option<String>,
    usage: Option<Usage>,
    saw_choice: bool,
}

impl SseParser {
    fn new(max_line_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            event_data: Vec::new(),
            total_bytes: 0,
            max_line_bytes,
            done: false,
            text: String::new(),
            model: None,
            finish_reason: None,
            usage: None,
            saw_choice: false,
        }
    }

    fn feed(
        &mut self,
        bytes: &[u8],
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<(), ProviderError> {
        if self.done {
            return Ok(());
        }
        self.buffer.extend_from_slice(bytes);
        loop {
            let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') else {
                if self.buffer.len() > self.max_line_bytes {
                    return Err(ProviderError::new(
                        ProviderErrorKind::LimitExceeded,
                        "SSE line exceeds configured limit",
                    ));
                }
                return Ok(());
            };
            let line: Vec<u8> = self.buffer.drain(..=position).collect();
            if line.len().saturating_sub(1) > self.max_line_bytes {
                return Err(ProviderError::new(
                    ProviderErrorKind::LimitExceeded,
                    "SSE line exceeds configured limit",
                ));
            }
            let line = String::from_utf8(line[..line.len() - 1].to_vec()).map_err(|_| {
                ProviderError::new(ProviderErrorKind::Protocol, "provider SSE was not UTF-8")
            })?;
            let line = line.strip_suffix('\r').unwrap_or(&line);
            if line.is_empty() {
                self.dispatch(on_event)?;
            } else if let Some(data) = line.strip_prefix("data:") {
                self.event_data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
            }
            if self.done {
                return Ok(());
            }
        }
    }

    fn finish(&mut self, on_event: &mut dyn FnMut(StreamEvent)) -> Result<(), ProviderError> {
        if self.done {
            return Ok(());
        }
        if !self.buffer.is_empty() {
            if self.buffer.len() > self.max_line_bytes {
                return Err(ProviderError::new(
                    ProviderErrorKind::LimitExceeded,
                    "SSE line exceeds configured limit",
                ));
            }
            let line = std::mem::take(&mut self.buffer);
            let line = String::from_utf8(line).map_err(|_| {
                ProviderError::new(ProviderErrorKind::Protocol, "provider SSE was not UTF-8")
            })?;
            let line = line.strip_suffix('\r').unwrap_or(&line);
            if let Some(data) = line.strip_prefix("data:") {
                self.event_data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
            }
        }
        self.dispatch(on_event)
    }

    fn dispatch(&mut self, on_event: &mut dyn FnMut(StreamEvent)) -> Result<(), ProviderError> {
        if self.event_data.is_empty() {
            return Ok(());
        }
        let data = self.event_data.join("\n");
        self.event_data.clear();
        if data == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let value: Value = serde_json::from_str(&data).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Protocol,
                "provider SSE data was not valid JSON",
            )
        })?;
        if value.get("error").is_some() {
            return Err(ProviderError::new(
                ProviderErrorKind::Http,
                "provider returned an error envelope",
            ));
        }
        if let Some(model) = value.get("model").and_then(Value::as_str) {
            self.model = Some(model.to_owned());
        }
        if let Some(usage) = value.get("usage") {
            let parsed = parse_usage(Some(usage))?;
            self.usage = Some(parsed);
            on_event(StreamEvent::Usage(parsed));
        }
        let Some(choices_value) = value.get("choices") else {
            if self.usage.is_some() {
                return Ok(());
            }
            return Err(ProviderError::new(
                ProviderErrorKind::Protocol,
                "provider stream event has no choices array",
            ));
        };
        let choices = choices_value.as_array().ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Protocol,
                "provider stream choices was not an array",
            )
        })?;
        let Some(choice) = choices.first() else {
            if self.usage.is_some() {
                return Ok(());
            }
            return Err(ProviderError::new(
                ProviderErrorKind::Protocol,
                "provider stream event has no choice",
            ));
        };
        let choice = choice.as_object().ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Protocol,
                "provider stream choice was not an object",
            )
        })?;
        self.saw_choice = true;
        let finish_reason = validate_finish_reason(choice.get("finish_reason"))?;
        if let Some(reason) = finish_reason.as_deref() {
            if let Some(error) = finish_reason_error(Some(reason), &self.text) {
                return Err(error);
            }
            self.finish_reason = Some(reason.to_owned());
        }
        let delta = choice.get("delta").ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Protocol,
                "provider stream choice has no delta",
            )
        })?;
        let delta = delta.as_object().ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Protocol,
                "provider stream delta was not an object",
            )
        })?;
        if has_refusal(delta.get("refusal"))? {
            return Err(ProviderError::new(
                ProviderErrorKind::Refused,
                "provider refused the request",
            )
            .with_partial(self.text.clone()));
        }
        if has_tool_call(delta.get("tool_calls"))? || has_tool_call(delta.get("function_call"))? {
            return Err(ProviderError::new(
                ProviderErrorKind::ToolCall,
                "provider returned a tool call instead of text",
            )
            .with_partial(self.text.clone()));
        }
        if let Some(role_value) = delta.get("role") {
            let role = role_value.as_str().ok_or_else(|| {
                ProviderError::new(
                    ProviderErrorKind::Protocol,
                    "provider role was not a string",
                )
            })?;
            let role = match role {
                "system" => MessageRole::System,
                "developer" => MessageRole::Developer,
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                _ => {
                    return Err(ProviderError::new(
                        ProviderErrorKind::Protocol,
                        "provider returned an unsupported message role",
                    ));
                }
            };
            on_event(StreamEvent::Role(role));
        }
        if let Some(content_value) = delta.get("content")
            && !content_value.is_null()
        {
            let content = content_value.as_str().ok_or_else(|| {
                ProviderError::new(
                    ProviderErrorKind::Protocol,
                    "provider content delta was not a string",
                )
            })?;
            self.text.push_str(content);
            on_event(StreamEvent::ContentDelta(content.to_owned()));
        }
        for key in ["reasoning", "reasoning_content"] {
            if let Some(reasoning_value) = delta.get(key)
                && !reasoning_value.is_null()
            {
                let reasoning = reasoning_value.as_str().ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorKind::Protocol,
                        "provider reasoning delta was not a string",
                    )
                })?;
                on_event(StreamEvent::ReasoningDelta(reasoning.to_owned()));
            }
        }
        Ok(())
    }
}
