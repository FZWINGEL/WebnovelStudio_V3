use super::*;

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
        self,
        stop: StopSignal,
        mut observer: F,
    ) -> Result<ChildOutcome, ContainmentError>
    where
        F: FnMut(ChildStream, &[u8]),
    {
        self.finish_or_stop_with_action(stop, |stream, bytes| {
            observer(stream, bytes);
            InteractiveAction::KeepOpen
        })
    }

    /// Finish an interactive child while allowing the bounded observer to
    /// append request packets or close stdin after the current write drains.
    /// The observer runs on this caller's thread and must remain short and
    /// nonblocking.
    pub fn finish_interactive<F>(
        self,
        stop: StopSignal,
        observer: F,
    ) -> Result<ChildOutcome, ContainmentError>
    where
        F: FnMut(ChildStream, &[u8]) -> InteractiveAction,
    {
        if !self.interactive {
            return Err(ContainmentError::InvalidInvocation(
                "interactive finish requires spawn_interactive".to_owned(),
            ));
        }
        self.finish_or_stop_with_action(stop, observer)
    }

    /// Drive a long-lived interactive child until it exits, is stopped, or is
    /// explicitly closed by the observer.  The callback is called at most
    /// every [`POLL_INTERVAL`] while idle and for every bounded output chunk;
    /// returning `Send` queues one bounded packet, while `Close` closes stdin
    /// after queued bytes drain and then confirms Job cleanup.
    ///
    /// Unlike the one-shot and ordinary interactive finish paths, persistent
    /// output is not subject to `max_total_output_bytes`.  Every chunk reaches
    /// the observer, while only a bounded diagnostic prefix is retained in the
    /// returned `ChildOutput`.
    pub fn finish_persistent<F>(
        mut self,
        stop: StopSignal,
        mut observer: F,
    ) -> Result<ChildOutcome, ContainmentError>
    where
        F: FnMut(PersistentEvent<'_>) -> InteractiveAction,
    {
        if !self.interactive {
            return Err(ContainmentError::InvalidInvocation(
                "persistent finish requires spawn_persistent or spawn_interactive".to_owned(),
            ));
        }
        // Accepting an interactive child here keeps the additive API useful
        // for callers that already construct one with spawn_interactive, while
        // spawn_persistent marks the intent earlier for queue compaction.
        self.persistent = true;

        let mut capture = Capture::default();
        let mut terminal = None;
        let mut termination_sent = false;
        let mut stop_deadline = None;
        let mut close_deadline = None;
        let mut observer_requested_stop = false;
        let mut last_tick = Instant::now() - POLL_INTERVAL;

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

            if self.close_stdin_when_drained
                && self.stdin_write.is_none()
                && self.stdin_offset >= self.stdin_packet.len()
                && self.stdin_pipe.handle().is_none()
            {
                close_deadline.get_or_insert(Instant::now() + self.limits.stop_grace);
            }

            let deadline = stop_deadline.or(close_deadline);
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    terminal.get_or_insert(ChildTermination::Stopped);
                    self.terminate_job(&mut capture, "TerminateJobObject(persistent-stop)")?;
                    termination_sent = true;
                    break;
                }
            } else if self.started_at.elapsed() >= self.limits.overall {
                terminal = Some(ChildTermination::TimedOut);
                self.terminate_job(&mut capture, "TerminateJobObject(persistent-timeout)")?;
                termination_sent = true;
                break;
            }

            if !stop.is_requested()
                && !self.close_stdin_when_drained
                && last_tick.elapsed() >= POLL_INTERVAL
            {
                last_tick = Instant::now();
                let action = observer(PersistentEvent::Tick);
                self.apply_interactive_action(action)?;
                if stop.is_requested() {
                    observer_requested_stop = true;
                }
            }

            let mut wait = if self.close_stdin_when_drained {
                POLL_INTERVAL
            } else {
                POLL_INTERVAL.saturating_sub(last_tick.elapsed())
            };
            if let Some(deadline) = deadline {
                wait = wait.min(deadline.saturating_duration_since(Instant::now()));
            }
            wait = wait.min(
                self.limits
                    .overall
                    .saturating_sub(self.started_at.elapsed()),
            );
            match self
                .output_rx
                .recv_timeout(wait.max(Duration::from_millis(1)))
            {
                Ok(message) => {
                    let action = capture.accept_persistent(
                        message,
                        &stop,
                        &mut observer_requested_stop,
                        &mut observer,
                    );
                    self.apply_interactive_action(action)?;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    if unsafe {
                        WaitForSingleObject(raw(&self.process), duration_ms(CLEANUP_TIMEOUT))
                    } == WAIT_OBJECT_0
                    {
                        terminal.get_or_insert(ChildTermination::Completed);
                        break;
                    }
                    return Err(
                        self.cleanup_error("persistent output channel disconnected", capture)
                    );
                }
            }
        }

        if !termination_sent
            && matches!(
                terminal,
                Some(ChildTermination::Completed | ChildTermination::Stopped)
            )
        {
            self.terminate_job(&mut capture, "TerminateJobObject(persistent-completion)")?;
            termination_sent = true;
        }
        if termination_sent {
            let wait =
                unsafe { WaitForSingleObject(raw(&self.process), duration_ms(CLEANUP_TIMEOUT)) };
            if wait != WAIT_OBJECT_0 {
                return Err(self.cleanup_error("persistent process wait", capture));
            }
            self.wait_for_job_empty(&mut capture)?;
        }
        self.drain_persistent(
            &mut capture,
            &stop,
            &mut observer_requested_stop,
            &mut observer,
        )?;
        debug_assert!(capture.total_bytes >= capture.bytes);
        if observer_requested_stop && terminal == Some(ChildTermination::Completed) {
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
                            stdin_bytes_written: self.stdin_total_written,
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
            stdin_bytes_written: self.stdin_total_written,
            stdout: capture.stdout,
            stderr: capture.stderr,
            truncated: capture.truncated,
            io_errors: capture.io_errors,
        };
        let output = self.join_workers(output)?;
        if terminal == Some(ChildTermination::Completed)
            && exit_code == 0
            && (self.stdin_write.is_some() || self.stdin_offset < self.stdin_packet.len())
        {
            return Err(ContainmentError::Cleanup {
                stage: "incomplete persistent stdin delivery",
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
                stage: "persistent reader I/O",
                partial: Some(output),
            });
        }
        if output.io_errors.iter().any(
            |error| matches!(error, ChildIoError::WriteStdin(code) if !expected_write_error(*code)),
        ) {
            return Err(ContainmentError::Cleanup {
                stage: "persistent stdin I/O",
                partial: Some(output),
            });
        }
        self.job.take();
        Ok(ChildOutcome {
            termination: terminal.unwrap_or(ChildTermination::Completed),
            output,
        })
    }

    pub(crate) fn finish_or_stop_with_action<F>(
        mut self,
        stop: StopSignal,
        mut observer: F,
    ) -> Result<ChildOutcome, ContainmentError>
    where
        F: FnMut(ChildStream, &[u8]) -> InteractiveAction,
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
                Ok(message) => {
                    let action = capture.accept(
                        message,
                        self.limits.max_total_output_bytes,
                        &stop,
                        &mut observer_requested_stop,
                        &mut observer,
                    );
                    self.apply_interactive_action(action)?;
                }
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
                            stdin_bytes_written: self.stdin_total_written,
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
            stdin_bytes_written: self.stdin_total_written,
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

    pub(crate) fn next_wait(&self, stop_deadline: Option<Instant>) -> Duration {
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

    pub(crate) fn apply_interactive_action(
        &mut self,
        action: InteractiveAction,
    ) -> Result<(), ContainmentError> {
        if !self.interactive {
            if action != InteractiveAction::KeepOpen {
                return Err(ContainmentError::InvalidInvocation(
                    "interactive input was returned for a finite child".to_owned(),
                ));
            }
            return Ok(());
        }
        match action {
            InteractiveAction::KeepOpen => {}
            InteractiveAction::Send(bytes) => {
                if self.close_stdin_when_drained {
                    return Err(ContainmentError::InvalidInvocation(
                        "interactive stdin was already closed".to_owned(),
                    ));
                }
                self.compact_stdin_queue();
                if self.persistent {
                    if bytes.len() > MAX_PACKET_BYTES {
                        return Err(ContainmentError::InvalidInvocation(format!(
                            "persistent stdin packet exceeds {MAX_PACKET_BYTES} bytes"
                        )));
                    }
                    let pending = self.stdin_packet.len().saturating_sub(self.stdin_offset);
                    if pending.saturating_add(bytes.len()) > MAX_PERSISTENT_PENDING_BYTES {
                        return Err(ContainmentError::InvalidInvocation(format!(
                            "persistent stdin queue exceeds {MAX_PERSISTENT_PENDING_BYTES} bytes"
                        )));
                    }
                } else if self.stdin_packet.len().saturating_add(bytes.len()) > MAX_PACKET_BYTES {
                    return Err(ContainmentError::InvalidInvocation(format!(
                        "interactive stdin exceeds {MAX_PACKET_BYTES} bytes"
                    )));
                }
                self.stdin_packet.extend(bytes);
            }
            InteractiveAction::Close => {
                self.close_stdin_when_drained = true;
                if self.stdin_write.is_none() && self.stdin_offset >= self.stdin_packet.len() {
                    self.stdin_pipe.close();
                }
            }
        }
        Ok(())
    }

    pub(crate) fn compact_stdin_queue(&mut self) {
        if !self.persistent || self.stdin_offset == 0 {
            return;
        }
        // Keep the queue index cheap for a long-lived process.  A one-shot or
        // ordinary interactive child keeps its historical cumulative packet
        // accounting and never enters this path.
        if self.stdin_offset == self.stdin_packet.len() || self.stdin_offset >= 64 * 1024 {
            self.stdin_packet.drain(..self.stdin_offset);
            self.stdin_offset = 0;
        }
    }

    pub(crate) fn progress_stdin(&mut self, capture: &mut Capture) -> Result<(), ContainmentError> {
        if let Some(pending) = self.stdin_write.as_ref() {
            if unsafe { WaitForSingleObject(raw(&pending.event), 0) } != WAIT_OBJECT_0 {
                return Ok(());
            }
            let mut transferred = 0_u32;
            let pending = self.stdin_write.take().expect("pending stdin write");
            match reap_pending_write(&pending, &mut transferred) {
                Ok(()) if transferred == pending.length => {
                    self.stdin_offset += pending.length as usize;
                    self.stdin_total_written += pending.length as usize;
                    self.compact_stdin_queue();
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
            if !self.interactive || self.close_stdin_when_drained {
                self.stdin_pipe.close();
            }
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
                self.stdin_total_written += pending.length as usize;
                self.compact_stdin_queue();
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

    pub(crate) fn cancel_pending_stdin(&mut self) -> Result<(), &'static str> {
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
                self.stdin_total_written += pending.length as usize;
                self.compact_stdin_queue();
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

    pub(crate) fn terminate_job(
        &mut self,
        capture: &mut Capture,
        operation: &'static str,
    ) -> Result<(), ContainmentError> {
        if self.cleanup_processes.is_none() {
            let handles = match job_process_handles(self.job.as_ref().expect("job handle")) {
                Ok(handles) => handles,
                Err(_code) => {
                    return Err(self.cleanup_error("job process list", std::mem::take(capture)));
                }
            };
            self.cleanup_processes = Some(handles);
        }
        if unsafe { TerminateJobObject(raw(self.job.as_ref().expect("job handle")), 1) } == 0 {
            return Err(self.cleanup_error(operation, std::mem::take(capture)));
        }
        let after_termination = match job_process_handles(self.job.as_ref().expect("job handle")) {
            Ok(handles) => handles,
            Err(_code) => {
                return Err(self.cleanup_error(
                    "job process list after termination",
                    std::mem::take(capture),
                ));
            }
        };
        if let Some(handles) = self.cleanup_processes.as_mut() {
            handles.extend(after_termination);
        }
        self.cancel_pending_stdin()
            .map_err(|stage| self.cleanup_error(stage, std::mem::take(capture)))?;
        if let Some(error) = self.stdin_error.take() {
            capture.io_errors.push(error);
        }
        Ok(())
    }

    pub(crate) fn drain_output<F>(
        &mut self,
        capture: &mut Capture,
        stop: &StopSignal,
        observer_requested_stop: &mut bool,
        observer: &mut F,
    ) -> Result<(), ContainmentError>
    where
        F: FnMut(ChildStream, &[u8]) -> InteractiveAction,
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
                    let action = capture.accept(
                        message,
                        self.limits.max_total_output_bytes,
                        stop,
                        observer_requested_stop,
                        observer,
                    );
                    self.apply_interactive_action(action)?;
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

    pub(crate) fn drain_persistent<F>(
        &mut self,
        capture: &mut Capture,
        stop: &StopSignal,
        observer_requested_stop: &mut bool,
        observer: &mut F,
    ) -> Result<(), ContainmentError>
    where
        F: FnMut(PersistentEvent<'_>) -> InteractiveAction,
    {
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        loop {
            if Instant::now() >= deadline {
                return Err(self.cleanup_error("persistent output drain", std::mem::take(capture)));
            }
            match self
                .output_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(message) => {
                    let ended = matches!(message, OutputMessage::End);
                    let action =
                        capture.accept_persistent(message, stop, observer_requested_stop, observer);
                    self.apply_interactive_action(action)?;
                    if ended && capture.ends == 2 {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(
                        self.cleanup_error("persistent output drain", std::mem::take(capture))
                    );
                }
                Err(RecvTimeoutError::Disconnected) => {
                    if capture.ends == 2 {
                        return Ok(());
                    }
                    return Err(self.cleanup_error(
                        "persistent output channel disconnected",
                        std::mem::take(capture),
                    ));
                }
            }
        }
    }

    pub(crate) fn wait_for_job_empty(&self, capture: &mut Capture) -> Result<(), ContainmentError> {
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        if let Some(handles) = &self.cleanup_processes {
            for handle in handles {
                let wait = unsafe {
                    WaitForSingleObject(
                        raw(handle),
                        duration_ms(deadline.saturating_duration_since(Instant::now())),
                    )
                };
                if wait != WAIT_OBJECT_0 {
                    return Err(self.cleanup_error("job process handles", std::mem::take(capture)));
                }
            }
        }
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

    pub(crate) fn join_workers(
        &mut self,
        mut output: ChildOutput,
    ) -> Result<ChildOutput, ContainmentError> {
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

    pub(crate) fn cleanup_error(&self, stage: &'static str, capture: Capture) -> ContainmentError {
        ContainmentError::Cleanup {
            stage,
            partial: Some(ChildOutput {
                exit_code: None,
                stdin_bytes_written: self.stdin_total_written,
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
    pub(crate) fn shutdown_workers(&mut self) {
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
