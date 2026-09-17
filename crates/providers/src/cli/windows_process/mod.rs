//! Windows CLI process containment and bounded I/O.
//!
//! This module is not a shell runner and is not a provider adapter.  It owns
//! only the mechanics needed by a future qualified adapter: an absolute
//! executable, a fixed argument vector, a bounded stdin packet, explicit
//! environment policy, a restricted inherited-handle list, and a Job Object
//! that contains the complete child tree.
//!
//! # Layout
//!
//! The module was one file; it is now split by responsibility:
//! `types` holds the public vocabulary and `RunningChild` state, `spawn` the
//! three spawn entry points, `child` the `RunningChild` impl and `Drop`,
//! `output` the pipe-worker capture internals, `pipes` the shared-pipe and
//! attribute-list wrappers, `env` the environment block and Job Object
//! helpers, and `ffi` the remaining Win32 call wrappers.
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
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER,
    ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_NOT_FOUND, GENERIC_WRITE,
    GetLastError, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    OPEN_EXISTING, PIPE_ACCESS_INBOUND, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_BASIC_PROCESS_ID_LIST,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, CreatePipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateEventW, CreateProcessW,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcessId,
    GetExitCodeProcess, InitializeProcThreadAttributeList, OpenProcess,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, PROCESS_SYNCHRONIZE, ResumeThread,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject,
};

mod child;
mod env;
mod ffi;
mod output;
mod pipes;
mod spawn;
#[cfg(test)]
mod tests;
mod types;

pub use spawn::*;
pub use types::*;

pub(crate) use env::*;
pub(crate) use ffi::*;
pub(crate) use output::*;
pub(crate) use pipes::*;
