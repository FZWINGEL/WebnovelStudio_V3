use super::*;

pub const MAX_PACKET_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OVERALL: Duration = Duration::from_secs(24 * 60 * 60);
pub const MAX_STOP_GRACE: Duration = Duration::from_secs(30);
pub const MAX_TOTAL_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
/// Maximum number of bytes that may wait in the persistent stdin queue.  This
/// is deliberately separate from `MAX_PACKET_BYTES`: a long-lived server can
/// accept many packets over its lifetime, but a slow or stalled server must
/// never make the parent retain an unbounded amount of input.
pub const MAX_PERSISTENT_PENDING_BYTES: usize = 8 * 1024 * 1024;
/// Persistent runs keep a bounded diagnostic prefix while delivering every
/// output chunk to the observer.  This prevents a healthy server's lifetime
/// traffic from becoming a process-wide output limit or an unbounded buffer.
pub const MAX_PERSISTENT_DIAGNOSTIC_BYTES: usize = 256 * 1024;
pub(crate) const OUTPUT_CHUNK_BYTES: usize = 8 * 1024;
pub(crate) const OUTPUT_QUEUE_CHUNKS: usize = 32;
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(20);
pub(crate) const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const MAX_JOB_PROCESSES: usize = 4096;

/// How the child environment is formed.  The caller must choose a policy;
/// there is no implicit environment or credential lookup in this primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentPolicy {
    /// Inherit the host environment exactly as CreateProcessW would.
    Inherit,
    /// Start with an empty environment block.
    Clear,
    /// Use only these key/value pairs.
    Explicit(BTreeMap<OsString, OsString>),
}

/// Limits applied to one invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildLimits {
    pub overall: Duration,
    pub stop_grace: Duration,
    pub max_total_output_bytes: usize,
}

impl Default for ChildLimits {
    fn default() -> Self {
        Self {
            overall: Duration::from_secs(120),
            stop_grace: Duration::from_secs(2),
            max_total_output_bytes: 4 * 1024 * 1024,
        }
    }
}

impl ChildLimits {
    pub(crate) fn validate(self) -> Result<Self, ContainmentError> {
        if self.overall.is_zero() {
            return Err(ContainmentError::InvalidInvocation(
                "overall deadline must be greater than zero".to_owned(),
            ));
        }
        if self.overall > MAX_OVERALL {
            return Err(ContainmentError::InvalidInvocation(format!(
                "overall deadline exceeds {MAX_OVERALL:?}"
            )));
        }
        if self.stop_grace.is_zero() {
            return Err(ContainmentError::InvalidInvocation(
                "stop grace must be greater than zero".to_owned(),
            ));
        }
        if self.stop_grace > MAX_STOP_GRACE {
            return Err(ContainmentError::InvalidInvocation(format!(
                "stop grace exceeds {MAX_STOP_GRACE:?}"
            )));
        }
        if self.max_total_output_bytes == 0 {
            return Err(ContainmentError::InvalidInvocation(
                "max_total_output_bytes must be greater than zero".to_owned(),
            ));
        }
        if self.max_total_output_bytes > MAX_TOTAL_OUTPUT_BYTES {
            return Err(ContainmentError::InvalidInvocation(format!(
                "max_total_output_bytes exceeds {MAX_TOTAL_OUTPUT_BYTES} bytes"
            )));
        }
        Ok(self)
    }
}

/// All values needed to create one child.  `arguments` are provider options;
/// packet bytes are always written to stdin and never appended to them.
#[derive(Debug, Clone)]
pub struct CliInvocation {
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub cwd: PathBuf,
    pub environment: EnvironmentPolicy,
    pub packet: Vec<u8>,
    pub limits: ChildLimits,
}

/// Cooperative local stop request.  It does not claim that an upstream
/// provider stopped billing or processing a request.
#[derive(Debug, Clone, Default)]
pub struct StopSignal(Arc<AtomicBool>);

