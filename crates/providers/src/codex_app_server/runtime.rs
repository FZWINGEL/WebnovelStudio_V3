//! Persistent, application-owned Codex app-server transport.
//!
//! One bounded IO worker owns one contained native process.  Requests are
//! isolated by fresh ephemeral app-server threads and are routed by the JSON-
//! RPC id plus thread/turn identity.  This module intentionally does not own
//! story persistence: callers provide the pre-turn and turn-id acknowledgement
//! hooks, and the hooks run on short-lived relay workers rather than inside the
//! process observer.

#![cfg(windows)]

use super::protocol::{
    self, JsonlDecoder, ProtocolError, RpcId, RpcMessage, ThreadStartAck, ThreadStartConfig,
    TurnAssembler, TurnCompleted, TurnEvent, TurnStartAck, TurnStatus,
};
use super::{
    AppServerConnectionSettlement, AppServerDelivery, AppServerDispatch, AppServerSubmission,
    AppServerTerminal, prepare_dispatch, turn_request,
};
use crate::cli::windows_process::{
    self, ChildStream, CliInvocation, ContainmentError, InteractiveAction, PersistentEvent,
    RunningChild, StopSignal,
};
use crate::codex_exec::{CodexFailureCode, CodexUsage};
use crate::codex_runner::{CodexRunResult, CodexRunStatus};
use crate::vocabulary::ProviderBinding;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;
use wns_kernel::{CoreError, CoreResult};

const MAX_ACTIVE_REQUESTS: usize = 8;
const EVENT_CAPACITY: usize = 64;
const COMMAND_CAPACITY: usize = MAX_ACTIVE_REQUESTS * 4;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const RPC_SETTLEMENT_TIMEOUT: Duration = Duration::from_secs(10);
const INTERRUPT_SETTLEMENT_TIMEOUT: Duration = Duration::from_secs(5);
const TURN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Ephemeral threads are unsubscribed after every turn, but an app-server may
/// retain implementation state beyond that acknowledgment.  Recycle an idle
/// process after a bounded number of completed threads instead of assuming
/// unbounded reuse is harmless.
const MAX_THREADS_PER_CONNECTION: usize = 128;
const CLIENT_NAME: &str = "webnovelstudio-v3";
const CLIENT_TITLE: &str = "WebnovelStudio V3";
const CLIENT_VERSION: &str = "3.0.0";

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

type BeforeTurn = Box<dyn Fn(&AppServerDispatch) -> CoreResult<()> + Send + 'static>;
type OnTurn = Box<dyn Fn(&AppServerDispatch, &str) -> CoreResult<()> + Send + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppServerHealth {
    Starting,
    Ready,
    Poisoned,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppServerFinished {
    pub result: CodexRunResult,
    pub delivery: AppServerDelivery,
    pub failure: Option<protocol::TurnFailure>,
    /// A bounded application-side diagnosis. This is separate from the
    /// provider's `turn.error.codexErrorInfo` and never contains upstream
    /// messages, frames, paths, or credentials.
    pub local_failure: Option<AppServerLocalFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppServerLocalFailure {
    Protocol(protocol::ProtocolFailureCode),
    ProcessExited,
    ProcessCleanup,
    RpcTimeout,
    Persistence,
    ConsumerDisconnected,
    ConsumerTooSlow,
    Unavailable,
}

impl AppServerLocalFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Protocol(code) => code.as_str(),
            Self::ProcessExited => "process_exited",
            Self::ProcessCleanup => "process_cleanup",
            Self::RpcTimeout => "rpc_timeout",
            Self::Persistence => "persistence",
            Self::ConsumerDisconnected => "consumer_disconnected",
            Self::ConsumerTooSlow => "consumer_too_slow",
            Self::Unavailable => "unavailable",
        }
    }

    pub const fn safe_detail(self) -> &'static str {
        match self {
            Self::Protocol(_) => {
                "The Codex app-server returned an unsupported or invalid protocol record."
            }
            Self::ProcessExited => {
                "The Codex app-server process exited before the request settled."
            }
            Self::ProcessCleanup => "The Codex app-server process cleanup did not settle.",
            Self::RpcTimeout => "The Codex app-server RPC did not settle before its deadline.",
            Self::Persistence => {
                "The application could not durably acknowledge the app-server request."
            }
            Self::ConsumerDisconnected => "The app-server response consumer disconnected.",
            Self::ConsumerTooSlow => {
                "The app-server response consumer could not process progress quickly enough."
            }
            Self::Unavailable => "The Codex app-server became unavailable.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppServerStreamEvent {
    AssistantDelta(String),
    Finished(Box<AppServerFinished>),
}

pub struct AppServerConnection {
    tx: SyncSender<Command>,
    state: Arc<Mutex<ConnectionState>>,
    stop: StopSignal,
    worker: Option<JoinHandle<()>>,
}

/// Opaque authentication payload supplied by an application-owned adapter.
/// Deliberately has no `Debug`, `Clone`, or `Serialize` implementation so
/// credentials stay on the private JSON-RPC write path.
pub struct AppServerAuth {
    pub method: String,
    pub params: Value,
}

#[derive(Debug)]
struct ConnectionState {
    health: AppServerHealth,
    active: usize,
    /// Set only after the owned persistent child has completed its local
    /// containment cleanup.  A joined worker with a failed cleanup is still
    /// unsafe to reuse or report as cleanly closed.
    cleanup_settled: bool,
}

#[derive(Clone)]
struct ConnectionHandle {
    tx: SyncSender<Command>,
    state: Arc<Mutex<ConnectionState>>,
}

pub struct AppServerReservation {
    handle: ConnectionHandle,
    request_id: String,
    started: bool,
    slot_released: bool,
}

