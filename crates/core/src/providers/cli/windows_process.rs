//! Windows CLI process containment and bounded I/O.
//!
//! This module is not a shell runner and is not a provider adapter.  It owns
//! only the mechanics needed by a future qualified adapter: an absolute
//! executable, a fixed argument vector, a bounded stdin packet, explicit
//! environment policy, a restricted inherited-handle list, and a Job Object
//! that contains the complete child tree.

#![cfg(windows)]

use std::cell::UnsafeCell;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString, c_void};
use std::mem::{size_of, size_of_val};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_INSUFFICIENT_BUFFER, ERROR_IO_INCOMPLETE,
    ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_NOT_FOUND, GENERIC_WRITE, GetLastError, HANDLE,
    HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    OPEN_EXISTING, PIPE_ACCESS_INBOUND, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, CreatePipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateEventW, CreateProcessW,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcessId,
    GetExitCodeProcess, InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};

/// Maximum packet accepted by this low-level boundary.  Provider adapters can
/// impose a smaller protocol-specific limit.
pub const MAX_PACKET_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OVERALL: Duration = Duration::from_secs(24 * 60 * 60);
pub const MAX_STOP_GRACE: Duration = Duration::from_secs(30);
pub const MAX_TOTAL_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const OUTPUT_CHUNK_BYTES: usize = 8 * 1024;
const OUTPUT_QUEUE_CHUNKS: usize = 32;
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);

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
    fn validate(self) -> Result<Self, ContainmentError> {
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
    process: OwnedHandle,
    job: Option<OwnedHandle>,
    output_rx: Receiver<OutputMessage>,
    worker_rx: Receiver<WorkerDone>,
    stdin_pipe: Arc<SharedPipe>,
    stdout_pipe: Arc<SharedPipe>,
    stderr_pipe: Arc<SharedPipe>,
    stdout_reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
    io_stop: StopSignal,
    stdin_packet: Vec<u8>,
    stdin_offset: usize,
    stdin_write: Option<Box<PendingWrite>>,
    stdin_error: Option<ChildIoError>,
    process_id: u32,
    limits: ChildLimits,
    started_at: Instant,
}

