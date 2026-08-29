//! argv + environment → a validated [`CliArgs`], through flags-2-env.
//!
//! The order of operations matters and is the same in every one of our CLIs:
//!
//! 1. **audit** `.cli-flags.toml` — a malformed contract is a config error, not
//!    a mysterious parse failure later;
//! 2. **`parse_structured`** — this returns argv-derived values, the resolved
//!    command, and the diagnostic channels *separately*, so a real environment
//!    variable can never be mistaken for something the user typed;
//! 3. **fail closed** on unknown options, invalid values, and stray operands;
//! 4. **layer** schema defaults < process environment < argv, then hand the
//!    merged map to `coerce` for typed conversion;
//! 5. **range-check** the typed values here, where the message can name the
//!    flag.
//!
//! Step 4 is why `provided_flags` is used rather than `flags`: `flags` carries
//! TOML defaults, and spreading those over the real environment would let a
//! default silently beat an env var the operator set.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use flags2env::BundledFlags2Env;

use crate::cli_config::CliConfig;
use crate::commands::Command;
use crate::env_map::{EnvMap, env_value, get_env_map};
use crate::error::CliError;
use crate::help::SUPPORTED_SHELLS;
use crate::probe::ProbeSettings;
use crate::regions::{parse_regions, select_regions, Region};

/// Everything a command needs, already validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliArgs {
    pub command: Command,
    pub regions_file: String,
    pub samples: usize,
    pub health_path: String,
    pub timeout_ms: u64,
    pub warmup: usize,
    /// Empty means "every region".
    pub only_region: String,
    pub json: bool,
    /// `health --url`: a node URL that bypasses the regions file.
    pub node_url: Option<String>,
    /// `completion --shell`.
    pub shell: String,
    /// Process env copied at parse time, then layered with argv-provided flags.
    pub env: EnvMap,
}

impl CliArgs {
    /// Reads and validates the regions file, applying `--only`.
    pub fn load_regions(&self) -> Result<Vec<Region>, CliError> {
        let json = std::fs::read_to_string(&self.regions_file).map_err(|error| {
            CliError::runtime(format!(
                "cannot read regions file {}: {error}",
                self.regions_file
            ))
        })?;
        let regions = parse_regions(&json)
            .map_err(|error| CliError::config(format!("invalid regions file: {error}")))?;
        select_regions(regions, &self.only_region).map_err(CliError::usage)
    }

    pub fn probe_settings(&self) -> ProbeSettings {
        ProbeSettings {
            health_path: self.health_path.clone(),
            samples: self.samples,
            warmup: self.warmup,
            timeout: Duration::from_millis(self.timeout_ms),
        }
    }

    /// Picks the single node `health` should talk to.
    ///
    /// `--url` wins outright. Otherwise the regions file must narrow to exactly
    /// one region, because silently probing the first of several would make the
    /// answer depend on file order.
    pub fn resolve_node(&self) -> Result<(Option<String>, String), CliError> {
        match self
            .node_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            Some(url) => match url.starts_with("https://") || url.starts_with("http://") {
                true => Ok((None, url.to_owned())),
                false => Err(CliError::usage("--url must start with http:// or https://")),
            },
            None => {
                let regions = self.load_regions()?;
                match regions.as_slice() {
                    [region] => Ok((Some(region.name.clone()), region.url.clone())),
                    [] => Err(CliError::usage(
                        "no regions to query; pass --url or a non-empty --regions file",
                    )),
                    many => Err(CliError::usage(format!(
                        "--only or --url is required: {} regions are selectable ({})",
                        many.len(),
                        many.iter()
                            .map(|region| region.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))),
                }
            }
        }
    }
}

