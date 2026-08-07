//! One module per subcommand, plus the dispatch table.
//!
//! Which command ran is decided by flags-2-env from the `[commands.*]` tables
//! in `.cli-flags.toml` — never by hand-matching argv here. [`Command`] is the
//! closed set that `.cli-flags.toml` may resolve to; adding a command means
//! adding it in both places, and the `command_set_matches_config` test fails
//! until you do.

pub mod completion;
pub mod health;
pub mod region;
pub mod regions;

use std::path::Path;

use crate::error::CliError;
use crate::flags::CliArgs;

/// The commands `.cli-flags.toml` can resolve to, after alias canonicalisation
/// (`closest` → `Region`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// List the selectable regions.
    Regions,
    /// Probe the regions and pick the closest.
    Region,
    /// Ask one region's node for its health and status.
    Health,
    /// Print a shell completion script.
    Completion,
}

impl Command {
    /// The canonical `[commands.*]` key, which is also what flags-2-env puts in
    /// `FLAGS2ENV_COMMAND`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regions => "regions",
            Self::Region => "region",
            Self::Health => "health",
            Self::Completion => "completion",
        }
    }

    /// Resolves the label flags-2-env reported. Aliases are already collapsed
    /// by the parser except for the command path itself, so `closest` is mapped
    /// here to keep one canonical spelling downstream.
    pub fn parse(label: &str) -> Result<Self, CliError> {
        match label {
            "regions" => Ok(Self::Regions),
            "region" | "closest" => Ok(Self::Region),
            "health" => Ok(Self::Health),
            "completion" => Ok(Self::Completion),
            "" => Err(CliError::usage(
                "no command given; expected one of: regions, region, health, completion",
            )),
            other => Err(CliError::usage(format!("unsupported command {other:?}"))),
        }
    }

    /// Every command, for the config-parity test and for diagnostics.
    pub const ALL: [Self; 4] = [Self::Regions, Self::Region, Self::Health, Self::Completion];
}

/// Runs the selected command and returns its exit code.
pub fn dispatch(args: &CliArgs, config_path: &Path) -> Result<i32, CliError> {
    match args.command {
        Command::Regions => regions::run(args),
        Command::Region => region::run(args),
        Command::Health => health::run(args),
        Command::Completion => completion::run(args, config_path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_set_matches_config() {
        // Guards the two-places problem: a `[commands.x]` table with no
        // `Command::X` would be accepted by the parser and then rejected at
        // dispatch, which is a confusing runtime failure rather than a build
        // failure.
        let config = include_str!("../../.cli-flags.toml");
        for command in Command::ALL {
            let table = format!("[commands.{}]", command.as_str());
            assert!(
                config.contains(&table),
                "{table} is missing from .cli-flags.toml"
            );
        }

        let declared = config
            .lines()
            .filter_map(|line| line.trim().strip_prefix("[commands."))
            .filter_map(|line| line.strip_suffix(']'))
            // Ignore nested tables such as `[commands.x.flags.y]`.
            .filter(|name| !name.contains('.'))
            .count();
        assert_eq!(
            declared,
            Command::ALL.len(),
            ".cli-flags.toml declares {declared} commands but Command::ALL has {}",
            Command::ALL.len()
        );
    }

    #[test]
    fn closest_is_an_alias_for_region() {
        assert_eq!(Command::parse("closest").unwrap(), Command::Region);
        assert_eq!(Command::parse("region").unwrap(), Command::Region);
    }

    #[test]
    fn unknown_commands_are_usage_errors() {
        let error = Command::parse("deploy").unwrap_err();
        assert_eq!(error.exit_code(), 2);
    }
}
