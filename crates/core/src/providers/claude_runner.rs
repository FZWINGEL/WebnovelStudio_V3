//! One contained Claude Code CLI invocation with bounded protocol progress.
//!
//! The worker owns no project state and never retries.  It uses the existing
//! Windows process boundary and [`super::claude_exec::ClaudeJsonlParser`],
//! forwarding only validated assistant text and a terminal typed result.

use super::claude_exec::{ClaudeEvent, ClaudeFailureCode, ClaudeJsonlParser, ClaudeUsage};
use super::claude_profile::{CLAUDE_INPUT_LIMIT_BYTES, CLAUDE_OUTPUT_LIMIT_BYTES};
use super::cli::windows_process::{
    self, ChildStream, ChildTermination, CliInvocation, ContainmentError, StopSignal,
};
use std::ffi::OsString;
use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const PROGRESS_CAPACITY: usize = 32;
// Claude's retained assistant text has a smaller product cap, but the
// process boundary must allow JSONL framing, duplicate terminal text, and
// usage records around that text.  Keep the wire cap aligned with the
// existing process default rather than treating it as an answer-text cap.
const CLAUDE_PROCESS_OUTPUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeRunStatus {
    Completed,
    Stopped,
    TimedOut,
    OutputLimit,
    InputLimit,
    /// The provider completed, but its reported model differs from the
    /// exact model requested by the author. The result remains inspectable;
    /// this runner never retries or silently substitutes a model.
    ModelMismatch,
    ProtocolFailure(ClaudeFailureCode),
    ConsumerTooSlow,
    ProcessUnavailable,
    CleanupUnresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeRunResult {
    pub status: ClaudeRunStatus,
    pub assistant_text: String,
    pub requested_model: Option<String>,
    /// Model name claimed by the provider's terminal result. This is kept
    /// separate from the exact requested ID; it is not an independently
    /// verified model-resolution proof.
    pub reported_model: Option<String>,
    pub usage: Option<ClaudeUsage>,
    pub confirmed_stdin_bytes: usize,
    pub cleanup_settled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeStreamEvent {
    AssistantDelta(String),
    Finished(ClaudeRunResult),
}

pub struct ClaudeStream {
    stop: StopSignal,
    events: Receiver<ClaudeStreamEvent>,
    worker: Option<JoinHandle<()>>,
    terminal: bool,
}

impl ClaudeStream {
    /// Start exactly one bounded invocation.  The packet is written by the
    /// process boundary exactly as supplied; it is never reformatted or
    /// retried here.
    pub fn start(invocation: CliInvocation, stop: StopSignal) -> io::Result<Self> {
        Self::start_with_resources(invocation, stop, ())
    }

    pub(crate) fn start_with_resources(
        invocation: CliInvocation,
        stop: StopSignal,
        resources: impl Send + 'static,
    ) -> io::Result<Self> {
        let invocation = bounded_invocation(invocation)?;
        let (sender, events) = mpsc::sync_channel(PROGRESS_CAPACITY);
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("webnovel-claude-process".into())
            .spawn(move || {
                let _resources = resources;
                let result = execute(invocation, worker_stop, &sender);
                let _ = sender.send(ClaudeStreamEvent::Finished(result));
            })?;
        Ok(Self {
            stop,
            events,
            worker: Some(worker),
            terminal: false,
        })
    }

    pub fn request_stop(&self) {
        self.stop.request_stop();
    }

    pub fn next_event(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<ClaudeStreamEvent>, &'static str> {
        if self.terminal {
            return Err("The Claude provider stream is already finished.");
        }
        match self.events.recv_timeout(timeout) {
            Ok(event) => {
                if matches!(event, ClaudeStreamEvent::Finished(_)) {
                    self.terminal = true;
                    if let Some(worker) = self.worker.take() {
                        let _ = worker.join();
                    }
                }
                Ok(Some(event))
            }
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => {
                self.terminal = true;
                Err("The Claude provider worker ended without confirming cleanup.")
            }
        }
    }
}

impl Drop for ClaudeStream {
    fn drop(&mut self) {
        if !self.terminal {
            self.stop.request_stop();
        }
    }
}

fn bounded_invocation(mut invocation: CliInvocation) -> io::Result<CliInvocation> {
    if invocation.packet.len() > CLAUDE_INPUT_LIMIT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Claude stdin packet exceeds the application input cap",
        ));
    }
    if invocation.limits.max_total_output_bytes > CLAUDE_PROCESS_OUTPUT_LIMIT_BYTES {
        invocation.limits.max_total_output_bytes = CLAUDE_PROCESS_OUTPUT_LIMIT_BYTES;
    }
    Ok(invocation)
}