/// Finds `.cli-flags.toml`: an explicit override, then the working directory,
/// then next to the installed binary (which is what makes a globally installed
/// `fiducia` work from any directory).
pub fn resolve_config_path() -> Result<PathBuf, String> {
    match std::env::var_os("FIDUCIA_FLAGS_CONFIG").filter(|value| !value.is_empty()) {
        Some(path) => {
            let path = PathBuf::from(path);
            path.is_file()
                .then_some(path)
                .ok_or_else(|| "FIDUCIA_FLAGS_CONFIG does not point to a readable file".to_owned())
        }
        None => {
            let from_cwd = std::env::current_dir()
                .ok()
                .map(|current| current.join(".cli-flags.toml"));
            let from_exe = std::env::current_exe().ok().and_then(|executable| {
                executable.parent().map(|parent| {
                    [
                        parent.join(".cli-flags.toml"),
                        parent.join("../share/fiducia-cli/.cli-flags.toml"),
                    ]
                })
            });
            from_cwd
                .into_iter()
                .chain(from_exe.into_iter().flatten())
                .find(|candidate| candidate.is_file())
                .ok_or_else(|| {
                    "cannot locate .cli-flags.toml; set FIDUCIA_FLAGS_CONFIG to its path".to_owned()
                })
        }
    }
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
        // The values are not echoed: an operand is as likely to be a secret as
        // a typo, and the count is enough to spot the mistake.
        return Err(format!(
            "unknown command or unexpected positional argument(s): {}",
            parsed.extras.len()
        ));
    }

    let mut raw_config = environment.into_iter().collect::<HashMap<_, _>>();
    // Command metadata is parser output, never operator input — an inherited
    // FLAGS2ENV_COMMAND must not be able to choose the command.
    raw_config.remove("FLAGS2ENV_COMMAND");
    let env = get_env_map(
        raw_config
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        parsed.provided_flags.clone(),
    );
    raw_config.extend(parsed.provided_flags);
    let typed = parser
        .coerce::<CliConfig, _>(&raw_config, Some(config_path))
        .map_err(|error| format!("invalid typed configuration: {error}"))?;

    let command = match typed.FLAGS2ENV_COMMAND.as_deref() {
        // No command means the default one, which keeps `fiducia -j` working.
        None | Some("") => Command::Region,
        Some(label) => Command::parse(label).map_err(|error| error.to_string())?,
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

    // Scoped defaults are only applied when their command runs, so the default
    // is restated here for the case where `completion` ran without `--shell`.
    let shell = typed
        .FIDUCIA_COMPLETION_SHELL
        .unwrap_or_else(|| "bash".to_owned());
    if command == Command::Completion && !SUPPORTED_SHELLS.contains(&shell.as_str()) {
        return Err(format!(
            "--shell must be one of: {}",
            SUPPORTED_SHELLS.join(", ")
        ));
    }

    Ok(CliArgs {
        command,
        regions_file,
        samples,
        health_path,
        timeout_ms,
        warmup,
        only_region: typed.FIDUCIA_ONLY_REGION.unwrap_or_default(),
        json: typed.FIDUCIA_JSON,
        node_url: typed.FIDUCIA_NODE_URL,
        shell,
        env,
    })
}

