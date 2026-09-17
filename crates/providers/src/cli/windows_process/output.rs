use super::*;

#[derive(Debug, Clone, Copy)]

pub(crate) enum OutputStream {
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

pub(crate) enum OutputMessage {
    Data(OutputStream, Vec<u8>),
    ReadFailure(OutputStream, u32),
    End,
}

pub(crate) struct WorkerDone {
    pub(crate) index: usize,
    pub(crate) error: Option<ChildIoError>,
}

#[derive(Default)]
pub(crate) struct Capture {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) bytes: usize,
    pub(crate) total_bytes: usize,
    pub(crate) truncated: bool,
    pub(crate) limit_reached: bool,
    pub(crate) ends: u8,
    pub(crate) io_errors: Vec<ChildIoError>,
}

impl Capture {
    pub(crate) fn accept<F>(
        &mut self,
        message: OutputMessage,
        limit: usize,
        stop: &StopSignal,
        observer_requested_stop: &mut bool,
        observer: &mut F,
    ) -> InteractiveAction
    where
        F: FnMut(ChildStream, &[u8]) -> InteractiveAction,
    {
        match message {
            OutputMessage::End => {
                self.ends = self.ends.saturating_add(1);
                InteractiveAction::KeepOpen
            }
            OutputMessage::ReadFailure(stream, code) => {
                self.io_errors.push(match stream {
                    OutputStream::Stdout => ChildIoError::ReadStdout(code),
                    OutputStream::Stderr => ChildIoError::ReadStderr(code),
                });
                InteractiveAction::KeepOpen
            }
            OutputMessage::Data(stream, bytes) => {
                self.total_bytes = self.total_bytes.saturating_add(bytes.len());
                let available = limit.saturating_sub(self.bytes);
                let retained = bytes.len().min(available);
                match stream {
                    OutputStream::Stdout => self.stdout.extend_from_slice(&bytes[..retained]),
                    OutputStream::Stderr => self.stderr.extend_from_slice(&bytes[..retained]),
                }
                self.bytes += retained;
                let mut action = InteractiveAction::KeepOpen;
                if retained > 0 && !stop.is_requested() {
                    action = observer(stream.into(), &bytes[..retained]);
                    if stop.is_requested() {
                        *observer_requested_stop = true;
                    }
                }
                if retained < bytes.len() {
                    self.truncated = true;
                    self.limit_reached = true;
                }
                action
            }
        }
    }

    pub(crate) fn accept_persistent<F>(
        &mut self,
        message: OutputMessage,
        stop: &StopSignal,
        observer_requested_stop: &mut bool,
        observer: &mut F,
    ) -> InteractiveAction
    where
        F: FnMut(PersistentEvent<'_>) -> InteractiveAction,
    {
        match message {
            OutputMessage::End => {
                self.ends = self.ends.saturating_add(1);
                InteractiveAction::KeepOpen
            }
            OutputMessage::ReadFailure(stream, code) => {
                self.io_errors.push(match stream {
                    OutputStream::Stdout => ChildIoError::ReadStdout(code),
                    OutputStream::Stderr => ChildIoError::ReadStderr(code),
                });
                InteractiveAction::KeepOpen
            }
            OutputMessage::Data(stream, bytes) => {
                self.total_bytes = self.total_bytes.saturating_add(bytes.len());
                let available = MAX_PERSISTENT_DIAGNOSTIC_BYTES.saturating_sub(self.bytes);
                let retained = bytes.len().min(available);
                match stream {
                    OutputStream::Stdout => self.stdout.extend_from_slice(&bytes[..retained]),
                    OutputStream::Stderr => self.stderr.extend_from_slice(&bytes[..retained]),
                }
                self.bytes += retained;
                if retained < bytes.len() {
                    self.truncated = true;
                }
                let mut action = InteractiveAction::KeepOpen;
                if !bytes.is_empty() && !stop.is_requested() {
                    action = observer(PersistentEvent::Output(stream.into(), &bytes));
                    if stop.is_requested() {
                        *observer_requested_stop = true;
                    }
                }
                action
            }
        }
    }
}

pub(crate) fn start_reader(
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

pub(crate) fn send_output(
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

pub(crate) enum PipeRead {
    Empty,
    Data(usize),
    End,
    Error(u32),
}

pub(crate) struct PendingWrite {
    pub(crate) handle: Arc<OwnedHandle>,
    pub(crate) event: OwnedHandle,
    // Windows mutates this asynchronously; Rust accesses it only via raw
    // pointers until the event and GetOverlappedResult confirm completion.
    pub(crate) overlapped: UnsafeCell<OVERLAPPED>,
    pub(crate) buffer: Box<[u8]>,
    pub(crate) length: u32,
}

// The operation is boxed before WriteFile receives its OVERLAPPED pointer and
// remains boxed until completion.  Its event, pipe handle, and input bytes are
// owned by the same allocation, so the raw handle field is safe to move only
// as part of this private, non-concurrent operation state.
unsafe impl Send for PendingWrite {}

pub(crate) fn reap_pending_write(pending: &PendingWrite, transferred: &mut u32) -> Result<(), u32> {
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
