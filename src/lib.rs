//! `fiducia` — the command-line client for fiducia.cloud.
//!
//! The binary in `src/main.rs` is deliberately thin: it hands argv to
//! [`run`] and turns the result into an exit code. Everything else is a module
//! with one job:
//!
//! | module | job |
//! | --- | --- |
//! | [`flags`] | argv + environment → a validated [`flags::CliArgs`], via flags-2-env |
//! | [`cli_config`] | the env-keyed struct generated from `.cli-flags.toml` |
//! | [`help`] | `--help` tables and shell completions, rendered by the C core |
//! | [`regions`] | pure region parsing, ranking, and selection |
//! | [`probe`] | the latency probe loop (the only network I/O for `region`) |
//! | [`commands`] | one module per subcommand, each returning a [`output::Report`] |
//! | [`output`] | human table vs. `--json` |
//! | [`error`] | [`error::CliError`] and the exit codes it maps to |
//!
//! The flag contract itself is not in this crate — it is `.cli-flags.toml`,
//! parsed at runtime by [flags-2-env](https://github.com/ORESoftware/flags-2-env).
//! Help text, shell completions, env-var names, defaults, and types all derive
//! from that one file.

// Regenerate after editing `.cli-flags.toml`:
//   flags2env generate rust .cli-flags.toml --name CliConfig > src/cli_config.rs
// CI diffs the file against fresh generator output, so it stays byte-identical.
// Command-scoped flags land there as `Option` even when they declare a default,
// because a scoped default only applies when its own command runs.
pub mod cli_config;
pub mod commands;
pub mod env_map;
pub mod error;
pub mod flags;
pub mod help;
pub mod output;
pub mod probe;
pub mod regions;

pub use env_map::{env_value, get_env_map, EnvMap};
pub use error::CliError;
pub use output::{Format, Report};
// Re-exported so integration tests and downstream callers keep the flat
// `fiducia_cli::parse_regions` paths they already use.
pub use regions::{closest, median, parse_regions, rank, select_regions, truthy, Region, RegionLatency};

/// The program name used in help tables, completion scripts, and diagnostics.
pub const PROGRAM: &str = "fiducia";

/// Parses `argv`, runs the selected command, and returns the process exit code.
///
/// `--help` short-circuits before any work, and usage errors print the same
/// generated table, so there is exactly one description of the CLI surface.
pub fn run(argv: &[String]) -> i32 {
    let config_path = match flags::resolve_config_path() {
        Ok(path) => path,
        Err(error) => return report(&CliError::config(error), None, argv),
    };

    if help::is_help_requested(argv) {
        return match help::help_table(&config_path, PROGRAM, argv) {
            Ok(table) => {
                print!("{table}");
                0
            }
            Err(error) => report(&error, None, argv),
        };
    }

    let args = match flags::parse_cli_args(argv, &config_path) {
        Ok(args) => args,
        Err(error) => return report(&CliError::usage(error), Some(&config_path), argv),
    };

    match commands::dispatch(&args, &config_path) {
        Ok(code) => code,
        Err(error) => report(&error, Some(&config_path), argv),
    }
}

/// Prints a diagnostic on stderr, follows usage errors with the generated help
/// table, and returns the error's exit code.
fn report(error: &CliError, config_path: Option<&std::path::Path>, argv: &[String]) -> i32 {
    eprintln!("{PROGRAM}: {error}");
    if error.wants_help() {
        if let Some(config_path) = config_path {
            if let Ok(table) = help::help_table(config_path, PROGRAM, argv) {
                eprint!("\n{table}");
            }
        }
    }
    error.exit_code()
}
