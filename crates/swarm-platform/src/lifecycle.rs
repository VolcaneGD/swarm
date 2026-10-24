//! Platform identity for process lifecycle coordination.

use crate::PlatformError;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn compose_boot_id(boot_entry: Uuid, boot_sequence: u32) -> Result<String, PlatformError> {
    if boot_entry.is_nil() {
        return Err(PlatformError::InvalidData);
    }
    let mut digest = Sha256::new();
    digest.update(boot_entry.as_bytes());
    digest.update(boot_sequence.to_le_bytes());
    let hash = digest.finalize();
    let mut bytes: [u8; 16] = hash[..16]
        .try_into()
        .map_err(|_| PlatformError::InvalidData)?;
    // UUID version 8 identifies this as a locally defined, hash-derived UUID.
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let identifier = Uuid::from_bytes(bytes);
    if identifier.is_nil() {
        return Err(PlatformError::InvalidData);
    }
    Ok(identifier.to_string())
}

#[cfg(windows)]
mod imp {
    use crate::PlatformError;
    use std::ffi::c_void;
    use uuid::Uuid;

    const SYSTEM_BOOT_ENVIRONMENT_INFORMATION: u32 = 90;
    const BOOT_INFORMATION_BUFFER_SIZE: u32 = 32;
    const PROCESS_TELEMETRY_ID_INFORMATION: u32 = 64;
    const PROCESS_TELEMETRY_BUFFER_SIZE: usize = 64 * 1024;

    struct TelemetryBuffer(Vec<u8>);

    impl Drop for TelemetryBuffer {
        fn drop(&mut self) {
            for byte in &mut self.0 {
                // The documented record includes SID, image path and command
                // line data after the fixed header; clear it before releasing.
                unsafe { std::ptr::write_volatile(byte, 0) };
            }
            std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
        }
    }

    type NtQuerySystemInformation =
        unsafe extern "system" fn(u32, *mut c_void, u32, *mut u32) -> i32;
    type NtQueryInformationProcess =
        unsafe extern "system" fn(*mut c_void, u32, *mut c_void, u32, *mut u32) -> i32;

    #[repr(C)]
    struct ProcessTelemetryIdInformation {
        header_size: u32,
        process_id: u32,
        process_start_key: u64,
        create_time: u64,
        create_interrupt_time: u64,
        create_unbiased_interrupt_time: u64,
        process_sequence_number: u64,
        session_create_time: u64,
        session_id: u32,
        boot_id: u32,
        image_checksum: u32,
        image_time_date_stamp: u32,
        user_sid_offset: u32,
        image_path_offset: u32,
        package_name_offset: u32,
        relative_app_name_offset: u32,
        command_line_offset: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, proc_name: *const u8) -> *mut c_void;
        fn GetCurrentProcess() -> *mut c_void;
        fn GetCurrentProcessId() -> u32;
    }

    fn ntdll() -> Result<*mut c_void, PlatformError> {
        let ntdll: [u16; 10] = [
            b'n' as u16,
            b't' as u16,
            b'd' as u16,
            b'l' as u16,
            b'l' as u16,
            b'.' as u16,
            b'd' as u16,
            b'l' as u16,
            b'l' as u16,
            0,
        ];
        let module = unsafe { GetModuleHandleW(ntdll.as_ptr()) };
        if module.is_null() {
            return Err(PlatformError::Unsupported);
        }
        Ok(module)
    }

    fn boot_entry_id() -> Result<Uuid, PlatformError> {
        let module = ntdll()?;
        let query =
            unsafe { GetProcAddress(module, c"NtQuerySystemInformation".as_ptr().cast::<u8>()) };
        if query.is_null() {
            return Err(PlatformError::Unsupported);
        }
        // NtQuerySystemInformation is deliberately resolved at runtime because
        // Microsoft does not guarantee a stable import-library contract.
        let query: NtQuerySystemInformation = unsafe { std::mem::transmute(query) };

        let mut output = [0u8; BOOT_INFORMATION_BUFFER_SIZE as usize];
        let mut returned = 0u32;
        let status = unsafe {
            query(
                SYSTEM_BOOT_ENVIRONMENT_INFORMATION,
                output.as_mut_ptr().cast(),
                BOOT_INFORMATION_BUFFER_SIZE,
                &mut returned,
            )
        };
        if status < 0 {
            return Err(PlatformError::Io);
        }
        // Windows' boot-environment structure has a GUID first, followed by
        // firmware type and (on newer systems) boot flags. Accept only the
        // observed v1/v2 structure sizes and fail closed on ABI drift.
        if !matches!(returned, 24 | 32) {
            return Err(PlatformError::InvalidData);
        }
        let bytes: [u8; 16] = output[..16]
            .try_into()
            .map_err(|_| PlatformError::InvalidData)?;
        let identifier = Uuid::from_bytes_le(bytes);
        if identifier.is_nil() {
            return Err(PlatformError::InvalidData);
        }
        Ok(identifier)
    }

    fn boot_sequence() -> Result<u32, PlatformError> {
        let module = ntdll()?;
        let query =
            unsafe { GetProcAddress(module, c"NtQueryInformationProcess".as_ptr().cast::<u8>()) };
        if query.is_null() {
            return Err(PlatformError::Unsupported);
        }
        // Microsoft documents this class (64), the function signature, and the
        // complete telemetry structure. Keep the dynamic lookup and require the
        // exact documented structure size so ABI drift fails closed.
        let query: NtQueryInformationProcess = unsafe { std::mem::transmute(query) };
        let mut output = TelemetryBuffer(vec![0u8; PROCESS_TELEMETRY_BUFFER_SIZE]);
        let mut returned = 0u32;
        let size = std::mem::size_of::<ProcessTelemetryIdInformation>();
        let status = unsafe {
            query(
                GetCurrentProcess(),
                PROCESS_TELEMETRY_ID_INFORMATION,
                output.0.as_mut_ptr().cast(),
                u32::try_from(output.0.len()).map_err(|_| PlatformError::InvalidData)?,
                &mut returned,
            )
        };
        if status < 0 {
            return Err(PlatformError::Io);
        }
        if returned as usize > output.0.len() || (returned as usize) < size || size != 96 {
            return Err(PlatformError::InvalidData);
        }
        let information = unsafe {
            output
                .0
                .as_ptr()
                .cast::<ProcessTelemetryIdInformation>()
                .read_unaligned()
        };
        let boot_id_end = std::mem::offset_of!(ProcessTelemetryIdInformation, boot_id)
            + std::mem::size_of::<u32>();
        if information.header_size as usize != size
            || boot_id_end > information.header_size as usize
            || information.process_id != unsafe { GetCurrentProcessId() }
        {
            return Err(PlatformError::InvalidData);
        }
        Ok(information.boot_id)
    }

    pub fn boot_id() -> Result<String, PlatformError> {
        super::compose_boot_id(boot_entry_id()?, boot_sequence()?)
    }
}

