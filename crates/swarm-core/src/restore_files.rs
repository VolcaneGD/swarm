//! Exclusive Windows RULES handles held across capture, apply and rollback.
use crate::protocol::Fault;
#[cfg(windows)]
use std::os::windows::{
    fs::{MetadataExt, OpenOptionsExt},
    io::AsRawHandle,
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

fn storage() -> Fault {
    Fault::new(
        "RESTORE_RULES",
        8,
        "Restore RULES targets could not be locked or written",
    )
}
struct Entry {
    path: PathBuf,
    file: Option<File>,
    content: Option<String>,
}
pub struct RestoreRulesGuard {
    entries: Vec<Entry>,
    _directories: Vec<File>,
}
impl RestoreRulesGuard {
    pub fn acquire(paths: &[PathBuf], recovering: bool) -> Result<Self, Fault> {
        let mut entries = Vec::new();
        let mut directories = Vec::new();
        for path in paths {
            if !path.is_absolute()
                || path.file_name().and_then(|n| n.to_str()) != Some("RULES.md")
                || entries.iter().any(|e: &Entry| e.path == *path)
            {
                return Err(storage());
            }
            for ancestor in path.parent().ok_or_else(storage)?.ancestors() {
                if ancestor.as_os_str().is_empty() {
                    continue;
                }
                let metadata = std::fs::symlink_metadata(ancestor).map_err(|_| storage())?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(storage());
                }
                #[cfg(windows)]
                {
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err(storage());
                    }
                    let file = OpenOptions::new()
                        .read(true)
                        .share_mode(3)
                        .custom_flags(0x0220_0000)
                        .open(ancestor)
                        .map_err(|_| storage())?;
                    if file.metadata().map_err(|_| storage())?.file_attributes() & 0x400 != 0 {
                        return Err(storage());
                    }
                    directories.push(file);
                }
            }
            let mut options = OpenOptions::new();
            options.read(true).write(true);
            #[cfg(windows)]
            options
                .access_mode(0x8000_0000 | 0x4000_0000 | 0x0001_0000)
                .share_mode(0)
                .custom_flags(0x0020_0000);
            let file = match options.open(path) {
                Ok(mut f) => {
                    check(&f)?;
                    let mut bytes = Vec::new();
                    Read::by_ref(&mut f)
                        .take(1024 * 1024 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| storage())?;
                    if bytes.len() > 1024 * 1024 {
                        return Err(storage());
                    }
                    let content = match String::from_utf8(bytes) {
                        Ok(s) => s,
                        Err(_) if recovering => String::new(),
                        Err(_) => return Err(storage()),
                    };
                    entries.push(Entry {
                        path: path.clone(),
                        file: Some(f),
                        content: Some(content),
                    });
                    continue;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return Err(storage()),
            };
            entries.push(Entry {
                path: path.clone(),
                file,
                content: None,
            });
        }
        Ok(Self {
            entries,
            _directories: directories,
        })
    }
    pub fn contents(&self) -> Vec<Option<String>> {
        self.entries.iter().map(|e| e.content.clone()).collect()
    }
    pub fn write(&mut self, contents: &[Option<String>]) -> Result<(), Fault> {
        if contents.len() != self.entries.len() {
            return Err(storage());
        }
        for (entry, content) in self.entries.iter_mut().zip(contents) {
            if content.as_ref().is_some_and(|s| s.len() > 1024 * 1024) {
                return Err(storage());
            }
            if entry.file.is_none() && content.is_some() {
                let mut options = OpenOptions::new();
                options.read(true).write(true).create_new(true);
                #[cfg(windows)]
                options
                    .access_mode(0x8000_0000 | 0x4000_0000 | 0x0001_0000)
                    .share_mode(0)
                    .custom_flags(0x0020_0000);
                entry.file = Some(options.open(&entry.path).map_err(|_| storage())?);
            }
            if let Some(file) = &mut entry.file {
                #[cfg(windows)]
                if content.is_some() {
                    disposition(file, false)?;
                }
                check(file)?;
                #[cfg(windows)]
                if content.is_none() {
                    disposition(file, true)?;
                }
                if let Some(text) = content {
                    file.seek(SeekFrom::Start(0)).map_err(|_| storage())?;
                    file.set_len(0).map_err(|_| storage())?;
                    file.write_all(text.as_bytes()).map_err(|_| storage())?;
                    file.sync_all().map_err(|_| storage())?;
                    file.seek(SeekFrom::Start(0)).map_err(|_| storage())?;
                    let mut check = String::new();
                    file.read_to_string(&mut check).map_err(|_| storage())?;
                    if check != *text {
                        return Err(storage());
                    }
                }
            }
        }
        Ok(())
    }
}
fn check(file: &File) -> Result<(), Fault> {
    let metadata = file.metadata().map_err(|_| storage())?;
    if !metadata.is_file() {
        return Err(storage());
    }
    #[cfg(windows)]
    {
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(storage());
        }
        if crate::rules_editor::hardlink_count(file)? != 1 {
            return Err(storage());
        }
    }
    Ok(())
}
#[cfg(windows)]
fn disposition(file: &File, delete: bool) -> Result<(), Fault> {
    #[repr(C)]
    struct Info {
        delete: u8,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileInformationByHandle(
            handle: *mut std::ffi::c_void,
            class: u32,
            info: *const std::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    let info = Info {
        delete: u8::from(delete),
    };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle().cast(),
            4,
            (&info as *const Info).cast(),
            std::mem::size_of::<Info>() as u32,
        )
    } == 0
    {
        return Err(storage());
    }
    Ok(())
}
