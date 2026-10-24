use std::io;
use swarm_cli::{Command, OutputFormat, parse_args};

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut stdin = io::stdin().lock();
    match parse_args(&args, &mut stdin) {
        Ok(Command::Help) => {
            print!("{}", swarm_cli::help_text());
        }
        Ok(Command::Version) => {
            println!("swarm {}", env!("CARGO_PKG_VERSION"));
        }
        Ok(Command::RuntimeStatus { data_dir, format }) => {
            match swarm_runtime::lifecycle::status(&data_dir) {
                Ok(data) => {
                    let mut stdout = io::stdout().lock();
                    if swarm_cli::emit_result(format, "runtime.status", &data, &mut stdout).is_err()
                    {
                        std::process::exit(8);
                    }
                }
                Err(error) => {
                    let mut stdout = io::stdout().lock();
                    let mut stderr = io::stderr().lock();
                    let _ = swarm_cli::emit_error(format, &error, &mut stdout, &mut stderr);
                    std::process::exit(error.exit_code);
                }
            }
        }
        Ok(Command::BugReport {
            data_dir,
            format,
            action,
            report_file,
        }) => match swarm_cli::execute_bug_report(&data_dir, &action, report_file.as_deref()) {
            Ok((operation, data)) => {
                if swarm_cli::emit_result(format, &operation, &data, &mut io::stdout().lock())
                    .is_err()
                {
                    std::process::exit(8);
                }
            }
            Err(error) => {
                let _ = swarm_cli::emit_error(
                    format,
                    &error,
                    &mut io::stdout().lock(),
                    &mut io::stderr().lock(),
                );
                std::process::exit(error.exit_code);
            }
        },
        Ok(Command::Request(spec)) => {
            let format = spec.format;
            let result = swarm_cli::execute(spec);
            match result {
                Ok((operation, data)) => {
                    let mut stdout = io::stdout().lock();
                    if swarm_cli::emit_result(format, &operation, &data, &mut stdout).is_err() {
                        std::process::exit(8);
                    }
                }
                Err(error) => {
                    let mut stdout = io::stdout().lock();
                    let mut stderr = io::stderr().lock();
                    let _ = swarm_cli::emit_error(format, &error, &mut stdout, &mut stderr);
                    std::process::exit(error.exit_code);
                }
            }
        }
        Err(error) => {
            let format = if args
                .windows(2)
                .any(|pair| pair[0] == "--format" && pair[1] == "json")
            {
                OutputFormat::Json
            } else {
                OutputFormat::Markdown
            };
            let mut stdout = io::stdout().lock();
            let mut stderr = io::stderr().lock();
            let _ = swarm_cli::emit_error(format, &error, &mut stdout, &mut stderr);
            std::process::exit(error.exit_code);
        }
    }
}
