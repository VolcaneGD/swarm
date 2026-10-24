//! Per-root registration in the current user's Windows logon startup list.
//!
//! This adapter only manages Swarm's uniquely named value in the shared Run
//! key. It never removes the key or changes another application's values.

use crate::PlatformError;

#[cfg(windows)]
mod imp {
    use super::PlatformError;
    use sha2::{Digest, Sha256};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use windows_sys::Win32::Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_PATH_NOT_FOUND,
    };
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const MAX_VALUE_BYTES: u32 = 64 * 1024;
    const MAX_RUN_COMMAND_UNITS: usize = 260;

    pub struct Registration {
        value_name: Vec<u16>,
        command: Vec<u16>,
        legacy_command: Option<Vec<u16>>,
        subkey: Vec<u16>,
    }

    impl std::fmt::Debug for Registration {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Registration { executable: [path], root: [path], value_name: [redacted] }")
        }
    }

    impl Registration {
        pub fn new(executable: &Path, root: &Path) -> Result<Self, PlatformError> {
            let executable = canonical_absolute(executable)?;
            let root = canonical_absolute(root)?;
            if !executable.is_file() || !root.is_dir() {
                return Err(PlatformError::InvalidRoot);
            }
            let value_name = value_name(&root)?;
            // Run dispatch does not reliably execute verbatim paths. Retain the
            // canonical root for ownership, but verify that shell paths still
            // resolve to the same objects before registering their command.
            for path in [&executable, &root] {
                if canonical_absolute(&shell_path(path)?)? != *path {
                    return Err(PlatformError::InvalidRoot);
                }
            }
            let command = command_line(&executable, &root)?;
            let legacy_command = raw_command_line(&executable, &root)
                .ok()
                .filter(|legacy| *legacy != command);
            Ok(Self {
                value_name: terminated_units(&value_name),
                command,
                legacy_command,
                subkey: terminated(OsStr::new(RUN_KEY)),
            })
        }

        /// Read whether this root's current startup command is present and exact.
        /// An exact legacy verbatim command is owned but not ready; an explicit
        /// enable upgrades it, or disable removes it. Reads never migrate it.
        /// A value with the same name but different or malformed data is an
        /// error; it is never treated as a Swarm registration.
        pub fn is_registered(&self) -> Result<bool, PlatformError> {
            let Some(key) = open_key(&self.subkey, KEY_QUERY_VALUE)? else {
                return Ok(false);
            };
            let value = query_value(key.0, &self.value_name)?;
            match value {
                None => Ok(false),
                Some(value) if value == self.command => Ok(true),
                Some(value) if self.legacy_command.as_ref() == Some(&value) => Ok(false),
                Some(_) => Err(PlatformError::InvalidData),
            }
        }

        /// Enable or disable this registration without touching other values.
        /// Conflicting values are refused on both paths to avoid takeover or
        /// deletion of a value this instance does not own.
        pub fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
            if enabled {
                if let Some(key) = open_key(&self.subkey, KEY_QUERY_VALUE | KEY_SET_VALUE)? {
                    match query_value(key.0, &self.value_name)? {
                        Some(value) if value == self.command => return Ok(()),
                        Some(value) if self.legacy_command.as_ref() == Some(&value) => {
                            return set_value(key.0, &self.value_name, &self.command);
                        }
                        Some(_) => return Err(PlatformError::InvalidData),
                        None => {
                            return set_value(key.0, &self.value_name, &self.command);
                        }
                    }
                }
                let key = create_key(&self.subkey)?;
                match query_value(key.0, &self.value_name)? {
                    Some(value) if value == self.command => Ok(()),
                    Some(value) if self.legacy_command.as_ref() == Some(&value) => {
                        set_value(key.0, &self.value_name, &self.command)
                    }
                    Some(_) => Err(PlatformError::InvalidData),
                    None => set_value(key.0, &self.value_name, &self.command),
                }
            } else {
                let Some(key) = open_key(&self.subkey, KEY_QUERY_VALUE | KEY_SET_VALUE)? else {
                    return Ok(());
                };
                match query_value(key.0, &self.value_name)? {
                    None => Ok(()),
                    Some(value)
                        if value == self.command
                            || self.legacy_command.as_ref() == Some(&value) =>
                    {
                        let code = unsafe { RegDeleteValueW(key.0, self.value_name.as_ptr()) };
                        if code == 0 || code == ERROR_FILE_NOT_FOUND {
                            Ok(())
                        } else {
                            Err(PlatformError::Io)
                        }
                    }
                    Some(_) => Err(PlatformError::InvalidData),
                }
            }
        }

        #[cfg(test)]
        fn for_test_key(
            executable: &Path,
            root: &Path,
            subkey: &str,
        ) -> Result<Self, PlatformError> {
            let mut registration = Self::new(executable, root)?;
            registration.subkey = terminated(OsStr::new(subkey));
            Ok(registration)
        }

        #[cfg(test)]
        fn test_value_name(&self) -> &[u16] {
            &self.value_name
        }
    }

    fn canonical_absolute(path: &Path) -> Result<PathBuf, PlatformError> {
        if !path.is_absolute() || path.as_os_str().is_empty() {
            return Err(PlatformError::InvalidRoot);
        }
        std::fs::canonicalize(path).map_err(|_| PlatformError::InvalidRoot)
    }

    fn value_name(root: &Path) -> Result<Vec<u16>, PlatformError> {
        let units = root.as_os_str().encode_wide().collect::<Vec<_>>();
        if units.contains(&0) {
            return Err(PlatformError::InvalidRoot);
        }
        let mut bytes = Vec::with_capacity(units.len() * 2);
        for unit in units {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let digest = Sha256::digest(bytes);
        let mut name = String::from("Swarm-");
        for byte in digest {
            use std::fmt::Write as _;
            write!(&mut name, "{byte:02x}").expect("writing to String cannot fail");
        }
        Ok(name.encode_utf16().collect())
    }

    fn command_line(executable: &Path, root: &Path) -> Result<Vec<u16>, PlatformError> {
        raw_command_line(&shell_path(executable)?, &shell_path(root)?)
    }

    fn shell_path(path: &Path) -> Result<PathBuf, PlatformError> {
        let text = path.to_str().ok_or(PlatformError::InvalidRoot)?;
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{unc}")));
        }
        if let Some(disk) = text.strip_prefix(r"\\?\") {
            let bytes = disk.as_bytes();
            if bytes.len() < 3
                || !bytes[0].is_ascii_alphabetic()
                || bytes[1] != b':'
                || bytes[2] != b'\\'
            {
                return Err(PlatformError::InvalidRoot);
            }
            return Ok(PathBuf::from(disk));
        }
        Ok(path.to_path_buf())
    }

    fn raw_command_line(executable: &Path, root: &Path) -> Result<Vec<u16>, PlatformError> {
        let exe = executable.as_os_str().encode_wide().collect::<Vec<_>>();
        let root = root.as_os_str().encode_wide().collect::<Vec<_>>();
        if exe.contains(&0) || root.contains(&0) {
            return Err(PlatformError::InvalidRoot);
        }
        let mut result = quote_argument(&exe);
        result.extend(" --data-dir ".encode_utf16());
        result.extend(quote_argument(&root));
        result.extend(" --windows-startup".encode_utf16());
        String::from_utf16(&result).map_err(|_| PlatformError::InvalidRoot)?;
        if result.len() > MAX_RUN_COMMAND_UNITS {
            return Err(PlatformError::TooLarge);
        }
        result.push(0);
        Ok(result)
    }

    fn quote_argument(argument: &[u16]) -> Vec<u16> {
        let mut quoted = Vec::with_capacity(argument.len() + 2);
        quoted.push(b'"' as u16);
        let mut slashes = 0usize;
        for &unit in argument {
            if unit == b'\\' as u16 {
                slashes += 1;
                continue;
            }
            if unit == b'"' as u16 {
                quoted.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2 + 1));
                quoted.push(unit);
            } else {
                quoted.extend(std::iter::repeat_n(b'\\' as u16, slashes));
                quoted.push(unit);
            }
            slashes = 0;
        }
        quoted.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
        quoted.push(b'"' as u16);
        quoted
    }

    fn terminated(value: &OsStr) -> Vec<u16> {
        let mut units = value.encode_wide().collect::<Vec<_>>();
        units.push(0);
        units
    }

    fn terminated_units(value: &[u16]) -> Vec<u16> {
        let mut units = value.to_vec();
        units.push(0);
        units
    }

    fn open_key(subkey: &[u16], access: u32) -> Result<Option<Key>, PlatformError> {
        let mut raw = std::ptr::null_mut();
        let code =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, access, &mut raw) };
        match code {
            0 => Ok(Some(Key(raw))),
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
            _ => Err(PlatformError::Io),
        }
    }

    fn create_key(subkey: &[u16]) -> Result<Key, PlatformError> {
        let mut raw = std::ptr::null_mut();
        let mut disposition = 0;
        let code = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                std::ptr::null(),
                &mut raw,
                &mut disposition,
            )
        };
        if code == 0 {
            Ok(Key(raw))
        } else {
            Err(PlatformError::Io)
        }
    }

    fn set_value(key: HKEY, name: &[u16], command: &[u16]) -> Result<(), PlatformError> {
        let byte_len = command
            .len()
            .checked_mul(2)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(PlatformError::TooLarge)?;
        let code = unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                byte_len,
            )
        };
        if code == 0 {
            Ok(())
        } else {
            Err(PlatformError::Io)
        }
    }

    fn query_value(key: HKEY, name: &[u16]) -> Result<Option<Vec<u16>>, PlatformError> {
        let mut kind = 0;
        let mut byte_len = 0u32;
        let first = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                std::ptr::null_mut(),
                &mut byte_len,
            )
        };
        if first == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if first != 0 && first != ERROR_MORE_DATA {
            return Err(PlatformError::Io);
        }
        if kind != REG_SZ
            || !(2..=MAX_VALUE_BYTES).contains(&byte_len)
            || !byte_len.is_multiple_of(2)
        {
            return Err(if byte_len > MAX_VALUE_BYTES {
                PlatformError::TooLarge
            } else {
                PlatformError::InvalidData
            });
        }
        let mut bytes = vec![0u8; byte_len as usize];
        let mut actual = byte_len;
        let second = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                bytes.as_mut_ptr(),
                &mut actual,
            )
        };
        if second != 0 || actual != byte_len || kind != REG_SZ {
            return Err(PlatformError::InvalidData);
        }
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        if units.last() != Some(&0) || units[..units.len() - 1].contains(&0) {
            return Err(PlatformError::InvalidData);
        }
        // Reject malformed surrogate sequences rather than accepting a value
        // that cannot represent a command through Rust's UTF-16 string APIs.
        String::from_utf16(&units).map_err(|_| PlatformError::InvalidData)?;
        Ok(Some(units))
    }

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use uuid::Uuid;
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, KEY_SET_VALUE, REG_BINARY, RegCreateKeyExW, RegDeleteKeyW,
            RegDeleteValueW, RegSetValueExW,
        };

        const TEST_KEY_ROOT: &str = r"Software\Swarm\Tests";

        struct IsolatedRegistration {
            registration: Registration,
            key: Vec<u16>,
            _temp: tempfile::TempDir,
        }

        impl IsolatedRegistration {
            fn new() -> Self {
                let temp = tempfile::Builder::new()
                    .prefix(&format!("swarm-startup-{}-", Uuid::new_v4()))
                    .tempdir()
                    .unwrap();
                let executable = std::env::current_exe().unwrap();
                let key_text = format!(r"{TEST_KEY_ROOT}\{}", Uuid::new_v4());
                let registration =
                    Registration::for_test_key(&executable, temp.path(), &key_text).unwrap();
                Self {
                    registration,
                    key: terminated(OsStr::new(&key_text)),
                    _temp: temp,
                }
            }

            fn put_raw(&self, kind: u32, data: &[u8]) {
                self.put_named_raw(self.registration.test_value_name(), kind, data);
            }

            fn put_named_raw(&self, name: &[u16], kind: u32, data: &[u8]) {
                let key = create_test_key(&self.key);
                let code = unsafe {
                    RegSetValueExW(
                        key,
                        name.as_ptr(),
                        0,
                        kind,
                        data.as_ptr(),
                        data.len() as u32,
                    )
                };
                unsafe { RegCloseKey(key) };
                assert_eq!(code, 0);
            }
        }

        impl Drop for IsolatedRegistration {
            fn drop(&mut self) {
                let _ = self.registration.set_enabled(false);
                // The unique leaf test key has no children. Only delete that
                // leaf; never touch the shared Run key or its other values.
                unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, self.key.as_ptr()) };
            }
        }

        fn create_test_key(path: &[u16]) -> HKEY {
            let mut key = std::ptr::null_mut();
            let mut disposition = 0;
            let code = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    std::ptr::null(),
                    0,
                    KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut key,
                    &mut disposition,
                )
            };
            assert_eq!(code, 0);
            key
        }

        #[test]
        fn new_is_idempotent_and_disable_only_removes_owned_value() {
            let fixture = IsolatedRegistration::new();
            let unrelated_name = terminated(OsStr::new("OtherApp"));
            let unrelated_data = "C:\\Other\\app.exe"
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            fixture.put_named_raw(&unrelated_name, REG_SZ, &unrelated_data);
            fixture.registration.set_enabled(false).unwrap();
            assert!(!fixture.registration.is_registered().unwrap());
            fixture.registration.set_enabled(true).unwrap();
            assert!(fixture.registration.is_registered().unwrap());
            assert_eq!(query_named_raw(&fixture, &unrelated_name), unrelated_data);
            fixture.registration.set_enabled(true).unwrap();
            fixture.registration.set_enabled(false).unwrap();
            fixture.registration.set_enabled(false).unwrap();
            assert!(!fixture.registration.is_registered().unwrap());
            assert_eq!(query_named_raw(&fixture, &unrelated_name), unrelated_data);
        }

        #[test]
        fn exact_legacy_command_is_read_only_until_explicit_enable_or_disable() {
            let fixture = IsolatedRegistration::new();
            let legacy = fixture.registration.legacy_command.as_ref().unwrap();
            let bytes = legacy
                .iter()
                .flat_map(|u| u.to_le_bytes())
                .collect::<Vec<_>>();
            let unrelated = terminated(OsStr::new("OtherApp"));
            fixture.put_named_raw(&unrelated, REG_SZ, &bytes);
            fixture.put_raw(REG_SZ, &bytes);
            assert!(!fixture.registration.is_registered().unwrap());
            assert_eq!(query_raw(&fixture), bytes);
            fixture.registration.set_enabled(true).unwrap();
            assert!(fixture.registration.is_registered().unwrap());
            assert_eq!(query_named_raw(&fixture, &unrelated), bytes);
            assert_eq!(
                query_raw(&fixture),
                fixture
                    .registration
                    .command
                    .iter()
                    .flat_map(|u| u.to_le_bytes())
                    .collect::<Vec<_>>()
            );
            fixture.put_raw(REG_SZ, &bytes);
            fixture.registration.set_enabled(false).unwrap();
            assert!(!fixture.registration.is_registered().unwrap());
            assert_eq!(query_named_raw(&fixture, &unrelated), bytes);
        }

        #[test]
        fn near_legacy_command_is_refused_without_replacement_or_removal() {
            let fixture = IsolatedRegistration::new();
            let mut legacy = fixture.registration.legacy_command.clone().unwrap();
            legacy.pop();
            legacy.extend(" --unexpected".encode_utf16());
            legacy.push(0);
            let bytes = legacy
                .iter()
                .flat_map(|u| u.to_le_bytes())
                .collect::<Vec<_>>();
            fixture.put_raw(REG_SZ, &bytes);
            assert_eq!(
                fixture.registration.is_registered(),
                Err(PlatformError::InvalidData)
            );
            assert_eq!(
                fixture.registration.set_enabled(true),
                Err(PlatformError::InvalidData)
            );
            assert_eq!(
                fixture.registration.set_enabled(false),
                Err(PlatformError::InvalidData)
            );
            assert_eq!(query_raw(&fixture), bytes);
        }

        #[test]
        fn constructor_refuses_shell_path_that_resolves_to_a_different_object() {
            let temp = tempfile::tempdir().unwrap();
            let canonical = temp.path().canonicalize().unwrap();
            let ordinary = canonical.join("data");
            let verbatim_only = canonical.join("data.");
            std::fs::create_dir(&ordinary).unwrap();
            std::fs::create_dir(&verbatim_only).unwrap();
            assert!(matches!(
                Registration::new(&std::env::current_exe().unwrap(), &verbatim_only),
                Err(PlatformError::InvalidRoot)
            ));
        }

        #[test]
        fn cleanup_removes_only_the_unique_test_leaf() {
            let fixture = IsolatedRegistration::new();
            let key = fixture.key.clone();
            fixture.registration.set_enabled(true).unwrap();
            drop(fixture);
            assert!(open_key(&key, KEY_QUERY_VALUE).unwrap().is_none());
        }

        #[test]
        fn value_name_is_a_stable_sha256_of_the_canonical_root() {
            let temp = tempfile::tempdir().unwrap();
            let executable = std::env::current_exe().unwrap();
            let first = Registration::new(&executable, temp.path()).unwrap();
            let alias = temp.path().join(".");
            let second = Registration::new(&executable, &alias).unwrap();
            assert_eq!(first.value_name, second.value_name);
            let text = String::from_utf16(&first.value_name[..first.value_name.len() - 1]).unwrap();
            assert!(text.starts_with("Swarm-"));
            assert_eq!(text.len(), "Swarm-".len() + 64);
            assert!(
                text["Swarm-".len()..]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            );
        }

        #[test]
        fn reads_and_disable_do_not_create_the_registry_key() {
            let fixture = IsolatedRegistration::new();
            assert!(!fixture.registration.is_registered().unwrap());
            fixture.registration.set_enabled(false).unwrap();
            assert!(open_key(&fixture.key, KEY_QUERY_VALUE).unwrap().is_none());
        }

        #[test]
        fn existing_mismatch_and_non_string_values_are_refused_without_replacement() {
            let fixture = IsolatedRegistration::new();
            let mut other = "not Swarm".encode_utf16().collect::<Vec<_>>();
            other.push(0);
            let bytes = other
                .iter()
                .flat_map(|u| u.to_le_bytes())
                .collect::<Vec<_>>();
            fixture.put_raw(REG_SZ, &bytes);
            assert_eq!(
                fixture.registration.is_registered(),
                Err(PlatformError::InvalidData)
            );
            assert_eq!(
                fixture.registration.set_enabled(true),
                Err(PlatformError::InvalidData)
            );
            assert_eq!(
                fixture.registration.set_enabled(false),
                Err(PlatformError::InvalidData)
            );
            assert_eq!(query_raw(&fixture), bytes);

            delete_test_value(&fixture);
            fixture.put_raw(REG_BINARY, &[1, 2, 3]);
            assert_eq!(
                fixture.registration.is_registered(),
                Err(PlatformError::InvalidData)
            );
            assert_eq!(
                fixture.registration.set_enabled(true),
                Err(PlatformError::InvalidData)
            );
            assert_eq!(query_raw(&fixture), [1, 2, 3]);
        }

        #[test]
        fn malformed_termination_and_oversized_values_fail_closed() {
            let fixture = IsolatedRegistration::new();
            fixture.put_raw(REG_SZ, &[b'a', 0, b'b', 0]);
            assert_eq!(
                fixture.registration.is_registered(),
                Err(PlatformError::InvalidData)
            );
            delete_test_value(&fixture);
            fixture.put_raw(REG_SZ, &vec![0u8; MAX_VALUE_BYTES as usize + 2]);
            assert_eq!(
                fixture.registration.is_registered(),
                Err(PlatformError::TooLarge)
            );
        }

        #[test]
        fn quoting_handles_spaces_quotes_trailing_slashes_and_non_unicode_units() {
            let argument = [
                b'C' as u16,
                b':' as u16,
                b'\\' as u16,
                b'a' as u16,
                b' ' as u16,
                b'b' as u16,
                b'\\' as u16,
                b'"' as u16,
                b'\\' as u16,
                0xd800,
            ];
            let quoted = quote_argument(&argument);
            assert_eq!(quoted.first(), Some(&(b'"' as u16)));
            assert_eq!(quoted.last(), Some(&(b'"' as u16)));
            assert!(quoted.contains(&0xd800));
            assert_eq!(
                quote_argument(&[b'C' as u16, b':' as u16, b'\\' as u16]),
                vec![
                    b'"' as u16,
                    b'C' as u16,
                    b':' as u16,
                    b'\\' as u16,
                    b'\\' as u16,
                    b'"' as u16
                ]
            );
        }

        #[test]
        fn command_rejects_unpaired_surrogates_before_a_registration_can_be_written() {
            use std::os::windows::ffi::OsStringExt;

            let malformed = PathBuf::from(std::ffi::OsString::from_wide(&[
                b'C' as u16,
                b':' as u16,
                b'\\' as u16,
                b'x' as u16,
                0xd800,
            ]));
            let valid = Path::new(r"C:\data");
            assert_eq!(
                command_line(&malformed, valid),
                Err(PlatformError::InvalidRoot)
            );
            assert_eq!(
                command_line(valid, &malformed),
                Err(PlatformError::InvalidRoot)
            );
        }

        #[test]
        fn command_quotes_paths_and_has_required_flags() {
            let exe = Path::new(r"C:\Program Files\Swarm\swarm-runtime.exe");
            let root = Path::new(r"C:\Data Sets\Swarm");
            let command = command_line(exe, root).unwrap();
            let command = String::from_utf16(&command[..command.len() - 1]).unwrap();
            assert!(command.starts_with(r#""C:\Program Files\Swarm\swarm-runtime.exe" --data-dir "C:\Data Sets\Swarm" --windows-startup"#));
            let japanese = command_line(
                Path::new(r"C:\Users\名前\Swarm.exe"),
                Path::new(r"C:\データ\Swarm"),
            )
            .unwrap();
            let japanese = String::from_utf16(&japanese[..japanese.len() - 1]).unwrap();
            assert!(japanese.contains(r"C:\Users\名前\Swarm.exe"));
            assert!(japanese.contains(r"C:\データ\Swarm"));
        }

        #[test]
        fn run_command_uses_shell_paths_for_canonical_disk_and_unc_inputs() {
            for (exe, root, expected) in [
                (
                    r"\\?\C:\Program Files\Swarm\runtime.exe",
                    r"\\?\D:\Data Sets\Swarm",
                    r#""C:\Program Files\Swarm\runtime.exe" --data-dir "D:\Data Sets\Swarm" --windows-startup"#,
                ),
                (
                    r"\\?\UNC\server\share\runtime.exe",
                    r"\\?\UNC\server\share\Data",
                    r#""\\server\share\runtime.exe" --data-dir "\\server\share\Data" --windows-startup"#,
                ),
            ] {
                let command = command_line(Path::new(exe), Path::new(root)).unwrap();
                assert_eq!(
                    String::from_utf16(&command[..command.len() - 1]).unwrap(),
                    expected
                );
            }
        }

        #[test]
        fn run_command_length_enforces_260_units_excluding_terminator() {
            let exe_text = format!("C:\\{}", "e".repeat(110));
            let root_text = format!("D:\\{}", "r".repeat(110));
            let command = command_line(Path::new(&exe_text), Path::new(&root_text)).unwrap();
            assert_eq!(command.len() - 1, MAX_RUN_COMMAND_UNITS);
            let canonical_command = command_line(
                Path::new(&format!(r"\\?\{exe_text}")),
                Path::new(&format!(r"\\?\{root_text}")),
            )
            .unwrap();
            assert_eq!(canonical_command, command);

            let too_long_root_text = format!("D:\\{}", "r".repeat(111));
            assert_eq!(
                command_line(Path::new(&exe_text), Path::new(&too_long_root_text)),
                Err(PlatformError::TooLarge)
            );
        }

        #[test]
        fn unsupported_verbatim_namespaces_are_not_written_to_run() {
            assert_eq!(
                command_line(
                    Path::new(r"\\?\Volume{123}\runtime.exe"),
                    Path::new(r"C:\Data")
                ),
                Err(PlatformError::InvalidRoot)
            );
        }

        #[test]
        fn constructor_rejects_relative_paths_and_wrong_path_kinds() {
            assert_eq!(
                canonical_absolute(Path::new("relative")),
                Err(PlatformError::InvalidRoot)
            );
            let temp = tempfile::tempdir().unwrap();
            let file = temp.path().join("not-a-directory");
            std::fs::write(&file, b"x").unwrap();
            let executable = std::env::current_exe().unwrap();
            assert!(matches!(
                Registration::new(&executable, &file),
                Err(PlatformError::InvalidRoot)
            ));
            assert!(matches!(
                Registration::new(temp.path(), temp.path()),
                Err(PlatformError::InvalidRoot)
            ));
        }

        fn delete_test_value(fixture: &IsolatedRegistration) {
            unsafe {
                let key = open_test_key(&fixture.key);
                let _ = RegDeleteValueW(key, fixture.registration.test_value_name().as_ptr());
                RegCloseKey(key);
            }
        }

        fn open_test_key(path: &[u16]) -> HKEY {
            let mut key = std::ptr::null_mut();
            let code = unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    KEY_QUERY_VALUE | KEY_SET_VALUE,
                    &mut key,
                )
            };
            assert_eq!(code, 0);
            key
        }

        fn query_raw(fixture: &IsolatedRegistration) -> Vec<u8> {
            query_named_raw(fixture, fixture.registration.test_value_name())
        }

        fn query_named_raw(fixture: &IsolatedRegistration, name: &[u16]) -> Vec<u8> {
            let key = open_test_key(&fixture.key);
            let mut kind = 0;
            let mut size = 0;
            let _ = unsafe {
                RegQueryValueExW(
                    key,
                    name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    std::ptr::null_mut(),
                    &mut size,
                )
            };
            let mut data = vec![0; size as usize];
            let _ = unsafe {
                RegQueryValueExW(
                    key,
                    name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut size,
                )
            };
            unsafe { RegCloseKey(key) };
            data
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::PlatformError;
    use std::path::Path;

    pub struct Registration;

    impl Registration {
        pub fn new(_executable: &Path, _root: &Path) -> Result<Self, PlatformError> {
            Err(PlatformError::Unsupported)
        }

        pub fn is_registered(&self) -> Result<bool, PlatformError> {
            Err(PlatformError::Unsupported)
        }

        pub fn set_enabled(&self, _enabled: bool) -> Result<(), PlatformError> {
            Err(PlatformError::Unsupported)
        }
    }
}

pub use imp::Registration;