/// Strips any `=value` before an unknown option reaches a diagnostic, so a
/// mistyped `--api-token=secret` cannot echo the secret.
fn diagnostic_option_name(option: &str) -> String {
    match option.strip_prefix("--") {
        Some(long) => format!("--{}", long.split('=').next().unwrap_or_default()),
        None => option.chars().take(2).collect(),
    }
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

    fn argv(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|token| (*token).to_owned()).collect()
    }

    #[test]
    fn parses_direct_binary_flags_and_command() {
        let parsed = parse(
            &argv(&[
                "fiducia",
                "region",
                "--samples=7",
                "--timeout=1500",
                "--json",
            ]),
            &[],
        )
        .expect("valid flags");
        assert_eq!(parsed.command, Command::Region);
        assert_eq!(parsed.samples, 7);
        assert_eq!(parsed.timeout_ms, 1500);
        assert!(parsed.json);
        assert_eq!(env_value(&parsed.env, "FIDUCIA_SAMPLES"), Some("7"));
    }

    #[test]
    fn canonicalizes_the_closest_command_alias() {
        let parsed = parse(&argv(&["fiducia", "closest"]), &[]).expect("valid alias");
        assert_eq!(parsed.command, Command::Region);
    }

    #[test]
    fn environment_cannot_spoof_parser_command_metadata() {
        let parsed = parse(&argv(&["fiducia"]), &[("FLAGS2ENV_COMMAND", "regions")])
            .expect("default command");
        assert_eq!(parsed.command, Command::Region);
    }

    #[test]
    fn environment_beats_schema_defaults() {
        let parsed =
            parse(&argv(&["fiducia", "region"]), &[("FIDUCIA_SAMPLES", "9")]).expect("valid env");
        assert_eq!(parsed.samples, 9);
    }

    #[test]
    fn cli_flags_beat_environment_values() {
        let parsed = parse(
            &argv(&["fiducia", "region", "--samples=7"]),
            &[("FIDUCIA_SAMPLES", "9")],
        )
        .expect("valid override");
        assert_eq!(parsed.samples, 7);
    }

    #[test]
    fn declared_boolean_environment_aliases_are_coerced() {
        let parsed =
            parse(&argv(&["fiducia", "regions"]), &[("FIDUCIA_JSON", "1")]).expect("bool alias");
        assert!(parsed.json);
    }

    #[test]
    fn unknown_flags_fail_closed() {
        let error = parse(
            &argv(&[
                "fiducia",
                "region",
                "--api-token=must-remain-environment-only",
            ]),
            &[],
        )
        .expect_err("unknown flag");
        assert!(error.contains("unknown command-line option"));
        assert!(error.contains("--api-token"));
        assert!(!error.contains("must-remain-environment-only"));
    }

    #[test]
    fn unsafe_probe_values_are_rejected() {
        assert!(parse(&argv(&["fiducia", "region", "--samples=0"]), &[]).is_err());
    }

    #[test]
    fn invalid_environment_values_get_toml_guidance_without_reflection() {
        let error = parse(
            &argv(&["fiducia", "regions"]),
            &[("FIDUCIA_SAMPLES", "do-not-reflect-this")],
        )
        .expect_err("invalid typed environment");
        assert!(error.contains("flags.samples"));
        assert!(error.contains("type = \"integer\""));
        assert!(!error.contains("do-not-reflect-this"));
    }

    #[test]
    fn command_scoped_url_is_only_accepted_under_health() {
        let parsed = parse(
            &argv(&["fiducia", "health", "--url=https://node.test"]),
            &[],
        )
        .expect("scoped flag under its command");
        assert_eq!(parsed.command, Command::Health);
        assert_eq!(parsed.node_url.as_deref(), Some("https://node.test"));

        // The same flag under a different command is not in scope, so it is an
        // unknown option rather than a silently ignored one.
        let error = parse(
            &argv(&["fiducia", "regions", "--url=https://node.test"]),
            &[],
        )
        .expect_err("out-of-scope flag");
        assert!(error.contains("unknown command-line option"));
    }

    #[test]
    fn completion_shell_defaults_to_bash_and_rejects_others() {
        let parsed = parse(&argv(&["fiducia", "completion"]), &[]).expect("scoped default");
        assert_eq!(parsed.command, Command::Completion);
        assert_eq!(parsed.shell, "bash");

        assert_eq!(
            parse(&argv(&["fiducia", "completion", "--shell=zsh"]), &[])
                .expect("zsh")
                .shell,
            "zsh"
        );
        assert!(parse(&argv(&["fiducia", "completion", "--shell=fish"]), &[]).is_err());
    }

    #[test]
    fn health_requires_a_single_resolvable_node() {
        let parsed = parse(&argv(&["fiducia", "health", "--url=ftp://node.test"]), &[])
            .expect("parse succeeds; the scheme is checked at resolve time");
        assert!(parsed.resolve_node().is_err());
    }

    #[test]
    fn parse_does_not_mutate_process_environment() {
        let before = std::env::var_os("FIDUCIA_SAMPLES");
        let parsed = parse(
            &argv(&["fiducia", "region", "--samples=7"]),
            &[("FIDUCIA_SAMPLES", "9")],
        )
        .expect("valid override");
        assert_eq!(env_value(&parsed.env, "FIDUCIA_SAMPLES"), Some("7"));
        assert_eq!(std::env::var_os("FIDUCIA_SAMPLES"), before);
    }

    #[test]
    fn source_does_not_mutate_process_environment() {
        const SRC: &str = include_str!("flags.rs");
        let production = SRC.split("#[cfg(test)]").next().unwrap_or(SRC);
        assert!(!production.contains("set_var"));
    }
}
