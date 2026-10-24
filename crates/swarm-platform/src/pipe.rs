//! Local-only Windows named-pipe transport.
//!
//! Callers pass a short Swarm endpoint name (for example `Swarm-<uuid>`); the
//! module owns the Windows namespace and its security boundary.

use crate::PlatformError;

const MAX_ENDPOINT_NAME: usize = 128;

fn validate_name(name: &str) -> Result<(), PlatformError> {
    let bytes = name.as_bytes();
    if bytes.len() > MAX_ENDPOINT_NAME
        || bytes.len() <= "Swarm-".len()
        || !name.starts_with("Swarm-")
        || bytes
            .iter()
            .any(|b| !b.is_ascii_alphanumeric() && *b != b'-' && *b != b'_')
    {
        return Err(PlatformError::InvalidEndpoint);
    }
    Ok(())
}

#[cfg(windows)]
mod imp {
    use super::{PlatformError, validate_name};
    use std::ffi::c_void;
    use std::io::{Read, Write};
    use std::mem::size_of;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::ptr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use std::time::Instant;
    use std::{io, thread};
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::IO::CancelIoEx;
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PIPE_NOWAIT,
        PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
        PIPE_WAIT, SetNamedPipeHandleState, WaitNamedPipeW,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const GENERIC_READ_WRITE: u32 = 0x8000_0000 | 0x4000_0000;
    const CONNECT_TIMEOUT_MS: u32 = 5_000;
    const IO_TIMEOUT: Duration = Duration::from_secs(5);
    const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
    const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(20);

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    impl OwnedHandle {
        fn into_raw(self) -> HANDLE {
            let raw = self.0;
            std::mem::forget(self);
            raw
        }
    }

    struct LocalBuffer(*mut c_void);

