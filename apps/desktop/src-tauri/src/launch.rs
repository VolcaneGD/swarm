use std::{ffi::OsString, path::PathBuf};

pub struct Launch {
    pub root: PathBuf,
    pub activation: Option<String>,
    explicit_root: bool,
}

pub fn parse(
    args: impl IntoIterator<Item = OsString>,
    default_root: PathBuf,
) -> Result<Launch, &'static str> {
    let mut args = args.into_iter();
    let mut root = None;
    let mut activation = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--data-dir") if root.is_none() => {
                let path = args
                    .next()
                    .filter(|path| !path.is_empty())
                    .ok_or("Swarm requires a data directory path")?;
                root = Some(PathBuf::from(path));
            }
            Some("--notification-activation") if activation.is_none() => {
                let uri = args
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .filter(|value| !value.is_empty() && value.len() <= 256)
                    .ok_or("Swarm received an invalid notification activation")?;
                activation = Some(uri);
            }
            _ => return Err("Swarm received an unknown or duplicate startup option"),
        }
    }
    Ok(Launch {
        explicit_root: root.is_some(),
        root: root.unwrap_or(default_root),
        activation,
    })
}

pub fn parse_with_binding(
    args: impl IntoIterator<Item = OsString>,
    default_root: PathBuf,
    binding: &std::path::Path,
) -> Result<Launch, &'static str> {
    use std::io::Read;
    let mut launch = parse(args, default_root)?;
    // A human's explicit command line always wins, including recovery from a
    // stale/unavailable saved root. Never merge arbitrary arguments from disk.
    if launch.explicit_root {
        return Ok(launch);
    }
    let metadata = match std::fs::symlink_metadata(binding) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(launch),
        Err(_) => return Err("Swarm could not read its saved Data selection"),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16384 {
        return Err("Swarm has invalid saved Data settings");
    }
    let mut bytes = Vec::new();
    std::fs::File::open(binding)
        .map_err(|_| "Swarm could not read its saved Data selection")?
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(|_| "Swarm could not read its saved Data selection")?;
    if bytes.len() > 16384 {
        return Err("Swarm has invalid saved Data settings");
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "Swarm has invalid saved Data settings")?;
    if value["schema_version"] != 1 || value.as_object().is_none_or(|v| v.len() != 2) {
        return Err("Swarm has invalid saved Data settings");
    }
    let root = value["data_dir"]
        .as_str()
        .filter(|root| !root.is_empty() && !root.chars().any(char::is_control))
        .map(PathBuf::from)
        .ok_or("Swarm has invalid saved Data settings")?;
    if !root.is_absolute() || !root.is_dir() {
        return Err(
            "Swarm's saved Data directory is unavailable; check the shortcut's --data-dir selection",
        );
    }
    launch.root = root;
    Ok(launch)
}

pub fn startup_error_text(message: &str) -> String {
    format!(
        "Swarmを起動できませんでした。\n{message}\n\nショートカットの --data-dir が既存のData保存先を指定しているか確認してください。Dataを削除・再作成しないでください。\n\nSwarm could not start. Check the shortcut's --data-dir selection. Do not delete or recreate existing Data."
    )
}

#[cfg(windows)]
pub fn show_startup_error(message: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            window: *mut std::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            kind: u32,
        ) -> i32;
    }
    let text: Vec<u16> = startup_error_text(message)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let title: Vec<u16> = "Swarm — 起動エラー / Startup error"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // The host may fail before Tauri/WebView exists, so use an OS-native dialog.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
    }
}
#[cfg(not(windows))]
pub fn show_startup_error(message: &str) {
    eprintln!("{}", startup_error_text(message));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relocated_no_argument_launch_preserves_data_and_explicit_override_wins() {
        let fixture = tempfile::tempdir().unwrap();
        let data = fixture.path().join("Original Data");
        std::fs::create_dir(&data).unwrap();
        let binding = fixture.path().join("swarm-launch.json");
        std::fs::write(
            &binding,
            serde_json::to_vec(&serde_json::json!({"schema_version":1,"data_dir":data})).unwrap(),
        )
        .unwrap();
        let restored =
            parse_with_binding([], fixture.path().join("Wrong default"), &binding).unwrap();
        assert_eq!(restored.root, data);
        std::fs::write(&binding, b"broken").unwrap();
        assert!(parse_with_binding([], fixture.path().to_owned(), &binding).is_err());
        let explicit = parse_with_binding(
            [OsString::from("--data-dir"), data.clone().into_os_string()],
            fixture.path().to_owned(),
            &binding,
        )
        .unwrap();
        assert_eq!(explicit.root, data);
        assert!(startup_error_text("failure").contains("Dataを削除・再作成しない"));
        assert!(startup_error_text("failure").contains("--data-dir"));
    }
    #[test]
    fn missing_binding_keeps_default_and_bad_saved_roots_fail_without_creation() {
        let fixture = tempfile::tempdir().unwrap();
        let binding = fixture.path().join("swarm-launch.json");
        let default = fixture.path().join("Default");
        assert_eq!(
            parse_with_binding([], default.clone(), &binding)
                .unwrap()
                .root,
            default
        );
        for root in [PathBuf::from("relative"), fixture.path().join("Missing")] {
            std::fs::write(
                &binding,
                serde_json::to_vec(&serde_json::json!({"schema_version":1,"data_dir":root}))
                    .unwrap(),
            )
            .unwrap();
            assert!(parse_with_binding([], default.clone(), &binding).is_err());
            assert!(!root.exists());
        }
        assert!(!default.exists());
    }
    #[test]
    fn explicit_options_are_single_bounded_and_never_extra_commands() {
        let parse_args =
            |args: &[&str]| parse(args.iter().map(OsString::from), PathBuf::from("C:/default"));
        let launch = parse_args(&[
            "--data-dir",
            "C:/private root",
            "--notification-activation",
            "scheme://thread/a/b",
        ])
        .unwrap();
        assert_eq!(launch.root, PathBuf::from("C:/private root"));
        assert_eq!(launch.activation.as_deref(), Some("scheme://thread/a/b"));
        for args in [
            vec!["--data-dir"],
            vec!["--data-dir", ""],
            vec!["--data-dir", "C:/a", "--data-dir", "C:/b"],
            vec!["--notification-activation"],
            vec![
                "--notification-activation",
                "a",
                "--notification-activation",
                "b",
            ],
            vec!["--notification-activation", "a", "--start"],
            vec!["--unknown"],
        ] {
            assert!(parse_args(&args).is_err());
        }
        assert!(parse_args(&["--notification-activation", &"x".repeat(257)]).is_err());
    }
}