/// Create a suspended, job-contained child and start bounded pipe workers.
pub fn spawn(invocation: CliInvocation) -> Result<RunningChild, ContainmentError> {
    let limits = invocation.limits.validate()?;
    if invocation.packet.len() > MAX_PACKET_BYTES {
        return Err(ContainmentError::InvalidInvocation(format!(
            "stdin packet exceeds {MAX_PACKET_BYTES} bytes"
        )));
    }
    validate_path("executable", &invocation.executable, true)?;
    validate_path("cwd", &invocation.cwd, false)?;
    let command_line = command_line(&invocation.executable, &invocation.arguments)?;
    let environment = EnvironmentBlock::new(&invocation.environment)?;
    let executable_wide = wide_path(&invocation.executable, "executable")?;
    let cwd_wide = wide_path(&invocation.cwd, "cwd")?;

    let job = create_job()?;
    let mut stdout_read = null_handle();
    let mut stdout_write = null_handle();
    let mut stderr_read = null_handle();
    let mut stderr_write = null_handle();
    let mut pipe_attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };

    let (stdin_read, stdin_write) = unsafe { create_stdin_pipe(&pipe_attributes)? };
    unsafe {
        if let Err(error) = create_pipe(
            &mut stdout_read,
            &mut stdout_write,
            &mut pipe_attributes,
            "CreatePipe(stdout)",
        ) {
            close_handle(stdin_read);
            close_handle(stdin_write);
            return Err(error);
        }
        if let Err(error) = create_pipe(
            &mut stderr_read,
            &mut stderr_write,
            &mut pipe_attributes,
            "CreatePipe(stderr)",
        ) {
            close_handle(stdin_read);
            close_handle(stdin_write);
            close_handle(stdout_read);
            close_handle(stdout_write);
            return Err(error);
        }
    }

    let stdin_parent = unsafe { owned_handle(stdin_write)? };
    let stdout_parent = unsafe { owned_handle(stdout_read)? };
    let stderr_parent = unsafe { owned_handle(stderr_read)? };
    unsafe {
        // The parent-side handles must never be inherited.  The child receives
        // exactly the three handles listed in PROC_THREAD_ATTRIBUTE_HANDLE_LIST.
        if let Err(error) = set_non_inheritable(
            stdin_parent.as_raw_handle() as HANDLE,
            "SetHandleInformation(stdin)",
        ) {
            close_handle(stdin_read);
            close_handle(stdout_write);
            close_handle(stderr_write);
            return Err(error);
        }
        if let Err(error) = set_non_inheritable(
            stdout_parent.as_raw_handle() as HANDLE,
            "SetHandleInformation(stdout)",
        ) {
            close_handle(stdin_read);
            close_handle(stdout_write);
            close_handle(stderr_write);
            return Err(error);
        }
        if let Err(error) = set_non_inheritable(
            stderr_parent.as_raw_handle() as HANDLE,
            "SetHandleInformation(stderr)",
        ) {
            close_handle(stdin_read);
            close_handle(stdout_write);
            close_handle(stderr_write);
            return Err(error);
        }
    }

    let child_handles = [stdin_read, stdout_write, stderr_write];
    let mut attribute_storage = match AttributeList::new(&child_handles) {
        Ok(attributes) => attributes,
        Err(error) => {
            unsafe {
                close_handle(stdin_read);
                close_handle(stdout_write);
                close_handle(stderr_write);
            }
            return Err(error);
        }
    };
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = stdin_read;
    startup.StartupInfo.hStdOutput = stdout_write;
    startup.StartupInfo.hStdError = stderr_write;
    startup.lpAttributeList = attribute_storage.as_mut_ptr();

    let mut command_line = command_line;
    let mut process_information = PROCESS_INFORMATION::default();
    let creation_flags = EXTENDED_STARTUPINFO_PRESENT
        | CREATE_SUSPENDED
        | CREATE_UNICODE_ENVIRONMENT
        | CREATE_NO_WINDOW;
    let created = unsafe {
        CreateProcessW(
            executable_wide.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            creation_flags,
            environment.as_ptr(),
            cwd_wide.as_ptr(),
            &startup.StartupInfo,
            &mut process_information,
        )
    };
    let create_error = if created == 0 {
        Some(last_error())
    } else {
        None
    };
    unsafe {
        attribute_storage.delete();
        close_handle(stdin_read);
        close_handle(stdout_write);
        close_handle(stderr_write);
    }
    if created == 0 {
        return Err(ContainmentError::Win32 {
            operation: "CreateProcessW",
            code: create_error.expect("CreateProcessW failure code"),
        });
    }

    let process = match unsafe { owned_handle(process_information.hProcess) } {
        Ok(process) => process,
        Err(error) => {
            unsafe { close_handle(process_information.hThread) };
            return Err(error);
        }
    };
    let thread = match unsafe { owned_handle(process_information.hThread) } {
        Ok(thread) => thread,
        Err(error) => {
            unsafe { close_handle(process_information.hThread) };
            if unsafe { TerminateProcess(raw(&process), 1) } == 0
                || unsafe { WaitForSingleObject(raw(&process), duration_ms(CLEANUP_TIMEOUT)) }
                    != WAIT_OBJECT_0
            {
                return Err(ContainmentError::Cleanup {
                    stage: "process handle cleanup",
                    partial: None,
                });
            }
            return Err(error);
        }
    };
    if unsafe { AssignProcessToJobObject(raw(&job), raw(&process)) } == 0 {
        let code = last_error();
        if let Some(cleanup) =
            terminate_unassigned_process(&process, "AssignProcessToJobObject cleanup").err()
        {
            return Err(cleanup);
        }
        return Err(ContainmentError::Win32 {
            operation: "AssignProcessToJobObject",
            code,
        });
    }
    let resume_result = unsafe { ResumeThread(raw(&thread)) };
    if resume_result != 1 {
        let code = if resume_result == u32::MAX {
            last_error()
        } else {
            0
        };
        if let Some(cleanup) =
            terminate_assigned_process(&job, &process, "ResumeThread cleanup").err()
        {
            return Err(cleanup);
        }
        return Err(ContainmentError::Win32 {
            operation: "ResumeThread",
            code,
        });
    }
    drop(thread);

    let stdin_pipe = Arc::new(SharedPipe::new(stdin_parent));
    let stdout_pipe = Arc::new(SharedPipe::new(stdout_parent));
    let stderr_pipe = Arc::new(SharedPipe::new(stderr_parent));
    let io_stop = StopSignal::new();
    let (output_tx, output_rx) = mpsc::sync_channel(OUTPUT_QUEUE_CHUNKS);
    let (worker_tx, worker_rx) = mpsc::channel();
    let stdout_reader = Some(start_reader(
        stdout_pipe.clone(),
        OutputStream::Stdout,
        output_tx.clone(),
        worker_tx.clone(),
        io_stop.clone(),
    ));
    let stderr_reader = Some(start_reader(
        stderr_pipe.clone(),
        OutputStream::Stderr,
        output_tx,
        worker_tx.clone(),
        io_stop.clone(),
    ));
    Ok(RunningChild {
        process,
        job: Some(job),
        output_rx,
        worker_rx,
        stdin_pipe,
        stdout_pipe,
        stderr_pipe,
        stdout_reader,
        stderr_reader,
        io_stop,
        stdin_packet: invocation.packet,
        stdin_offset: 0,
        stdin_write: None,
        stdin_error: None,
        process_id: process_information.dwProcessId,
        limits,
        started_at: Instant::now(),
    })
}

impl RunningChild {
    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    /// Wait for normal completion, local Stop, timeout, or output overflow.
    /// The process tree is terminated through the Job Object when required;
    /// the returned output is the exact bounded prefix captured before that
    /// terminal action.
    pub fn finish_or_stop(self, stop: StopSignal) -> Result<ChildOutcome, ContainmentError> {
        self.finish_or_stop_with_output(stop, |_stream, _bytes| {})
    }