fn execute(
    invocation: CliInvocation,
    stop: StopSignal,
    sender: &SyncSender<ClaudeStreamEvent>,
) -> ClaudeRunResult {
    let requested_model = requested_model(&invocation.arguments);
    let mut parser = ClaudeJsonlParser::new();
    let mut observed = String::new();
    if stop.is_requested() {
        return result(
            ClaudeRunStatus::Stopped,
            observed,
            requested_model,
            None,
            0,
            true,
        );
    }
    let mut failure = None;
    let mut consumer_slow = false;
    let mut output_limited = false;
    let process = match windows_process::spawn(invocation) {
        Ok(process) => process,
        Err(error) => return process_error(error, requested_model, observed, false),
    };
    let observer_stop = stop.clone();
    let result = process.finish_or_stop_with_output(stop, |stream, bytes| {
        if stream != ChildStream::Stdout || failure.is_some() || consumer_slow {
            return;
        }
        match parser.feed(bytes) {
            Ok(events) => {
                for event in events {
                    let ClaudeEvent::AssistantDelta(text) = event;
                    let chunk = append_delta(&mut observed, &text, &mut output_limited);
                    if !chunk.is_empty()
                        && sender
                            .try_send(ClaudeStreamEvent::AssistantDelta(chunk))
                            .is_err()
                    {
                        consumer_slow = true;
                        observer_stop.request_stop();
                        break;
                    }
                    if output_limited {
                        observer_stop.request_stop();
                        break;
                    }
                }
            }
            Err(error) => {
                let chunk =
                    append_partial(&mut observed, &error.partial_output, &mut output_limited);
                if !chunk.is_empty()
                    && sender
                        .try_send(ClaudeStreamEvent::AssistantDelta(chunk))
                        .is_err()
                {
                    consumer_slow = true;
                }
                failure = Some(error.code);
                observer_stop.request_stop();
            }
        }
    });
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return process_error(error, requested_model, observed, true),
    };
    let mut status = if consumer_slow {
        ClaudeRunStatus::ConsumerTooSlow
    } else if output_limited {
        ClaudeRunStatus::OutputLimit
    } else if let Some(code) = failure {
        ClaudeRunStatus::ProtocolFailure(code)
    } else {
        match outcome.termination {
            ChildTermination::Completed => ClaudeRunStatus::Completed,
            ChildTermination::Stopped => ClaudeRunStatus::Stopped,
            ChildTermination::TimedOut => ClaudeRunStatus::TimedOut,
            ChildTermination::OutputLimitExceeded => ClaudeRunStatus::OutputLimit,
        }
    };
    let mut reported_model = None;
    let mut usage = None;
    if status == ClaudeRunStatus::Completed {
        match parser.finish(
            outcome
                .output
                .exit_code
                .and_then(|code| i32::try_from(code).ok())
                .unwrap_or(-1),
        ) {
            Ok(completed) => {
                reported_model = completed.model;
                usage = completed.usage;
                let (retained, exceeded) = retain_text(&completed.assistant_text);
                observed = retained;
                if exceeded {
                    status = ClaudeRunStatus::OutputLimit;
                } else if requested_model.is_some()
                    && reported_model.is_some()
                    && requested_model != reported_model
                {
                    status = ClaudeRunStatus::ModelMismatch;
                }
            }
            Err(error) => {
                let _ = append_partial(&mut observed, &error.partial_output, &mut output_limited);
                status = ClaudeRunStatus::ProtocolFailure(error.code);
            }
        }
        if !outcome.output.io_errors.is_empty() || outcome.output.truncated {
            status = ClaudeRunStatus::ProtocolFailure(ClaudeFailureCode::Incomplete);
        }
    }
    ClaudeRunResult {
        status,
        assistant_text: observed,
        requested_model,
        reported_model,
        usage,
        confirmed_stdin_bytes: outcome.output.stdin_bytes_written,
        cleanup_settled: true,
    }
}

