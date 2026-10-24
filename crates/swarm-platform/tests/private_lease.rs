#![cfg(windows)]

use std::{
    ffi::c_void,
    fs,
    mem::{size_of, zeroed},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};
use swarm_platform::CredentialStore;
use windows_sys::Win32::{
    Foundation::{HANDLE, LocalFree},
    Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT, SetSecurityInfo},
    Security::{
        ACCESS_ALLOWED_ACE, ACL, ACL_REVISION, ACL_SIZE_INFORMATION, AclSizeInformation,
        AddAccessAllowedAce, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation,
        GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
        GetTokenInformation, InitializeAcl, IsValidAcl, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

fn user_sid_matches(sid: *mut c_void) -> bool {
    let mut token: HANDLE = std::ptr::null_mut();
    assert_ne!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) },
        0
    );
    let mut needed = 0;
    unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
    assert_ne!(needed, 0);
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
    let user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
    let matches = unsafe { EqualSid(user.User.Sid, sid) } != 0;
    unsafe { windows_sys::Win32::Foundation::CloseHandle(token) };
    matches
}

fn has_expected_private_acl(file: &fs::File) -> bool {
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || descriptor.is_null() {
        return false;
    }

    let valid = unsafe {
        let mut owner = std::ptr::null_mut();
        let mut owner_defaulted = 0;
        let mut control = 0;
        let mut revision = 0;
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl = std::ptr::null_mut::<ACL>();
        if GetSecurityDescriptorOwner(descriptor, &mut owner, &mut owner_defaulted) == 0
            || owner.is_null()
            || !user_sid_matches(owner)
            || GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) == 0
            || control & SE_DACL_PROTECTED == 0
            || GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) == 0
            || present == 0
            || defaulted != 0
            || dacl.is_null()
            || IsValidAcl(dacl) == 0
        {
            false
        } else {
            let mut info: ACL_SIZE_INFORMATION = zeroed();
            if GetAclInformation(
                dacl,
                (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            ) == 0
                || info.AceCount != 1
            {
                false
            } else {
                let mut ace_ptr = std::ptr::null_mut();
                if GetAce(dacl, 0, &mut ace_ptr) == 0 || ace_ptr.is_null() {
                    false
                } else {
                    let ace = &*ace_ptr.cast::<ACCESS_ALLOWED_ACE>();
                    let ace_sid = std::ptr::addr_of!(ace.SidStart) as *mut c_void;
                    ace.Header.AceType == 0
                        && ace.Header.AceFlags == 0
                        && ace.Mask == windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS
                        && EqualSid(ace_sid, owner) != 0
                }
            }
        }
    };
    unsafe { LocalFree(descriptor.cast()) };
    valid
}