    /// Finish the child while synchronously observing each accepted bounded
    /// output prefix. The callback runs on this caller's thread and must be
    /// short and nonblocking; an arbitrary blocking callback can delay local
    /// cleanup beyond the configured process deadlines.
    ///
    /// The callback receives each stream's bytes in that stream's read order,
    /// including bytes accepted during the final drain. Bytes beyond the
    /// combined output cap are retained neither in the outcome nor delivered
    /// to the callback. Once the finish path observes `Stop`, subsequent
    /// accepted bytes remain available in the bounded outcome but are not
    /// observed.
    pub fn finish_or_stop_with_output<F>(
        mut self,
        stop: StopSignal,
        mut observer: F,
    ) -> Result<ChildOutcome, ContainmentError>
    where
        F: FnMut(ChildStream, &[u8]),
    {
        let mut capture = Capture::default();
        let mut terminal = None;
        let mut stop_deadline = None;
        let mut termination_sent = false;
        let mut observer_requested_stop = false;

        loop {
            if !stop.is_requested() {
                self.progress_stdin(&mut capture)?;
            }
            let process_done =
                unsafe { WaitForSingleObject(raw(&self.process), 0) } == WAIT_OBJECT_0;
            if process_done {
                terminal.get_or_insert(if stop.is_requested() {
                    ChildTermination::Stopped
                } else {
                    ChildTermination::Completed
                });
                break;
            }

            if stop.is_requested() {
                terminal.get_or_insert(ChildTermination::Stopped);
                let grace_deadline = Instant::now() + self.limits.stop_grace;
                let overall_deadline = self.started_at + self.limits.overall;
                stop_deadline.get_or_insert(grace_deadline.min(overall_deadline));
            }
            if let Some(deadline) = stop_deadline {
                if Instant::now() >= deadline {
                    self.terminate_job(&mut capture, "TerminateJobObject(stop)")?;
                    termination_sent = true;
                    break;
                }
            } else if self.started_at.elapsed() >= self.limits.overall {
                terminal = Some(ChildTermination::TimedOut);
                self.terminate_job(&mut capture, "TerminateJobObject(timeout)")?;
                termination_sent = true;
                break;
            }

            let wait = self.next_wait(stop_deadline);
            match self.output_rx.recv_timeout(wait) {
                Ok(message) => capture.accept(
                    message,
                    self.limits.max_total_output_bytes,
                    &stop,
                    &mut observer_requested_stop,
                    &mut observer,
                ),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    if unsafe {
                        WaitForSingleObject(raw(&self.process), duration_ms(CLEANUP_TIMEOUT))
                    } == WAIT_OBJECT_0
                    {
                        terminal.get_or_insert(ChildTermination::Completed);
                        break;
                    }
                    return Err(self.cleanup_error("output channel disconnected", capture));
                }
            }
            if capture.limit_reached && terminal.is_none() {
                terminal = Some(ChildTermination::OutputLimitExceeded);
                self.terminate_job(&mut capture, "TerminateJobObject(output-limit)")?;
                termination_sent = true;
                break;
            }
        }

