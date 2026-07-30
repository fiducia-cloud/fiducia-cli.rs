//! Cross-platform flags2env enforcement for the actual `fiducia` binary.
//!
//! The shell launcher remains a compatibility convenience, but direct binary
//! execution now uses the same audited contract on Linux, macOS, and Windows.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use flags2env::BundledFlags2Env;

use crate::cli_config::CliConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliArgs {
    pub command: String,
    pub regions_file: String,
    pub samples: usize,
    pub health_path: String,
    pub timeout_ms: u64,
    pub warmup: usize,
    pub only_region: String,
    pub json: bool,
}

pub fn resolve_config_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("FIDUCIA_FLAGS_CONFIG").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        return path
            .is_file()
            .then_some(path)
            .ok_or_else(|| "FIDUCIA_FLAGS_CONFIG does not point to a readable file".to_owned());
    }

    let mut candidates = Vec::new();
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join(".cli-flags.toml"));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join(".cli-flags.toml"));
            candidates.push(parent.join("../share/fiducia-cli/.cli-flags.toml"));
        }
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            "cannot locate .cli-flags.toml; set FIDUCIA_FLAGS_CONFIG to its path".to_owned()
        })
}

pub fn parse_cli_args(argv: &[String], config_path: &Path) -> Result<CliArgs, String> {
    let environment = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)));
    parse_cli_args_with_env(argv, config_path, environment)
}

fn parse_cli_args_with_env(
    argv: &[String],
    config_path: &Path,
    environment: impl IntoIterator<Item = (String, String)>,
) -> Result<CliArgs, String> {
    let config_path = config_path
        .to_str()
        .ok_or_else(|| ".cli-flags.toml path is not valid UTF-8".to_owned())?;
    let parser = BundledFlags2Env::new();
    parser
        .audit_config(Some(config_path))
        .map_err(|error| format!("flags-2-env configuration audit failed: {error}"))?;
    let parsed = parser
        .parse_structured(argv, Some(config_path))
        .map_err(|error| format!("flags-2-env parse failed: {error}"))?;

    if !parsed.unknown_options.is_empty() {
        let option_names = parsed
            .unknown_options
            .iter()
            .map(|option| diagnostic_option_name(option))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("unknown command-line option(s): {option_names}"));
    }
    if !parsed.errors.is_empty() {
        return Err(format!(
            "invalid command-line value(s): {}",
            parsed.errors.join("; ")
        ));
    }
    if !parsed.extras.is_empty() {
        return Err(format!(
            "unknown command or unexpected positional argument(s): {}",
            parsed.extras.len()
        ));
    }

    let mut raw_config = environment.into_iter().collect::<HashMap<_, _>>();
    raw_config.remove("FLAGS2ENV_COMMAND");
    raw_config.extend(parsed.provided_flags);
    let typed = parser
        .coerce::<CliConfig, _>(&raw_config, Some(config_path))
        .map_err(|error| format!("invalid typed configuration: {error}"))?;

    let command = match typed.FLAGS2ENV_COMMAND.as_deref() {
        None | Some("") => "region".to_owned(),
        Some("region") => "region".to_owned(),
        Some("regions") => "regions".to_owned(),
        Some(_) => return Err("flags-2-env resolved an unsupported command".to_owned()),
    };
    let regions_file = typed.FIDUCIA_REGIONS_FILE;
    if regions_file.trim().is_empty() {
        return Err("--regions must not be empty".to_owned());
    }
    let samples = bounded_usize(typed.FIDUCIA_SAMPLES, "FIDUCIA_SAMPLES", 1, 100)?;
    let health_path = typed.FIDUCIA_HEALTH_PATH;
    if !health_path.starts_with('/') || health_path.chars().any(char::is_control) {
        return Err("--path must be an absolute HTTP path without control characters".to_owned());
    }
    let timeout_ms = bounded_u64(typed.FIDUCIA_TIMEOUT_MS, "FIDUCIA_TIMEOUT_MS", 1, 60_000)?;
    let warmup = bounded_usize(typed.FIDUCIA_WARMUP, "FIDUCIA_WARMUP", 0, 100)?;
    let only_region = typed.FIDUCIA_ONLY_REGION.unwrap_or_default();
    let json = typed.FIDUCIA_JSON;

    Ok(CliArgs {
        command,
        regions_file,
        samples,
        health_path,
        timeout_ms,
        warmup,
        only_region,
        json,
    })
}

