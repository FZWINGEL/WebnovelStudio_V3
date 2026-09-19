use super::*;

pub(crate) struct EnvironmentBlock {
    pub(crate) values: Option<Vec<u16>>,
}

impl EnvironmentBlock {
    pub(crate) fn new(policy: &EnvironmentPolicy) -> Result<Self, ContainmentError> {
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

    pub(crate) fn as_ptr(&self) -> *const c_void {
        self.values
            .as_ref()
            .map_or(std::ptr::null(), |values| values.as_ptr().cast::<c_void>())
    }
}

pub(crate) fn create_job() -> Result<OwnedHandle, ContainmentError> {
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

pub(crate) fn job_active_processes(job: &OwnedHandle) -> Result<u32, u32> {
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

/// Capture synchronization handles for every process currently returned by a
/// complete Job PID snapshot.  Cleanup takes one snapshot before termination
/// and another immediately after it to cover members that appeared during the
/// termination race.  Job containment remains authoritative for the whole
/// tree; these handles only provide bounded evidence that observed members'
/// process objects have become signaled.
pub(crate) fn job_process_handles(job: &OwnedHandle) -> Result<Vec<OwnedHandle>, u32> {
    let mut storage = vec![0_usize; 2 + MAX_JOB_PROCESSES];
    let mut returned = 0_u32;
    if unsafe {
        QueryInformationJobObject(
            raw(job),
            JobObjectBasicProcessIdList,
            storage.as_mut_ptr().cast(),
            (storage.len() * size_of::<usize>()) as u32,
            &mut returned,
        )
    } == 0
    {
        return Err(last_error());
    }
    let list = unsafe { &*storage.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() };
    let assigned =
        usize::try_from(list.NumberOfAssignedProcesses).map_err(|_| ERROR_INVALID_PARAMETER)?;
    let count =
        usize::try_from(list.NumberOfProcessIdsInList).map_err(|_| ERROR_INVALID_PARAMETER)?;
    if assigned > MAX_JOB_PROCESSES || count > MAX_JOB_PROCESSES || count != assigned {
        return Err(ERROR_INVALID_PARAMETER);
    }
    let ids = unsafe { std::slice::from_raw_parts(list.ProcessIdList.as_ptr(), count) };
    let mut handles = Vec::with_capacity(count);
    for process_id in ids {
        let process_id = u32::try_from(*process_id).map_err(|_| ERROR_INVALID_PARAMETER)?;
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, process_id) };
        if process.is_null() || process == INVALID_HANDLE_VALUE {
            let code = last_error();
            // A process may have exited between the Job query and OpenProcess;
            // in that case it is already settled and needs no retained handle.
            if code == ERROR_INVALID_PARAMETER {
                continue;
            }
            return Err(code);
        }
        handles.push(unsafe { OwnedHandle::from_raw_handle(process.cast()) });
    }
    Ok(handles)
}

pub(crate) fn terminate_unassigned_process(
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

pub(crate) fn terminate_assigned_process(
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
