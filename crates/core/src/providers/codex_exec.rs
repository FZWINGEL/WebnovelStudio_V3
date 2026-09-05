//! A bounded, pure parser for the JSONL protocol emitted by a Codex exec.
//!
//! This module deliberately owns no process, filesystem, SQLite, or Tauri
//! behavior. A process adapter can feed stdout chunks into
//! CodexJsonlParser and make its own retry and lifecycle decisions from the
//! typed outcome.

use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;

/// Maximum bytes in one JSONL record, excluding its line ending.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Maximum bytes accepted from one invocation.
pub const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
/// Maximum JSONL records accepted from one invocation.
pub const MAX_EVENTS: usize = 8 * 1024;
/// Maximum newly observed provider text, including reasoning and warnings.
pub const MAX_TEXT_BYTES: usize = 512 * 1024;
/// Maximum warning records retained in an outcome.
pub const MAX_WARNINGS: usize = 512;
/// Maximum warning text retained in an outcome.
pub const MAX_WARNING_BYTES: usize = 512 * 1024;

/// A bounded item emitted while feeding stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexEvent {
    /// A newly observed suffix of the single assistant message.
    AssistantDelta(String),
    /// A bounded provider warning from an error item.
    Warning(String),
}

/// Usage reported by turn.completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodexUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}

/// Successful parser result. The output is the exact concatenation of
/// assistant deltas; reasoning is intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexOutcome {
    pub assistant_text: String,
    pub usage: CodexUsage,
    pub warnings: Vec<String>,
}

/// Stable, sanitized failure classes for callers and UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexFailureCode {
    InvalidUtf8,
    InvalidJson,
    Protocol,
    UnsupportedItem,
    InvalidUsage,
    ProviderFailure,
    LimitExceeded,
    Incomplete,
    NonZeroExit,
}

impl fmt::Display for CodexFailureCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidUtf8 => "InvalidUtf8",
            Self::InvalidJson => "InvalidJson",
            Self::Protocol => "Protocol",
            Self::UnsupportedItem => "UnsupportedItem",
            Self::InvalidUsage => "InvalidUsage",
            Self::ProviderFailure => "ProviderFailure",
            Self::LimitExceeded => "LimitExceeded",
            Self::Incomplete => "Incomplete",
            Self::NonZeroExit => "NonZeroExit",
        })
    }
}

/// A sticky failure. partial_output contains only validated assistant text
/// seen before the failure; malformed input is never copied into this value or
/// the failure detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexFailure {
    pub code: CodexFailureCode,
    pub detail: String,
    pub partial_output: String,
}

impl fmt::Display for CodexFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for CodexFailure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Terminal {
    Completed,
}

#[derive(Debug, Clone)]
struct ItemState {
    item_type: String,
    text: String,
    completed: bool,
    last_item: Value,
    last_event: String,
}

/// Incremental parser for one Codex JSONL stdout stream.
#[derive(Debug, Default)]
pub struct CodexJsonlParser {
    buffer: Vec<u8>,
    total_bytes: usize,
    event_count: usize,
    observed_text_bytes: usize,
    warning_bytes: usize,
    warnings: Vec<String>,
    thread_id: Option<String>,
    turn_started: bool,
    terminal: Option<Terminal>,
    usage: Option<CodexUsage>,
    items: BTreeMap<String, ItemState>,
    completed_assistant_messages: usize,
    assistant_text: String,
    sticky_failure: Option<CodexFailure>,
    finished: bool,
}