fn diagnostic_option_name(option: &str) -> String {
    if let Some(long) = option.strip_prefix("--") {
        return format!("--{}", long.split('=').next().unwrap_or_default());
    }
    option.chars().take(2).collect()
}

fn bounded_usize(value: i64, name: &str, min: usize, max: usize) -> Result<usize, String> {
    let parsed =
        usize::try_from(value).map_err(|_| format!("{name} must be a non-negative integer"))?;
    (min..=max)
        .contains(&parsed)
        .then_some(parsed)
        .ok_or_else(|| format!("{name} must be between {min} and {max}"))
}

fn bounded_u64(value: i64, name: &str, min: u64, max: u64) -> Result<u64, String> {
    let parsed =
        u64::try_from(value).map_err(|_| format!("{name} must be a non-negative integer"))?;
    (min..=max)
        .contains(&parsed)
        .then_some(parsed)
        .ok_or_else(|| format!("{name} must be between {min} and {max}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".cli-flags.toml")
    }

    fn parse(argv: &[String], environment: &[(&str, &str)]) -> Result<CliArgs, String> {
        parse_cli_args_with_env(
            argv,
            &config_path(),
            environment
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned())),
        )
    }

    #[test]
    fn parses_direct_binary_flags_and_command() {
        let argv = vec![
            "fiducia".to_owned(),
            "region".to_owned(),
            "--samples=7".to_owned(),
            "--timeout=1500".to_owned(),
            "--json".to_owned(),
        ];
        let parsed = parse(&argv, &[]).expect("valid flags");
        assert_eq!(parsed.command, "region");
        assert_eq!(parsed.samples, 7);
        assert_eq!(parsed.timeout_ms, 1500);
        assert!(parsed.json);
    }

    #[test]
    fn canonicalizes_the_closest_command_alias() {
        let argv = vec!["fiducia".to_owned(), "closest".to_owned()];
        let parsed = parse(&argv, &[]).expect("valid alias");
        assert_eq!(parsed.command, "region");
    }

    #[test]
    fn environment_cannot_spoof_parser_command_metadata() {
        let argv = vec!["fiducia".to_owned()];
        let parsed = parse(&argv, &[("FLAGS2ENV_COMMAND", "regions")]).expect("default command");
        assert_eq!(parsed.command, "region");
    }

    #[test]
    fn environment_beats_schema_defaults() {
        let argv = vec!["fiducia".to_owned(), "region".to_owned()];
        let parsed = parse(&argv, &[("FIDUCIA_SAMPLES", "9")]).expect("valid environment");
        assert_eq!(parsed.samples, 9);
    }

    #[test]
    fn cli_flags_beat_environment_values() {
        let argv = vec![
            "fiducia".to_owned(),
            "region".to_owned(),
            "--samples=7".to_owned(),
        ];
        let parsed = parse(&argv, &[("FIDUCIA_SAMPLES", "9")]).expect("valid override");
        assert_eq!(parsed.samples, 7);
    }

    #[test]
    fn declared_boolean_environment_aliases_are_coerced() {
        let argv = vec!["fiducia".to_owned(), "regions".to_owned()];
        let parsed = parse(&argv, &[("FIDUCIA_JSON", "1")]).expect("valid boolean alias");
        assert!(parsed.json);
    }

    #[test]
    fn unknown_flags_fail_closed() {
        let argv = vec![
            "fiducia".to_owned(),
            "region".to_owned(),
            "--api-token=must-remain-environment-only".to_owned(),
        ];
        let error = parse(&argv, &[]).expect_err("unknown flag");
        assert!(error.contains("unknown command-line option"));
        assert!(error.contains("--api-token"));
        assert!(!error.contains("must-remain-environment-only"));
    }

    #[test]
    fn unsafe_probe_values_are_rejected() {
        let argv = vec![
            "fiducia".to_owned(),
            "region".to_owned(),
            "--samples=0".to_owned(),
        ];
        assert!(parse(&argv, &[]).is_err());
    }

    #[test]
    fn invalid_environment_values_get_toml_guidance_without_reflection() {
        let argv = vec!["fiducia".to_owned(), "regions".to_owned()];
        let error = parse(&argv, &[("FIDUCIA_SAMPLES", "do-not-reflect-this")])
            .expect_err("invalid typed environment");
        assert!(error.contains("flags.samples"));
        assert!(error.contains("type = \"integer\""));
        assert!(!error.contains("do-not-reflect-this"));
    }
}
