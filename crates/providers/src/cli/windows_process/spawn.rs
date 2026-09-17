use super::*;

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
        cleanup_processes: None,
        stdin_packet: invocation.packet,
        stdin_offset: 0,
        stdin_total_written: 0,
        stdin_write: None,
        stdin_error: None,
        interactive: false,
        persistent: false,
        close_stdin_when_drained: false,
        process_id: process_information.dwProcessId,
        limits,
        started_at: Instant::now(),
    })
}

/// Create a job-contained child whose stdin remains open after the initial
/// packet. Additional bounded packets may be appended by
/// [`RunningChild::finish_interactive`]. The ordinary [`spawn`] path retains
/// its existing write-and-close behavior.
pub fn spawn_interactive(invocation: CliInvocation) -> Result<RunningChild, ContainmentError> {
    let mut child = spawn(invocation)?;
    child.interactive = true;
    Ok(child)
}

/// Create a job-contained child intended to stay alive for multiple bounded
/// request/response exchanges.  The process remains owned by the caller and
/// must be consumed with [`RunningChild::finish_persistent`] or dropped.
pub fn spawn_persistent(invocation: CliInvocation) -> Result<RunningChild, ContainmentError> {
    let mut child = spawn(invocation)?;
    child.interactive = true;
    child.persistent = true;
    Ok(child)
}
