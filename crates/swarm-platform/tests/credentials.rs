#![cfg(windows)]

// These are real DPAPI and Windows ACL checks. This target intentionally has no non-Windows fake.

use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    os::windows::ffi::{OsStrExt, OsStringExt},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use swarm_platform::CredentialStore;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
        GetNamedSecurityInfoW, SE_FILE_OBJECT,
    },
    Security::{
        ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION,
        GetAce, GetAclInformation, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
        GetTokenInformation, OWNER_SECURITY_INFORMATION, SE_DACL_PROTECTED, TOKEN_QUERY,
        TOKEN_USER, TokenUser,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

fn open(root: &Path) -> CredentialStore {
    CredentialStore::open(root).expect("open credential store")
}

fn files_below(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).expect("read credential directory") {
            let entry = entry.expect("read directory entry");
            let kind = entry.file_type().expect("read entry type");
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
    }
    files
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

struct AclSnapshot {
    sddl: String,
    owner_sid: String,
    protected: bool,
    ace_sids: Vec<String>,
    ace_types: Vec<u8>,
    ace_flags: Vec<u8>,
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

unsafe fn sid_string(sid: *mut c_void) -> String {
    let mut value = std::ptr::null_mut();
    assert_ne!(unsafe { ConvertSidToStringSidW(sid, &mut value) }, 0);
    let result = unsafe {
        std::ffi::OsString::from_wide(std::slice::from_raw_parts(value, wide_len(value)))
    }
    .to_string_lossy()
    .into_owned();
    unsafe { LocalFree(value.cast()) };
    result
}

unsafe fn wide_len(value: *const u16) -> usize {
    let mut len = 0;
    while unsafe { *value.add(len) } != 0 {
        len += 1;
    }
    len
}

fn current_user_sid() -> String {
    let mut token: HANDLE = std::ptr::null_mut();
    assert_ne!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) },
        0
    );
    struct TokenGuard(HANDLE);
    impl Drop for TokenGuard {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }
    let _token_guard = TokenGuard(token);
    let mut needed = 0u32;
    unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
    assert!(needed > 0);
    let mut storage = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    assert_ne!(
        unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                storage.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        },
        0
    );
    let token_user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
    unsafe { sid_string(token_user.User.Sid) }
}

fn acl_snapshot(path: &Path) -> AclSnapshot {
    let path = wide(path);
    let mut owner = std::ptr::null_mut();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(status, 0, "GetNamedSecurityInfoW failed with {status}");
    struct DescriptorGuard(*mut c_void);
    impl Drop for DescriptorGuard {
        fn drop(&mut self) {
            unsafe { LocalFree(self.0.cast()) };
        }
    }
    let _descriptor_guard = DescriptorGuard(descriptor);

    let owner_sid = unsafe { sid_string(owner) };
    let mut sddl = std::ptr::null_mut();
    let mut sddl_len = 0u32;
    assert_ne!(
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                1,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut sddl,
                &mut sddl_len,
            )
        },
        0
    );
    let sddl_string = unsafe {
        std::ffi::OsString::from_wide(std::slice::from_raw_parts(
            sddl,
            sddl_len.saturating_sub(1) as usize,
        ))
    }
    .to_string_lossy()
    .into_owned();
    unsafe { LocalFree(sddl.cast()) };

    let mut control = 0u16;
    let mut revision = 0u32;
    assert_ne!(
        unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) },
        0
    );
    let mut present = 0;
    let mut defaulted = 0;
    assert_ne!(
        unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) },
        0
    );
    assert_ne!(present, 0, "DACL is absent");
    assert_ne!(dacl, std::ptr::null_mut(), "DACL is null");
    let mut info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    assert_ne!(
        unsafe {
            GetAclInformation(
                dacl,
                (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        },
        0
    );
    let mut ace_sids = Vec::new();
    let mut ace_types = Vec::new();
    let mut ace_flags = Vec::new();
    for index in 0..info.AceCount {
        let mut ace = std::ptr::null_mut();
        assert_ne!(unsafe { GetAce(dacl, index, &mut ace) }, 0);
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        let sid = unsafe {
            ace.cast::<u8>()
                .add(size_of::<ACE_HEADER>() + size_of::<u32>())
                .cast::<c_void>()
        };
        ace_sids.push(unsafe { sid_string(sid) });
        ace_types.push(header.AceType);
        ace_flags.push(header.AceFlags);
    }
    AclSnapshot {
        sddl: sddl_string,
        owner_sid,
        protected: control & SE_DACL_PROTECTED != 0,
        ace_sids,
        ace_types,
        ace_flags,
    }
}

#[test]
fn values_round_trip_after_reopening_the_store() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("credentials");
    let store = open(&root);
    store.put("session-token", b"token-value-01").unwrap();
    store.put("resume-secret", b"resume-value-02").unwrap();
    drop(store);

    let reopened = open(&root);
    assert_eq!(
        reopened.get("session-token").unwrap().as_deref(),
        Some(&b"token-value-01"[..])
    );
    assert_eq!(
        reopened.get("resume-secret").unwrap().as_deref(),
        Some(&b"resume-value-02"[..])
    );
    assert_eq!(reopened.get("missing").unwrap(), None);
}