impl StopSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request_stop(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Which captured child stream produced an observed output chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildStream {
    Stdout,
    Stderr,
}

/// Terminal reason after local cleanup settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildTermination {
    Completed,
    Stopped,
    TimedOut,
    OutputLimitExceeded,
}

/// Captured output.  A limit outcome retains the prefix accepted before the
/// cap was reached and marks it as truncated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildOutput {
    /// The process exit code after local process cleanup, when available.
    pub exit_code: Option<u32>,
    /// Confirmed writes to the child's stdin pipe; not proof of model reading.
    pub stdin_bytes_written: usize,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncated: bool,
    pub io_errors: Vec<ChildIoError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildIoError {
    ReadStdout(u32),
    ReadStderr(u32),
    WriteStdin(i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildOutcome {
    pub termination: ChildTermination,
    pub output: ChildOutput,
}

/// Action returned by an interactive stdout observer. Interactive children
/// keep stdin open between bounded JSONL request packets; the observer decides
/// when to append another packet or close the pipe after pending bytes drain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractiveAction {
    KeepOpen,
    Send(Vec<u8>),
    Close,
}

/// Event delivered by [`RunningChild::finish_persistent`].  `Tick` is
/// emitted at the same bounded cadence used by the pipe loop, including when
/// the child is completely idle.  `Output` contains the full read chunk; the
/// persistent diagnostic prefix is tracked independently and never truncates
/// the stream delivered to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentEvent<'a> {
    Tick,
    Output(ChildStream, &'a [u8]),
}

// A TimedOut/Stopped outcome means the Job Object was terminated and local
// worker cleanup settled.  A cleanup failure is returned as ContainmentError;
// it never gets relabelled as an ordinary provider failure or successful run.

/// A containment failure is kept distinct from a provider's nonzero exit.
/// If cleanup failed after output began, `partial` preserves the captured
/// prefix for diagnostics and an explicit recovery decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainmentError {
    InvalidInvocation(String),
    Win32 {
        operation: &'static str,
        code: u32,
    },
    Cleanup {
        stage: &'static str,
        partial: Option<ChildOutput>,
    },
}

impl std::fmt::Display for ContainmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInvocation(message) => formatter.write_str(message),
            Self::Win32 { operation, code } => write!(formatter, "{operation} failed ({code})"),
            Self::Cleanup { stage, .. } => write!(formatter, "cleanup did not settle at {stage}"),
        }
    }
}

impl std::error::Error for ContainmentError {}

/// The sole owner of the process and Job Object handles.  Dropping it kills
/// the associated tree as a final RAII guard; normal callers should consume it
/// with [`RunningChild::finish_or_stop`] so the outcome is explicit.
pub struct RunningChild {
    pub(crate) process: OwnedHandle,
    pub(crate) job: Option<OwnedHandle>,
    pub(crate) output_rx: Receiver<OutputMessage>,
    pub(crate) worker_rx: Receiver<WorkerDone>,
    pub(crate) stdin_pipe: Arc<SharedPipe>,
    pub(crate) stdout_pipe: Arc<SharedPipe>,
    pub(crate) stderr_pipe: Arc<SharedPipe>,
    pub(crate) stdout_reader: Option<JoinHandle<()>>,
    pub(crate) stderr_reader: Option<JoinHandle<()>>,
    pub(crate) io_stop: StopSignal,
    pub(crate) cleanup_processes: Option<Vec<OwnedHandle>>,
    pub(crate) stdin_packet: Vec<u8>,
    pub(crate) stdin_offset: usize,
    pub(crate) stdin_total_written: usize,
    pub(crate) stdin_write: Option<Box<PendingWrite>>,
    pub(crate) stdin_error: Option<ChildIoError>,
    pub(crate) interactive: bool,
    pub(crate) persistent: bool,
    pub(crate) close_stdin_when_drained: bool,
    pub(crate) process_id: u32,
    pub(crate) limits: ChildLimits,
    pub(crate) started_at: Instant,
}
