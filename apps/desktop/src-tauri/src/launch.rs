use std::{ffi::OsString, path::PathBuf};

pub struct Launch {
    pub root: PathBuf,
    pub activation: Option<String>,
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
        root: root.unwrap_or(default_root),
        activation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