pub struct AppServerStream {
    request_id: String,
    tx: SyncSender<Command>,
    events: Receiver<AppServerStreamEvent>,
    terminal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestPhase {
    ThreadStart,
    BeforeTurn,
    TurnStart,
    Active,
    Interrupt,
    Unsubscribe,
}

struct BeginCommand {
    request_id: String,
    binding: ProviderBinding,
    packet: String,
    thread: ThreadStartConfig,
    stop: StopSignal,
    before_turn: Option<BeforeTurn>,
    on_turn: Option<OnTurn>,
    events: SyncSender<AppServerStreamEvent>,
}

enum Command {
    Begin(Box<BeginCommand>),
    SubmitTurn {
        request_id: String,
    },
    BeforeTurnFailed {
        request_id: String,
        error: CoreError,
    },
    TurnPersisted {
        request_id: String,
        error: Option<String>,
    },
    RequestStop {
        request_id: String,
    },
    Shutdown,
}

struct RequestState {
    binding: ProviderBinding,
    packet: String,
    thread: ThreadStartConfig,
    stop: StopSignal,
    before_turn: Option<BeforeTurn>,
    on_turn: Option<OnTurn>,
    events: SyncSender<AppServerStreamEvent>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    dispatch: Option<AppServerDispatch>,
    assembler: Option<TurnAssembler>,
    submission: AppServerSubmission,
    phase: RequestPhase,
    phase_deadline: Option<Instant>,
    interrupt_sent: bool,
    completed: Option<TurnCompleted>,
    turn_persistence_pending: bool,
    unsubscribe_settled: bool,
    terminal_sent: bool,
}

enum PendingRpc {
    Initialize,
    Authenticate,
    ThreadStart(String),
    TurnStart(String),
    TurnInterrupt(String),
    ThreadUnsubscribe(String),
}

struct Driver {
    child: Option<RunningChild>,
    stop: StopSignal,
    commands: Receiver<Command>,
    commands_tx: SyncSender<Command>,
    state: Arc<Mutex<ConnectionState>>,
    generation: String,
    decoder: JsonlDecoder,
    pending: HashMap<RpcId, PendingRpc>,
    retired_turn_rpcs: HashSet<RpcId>,
    requests: HashMap<String, RequestState>,
    ready: Option<SyncSender<CoreResult<()>>>,
    ready_sent: bool,
    shutdown_requested: bool,
    poisoned: bool,
    local_failure: Option<AppServerLocalFailure>,
    completed_threads: usize,
    max_threads_per_connection: usize,
    recycle_requested: bool,
    next_rpc: u64,
    resources: Box<dyn Send>,
    auth: Option<AppServerAuth>,
}

impl AppServerConnection {
    /// Starts the contained process and waits for a successful initialize /
    /// initialized handshake. `invocation.packet` is ignored intentionally:
    /// initialization is owned by this driver so callers cannot accidentally
    /// dispatch a generation before the connection is ready.
    pub fn start(invocation: CliInvocation, resources: impl Send + 'static) -> CoreResult<Self> {
        Self::start_with_auth(invocation, resources, None)
    }

    /// Starts with an optional opaque account/login payload. The payload is
    /// sent only after initialize/initialized and is never retained after the
    /// handshake response.
    pub fn start_with_auth(
        invocation: CliInvocation,
        resources: impl Send + 'static,
        auth: Option<AppServerAuth>,
    ) -> CoreResult<Self> {
        Self::start_with_config(invocation, resources, auth, MAX_THREADS_PER_CONNECTION)
    }

    /// Starts with a custom thread-recycle threshold (useful for testing process
    /// recycling without executing hundreds of sequential IPC turns).
    pub fn start_with_thread_threshold(
        invocation: CliInvocation,
        resources: impl Send + 'static,
        max_threads_per_connection: usize,
    ) -> CoreResult<Self> {
        Self::start_with_config(invocation, resources, None, max_threads_per_connection)
    }

    pub fn start_with_config(
        invocation: CliInvocation,
        resources: impl Send + 'static,
        auth: Option<AppServerAuth>,
        max_threads_per_connection: usize,
    ) -> CoreResult<Self> {
        if auth.as_ref().is_some_and(|auth| {
            !protocol::valid_identifier(&auth.method) || !auth.params.is_object()
        }) {
            return Err(protocol_error(ProtocolError::InvalidEnvelope));
        }
        let generation = Uuid::new_v4().to_string();
        let mut init_params = json!({
            "clientInfo": {
                "name": CLIENT_NAME,
                "title": CLIENT_TITLE,
                "version": CLIENT_VERSION,
            }
        });
        // ChatGPT account authentication is exposed through the app-server's
        // experimental API. Keep the capability opt-in and scoped to the
        // authenticated launch so an ordinary generation connection does not
        // advertise unsupported experimental behavior.
        if auth.is_some() {
            init_params["capabilities"] = json!({"experimentalApi": true});
        }
        let init = protocol::json_line("initialize-1", "initialize", init_params)
            .map_err(protocol_error)?;
        let mut invocation = invocation;
        invocation.packet = init;
        let child = windows_process::spawn_persistent(invocation).map_err(containment_error)?;
        let stop = StopSignal::new();
        let (tx, commands) = mpsc::sync_channel(COMMAND_CAPACITY);
        let worker_tx = tx.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let state = Arc::new(Mutex::new(ConnectionState {
            health: AppServerHealth::Starting,
            active: 0,
            cleanup_settled: false,
        }));
        let worker_state = Arc::clone(&state);
        let worker_stop = stop.clone();
        let worker_generation = generation.clone();
        let worker_auth = auth;
        let worker = thread::Builder::new()
            .name("webnovel-codex-app-server".into())
            .spawn(move || {
                let mut driver = Driver {
                    child: Some(child),
                    stop: worker_stop.clone(),
                    commands,
                    commands_tx: worker_tx,
                    state: worker_state,
                    generation: worker_generation,
                    decoder: JsonlDecoder::new(),
                    pending: HashMap::from([(
                        RpcId::string("initialize-1"),
                        PendingRpc::Initialize,
                    )]),
                    retired_turn_rpcs: HashSet::new(),
                    requests: HashMap::new(),
                    ready: Some(ready_tx),
                    ready_sent: false,
                    shutdown_requested: false,
                    poisoned: false,
                    local_failure: None,
                    completed_threads: 0,
                    max_threads_per_connection,
                    recycle_requested: false,
                    next_rpc: 1,
                    resources: Box::new(resources),
                    auth: worker_auth,
                };
                driver.run();
            })
            .map_err(|error| CoreError::new("WorkerUnavailable", &error.to_string()))?;
        match ready_rx.recv_timeout(STARTUP_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                tx,
                state,
                stop,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                stop.request_stop();
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                stop.request_stop();
                let _ = worker.join();
                Err(CoreError::new(
                    "CodexAppServerUnavailable",
                    "The Codex app-server did not become ready.",
                ))
            }
        }
    }

    pub fn try_reserve(&self) -> CoreResult<AppServerReservation> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| unavailable("connection state poisoned"))?;
        if state.health != AppServerHealth::Ready {
            return Err(unavailable("The Codex app-server is not ready."));
        }
        if state.active >= MAX_ACTIVE_REQUESTS {
            return Err(CoreError::new(
                "ProviderBusy",
                "The persistent Codex connection has reached its concurrent request limit.",
            ));
        }
        state.active += 1;
        let request_id = format!(
            "request-{}",
            NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed)
        );
        Ok(AppServerReservation {
            handle: ConnectionHandle {
                tx: self.tx.clone(),
                state: Arc::clone(&self.state),
            },
            request_id,
            started: false,
            slot_released: false,
        })
    }

    pub fn health(&self) -> AppServerHealth {
        self.state
            .lock()
            .map(|state| state.health)
            .unwrap_or(AppServerHealth::Poisoned)
    }

    pub fn active_count(&self) -> usize {
        // A poisoned mutex must never look idle: callers could otherwise
        // admit a paid request while the connection's accounting is unknown.
        self.state
            .lock()
            .map(|state| state.active)
            .unwrap_or(MAX_ACTIVE_REQUESTS)
    }

    pub fn shutdown(mut self) -> CoreResult<()> {
        // Do not rely on a blocking queue send.  A full queue is expected
        // during shutdown and the shared stop signal is sufficient to make
        // the process observer enter bounded cleanup.  A disconnected queue
        // means the worker may already have exited; still join it and inspect
        // its published cleanup state before returning.
        let _ = self.tx.try_send(Command::Shutdown);
        self.stop.request_stop();
        let join_result = self
            .worker
            .take()
            .map(|worker| worker.join())
            .transpose()
            .map_err(|_| unavailable("The Codex app-server worker did not settle."));
        join_result?;
        require_cleanup_settled(&self.state)
    }
}

