use super::*;

pub(crate) struct SharedPipe {
    pub(crate) handle: Mutex<Option<Arc<OwnedHandle>>>,
}

impl SharedPipe {
    pub(crate) fn new(handle: OwnedHandle) -> Self {
        Self {
            handle: Mutex::new(Some(Arc::new(handle))),
        }
    }

    pub(crate) fn close(&self) {
        if let Ok(mut handle) = self.handle.lock() {
            handle.take();
        }
    }

    pub(crate) fn handle(&self) -> Option<Arc<OwnedHandle>> {
        self.handle.lock().ok()?.as_ref().cloned()
    }
}

pub(crate) fn pipe_read(pipe: &SharedPipe, buffer: &mut [u8; OUTPUT_CHUNK_BYTES]) -> PipeRead {
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

pub(crate) struct AttributeList {
    pub(crate) storage: Vec<usize>,
    pub(crate) ptr: windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
    pub(crate) initialized: bool,
}

impl AttributeList {
    pub(crate) fn new(handles: &[HANDLE; 3]) -> Result<Self, ContainmentError> {
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

    pub(crate) fn as_mut_ptr(
        &mut self,
    ) -> windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST {
        self.ptr
    }

    pub(crate) unsafe fn delete(&mut self) {
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