#[test]
fn ciphertext_files_never_contain_secrets_and_overwrite_keeps_only_latest_value() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("credentials");
    let first = b"PLAINTEXT_MARKER_OLD_92b7a5";
    let latest = b"PLAINTEXT_MARKER_LATEST_f1540c";
    let store = open(&root);
    store.put("agent-session", first).unwrap();
    store.put("agent-session", latest).unwrap();
    drop(store);

    let reopened = open(&root);
    assert_eq!(
        reopened.get("agent-session").unwrap().as_deref(),
        Some(&latest[..])
    );
    let files = files_below(&root);
    assert!(
        !files.is_empty(),
        "credential ciphertext should be persisted"
    );
    for file in files {
        let bytes = fs::read(&file).expect("read persisted credential file");
        assert!(
            !contains_bytes(&bytes, first),
            "old plaintext leaked into {file:?}"
        );
        assert!(
            !contains_bytes(&bytes, latest),
            "latest plaintext leaked into {file:?}"
        );
        let name = file
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        assert!(
            !name.ends_with(".tmp") && !name.ends_with(".partial"),
            "temporary file remained: {file:?}"
        );
    }
}

#[test]
fn removal_is_idempotent_and_missing_keys_stay_missing_after_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("credentials");
    let store = open(&root);
    store.remove("not-present").unwrap();
    store.put("identity", b"secret").unwrap();
    store.remove("identity").unwrap();
    store.remove("identity").unwrap();
    assert_eq!(store.get("identity").unwrap(), None);
    drop(store);
    assert_eq!(open(&root).get("identity").unwrap(), None);
}

#[test]
fn malformed_keys_are_rejected_without_writing_outside_the_store() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("credentials");
    let outside = temp.path().join("outside");
    let store = open(&root);
    for key in [
        "",
        ".",
        "..",
        "../outside",
        "a/b",
        "a\\b",
        "nested/../outside",
    ] {
        assert!(
            store.put(key, b"must-not-escape").is_err(),
            "accepted key {key:?}"
        );
        assert!(store.get(key).is_err(), "read accepted invalid key {key:?}");
        assert!(
            store.remove(key).is_err(),
            "removed through invalid key {key:?}"
        );
    }
    assert!(!outside.exists(), "invalid key created an outside path");
    assert!(
        files_below(&root).is_empty(),
        "invalid keys wrote files inside the store"
    );
}

#[test]
fn oversized_write_is_rejected_without_replacing_existing_credential() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("credentials");
    let store = open(&root);
    store.put("refresh", b"previous-value").unwrap();
    let oversized = vec![b'x'; 64 * 1024 + 1];
    assert!(store.put("refresh", &oversized).is_err());
    assert_eq!(
        store.get("refresh").unwrap().as_deref(),
        Some(&b"previous-value"[..])
    );
    drop(store);
    assert_eq!(
        open(&root).get("refresh").unwrap().as_deref(),
        Some(&b"previous-value"[..])
    );
}