impl Drop for AppServerConnection {
    fn drop(&mut self) {
        // Drop is best-effort and nonblocking: the worker owns the Job Object
        // and shared stop signal, so the process tree is still contained and
        // settles even when the last desktop owner disappears unexpectedly.
        let _ = self.tx.try_send(Command::Shutdown);
        self.stop.request_stop();
    }
}

impl AppServerReservation {
    pub fn start(
        mut self,
        binding: ProviderBinding,
        packet: String,
        thread: ThreadStartConfig,
        stop: StopSignal,
        before_turn: impl Fn(&AppServerDispatch) -> CoreResult<()> + Send + 'static,
        on_turn: impl Fn(&AppServerDispatch, &str) -> CoreResult<()> + Send + 'static,
    ) -> CoreResult<AppServerStream> {
        if packet.is_empty() || packet.len() > 24 * 1024 {
            return Err(CoreError::new(
                "ProviderInputTooLarge",
                "The Codex app-server packet exceeds the application input allowance.",
            ));
        }
        binding
            .validate()
            .map_err(|detail| CoreError::new("ProviderProfileInvalid", &detail))?;
        thread.validate().map_err(protocol_error)?;
        if thread.model != binding.model_id
            || thread.reasoning_effort != binding.reasoning
            || thread.service_tier != binding.service_tier.as_deref().unwrap_or("default")
        {
            return Err(CoreError::new(
                "ProviderProfileInvalid",
                "The app-server thread settings do not match the selected model binding.",
            ));
        }
        let (events_tx, events_rx) = mpsc::sync_channel(EVENT_CAPACITY);
        let request_id = self.request_id.clone();
        let command = Command::Begin(Box::new(BeginCommand {
            request_id: request_id.clone(),
            binding,
            packet,
            thread,
            stop: stop.clone(),
            before_turn: Some(Box::new(before_turn)),
            on_turn: Some(Box::new(on_turn)),
            events: events_tx,
        }));
        if let Err(error) = self.handle.tx.try_send(command) {
            self.release_slot();
            return Err(match error {
                TrySendError::Full(_) => CoreError::new(
                    "ProviderBusy",
                    "The Codex app-server command queue is full.",
                ),
                TrySendError::Disconnected(_) => {
                    unavailable("The Codex app-server worker stopped.")
                }
            });
        }
        self.started = true;
        Ok(AppServerStream {
            request_id,
            tx: self.handle.tx.clone(),
            events: events_rx,
            terminal: false,
        })
    }

    fn release_slot(&mut self) {
        if self.slot_released {
            return;
        }
        if let Ok(mut state) = self.handle.state.lock() {
            state.active = state.active.saturating_sub(1);
        }
        self.slot_released = true;
    }
}

impl Drop for AppServerReservation {
    fn drop(&mut self) {
        if !self.started && !self.slot_released {
            self.release_slot();
        }
    }
}

impl AppServerStream {
    pub fn request_stop(&self) {
        // The IO worker must first send `turn/interrupt` and wait for the
        // matching `turn/completed`. Requesting the process-wide stop signal
        // here would terminate the child before that protocol settlement and
        // turn every normal Stop into an unresolved cleanup.
        let _ = self.tx.try_send(Command::RequestStop {
            request_id: self.request_id.clone(),
        });
    }

    pub fn recv_timeout(&mut self, timeout: Duration) -> CoreResult<Option<AppServerStreamEvent>> {
        if self.terminal {
            return Err(unavailable("The app-server request is already finished."));
        }
        match self.events.recv_timeout(timeout) {
            Ok(event @ AppServerStreamEvent::Finished(_)) => {
                self.terminal = true;
                Ok(Some(event))
            }
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(unavailable(
                "The app-server request ended without a terminal result.",
            )),
        }
    }

    /// Alias used by callers that use the one-shot runner's naming.
    pub fn next_event(&mut self, timeout: Duration) -> CoreResult<Option<AppServerStreamEvent>> {
        self.recv_timeout(timeout)
    }
}

impl Drop for AppServerStream {
    fn drop(&mut self) {
        if !self.terminal {
            self.request_stop();
        }
    }
}

impl Driver {
    fn run(&mut self) {
        let stop = self.stop.clone();
        // Keep the opaque resources alive for the lifetime of this worker.
        let _ = &self.resources;
        let child = self.child.take().expect("driver child exists");
        let outcome = child.finish_persistent(stop, |event| self.observe(event));
        self.finish_process(outcome);
    }

    fn observe(&mut self, event: PersistentEvent<'_>) -> InteractiveAction {
        let mut outgoing = Vec::new();
        if let Err(error) = self.drain_commands(&mut outgoing) {
            self.poison(error, &mut outgoing);
        }
        match event {
            PersistentEvent::Output(stream, bytes)
                if stream == ChildStream::Stdout && !self.poisoned =>
            {
                match self.decoder.push(bytes) {
                    Ok(messages) => {
                        for message in messages {
                            if let Err(error) = self.accept_message(message, &mut outgoing) {
                                self.poison(error, &mut outgoing);
                                break;
                            }
                        }
                    }
                    Err(error) => self.poison(protocol_error(error), &mut outgoing),
                }
            }
            _ => {}
        }
        if self.shutdown_requested || self.poisoned {
            self.stop.request_stop();
        }
        if outgoing.is_empty() {
            InteractiveAction::KeepOpen
        } else {
            InteractiveAction::Send(outgoing.into_iter().flatten().collect())
        }
    }