fn replace_dacl_with_two_full_access_aces(file: &fs::File) {
    let mut descriptor = std::ptr::null_mut();
    assert_eq!(
        unsafe {
            GetSecurityInfo(
                file.as_raw_handle() as HANDLE,
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        },
        0
    );
    let mut owner = std::ptr::null_mut();
    let mut owner_defaulted = 0;
    assert_ne!(
        unsafe { GetSecurityDescriptorOwner(descriptor, &mut owner, &mut owner_defaulted) },
        0
    );

    // S-1-1-0 (Everyone), held in aligned storage for AddAccessAllowedAce.
    let everyone_sid_bytes = [1u8, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];
    let mut everyone_storage = [0usize; 2];
    unsafe {
        std::ptr::copy_nonoverlapping(
            everyone_sid_bytes.as_ptr(),
            everyone_storage.as_mut_ptr().cast::<u8>(),
            everyone_sid_bytes.len(),
        );
    }
    let mut acl_storage = [0usize; 128];
    let acl = acl_storage.as_mut_ptr().cast::<ACL>();
    assert_ne!(
        unsafe {
            InitializeAcl(
                acl,
                (acl_storage.len() * size_of::<usize>()) as u32,
                ACL_REVISION,
            )
        },
        0
    );
    assert_ne!(
        unsafe {
            AddAccessAllowedAce(
                acl,
                ACL_REVISION,
                windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS,
                owner,
            )
        },
        0
    );
    assert_ne!(
        unsafe {
            AddAccessAllowedAce(
                acl,
                ACL_REVISION,
                windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS,
                everyone_storage.as_mut_ptr().cast(),
            )
        },
        0
    );
    assert_eq!(
        unsafe {
            SetSecurityInfo(
                file.as_raw_handle() as HANDLE,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                acl,
                std::ptr::null_mut(),
            )
        },
        0
    );
    unsafe { LocalFree(descriptor.cast()) };
}

fn private_store(root: &Path) -> CredentialStore {
    CredentialStore::open(root).expect("open private credential store")
}

#[test]
#[ignore = "invoked only by the cross-process lease test"]
fn private_lease_child() {
    let root = std::env::var_os("SWARM_PRIVATE_LEASE_TEST_ROOT").unwrap();
    let store = private_store(Path::new(&root));
    match store.private_lease(".host.lock") {
        Ok(_held) => println!("LEASE_CHILD_ACQUIRED"),
        Err(error) if error.raw_os_error() == Some(32) => println!("LEASE_CHILD_EXCLUDED"),
        Err(error) => panic!("unexpected child error: {error}"),
    }
}

#[test]
fn separate_process_preserves_the_original_plaintext_lease_path() {
    use std::os::windows::process::CommandExt;
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("private");
    let store = private_store(&root);
    drop(store.private_lease(".host.lock").unwrap());
    // Simulate an older process's exact existing pathname/share-mode contract.
    let old_holder = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(root.join(".host.lock"))
        .unwrap();
    let child = || {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .creation_flags(0x0800_0000)
            .args(["--exact", "private_lease_child", "--ignored", "--nocapture"])
            .env("SWARM_PRIVATE_LEASE_TEST_ROOT", &root)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    assert!(child().contains("LEASE_CHILD_EXCLUDED"));
    drop(old_holder);
    assert!(child().contains("LEASE_CHILD_ACQUIRED"));
}

#[test]
fn lease_creation_sets_private_acl_and_excludes_until_handle_drop() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("private");
    let store = private_store(&root);

    let held = store.private_lease(".host.lock").unwrap();
    assert!(has_expected_private_acl(&held));
    let contention = store.private_lease(".host.lock").unwrap_err();
    assert_eq!(contention.raw_os_error(), Some(32));

    drop(held);
    drop(store.private_lease(".host.lock").unwrap());
    assert!(root.join(".host.lock").is_file());
}

#[test]
fn lease_rejects_unsafe_names_nonempty_files_and_hard_links_without_mutation() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("private");
    let store = private_store(&root);

    for name in [
        "",
        ".",
        "..",
        "../outside",
        "a/b",
        "a\\b",
        "C:lock",
        "NUL",
        "CON.lock",
        "COM1.lock",
        "LPT9",
        "runtime.lock.",
    ] {
        assert!(store.private_lease(name).is_err(), "accepted {name:?}");
    }

    fs::write(root.join("runtime.lock"), b"occupied").unwrap();
    assert_eq!(
        store.private_lease("runtime.lock").unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(root.join("runtime.lock")).unwrap(), b"occupied");

    drop(store.private_lease("hardlink.lock").unwrap());
    let outside = parent.path().join("alias.lock");
    fs::hard_link(root.join("hardlink.lock"), &outside).unwrap();
    assert!(store.private_lease("hardlink.lock").is_err());
    assert_eq!(fs::metadata(&outside).unwrap().len(), 0);

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_file;
        let target = root.join("runtime.lock");
        let link = root.join("reparse.lock");
        if symlink_file(&target, &link).is_ok() {
            assert!(store.private_lease("reparse.lock").is_err());
            assert!(
                fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        }
    }

    drop(store.private_lease("permissive.lock").unwrap());
    let permissive = fs::OpenOptions::new()
        .access_mode(windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS)
        .open(root.join("permissive.lock"))
        .unwrap();
    replace_dacl_with_two_full_access_aces(&permissive);
    drop(permissive);
    assert!(store.private_lease("permissive.lock").is_err());
    let unchanged = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("permissive.lock"))
        .unwrap();
    assert!(!has_expected_private_acl(&unchanged));
}
