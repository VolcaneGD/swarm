//! Detached local process launching without inherited standard handles.

use crate::PlatformError;
use std::ffi::OsString;
use std::path::Path;

#[cfg(windows)]
mod imp {
    use super::{OsString, Path, PlatformError};
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CreateProcessW, GetExitCodeProcess, PROCESS_INFORMATION, STARTUPINFOW,
    };

    const MAX_COMMAND_LINE_UNITS: usize = 32_767;

    /// A process handle that never terminates its child when dropped.
    pub struct BackgroundProcess {
        process: HANDLE,
    }

    impl BackgroundProcess {
        /// Returns whether the child process has exited, without waiting.
        pub fn has_exited(&self) -> Result<bool, PlatformError> {
            let mut exit_code = 0u32;
            if unsafe { GetExitCodeProcess(self.process, &mut exit_code) } == 0 {
                return Err(PlatformError::Io);
            }
            Ok(exit_code != STILL_ACTIVE as u32)
        }
    }

    impl Drop for BackgroundProcess {
        fn drop(&mut self) {
            if !self.process.is_null() {
                unsafe { CloseHandle(self.process) };
                self.process = ptr::null_mut();
            }
        }
    }

    /// Starts an executable in the background with no shell or inherited handles.
    ///
    /// The environment is inherited, but stdin/stdout/stderr are not inherited
    /// and are not assigned to a console. Dropping the returned handle only
    /// closes the observation handle; it does not terminate the process.
    pub fn spawn_background(
        executable: &Path,
        args: &[OsString],
    ) -> Result<BackgroundProcess, PlatformError> {
        let executable_units = encode_argument(executable.as_os_str().encode_wide())?;
        let mut application = executable_units.clone();
        application.push(0);
        let mut command_line = quote_windows_argument(&executable_units);
        if command_line.len() + 1 > MAX_COMMAND_LINE_UNITS {
            return Err(PlatformError::TooLarge);
        }
        for argument in args {
            let argument = encode_argument(argument.encode_wide())?;
            let quoted = quote_windows_argument(&argument);
            if command_line.len() + 1 + quoted.len() + 1 > MAX_COMMAND_LINE_UNITS {
                return Err(PlatformError::TooLarge);
            }
            command_line.push(b' ' as u16);
            command_line.extend(quoted);
        }
        if command_line.len() + 1 > MAX_COMMAND_LINE_UNITS {
            return Err(PlatformError::TooLarge);
        }
        command_line.push(0);

        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            ..STARTUPINFOW::default()
        };
        let mut process_info = PROCESS_INFORMATION::default();
        let created = unsafe {
            CreateProcessW(
                application.as_mut_ptr(),
                command_line.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                CREATE_NO_WINDOW,
                ptr::null::<c_void>(),
                ptr::null(),
                &startup,
                &mut process_info,
            )
        };
        if created == 0 {
            return Err(PlatformError::Io);
        }
        unsafe { CloseHandle(process_info.hThread) };
        if process_info.hProcess.is_null() {
            return Err(PlatformError::Io);
        }
        Ok(BackgroundProcess {
            process: process_info.hProcess,
        })
    }

    fn encode_argument(units: impl Iterator<Item = u16>) -> Result<Vec<u16>, PlatformError> {
        let encoded: Vec<_> = units.take(MAX_COMMAND_LINE_UNITS + 1).collect();
        if encoded.len() > MAX_COMMAND_LINE_UNITS {
            return Err(PlatformError::TooLarge);
        }
        if encoded.contains(&0) {
            return Err(PlatformError::InvalidData);
        }
        Ok(encoded)
    }

    fn quote_windows_argument(argument: &[u16]) -> Vec<u16> {
        let needs_quotes = argument.is_empty()
            || argument
                .iter()
                .any(|unit| *unit == 32 || matches!(*unit, 9 | 10 | 13 | 34));
        if !needs_quotes {
            return argument.to_vec();
        }

        let mut quoted = Vec::with_capacity(argument.len() + 2);
        quoted.push(b'\"' as u16);
        let mut backslashes = 0usize;
        for unit in argument.iter().copied() {
            if unit == b'\\' as u16 {
                backslashes += 1;
                continue;
            }
            if unit == b'\"' as u16 {
                quoted.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
                quoted.push(b'\"' as u16);
            } else {
                quoted.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
                quoted.push(unit);
            }
            backslashes = 0;
        }
        quoted.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
        quoted.push(b'\"' as u16);
        quoted
    }

    #[cfg(test)]
    mod tests {
        use super::{encode_argument, quote_windows_argument};

        fn wide(text: &str) -> Vec<u16> {
            text.encode_utf16().collect()
        }

        fn text(units: &[u16]) -> String {
            String::from_utf16(units).unwrap()
        }

        #[test]
        fn quotes_empty_whitespace_quotes_and_trailing_backslashes_for_windows_argv() {
            assert_eq!(text(&quote_windows_argument(&wide("plain"))), "plain");
            assert_eq!(text(&quote_windows_argument(&[])), "\"\"");
            assert_eq!(
                text(&quote_windows_argument(&wide("path with spaces\\"))),
                "\"path with spaces\\\\\""
            );
            assert_eq!(
                text(&quote_windows_argument(&wide("a\\\"b"))),
                "\"a\\\\\\\"b\""
            );
            assert_eq!(
                text(&quote_windows_argument(&wide("日本語 spaced"))),
                "\"日本語 spaced\""
            );
            let invalid_unicode = [0xd800, 32, 0xdfff];
            let quoted = quote_windows_argument(&invalid_unicode);
            assert_eq!(&quoted[1..quoted.len() - 1], &invalid_unicode);
        }

        #[test]
        fn rejects_embedded_nul_in_the_executable_name() {
            assert!(encode_argument([b'a' as u16, 0, b'b' as u16].into_iter()).is_err());
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{OsString, Path, PlatformError};

    pub struct BackgroundProcess;

    impl BackgroundProcess {
        pub fn has_exited(&self) -> Result<bool, PlatformError> {
            Err(PlatformError::Unsupported)
        }
    }

    pub fn spawn_background(
        _executable: &Path,
        _args: &[OsString],
    ) -> Result<BackgroundProcess, PlatformError> {
        Err(PlatformError::Unsupported)
    }
}

pub use imp::{BackgroundProcess, spawn_background};
