//! Pure JSON-RPC/JSONL protocol support for the local Codex app-server.
//!
//! This module deliberately has no process, filesystem, or persistence
//! responsibilities.  The runtime feeds bounded stdout frames into the
//! decoder and owns request routing; this module only validates the wire
//! records and assembles one isolated turn.

use serde_json::{Map, Value};
use std::fmt;

/// Maximum JSONL record accepted from the app-server.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Maximum retained answer text for one request.
pub const MAX_ANSWER_BYTES: usize = 64 * 1024;
/// Maximum decoded records accepted while a connection is alive.
pub const MAX_RECORDS: usize = 32 * 1024;
/// Unknown notifications are harmless but bounded so a faulty server cannot
/// turn the connection worker into an unbounded memory sink.
pub const MAX_UNKNOWN_NOTIFICATIONS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    InvalidUtf8,
    FrameTooLarge,
    InvalidJson,
    InvalidEnvelope,
    InvalidIdentifier,
    MismatchedResponse,
    UnexpectedRequest,
    UnsupportedItem,
    InvalidTurn,
    OutputLimit,
    IncompleteFrame,
    DuplicateTerminal,
    FinalTextMismatch,
    UnknownNotificationLimit,
}

/// Stable, bounded identifiers for protocol failures.  These are suitable for
/// diagnostics and telemetry; they intentionally contain no server-provided
/// text, frame contents, or identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolFailureCode {
    InvalidUtf8,
    FrameTooLarge,
    InvalidJson,
    InvalidEnvelope,
    InvalidIdentifier,
    MismatchedResponse,
    UnexpectedRequest,
    UnsupportedItem,
    InvalidTurn,
    OutputLimit,
    IncompleteFrame,
    DuplicateTerminal,
    FinalTextMismatch,
    UnknownNotificationLimit,
}

impl ProtocolFailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidUtf8 => "protocol_invalid_utf8",
            Self::FrameTooLarge => "protocol_frame_too_large",
            Self::InvalidJson => "protocol_invalid_json",
            Self::InvalidEnvelope => "protocol_invalid_envelope",
            Self::InvalidIdentifier => "protocol_invalid_identifier",
            Self::MismatchedResponse => "protocol_mismatched_response",
            Self::UnexpectedRequest => "protocol_unexpected_request",
            Self::UnsupportedItem => "protocol_unsupported_item",
            Self::InvalidTurn => "protocol_invalid_turn",
            Self::OutputLimit => "protocol_output_limit",
            Self::IncompleteFrame => "protocol_incomplete_frame",
            Self::DuplicateTerminal => "protocol_duplicate_terminal",
            Self::FinalTextMismatch => "protocol_final_text_mismatch",
            Self::UnknownNotificationLimit => "protocol_unknown_notification_limit",
        }
    }

    #[cfg(windows)]
    pub(crate) fn from_detail(detail: &str) -> Option<Self> {
        match detail {
            "The Codex app-server returned invalid UTF-8." => Some(Self::InvalidUtf8),
            "The Codex app-server returned an oversized JSONL record." => Some(Self::FrameTooLarge),
            "The Codex app-server returned invalid JSON." => Some(Self::InvalidJson),
            "The Codex app-server returned an invalid JSON-RPC record." => {
                Some(Self::InvalidEnvelope)
            }
            "The Codex app-server returned an invalid identifier." => Some(Self::InvalidIdentifier),
            "The Codex app-server returned an unmatched response." => {
                Some(Self::MismatchedResponse)
            }
            "The Codex app-server requested an unsupported operation." => {
                Some(Self::UnexpectedRequest)
            }
            "The Codex app-server returned an unsupported story item." => {
                Some(Self::UnsupportedItem)
            }
            "The Codex app-server returned an invalid turn record." => Some(Self::InvalidTurn),
            "The Codex app-server answer exceeded the retained output limit." => {
                Some(Self::OutputLimit)
            }
            "The Codex app-server closed with an incomplete JSONL record." => {
                Some(Self::IncompleteFrame)
            }
            "The Codex app-server returned more than one terminal event." => {
                Some(Self::DuplicateTerminal)
            }
            "The Codex app-server final text did not match streamed text." => {
                Some(Self::FinalTextMismatch)
            }
            "The Codex app-server returned too many unknown notifications." => {
                Some(Self::UnknownNotificationLimit)
            }
            _ => None,
        }
    }
}

impl ProtocolError {
    pub const fn failure_code(&self) -> ProtocolFailureCode {
        match self {
            Self::InvalidUtf8 => ProtocolFailureCode::InvalidUtf8,
            Self::FrameTooLarge => ProtocolFailureCode::FrameTooLarge,
            Self::InvalidJson => ProtocolFailureCode::InvalidJson,
            Self::InvalidEnvelope => ProtocolFailureCode::InvalidEnvelope,
            Self::InvalidIdentifier => ProtocolFailureCode::InvalidIdentifier,
            Self::MismatchedResponse => ProtocolFailureCode::MismatchedResponse,
            Self::UnexpectedRequest => ProtocolFailureCode::UnexpectedRequest,
            Self::UnsupportedItem => ProtocolFailureCode::UnsupportedItem,
            Self::InvalidTurn => ProtocolFailureCode::InvalidTurn,
            Self::OutputLimit => ProtocolFailureCode::OutputLimit,
            Self::IncompleteFrame => ProtocolFailureCode::IncompleteFrame,
            Self::DuplicateTerminal => ProtocolFailureCode::DuplicateTerminal,
            Self::FinalTextMismatch => ProtocolFailureCode::FinalTextMismatch,
            Self::UnknownNotificationLimit => ProtocolFailureCode::UnknownNotificationLimit,
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidUtf8 => "The Codex app-server returned invalid UTF-8.",
            Self::FrameTooLarge => "The Codex app-server returned an oversized JSONL record.",
            Self::InvalidJson => "The Codex app-server returned invalid JSON.",
            Self::InvalidEnvelope => "The Codex app-server returned an invalid JSON-RPC record.",
            Self::InvalidIdentifier => "The Codex app-server returned an invalid identifier.",
            Self::MismatchedResponse => "The Codex app-server returned an unmatched response.",
            Self::UnexpectedRequest => "The Codex app-server requested an unsupported operation.",
            Self::UnsupportedItem => "The Codex app-server returned an unsupported story item.",
            Self::InvalidTurn => "The Codex app-server returned an invalid turn record.",
            Self::OutputLimit => "The Codex app-server answer exceeded the retained output limit.",
            Self::IncompleteFrame => "The Codex app-server closed with an incomplete JSONL record.",
            Self::DuplicateTerminal => {
                "The Codex app-server returned more than one terminal event."
            }
            Self::FinalTextMismatch => {
                "The Codex app-server final text did not match streamed text."
            }
            Self::UnknownNotificationLimit => {
                "The Codex app-server returned too many unknown notifications."
            }
        })
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RpcId {
    String(String),
    Number(u64),
}

impl RpcId {
    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }

    fn from_value(value: &Value) -> Result<Self, ProtocolError> {
        match value {
            Value::String(value) if valid_identifier(value) => Ok(Self::String(value.clone())),
            Value::Number(value) => value
                .as_u64()
                .map(Self::Number)
                .ok_or(ProtocolError::InvalidIdentifier),
            _ => Err(ProtocolError::InvalidIdentifier),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RpcMessage {
    Response {
        id: RpcId,
        result: Option<Value>,
        error: Option<Value>,
    },
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: RpcId,
        method: String,
        params: Value,
    },
}

/// A bounded JSONL decoder.  It accepts arbitrary UTF-8 chunk boundaries and
/// never returns a line until its newline delimiter has been observed.
#[derive(Debug, Default)]
pub struct JsonlDecoder {
    buffer: Vec<u8>,
    records: usize,
    unknown_notifications: usize,
}

impl JsonlDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<RpcMessage>, ProtocolError> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > MAX_FRAME_BYTES {
            // A complete line may still be present after a large preceding
            // record, but retaining it would already violate the frame cap.
            return Err(ProtocolError::FrameTooLarge);
        }
        let mut messages = Vec::new();
        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line = self.buffer.drain(..=newline).collect::<Vec<_>>();
            let line = line.strip_suffix(b"\n").unwrap_or(&line);
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            if line.len() > MAX_FRAME_BYTES {
                return Err(ProtocolError::FrameTooLarge);
            }
            let text = std::str::from_utf8(line).map_err(|_| ProtocolError::InvalidUtf8)?;
            let value: Value =
                serde_json::from_str(text).map_err(|_| ProtocolError::InvalidJson)?;
            let message = parse_message(value)?;
            self.records = self.records.saturating_add(1);
            if self.records > MAX_RECORDS {
                return Err(ProtocolError::FrameTooLarge);
            }
            messages.push(message);
        }
        Ok(messages)
    }

    pub fn finish(&self) -> Result<(), ProtocolError> {
        if self.buffer.is_empty() {
            Ok(())
        } else {
            Err(ProtocolError::IncompleteFrame)
        }
    }

    pub fn pending_bytes(&self) -> usize {
        self.buffer.len()
    }

    /// Count a notification only after the runtime has determined that its
    /// method is outside the supported bounded protocol surface. Known
    /// streaming notifications are not limited by this counter.
    pub fn note_unknown_notification(&mut self) -> Result<(), ProtocolError> {
        self.unknown_notifications = self.unknown_notifications.saturating_add(1);
        if self.unknown_notifications > MAX_UNKNOWN_NOTIFICATIONS {
            Err(ProtocolError::UnknownNotificationLimit)
        } else {
            Ok(())
        }
    }
}

fn parse_message(value: Value) -> Result<RpcMessage, ProtocolError> {
    let object = value.as_object().ok_or(ProtocolError::InvalidEnvelope)?;
    if object
        .get("jsonrpc")
        .is_some_and(|value| value.as_str() != Some("2.0"))
    {
        return Err(ProtocolError::InvalidEnvelope);
    }
    let method = object.get("method").and_then(Value::as_str);
    let params = object
        .get("params")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let id = object.get("id").map(RpcId::from_value).transpose()?;
    match (method, id) {
        (Some(method), Some(id)) => Ok(RpcMessage::ServerRequest {
            id,
            method: method.to_owned(),
            params,
        }),
        (Some(method), None) if valid_identifier(method) => Ok(RpcMessage::Notification {
            method: method.to_owned(),
            params,
        }),
        (Some(_), None) => Err(ProtocolError::InvalidIdentifier),
        (None, Some(id)) => {
            let result = object.get("result").cloned();
            let error = object.get("error").cloned();
            if result.is_none() == error.is_none() {
                return Err(ProtocolError::InvalidEnvelope);
            }
            Ok(RpcMessage::Response { id, result, error })
        }
        (None, None) => Err(ProtocolError::InvalidEnvelope),
    }
}

