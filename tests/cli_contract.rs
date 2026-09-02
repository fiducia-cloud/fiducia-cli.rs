//! End-to-end assertions on the built binary.
//!
//! The unit tests in `src/flags.rs` cover parsing; these cover the things only
//! a real process can show: that `--help` is rendered from `.cli-flags.toml`
//! rather than a Rust string, that command-scoped flags stay scoped, and that
//! the documented exit codes are what a script actually observes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn fiducia(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fiducia"))
        .args(args)
        .current_dir(repo_root())
        // Pin the width so the table layout does not depend on the terminal
        // running the test.
        .env("COLUMNS", "100")
        .output()
        .expect("the fiducia binary should run")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn root_help_lists_every_declared_command() {
    let output = fiducia(&["--help"]);
    assert!(output.status.success());
    let help = stdout(&output);

    // Sourced from .cli-flags.toml, so this is really asserting that the two
    // stay in sync without a hand-maintained usage string in between.
    for command in ["regions", "region", "closest", "health", "completion"] {
        assert!(help.contains(command), "root help omits {command}:\n{help}");
    }
    for flag in ["--regions", "--samples", "--json", "FIDUCIA_TIMEOUT_MS"] {
        assert!(help.contains(flag), "root help omits {flag}:\n{help}");
    }
}

#[test]
fn command_scoped_flags_appear_only_under_their_command() {
    let scoped = stdout(&fiducia(&["health", "--help"]));
    assert!(
        scoped.contains("--url"),
        "health help omits --url:\n{scoped}"
    );

    let root = stdout(&fiducia(&["--help"]));
    assert!(
        !root.contains("--url"),
        "a command-scoped flag leaked into the root help table:\n{root}"
    );

    // And it is rejected outright outside its command rather than ignored.
    let output = fiducia(&["regions", "--url=https://node.test"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unknown command-line option"));
}

#[test]
fn completion_scripts_are_emitted_for_both_shells() {
    for shell in ["bash", "zsh"] {
        let output = fiducia(&["completion", "--shell", shell]);
        assert!(output.status.success(), "{shell} completion failed");
        let script = stdout(&output);
        assert!(
            script.contains("fiducia"),
            "{shell} script names no command"
        );
        // The point of a static script: no runtime dependency on the parser.
        assert!(
            !script.contains("flags2env audit"),
            "{shell} completion shells out at completion time"
        );
    }

    let rejected = fiducia(&["completion", "--shell", "fish"]);
    assert_eq!(rejected.status.code(), Some(2));
}

#[test]
fn exit_codes_match_the_documented_contract() {
    // 2 — bad invocation.
    assert_eq!(fiducia(&["regions", "--nope"]).status.code(), Some(2));
    assert_eq!(
        fiducia(&["definitely-not-a-command"]).status.code(),
        Some(2)
    );

    // 3 — the contract itself could not be read.
    let broken = Command::new(env!("CARGO_BIN_EXE_fiducia"))
        .arg("regions")
        .current_dir(repo_root())
        .env("FIDUCIA_FLAGS_CONFIG", "/nonexistent/.cli-flags.toml")
        .output()
        .expect("the fiducia binary should run");
    assert_eq!(broken.status.code(), Some(3));

    // 1 — the invocation was fine, the work failed.
    assert_eq!(
        fiducia(&["regions", "--regions=/nonexistent/regions.json"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn rejected_option_values_are_not_reflected_back() {
    // A mistyped flag is as likely to carry a secret as a typo, so diagnostics
    // name the option and never its value.
    let sentinel = "must-remain-environment-only";
    let output = fiducia(&["regions", &format!("--api-token={sentinel}")]);
    assert_eq!(output.status.code(), Some(2));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(combined.contains("--api-token"));
    assert!(!combined.contains(sentinel));
}