#[test]
fn tampered_ciphertext_and_ciphertext_copied_between_keys_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("credentials");
    let store = open(&root);
    store.put("key-a", b"alpha-secret").unwrap();
    let after_a = files_below(&root);
    assert_eq!(
        after_a.len(),
        1,
        "expected one persisted ciphertext for key-a"
    );
    store.put("key-b", b"bravo-secret").unwrap();
    let all = files_below(&root);
    assert_eq!(all.len(), 2, "expected one persisted ciphertext per key");
    let file_a = after_a[0].clone();
    let file_b = all.iter().find(|path| **path != file_a).unwrap().clone();

    let cipher_a = fs::read(&file_a).unwrap();
    let mut cipher_b = fs::read(&file_b).unwrap();
    assert!(!cipher_a.is_empty());
    let index = cipher_b.len() / 2;
    cipher_b[index] ^= 0x80;
    fs::write(&file_b, &cipher_b).unwrap();
    assert!(
        store.get("key-b").is_err(),
        "modified ciphertext was accepted"
    );

    fs::write(&file_b, &cipher_a).unwrap();
    assert!(
        store.get("key-b").is_err(),
        "ciphertext for key-a was accepted under key-b"
    );
    assert_eq!(
        store.get("key-a").unwrap().as_deref(),
        Some(&b"alpha-secret"[..])
    );
}

#[test]
fn new_store_under_modify_only_parent_is_private_without_changing_parent() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("modify-only");
    fs::create_dir(&parent).unwrap();
    let path = parent.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User; \
         $acl=New-Object Security.AccessControl.DirectorySecurity; \
         $acl.SetOwner($sid); $acl.SetAccessRuleProtection($true,$false); \
         $rule=New-Object Security.AccessControl.FileSystemAccessRule($sid,'Modify','ContainerInherit,ObjectInherit','None','Allow'); \
         $acl.AddAccessRule($rule); $directory=New-Object IO.DirectoryInfo('{path}'); $directory.SetAccessControl($acl)"
    );
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .unwrap();
    assert!(status.success(), "create isolated Modify-only parent ACL");
    let before = acl_snapshot(&parent).sddl;
    let root = parent.join("new-store");
    let store = open(&root);
    store.put("identity", b"modify-parent-fixture").unwrap();
    assert_eq!(
        store.get("identity").unwrap().as_deref(),
        Some(&b"modify-parent-fixture"[..])
    );
    drop(store.lock("reacquire").unwrap());
    drop(store.lock("reacquire").unwrap());
    let expected_sid = current_user_sid();
    for path in std::iter::once(root.clone()).chain(files_below(&root)) {
        let acl = acl_snapshot(&path);
        assert_eq!(acl.owner_sid, expected_sid);
        assert!(acl.protected);
        assert_eq!(acl.ace_sids.as_slice(), std::slice::from_ref(&expected_sid));
        assert_eq!(acl.ace_types, [0]);
        assert_eq!(acl.ace_flags, [0]);
    }
    assert_eq!(acl_snapshot(&parent).sddl, before, "parent ACL changed");
}

#[test]
fn store_directory_and_files_have_protected_current_user_only_acls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("credentials");
    let store = open(&root);
    store.put("acl-probe", b"not-the-secret-marker").unwrap();
    // A new lock is another protected object whose default owner may differ
    // from the user when this test runs with an elevated administrator token.
    drop(store.lock("acl-lock-probe").unwrap());

    let expected_sid = current_user_sid();
    let paths = std::iter::once(root.clone()).chain(files_below(&root));
    let paths = paths.collect::<Vec<_>>();
    assert!(
        paths.len() >= 3,
        "inspect store root, ciphertext and lock file ACLs"
    );
    for path in paths {
        let acl = acl_snapshot(&path);
        assert!(
            acl.protected,
            "DACL inheritance is not protected for {path:?}"
        );
        assert_eq!(acl.owner_sid, expected_sid, "unexpected owner for {path:?}");
        assert!(!acl.ace_sids.is_empty(), "DACL has no ACEs for {path:?}");
        for ((sid, ace_type), flags) in acl.ace_sids.iter().zip(&acl.ace_types).zip(&acl.ace_flags)
        {
            assert_eq!(
                sid, &expected_sid,
                "DACL includes another principal for {path:?}"
            );
            assert_eq!(*ace_type, 0, "DACL contains a non-allow ACE for {path:?}");
            assert_eq!(
                *flags, 0,
                "DACL contains an inherited or flagged ACE for {path:?}"
            );
        }
    }
}