    fn drain_commands(&mut self, outgoing: &mut Vec<Vec<u8>>) -> CoreResult<()> {
        loop {
            match self.commands.try_recv() {
                Ok(command) => self.accept_command(command, outgoing)?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.shutdown_requested = true;
                    break;
                }
            }
        }
        self.check_timeouts(outgoing)
    }

    fn accept_command(&mut self, command: Command, outgoing: &mut Vec<Vec<u8>>) -> CoreResult<()> {
        match command {
            Command::Begin(begin) => {
                let BeginCommand {
                    request_id,
                    binding,
                    packet,
                    thread,
                    stop,
                    before_turn,
                    on_turn,
                    events,
                } = *begin;
                if self.poisoned || self.shutdown_requested {
                    self.finish_not_sent(
                        RequestState {
                            binding,
                            packet,
                            thread,
                            stop,
                            before_turn,
                            on_turn,
                            events,
                            thread_id: None,
                            turn_id: None,
                            dispatch: None,
                            assembler: None,
                            submission: AppServerSubmission::NotSent,
                            phase: RequestPhase::ThreadStart,
                            phase_deadline: None,
                            interrupt_sent: false,
                            completed: None,
                            turn_persistence_pending: false,
                            unsubscribe_settled: false,
                            terminal_sent: false,
                        },
                        "The Codex app-server is unavailable.",
                    );
                    return Ok(());
                }
                let rpc = self.next_rpc("thread");
                let state = RequestState {
                    binding,
                    packet,
                    thread,
                    stop,
                    before_turn,
                    on_turn,
                    events,
                    thread_id: None,
                    turn_id: None,
                    dispatch: None,
                    assembler: None,
                    submission: AppServerSubmission::NotSent,
                    phase: RequestPhase::ThreadStart,
                    phase_deadline: Some(Instant::now() + RPC_SETTLEMENT_TIMEOUT),
                    interrupt_sent: false,
                    completed: None,
                    turn_persistence_pending: false,
                    unsubscribe_settled: false,
                    terminal_sent: false,
                };
                let thread = state.thread.clone();
                self.pending.insert(
                    RpcId::string(rpc.clone()),
                    PendingRpc::ThreadStart(request_id.clone()),
                );
                self.requests.insert(request_id, state);
                outgoing.push(
                    protocol::json_line(rpc, "thread/start", thread.params())
                        .map_err(protocol_error)?,
                );
            }
            Command::SubmitTurn { request_id } => self.submit_turn(&request_id, outgoing)?,
            Command::BeforeTurnFailed { request_id, error } => {
                if let Some(request) = self.requests.remove(&request_id) {
                    if error.code == "UncertainOutcome" {
                        self.finish_uncertain(request);
                        self.poison(error, outgoing);
                    } else if error.code == "RequestStopped" {
                        // Stop was observed before the durable pre-turn hook
                        // ran, so no claim exists for this prepared dispatch.
                        self.finish_not_sent(request, "The request was stopped before turn/start.");
                    } else {
                        self.finish_rejected(request);
                    }
                }
            }
            Command::TurnPersisted { request_id, error } => {
                if error.is_some() {
                    // The durable turn acknowledgement is part of the
                    // external-submit fence. If it fails, preserve an
                    // unresolved receipt and poison the shared process. Leave
                    // the request owned by the process observer: it may
                    // already contain an authoritative terminal and answer,
                    // which finish_process can retain while settling the
                    // shared child. The callback's raw detail never enters
                    // the diagnostic.
                    self.poison(
                        CoreError::new(
                            "UncertainOutcome",
                            "The app-server turn acknowledgement could not be persisted.",
                        ),
                        outgoing,
                    );
                    let _ = request_id;
                } else {
                    let should_finish = self.requests.get_mut(&request_id).is_some_and(|request| {
                        request.turn_persistence_pending = false;
                        request.completed.is_some() && request.unsubscribe_settled
                    });
                    if should_finish {
                        self.finish_request(&request_id, outgoing);
                    }
                }
            }
            Command::RequestStop { request_id } => self.request_stop(&request_id, outgoing)?,
            Command::Shutdown => {
                self.shutdown_requested = true;
                for request in self.requests.values() {
                    request.stop.request_stop();
                }
            }
        }
        Ok(())
    }

    fn submit_turn(&mut self, request_id: &str, outgoing: &mut Vec<Vec<u8>>) -> CoreResult<()> {
        let stopped = self
            .requests
            .get(request_id)
            .is_some_and(|request| request.stop.is_requested());
        if stopped {
            let request = self.requests.remove(request_id).expect("request exists");
            self.finish_not_submitted(request);
            return Ok(());
        }
        let (dispatch, binding, packet) = {
            let request = self
                .requests
                .get(request_id)
                .ok_or_else(|| unavailable("The app-server request disappeared."))?;
            (
                request
                    .dispatch
                    .clone()
                    .ok_or_else(|| unavailable("The app-server dispatch was not prepared."))?,
                request.binding.clone(),
                request.packet.clone(),
            )
        };
        let rpc = dispatch.rpc_id.clone();
        let mut frame = serde_json::to_vec(&turn_request(&dispatch, &binding, &packet))
            .map_err(CoreError::from)?;
        frame.push(b'\n');
        self.pending.insert(
            RpcId::string(rpc.clone()),
            PendingRpc::TurnStart(request_id.to_owned()),
        );
        outgoing.push(frame);
        if let Some(request) = self.requests.get_mut(request_id) {
            // Once this bounded frame is committed to the worker batch, a
            // missing response is an uncertain submission. It must not be
            // reported as NotSent merely because no turn ID was acknowledged.
            request.submission = AppServerSubmission::Uncertain;
            let now = Instant::now();
            request.phase = RequestPhase::TurnStart;
            request.phase_deadline = Some(now + RPC_SETTLEMENT_TIMEOUT);
        }
        Ok(())
    }

    fn request_stop(&mut self, request_id: &str, outgoing: &mut Vec<Vec<u8>>) -> CoreResult<()> {
        let Some(request) = self.requests.get(request_id) else {
            return Ok(());
        };
        request.stop.request_stop();
        let interrupt = if !request.interrupt_sent && !request.terminal_sent {
            request.thread_id.clone().zip(request.turn_id.clone())
        } else {
            None
        };
        if let Some((thread_id, turn_id)) = interrupt {
            let rpc = self.next_rpc("interrupt");
            if let Some(request) = self.requests.get_mut(request_id) {
                request.interrupt_sent = true;
                request.phase = RequestPhase::Interrupt;
                request.phase_deadline = Some(Instant::now() + INTERRUPT_SETTLEMENT_TIMEOUT);
            }
            self.pending.insert(
                RpcId::string(rpc.clone()),
                PendingRpc::TurnInterrupt(request_id.to_owned()),
            );
            outgoing.push(
                protocol::json_line(
                    rpc,
                    "turn/interrupt",
                    json!({"threadId":thread_id,"turnId":turn_id}),
                )
                .map_err(protocol_error)?,
            );
        } else if request.turn_id.is_none() && request.submission == AppServerSubmission::Uncertain
        {
            // There is no owned turn to interrupt (for example, a lost
            // turn/start acknowledgment). The process itself must settle so
            // the request can be reported as uncertain rather than hanging.
            self.stop.request_stop();
        }
        Ok(())
    }

    fn accept_message(
        &mut self,
        message: RpcMessage,
        outgoing: &mut Vec<Vec<u8>>,
    ) -> CoreResult<()> {
        match message {
            RpcMessage::ServerRequest { .. } => {
                Err(protocol_error(ProtocolError::UnexpectedRequest))
            }
            RpcMessage::Response { id, result, error } => {
                self.accept_response(id, result, error, outgoing)
            }
            RpcMessage::Notification { method, params } => {
                self.accept_notification(method, params, outgoing)
            }
        }
    }

    fn accept_response(
        &mut self,
        id: RpcId,
        result: Option<Value>,
        error: Option<Value>,
        outgoing: &mut Vec<Vec<u8>>,
    ) -> CoreResult<()> {
        let pending = match self.pending.remove(&id) {
            Some(pending) => pending,
            None if self.retired_turn_rpcs.remove(&id) => {
                // A turn notification can authoritatively acknowledge a
                // submitted turn before its response frame arrives. A late
                // response for that already-settled RPC is harmless and must
                // not poison an otherwise healthy connection.
                return Ok(());
            }
            None => return Err(protocol_error(ProtocolError::MismatchedResponse)),
        };
        if error.is_some() {
            match pending {
                PendingRpc::Initialize => {
                    return Err(unavailable(
                        "The Codex app-server initialize request failed.",
                    ));
                }
                PendingRpc::Authenticate => {
                    return Err(unavailable(
                        "The Codex app-server authentication request failed.",
                    ));
                }
                PendingRpc::ThreadStart(request_id) => {
                    if let Some(request) = self.requests.remove(&request_id) {
                        self.finish_rejected(request);
                    }
                }
                PendingRpc::TurnStart(request_id) => {
                    if let Some(request) = self.requests.remove(&request_id) {
                        self.finish_not_submitted(request);
                    }
                }
                PendingRpc::TurnInterrupt(_request_id) => {
                    return Err(unavailable(
                        "The Codex app-server did not acknowledge interruption.",
                    ));
                }
                PendingRpc::ThreadUnsubscribe(request_id) => {
                    self.finish_unsubscribe(&request_id, outgoing)
                }
            }
            return Ok(());
        }
        let result = result
            .ok_or(ProtocolError::InvalidEnvelope)
            .map_err(protocol_error)?;
        match pending {
            PendingRpc::Initialize => {
                if !result.is_object() {
                    return Err(protocol_error(ProtocolError::InvalidEnvelope));
                }
                outgoing.push(
                    protocol::notification("initialized", json!({})).map_err(protocol_error)?,
                );
                if let Some(auth) = self.auth.take() {
                    let rpc = self.next_rpc("auth");
                    self.pending
                        .insert(RpcId::string(rpc.clone()), PendingRpc::Authenticate);
                    outgoing.push(
                        protocol::json_line(rpc, &auth.method, auth.params)
                            .map_err(protocol_error)?,
                    );
                } else {
                    self.publish_ready()?;
                }
            }
            PendingRpc::Authenticate => {
                if !result.is_object() {
                    return Err(protocol_error(ProtocolError::InvalidEnvelope));
                }
                self.publish_ready()?;
            }
            PendingRpc::ThreadStart(request_id) => {
                let requested = self
                    .requests
                    .get(&request_id)
                    .ok_or_else(|| protocol_error(ProtocolError::MismatchedResponse))?
                    .thread
                    .clone();
                let ack =
                    ThreadStartAck::from_result(&result, &requested).map_err(protocol_error)?;
                let dispatch_rpc = self.next_rpc("turn");
                let generation = self.generation.clone();
                let (dispatch, before, stop) = {
                    let request = self
                        .requests
                        .get_mut(&request_id)
                        .ok_or_else(|| protocol_error(ProtocolError::MismatchedResponse))?;
                    let dispatch = prepare_dispatch(
                        generation,
                        ack.thread_id.clone(),
                        dispatch_rpc,
                        &request.binding,
                        &request.packet,
                    )?;
                    request.thread_id = Some(ack.thread_id.clone());
                    request.dispatch = Some(dispatch.clone());
                    request.assembler =
                        Some(TurnAssembler::new(ack.thread_id).map_err(protocol_error)?);
                    request.phase = RequestPhase::BeforeTurn;
                    request.phase_deadline = None;
                    (dispatch, request.before_turn.take(), request.stop.clone())
                };
                let tx = self.tx_for_commands();
                let request_id_for_thread = request_id.clone();
                if let Some(before) = before {
                    thread::spawn(move || {
                        if stop.is_requested() {
                            let _ = tx.send(Command::BeforeTurnFailed {
                                request_id: request_id_for_thread.clone(),
                                error: CoreError::new(
                                    "RequestStopped",
                                    "The request was stopped before turn/start.",
                                ),
                            });
                            return;
                        }
                        match before(&dispatch) {
                            Ok(()) => {
                                let _ = tx.send(Command::SubmitTurn {
                                    request_id: request_id_for_thread,
                                });
                            }
                            Err(error) => {
                                let _ = tx.send(Command::BeforeTurnFailed {
                                    request_id: request_id_for_thread,
                                    error,
                                });
                            }
                        }
                    });
                } else {
                    self.submit_turn(&request_id, outgoing)?;
                }
            }
            PendingRpc::TurnStart(request_id) => {
                let ack = TurnStartAck::from_result(&result).map_err(protocol_error)?;
                let request = self
                    .requests
                    .get_mut(&request_id)
                    .ok_or_else(|| protocol_error(ProtocolError::MismatchedResponse))?;
                if request
                    .turn_id
                    .as_deref()
                    .is_some_and(|turn_id| turn_id != ack.turn_id)
                {
                    return Err(protocol_error(ProtocolError::InvalidTurn));
                }
                request.submission = AppServerSubmission::Acknowledged;
                request.turn_id = Some(ack.turn_id.clone());
                let now = Instant::now();
                request.phase = RequestPhase::Active;
                request.phase_deadline = Some(now + TURN_TIMEOUT);
                let dispatch = request
                    .dispatch
                    .clone()
                    .ok_or_else(|| protocol_error(ProtocolError::MismatchedResponse))?;
                let on_turn = request.on_turn.take();
                request.turn_persistence_pending = on_turn.is_some();
                let tx = self.tx_for_commands();
                let request_id_for_turn = request_id.clone();
                if let Some(on_turn) = on_turn {
                    thread::spawn(move || {
                        let error = on_turn(&dispatch, &ack.turn_id)
                            .err()
                            .map(|error| error.detail);
                        let _ = tx.send(Command::TurnPersisted {
                            request_id: request_id_for_turn,
                            error,
                        });
                    });
                }
            }
            PendingRpc::TurnInterrupt(_) => {}
            PendingRpc::ThreadUnsubscribe(request_id) => {
                self.finish_unsubscribe(&request_id, outgoing)
            }
        }
        Ok(())
    }

    /// Publish the observable Ready state before waking the `start` caller.
    ///
    /// Callers may reserve a request immediately after `start` returns, so
    /// the readiness acknowledgement and the shared health state must be one
    /// ordered publication. A poisoned state cannot safely report success.
    fn publish_ready(&mut self) -> CoreResult<()> {
        let ready = &mut self.ready;
        publish_ready_state(&self.state, &mut self.ready_sent, || {
            if let Some(ready) = ready.take() {
                let _ = ready.send(Ok(()));
            }
        })
    }

    fn accept_notification(
        &mut self,
        method: String,
        params: Value,
        outgoing: &mut Vec<Vec<u8>>,
    ) -> CoreResult<()> {
        let known = matches!(
            method.as_str(),
            "turn/started"
                | "item/started"
                | "item/completed"
                | "item/agentMessage/delta"
                | "thread/tokenUsage/updated"
                | "turn/completed"
                | "thread/started"
                | "server/ready"
        );
        if !known {
            self.decoder
                .note_unknown_notification()
                .map_err(protocol_error)?;
            return Ok(());
        }
        let message = RpcMessage::Notification { method, params };
        let ids = self.requests.keys().cloned().collect::<Vec<_>>();
        for request_id in ids {
            let (events, completed_thread, inferred_ack) = {
                let Some(request) = self.requests.get_mut(&request_id) else {
                    continue;
                };
                let Some(assembler) = request.assembler.as_mut() else {
                    continue;
                };
                let events = assembler.accept(&message).map_err(protocol_error)?;
                let inferred_ack = if request.submission == AppServerSubmission::Uncertain
                    && request.turn_id.is_none()
                {
                    if let Some(turn_id) = assembler.turn_id() {
                        let turn_id = turn_id.to_owned();
                        let dispatch = request
                            .dispatch
                            .clone()
                            .ok_or_else(|| protocol_error(ProtocolError::MismatchedResponse))?;
                        request.submission = AppServerSubmission::Acknowledged;
                        request.turn_id = Some(turn_id.clone());
                        request.phase = RequestPhase::Active;
                        request.phase_deadline = Some(Instant::now() + TURN_TIMEOUT);
                        let on_turn = request.on_turn.take();
                        request.turn_persistence_pending = on_turn.is_some();
                        Some((turn_id, dispatch, on_turn))
                    } else {
                        None
                    }
                } else {
                    None
                };
                let mut completed_thread = None;
                for event in &events {
                    if let TurnEvent::Completed(completed) = event {
                        request.completed = Some(completed.clone());
                        request.phase = RequestPhase::Unsubscribe;
                        request.phase_deadline = Some(Instant::now() + RPC_SETTLEMENT_TIMEOUT);
                        completed_thread = request.thread_id.clone();
                    }
                }
                (events, completed_thread, inferred_ack)
            };
            if let Some((turn_id, dispatch, on_turn)) = inferred_ack {
                let rpc_id = RpcId::string(dispatch.rpc_id.clone());
                if matches!(
                    self.pending.get(&rpc_id),
                    Some(PendingRpc::TurnStart(pending_request)) if pending_request == &request_id
                ) {
                    self.pending.remove(&rpc_id);
                    self.retired_turn_rpcs.insert(rpc_id);
                }
                if let Some(on_turn) = on_turn {
                    let tx = self.tx_for_commands();
                    let request_id_for_turn = request_id.clone();
                    thread::spawn(move || {
                        let error = on_turn(&dispatch, &turn_id).err().map(|error| error.detail);
                        let _ = tx.send(Command::TurnPersisted {
                            request_id: request_id_for_turn,
                            error,
                        });
                    });
                }
            }
            for event in events {
                match event {
                    TurnEvent::AssistantDelta(text) => {
                        self.emit(&request_id, AppServerStreamEvent::AssistantDelta(text))?
                    }
                    TurnEvent::Completed(_) => {
                        let thread_id = completed_thread
                            .clone()
                            .ok_or_else(|| protocol_error(ProtocolError::InvalidTurn))?;
                        let rpc = self.next_rpc("unsubscribe");
                        self.pending.insert(
                            RpcId::string(rpc.clone()),
                            PendingRpc::ThreadUnsubscribe(request_id.clone()),
                        );
                        outgoing.push(
                            protocol::json_line(
                                rpc,
                                "thread/unsubscribe",
                                json!({"threadId":thread_id}),
                            )
                            .map_err(protocol_error)?,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn check_timeouts(&mut self, outgoing: &mut Vec<Vec<u8>>) -> CoreResult<()> {
        let ids = self.requests.keys().cloned().collect::<Vec<_>>();
        for request_id in ids {
            let Some((phase, deadline, stopped, interrupt_sent, terminal_sent)) =
                self.requests.get(&request_id).map(|request| {
                    (
                        request.phase,
                        request.phase_deadline,
                        request.stop.is_requested(),
                        request.interrupt_sent,
                        request.terminal_sent,
                    )
                })
            else {
                continue;
            };
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                if phase == RequestPhase::Active {
                    self.request_stop(&request_id, outgoing)?;
                } else {
                    self.poison(
                        unavailable(match phase {
                            RequestPhase::ThreadStart => {
                                "The Codex app-server thread/start did not settle."
                            }
                            RequestPhase::BeforeTurn => {
                                "The Codex app-server pre-turn operation did not settle."
                            }
                            RequestPhase::TurnStart => {
                                "The Codex app-server turn/start did not settle."
                            }
                            RequestPhase::Interrupt => {
                                "The Codex app-server interruption did not settle."
                            }
                            RequestPhase::Unsubscribe => {
                                "The Codex app-server thread cleanup did not settle."
                            }
                            RequestPhase::Active => unreachable!(),
                        }),
                        outgoing,
                    );
                    break;
                }
                continue;
            }
            if stopped && !interrupt_sent && !terminal_sent {
                self.request_stop(&request_id, outgoing)?;
            }
        }
        Ok(())
    }

    fn finish_unsubscribe(&mut self, request_id: &str, outgoing: &mut Vec<Vec<u8>>) {
        let persistence_pending = self.requests.get_mut(request_id).is_some_and(|request| {
            if request.turn_persistence_pending {
                request.unsubscribe_settled = true;
                true
            } else {
                false
            }
        });
        if !persistence_pending {
            self.finish_request(request_id, outgoing);
        }
    }

    fn finish_request(&mut self, request_id: &str, _outgoing: &mut Vec<Vec<u8>>) {
        let Some(mut request) = self.requests.remove(request_id) else {
            return;
        };
        let completed = request.completed.take();
        self.completed_threads = self.completed_threads.saturating_add(1);
        if self.completed_threads >= self.max_threads_per_connection {
            self.recycle_requested = true;
            if let Ok(mut state) = self.state.lock() {
                // Stop admission before asking the process to close. Existing
                // requests are allowed to settle before the idle recycle.
                state.health = AppServerHealth::Closed;
            }
        }
        let connection = if self.recycle_requested {
            AppServerConnectionSettlement::Closed
        } else {
            AppServerConnectionSettlement::Reusable
        };
        let delivery = AppServerDelivery {
            dispatch: request.dispatch.clone(),
            submission: request.submission,
            turn_id: request.turn_id.clone(),
            terminal: completed.as_ref().map(|completed| match completed.status {
                TurnStatus::Completed => AppServerTerminal::Completed,
                TurnStatus::Interrupted => AppServerTerminal::Interrupted,
                TurnStatus::Failed | TurnStatus::InProgress => AppServerTerminal::Failed,
            }),
            request_settled: true,
            connection,
        };
        let failure = completed
            .as_ref()
            .and_then(|completed| completed.failure.clone());
        let mut result = completed.map_or_else(
            || {
                run_result(
                    CodexRunStatus::ProtocolFailure(CodexFailureCode::Incomplete),
                    String::new(),
                    None,
                )
            },
            |completed| {
                run_result(
                    status_for(completed.status),
                    completed.text,
                    completed.usage,
                )
            },
        );
        result.cleanup_settled = delivery.request_settled;
        let _ = request
            .events
            .try_send(AppServerStreamEvent::Finished(Box::new(
                AppServerFinished {
                    result,
                    delivery,
                    failure,
                    local_failure: None,
                },
            )));
        request.terminal_sent = true;
        self.release_active();
        if self.recycle_requested && self.requests.is_empty() {
            self.shutdown_requested = true;
            self.stop.request_stop();
        }
    }

    fn finish_not_sent(&mut self, mut request: RequestState, detail: &str) {
        self.finish_delivery(
            &mut request,
            AppServerDelivery::not_sent(),
            run_result(CodexRunStatus::Stopped, String::new(), None),
        );
        let _ = detail;
    }

    fn finish_rejected(&mut self, mut request: RequestState) {
        self.finish_delivery(
            &mut request,
            AppServerDelivery::not_sent(),
            run_result(
                CodexRunStatus::ProtocolFailure(CodexFailureCode::ProviderFailure),
                String::new(),
                None,
            ),
        );
    }

    fn finish_not_submitted(&mut self, mut request: RequestState) {
        let delivery = AppServerDelivery {
            dispatch: request.dispatch.clone(),
            submission: AppServerSubmission::NotSent,
            turn_id: None,
            terminal: None,
            request_settled: true,
            connection: AppServerConnectionSettlement::Reusable,
        };
        self.finish_delivery(
            &mut request,
            delivery,
            run_result(CodexRunStatus::Stopped, String::new(), None),
        );
    }

    fn finish_uncertain(&mut self, mut request: RequestState) {
        let delivery = AppServerDelivery {
            dispatch: request.dispatch.clone(),
            submission: AppServerSubmission::Uncertain,
            turn_id: None,
            terminal: None,
            request_settled: false,
            connection: AppServerConnectionSettlement::Unresolved,
        };
        self.finish_delivery(
            &mut request,
            delivery,
            run_result(CodexRunStatus::CleanupUnresolved, String::new(), None),
        );
    }

    fn finish_delivery(
        &mut self,
        request: &mut RequestState,
        delivery: AppServerDelivery,
        mut result: CodexRunResult,
    ) {
        result.cleanup_settled = delivery.request_settled;
        let _ = request
            .events
            .try_send(AppServerStreamEvent::Finished(Box::new(
                AppServerFinished {
                    result,
                    delivery,
                    failure: None,
                    local_failure: self.local_failure,
                },
            )));
        request.terminal_sent = true;
        self.release_active();
    }

    fn emit(&self, request_id: &str, event: AppServerStreamEvent) -> CoreResult<()> {
        let Some(request) = self.requests.get(request_id) else {
            return Ok(());
        };
        match request.events.try_send(event) {
            Ok(()) => Ok(()),
            Err(TrySendError::Disconnected(_)) => {
                Err(unavailable("The app-server request consumer disconnected."))
            }
            Err(TrySendError::Full(_)) => {
                Err(unavailable("The app-server request consumer is too slow."))
            }
        }
    }

    fn poison(&mut self, error: CoreError, _outgoing: &mut Vec<Vec<u8>>) {
        if self.local_failure.is_none() {
            self.local_failure = Some(local_failure_for_error(&error));
        }
        self.poisoned = true;
        if let Ok(mut state) = self.state.lock() {
            state.health = AppServerHealth::Poisoned;
        }
        if self.ready_sent {
            // Initialization already settled; there is no readiness waiter.
        } else if let Some(ready) = self.ready.take() {
            let _ = ready.send(Err(error));
        }
        self.stop.request_stop();
    }

    fn finish_process(
        &mut self,
        result: Result<crate::cli::windows_process::ChildOutcome, ContainmentError>,
    ) {
        if self.ready_sent {
            // Initialization already settled; there is no readiness waiter.
        } else if let Some(ready) = self.ready.take() {
            let _ = ready.send(Err(unavailable(
                "The Codex app-server closed during initialization.",
            )));
        }
        let connection = if result.is_ok() {
            AppServerConnectionSettlement::Closed
        } else {
            AppServerConnectionSettlement::Unresolved
        };
        let local_failure = self.local_failure.or_else(|| {
            Some(if result.is_ok() {
                AppServerLocalFailure::ProcessExited
            } else {
                AppServerLocalFailure::ProcessCleanup
            })
        });
        let ids = self.requests.keys().cloned().collect::<Vec<_>>();
        for request_id in ids {
            if let Some(request) = self.requests.remove(&request_id) {
                let completed_text = request
                    .completed
                    .as_ref()
                    .map(|completed| completed.text.clone());
                let completed_usage = request
                    .completed
                    .as_ref()
                    .and_then(|completed| completed.usage);
                let completed_terminal =
                    request
                        .completed
                        .as_ref()
                        .map(|completed| match completed.status {
                            TurnStatus::Completed => AppServerTerminal::Completed,
                            TurnStatus::Interrupted => AppServerTerminal::Interrupted,
                            TurnStatus::Failed | TurnStatus::InProgress => {
                                AppServerTerminal::Failed
                            }
                        });
                let assistant_text = completed_text
                    .or_else(|| {
                        request
                            .assembler
                            .as_ref()
                            .map(|assembler| assembler.observed_text().to_owned())
                    })
                    .unwrap_or_default();
                let delivery = AppServerDelivery {
                    dispatch: request.dispatch,
                    // An acknowledged turn ID remains acknowledged even when
                    // the process dies before terminal settlement. Only an
                    // actually submitted frame without its response is
                    // uncertain; collapsing the acknowledged case would
                    // produce a receipt that fails its own identity contract.
                    submission: request.submission,
                    turn_id: request.turn_id,
                    terminal: completed_terminal,
                    request_settled: false,
                    connection,
                };
                let mut result = run_result(
                    CodexRunStatus::CleanupUnresolved,
                    assistant_text,
                    completed_usage,
                );
                result.cleanup_settled = delivery.request_settled;
                let _ = request
                    .events
                    .try_send(AppServerStreamEvent::Finished(Box::new(
                        AppServerFinished {
                            result,
                            delivery,
                            failure: None,
                            local_failure,
                        },
                    )));
                self.release_active();
            }
        }
        if let Ok(mut state) = self.state.lock() {
            state.health = if self.poisoned {
                AppServerHealth::Poisoned
            } else {
                AppServerHealth::Closed
            };
            // `finish_persistent` returns Ok only after the Job Object and
            // child process have settled.  Any containment error leaves
            // cleanup unresolved and makes shutdown fail closed.
            state.cleanup_settled = result.is_ok();
        }
    }

    fn next_rpc(&mut self, prefix: &str) -> String {
        let value = self.next_rpc;
        self.next_rpc = self.next_rpc.saturating_add(1);
        format!("{prefix}-{value}")
    }

    fn tx_for_commands(&self) -> SyncSender<Command> {
        self.commands_tx.clone()
    }

    fn release_active(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.active = state.active.saturating_sub(1);
        }
    }
}

fn run_result(
    status: CodexRunStatus,
    assistant_text: String,
    usage: Option<protocol::TurnUsage>,
) -> CodexRunResult {
    CodexRunResult {
        status,
        assistant_text,
        usage: usage.map(|usage| CodexUsage {
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            cache_write_input_tokens: usage.cache_write_input_tokens,
            output_tokens: usage.output_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        }),
        confirmed_stdin_bytes: 0,
        warning_count: 0,
        cleanup_settled: true,
    }
}

fn local_failure_for_error(error: &CoreError) -> AppServerLocalFailure {
    if error.code == "CodexAppServerProtocol"
        && let Some(code) = protocol::ProtocolFailureCode::from_detail(&error.detail)
    {
        return AppServerLocalFailure::Protocol(code);
    }
    if error.code == "UncertainOutcome"
        || error.code == "PersistenceUnavailable"
        || error.code.starts_with("Persistence")
    {
        return AppServerLocalFailure::Persistence;
    }
    if error.code == "CodexAppServerUnavailable" {
        if error.detail.contains("consumer disconnected") {
            return AppServerLocalFailure::ConsumerDisconnected;
        }
        if error.detail.contains("consumer could not process") {
            return AppServerLocalFailure::ConsumerTooSlow;
        }
        if error.detail.contains("did not settle") {
            return AppServerLocalFailure::RpcTimeout;
        }
    }
    AppServerLocalFailure::Unavailable
}

fn status_for(status: TurnStatus) -> CodexRunStatus {
    match status {
        TurnStatus::Completed => CodexRunStatus::Completed,
        TurnStatus::Interrupted => CodexRunStatus::Stopped,
        TurnStatus::Failed | TurnStatus::InProgress => {
            CodexRunStatus::ProtocolFailure(CodexFailureCode::ProviderFailure)
        }
    }
}

fn protocol_error(error: ProtocolError) -> CoreError {
    CoreError::new("CodexAppServerProtocol", &error.to_string())
}

fn containment_error(error: ContainmentError) -> CoreError {
    CoreError::new("CodexAppServerUnavailable", &error.to_string())
}

fn unavailable(detail: &str) -> CoreError {
    CoreError::new("CodexAppServerUnavailable", detail)
}

fn publish_ready_state(
    state: &Arc<Mutex<ConnectionState>>,
    ready_sent: &mut bool,
    acknowledge: impl FnOnce(),
) -> CoreResult<()> {
    {
        let mut state = state
            .lock()
            .map_err(|_| unavailable("connection state poisoned"))?;
        state.health = AppServerHealth::Ready;
    }
    *ready_sent = true;
    acknowledge();
    Ok(())
}

fn require_cleanup_settled(state: &Arc<Mutex<ConnectionState>>) -> CoreResult<()> {
    let cleanup_settled = state
        .lock()
        .map(|state| state.cleanup_settled)
        .map_err(|_| unavailable("The Codex app-server cleanup state is unavailable."))?;
    if !cleanup_settled {
        return Err(unavailable(
            "The Codex app-server worker stopped without settled process cleanup.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_check_fails_closed_until_worker_settles() {
        let state = Arc::new(Mutex::new(ConnectionState {
            health: AppServerHealth::Closed,
            active: 0,
            cleanup_settled: false,
        }));
        let error = require_cleanup_settled(&state).expect_err("unsettled cleanup must fail");
        assert_eq!(error.code, "CodexAppServerUnavailable");

        state.lock().expect("state lock").cleanup_settled = true;
        require_cleanup_settled(&state).expect("settled cleanup should be accepted");
    }

    #[test]
    fn readiness_acknowledges_only_after_ready_state_is_published() {
        let state = Arc::new(Mutex::new(ConnectionState {
            health: AppServerHealth::Starting,
            active: 0,
            cleanup_settled: false,
        }));
        let mut ready_sent = false;
        let state_for_ack = Arc::clone(&state);

        publish_ready_state(&state, &mut ready_sent, move || {
            assert_eq!(
                state_for_ack.lock().expect("state lock").health,
                AppServerHealth::Ready
            );
        })
        .expect("ready state should publish");

        assert!(ready_sent);
        assert_eq!(
            state.lock().expect("state lock").health,
            AppServerHealth::Ready
        );
    }

    #[test]
    fn reservation_release_is_exactly_once() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let state = Arc::new(Mutex::new(ConnectionState {
            health: AppServerHealth::Ready,
            active: 1,
            cleanup_settled: false,
        }));
        let mut reservation = AppServerReservation {
            handle: ConnectionHandle {
                tx,
                state: Arc::clone(&state),
            },
            request_id: "request-test".into(),
            started: false,
            slot_released: false,
        };

        reservation.release_slot();
        reservation.release_slot();
        assert_eq!(state.lock().expect("state lock").active, 0);
        drop(reservation);
        assert_eq!(state.lock().expect("state lock").active, 0);
    }

    #[test]
    fn local_failure_mapping_retains_only_bounded_categories() {
        let protocol = local_failure_for_error(&protocol_error(ProtocolError::InvalidTurn));
        assert_eq!(
            protocol,
            AppServerLocalFailure::Protocol(protocol::ProtocolFailureCode::InvalidTurn)
        );
        assert_eq!(protocol.as_str(), "protocol_invalid_turn");

        let persistence = local_failure_for_error(&CoreError::new(
            "UncertainOutcome",
            "discarded persistence detail",
        ));
        assert_eq!(persistence, AppServerLocalFailure::Persistence);
        assert_eq!(
            persistence.safe_detail(),
            "The application could not durably acknowledge the app-server request."
        );

        let timeout = local_failure_for_error(&unavailable(
            "The Codex app-server thread cleanup did not settle.",
        ));
        assert_eq!(timeout, AppServerLocalFailure::RpcTimeout);
    }
}
