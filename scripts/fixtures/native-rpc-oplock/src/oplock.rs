//! Local filesystem coordination only. No product hook or native UI automation.
use std::{
    fs::{File, OpenOptions},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_IO_PENDING, GetLastError, HANDLE, WAIT_OBJECT_0},
    Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_OVERLAPPED},
    System::{
        IO::{CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED},
        Ioctl::{
            FSCTL_REQUEST_OPLOCK, OPLOCK_LEVEL_CACHE_HANDLE, OPLOCK_LEVEL_CACHE_READ,
            REQUEST_OPLOCK_CURRENT_VERSION, REQUEST_OPLOCK_INPUT_BUFFER,
            REQUEST_OPLOCK_INPUT_FLAG_REQUEST, REQUEST_OPLOCK_OUTPUT_BUFFER,
        },
        Threading::{CreateEventW, WaitForSingleObject},
    },
};
pub struct ReadHandleOplock {
    file: File,
    event: HANDLE,
    overlapped: Box<OVERLAPPED>,
    _input: Box<REQUEST_OPLOCK_INPUT_BUFFER>,
    output: Box<REQUEST_OPLOCK_OUTPUT_BUFFER>,
}
impl ReadHandleOplock {
    pub fn new(path: &Path) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OVERLAPPED | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let mut value = Self {
            file,
            event,
            overlapped: Box::new(unsafe { std::mem::zeroed() }),
            _input: Box::new(REQUEST_OPLOCK_INPUT_BUFFER {
                StructureVersion: REQUEST_OPLOCK_CURRENT_VERSION as u16,
                StructureLength: std::mem::size_of::<REQUEST_OPLOCK_INPUT_BUFFER>() as u16,
                RequestedOplockLevel: OPLOCK_LEVEL_CACHE_READ | OPLOCK_LEVEL_CACHE_HANDLE,
                Flags: REQUEST_OPLOCK_INPUT_FLAG_REQUEST,
            }),
            output: Box::new(unsafe { std::mem::zeroed() }),
        };
        value.overlapped.hEvent = event;
        let ok = unsafe {
            DeviceIoControl(
                value.file.as_raw_handle(),
                FSCTL_REQUEST_OPLOCK,
                (&*value._input as *const REQUEST_OPLOCK_INPUT_BUFFER).cast(),
                std::mem::size_of::<REQUEST_OPLOCK_INPUT_BUFFER>() as u32,
                (&mut *value.output as *mut REQUEST_OPLOCK_OUTPUT_BUFFER).cast(),
                std::mem::size_of::<REQUEST_OPLOCK_OUTPUT_BUFFER>() as u32,
                ptr::null_mut(),
                &mut *value.overlapped,
            )
        };
        if ok != 0 || unsafe { GetLastError() } != ERROR_IO_PENDING {
            return Err(std::io::Error::last_os_error());
        }
        Ok(value)
    }
    pub fn broken(&mut self, timeout_ms: u32) -> std::io::Result<Option<(u32, u32, u32)>> {
        if unsafe { WaitForSingleObject(self.event, timeout_ms) } != WAIT_OBJECT_0 {
            return Ok(None);
        }
        let mut returned = 0;
        if unsafe {
            GetOverlappedResult(
                self.file.as_raw_handle(),
                &*self.overlapped,
                &mut returned,
                0,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Some((
            self.output.OriginalOplockLevel,
            self.output.NewOplockLevel,
            self.output.Flags,
        )))
    }
}
impl Drop for ReadHandleOplock {
    fn drop(&mut self) {
        unsafe {
            CancelIoEx(self.file.as_raw_handle(), &*self.overlapped);
            let mut returned = 0;
            GetOverlappedResult(
                self.file.as_raw_handle(),
                &*self.overlapped,
                &mut returned,
                1,
            );
            CloseHandle(self.event);
        }
    }
}