/// Returns a stable UUID for the current Windows boot.
///
/// The identity combines the active boot-entry GUID with the kernel's current
/// process telemetry boot sequence. It does not use wall-clock time, session
/// identity, or caller-provided values. Native API or ABI failures are closed.
#[cfg(windows)]
pub fn boot_id() -> Result<String, PlatformError> {
    imp::boot_id()
}

/// Boot identity is only available on Windows.
#[cfg(not(windows))]
pub fn boot_id() -> Result<String, PlatformError> {
    Err(PlatformError::Unsupported)
}

#[cfg(all(test, windows))]
mod tests {
    use super::{boot_id, compose_boot_id};
    use uuid::Uuid;

    #[test]
    fn boot_sequence_changes_the_uuid_and_repeated_identity_is_stable() {
        let entry = Uuid::parse_str("7aad7f64-c59c-11f1-8956-a18364463bc7").unwrap();
        let first = compose_boot_id(entry, 123).unwrap();
        assert_eq!(compose_boot_id(entry, 123).unwrap(), first);
        assert_ne!(compose_boot_id(entry, 124).unwrap(), first);
        assert_eq!(Uuid::parse_str(&first).unwrap().get_version_num(), 8);
    }

    #[test]
    fn invalid_boot_identity_inputs_fail_closed() {
        assert!(compose_boot_id(Uuid::nil(), 123).is_err());
        let entry = Uuid::parse_str("7aad7f64-c59c-11f1-8956-a18364463bc7").unwrap();
        // Windows numbers its initial boot session zero; it is a valid value.
        assert_ne!(
            compose_boot_id(entry, 0).unwrap(),
            compose_boot_id(entry, 1).unwrap()
        );
    }

    #[test]
    fn boot_identifier_is_a_stable_non_nil_guid_for_the_current_boot() {
        let first = boot_id().expect("Windows boot identifier query should succeed");
        let second = boot_id().expect("repeated query should succeed");
        assert_eq!(first, second);
        assert!(!Uuid::parse_str(&first).unwrap().is_nil());
    }

    #[test]
    #[ignore = "invoked only by the cross-process boot identity test"]
    fn boot_identifier_child_report() {
        println!("SWARM_BOOT_ID={}", boot_id().expect("child boot query"));
    }

    #[test]
    fn independent_process_observes_the_same_kernel_boot_identity() {
        use std::os::windows::process::CommandExt;
        let expected = boot_id().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .creation_flags(0x0800_0000)
            .args([
                "--exact",
                "lifecycle::tests::boot_identifier_child_report",
                "--ignored",
                "--nocapture",
            ])
            .output()
            .expect("spawn isolated boot query child");
        assert!(result.status.success());
        let output = String::from_utf8(result.stdout).unwrap();
        let actual = output
            .lines()
            .find_map(|line| line.strip_prefix("SWARM_BOOT_ID="))
            .expect("child boot report");
        assert_eq!(actual, expected);
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::boot_id;
    use crate::PlatformError;

    #[test]
    fn boot_identifier_is_unsupported_off_windows() {
        assert!(matches!(boot_id(), Err(PlatformError::Unsupported)));
    }
}
