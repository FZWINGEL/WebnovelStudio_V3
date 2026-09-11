//! One contained Codex invocation with bounded, protocol-validated progress.
//!
//! The process observer never performs database work or waits on its consumer.
//! A slow consumer causes an explicit stop/failure instead of an unbounded queue.
use super::cli::windows_process::{
    self, ChildStream, ChildTermination, CliInvocation, ContainmentError, StopSignal,
};
use super::codex_exec::{CodexEvent, CodexFailureCode, CodexJsonlParser, CodexUsage};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const PROGRESS_CAPACITY: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexRunStatus {
    Completed,
    Stopped,
    TimedOut,
    OutputLimit,
    ProtocolFailure(CodexFailureCode),
    ConsumerTooSlow,
    ProcessUnavailable,
    CleanupUnresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexRunResult {
    pub status: CodexRunStatus,
    pub assistant_text: String,
    pub usage: Option<CodexUsage>,
    pub confirmed_stdin_bytes: usize,
    pub warning_count: usize,
    pub cleanup_settled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexStreamEvent {
    AssistantDelta(String),
    Finished(CodexRunResult),
}

pub struct CodexStream {
    stop: StopSignal,
    events: Receiver<CodexStreamEvent>,
    worker: Option<JoinHandle<()>>,
    terminal: bool,
}

impl CodexStream {
    /// Does not retry. The caller supplies an already validated executable,
    /// profile, isolated working directory, exact stdin packet, and limits.
    pub fn start(invocation: CliInvocation, stop: StopSignal) -> std::io::Result<Self> {
        Self::start_with_resources(invocation, stop, ())
    }

    pub(crate) fn start_with_resources(
        invocation: CliInvocation,
        stop: StopSignal,
        resources: impl Send + 'static,
    ) -> std::io::Result<Self> {
        let (sender, events) = mpsc::sync_channel(PROGRESS_CAPACITY);
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("webnovel-codex-process".into())
            .spawn(move || {
                let _resources = resources;
                let result = execute(invocation, worker_stop, &sender);
                // Process cleanup is already complete (or explicitly unresolved).
                // Dropping the consumer disconnects this bounded terminal send.
                let _ = sender.send(CodexStreamEvent::Finished(result));
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

    /// Timeout means the same invocation is still pending, never permission
    /// to restart it. Disconnection without a terminal event is an error.
    pub fn next_event(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<CodexStreamEvent>, &'static str> {
        if self.terminal {
            return Err("The provider stream is already finished.");
        }
        match self.events.recv_timeout(timeout) {
            Ok(event) => {
                if matches!(event, CodexStreamEvent::Finished(_)) {
                    self.terminal = true;
                    // A terminal event follows explicit process settlement.
                    // The worker has no remaining work other than returning.
                    if let Some(worker) = self.worker.take() {
                        let _ = worker.join();
                    }
                }
                Ok(Some(event))
            }
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => {
                self.terminal = true;
                Err("The provider worker ended without confirming cleanup.")
            }
        }
    }
}

impl Drop for CodexStream {
    fn drop(&mut self) {
        // Explicit Finished is the proof boundary. Drop requests cleanup but
        // does not block the UI or pretend that upstream billing has stopped.
        if !self.terminal {
            self.stop.request_stop();
        }
    }
}

fn execute(
    invocation: CliInvocation,
    stop: StopSignal,
    sender: &SyncSender<CodexStreamEvent>,
) -> CodexRunResult {
    let mut parser = CodexJsonlParser::new();
    if stop.is_requested() {
        return CodexRunResult {
            status: CodexRunStatus::Stopped,
            assistant_text: String::new(),
            usage: None,
            confirmed_stdin_bytes: 0,
            warning_count: 0,
            cleanup_settled: true,
        };
    }
    let mut failure = None;
    let mut consumer_slow = false;
    let process = match windows_process::spawn(invocation) {
        Ok(process) => process,
        Err(error) => return process_error(error, &parser, false),
    };
    let observer_stop = stop.clone();
    let result = process.finish_or_stop_with_output(stop, |stream, bytes| {
        // stderr is bounded by the process layer. Never display, persist, or
        // forward its raw diagnostics (which may contain account information).
        if stream != ChildStream::Stdout || failure.is_some() || consumer_slow {
            return;
        }
        match parser.feed(bytes) {
            Ok(events) => {
                for event in events {
                    if let CodexEvent::AssistantDelta(text) = event
                        && sender
                            .try_send(CodexStreamEvent::AssistantDelta(text))
                            .is_err()
                    {
                        consumer_slow = true;
                        observer_stop.request_stop();
                        break;
                    }
                }
            }
            Err(error) => {
                failure = Some(error.code);
                observer_stop.request_stop();
            }
        }
    });
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return process_error(error, &parser, true),
    };
    let mut status = if consumer_slow {
        CodexRunStatus::ConsumerTooSlow
    } else if let Some(code) = failure {
        CodexRunStatus::ProtocolFailure(code)
    } else {
        match outcome.termination {
            ChildTermination::Completed => CodexRunStatus::Completed,
            ChildTermination::Stopped => CodexRunStatus::Stopped,
            ChildTermination::TimedOut => CodexRunStatus::TimedOut,
            ChildTermination::OutputLimitExceeded => CodexRunStatus::OutputLimit,
        }
    };
    if status == CodexRunStatus::Completed {
        match parser.finish(
            outcome
                .output
                .exit_code
                .and_then(|code| i32::try_from(code).ok())
                .unwrap_or(-1),
        ) {
            Ok(_) => {}
            Err(error) => {
                status = CodexRunStatus::ProtocolFailure(error.code);
            }
        }
        if !outcome.output.io_errors.is_empty() || outcome.output.truncated {
            status = CodexRunStatus::ProtocolFailure(CodexFailureCode::Incomplete);
        }
    }
    CodexRunResult {
        status,
        assistant_text: parser.partial_text().into(),
        usage: parser.observed_usage(),
        confirmed_stdin_bytes: outcome.output.stdin_bytes_written,
        warning_count: parser.warning_count(),
        cleanup_settled: true,
    }
}

fn process_error(
    error: ContainmentError,
    parser: &CodexJsonlParser,
    started: bool,
) -> CodexRunResult {
    let (status, confirmed_stdin_bytes, cleanup_settled) = match error {
        ContainmentError::Cleanup { partial, .. } => (
            CodexRunStatus::CleanupUnresolved,
            partial.map_or(0, |output| output.stdin_bytes_written),
            false,
        ),
        // Spawn rejects invalid arguments and OS startup failures before stdin
        // delivery. Any post-spawn settlement failure is the Cleanup variant.
        _ if !started => (CodexRunStatus::ProcessUnavailable, 0, true),
        _ => (CodexRunStatus::CleanupUnresolved, 0, false),
    };
    CodexRunResult {
        status,
        assistant_text: parser.partial_text().into(),
        usage: parser.observed_usage(),
        confirmed_stdin_bytes,
        warning_count: parser.warning_count(),
        cleanup_settled,
    }
}
