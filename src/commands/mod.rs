//! The command vocabulary shared by dispatch and help-completeness tests.

pub mod completion;
pub mod health;
pub mod region;
pub mod regions;
pub mod version;

use crate::error::CliError;
use crate::flags::CliArgs;
use crate::output::{self, Format};
use std::path::Path;

/// Every supported subcommand. Keep this list aligned with `.cli-flags.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Region,
    Regions,
    Health,
    Completion,
    Version,
}

impl Command {
    pub const ALL: [Self; 5] = [
        Self::Region,
        Self::Regions,
        Self::Health,
        Self::Completion,
        Self::Version,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Region => "region",
            Self::Regions => "regions",
            Self::Health => "health",
            Self::Completion => "completion",
            Self::Version => "version",
        }
    }

    pub fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "region" | "closest" => Ok(Self::Region),
            "regions" => Ok(Self::Regions),
            "health" => Ok(Self::Health),
            "completion" => Ok(Self::Completion),
            "version" => Ok(Self::Version),
            other => Err(CliError::usage(format!("unknown command: {other}"))),
        }
    }
}

/// Dispatches one validated command.
pub fn dispatch(args: &CliArgs, config_path: &Path) -> Result<i32, CliError> {
    let format = Format::from_json_flag(args.json);
    match args.command {
        Command::Region => output::emit(&region::run(args)?, format),
        Command::Regions => output::emit(&regions::run(args)?, format),
        Command::Health => output::emit(&health::run(args)?, format),
        Command::Completion => completion::run(args, config_path),
        Command::Version => output::emit(&version::run(), format),
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn command_enum_matches_contract_tables() {
        let config = include_str!("../../.cli-flags.toml");
        for command in Command::ALL {
            let table = format!("[commands.{}]", command.name());
            assert!(
                config.lines().any(|line| line.trim() == table),
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
        assert!(matches!(Command::parse("closest"), Ok(Command::Region)));
        assert!(matches!(Command::parse("region"), Ok(Command::Region)));
    }

    #[test]
    fn unknown_commands_are_usage_errors() {
        let error = Command::parse("deploy").unwrap_err();
        assert_eq!(error.exit_code(), 2);
    }
}
