//! A bounded, pure decoder for Claude CLI JSONL output.
//!
//! This module intentionally does not start a process, read configuration, or
//! authenticate.  A future adapter can feed stdout chunks to
//! [`ClaudeJsonlParser`] and decide how to report its typed outcome.

use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;

/// Maximum bytes in one JSONL record, excluding its line ending.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Maximum bytes accepted from one invocation.
pub const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
/// Maximum JSONL records accepted from one invocation.
pub const MAX_EVENTS: usize = 8 * 1024;
/// Maximum textual data observed, including ignored thinking blocks.
pub const MAX_TEXT_BYTES: usize = 512 * 1024;
/// Maximum simultaneously tracked stream blocks.
pub const MAX_BLOCKS: usize = 128;

/// A newly visible assistant suffix.  Thinking and redacted thinking are
/// deliberately never emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeEvent {
    AssistantDelta(String),
}

/// Usage reported by Claude, when the provider includes it.  Individual
/// counters remain optional because message-start/message-delta records often
/// report only one part of the eventual usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaudeUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
}

/// A successfully completed single assistant response.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeOutcome {
    pub assistant_text: String,
    pub session_id: String,
    pub model: Option<String>,
    pub stop_reason: String,
    pub result_subtype: String,
    /// Convenience selection: result usage when reported, otherwise stream
    /// usage. Use the two origin fields below when the distinction matters.
    pub usage: Option<ClaudeUsage>,
    /// Cumulative usage observed in message-start/message-delta records (or a
    /// full assistant message when no partial stream was present).
    pub stream_usage: Option<ClaudeUsage>,
    /// Usage reported by the terminal CLI result, preserved without filling
    /// omitted fields from stream usage.
    pub result_usage: Option<ClaudeUsage>,
}

/// Stable, sanitized failure classes for callers and UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeFailureCode {
    InvalidUtf8,
    InvalidJson,
    Protocol,
    UnsupportedEvent,
    ConfigurationRejected,
    InvalidUsage,
    ProviderFailure,
    Refused,
    Truncated,
    LimitExceeded,
    Incomplete,
    NonZeroExit,
}

impl fmt::Display for ClaudeFailureCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidUtf8 => "InvalidUtf8",
            Self::InvalidJson => "InvalidJson",
            Self::Protocol => "Protocol",
            Self::UnsupportedEvent => "UnsupportedEvent",
            Self::ConfigurationRejected => "ConfigurationRejected",
            Self::InvalidUsage => "InvalidUsage",
            Self::ProviderFailure => "ProviderFailure",
            Self::Refused => "Refused",
            Self::Truncated => "Truncated",
            Self::LimitExceeded => "LimitExceeded",
            Self::Incomplete => "Incomplete",
            Self::NonZeroExit => "NonZeroExit",
        })
    }
}

/// A sticky, sanitized failure.  `partial_output` contains only validated
/// assistant text observed before the failure.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeFailure {
    pub code: ClaudeFailureCode,
    pub detail: String,
    pub partial_output: String,
}

impl fmt::Display for ClaudeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for ClaudeFailure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Text,
    Thinking,
    RedactedThinking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlockState {
    kind: BlockKind,
    stopped: bool,
}

/// Incremental parser for one Claude JSONL stdout stream.
#[derive(Debug, Default)]
pub struct ClaudeJsonlParser {
    buffer: Vec<u8>,
    total_bytes: usize,
    event_count: usize,
    observed_text_bytes: usize,
    session_id: Option<String>,
    message_id: Option<String>,
    model: Option<String>,
    init_seen: bool,
    stream_message_started: bool,
    stream_message_stopped: bool,
    full_assistant_seen: bool,
    blocks: BTreeMap<u64, BlockState>,
    stream_text_block_count: usize,
    stop_reason: Option<String>,
    stream_usage: Option<ClaudeUsage>,
    result_usage: Option<ClaudeUsage>,
    assistant_text: String,
    result_seen: bool,
    result_subtype: Option<String>,
    result_text: Option<String>,
    sticky_failure: Option<ClaudeFailure>,
    finished: bool,
}