    impl Drop for LocalBuffer {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { LocalFree(self.0) };
            }
        }
    }

    struct TokenSid {
        _storage: Vec<usize>,
        sid: *mut c_void,
    }

    fn token_sid(token: HANDLE) -> Result<TokenSid, PlatformError> {
        let mut needed = 0u32;
        unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut needed) };
        if needed == 0 {
            return Err(PlatformError::Io);
        }
        let mut storage = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                storage.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return Err(PlatformError::Io);
        }
        let user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
        Ok(TokenSid {
            _storage: storage,
            sid: user.User.Sid,
        })
    }

    fn current_user_sid() -> Result<TokenSid, PlatformError> {
        let mut raw = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
            return Err(PlatformError::Io);
        }
        let token = OwnedHandle(raw);
        token_sid(token.0)
    }

    fn security_attributes() -> Result<(LocalBuffer, SECURITY_ATTRIBUTES), PlatformError> {
        let user = current_user_sid()?;
        let mut sid_string = ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(user.sid, &mut sid_string) } == 0 {
            return Err(PlatformError::Io);
        }
        let sid_text = LocalBuffer(sid_string.cast());
        let sid_len = unsafe {
            let mut len = 0;
            while *sid_string.add(len) != 0 {
                len += 1;
            }
            len
        };
        let sid = unsafe { std::slice::from_raw_parts(sid_string, sid_len) };
        let mut sddl: Vec<u16> = "D:P(A;;GA;;;".encode_utf16().collect();
        sddl.extend_from_slice(sid);
        sddl.extend(")".encode_utf16());
        sddl.push(0);
        drop(sid_text);

        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(PlatformError::Io);
        }
        let descriptor = LocalBuffer(descriptor);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        Ok((descriptor, attributes))
    }

    fn full_name(name: &str) -> Result<Vec<u16>, PlatformError> {
        validate_name(name)?;
        Ok(format!(r"\\.\pipe\{name}")
            .encode_utf16()
            .chain(Some(0))
            .collect())
    }

    fn create_instance(name: &[u16], first: bool) -> Result<OwnedHandle, PlatformError> {
        let (descriptor, attributes) = security_attributes()?;
        let mut flags = PIPE_ACCESS_DUPLEX;
        if first {
            flags |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let raw = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                flags,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                &attributes,
            )
        };
        drop(descriptor);
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(PlatformError::Io)
        } else {
            Ok(OwnedHandle(raw))
        }
    }

    pub struct Listener {
        name: Vec<u16>,
        pending: Option<std::fs::File>,
    }

    impl Listener {
        pub fn bind(name: &str) -> Result<Self, PlatformError> {
            let name = full_name(name)?;
            let pending = create_instance(&name, true)?;
            Ok(Self {
                name,
                pending: Some(unsafe { std::fs::File::from_raw_handle(pending.into_raw()) }),
            })
        }

        pub fn accept(&mut self) -> Result<Connection, PlatformError> {
            let connected = self.pending.take().ok_or(PlatformError::Io)?;
            let result =
                unsafe { ConnectNamedPipe(connected.as_raw_handle() as HANDLE, ptr::null_mut()) };
            if result == 0 {
                let error = unsafe { GetLastError() };
                // A client may connect and close before accept begins. Hand its
                // disconnected handle to the reader and keep the listener alive.
                if !matches!(error, ERROR_PIPE_CONNECTED | ERROR_NO_DATA) {
                    self.pending = Some(connected);
                    return Err(PlatformError::Io);
                }
            }
            let connected_at = Instant::now();

            // Keep the connected first instance alive while opening its successor.
            // This prevents a gap in which another process could claim the name.
            match create_instance(&self.name, false) {
                Ok(next) => {
                    self.pending = Some(unsafe { std::fs::File::from_raw_handle(next.into_raw()) })
                }
                Err(error) => return Err(error),
            }
            Ok(Connection {
                file: connected,
                io_timeout: IO_TIMEOUT,
                deadline: connected_at + TOTAL_TIMEOUT,
            })
        }

        /// Accepts a connection while polling the listener for shutdown.
        ///
        /// Cancellation releases the pending named-pipe instance and leaves
        /// this Listener closed. Accepted Connections are restored to blocking
        /// wait mode before they are returned, so their I/O deadlines retain
        /// the same behavior as `accept`.
        pub fn accept_until(
            &mut self,
            stopping: &AtomicBool,
        ) -> Result<Option<Connection>, PlatformError> {
            if stopping.load(Ordering::Acquire) {
                self.pending.take();
                return Ok(None);
            }
            let ready = {
                let pending = self.pending.as_ref().ok_or(PlatformError::Io)?;
                match connect_until(pending, stopping) {
                    Ok(ready) => ready,
                    Err(error) => {
                        self.pending.take();
                        return Err(error);
                    }
                }
            };
            if !ready || stopping.load(Ordering::Acquire) {
                self.pending.take();
                return Ok(None);
            }

            let connected = self.pending.take().ok_or(PlatformError::Io)?;
            let connected_at = Instant::now();
            // Keep the connected first instance alive while opening its successor.
            // This prevents a gap in which another process could claim the name.
            match create_instance(&self.name, false) {
                Ok(next) => {
                    self.pending = Some(unsafe { std::fs::File::from_raw_handle(next.into_raw()) })
                }
                Err(error) => return Err(error),
            }
            if stopping.load(Ordering::Acquire) {
                self.pending.take();
                drop(connected);
                return Ok(None);
            }
            Ok(Some(Connection {
                file: connected,
                io_timeout: IO_TIMEOUT,
                deadline: connected_at + TOTAL_TIMEOUT,
            }))
        }
    }

    fn connect_until(pipe: &std::fs::File, stopping: &AtomicBool) -> Result<bool, PlatformError> {
        set_pipe_wait_mode(pipe, PIPE_NOWAIT)?;
        let mut armed = false;
        let result = loop {
            if stopping.load(Ordering::Acquire) {
                break Ok(false);
            }
            let connected =
                unsafe { ConnectNamedPipe(pipe.as_raw_handle() as HANDLE, ptr::null_mut()) };
            if connected != 0 {
                // The first successful nonblocking call only arms a new pipe
                // instance for listening. It does not mean a client connected.
                if armed {
                    break Err(PlatformError::Io);
                }
                armed = true;
                thread::sleep(ACCEPT_POLL_INTERVAL);
                continue;
            }
            match unsafe { GetLastError() } {
                windows_sys::Win32::Foundation::ERROR_PIPE_LISTENING => {
                    thread::sleep(ACCEPT_POLL_INTERVAL);
                }
                ERROR_PIPE_CONNECTED | ERROR_NO_DATA => break Ok(true),
                _ => break Err(PlatformError::Io),
            }
        };
        let restore = set_pipe_wait_mode(pipe, PIPE_WAIT);
        match (result, restore) {
            (_, Err(error)) => Err(error),
            (result, Ok(())) => result,
        }
    }

    fn set_pipe_wait_mode(pipe: &std::fs::File, mode: u32) -> Result<(), PlatformError> {
        if unsafe {
            SetNamedPipeHandleState(
                pipe.as_raw_handle() as HANDLE,
                &mode,
                ptr::null(),
                ptr::null(),
            )
        } == 0
        {
            Err(PlatformError::Io)
        } else {
            Ok(())
        }
    }

    pub struct Connection {
        file: std::fs::File,
        io_timeout: Duration,
        deadline: Instant,
    }

    impl Connection {
        /// Set both the per-call I/O limit and this connection's total lifetime.
        pub fn set_timeout(&mut self, timeout: Duration) {
            self.io_timeout = timeout;
            self.set_deadline(timeout);
        }

        /// Set the maximum lifetime of this connection, including time between I/O calls.
        pub fn set_deadline(&mut self, timeout: Duration) {
            self.deadline = Instant::now() + timeout;
        }

        /// Set the maximum duration of each blocking read, write, or flush call.
        pub fn set_io_timeout(&mut self, timeout: Duration) {
            self.io_timeout = timeout;
        }
    }

    impl Read for Connection {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            io_with_timeout(&self.file, self.io_timeout, self.deadline, |file| {
                let mut file = file;
                file.read(buf)
            })
        }
    }

    impl Write for Connection {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            io_with_timeout(&self.file, self.io_timeout, self.deadline, |file| {
                let mut file = file;
                file.write(buf)
            })
        }

        fn flush(&mut self) -> std::io::Result<()> {
            io_with_timeout(&self.file, self.io_timeout, self.deadline, |file| {
                let mut file = file;
                file.flush()
            })
        }
    }

    fn io_with_timeout<T>(
        file: &std::fs::File,
        io_timeout: Duration,
        deadline: Instant,
        operation: impl FnOnce(&std::fs::File) -> io::Result<T>,
    ) -> io::Result<T> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "named pipe connection deadline expired",
            ));
        }
        let timeout = io_timeout.min(remaining);
        if timeout.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "named pipe I/O timed out",
            ));
        }
        let cancellation_handle = file.try_clone()?;
        let timed_out = Arc::new(AtomicBool::new(false));
        let timer_fired = Arc::clone(&timed_out);
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let watchdog = thread::Builder::new()
            .name("swarm-pipe-io-timeout".into())
            .spawn(move || {
                if matches!(
                    finished_rx.recv_timeout(timeout),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ) {
                    timer_fired.store(true, Ordering::Release);
                    // Retry until completion: a timeout may race with the caller
                    // entering the blocking operation after the first cancellation.
                    loop {
                        unsafe {
                            CancelIoEx(cancellation_handle.as_raw_handle() as HANDLE, ptr::null());
                        }
                        if !matches!(
                            finished_rx.recv_timeout(Duration::from_millis(20)),
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                        ) {
                            break;
                        }
                    }
                }
            })?;

        let result = operation(file);
        let _ = finished_tx.send(());
        let _ = watchdog.join();
        if timed_out.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "named pipe I/O timed out",
            ))
        } else {
            result
        }
    }

    pub fn connect(name: &str) -> Result<Connection, PlatformError> {
        let pipe_name = full_name(name)?;
        let deadline = Instant::now() + Duration::from_millis(u64::from(CONNECT_TIMEOUT_MS));
        let raw = loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(PlatformError::Io);
            }
            let wait_ms = remaining.as_millis().max(1).min(u128::from(u32::MAX)) as u32;
            if unsafe { WaitNamedPipeW(pipe_name.as_ptr(), wait_ms) } == 0 {
                return Err(PlatformError::Io);
            }
            let raw = unsafe {
                CreateFileW(
                    pipe_name.as_ptr(),
                    GENERIC_READ_WRITE,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    0,
                    ptr::null_mut(),
                )
            };
            if !raw.is_null() && raw != INVALID_HANDLE_VALUE {
                break raw;
            }
            // WaitNamedPipe observes availability; it does not reserve the
            // instance. A competing client may open it before CreateFile.
            // Retry only this pre-send race, within the original total budget.
            if unsafe { GetLastError() } != ERROR_PIPE_BUSY {
                return Err(PlatformError::Io);
            }
        };
        let connected_at = Instant::now();
        let connection = Connection {
            file: unsafe { std::fs::File::from_raw_handle(raw) },
            io_timeout: IO_TIMEOUT,
            deadline: connected_at + TOTAL_TIMEOUT,
        };
        verify_server_user(&connection)?;
        Ok(connection)
    }

    fn verify_server_user(connection: &Connection) -> Result<(), PlatformError> {
        let mut pid = 0;
        if unsafe {
            GetNamedPipeServerProcessId(connection.file.as_raw_handle() as HANDLE, &mut pid)
        } == 0
            || pid == 0
        {
            return Err(PlatformError::Protection);
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return Err(PlatformError::Protection);
        }
        let process = OwnedHandle(process);
        let mut raw_token = ptr::null_mut();
        if unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut raw_token) } == 0 {
            return Err(PlatformError::Protection);
        }
        let token = OwnedHandle(raw_token);
        let server_sid = token_sid(token.0).map_err(|_| PlatformError::Protection)?;
        let current_sid = current_user_sid().map_err(|_| PlatformError::Protection)?;
        if unsafe { windows_sys::Win32::Security::EqualSid(server_sid.sid, current_sid.sid) } == 0 {
            return Err(PlatformError::Protection);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{PlatformError, validate_name};
    use std::io::{self, Read, Write};
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    pub struct Listener;
    pub struct Connection;

    impl Connection {
        pub fn set_timeout(&mut self, _timeout: Duration) {}
        pub fn set_io_timeout(&mut self, _timeout: Duration) {}
        pub fn set_deadline(&mut self, _timeout: Duration) {}
    }

    impl Read for Connection {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "named pipes require Windows",
            ))
        }
    }

    impl Write for Connection {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "named pipes require Windows",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "named pipes require Windows",
            ))
        }
    }

    impl Listener {
        pub fn bind(name: &str) -> Result<Self, PlatformError> {
            validate_name(name)?;
            Err(PlatformError::Unsupported)
        }

        pub fn accept(&mut self) -> Result<Connection, PlatformError> {
            Err(PlatformError::Unsupported)
        }

        pub fn accept_until(
            &mut self,
            _stopping: &AtomicBool,
        ) -> Result<Option<Connection>, PlatformError> {
            Err(PlatformError::Unsupported)
        }
    }

    pub fn connect(name: &str) -> Result<Connection, PlatformError> {
        validate_name(name)?;
        Err(PlatformError::Unsupported)
    }
}

pub use imp::{Connection, Listener, connect};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_names_outside_the_swarm_endpoint_namespace() {
        for name in ["", "other-pipe", "Swarm-../outside", "Swarm-nul\0x"] {
            assert!(matches!(
                validate_name(name),
                Err(PlatformError::InvalidEndpoint)
            ));
        }
        assert!(validate_name("Swarm-0123456789abcdef").is_ok());
        assert!(validate_name(&format!("Swarm-{}", "a".repeat(123))).is_err());
    }
}
