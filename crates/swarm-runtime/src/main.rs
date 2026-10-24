#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let human = args.len() == 3 && args[2] == "--human-start";
    let startup = args.len() == 3 && args[2] == "--windows-startup";
    if !(args.len() == 2 || human || startup) || args[0] != "--data-dir" {
        eprintln!("Usage: swarm-runtime --data-dir PATH [--human-start | --windows-startup]");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[1]);
    let result = if human {
        swarm_runtime::serve_human(&root)
    } else if startup {
        swarm_runtime::serve_windows_startup(&root)
    } else {
        swarm_runtime::serve(&root, None)
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(error.exit_code);
    }
}