impl ClaudeJsonlParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed an arbitrary stdout chunk. UTF-8 records may be split across
    /// calls. Once a failure is returned, every later operation returns the
    /// same sanitized failure.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        if let Some(failure) = &self.sticky_failure {
            return Err(failure.clone());
        }
        if self.finished {
            return self.fail(ClaudeFailureCode::Protocol, "parser is already finished");
        }
        if bytes.len() > MAX_TOTAL_BYTES.saturating_sub(self.total_bytes) {
            return self.fail(
                ClaudeFailureCode::LimitExceeded,
                "stdout byte limit exceeded",
            );
        }
        self.total_bytes += bytes.len();
        self.buffer.extend_from_slice(bytes);

        let mut events = Vec::new();
        loop {
            let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') else {
                if self.buffer.len() > MAX_LINE_BYTES {
                    return self.fail(
                        ClaudeFailureCode::LimitExceeded,
                        "JSONL line limit exceeded",
                    );
                }
                break;
            };
            if newline > MAX_LINE_BYTES {
                return self.fail(
                    ClaudeFailureCode::LimitExceeded,
                    "JSONL line limit exceeded",
                );
            }
            let mut line = self.buffer.drain(..=newline).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            events.extend(self.process_line(&line)?);
        }
        Ok(events)
    }

    /// Finish one stream. The final JSON record may omit its newline. A
    /// nonzero process exit never becomes a successful provider result.
    pub fn finish(&mut self, exit_code: i32) -> Result<ClaudeOutcome, ClaudeFailure> {
        if let Some(failure) = &self.sticky_failure {
            return Err(failure.clone());
        }
        if self.finished {
            return self.fail(ClaudeFailureCode::Protocol, "parser is already finished");
        }
        self.finished = true;
        if !self.buffer.is_empty() {
            if self.buffer.len() > MAX_LINE_BYTES {
                return self.fail(
                    ClaudeFailureCode::LimitExceeded,
                    "JSONL line limit exceeded",
                );
            }
            let mut line = std::mem::take(&mut self.buffer);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line)?;
        }
        if exit_code != 0 {
            return self.fail(
                ClaudeFailureCode::NonZeroExit,
                "provider process exited nonzero",
            );
        }
        if !self.init_seen || !self.result_seen {
            return self.fail(
                ClaudeFailureCode::Incomplete,
                "stream did not complete initialization and result",
            );
        }
        if self.stream_message_started && !self.stream_message_stopped {
            return self.fail(ClaudeFailureCode::Incomplete, "stream message did not stop");
        }
        if self.blocks.values().any(|block| !block.stopped) {
            return self.fail(
                ClaudeFailureCode::Incomplete,
                "stream contains an incomplete content block",
            );
        }
        if self.assistant_text.is_empty() {
            return self.fail(ClaudeFailureCode::Incomplete, "assistant response is empty");
        }
        let Some(stop_reason) = self.stop_reason.clone() else {
            return self.fail(
                ClaudeFailureCode::Incomplete,
                "assistant response has no stop reason",
            );
        };
        self.validate_stop_reason(&stop_reason)?;
        let Some(result_text) = &self.result_text else {
            return self.fail(ClaudeFailureCode::Incomplete, "result has no response text");
        };
        if result_text != &self.assistant_text {
            return self.fail(
                ClaudeFailureCode::Protocol,
                "result text disagrees with assistant text",
            );
        }
        Ok(ClaudeOutcome {
            assistant_text: self.assistant_text.clone(),
            session_id: self.session_id.clone().expect("init checked above"),
            model: self.model.clone(),
            stop_reason,
            result_subtype: self.result_subtype.clone().expect("result checked above"),
            usage: self.result_usage.or(self.stream_usage),
            stream_usage: self.stream_usage,
            result_usage: self.result_usage,
        })
    }

    fn process_line(&mut self, line: &[u8]) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        let result = self.process_line_inner(line);
        if let Err(failure) = &result {
            self.sticky_failure = Some(failure.clone());
        }
        result
    }

    fn process_line_inner(&mut self, line: &[u8]) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        if line.is_empty() {
            return self.fail(ClaudeFailureCode::InvalidJson, "empty JSONL record");
        }
        self.event_count = self.event_count.checked_add(1).ok_or_else(|| {
            self.failure(ClaudeFailureCode::LimitExceeded, "event limit exceeded")
        })?;
        if self.event_count > MAX_EVENTS {
            return self.fail(ClaudeFailureCode::LimitExceeded, "event limit exceeded");
        }
        let text = std::str::from_utf8(line).map_err(|_| {
            self.failure(ClaudeFailureCode::InvalidUtf8, "JSONL record is not UTF-8")
        })?;
        let value = serde_json::from_str::<Value>(text).map_err(|_| {
            self.failure(
                ClaudeFailureCode::InvalidJson,
                "JSONL record is not valid JSON",
            )
        })?;
        let object = value.as_object().ok_or_else(|| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "JSONL record must be an object",
            )
        })?;
        let event_type = object.get("type").and_then(Value::as_str).ok_or_else(|| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "JSONL record has no event type",
            )
        })?;
        self.handle_event(event_type, object)
    }

    fn handle_event(
        &mut self,
        event_type: &str,
        object: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        if self.result_seen {
            return self.fail(ClaudeFailureCode::Protocol, "event appeared after result");
        }
        match event_type {
            "system" => self.handle_system(object),
            "stream_event" => self.handle_stream_event(object),
            "assistant" => self.handle_assistant(object),
            "result" => self.handle_result(object),
            "user" => self.fail(
                ClaudeFailureCode::UnsupportedEvent,
                "user messages are not accepted",
            ),
            "error" => self.provider_failure(),
            _ => self.fail(ClaudeFailureCode::Protocol, "unknown JSONL event type"),
        }
    }

    fn handle_system(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        if object.get("subtype").and_then(Value::as_str) != Some("init") {
            return self.fail(ClaudeFailureCode::Protocol, "unknown system event subtype");
        }
        if self.init_seen || self.stream_message_started || self.full_assistant_seen {
            return self.fail(ClaudeFailureCode::Protocol, "init event is out of order");
        }
        let session_id = required_nonempty_string(object, "session_id")
            .ok_or_else(|| self.failure(ClaudeFailureCode::Protocol, "init has no session id"))?;
        self.require_empty_array(object, "tools")?;
        self.require_empty_array(object, "mcp_servers")?;
        for field in ["plugins", "agents", "skills"] {
            if object.contains_key(field) {
                self.require_empty_array(object, field)?;
            }
        }
        if let Some(model) = optional_nonempty_string(object, "model").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.set_model(model)?;
        }
        self.session_id = Some(session_id.to_owned());
        self.init_seen = true;
        Ok(Vec::new())
    }

    fn handle_stream_event(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        self.require_initialized()?;
        if object.get("error").is_some_and(|value| !value.is_null()) {
            return self.provider_failure();
        }
        if self.full_assistant_seen {
            return self.fail(
                ClaudeFailureCode::Protocol,
                "stream appeared after full assistant message",
            );
        }
        if let Some(session_id) = optional_nonempty_string(object, "session_id").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.check_session(session_id)?;
        }
        if object
            .get("parent_tool_use_id")
            .is_some_and(|value| !value.is_null())
        {
            return self.unsupported_surface();
        }
        let event = object
            .get("event")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "stream event has no nested event",
                )
            })?;
        let nested_type = required_nonempty_string(event, "type").ok_or_else(|| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "stream event has no nested type",
            )
        })?;
        if event.get("error").is_some_and(|value| !value.is_null()) {
            return self.provider_failure();
        }
        match nested_type {
            "message_start" => self.handle_message_start(event),
            "content_block_start" => self.handle_content_block_start(object, event),
            "content_block_delta" => self.handle_content_block_delta(object, event),
            "content_block_stop" => self.handle_content_block_stop(object, event),
            "message_delta" => self.handle_message_delta(event),
            "message_stop" => self.handle_message_stop(),
            _ => self.fail(ClaudeFailureCode::Protocol, "unknown stream event type"),
        }
    }

    fn handle_message_start(
        &mut self,
        event: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        if self.stream_message_started {
            return self.fail(ClaudeFailureCode::Protocol, "duplicate message start");
        }
        let message = event
            .get("message")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                self.failure(ClaudeFailureCode::Protocol, "message start has no message")
            })?;
        if message.get("error").is_some_and(|value| !value.is_null()) {
            return self.provider_failure();
        }
        if message
            .get("parent_tool_use_id")
            .is_some_and(|value| !value.is_null())
        {
            return self.unsupported_surface();
        }
        if message.get("type").and_then(Value::as_str) != Some("message")
            || message.get("role").and_then(Value::as_str) != Some("assistant")
        {
            return self.fail(
                ClaudeFailureCode::Protocol,
                "message start is not an assistant message",
            );
        }
        if let Some(message_id) = optional_nonempty_string(message, "id").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.set_message_id(message_id)?;
        }
        if let Some(content) = message.get("content") {
            let content = content.as_array().ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "message start content is not an array",
                )
            })?;
            if !content.is_empty() {
                return self.unsupported_surface();
            }
        }
        if let Some(session_id) = optional_nonempty_string(message, "session_id").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.check_session(session_id)?;
        }
        if let Some(model) = optional_nonempty_string(message, "model").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.set_model(model)?;
        }
        if let Some(usage) = message.get("usage") {
            self.merge_stream_usage(
                parse_usage(usage).map_err(|code| self.failure(code, "usage is invalid"))?,
            )?;
        }
        self.stream_message_started = true;
        Ok(Vec::new())
    }

    fn handle_content_block_start(
        &mut self,
        outer: &Map<String, Value>,
        event: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        self.require_stream_message()?;
        let index = self.block_index(outer, event)?;
        if self.blocks.contains_key(&index) {
            return self.fail(ClaudeFailureCode::Protocol, "duplicate content block index");
        }
        if self.blocks.len() >= MAX_BLOCKS {
            return self.fail(
                ClaudeFailureCode::LimitExceeded,
                "content block limit exceeded",
            );
        }
        let block = event
            .get("content_block")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "content block start has no block",
                )
            })?;
        let mut events = Vec::new();
        let kind = match required_nonempty_string(block, "type") {
            Some("text") => {
                let text = required_string(block, "text").ok_or_else(|| {
                    self.failure(ClaudeFailureCode::Protocol, "text block has no text")
                })?;
                self.observe_text(text)?;
                self.stream_text_block_count = self.stream_text_block_count.saturating_add(1);
                if self.stream_text_block_count > 1 {
                    return self.fail(
                        ClaudeFailureCode::UnsupportedEvent,
                        "multiple text blocks are not accepted",
                    );
                }
                if !text.is_empty() {
                    events = self.append_text(text)?;
                }
                BlockKind::Text
            }
            Some("thinking") => {
                let thinking = required_string(block, "thinking").ok_or_else(|| {
                    self.failure(ClaudeFailureCode::Protocol, "thinking block has no text")
                })?;
                self.observe_text(thinking)?;
                BlockKind::Thinking
            }
            Some("redacted_thinking") => {
                let data = required_string(block, "data").ok_or_else(|| {
                    self.failure(
                        ClaudeFailureCode::Protocol,
                        "redacted thinking block has no data",
                    )
                })?;
                self.observe_text(data)?;
                BlockKind::RedactedThinking
            }
            Some("tool_use") | Some("server_tool_use") | Some("tool_result") => {
                return self.unsupported_surface();
            }
            _ => return self.fail(ClaudeFailureCode::Protocol, "unknown content block type"),
        };
        self.blocks.insert(
            index,
            BlockState {
                kind,
                stopped: false,
            },
        );
        Ok(events)
    }

    fn handle_content_block_delta(
        &mut self,
        outer: &Map<String, Value>,
        event: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        self.require_stream_message()?;
        let index = self.block_index(outer, event)?;
        let block = self.blocks.get(&index).copied().ok_or_else(|| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "content block delta has no block",
            )
        })?;
        if block.stopped {
            return self.fail(
                ClaudeFailureCode::Protocol,
                "content block changed after stop",
            );
        }
        let delta = event
            .get("delta")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "content block delta has no delta",
                )
            })?;
        let delta_type = required_nonempty_string(delta, "type").ok_or_else(|| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "content block delta has no type",
            )
        })?;
        match (block.kind, delta_type) {
            (BlockKind::Text, "text_delta") => {
                let text = required_string(delta, "text").ok_or_else(|| {
                    self.failure(ClaudeFailureCode::Protocol, "text delta has no text")
                })?;
                self.observe_text(text)?;
                self.append_text(text)
            }
            (BlockKind::Thinking, "thinking_delta") => {
                let thinking = required_string(delta, "thinking").ok_or_else(|| {
                    self.failure(ClaudeFailureCode::Protocol, "thinking delta has no text")
                })?;
                self.observe_text(thinking)?;
                Ok(Vec::new())
            }
            (BlockKind::Thinking, "signature_delta") => {
                let signature = required_string(delta, "signature").ok_or_else(|| {
                    self.failure(
                        ClaudeFailureCode::Protocol,
                        "signature delta has no signature",
                    )
                })?;
                self.observe_text(signature)?;
                Ok(Vec::new())
            }
            (BlockKind::RedactedThinking, "redacted_thinking_delta") => {
                let data = required_string(delta, "data").ok_or_else(|| {
                    self.failure(
                        ClaudeFailureCode::Protocol,
                        "redacted thinking delta has no data",
                    )
                })?;
                self.observe_text(data)?;
                Ok(Vec::new())
            }
            (_, "input_json_delta") | (_, "tool_use_delta") => self.unsupported_surface(),
            _ => self.fail(
                ClaudeFailureCode::Protocol,
                "incompatible content block delta",
            ),
        }
    }

    fn handle_content_block_stop(
        &mut self,
        outer: &Map<String, Value>,
        event: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        self.require_stream_message()?;
        let index = self.block_index(outer, event)?;
        if !self.blocks.contains_key(&index) {
            return self.fail(
                ClaudeFailureCode::Protocol,
                "content block stop has no block",
            );
        }
        let block = self
            .blocks
            .get_mut(&index)
            .expect("block existence checked");
        if block.stopped {
            return self.fail(ClaudeFailureCode::Protocol, "duplicate content block stop");
        }
        block.stopped = true;
        Ok(Vec::new())
    }

    fn handle_message_delta(
        &mut self,
        event: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        self.require_stream_message()?;
        let delta = event
            .get("delta")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                self.failure(ClaudeFailureCode::Protocol, "message delta has no delta")
            })?;
        if let Some(value) = delta.get("stop_reason")
            && !value.is_null()
        {
            let stop_reason = required_nonempty_string(delta, "stop_reason").ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "message delta stop reason is invalid",
                )
            })?;
            self.set_stop_reason(stop_reason)?;
        }
        if let Some(usage) = event.get("usage") {
            self.merge_stream_usage(
                parse_usage(usage).map_err(|code| self.failure(code, "usage is invalid"))?,
            )?;
        }
        Ok(Vec::new())
    }

    fn handle_message_stop(&mut self) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        self.require_stream_message()?;
        if self.stream_message_stopped {
            return self.fail(ClaudeFailureCode::Protocol, "duplicate message stop");
        }
        if self.blocks.values().any(|block| !block.stopped) {
            return self.fail(
                ClaudeFailureCode::Protocol,
                "message stopped with open content block",
            );
        }
        self.stream_message_stopped = true;
        Ok(Vec::new())
    }

    fn handle_assistant(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        self.require_initialized()?;
        if self.full_assistant_seen {
            return self.fail(
                ClaudeFailureCode::Protocol,
                "duplicate full assistant message",
            );
        }
        if let Some(parent) = object.get("parent_tool_use_id")
            && !parent.is_null()
        {
            return self.unsupported_surface();
        }
        if let Some(session_id) = optional_nonempty_string(object, "session_id").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.check_session(session_id)?;
        }
        let message = object
            .get("message")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "assistant event has no message",
                )
            })?;
        if object.get("error").is_some_and(|value| !value.is_null())
            || message.get("error").is_some_and(|value| !value.is_null())
        {
            return self.provider_failure();
        }
        if message.get("type").and_then(Value::as_str) != Some("message")
            || message.get("role").and_then(Value::as_str) != Some("assistant")
        {
            return self.fail(
                ClaudeFailureCode::Protocol,
                "assistant event is not an assistant message",
            );
        }
        if let Some(message_id) = optional_nonempty_string(message, "id").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.set_message_id(message_id)?;
        }
        if let Some(model) = optional_nonempty_string(message, "model").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.set_model(model)?;
        }
        if let Some(usage) = message.get("usage") {
            let usage =
                parse_usage(usage).map_err(|code| self.failure(code, "usage is invalid"))?;
            if self.stream_message_started {
                if let Some(stream_usage) = self.stream_usage {
                    if stream_usage != usage {
                        return self.fail(
                            ClaudeFailureCode::InvalidUsage,
                            "full assistant usage disagrees with stream usage",
                        );
                    }
                } else {
                    self.stream_usage = Some(usage);
                }
            } else {
                self.stream_usage = Some(usage);
            }
        }
        if let Some(stop_reason) =
            optional_nonempty_string(message, "stop_reason").map_err(|_| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "optional string field is invalid",
                )
            })?
        {
            self.set_stop_reason(stop_reason)?;
        }
        let content = message
            .get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "assistant message content is not an array",
                )
            })?;
        let mut text = String::new();
        let mut text_blocks = 0usize;
        for block in content {
            let block = block.as_object().ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "assistant content block is not an object",
                )
            })?;
            match required_nonempty_string(block, "type") {
                Some("text") => {
                    let value = required_string(block, "text").ok_or_else(|| {
                        self.failure(
                            ClaudeFailureCode::Protocol,
                            "assistant text block has no text",
                        )
                    })?;
                    self.observe_text(value)?;
                    text_blocks = text_blocks.saturating_add(1);
                    if text_blocks > 1 {
                        return self.fail(
                            ClaudeFailureCode::UnsupportedEvent,
                            "multiple text blocks are not accepted",
                        );
                    }
                    text.push_str(value);
                }
                Some("thinking") => {
                    let value = required_string(block, "thinking").ok_or_else(|| {
                        self.failure(
                            ClaudeFailureCode::Protocol,
                            "assistant thinking block has no text",
                        )
                    })?;
                    self.observe_text(value)?;
                }
                Some("redacted_thinking") => {
                    let value = required_string(block, "data").ok_or_else(|| {
                        self.failure(
                            ClaudeFailureCode::Protocol,
                            "assistant redacted thinking block has no data",
                        )
                    })?;
                    self.observe_text(value)?;
                }
                Some("tool_use") | Some("server_tool_use") | Some("tool_result") => {
                    return self.unsupported_surface();
                }
                _ => {
                    return self.fail(
                        ClaudeFailureCode::Protocol,
                        "unknown assistant content block type",
                    );
                }
            }
        }
        let events = if self.stream_message_started {
            if text != self.assistant_text {
                return self.fail(
                    ClaudeFailureCode::Protocol,
                    "full assistant message disagrees with streamed text",
                );
            }
            Vec::new()
        } else if !text.is_empty() {
            self.append_text(&text)?
        } else {
            Vec::new()
        };
        self.full_assistant_seen = true;
        Ok(events)
    }

    fn handle_result(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        if object.get("error").is_some_and(|value| !value.is_null()) {
            return self.provider_failure();
        }
        if object.get("is_error").and_then(Value::as_bool) == Some(true) {
            return self.provider_failure();
        }
        if object.get("is_error").and_then(Value::as_bool) != Some(false) {
            return self.fail(
                ClaudeFailureCode::Protocol,
                "result has no explicit nonerror status",
            );
        }
        if object.get("subtype").and_then(Value::as_str) != Some("success") {
            return self.provider_failure();
        }
        if object.contains_key("permission_denials") || object.contains_key("deferred_tool_use") {
            return self.unsupported_surface();
        }
        if object
            .get("parent_tool_use_id")
            .is_some_and(|value| !value.is_null())
        {
            return self.unsupported_surface();
        }
        self.require_initialized()?;
        if self.stream_message_started && !self.stream_message_stopped {
            return self.fail(
                ClaudeFailureCode::Incomplete,
                "result arrived before message stop",
            );
        }
        if let Some(session_id) = optional_nonempty_string(object, "session_id").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.check_session(session_id)?;
        }
        if let Some(model) = optional_nonempty_string(object, "model").map_err(|_| {
            self.failure(
                ClaudeFailureCode::Protocol,
                "optional string field is invalid",
            )
        })? {
            self.set_model(model)?;
        }
        if let Some(stop_reason) =
            optional_nonempty_string(object, "stop_reason").map_err(|_| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "optional string field is invalid",
                )
            })?
        {
            self.set_stop_reason(stop_reason)?;
        }
        if let Some(usage) = object.get("usage") {
            self.merge_result_usage(
                parse_usage(usage).map_err(|code| self.failure(code, "usage is invalid"))?,
            )?;
        }
        if let Some(cost) = object.get("total_cost_usd") {
            let value = cost.as_f64().ok_or_else(|| {
                self.failure(ClaudeFailureCode::InvalidUsage, "result cost is invalid")
            })?;
            if !value.is_finite() || value < 0.0 {
                return self.fail(ClaudeFailureCode::InvalidUsage, "result cost is invalid");
            }
        }
        let result = required_string(object, "result").ok_or_else(|| {
            self.failure(ClaudeFailureCode::Protocol, "result has no response text")
        })?;
        self.observe_text(result)?;
        self.result_text = Some(result.to_owned());
        self.result_subtype = Some("success".to_owned());
        self.result_seen = true;
        Ok(Vec::new())
    }

    fn append_text(&mut self, text: &str) -> Result<Vec<ClaudeEvent>, ClaudeFailure> {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let new_len = self
            .assistant_text
            .len()
            .checked_add(text.len())
            .ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::LimitExceeded,
                    "assistant text limit exceeded",
                )
            })?;
        if new_len > MAX_TEXT_BYTES {
            return self.fail(
                ClaudeFailureCode::LimitExceeded,
                "assistant text limit exceeded",
            );
        }
        self.assistant_text.push_str(text);
        Ok(vec![ClaudeEvent::AssistantDelta(text.to_owned())])
    }

    fn observe_text(&mut self, text: &str) -> Result<(), ClaudeFailure> {
        let new_total = self
            .observed_text_bytes
            .checked_add(text.len())
            .ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::LimitExceeded,
                    "provider text limit exceeded",
                )
            })?;
        if new_total > MAX_TEXT_BYTES {
            return self.fail(
                ClaudeFailureCode::LimitExceeded,
                "provider text limit exceeded",
            );
        }
        self.observed_text_bytes = new_total;
        Ok(())
    }

    fn block_index(
        &self,
        outer: &Map<String, Value>,
        event: &Map<String, Value>,
    ) -> Result<u64, ClaudeFailure> {
        event
            .get("index")
            .or_else(|| outer.get("index"))
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                self.failure(
                    ClaudeFailureCode::Protocol,
                    "content block has no valid index",
                )
            })
    }

    fn require_initialized(&self) -> Result<(), ClaudeFailure> {
        if self.init_seen {
            Ok(())
        } else {
            Err(self.failure(ClaudeFailureCode::Protocol, "event appeared before init"))
        }
    }

    fn require_stream_message(&self) -> Result<(), ClaudeFailure> {
        if self.stream_message_started && !self.stream_message_stopped {
            Ok(())
        } else {
            Err(self.failure(ClaudeFailureCode::Protocol, "stream event is out of order"))
        }
    }

    fn require_empty_array(
        &self,
        object: &Map<String, Value>,
        field: &str,
    ) -> Result<(), ClaudeFailure> {
        let value = object.get(field).ok_or_else(|| {
            self.failure(
                ClaudeFailureCode::ConfigurationRejected,
                "init is missing execution surface list",
            )
        })?;
        if !value.as_array().is_some_and(|array| array.is_empty()) {
            return Err(self.failure(
                ClaudeFailureCode::ConfigurationRejected,
                "init declares an execution surface",
            ));
        }
        Ok(())
    }

    fn check_session(&self, session_id: &str) -> Result<(), ClaudeFailure> {
        if self.session_id.as_deref() == Some(session_id) {
            Ok(())
        } else {
            Err(self.failure(ClaudeFailureCode::Protocol, "session identity changed"))
        }
    }

    fn set_model(&mut self, model: &str) -> Result<(), ClaudeFailure> {
        if let Some(previous) = &self.model {
            if previous != model {
                return self.fail(ClaudeFailureCode::Protocol, "reported model changed");
            }
        } else {
            self.observe_text(model)?;
            self.model = Some(model.to_owned());
        }
        Ok(())
    }

    fn set_message_id(&mut self, message_id: &str) -> Result<(), ClaudeFailure> {
        if let Some(previous) = &self.message_id {
            if previous != message_id {
                return self.fail(ClaudeFailureCode::Protocol, "message identity changed");
            }
        } else {
            self.message_id = Some(message_id.to_owned());
        }
        Ok(())
    }

    fn set_stop_reason(&mut self, stop_reason: &str) -> Result<(), ClaudeFailure> {
        if let Some(previous) = &self.stop_reason {
            if previous != stop_reason {
                return self.fail(ClaudeFailureCode::Protocol, "stop reason changed");
            }
        } else {
            self.stop_reason = Some(stop_reason.to_owned());
        }
        Ok(())
    }

    fn validate_stop_reason(&mut self, stop_reason: &str) -> Result<(), ClaudeFailure> {
        match stop_reason {
            "end_turn" | "stop_sequence" => Ok(()),
            "refusal" => self.fail(ClaudeFailureCode::Refused, "provider refused the request"),
            "max_tokens" | "max_output_tokens" | "length" => self.fail(
                ClaudeFailureCode::Truncated,
                "provider response was truncated",
            ),
            "tool_use" => self.unsupported_surface(),
            _ => self.fail(ClaudeFailureCode::Protocol, "unknown assistant stop reason"),
        }
    }

    fn merge_stream_usage(&mut self, usage: ClaudeUsage) -> Result<(), ClaudeFailure> {
        self.stream_usage = Some(match self.stream_usage {
            Some(previous) => merge_usage_values(previous, usage).map_err(|_| {
                self.failure(ClaudeFailureCode::InvalidUsage, "stream usage regressed")
            })?,
            None => usage,
        });
        Ok(())
    }

    fn merge_result_usage(&mut self, usage: ClaudeUsage) -> Result<(), ClaudeFailure> {
        if let Some(stream) = self.stream_usage
            && !usage_covers_stream(stream, usage)
        {
            return self.fail(
                ClaudeFailureCode::InvalidUsage,
                "result usage regressed from stream usage",
            );
        }
        self.result_usage = Some(usage);
        Ok(())
    }

    fn unsupported_surface<T>(&mut self) -> Result<T, ClaudeFailure> {
        self.fail(
            ClaudeFailureCode::UnsupportedEvent,
            "tool, user, or subagent surface is not accepted",
        )
    }

    fn provider_failure<T>(&mut self) -> Result<T, ClaudeFailure> {
        self.fail(
            ClaudeFailureCode::ProviderFailure,
            "provider reported a failure",
        )
    }

    fn failure(&self, code: ClaudeFailureCode, detail: &str) -> ClaudeFailure {
        ClaudeFailure {
            code,
            detail: detail.to_owned(),
            partial_output: self.assistant_text.clone(),
        }
    }

    fn fail<T>(&mut self, code: ClaudeFailureCode, detail: &str) -> Result<T, ClaudeFailure> {
        let failure = self.failure(code, detail);
        self.sticky_failure = Some(failure.clone());
        Err(failure)
    }
}