fn process_error(
    error: ContainmentError,
    requested_model: Option<String>,
    observed: String,
    started: bool,
) -> ClaudeRunResult {
    let (status, confirmed_stdin_bytes, cleanup_settled) = match error {
        ContainmentError::Cleanup { partial, .. } => (
            ClaudeRunStatus::CleanupUnresolved,
            partial.map_or(0, |output| output.stdin_bytes_written),
            false,
        ),
        _ if !started => (ClaudeRunStatus::ProcessUnavailable, 0, true),
        _ => (ClaudeRunStatus::CleanupUnresolved, 0, false),
    };
    ClaudeRunResult {
        status,
        assistant_text: observed,
        requested_model,
        reported_model: None,
        usage: None,
        confirmed_stdin_bytes,
        cleanup_settled,
    }
}

fn requested_model(arguments: &[OsString]) -> Option<String> {
    arguments
        .windows(2)
        .find(|pair| pair[0] == "--model")
        .and_then(|pair| pair[1].to_str())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_owned)
}

fn result(
    status: ClaudeRunStatus,
    assistant_text: String,
    requested_model: Option<String>,
    usage: Option<ClaudeUsage>,
    confirmed_stdin_bytes: usize,
    cleanup_settled: bool,
) -> ClaudeRunResult {
    ClaudeRunResult {
        status,
        assistant_text,
        requested_model,
        reported_model: None,
        usage,
        confirmed_stdin_bytes,
        cleanup_settled,
    }
}

