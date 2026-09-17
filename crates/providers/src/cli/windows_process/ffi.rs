use super::*;

pub(crate) unsafe fn create_pipe(
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
pub(crate) unsafe fn create_stdin_pipe(
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

pub(crate) unsafe fn set_non_inheritable(
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

pub(crate) unsafe fn owned_handle(handle: HANDLE) -> Result<OwnedHandle, ContainmentError> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(ContainmentError::Win32 {
            operation: "handle creation",
            code: unsafe { GetLastError() },
        });
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.cast()) })
}

pub(crate) fn raw(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle() as HANDLE
}

pub(crate) fn null_handle() -> HANDLE {
    std::ptr::null_mut()
}

pub(crate) unsafe fn close_handle(handle: HANDLE) {
    if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
        unsafe { CloseHandle(handle) };
    }
}

pub(crate) fn last_error() -> u32 {
    unsafe { GetLastError() }
}

pub(crate) fn exit_code(process: &OwnedHandle) -> Result<u32, ContainmentError> {
    let mut code = 0_u32;
    if unsafe { GetExitCodeProcess(raw(process), &mut code) } == 0 {
        return Err(ContainmentError::Win32 {
            operation: "GetExitCodeProcess",
            code: last_error(),
        });
    }
    Ok(code)
}

pub(crate) fn duration_ms(duration: Duration) -> u32 {
    duration.as_millis().min(u32::MAX as u128) as u32
}

pub(crate) fn expected_write_error(code: i32) -> bool {
    code == 995 || code == ERROR_BROKEN_PIPE as i32 || code == ERROR_NO_DATA as i32
}

pub(crate) fn validate_path(
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

pub(crate) fn checked_wide(value: &OsStr, label: &str) -> Result<Vec<u16>, ContainmentError> {
    let result: Vec<u16> = value.encode_wide().collect();
    if result.contains(&0) {
        return Err(ContainmentError::InvalidInvocation(format!(
            "{label} must not contain NUL"
        )));
    }
    Ok(result)
}

pub(crate) fn wide_path(path: &Path, label: &'static str) -> Result<Vec<u16>, ContainmentError> {
    let mut result = checked_wide(path.as_os_str(), label)?;
    result.push(0);
    Ok(result)
}

pub(crate) fn command_line(
    executable: &Path,
    arguments: &[OsString],
) -> Result<Vec<u16>, ContainmentError> {
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

pub(crate) fn quote_windows_arg(value: &[u16]) -> Vec<u16> {
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