#[test]
fn backup_directory_and_published_file_have_current_user_only_protected_acls() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("backups");
    let store = swarm_platform::BackupStore::open(&root).unwrap();
    store
        .write_new("acl-test", |file| {
            file.write_all(b"private backup")
                .map_err(|_| swarm_platform::PlatformError::Io)
        })
        .unwrap();
    let expected = current_user_sid();
    for path in [&root, &root.join("acl-test.backup")] {
        let acl = acl_snapshot(path);
        assert!(acl.protected);
        assert_eq!(acl.owner_sid, expected);
        assert_eq!(acl.ace_sids, vec![expected.clone()]);
        assert_eq!(acl.ace_types, vec![0]);
        assert_eq!(acl.ace_flags, vec![0]);
    }
}

#[test]
fn opening_a_junction_as_the_store_root_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let junction = temp.path().join("store-junction");
    fs::create_dir(&target).unwrap();
    let preserved = target.join("keep.bin");
    fs::write(&preserved, b"target-content").unwrap();
    let create = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "New-Item -ItemType Junction -Path $env:SWARM_CREDENTIAL_JUNCTION -Target $env:SWARM_CREDENTIAL_TARGET | Out-Null",
        ])
        .env("SWARM_CREDENTIAL_JUNCTION", &junction)
        .env("SWARM_CREDENTIAL_TARGET", &target)
        .output()
        .expect("start PowerShell to create a temporary directory junction");
    if !create.status.success() {
        panic!(
            "could not create temporary directory junction; environment limitation, not a passing check: {}",
            String::from_utf8_lossy(&create.stderr),
        );
    }

    assert!(CredentialStore::open(&junction).is_err());
    assert_eq!(fs::read(&preserved).unwrap(), b"target-content");
    fs::remove_dir(&junction).expect("remove junction itself without traversing target");
    assert_eq!(fs::read(&preserved).unwrap(), b"target-content");
}

#[test]
fn opening_an_existing_unrelated_directory_refuses_without_changing_acl_or_content() {
    let temp = tempfile::tempdir().unwrap();
    let unrelated = temp.path().join("unrelated");
    fs::create_dir(&unrelated).unwrap();
    let marker = unrelated.join("keep.txt");
    fs::write(&marker, b"unrelated-content").unwrap();
    let before = acl_snapshot(&unrelated).sddl;

    assert!(CredentialStore::open(&unrelated).is_err());

    assert_eq!(
        acl_snapshot(&unrelated).sddl,
        before,
        "failed open changed the unrelated directory ACL"
    );
    assert_eq!(fs::read(&marker).unwrap(), b"unrelated-content");
    assert_eq!(files_below(&unrelated).len(), 1);
}

#[test]
fn hard_linked_credential_is_refused_without_changing_external_link_or_acl() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("credentials");
    let store = open(&root);
    store.put("linked-value", b"hard-link-secret").unwrap();
    let stored_files = files_below(&root);
    assert_eq!(
        stored_files.len(),
        1,
        "expected one ciphertext file for one key"
    );
    let external = temp.path().join("external-ciphertext.bin");
    if let Err(error) = fs::hard_link(&stored_files[0], &external) {
        panic!(
            "could not create a temporary same-volume hard link; environment limitation: {error}"
        );
    }
    let bytes_before = fs::read(&external).unwrap();
    let acl_before = acl_snapshot(&external).sddl;

    assert!(store.get("linked-value").is_err());
    assert!(store.put("linked-value", b"replacement-secret").is_err());

    assert_eq!(fs::read(&external).unwrap(), bytes_before);
    assert_eq!(acl_snapshot(&external).sddl, acl_before);
    fs::remove_file(&external).unwrap();
    assert_eq!(
        store.get("linked-value").unwrap().as_deref(),
        Some(&b"hard-link-secret"[..])
    );
}