        if !termination_sent
            && matches!(
                terminal,
                Some(ChildTermination::Completed | ChildTermination::Stopped)
            )
        {
            // A provider may leave descendants holding stdout/stderr open
            // after its root exits.  Close the whole job before draining so
            // inherited pipe handles cannot extend the cleanup indefinitely.
            self.terminate_job(&mut capture, "TerminateJobObject(completion)")?;
            termination_sent = true;
        }
        if termination_sent {
            let wait =
                unsafe { WaitForSingleObject(raw(&self.process), duration_ms(CLEANUP_TIMEOUT)) };
            if wait != WAIT_OBJECT_0 {
                return Err(self.cleanup_error("process wait", capture));
            }
            self.wait_for_job_empty(&mut capture)?;
        }
        self.drain_output(
            &mut capture,
            &stop,
            &mut observer_requested_stop,
            &mut observer,
        )?;
        if capture.limit_reached && terminal == Some(ChildTermination::Completed) {
            // A fast root can exit before its final output has been drained.
            // Keep the terminal reason consistent with the same cap observed
            // while the root was still running.
            terminal = Some(ChildTermination::OutputLimitExceeded);
        } else if observer_requested_stop && terminal == Some(ChildTermination::Completed) {
            // The observer may request Stop while the final drain is still
            // delivering output after the root process has exited.
            terminal = Some(ChildTermination::Stopped);
        }
        let exit_code = match exit_code(&self.process) {
            Ok(code) => code,
            Err(error) => {
                return Err(match error {
                    ContainmentError::Win32 { operation, .. } => ContainmentError::Cleanup {
                        stage: operation,
                        partial: Some(ChildOutput {
                            exit_code: None,
                            stdin_bytes_written: self.stdin_offset,
                            stdout: capture.stdout,
                            stderr: capture.stderr,
                            truncated: capture.truncated,
                            io_errors: capture.io_errors,
                        }),
                    },
                    other => other,
                });
            }
        };
        let output = ChildOutput {
            exit_code: Some(exit_code),
            stdin_bytes_written: self.stdin_offset,
            stdout: capture.stdout,
            stderr: capture.stderr,
            truncated: capture.truncated,
            io_errors: capture.io_errors,
        };
        let output = self.join_workers(output)?;
        if terminal == Some(ChildTermination::Completed)
            && exit_code == 0
            && output.stdin_bytes_written != self.stdin_packet.len()
        {
            return Err(ContainmentError::Cleanup {
                stage: "incomplete stdin delivery",
                partial: Some(output),
            });
        }
        if output.io_errors.iter().any(|error| {
            matches!(
                error,
                ChildIoError::ReadStdout(_) | ChildIoError::ReadStderr(_)
            )
        }) {
            return Err(ContainmentError::Cleanup {
                stage: "reader I/O",
                partial: Some(output),
            });
        }
        if output.io_errors.iter().any(
            |error| matches!(error, ChildIoError::WriteStdin(code) if !expected_write_error(*code)),
        ) {
            return Err(ContainmentError::Cleanup {
                stage: "stdin I/O",
                partial: Some(output),
            });
        }
        // Close the Job Object only after process and pipe workers settle.  It
        // is optional here so RunningChild's Drop guard does not run twice.
        self.job.take();
        Ok(ChildOutcome {
            termination: terminal.unwrap_or(ChildTermination::Completed),
            output,
        })
    }

    fn next_wait(&self, stop_deadline: Option<Instant>) -> Duration {
        let mut wait = POLL_INTERVAL;
        if let Some(deadline) = stop_deadline {
            wait = wait.min(deadline.saturating_duration_since(Instant::now()));
        }
        wait = wait.min(
            self.limits
                .overall
                .saturating_sub(self.started_at.elapsed()),
        );
        wait.max(Duration::from_millis(1))
    }

    fn progress_stdin(&mut self, capture: &mut Capture) -> Result<(), ContainmentError> {
        if let Some(pending) = self.stdin_write.as_ref() {
            if unsafe { WaitForSingleObject(raw(&pending.event), 0) } != WAIT_OBJECT_0 {
                return Ok(());
            }
            let mut transferred = 0_u32;
            let pending = self.stdin_write.take().expect("pending stdin write");
            match reap_pending_write(&pending, &mut transferred) {
                Ok(()) if transferred == pending.length => {
                    self.stdin_offset += pending.length as usize;
                }
                Ok(()) => {
                    self.stdin_error = Some(ChildIoError::WriteStdin(0));
                    self.stdin_pipe.close();
                }
                Err(code) if code == ERROR_IO_PENDING => {
                    self.stdin_write = Some(pending);
                    return Ok(());
                }
                Err(code) => {
                    self.stdin_error = Some(ChildIoError::WriteStdin(code as i32));
                    self.stdin_pipe.close();
                }
            }
        }
        if self.stdin_write.is_some() {
            return Ok(());
        }
        if let Some(error) = self.stdin_error.take() {
            capture.io_errors.push(error);
            return Ok(());
        }
        if self.stdin_offset >= self.stdin_packet.len() {
            self.stdin_pipe.close();
            return Ok(());
        }

        let Some(handle) = self.stdin_pipe.handle() else {
            return Ok(());
        };
        let end = (self.stdin_offset + 4096).min(self.stdin_packet.len());
        let bytes = self.stdin_packet[self.stdin_offset..end]
            .to_vec()
            .into_boxed_slice();
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        let event = match unsafe { owned_handle(event) } {
            Ok(event) => event,
            Err(ContainmentError::Win32 { code, .. }) => {
                capture
                    .io_errors
                    .push(ChildIoError::WriteStdin(code as i32));
                self.stdin_pipe.close();
                return Ok(());
            }
            Err(ContainmentError::InvalidInvocation(_)) | Err(ContainmentError::Cleanup { .. }) => {
                capture.io_errors.push(ChildIoError::WriteStdin(0));
                self.stdin_pipe.close();
                return Ok(());
            }
        };
        let mut pending = Box::new(PendingWrite {
            handle,
            event,
            overlapped: UnsafeCell::new(OVERLAPPED::default()),
            buffer: bytes,
            length: 0,
        });
        pending.length = pending.buffer.len() as u32;
        pending.overlapped.get_mut().hEvent = raw(&pending.event);
        let mut transferred = 0_u32;
        let started = unsafe {
            WriteFile(
                raw(&pending.handle),
                pending.buffer.as_ptr(),
                pending.length,
                &mut transferred,
                pending.overlapped.get(),
            )
        };
        if started != 0 {
            if transferred == pending.length {
                self.stdin_offset += pending.length as usize;
            } else {
                capture.io_errors.push(ChildIoError::WriteStdin(0));
                self.stdin_pipe.close();
            }
            return Ok(());
        }
        let error = last_error();
        if error == ERROR_IO_PENDING {
            self.stdin_write = Some(pending);
        } else {
            capture
                .io_errors
                .push(ChildIoError::WriteStdin(error as i32));
            self.stdin_pipe.close();
        }
        Ok(())
    }

    fn cancel_pending_stdin(&mut self) -> Result<(), &'static str> {
        let Some(pending) = self.stdin_write.take() else {
            self.stdin_pipe.close();
            return Ok(());
        };
        if unsafe { CancelIoEx(raw(&pending.handle), pending.overlapped.get()) } == 0
            && last_error() != ERROR_NOT_FOUND
        {
            self.stdin_write = Some(pending);
            return Err("stdin cancellation request");
        }
        if unsafe { WaitForSingleObject(raw(&pending.event), duration_ms(CLEANUP_TIMEOUT)) }
            != WAIT_OBJECT_0
        {
            self.stdin_write = Some(pending);
            return Err("stdin cancellation wait");
        }
        let mut transferred = 0_u32;
        match reap_pending_write(&pending, &mut transferred) {
            Ok(()) if transferred == pending.length => {
                self.stdin_offset += pending.length as usize;
            }
            Ok(()) => {
                self.stdin_error = Some(ChildIoError::WriteStdin(995));
            }
            Err(code) if code == ERROR_IO_PENDING => {
                self.stdin_write = Some(pending);
                return Err("stdin cancellation");
            }
            Err(code) => {
                self.stdin_error = Some(ChildIoError::WriteStdin(code as i32));
            }
        }
        self.stdin_pipe.close();
        Ok(())
    }

    fn terminate_job(
        &mut self,
        capture: &mut Capture,
        operation: &'static str,
    ) -> Result<(), ContainmentError> {
        if unsafe { TerminateJobObject(raw(self.job.as_ref().expect("job handle")), 1) } == 0 {
            return Err(self.cleanup_error(operation, std::mem::take(capture)));
        }
        self.cancel_pending_stdin()
            .map_err(|stage| self.cleanup_error(stage, std::mem::take(capture)))?;
        if let Some(error) = self.stdin_error.take() {
            capture.io_errors.push(error);
        }
        Ok(())
    }

    fn drain_output<F>(
        &self,
        capture: &mut Capture,
        stop: &StopSignal,
        observer_requested_stop: &mut bool,
        observer: &mut F,
    ) -> Result<(), ContainmentError>
    where
        F: FnMut(ChildStream, &[u8]),
    {
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        loop {
            if Instant::now() >= deadline {
                return Err(self.cleanup_error("output drain", std::mem::take(capture)));
            }
            match self
                .output_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(message) => {
                    let ended = matches!(message, OutputMessage::End);
                    capture.accept(
                        message,
                        self.limits.max_total_output_bytes,
                        stop,
                        observer_requested_stop,
                        observer,
                    );
                    if ended && capture.ends == 2 {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(self.cleanup_error("output drain", std::mem::take(capture)));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    if capture.ends == 2 {
                        return Ok(());
                    }
                    return Err(
                        self.cleanup_error("output channel disconnected", std::mem::take(capture))
                    );
                }
            }
        }
    }

    fn wait_for_job_empty(&self, capture: &mut Capture) -> Result<(), ContainmentError> {
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        loop {
            let active = match job_active_processes(self.job.as_ref().expect("job handle")) {
                Ok(active) => active,
                Err(_code) => {
                    return Err(self.cleanup_error("job query", std::mem::take(capture)));
                }
            };
            if active == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(self.cleanup_error("job descendants", std::mem::take(capture)));
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn join_workers(&mut self, mut output: ChildOutput) -> Result<ChildOutput, ContainmentError> {
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        let mut completed = [false; 2];
        let mut complete_count = 0_usize;
        while complete_count < completed.len() {
            match self
                .worker_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(WorkerDone { index, error }) if !completed[index] => {
                    if let Some(error) = error {
                        output.io_errors.push(error);
                        return Err(ContainmentError::Cleanup {
                            stage: "worker I/O",
                            partial: Some(output),
                        });
                    }
                    completed[index] = true;
                    complete_count += 1;
                }
                Ok(WorkerDone { .. }) => {}
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        if complete_count != completed.len() {
            return Err(ContainmentError::Cleanup {
                stage: "worker join",
                partial: Some(output),
            });
        }
        for worker in [self.stdout_reader.take(), self.stderr_reader.take()]
            .into_iter()
            .flatten()
        {
            if worker.join().is_err() {
                return Err(ContainmentError::Cleanup {
                    stage: "worker join",
                    partial: Some(output),
                });
            }
        }
        Ok(output)
    }

    fn cleanup_error(&self, stage: &'static str, capture: Capture) -> ContainmentError {
        ContainmentError::Cleanup {
            stage,
            partial: Some(ChildOutput {
                exit_code: None,
                stdin_bytes_written: self.stdin_offset,
                stdout: capture.stdout,
                stderr: capture.stderr,
                truncated: capture.truncated,
                io_errors: capture.io_errors,
            }),
        }
    }
}

impl Drop for RunningChild {
    fn drop(&mut self) {
        self.shutdown_workers();
    }
}

impl RunningChild {
    fn shutdown_workers(&mut self) {
        if self.job.is_none() && self.stdout_reader.is_none() && self.stderr_reader.is_none() {
            return;
        }
        self.io_stop.request_stop();
        unsafe {
            if let Some(job) = self.job.as_ref() {
                TerminateJobObject(raw(job), 1);
            }
            WaitForSingleObject(raw(&self.process), duration_ms(CLEANUP_TIMEOUT));
        }
        if self.cancel_pending_stdin().is_err() {
            // A cancellation timeout leaves the boxed OVERLAPPED and its
            // packet buffer owned by the kernel operation.  Leak that complete
            // owner rather than freeing memory still referenced by Windows.
            if let Some(pending) = self.stdin_write.take() {
                let _ = Box::leak(pending);
            }
        }
        // Closing the shared handles is the final cancellation path. Readers
        // poll, and the parent-owned stdin operation has already been
        // canceled or retained as an explicit unresolved cleanup owner.
        self.stdin_pipe.close();
        self.stdout_pipe.close();
        self.stderr_pipe.close();
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        let mut completed = [false; 2];
        let mut complete_count = 0_usize;
        while complete_count < completed.len() {
            match self
                .worker_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(WorkerDone { index, .. }) if !completed[index] => {
                    completed[index] = true;
                    complete_count += 1;
                }
                Ok(WorkerDone { .. }) => {}
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        // A WorkerDone is emitted only after that worker has released its
        // pipe operation and is about to exit, so joining is immediate once
        // both notifications arrive.  Do not unconditionally join after
        // an incomplete notification: that would turn Drop into an unbounded
        // wait. Drop is best effort: on unresolved cleanup, reader handles may
        // detach. Only finish_or_stop can report confirmed worker/Job cleanup.
        if complete_count == completed.len() {
            for worker in [self.stdout_reader.take(), self.stderr_reader.take()]
                .into_iter()
                .flatten()
            {
                let _ = worker.join();
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum OutputStream {
    Stdout,
    Stderr,
}

impl From<OutputStream> for ChildStream {
    fn from(stream: OutputStream) -> Self {
        match stream {
            OutputStream::Stdout => Self::Stdout,
            OutputStream::Stderr => Self::Stderr,
        }
    }
}

enum OutputMessage {
    Data(OutputStream, Vec<u8>),
    ReadFailure(OutputStream, u32),
    End,
}

struct WorkerDone {
    index: usize,
    error: Option<ChildIoError>,
}

#[derive(Default)]
struct Capture {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    bytes: usize,
    truncated: bool,
    limit_reached: bool,
    ends: u8,
    io_errors: Vec<ChildIoError>,
}

impl Capture {
    fn accept<F>(
        &mut self,
        message: OutputMessage,
        limit: usize,
        stop: &StopSignal,
        observer_requested_stop: &mut bool,
        observer: &mut F,
    ) where
        F: FnMut(ChildStream, &[u8]),
    {
        match message {
            OutputMessage::End => self.ends = self.ends.saturating_add(1),
            OutputMessage::ReadFailure(stream, code) => {
                self.io_errors.push(match stream {
                    OutputStream::Stdout => ChildIoError::ReadStdout(code),
                    OutputStream::Stderr => ChildIoError::ReadStderr(code),
                });
            }
            OutputMessage::Data(stream, bytes) => {
                let available = limit.saturating_sub(self.bytes);
                let retained = bytes.len().min(available);
                match stream {
                    OutputStream::Stdout => self.stdout.extend_from_slice(&bytes[..retained]),
                    OutputStream::Stderr => self.stderr.extend_from_slice(&bytes[..retained]),
                }
                self.bytes += retained;
                if retained > 0 && !stop.is_requested() {
                    observer(stream.into(), &bytes[..retained]);
                    if stop.is_requested() {
                        *observer_requested_stop = true;
                    }
                }
                if retained < bytes.len() {
                    self.truncated = true;
                    self.limit_reached = true;
                }
            }
        }
    }
}

fn start_reader(
    pipe: Arc<SharedPipe>,
    stream: OutputStream,
    sender: SyncSender<OutputMessage>,
    done: mpsc::Sender<WorkerDone>,
    stop: StopSignal,
) -> JoinHandle<()> {
    let done_index = match stream {
        OutputStream::Stdout => 0,
        OutputStream::Stderr => 1,
    };
    thread::spawn(move || {
        let mut buffer = [0_u8; OUTPUT_CHUNK_BYTES];
        loop {
            if stop.is_requested() {
                break;
            }
            match pipe_read(&pipe, &mut buffer) {
                PipeRead::Empty => thread::sleep(Duration::from_millis(5)),
                PipeRead::End => break,
                PipeRead::Error(code) => {
                    let _ = send_output(&sender, OutputMessage::ReadFailure(stream, code), &stop);
                    break;
                }
                PipeRead::Data(size) => {
                    if !send_output(
                        &sender,
                        OutputMessage::Data(stream, buffer[..size].to_vec()),
                        &stop,
                    ) {
                        let _ = done.send(WorkerDone {
                            index: done_index,
                            error: None,
                        });
                        return;
                    }
                }
            }
        }
        let _ = send_output(&sender, OutputMessage::End, &stop);
        let _ = done.send(WorkerDone {
            index: done_index,
            error: None,
        });
    })
}

fn send_output(
    sender: &SyncSender<OutputMessage>,
    mut message: OutputMessage,
    stop: &StopSignal,
) -> bool {
    loop {
        match sender.try_send(message) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(returned)) => {
                if stop.is_requested() {
                    return false;
                }
                message = returned;
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

enum PipeRead {
    Empty,
    Data(usize),
    End,
    Error(u32),
}

struct PendingWrite {
    handle: Arc<OwnedHandle>,
    event: OwnedHandle,
    // Windows mutates this asynchronously; Rust accesses it only via raw
    // pointers until the event and GetOverlappedResult confirm completion.
    overlapped: UnsafeCell<OVERLAPPED>,
    buffer: Box<[u8]>,
    length: u32,
}

// The operation is boxed before WriteFile receives its OVERLAPPED pointer and
// remains boxed until completion.  Its event, pipe handle, and input bytes are
// owned by the same allocation, so the raw handle field is safe to move only
// as part of this private, non-concurrent operation state.
unsafe impl Send for PendingWrite {}

fn reap_pending_write(pending: &PendingWrite, transferred: &mut u32) -> Result<(), u32> {
    let deadline = Instant::now() + CLEANUP_TIMEOUT;
    loop {
        if Instant::now() >= deadline {
            return Err(ERROR_IO_PENDING);
        }
        let wait = unsafe {
            WaitForSingleObject(
                raw(&pending.event),
                duration_ms(deadline.saturating_duration_since(Instant::now())),
            )
        };
        if wait == WAIT_TIMEOUT {
            return Err(ERROR_IO_PENDING);
        }
        if wait != WAIT_OBJECT_0 {
            return Err(last_error());
        }
        if unsafe {
            GetOverlappedResult(
                raw(&pending.handle),
                pending.overlapped.get(),
                transferred,
                0,
            )
        } != 0
        {
            return Ok(());
        }
        let error = last_error();
        if error != ERROR_IO_INCOMPLETE {
            return Err(error);
        }
    }
}

struct SharedPipe {
    handle: Mutex<Option<Arc<OwnedHandle>>>,
}

impl SharedPipe {
    fn new(handle: OwnedHandle) -> Self {
        Self {
            handle: Mutex::new(Some(Arc::new(handle))),
        }
    }

    fn close(&self) {
        if let Ok(mut handle) = self.handle.lock() {
            handle.take();
        }
    }

    fn handle(&self) -> Option<Arc<OwnedHandle>> {
        self.handle.lock().ok()?.as_ref().cloned()
    }
}

fn pipe_read(pipe: &SharedPipe, buffer: &mut [u8; OUTPUT_CHUNK_BYTES]) -> PipeRead {
    let Some(handle) = pipe.handle() else {
        return PipeRead::End;
    };
    unsafe {
        let handle = raw(&handle);
        let mut available = 0_u32;
        if PeekNamedPipe(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        ) == 0
        {
            let error = GetLastError();
            return if error == ERROR_BROKEN_PIPE || error == ERROR_NO_DATA {
                PipeRead::End
            } else {
                PipeRead::Error(error)
            };
        }
        if available == 0 {
            return PipeRead::Empty;
        }
        let requested = available.min(buffer.len() as u32);
        let mut read = 0_u32;
        if ReadFile(
            handle,
            buffer.as_mut_ptr(),
            requested,
            &mut read,
            std::ptr::null_mut(),
        ) == 0
        {
            let error = GetLastError();
            return if error == ERROR_BROKEN_PIPE || error == ERROR_NO_DATA {
                PipeRead::End
            } else {
                PipeRead::Error(error)
            };
        }
        if read == 0 {
            PipeRead::Empty
        } else {
            PipeRead::Data(read as usize)
        }
    }
}

struct AttributeList {
    storage: Vec<usize>,
    ptr: windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
    initialized: bool,
}

impl AttributeList {
    fn new(handles: &[HANDLE; 3]) -> Result<Self, ContainmentError> {
        let mut bytes = 0_usize;
        let first =
            unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut bytes) };
        if first != 0 || last_error() != ERROR_INSUFFICIENT_BUFFER {
            return Err(ContainmentError::Win32 {
                operation: "InitializeProcThreadAttributeList(size)",
                code: last_error(),
            });
        }
        let words = bytes.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let ptr = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(ptr, 1, 0, &mut bytes) } == 0 {
            return Err(ContainmentError::Win32 {
                operation: "InitializeProcThreadAttributeList",
                code: last_error(),
            });
        }
        if unsafe {
            UpdateProcThreadAttribute(
                ptr,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(handles),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
        {
            let code = last_error();
            unsafe { DeleteProcThreadAttributeList(ptr) };
            return Err(ContainmentError::Win32 {
                operation: "UpdateProcThreadAttribute(handle-list)",
                code,
            });
        }
        Ok(Self {
            storage,
            ptr,
            initialized: true,
        })
    }

    fn as_mut_ptr(
        &mut self,
    ) -> windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST {
        self.ptr
    }

    unsafe fn delete(&mut self) {
        if self.initialized {
            unsafe { DeleteProcThreadAttributeList(self.ptr) };
            self.initialized = false;
        }
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe { self.delete() };
        let _ = &self.storage;
    }
}

struct EnvironmentBlock {
    values: Option<Vec<u16>>,
}

impl EnvironmentBlock {
    fn new(policy: &EnvironmentPolicy) -> Result<Self, ContainmentError> {
        let values = match policy {
            EnvironmentPolicy::Inherit => None,
            EnvironmentPolicy::Clear => Some(vec![0, 0]),
            EnvironmentPolicy::Explicit(entries) => {
                let mut block = Vec::new();
                for (key, value) in entries {
                    let key = checked_wide(key, "environment key")?;
                    if key.contains(&('=' as u16)) {
                        return Err(ContainmentError::InvalidInvocation(
                            "environment keys must not contain '='".to_owned(),
                        ));
                    }
                    let value = checked_wide(value, "environment value")?;
                    block.extend(key);
                    block.push('=' as u16);
                    block.extend(value);
                    block.push(0);
                }
                if block.is_empty() {
                    block.push(0);
                }
                block.push(0);
                Some(block)
            }
        };
        Ok(Self { values })
    }

    fn as_ptr(&self) -> *const c_void {
        self.values
            .as_ref()
            .map_or(std::ptr::null(), |values| values.as_ptr().cast::<c_void>())
    }
}

fn create_job() -> Result<OwnedHandle, ContainmentError> {
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    let job = unsafe { owned_handle(job)? };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            raw(&job),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        return Err(ContainmentError::Win32 {
            operation: "SetInformationJobObject",
            code: last_error(),
        });
    }
    Ok(job)
}

fn job_active_processes(job: &OwnedHandle) -> Result<u32, u32> {
    let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    let mut returned = 0_u32;
    if unsafe {
        QueryInformationJobObject(
            raw(job),
            JobObjectBasicAccountingInformation,
            (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            &mut returned,
        )
    } == 0
    {
        return Err(last_error());
    }
    Ok(accounting.ActiveProcesses)
}

fn terminate_unassigned_process(
    process: &OwnedHandle,
    stage: &'static str,
) -> Result<(), ContainmentError> {
    if unsafe { TerminateProcess(raw(process), 1) } == 0 {
        return Err(ContainmentError::Cleanup {
            stage,
            partial: None,
        });
    }
    let wait = unsafe { WaitForSingleObject(raw(process), duration_ms(CLEANUP_TIMEOUT)) };
    if wait != WAIT_OBJECT_0 {
        return Err(ContainmentError::Cleanup {
            stage,
            partial: None,
        });
    }
    Ok(())
}

fn terminate_assigned_process(
    job: &OwnedHandle,
    process: &OwnedHandle,
    stage: &'static str,
) -> Result<(), ContainmentError> {
    if unsafe { TerminateJobObject(raw(job), 1) } == 0 {
        return Err(ContainmentError::Cleanup {
            stage,
            partial: None,
        });
    }
    let wait = unsafe { WaitForSingleObject(raw(process), duration_ms(CLEANUP_TIMEOUT)) };
    if wait != WAIT_OBJECT_0 {
        return Err(ContainmentError::Cleanup {
            stage,
            partial: None,
        });
    }
    Ok(())
}

unsafe fn create_pipe(
    read: &mut HANDLE,
    write: &mut HANDLE,
    attributes: *mut SECURITY_ATTRIBUTES,
    operation: &'static str,
) -> Result<(), ContainmentError> {
    if unsafe { CreatePipe(read, write, attributes, 0) } == 0 {
        return Err(ContainmentError::Win32 {
            operation,
            code: unsafe { GetLastError() },
        });
    }
    Ok(())
}

/// Build the stdin direction as a named pipe so the parent write endpoint can
/// use genuinely overlapped I/O.  The child only receives the server read
/// endpoint.  A synchronous anonymous-pipe write can otherwise leave the
/// supervisor stuck behind a writer mutex while cancellation races with the
/// call entering `WriteFile`.
unsafe fn create_stdin_pipe(
    attributes: &SECURITY_ATTRIBUTES,
) -> Result<(HANDLE, HANDLE), ContainmentError> {
    let nonce = uuid::Uuid::new_v4();
    let name = format!(r"\\.\pipe\webnovelstudio-stdin-{}-{nonce}", unsafe {
        GetCurrentProcessId()
    });
    let mut name_wide: Vec<u16> = name.encode_utf16().collect();
    name_wide.push(0);
    let read = unsafe {
        CreateNamedPipeW(
            name_wide.as_ptr(),
            PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            0,
            0,
            attributes,
        )
    };
    if read.is_null() || read == INVALID_HANDLE_VALUE {
        return Err(ContainmentError::Win32 {
            operation: "CreateNamedPipeW(stdin)",
            code: last_error(),
        });
    }
    let write = unsafe {
        CreateFileW(
            name_wide.as_ptr(),
            GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    };
    if write.is_null() || write == INVALID_HANDLE_VALUE {
        let code = last_error();
        unsafe { close_handle(read) };
        return Err(ContainmentError::Win32 {
            operation: "CreateFileW(stdin)",
            code,
        });
    }
    Ok((read, write))
}

unsafe fn set_non_inheritable(
    handle: HANDLE,
    operation: &'static str,
) -> Result<(), ContainmentError> {
    if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(ContainmentError::Win32 {
            operation,
            code: unsafe { GetLastError() },
        });
    }
    Ok(())
}

unsafe fn owned_handle(handle: HANDLE) -> Result<OwnedHandle, ContainmentError> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(ContainmentError::Win32 {
            operation: "handle creation",
            code: unsafe { GetLastError() },
        });
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.cast()) })
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle() as HANDLE
}

fn null_handle() -> HANDLE {
    std::ptr::null_mut()
}

unsafe fn close_handle(handle: HANDLE) {
    if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
        unsafe { CloseHandle(handle) };
    }
}

fn last_error() -> u32 {
    unsafe { GetLastError() }
}

fn exit_code(process: &OwnedHandle) -> Result<u32, ContainmentError> {
    let mut code = 0_u32;
    if unsafe { GetExitCodeProcess(raw(process), &mut code) } == 0 {
        return Err(ContainmentError::Win32 {
            operation: "GetExitCodeProcess",
            code: last_error(),
        });
    }
    Ok(code)
}

fn duration_ms(duration: Duration) -> u32 {
    duration.as_millis().min(u32::MAX as u128) as u32
}

fn expected_write_error(code: i32) -> bool {
    code == 995 || code == ERROR_BROKEN_PIPE as i32 || code == ERROR_NO_DATA as i32
}

fn validate_path(
    label: &'static str,
    path: &Path,
    executable: bool,
) -> Result<(), ContainmentError> {
    if !path.is_absolute() {
        return Err(ContainmentError::InvalidInvocation(format!(
            "{label} must be absolute"
        )));
    }
    if executable
        && !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err(ContainmentError::InvalidInvocation(
            "executable must have an .exe extension".to_owned(),
        ));
    }
    if !executable && !path.is_dir() {
        return Err(ContainmentError::InvalidInvocation(format!(
            "{label} must be an existing directory"
        )));
    }
    Ok(())
}

fn checked_wide(value: &OsStr, label: &str) -> Result<Vec<u16>, ContainmentError> {
    let result: Vec<u16> = value.encode_wide().collect();
    if result.contains(&0) {
        return Err(ContainmentError::InvalidInvocation(format!(
            "{label} must not contain NUL"
        )));
    }
    Ok(result)
}

fn wide_path(path: &Path, label: &'static str) -> Result<Vec<u16>, ContainmentError> {
    let mut result = checked_wide(path.as_os_str(), label)?;
    result.push(0);
    Ok(result)
}

fn command_line(executable: &Path, arguments: &[OsString]) -> Result<Vec<u16>, ContainmentError> {
    let mut values = Vec::with_capacity(arguments.len() + 1);
    values.push(executable.as_os_str().to_os_string());
    values.extend(arguments.iter().cloned());
    let mut result = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            result.push(b' ' as u16);
        }
        let wide = checked_wide(value, "argument")?;
        result.extend(quote_windows_arg(&wide));
    }
    result.push(0);
    Ok(result)
}