pub fn json_line(
    id: impl Into<String>,
    method: &str,
    params: Value,
) -> Result<Vec<u8>, ProtocolError> {
    let id = id.into();
    if !valid_identifier(&id) {
        return Err(ProtocolError::InvalidIdentifier);
    }
    if !valid_identifier(method) {
        return Err(ProtocolError::InvalidIdentifier);
    }
    let value = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let mut bytes = serde_json::to_vec(&value).map_err(|_| ProtocolError::InvalidEnvelope)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge);
    }
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn notification(method: &str, params: Value) -> Result<Vec<u8>, ProtocolError> {
    if !valid_identifier(method) {
        return Err(ProtocolError::InvalidIdentifier);
    }
    let value = serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params });
    let mut bytes = serde_json::to_vec(&value).map_err(|_| ProtocolError::InvalidEnvelope)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge);
    }
    bytes.push(b'\n');
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadStartConfig {
    pub cwd: Option<String>,
    pub base_instructions: Option<String>,
    pub developer_instructions: Option<String>,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub service_tier: String,
}

impl ThreadStartConfig {
    pub fn params(&self) -> Value {
        serde_json::json!({
            "cwd": self.cwd,
            "baseInstructions": self.base_instructions,
            "developerInstructions": self.developer_instructions,
            "model": self.model,
            "config": self.reasoning_effort.as_ref().map(|effort| serde_json::json!({"model_reasoning_effort": effort})),
            "serviceTier": self.service_tier,
            "ephemeral": true,
            "approvalPolicy": "never",
            "sandbox": "read-only",
        })
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if !valid_identifier(&self.model)
            || self
                .reasoning_effort
                .as_deref()
                .is_some_and(|v| !valid_identifier(v))
            || !valid_identifier(&self.service_tier)
            || self
                .cwd
                .as_deref()
                .is_some_and(|v| v.chars().any(char::is_control))
        {
            return Err(ProtocolError::InvalidIdentifier);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadStartAck {
    pub thread_id: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
}

impl ThreadStartAck {
    pub fn from_result(
        result: &Value,
        requested: &ThreadStartConfig,
    ) -> Result<Self, ProtocolError> {
        let object = result.as_object().ok_or(ProtocolError::InvalidEnvelope)?;
        let thread = object
            .get("thread")
            .and_then(Value::as_object)
            .ok_or(ProtocolError::InvalidEnvelope)?;
        let thread_id = string_field(thread, "id")?;
        if thread.get("ephemeral") != Some(&Value::Bool(true))
            || thread.get("path") != Some(&Value::Null)
        {
            return Err(ProtocolError::InvalidTurn);
        }
        let model = string_field(object, "model")?;
        let reasoning_effort = nullable_string_field(object, "reasoningEffort")?;
        let service_tier = nullable_string_field(object, "serviceTier")?;
        let sources = object
            .get("instructionSources")
            .and_then(Value::as_array)
            .ok_or(ProtocolError::InvalidEnvelope)?;
        if !sources.is_empty() {
            return Err(ProtocolError::InvalidTurn);
        }
        if model != requested.model
            || reasoning_effort != requested.reasoning_effort
            || service_tier.as_deref().unwrap_or("default") != requested.service_tier
        {
            return Err(ProtocolError::InvalidTurn);
        }
        Ok(Self {
            thread_id,
            model,
            reasoning_effort,
            service_tier,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnStartAck {
    pub turn_id: String,
}

impl TurnStartAck {
    pub fn from_result(result: &Value) -> Result<Self, ProtocolError> {
        let turn = result
            .get("turn")
            .and_then(Value::as_object)
            .ok_or(ProtocolError::InvalidEnvelope)?;
        let turn_id = string_field(turn, "id")?;
        Ok(Self { turn_id })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnCompleted {
    pub thread_id: String,
    pub turn_id: String,
    pub status: TurnStatus,
    pub text: String,
    pub usage: Option<TurnUsage>,
    pub failure: Option<TurnFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnFailure {
    pub code: String,
    pub http_status_code: Option<u16>,
}

impl TurnFailure {
    /// Render only the provider failure fields that passed the app-server
    /// allowlist.  This is deliberately safe even for a value constructed by
    /// a caller rather than by `parse_turn_failure`: arbitrary upstream
    /// messages and diagnostics never enter a durable error string.
    pub fn safe_detail(&self) -> String {
        let code = if is_allowed_failure_code(&self.code) {
            self.code.as_str()
        } else {
            "other"
        };
        match self
            .http_status_code
            .filter(|status| (100..=599).contains(status))
        {
            Some(status) => format!("Codex provider failure: {code} (HTTP {status})"),
            None => format!("Codex provider failure: {code}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStatus {
    Completed,
    Interrupted,
    Failed,
    InProgress,
}

impl TurnStatus {
    fn from_str(value: &str) -> Result<Self, ProtocolError> {
        match value {
            "completed" => Ok(Self::Completed),
            "interrupted" => Ok(Self::Interrupted),
            "failed" => Ok(Self::Failed),
            "inProgress" => Ok(Self::InProgress),
            _ => Err(ProtocolError::InvalidTurn),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    AssistantDelta(String),
    Completed(TurnCompleted),
}

#[derive(Debug, Clone)]
pub struct TurnAssembler {
    thread_id: String,
    turn_id: Option<String>,
    final_item_id: Option<String>,
    authoritative_final_text: Option<String>,
    observed_text: String,
    usage: Option<TurnUsage>,
    terminal: bool,
}

impl TurnAssembler {
    pub fn new(thread_id: impl Into<String>) -> Result<Self, ProtocolError> {
        let thread_id = thread_id.into();
        if !valid_identifier(&thread_id) {
            return Err(ProtocolError::InvalidIdentifier);
        }
        Ok(Self {
            thread_id,
            turn_id: None,
            final_item_id: None,
            authoritative_final_text: None,
            observed_text: String::new(),
            usage: None,
            terminal: false,
        })
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    pub fn observed_text(&self) -> &str {
        &self.observed_text
    }

    pub fn accept(&mut self, message: &RpcMessage) -> Result<Vec<TurnEvent>, ProtocolError> {
        let RpcMessage::Notification { method, params } = message else {
            return Ok(Vec::new());
        };
        let object = params.as_object().ok_or(ProtocolError::InvalidEnvelope)?;
        match method.as_str() {
            "turn/started" => {
                if object.get("threadId").and_then(Value::as_str) != Some(&self.thread_id) {
                    return Ok(Vec::new());
                }
                let turn = object
                    .get("turn")
                    .and_then(Value::as_object)
                    .ok_or(ProtocolError::InvalidTurn)?;
                let turn_id = string_field(turn, "id")?;
                if let Some(current) = &self.turn_id {
                    if current != &turn_id {
                        return Err(ProtocolError::InvalidTurn);
                    }
                } else {
                    self.turn_id = Some(turn_id);
                }
                Ok(Vec::new())
            }
            "item/started" | "item/completed" => {
                if !self.same_turn(object) {
                    return Ok(Vec::new());
                }
                let item = object
                    .get("item")
                    .and_then(Value::as_object)
                    .ok_or(ProtocolError::InvalidEnvelope)?;
                let item_type = item
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or(ProtocolError::UnsupportedItem)?;
                if !is_allowed_item_type(item_type) {
                    return Err(ProtocolError::UnsupportedItem);
                }
                if item_type == "agentMessage" {
                    let id = string_field(item, "id")?;
                    let phase = item.get("phase").and_then(Value::as_str);
                    if phase != Some("commentary") && phase != Some("analysis") {
                        if let Some(current) = &self.final_item_id
                            && current != &id
                        {
                            return Err(ProtocolError::InvalidTurn);
                        }
                        self.final_item_id = Some(id);
                        // `item/started` commonly carries an empty or
                        // provisional text field. Only the completed item is
                        // authoritative; comparing the start payload to the
                        // final text would reject valid native turns.
                        if method == "item/completed" {
                            let Some(text) = item.get("text") else {
                                return Err(ProtocolError::InvalidTurn);
                            };
                            let text = text_field(text)?;
                            if self
                                .authoritative_final_text
                                .as_ref()
                                .is_some_and(|current| current != &text)
                            {
                                return Err(ProtocolError::FinalTextMismatch);
                            }
                            self.authoritative_final_text = Some(text);
                        }
                    }
                }
                Ok(Vec::new())
            }
            "item/agentMessage/delta" => {
                if !self.same_turn(object) {
                    return Ok(Vec::new());
                }
                let Some(item_id) = object.get("itemId").and_then(Value::as_str) else {
                    return Err(ProtocolError::InvalidEnvelope);
                };
                // Deltas without a preceding final agent-message item are not
                // streamed: their phase is unknown and could be commentary.
                if self.final_item_id.as_deref() != Some(item_id) {
                    return Ok(Vec::new());
                }
                let delta = object
                    .get("delta")
                    .and_then(Value::as_str)
                    .ok_or(ProtocolError::InvalidEnvelope)?;
                if self.observed_text.len().saturating_add(delta.len()) > MAX_ANSWER_BYTES {
                    return Err(ProtocolError::OutputLimit);
                }
                self.observed_text.push_str(delta);
                Ok(vec![TurnEvent::AssistantDelta(delta.to_owned())])
            }
            "thread/tokenUsage/updated" => {
                if !self.same_turn(object) {
                    return Ok(Vec::new());
                }
                self.usage = Some(parse_thread_usage(object.get("tokenUsage"))?);
                Ok(Vec::new())
            }
            "turn/completed" => {
                if object.get("threadId").and_then(Value::as_str) != Some(self.thread_id.as_str()) {
                    return Ok(Vec::new());
                }
                if self.terminal {
                    return Err(ProtocolError::DuplicateTerminal);
                }
                let turn = object
                    .get("turn")
                    .and_then(Value::as_object)
                    .ok_or(ProtocolError::InvalidTurn)?;
                let turn_id = string_field(turn, "id")?;
                if let Some(current) = &self.turn_id {
                    if current != &turn_id {
                        return Err(ProtocolError::InvalidTurn);
                    }
                } else {
                    // A terminal notification can be the first turn event
                    // observed when the turn/start response is delayed or
                    // lost. The thread-scoped turn ID is still an
                    // unambiguous acknowledgement for this submitted turn.
                    self.turn_id = Some(turn_id.clone());
                }
                let status = TurnStatus::from_str(
                    turn.get("status")
                        .and_then(Value::as_str)
                        .ok_or(ProtocolError::InvalidTurn)?,
                )?;
                if status == TurnStatus::InProgress {
                    return Err(ProtocolError::InvalidTurn);
                }
                let terminal_items_empty = turn
                    .get("items")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty);
                let text = match final_text(turn, self.final_item_id.as_deref()) {
                    Ok(text) => text,
                    Err(ProtocolError::InvalidTurn)
                        if status == TurnStatus::Completed
                            && terminal_items_empty
                            && self.authoritative_final_text.is_some() =>
                    {
                        self.authoritative_final_text
                            .clone()
                            .ok_or(ProtocolError::InvalidTurn)?
                    }
                    Err(ProtocolError::InvalidTurn) if status != TurnStatus::Completed => {
                        // Interrupted/failed turns may legitimately omit a
                        // final agent item. Preserve the validated streamed
                        // prefix as the safe partial result.
                        self.observed_text.clone()
                    }
                    Err(error) => return Err(error),
                };
                if text.len() > MAX_ANSWER_BYTES {
                    return Err(ProtocolError::OutputLimit);
                }
                if (status == TurnStatus::Completed || !text.is_empty())
                    && !text.starts_with(&self.observed_text)
                {
                    return Err(ProtocolError::FinalTextMismatch);
                }
                self.terminal = true;
                let usage = match turn.get("usage") {
                    Some(value) => parse_usage(Some(value))?,
                    None => self.usage,
                };
                let failure = parse_turn_failure(turn);
                Ok(vec![TurnEvent::Completed(TurnCompleted {
                    thread_id: self.thread_id.clone(),
                    turn_id,
                    status,
                    text,
                    usage,
                    failure,
                })])
            }
            _ => Ok(Vec::new()),
        }
    }

    fn same_turn(&self, object: &Map<String, Value>) -> bool {
        object.get("threadId").and_then(Value::as_str) == Some(self.thread_id.as_str())
            && self
                .turn_id
                .as_deref()
                .is_none_or(|turn_id| object.get("turnId").and_then(Value::as_str) == Some(turn_id))
    }
}

fn final_text(turn: &Map<String, Value>, selected: Option<&str>) -> Result<String, ProtocolError> {
    let items = turn
        .get("items")
        .and_then(Value::as_array)
        .ok_or(ProtocolError::InvalidTurn)?;
    let mut candidates = Vec::new();
    for item in items {
        let Some(item) = item.as_object() else {
            return Err(ProtocolError::UnsupportedItem);
        };
        let item_type = item
            .get("type")
            .and_then(Value::as_str)
            .ok_or(ProtocolError::UnsupportedItem)?;
        if !is_allowed_item_type(item_type) {
            return Err(ProtocolError::UnsupportedItem);
        }
        if item_type != "agentMessage" {
            continue;
        }
        let id = string_field(item, "id")?;
        let phase = item.get("phase").and_then(Value::as_str);
        if phase == Some("commentary") || phase == Some("analysis") {
            continue;
        }
        let text = text_field(item.get("text").ok_or(ProtocolError::InvalidTurn)?)?;
        if selected.is_none() || selected == Some(id.as_str()) {
            candidates.push(text);
        }
    }
    candidates.pop().ok_or(ProtocolError::InvalidTurn)
}

fn text_field(value: &Value) -> Result<String, ProtocolError> {
    let value = value.as_str().ok_or(ProtocolError::InvalidEnvelope)?;
    if value.len() > MAX_ANSWER_BYTES {
        return Err(ProtocolError::OutputLimit);
    }
    Ok(value.to_owned())
}

fn is_allowed_item_type(item_type: &str) -> bool {
    // These are passive story/conversation records. Tool execution, file
    // changes, MCP calls, and any future item type remain fail-closed until
    // the application has an explicit policy and validator for them.
    matches!(
        item_type,
        "userMessage" | "reasoning" | "agentMessage" | "contextCompaction"
    )
}

fn parse_turn_failure(turn: &Map<String, Value>) -> Option<TurnFailure> {
    let info = turn
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("codexErrorInfo"))?;
    match info {
        Value::String(code) if is_allowed_failure_code(code) => Some(TurnFailure {
            code: code.clone(),
            http_status_code: None,
        }),
        Value::Object(object) if object.len() == 1 => {
            let (code, payload) = object.iter().next()?;
            if !is_allowed_failure_code(code) {
                return None;
            }
            let http_status_code = payload
                .as_object()
                .and_then(|payload| payload.get("httpStatusCode"))
                .and_then(Value::as_u64)
                .filter(|status| (100..=599).contains(status))
                .and_then(|status| u16::try_from(status).ok());
            Some(TurnFailure {
                code: code.clone(),
                http_status_code,
            })
        }
        _ => None,
    }
}

fn is_allowed_failure_code(code: &str) -> bool {
    matches!(
        code,
        "contextWindowExceeded"
            | "sessionBudgetExceeded"
            | "usageLimitExceeded"
            | "rateLimitExceeded"
            | "serverOverloaded"
            | "cyberPolicy"
            | "misalignmentPolicyViolation"
            | "internalServerError"
            | "unauthorized"
            | "badRequest"
            | "threadRollbackFailed"
            | "sandboxError"
            | "other"
            | "httpConnectionFailed"
            | "responseStreamConnectionFailed"
            | "responseStreamDisconnected"
            | "responseTooManyFailedAttempts"
            | "activeTurnNotSteerable"
    )
}

fn parse_usage(value: Option<&Value>) -> Result<Option<TurnUsage>, ProtocolError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let object = value.as_object().ok_or(ProtocolError::InvalidEnvelope)?;
    let number = |name: &str| -> Result<u64, ProtocolError> {
        object
            .get(name)
            .and_then(Value::as_u64)
            .ok_or(ProtocolError::InvalidEnvelope)
    };
    Ok(Some(TurnUsage {
        input_tokens: number("inputTokens")?,
        cached_input_tokens: object
            .get("cachedInputTokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cache_write_input_tokens: object
            .get("cacheWriteInputTokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: number("outputTokens")?,
        reasoning_output_tokens: object
            .get("reasoningOutputTokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    }))
}

fn parse_thread_usage(object: Option<&Value>) -> Result<TurnUsage, ProtocolError> {
    let object = object
        .and_then(Value::as_object)
        .ok_or(ProtocolError::InvalidEnvelope)?;
    // `last` is the usage for this turn. `total` is intentionally ignored:
    // retaining cumulative counters here would double-count when the server
    // emits more than one update for the same turn.
    let last = object.get("last").ok_or(ProtocolError::InvalidEnvelope)?;
    parse_usage(Some(last))?.ok_or(ProtocolError::InvalidEnvelope)
}

fn string_field(object: &Map<String, Value>, name: &str) -> Result<String, ProtocolError> {
    let value = object
        .get(name)
        .and_then(Value::as_str)
        .ok_or(ProtocolError::InvalidEnvelope)?;
    if !valid_identifier(value) {
        return Err(ProtocolError::InvalidIdentifier);
    }
    Ok(value.to_owned())
}

fn nullable_string_field(
    object: &Map<String, Value>,
    name: &str,
) -> Result<Option<String>, ProtocolError> {
    match object.get(name) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if valid_identifier(value) => Ok(Some(value.clone())),
        _ => Err(ProtocolError::InvalidEnvelope),
    }
}

pub fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(value: Value) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        bytes
    }

    fn thread_start_result() -> Value {
        serde_json::json!({
            "thread": {"id":"thread-1", "ephemeral":true, "path":null},
            "model":"gpt-6-astra",
            "reasoningEffort":"low",
            "serviceTier":"default",
            "instructionSources": []
        })
    }

    #[test]
    fn decoder_handles_utf8_split_and_crlf() {
        let mut decoder = JsonlDecoder::new();
        let bytes =
            line(serde_json::json!({"jsonrpc":"2.0","method":"notice","params":{"text":"é"}}));
        let split = bytes.iter().position(|byte| *byte == 0xc3).unwrap() + 1;
        assert!(decoder.push(&bytes[..split]).unwrap().is_empty());
        assert_eq!(decoder.push(&bytes[split..]).unwrap().len(), 1);
    }

    #[test]
    fn decoder_rejects_malformed_and_incomplete_frames() {
        let mut decoder = JsonlDecoder::new();
        assert_eq!(
            decoder.push(b"{nope}\n").unwrap_err(),
            ProtocolError::InvalidJson
        );
        let mut decoder = JsonlDecoder::new();
        assert!(decoder.push(b"{\"jsonrpc\":\"2.0\"}").is_err() || decoder.finish().is_err());
    }

    #[test]
    fn responses_and_server_requests_are_distinguished() {
        let mut decoder = JsonlDecoder::new();
        let bytes = [
            line(serde_json::json!({"jsonrpc":"2.0","id":"x","result":{}})),
            line(
                serde_json::json!({"jsonrpc":"2.0","id":"y","method":"server/request","params":{}}),
            ),
        ]
        .concat();
        let messages = decoder.push(&bytes).unwrap();
        assert!(matches!(messages[0], RpcMessage::Response { .. }));
        assert!(matches!(messages[1], RpcMessage::ServerRequest { .. }));
    }

    #[test]
    fn decoder_accepts_native_frames_without_jsonrpc_version() {
        let mut decoder = JsonlDecoder::new();
        let messages = decoder
            .push(&line(serde_json::json!({"id":"native","result":{}})))
            .unwrap();
        assert!(
            matches!(messages.as_slice(), [RpcMessage::Response { id: RpcId::String(id), .. }] if id == "native")
        );
        assert!(
            decoder
                .push(&line(serde_json::json!({"jsonrpc":"1.0","method":"bad"})))
                .is_err()
        );
    }

    #[test]
    fn thread_start_requires_empty_instruction_sources_and_exact_settings() {
        let requested = ThreadStartConfig {
            cwd: Some("C:/story".into()),
            base_instructions: None,
            developer_instructions: Some("write".into()),
            model: "gpt-6-astra".into(),
            reasoning_effort: Some("low".into()),
            service_tier: "default".into(),
        };
        assert_eq!(
            ThreadStartAck::from_result(&thread_start_result(), &requested)
                .unwrap()
                .thread_id,
            "thread-1"
        );
        let mut bad = thread_start_result();
        bad["instructionSources"] = serde_json::json!(["C:/ambient.md"]);
        assert!(ThreadStartAck::from_result(&bad, &requested).is_err());
    }

    #[test]
    fn assembler_ignores_interleaved_threads_and_commentary() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        let started = RpcMessage::Notification {
            method: "turn/started".into(),
            params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
        };
        assembler.accept(&started).unwrap();
        let other = RpcMessage::Notification {
            method: "item/agentMessage/delta".into(),
            params: serde_json::json!({"threadId":"thread-2","turnId":"turn-2","itemId":"item-2","delta":"wrong"}),
        };
        assert!(assembler.accept(&other).unwrap().is_empty());
        let commentary = RpcMessage::Notification {
            method: "item/started".into(),
            params: serde_json::json!({"threadId":"thread-1","turnId":"turn-1","item":{"id":"comment","type":"agentMessage","phase":"commentary"}}),
        };
        assembler.accept(&commentary).unwrap();
        let final_item = RpcMessage::Notification {
            method: "item/started".into(),
            params: serde_json::json!({"threadId":"thread-1","turnId":"turn-1","item":{"id":"answer","type":"agentMessage","phase":"final"}}),
        };
        assembler.accept(&final_item).unwrap();
        let delta = RpcMessage::Notification {
            method: "item/agentMessage/delta".into(),
            params: serde_json::json!({"threadId":"thread-1","turnId":"turn-1","itemId":"answer","delta":"hello"}),
        };
        assert!(
            matches!(assembler.accept(&delta).unwrap().as_slice(), [TurnEvent::AssistantDelta(text)] if text == "hello")
        );
        let completed = RpcMessage::Notification {
            method: "turn/completed".into(),
            params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1","status":"completed","items":[{"id":"comment","type":"agentMessage","phase":"commentary","text":"aside"},{"id":"answer","type":"agentMessage","phase":"final","text":"hello world"}]}}),
        };
        let events = assembler.accept(&completed).unwrap();
        assert!(matches!(&events[0], TurnEvent::Completed(result) if result.text == "hello world"));
    }

    #[test]
    fn passive_items_are_allowed_but_tool_items_fail_closed() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            })
            .unwrap();
        let completed = RpcMessage::Notification {
            method: "turn/completed".into(),
            params: serde_json::json!({
                "threadId":"thread-1",
                "turn": {"id":"turn-1","status":"completed","items":[
                    {"id":"prompt","type":"userMessage"},
                    {"id":"thought","type":"reasoning"},
                    {"id":"compact","type":"contextCompaction"},
                    {"id":"answer","type":"agentMessage","phase":"final","text":"done"}
                ]}
            }),
        };
        assert!(matches!(
            assembler.accept(&completed).unwrap().as_slice(),
            [TurnEvent::Completed(result)] if result.text == "done"
        ));

        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            })
            .unwrap();
        let unsupported = RpcMessage::Notification {
            method: "turn/completed".into(),
            params: serde_json::json!({
                "threadId":"thread-1",
                "turn": {"id":"turn-1","status":"completed","items":[
                    {"id":"shell","type":"commandExecution","command":"echo unsafe"},
                    {"id":"answer","type":"agentMessage","phase":"final","text":"done"}
                ]}
            }),
        };
        assert_eq!(
            assembler.accept(&unsupported).unwrap_err(),
            ProtocolError::UnsupportedItem
        );
    }

    #[test]
    fn unsupported_item_on_other_thread_is_ignored_without_poisoning() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            })
            .unwrap();
        let other_thread_tool = RpcMessage::Notification {
            method: "item/started".into(),
            params: serde_json::json!({
                "threadId":"thread-2","turnId":"turn-2",
                "item":{"id":"shell","type":"commandExecution"}
            }),
        };
        assert!(assembler.accept(&other_thread_tool).unwrap().is_empty());
        let valid_final = RpcMessage::Notification {
            method: "turn/completed".into(),
            params: serde_json::json!({
                "threadId":"thread-1",
                "turn":{"id":"turn-1","status":"completed","items":[
                    {"id":"answer","type":"agentMessage","phase":"final","text":"done"}
                ]}
            }),
        };
        assert!(matches!(
            assembler.accept(&valid_final).unwrap().as_slice(),
            [TurnEvent::Completed(result)] if result.text == "done"
        ));
    }

    #[test]
    fn completed_empty_items_uses_authoritative_item_completion_text() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            })
            .unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "item/started".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "item":{"id":"answer","type":"agentMessage","phase":"final"}
                }),
            })
            .unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "item/agentMessage/delta".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "itemId":"answer","delta":"Hello"
                }),
            })
            .unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "item/completed".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "item":{"id":"answer","type":"agentMessage","phase":"final","text":"Hello world"}
                }),
            })
            .unwrap();
        let completed = assembler
            .accept(&RpcMessage::Notification {
                method: "turn/completed".into(),
                params: serde_json::json!({
                    "threadId":"thread-1",
                    "turn":{"id":"turn-1","status":"completed","items":[]}
                }),
            })
            .unwrap();
        assert!(matches!(
            completed.as_slice(),
            [TurnEvent::Completed(result)] if result.text == "Hello world"
        ));
    }

    #[test]
    fn completed_turn_with_only_unconfirmed_deltas_is_rejected() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        for message in [
            RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            },
            RpcMessage::Notification {
                method: "item/started".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "item":{"id":"answer","type":"agentMessage","phase":"final"}
                }),
            },
            RpcMessage::Notification {
                method: "item/agentMessage/delta".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "itemId":"answer","delta":"unconfirmed"
                }),
            },
        ] {
            assembler.accept(&message).unwrap();
        }
        let completed = RpcMessage::Notification {
            method: "turn/completed".into(),
            params: serde_json::json!({
                "threadId":"thread-1",
                "turn":{"id":"turn-1","status":"completed","items":[]}
            }),
        };
        assert_eq!(
            assembler.accept(&completed).unwrap_err(),
            ProtocolError::InvalidTurn
        );
    }

    #[test]
    fn failed_turn_exposes_only_allowlisted_failure_metadata() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            })
            .unwrap();
        let completed = RpcMessage::Notification {
            method: "turn/completed".into(),
            params: serde_json::json!({
                "threadId":"thread-1",
                "turn": {
                    "id":"turn-1",
                    "status":"failed",
                    "items":[],
                    "error": {
                        "message":"do not retain this provider detail",
                        "additionalDetails":"do not retain this either",
                        "codexErrorInfo": {
                            "httpConnectionFailed": {"httpStatusCode": 429}
                        }
                    }
                }
            }),
        };
        let events = assembler.accept(&completed).unwrap();
        assert!(matches!(
            events.as_slice(),
            [TurnEvent::Completed(result)]
                if result.failure == Some(TurnFailure {
                    code: "httpConnectionFailed".into(),
                    http_status_code: Some(429),
                })
        ));

        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            })
            .unwrap();
        let invalid_status = serde_json::json!({
            "threadId":"thread-1",
            "turn": {
                "id":"turn-1", "status":"failed", "items":[],
                "error":{"codexErrorInfo":{"rateLimitExceeded":{"httpStatusCode":700}}}
            }
        });
        let events = assembler
            .accept(&RpcMessage::Notification {
                method: "turn/completed".into(),
                params: invalid_status,
            })
            .unwrap();
        assert!(matches!(
            events.as_slice(),
            [TurnEvent::Completed(result)]
                if result.failure == Some(TurnFailure {
                    code: "rateLimitExceeded".into(),
                    http_status_code: None,
                })
        ));
    }

    #[test]
    fn turn_failure_safe_detail_preserves_allowlisted_code_and_status_only() {
        let failure = TurnFailure {
            code: "rateLimitExceeded".into(),
            http_status_code: Some(429),
        };
        assert_eq!(
            failure.safe_detail(),
            "Codex provider failure: rateLimitExceeded (HTTP 429)"
        );

        let forged = TurnFailure {
            code: "raw upstream message\nwith diagnostics".into(),
            http_status_code: Some(700),
        };
        assert_eq!(forged.safe_detail(), "Codex provider failure: other");
    }

    #[test]
    fn started_empty_agent_item_then_completed_full_text_is_authoritative() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        for message in [
            RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            },
            RpcMessage::Notification {
                method: "item/started".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "item":{"id":"answer","type":"agentMessage","phase":"final","text":""}
                }),
            },
            RpcMessage::Notification {
                method: "item/agentMessage/delta".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "itemId":"answer","delta":"Hello world"
                }),
            },
            RpcMessage::Notification {
                method: "item/completed".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "item":{"id":"answer","type":"agentMessage","phase":"final","text":"Hello world"}
                }),
            },
        ] {
            assembler.accept(&message).unwrap();
        }
        let events = assembler
            .accept(&RpcMessage::Notification {
                method: "turn/completed".into(),
                params: serde_json::json!({
                    "threadId":"thread-1",
                    "turn":{"id":"turn-1","status":"completed","items":[]}
                }),
            })
            .unwrap();
        assert!(matches!(
            events.as_slice(),
            [TurnEvent::Completed(result)] if result.text == "Hello world"
        ));
    }

    #[test]
    fn completed_agent_item_conflicting_with_streamed_text_is_rejected() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        for message in [
            RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            },
            RpcMessage::Notification {
                method: "item/started".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "item":{"id":"answer","type":"agentMessage","phase":"final","text":""}
                }),
            },
            RpcMessage::Notification {
                method: "item/agentMessage/delta".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "itemId":"answer","delta":"Hello"
                }),
            },
        ] {
            assembler.accept(&message).unwrap();
        }
        assembler
            .accept(&RpcMessage::Notification {
                method: "item/completed".into(),
                params: serde_json::json!({
                    "threadId":"thread-1","turnId":"turn-1",
                    "item":{"id":"answer","type":"agentMessage","phase":"final","text":"Goodbye"}
                }),
            })
            .unwrap();
        let error = assembler
            .accept(&RpcMessage::Notification {
                method: "turn/completed".into(),
                params: serde_json::json!({
                    "threadId":"thread-1",
                    "turn":{"id":"turn-1","status":"completed","items":[]}
                }),
            })
            .unwrap_err();
        assert_eq!(error, ProtocolError::FinalTextMismatch);
    }

    #[test]
    fn final_text_must_extend_observed_prefix_and_terminal_is_sticky() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        for message in [
            RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            },
            RpcMessage::Notification {
                method: "item/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turnId":"turn-1","item":{"id":"answer","type":"agentMessage"}}),
            },
            RpcMessage::Notification {
                method: "item/agentMessage/delta".into(),
                params: serde_json::json!({"threadId":"thread-1","turnId":"turn-1","itemId":"answer","delta":"hello"}),
            },
        ] {
            assembler.accept(&message).unwrap();
        }
        let bad = RpcMessage::Notification {
            method: "turn/completed".into(),
            params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1","status":"completed","items":[{"id":"answer","type":"agentMessage","text":"goodbye"}]}}),
        };
        assert_eq!(
            assembler.accept(&bad).unwrap_err(),
            ProtocolError::FinalTextMismatch
        );
    }

    #[test]
    fn interrupted_turn_can_settle_without_an_agent_message() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            })
            .unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "item/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turnId":"turn-1","item":{"id":"answer","type":"agentMessage","phase":"final"}}),
            })
            .unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "item/agentMessage/delta".into(),
                params: serde_json::json!({"threadId":"thread-1","turnId":"turn-1","itemId":"answer","delta":"partial"}),
            })
            .unwrap();
        let completed = RpcMessage::Notification {
            method: "turn/completed".into(),
            params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1","status":"interrupted","items":[]}}),
        };
        assert!(
            matches!(assembler.accept(&completed).unwrap().as_slice(), [TurnEvent::Completed(result)] if result.status == TurnStatus::Interrupted)
        );
    }

    #[test]
    fn interrupt_response_is_not_a_turn_terminal() {
        let response = RpcMessage::Response {
            id: RpcId::string("interrupt"),
            result: Some(serde_json::json!({})),
            error: None,
        };
        assert!(matches!(response, RpcMessage::Response { .. }));
    }

    #[test]
    fn usage_updates_replace_with_last_turn_counters() {
        let mut assembler = TurnAssembler::new("thread-1").unwrap();
        assembler
            .accept(&RpcMessage::Notification {
                method: "turn/started".into(),
                params: serde_json::json!({"threadId":"thread-1","turn":{"id":"turn-1"}}),
            })
            .unwrap();
        for (input, output, total) in [(3, 2, 100), (4, 5, 101)] {
            assembler
                .accept(&RpcMessage::Notification {
                    method: "thread/tokenUsage/updated".into(),
                    params: serde_json::json!({
                        "threadId":"thread-1",
                        "turnId":"turn-1",
                        "tokenUsage": {
                            "last": {"inputTokens":input,"outputTokens":output},
                            "total": {"inputTokens":total,"outputTokens":total}
                        }
                    }),
                })
                .unwrap();
        }
        let completed = assembler
            .accept(&RpcMessage::Notification {
                method: "turn/completed".into(),
                params: serde_json::json!({
                    "threadId":"thread-1",
                    "turn":{"id":"turn-1","status":"completed","items":[
                        {"id":"answer","type":"agentMessage","phase":"final","text":"done"}
                    ]}
                }),
            })
            .unwrap();
        assert!(matches!(
            completed.as_slice(),
            [TurnEvent::Completed(result)]
                if result.usage == Some(TurnUsage {
                    input_tokens: 4,
                    cached_input_tokens: 0,
                    cache_write_input_tokens: 0,
                    output_tokens: 5,
                    reasoning_output_tokens: 0,
                })
        ));
    }
}