fn prefix(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn retain_text(text: &str) -> (String, bool) {
    let retained = prefix(text, CLAUDE_OUTPUT_LIMIT_BYTES);
    (retained.to_owned(), retained.len() < text.len())
}

fn append_delta(observed: &mut String, text: &str, output_limited: &mut bool) -> String {
    let remaining = CLAUDE_OUTPUT_LIMIT_BYTES.saturating_sub(observed.len());
    if text.len() > remaining {
        *output_limited = true;
    }
    let chunk = prefix(text, remaining);
    observed.push_str(chunk);
    chunk.to_owned()
}

fn append_partial(observed: &mut String, partial: &str, output_limited: &mut bool) -> String {
    let Some(suffix) = partial.strip_prefix(observed.as_str()) else {
        return String::new();
    };
    append_delta(observed, suffix, output_limited)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::cli::windows_process::{ChildLimits, EnvironmentPolicy};
    use std::path::PathBuf;

    fn invocation(packet: Vec<u8>) -> CliInvocation {
        CliInvocation {
            executable: PathBuf::from(r"C:\owned\claude.exe"),
            arguments: vec!["-p".into(), "--model".into(), "claude-sonnet-5".into()],
            cwd: PathBuf::from(r"C:\owned\empty"),
            environment: EnvironmentPolicy::Clear,
            packet,
            limits: ChildLimits::default(),
        }
    }

    #[test]
    fn rejects_packets_over_the_claude_input_cap_before_thread_creation() {
        let result = ClaudeStream::start(
            invocation(vec![b'x'; CLAUDE_INPUT_LIMIT_BYTES + 1]),
            StopSignal::new(),
        );
        assert_eq!(
            result.err().expect("input cap rejection").kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn clamps_process_output_limit_and_keeps_packet_bytes_exact() {
        let packet = b"{\"synthetic\":true}".to_vec();
        let mut input = invocation(packet.clone());
        input.limits.max_total_output_bytes = usize::MAX;
        let bounded = bounded_invocation(input).expect("bounded invocation");
        assert_eq!(bounded.packet, packet);
        assert_eq!(
            bounded.limits.max_total_output_bytes,
            CLAUDE_PROCESS_OUTPUT_LIMIT_BYTES
        );
    }

    #[test]
    fn keeps_valid_text_seen_before_same_chunk_malformed_json() {
        let mut parser = ClaudeJsonlParser::new();
        let bytes = br#"{"type":"system","subtype":"init","session_id":"s","tools":[],"mcp_servers":[]}
{"type":"assistant","session_id":"s","message":{"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"partial"}]}}
not-json
"#;
        let failure = parser.feed(bytes).expect_err("malformed trailing record");
        assert_eq!(failure.code, ClaudeFailureCode::InvalidJson);
        assert_eq!(failure.partial_output, "partial");

        let mut observed = String::new();
        let mut output_limited = false;
        let chunk = append_partial(&mut observed, &failure.partial_output, &mut output_limited);
        assert_eq!(chunk, "partial");
        assert_eq!(observed, "partial");
        assert!(!output_limited);
    }

    #[test]
    fn caps_unterminated_final_result_text_without_splitting_utf8() {
        let mut parser = ClaudeJsonlParser::new();
        parser
            .feed(
                br#"{"type":"system","subtype":"init","session_id":"s","tools":[],"mcp_servers":[]}
"#,
            )
            .expect("init");
        let text = "🙂".repeat(CLAUDE_OUTPUT_LIMIT_BYTES / "🙂".len() + 2);
        let assistant = format!(
            "{{\"type\":\"assistant\",\"session_id\":\"s\",\"message\":{{\"type\":\"message\",\"role\":\"assistant\",\"stop_reason\":\"end_turn\",\"content\":[{{\"type\":\"text\",\"text\":{}}}]}}}}\n",
            serde_json::to_string(&text).expect("text JSON")
        );
        let result = format!(
            "{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"session_id\":\"s\",\"result\":{}}}",
            serde_json::to_string(&text).expect("result JSON")
        );
        parser.feed(assistant.as_bytes()).expect("assistant");
        parser.feed(result.as_bytes()).expect("buffer final result");
        let completed = parser.finish(0).expect("completed result");
        let (retained, exceeded) = retain_text(&completed.assistant_text);
        assert!(exceeded);
        assert!(retained.len() <= CLAUDE_OUTPUT_LIMIT_BYTES);
        assert!(retained.ends_with('🙂') || retained.is_empty());
    }

    #[test]
    fn extracts_requested_model_and_preserves_unicode_prefix_boundaries() {
        assert_eq!(
            requested_model(&[
                "--model".into(),
                "claude-opus-5".into(),
                "--effort".into(),
                "high".into()
            ])
            .as_deref(),
            Some("claude-opus-5")
        );
        assert_eq!(prefix("海🙂llo", 4), "海");
        assert_eq!(prefix("海🙂llo", 7), "海🙂");
    }

    #[test]
    fn process_failure_keeps_partial_text_without_exposing_diagnostics() {
        let result = process_error(
            ContainmentError::Cleanup {
                stage: "fixture",
                partial: Some(crate::providers::cli::windows_process::ChildOutput {
                    exit_code: None,
                    stdin_bytes_written: 17,
                    stdout: b"ignored raw stdout".to_vec(),
                    stderr: b"SECRET_DIAGNOSTIC".to_vec(),
                    truncated: false,
                    io_errors: Vec::new(),
                }),
            },
            Some("claude-sonnet-5".into()),
            "partial answer".into(),
            true,
        );
        assert_eq!(result.status, ClaudeRunStatus::CleanupUnresolved);
        assert_eq!(result.assistant_text, "partial answer");
        assert_eq!(result.confirmed_stdin_bytes, 17);
        assert!(!result.cleanup_settled);
        assert!(!format!("{result:?}").contains("SECRET_DIAGNOSTIC"));
    }
}
