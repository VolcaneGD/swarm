use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::Cursor;
use swarm_cli::{Command, parse_args};

#[test]
fn bundled_skill_selection_and_local_model_examples_match_current_parser() {
    let skill = include_str!("../../../.agents/skills/swarm-cli/SKILL.md");
    assert!(
        !skill.contains("there is no common --project flag"),
        "bundled Skill contradicts current project selection"
    );
    assert!(
        skill.contains("--local-model"),
        "bundled Skill must explain known-local unknown names"
    );
    let mut count = 0;
    for line in skill.lines().filter(|line| line.starts_with("swarm ")) {
        let args: Vec<OsString> = line
            .split_whitespace()
            .skip(1)
            .map(OsString::from)
            .collect();
        assert!(
            parse_args(&args, &mut Cursor::new([])).is_ok(),
            "bundled Skill example rejected"
        );
        count += 1;
    }
    assert_eq!(
        count, 2,
        "selection and local-model examples must both be present"
    );
}

#[test]
fn reference_examples_parse_and_cover_every_public_command() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let document = include_str!("../../../docs/CLI_REFERENCE.md");
    let examples = document
        .split("<!-- cli-examples:start -->")
        .nth(1)
        .unwrap()
        .split("<!-- cli-examples:end -->")
        .next()
        .unwrap();
    let mut operations = BTreeSet::new();
    let mut search_scopes = BTreeSet::new();
    let mut body_sources = BTreeSet::new();
    for line in examples.lines().filter(|line| line.starts_with("swarm ")) {
        // Reference examples deliberately use single-word arguments, with no
        // shell interpolation. This verifies CLI syntax, not a shell grammar.
        let words: Vec<_> = line.split_whitespace().skip(1).collect();
        let args: Vec<OsString> = words
            .iter()
            .map(|word| {
                if word.starts_with("docs/") {
                    workspace.join(word).into_os_string()
                } else {
                    OsString::from(word)
                }
            })
            .collect();
        let parsed = parse_args(&args, &mut Cursor::new("標準入力の本文".as_bytes()))
            .unwrap_or_else(|error| panic!("reference example failed: {line}: {error}"));
        match parsed {
            Command::Help => {
                operations.insert("help".to_owned());
            }
            Command::Version => {
                operations.insert("version".to_owned());
            }
            Command::RuntimeStatus { .. } => {
                operations.insert("runtime.status".to_owned());
            }
            Command::Request(spec) => {
                operations.insert(spec.operation.clone());
                if spec.operation == "search" {
                    search_scopes.insert(spec.args["scope"].as_str().unwrap().to_owned());
                }
                if spec.operation == "post.create" {
                    for source in ["--body", "--body-file", "--stdin"] {
                        if words.contains(&source) {
                            body_sources.insert(source);
                        }
                    }
                    assert!(!spec.args["body"].as_str().unwrap().is_empty());
                }
                if spec.operation == "rules.check" {
                    assert_eq!(spec.context["task"], "review");
                    assert_eq!(spec.args["target"]["no"], 1);
                }
            }
        }
    }
    let mut expected = BTreeSet::from(["help".to_owned(), "version".to_owned()]);
    for line in swarm_cli::help_text()
        .lines()
        .filter(|line| line.starts_with("  "))
    {
        let (area, actions) = line.trim().split_once(' ').unwrap();
        if area == "search" {
            expected.insert("search".to_owned());
        } else {
            for action in actions.split('|') {
                expected.insert(format!("{area}.{action}"));
            }
        }
    }
    assert_eq!(operations, expected);
    assert!(
        search_scopes.is_superset(
            &["body", "board_name", "thread_name"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        )
    );
    assert_eq!(
        body_sources,
        BTreeSet::from(["--body", "--body-file", "--stdin"])
    );
}

#[test]
fn actual_binary_help_and_version_match_the_reference() {
    let binary = env!("CARGO_BIN_EXE_swarm");
    let help = std::process::Command::new(binary)
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert_eq!(help, swarm_cli::help_text());
    for scope in ["body", "board_name", "thread_name"] {
        assert!(help.contains(scope));
    }
    let version = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        format!("swarm {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(
        include_str!("../../../docs/CLI_REFERENCE.md")
            .contains(&format!("swarm {}", env!("CARGO_PKG_VERSION")))
    );
}

#[test]
fn bilingual_quickstart_cli_examples_match_and_parse() {
    let japanese = include_str!("../../../docs/QUICKSTART.md");
    let english = include_str!("../../../docs/QUICKSTART.en.md");
    let prefix = "& .\\target\\debug\\swarm.exe ";
    let examples = |document: &str| {
        document
            .lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let examples = examples(japanese);
    let translated = english
        .lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        examples, translated,
        "translated commands must stay aligned"
    );
    assert_eq!(examples.len(), 10);
    for example in examples {
        // These examples contain single-word values or single-quoted words.
        // Resolve the documented data-root variable without executing a shell.
        let args: Vec<OsString> = example
            .split_whitespace()
            .map(|word| match word {
                "$swarmDemoRoot" => OsString::from("C:\\Swarm-Quickstart-Test"),
                other => OsString::from(other.trim_matches('\'')),
            })
            .collect();
        parse_args(&args, &mut Cursor::new([]))
            .unwrap_or_else(|error| panic!("quickstart example failed: {example}: {error}"));
    }
}

#[test]
fn ai_usage_examples_parse_with_real_utf8_files() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let document = include_str!("../../../docs/AI_USAGE.md");
    let examples = document
        .split("<!-- ai-examples:start -->")
        .nth(1)
        .unwrap()
        .split("<!-- ai-examples:end -->")
        .next()
        .unwrap();
    let mut count = 0;
    for line in examples.lines().filter(|line| line.starts_with("swarm ")) {
        let args: Vec<OsString> = line
            .split_whitespace()
            .skip(1)
            .map(|word| {
                if word.starts_with("docs/") {
                    workspace.join(word).into_os_string()
                } else {
                    OsString::from(word)
                }
            })
            .collect();
        parse_args(&args, &mut Cursor::new([]))
            .unwrap_or_else(|error| panic!("AI example failed: {line}: {error}"));
        count += 1;
    }
    assert_eq!(count, 18);
}
