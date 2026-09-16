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

pub mod cli_config;
pub mod commands;
pub mod env_map;
pub mod error;
pub mod flags;
pub mod help;
pub mod output;
pub mod probe;
pub mod regions;
pub mod runtime_policy;

use ores_clis_core::{parse_shared_argv, CliPolicy, EnvironmentHints, TerminalState};

pub use env_map::{env_value, get_env_map, EnvMap};
pub use error::CliError;
pub use output::{Format, Report};
pub use regions::{
    closest, median, parse_regions, rank, select_regions, truthy, Region, RegionLatency,
};

pub const PROGRAM: &str = "fiducia";

pub fn run(argv: &[String]) -> i32 {
    let terminals = TerminalState::detect();
    let environment = EnvironmentHints::detect();
    let shared = match parse_shared_argv(argv.iter().skip(1).cloned()) {
        Ok(shared) => shared,
        Err(error) => {
            runtime_policy::install(CliPolicy::default().resolve(terminals, environment));
            output::emit_error(format!("{PROGRAM}: {error}"));
            return 2;
        }
    };
    let runtime = shared.policy.resolve(terminals, environment);
    runtime_policy::install(runtime);

    let legacy_output_explicit = std::env::var_os("FIDUCIA_JSON").is_some()
        || shared
            .passthrough
            .iter()
            .any(|arg| arg == "-j" || arg.starts_with("--json="));
    let consumer_argv = std::iter::once(
        argv.first()
            .cloned()
            .unwrap_or_else(|| PROGRAM.to_owned()),
    )
    .chain(shared.passthrough.iter().cloned())
    .collect::<Vec<_>>();
    let argv = consumer_argv.as_slice();

    let config_path = match flags::resolve_config_path() {
        Ok(path) => path,
        Err(error) => return report(&CliError::config(error), None, argv),
    };

    if help::is_help_requested(argv) {
        return match help::help_table(&config_path, PROGRAM, argv) {
            Ok(table) => match output::emit_informational(&table) {
                Ok(()) => 0,
                Err(error) => report(&error, None, argv),
            },
            Err(error) => report(&error, None, argv),
        };
    }

    let parsed = match flags::parse_cli_args(argv, &config_path) {
        Ok(args) => args,
        Err(error) => return report(&CliError::usage(error), Some(&config_path), argv),
    };
    let args = if shared.output_was_explicit() || !legacy_output_explicit {
        flags::CliArgs {
            json: runtime.json(),
            ..parsed
        }
    } else {
        parsed
    };

    match commands::dispatch(&args, &config_path) {
        Ok(code) => code,
        Err(error) => report(&error, Some(&config_path), argv),
    }
}

fn report(error: &CliError, config_path: Option<&std::path::Path>, argv: &[String]) -> i32 {
    output::emit_error(format!("{PROGRAM}: {error}"));
    if error.wants_help() {
        if let Some(config_path) = config_path {
            if let Ok(table) = help::help_table(config_path, PROGRAM, argv) {
                output::emit_diagnostic_text(&format!("\n{table}"));
            }
        }
    }
    error.exit_code()
}