fn required_string<'a>(object: &'a Map<String, Value>, field: &str) -> Option<&'a str> {
    object.get(field).and_then(Value::as_str)
}

fn required_nonempty_string<'a>(object: &'a Map<String, Value>, field: &str) -> Option<&'a str> {
    required_string(object, field).filter(|value| !value.is_empty())
}

fn optional_nonempty_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, ()> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(value) = value.as_str().filter(|value| !value.is_empty()) else {
        return Err(());
    };
    Ok(Some(value))
}

fn parse_usage(value: &Value) -> Result<ClaudeUsage, ClaudeFailureCode> {
    let object = value.as_object().ok_or(ClaudeFailureCode::InvalidUsage)?;
    Ok(ClaudeUsage {
        input_tokens: parse_counter(object, "input_tokens")?,
        output_tokens: parse_counter(object, "output_tokens")?,
        cache_creation_input_tokens: parse_counter(object, "cache_creation_input_tokens")?,
        cache_read_input_tokens: parse_counter(object, "cache_read_input_tokens")?,
    })
}

fn parse_counter(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<u64>, ClaudeFailureCode> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    value
        .as_u64()
        .map(Some)
        .ok_or(ClaudeFailureCode::InvalidUsage)
}

fn merge_usage_values(previous: ClaudeUsage, next: ClaudeUsage) -> Result<ClaudeUsage, ()> {
    Ok(ClaudeUsage {
        input_tokens: merge_counter(previous.input_tokens, next.input_tokens)?,
        output_tokens: merge_counter(previous.output_tokens, next.output_tokens)?,
        cache_creation_input_tokens: merge_counter(
            previous.cache_creation_input_tokens,
            next.cache_creation_input_tokens,
        )?,
        cache_read_input_tokens: merge_counter(
            previous.cache_read_input_tokens,
            next.cache_read_input_tokens,
        )?,
    })
}

fn usage_covers_stream(stream: ClaudeUsage, result: ClaudeUsage) -> bool {
    counter_covers(stream.input_tokens, result.input_tokens)
        && counter_covers(stream.output_tokens, result.output_tokens)
        && counter_covers(
            stream.cache_creation_input_tokens,
            result.cache_creation_input_tokens,
        )
        && counter_covers(
            stream.cache_read_input_tokens,
            result.cache_read_input_tokens,
        )
}

fn counter_covers(stream: Option<u64>, result: Option<u64>) -> bool {
    match (stream, result) {
        (Some(stream), Some(result)) => result >= stream,
        _ => true,
    }
}

/// Claude stream and result usage counters are cumulative.  Missing fields
/// remain unknown, while a present counter may stay equal or increase.
fn merge_counter(previous: Option<u64>, next: Option<u64>) -> Result<Option<u64>, ()> {
    match (previous, next) {
        (Some(previous), Some(next)) if next >= previous => Ok(Some(next)),
        (Some(previous), None) => Ok(Some(previous)),
        (None, Some(next)) => Ok(Some(next)),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(()),
    }
}