fn quote_windows_arg(value: &[u16]) -> Vec<u16> {
    let needs_quotes = value.is_empty()
        || value.iter().any(|character| {
            *character == b' ' as u16 || *character == b'\t' as u16 || *character == b'"' as u16
        });
    if !needs_quotes {
        return value.to_vec();
    }
    let mut result = vec![b'"' as u16];
    let mut backslashes = 0_usize;
    for character in value {
        if *character == b'\\' as u16 {
            backslashes += 1;
        } else if *character == b'"' as u16 {
            result.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
            result.push(b'"' as u16);
            backslashes = 0;
        } else {
            result.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
            result.push(*character);
            backslashes = 0;
        }
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    result.push(b'"' as u16);
    result
}

#[cfg(test)]
mod tests {
    use super::{EnvironmentBlock, EnvironmentPolicy, owned_handle, quote_windows_arg};
    use std::collections::BTreeMap;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

    #[test]
    fn rejects_both_windows_invalid_handle_forms() {
        assert!(unsafe { owned_handle(std::ptr::null_mut()) }.is_err());
        assert!(unsafe { owned_handle(INVALID_HANDLE_VALUE) }.is_err());
    }

    #[test]
    fn quotes_spaces_quotes_and_trailing_backslashes() {
        assert_eq!(
            String::from_utf16(&quote_windows_arg(&[])).expect("valid UTF-16"),
            "\"\""
        );
        assert_eq!(
            String::from_utf16(&quote_windows_arg(
                &"plain".encode_utf16().collect::<Vec<_>>()
            ))
            .expect("valid UTF-16"),
            "plain"
        );
        assert_eq!(
            String::from_utf16(&quote_windows_arg(
                &"a b\\".encode_utf16().collect::<Vec<_>>()
            ))
            .expect("valid UTF-16"),
            "\"a b\\\\\""
        );
        assert_eq!(
            String::from_utf16(&quote_windows_arg(
                &"a\"b".encode_utf16().collect::<Vec<_>>()
            ))
            .expect("valid UTF-16"),
            "\"a\\\"b\""
        );
    }

    #[test]
    fn explicit_empty_environment_is_double_nul_terminated() {
        let block = EnvironmentBlock::new(&EnvironmentPolicy::Explicit(BTreeMap::new()))
            .expect("empty environment");
        assert_eq!(block.values, Some(vec![0, 0]));
    }
}