impl CodexJsonlParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed an arbitrary stdout chunk. UTF-8 records may be split across
    /// calls. Once a failure is returned, every later operation returns the
    /// same sanitized failure.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<CodexEvent>, CodexFailure> {
        if let Some(failure) = &self.sticky_failure {
            return Err(failure.clone());
        }
        if self.finished {
            return self.fail(CodexFailureCode::Protocol, "parser is already finished");
        }
        if bytes.len() > MAX_TOTAL_BYTES.saturating_sub(self.total_bytes) {
            return self.fail(
                CodexFailureCode::LimitExceeded,
                "stdout byte limit exceeded",
            );
        }
        self.total_bytes += bytes.len();
        self.buffer.extend_from_slice(bytes);

        let mut events = Vec::new();
        loop {
            let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') else {
                if self.buffer.len() > MAX_LINE_BYTES {
                    return self.fail(CodexFailureCode::LimitExceeded, "JSONL line limit exceeded");
                }
                break;
            };
            if newline > MAX_LINE_BYTES {
                return self.fail(CodexFailureCode::LimitExceeded, "JSONL line limit exceeded");
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

    /// Finish one stream. A final record need not have a newline. The process
    /// exit code is supplied by the caller; a nonzero code never becomes a
    /// successful provider result.
    pub fn finish(&mut self, exit_code: i32) -> Result<CodexOutcome, CodexFailure> {
        if let Some(failure) = &self.sticky_failure {
            return Err(failure.clone());
        }
        if self.finished {
            return self.fail(CodexFailureCode::Protocol, "parser is already finished");
        }
        self.finished = true;
        if !self.buffer.is_empty() {
            if self.buffer.len() > MAX_LINE_BYTES {
                return self.fail(CodexFailureCode::LimitExceeded, "JSONL line limit exceeded");
            }
            let mut line = std::mem::take(&mut self.buffer);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line)?;
        }
        if exit_code != 0 {
            return self.fail(
                CodexFailureCode::NonZeroExit,
                "provider process exited nonzero",
            );
        }
        if self.thread_id.is_none() || !self.turn_started || self.terminal.is_none() {
            return self.fail(
                CodexFailureCode::Incomplete,
                "stream did not complete one thread and turn",
            );
        }
        if self.items.values().any(|item| !item.completed) {
            return self.fail(
                CodexFailureCode::Incomplete,
                "stream contains an incomplete item",
            );
        }
        if self.completed_assistant_messages != 1 {
            return self.fail(
                CodexFailureCode::Protocol,
                "stream must contain exactly one completed assistant message",
            );
        }
        if self.assistant_text.is_empty() {
            return self.fail(
                CodexFailureCode::Incomplete,
                "completed assistant message is empty",
            );
        }
        let Some(usage) = self.usage else {
            return self.fail(
                CodexFailureCode::InvalidUsage,
                "completed turn has no usage",
            );
        };
        Ok(CodexOutcome {
            assistant_text: self.assistant_text.clone(),
            usage,
            warnings: self.warnings.clone(),
        })
    }

    fn process_line(&mut self, line: &[u8]) -> Result<Vec<CodexEvent>, CodexFailure> {
        let result = self.process_line_inner(line);
        if let Err(failure) = &result {
            self.sticky_failure = Some(failure.clone());
        }
        result
    }

    fn process_line_inner(&mut self, line: &[u8]) -> Result<Vec<CodexEvent>, CodexFailure> {
        if line.is_empty() {
            return self.fail(CodexFailureCode::InvalidJson, "empty JSONL record");
        }
        self.event_count = self
            .event_count
            .checked_add(1)
            .ok_or_else(|| self.failure(CodexFailureCode::LimitExceeded, "event limit exceeded"))?;
        if self.event_count > MAX_EVENTS {
            return self.fail(CodexFailureCode::LimitExceeded, "event limit exceeded");
        }
        let text = std::str::from_utf8(line).map_err(|_| {
            self.failure(CodexFailureCode::InvalidUtf8, "JSONL record is not UTF-8")
        })?;
        let value = serde_json::from_str::<Value>(text).map_err(|_| {
            self.failure(
                CodexFailureCode::InvalidJson,
                "JSONL record is not valid JSON",
            )
        })?;
        let object = value.as_object().ok_or_else(|| {
            self.failure(CodexFailureCode::Protocol, "JSONL record must be an object")
        })?;
        let event_type = object.get("type").and_then(Value::as_str).ok_or_else(|| {
            self.failure(CodexFailureCode::Protocol, "JSONL record has no event type")
        })?;
        self.handle_event(event_type, object)
    }

    fn handle_event(
        &mut self,
        event_type: &str,
        object: &Map<String, Value>,
    ) -> Result<Vec<CodexEvent>, CodexFailure> {
        if self.terminal.is_some() {
            return self.fail(
                CodexFailureCode::Protocol,
                "event appeared after terminal event",
            );
        }
        match event_type {
            "thread.started" => {
                if self.thread_id.is_some() || self.turn_started {
                    return self.fail(CodexFailureCode::Protocol, "duplicate thread.started event");
                }
                let thread_id = required_string(object, "thread_id").ok_or_else(|| {
                    self.failure(
                        CodexFailureCode::Protocol,
                        "thread.started has no thread id",
                    )
                })?;
                if thread_id.is_empty() {
                    return self.fail(
                        CodexFailureCode::Protocol,
                        "thread.started has an empty thread id",
                    );
                }
                self.thread_id = Some(thread_id.to_owned());
                Ok(Vec::new())
            }
            "turn.started" => {
                if self.thread_id.is_none() || self.turn_started {
                    return self.fail(CodexFailureCode::Protocol, "turn.started is out of order");
                }
                self.turn_started = true;
                Ok(Vec::new())
            }
            "turn.completed" => {
                if self.thread_id.is_none() || !self.turn_started {
                    return self.fail(CodexFailureCode::Protocol, "turn.completed is out of order");
                }
                let usage_value = object.get("usage").ok_or_else(|| {
                    self.failure(
                        CodexFailureCode::InvalidUsage,
                        "turn.completed has no usage",
                    )
                })?;
                let usage = parse_usage(usage_value).ok_or_else(|| {
                    self.failure(
                        CodexFailureCode::InvalidUsage,
                        "turn.completed usage is invalid",
                    )
                })?;
                self.usage = Some(usage);
                self.terminal = Some(Terminal::Completed);
                Ok(Vec::new())
            }
            "turn.failed" => self.provider_failure(),
            "error" => self.provider_failure(),
            "item.started" | "item.updated" | "item.completed" => {
                if self.thread_id.is_none() {
                    return self.fail(
                        CodexFailureCode::Protocol,
                        "item appeared before thread.started",
                    );
                }
                let item = object.get("item").ok_or_else(|| {
                    self.failure(CodexFailureCode::Protocol, "item event has no item")
                })?;
                self.handle_item(event_type, item)
            }
            _ => self.fail(CodexFailureCode::Protocol, "unknown JSONL event type"),
        }
    }

    fn handle_item(
        &mut self,
        event_type: &str,
        item: &Value,
    ) -> Result<Vec<CodexEvent>, CodexFailure> {
        let object = item
            .as_object()
            .ok_or_else(|| self.failure(CodexFailureCode::Protocol, "item must be an object"))?;
        let item_id = required_string(object, "id")
            .ok_or_else(|| self.failure(CodexFailureCode::Protocol, "item has no id"))?;
        if item_id.is_empty() {
            return self.fail(CodexFailureCode::Protocol, "item has an empty id");
        }
        let item_type = required_string(object, "type")
            .ok_or_else(|| self.failure(CodexFailureCode::Protocol, "item has no type"))?;
        let (kind, text) = match item_type {
            "agent_message" | "reasoning" => (
                item_type,
                required_string(object, "text").ok_or_else(|| {
                    self.failure(CodexFailureCode::Protocol, "text item has no text")
                })?,
            ),
            "error" => (
                item_type,
                required_string(object, "message").ok_or_else(|| {
                    self.failure(CodexFailureCode::Protocol, "error item has no message")
                })?,
            ),
            _ => return self.fail(CodexFailureCode::UnsupportedItem, "unsupported item type"),
        };
        if kind != "error" && !self.turn_started {
            return self.fail(
                CodexFailureCode::Protocol,
                "text item appeared before turn.started",
            );
        }
        if kind == "error" && text.is_empty() {
            return self.fail(
                CodexFailureCode::Protocol,
                "error item has an empty message",
            );
        }

        let completed_event = event_type == "item.completed";
        let prior = self.items.get(item_id).cloned();
        let delta = if let Some(state) = &prior {
            if state.item_type != kind {
                return self.fail(CodexFailureCode::Protocol, "item id changed type");
            }
            if state.last_item == *item {
                if state.completed {
                    if state.last_event == "item.completed" && completed_event {
                        return Ok(Vec::new());
                    }
                    return self.fail(
                        CodexFailureCode::Protocol,
                        "completed item received a conflicting terminal event",
                    );
                }
                if completed_event {
                    let state = ItemState {
                        item_type: state.item_type.clone(),
                        text: state.text.clone(),
                        completed: true,
                        last_item: state.last_item.clone(),
                        last_event: event_type.to_owned(),
                    };
                    self.items.insert(item_id.to_owned(), state);
                    if kind == "agent_message" {
                        self.completed_assistant_messages += 1;
                    }
                }
                return Ok(Vec::new());
            }
            if state.completed {
                return self.fail(
                    CodexFailureCode::Protocol,
                    "completed item changed after terminal snapshot",
                );
            }
            if !text.starts_with(&state.text) {
                return self.fail(
                    CodexFailureCode::Protocol,
                    "item snapshot was not append-only",
                );
            }
            &text[state.text.len()..]
        } else {
            text
        };
        self.observe_text(delta)?;

        let state = ItemState {
            item_type: kind.to_owned(),
            text: text.to_owned(),
            completed: prior.as_ref().is_some_and(|state| state.completed) || completed_event,
            last_item: item.clone(),
            last_event: event_type.to_owned(),
        };
        self.items.insert(item_id.to_owned(), state);

        if completed_event
            && !prior.as_ref().is_some_and(|state| state.completed)
            && kind == "agent_message"
        {
            self.completed_assistant_messages += 1;
        }

        if delta.is_empty() {
            return Ok(Vec::new());
        }
        match kind {
            "agent_message" => {
                self.assistant_text.push_str(delta);
                Ok(vec![CodexEvent::AssistantDelta(delta.to_owned())])
            }
            "error" => {
                let warning = "provider warning";
                self.observe_warning(warning)?;
                Ok(vec![CodexEvent::Warning(warning.to_owned())])
            }
            "reasoning" => Ok(Vec::new()),
            _ => unreachable!(),
        }
    }

    fn observe_text(&mut self, text: &str) -> Result<(), CodexFailure> {
        let new_total = self
            .observed_text_bytes
            .checked_add(text.len())
            .ok_or_else(|| {
                self.failure(
                    CodexFailureCode::LimitExceeded,
                    "provider text limit exceeded",
                )
            })?;
        if new_total > MAX_TEXT_BYTES {
            return self.fail(
                CodexFailureCode::LimitExceeded,
                "provider text limit exceeded",
            );
        }
        self.observed_text_bytes = new_total;
        Ok(())
    }

    fn observe_warning(&mut self, warning: &str) -> Result<(), CodexFailure> {
        if self.warnings.len() >= MAX_WARNINGS {
            return self.fail(CodexFailureCode::LimitExceeded, "warning limit exceeded");
        }
        let new_total = self
            .warning_bytes
            .checked_add(warning.len())
            .ok_or_else(|| {
                self.failure(
                    CodexFailureCode::LimitExceeded,
                    "warning text limit exceeded",
                )
            })?;
        if new_total > MAX_WARNING_BYTES {
            return self.fail(
                CodexFailureCode::LimitExceeded,
                "warning text limit exceeded",
            );
        }
        self.warning_bytes = new_total;
        self.warnings.push(warning.to_owned());
        Ok(())
    }

    fn provider_failure(&mut self) -> Result<Vec<CodexEvent>, CodexFailure> {
        self.fail(
            CodexFailureCode::ProviderFailure,
            "provider reported a failure",
        )
    }

    fn failure(&self, code: CodexFailureCode, detail: &str) -> CodexFailure {
        CodexFailure {
            code,
            detail: detail.to_owned(),
            partial_output: self.assistant_text.clone(),
        }
    }

    fn fail<T>(&mut self, code: CodexFailureCode, detail: &str) -> Result<T, CodexFailure> {
        self.fail_with_detail(code, detail.to_owned())
    }

    fn fail_with_detail<T>(
        &mut self,
        code: CodexFailureCode,
        detail: String,
    ) -> Result<T, CodexFailure> {
        let failure = CodexFailure {
            code,
            detail,
            partial_output: self.assistant_text.clone(),
        };
        self.sticky_failure = Some(failure.clone());
        Err(failure)
    }
}

fn required_string<'a>(object: &'a Map<String, Value>, field: &str) -> Option<&'a str> {
    object.get(field).and_then(Value::as_str)
}

fn parse_usage(value: &Value) -> Option<CodexUsage> {
    let object = value.as_object()?;
    Some(CodexUsage {
        input_tokens: parse_nonnegative_i64(object.get("input_tokens")?)?,
        cached_input_tokens: parse_nonnegative_i64(object.get("cached_input_tokens")?)?,
        cache_write_input_tokens: object
            .get("cache_write_input_tokens")
            .map(parse_nonnegative_i64)
            .unwrap_or(Some(0))?,
        output_tokens: parse_nonnegative_i64(object.get("output_tokens")?)?,
        reasoning_output_tokens: parse_nonnegative_i64(object.get("reasoning_output_tokens")?)?,
    })
}

fn parse_nonnegative_i64(value: &Value) -> Option<u64> {
    u64::try_from(value.as_i64()?).ok()
}
