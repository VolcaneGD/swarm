#![windows_subsystem = "windows"]
//! Compare actual shell handling; invalid CLI argument exits before any Data use.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, fs::OpenOptions, io::Write, os::windows::ffi::OsStrExt, path::Path};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError},
    System::Threading::{GetExitCodeProcess, WaitForSingleObject},
    UI::Shell::{
        SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        ShellExecuteExW,
    },
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const STAGE: &str = "C:/Swarm-Validation/Inputs/startup-fixed-20261024";
const RECEIPTS: &str = "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024";
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut std::ffi::c_void, flags: u32) -> i32;
    fn CoUninitialize();
}
struct ComGuard;
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(window: isize, text: *const u16, title: *const u16, kind: u32) -> i32;
}
fn probe(path: &Path) -> Result<Value> {
    let file: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let verb: Vec<_> = "open".encode_utf16().chain(Some(0)).collect();
    let args: Vec<_> = "--swarm-path-probe".encode_utf16().chain(Some(0)).collect();
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC;
    info.lpFile = file.as_ptr();
    info.lpVerb = verb.as_ptr();
    info.lpParameters = args.as_ptr();
    info.nShow = 0;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = unsafe { GetLastError() };
        return Ok(json!({"shell_success":false,"win32_error":error}));
    }
    if info.hProcess.is_null() {
        return Err("No owned child handle; preserve state".into());
    }
    let wait = unsafe { WaitForSingleObject(info.hProcess, 10000) };
    let mut code = 0;
    let ok = unsafe { GetExitCodeProcess(info.hProcess, &mut code) };
    unsafe { CloseHandle(info.hProcess) };
    if wait != 0 || ok == 0 || code != 2 {
        return Err("Owned probe outcome unknown; do not repeat".into());
    }
    Ok(json!({"shell_success":true,"exit_code":code,"data_opened":false}))
}
fn run() -> Result<Value> {
    if std::env::var("COMPUTERNAME")?.to_uppercase() != "DESKTOP-1TAP8BI"
        || std::env::var("USERNAME")?.to_uppercase() != "GPT"
    {
        return Err("Wrong guest".into());
    }
    if !Path::new(RECEIPTS)
        .join("startup-direct-invocation-result.json")
        .is_file()
    {
        return Err("Prior direct invocation result required".into());
    }
    let target = std::path::PathBuf::from(STAGE.replace('/', "\\")).join("swarm-runtime.exe");
    if format!("{:x}", Sha256::digest(fs::read(&target)?))
        != "c7b93c9f57314a233e59ff523b03aa0626034bf30f680ac0d16e3bb234325a57"
    {
        return Err("Candidate pin mismatch".into());
    }
    let receipt = Path::new(RECEIPTS).join("startup-shell-path-probe.json");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(receipt)?;
    if unsafe { CoInitializeEx(std::ptr::null_mut(), 2) } < 0 {
        return Err("COM initialization failed".into());
    }
    let _com = ComGuard;
    let ordinary = probe(&target)?;
    let extended = probe(&target.canonicalize()?)?;
    let result = json!({"ordinary":ordinary,"canonical_extended":extended,"invalid_argument_only":true,"data_touched":false,"automatic_startup_accepted":false});
    output.write_all(serde_json::to_string_pretty(&result)?.as_bytes())?;
    Ok(result)
}
fn main() {
    let message = match run() {
        Ok(v) => format!("SHELL_PATH_DIAGNOSTIC\n{v:#}"),
        Err(e) => format!("DIAGNOSTIC_FAILED\n{e}"),
    };
    let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<_> = "Swarm shell path result"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe { MessageBoxW(0, text.as_ptr(), title.as_ptr(), 0) };
}
