//! Cross-platform flags2env enforcement for the actual `fiducia` binary.
//!
//! The shell launcher remains a compatibility convenience, but direct binary
//! execution now uses the same audited contract on Linux, macOS, and Windows.

use std::path::{Path, PathBuf};

use flags2env::BundledFlags2Env;

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
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        candidates.push(parent.join(".cli-flags.toml"));
        candidates.push(parent.join("../share/fiducia-cli/.cli-flags.toml"));
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            "cannot locate .cli-flags.toml; set FIDUCIA_FLAGS_CONFIG to its path".to_owned()
        })
}

pub fn parse_cli_args(argv: &[String], config_path: &Path) -> Result<CliArgs, String> {
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
        return Err(format!(
            "unknown command-line option(s): {}",
            parsed.unknown_options.join(", ")
        ));
    }
    if !parsed.errors.is_empty() {
        return Err(format!(
            "invalid command-line value(s): {}",
            parsed.errors.join("; ")
        ));
    }
    if parsed.extras.len() > 1 {
        return Err(format!(
            "unexpected positional argument(s): {}",
            parsed.extras[1..].join(", ")
        ));
    }

    let command = parsed
        .extras
        .first()
        .cloned()
        .unwrap_or_else(|| "region".to_owned());
    if !matches!(command.as_str(), "region" | "regions" | "closest") {
        return Err(format!("unknown command: {command}"));
    }

    let regions_file = value(&parsed.flags, "FIDUCIA_REGIONS_FILE", "edge-regions.json");
    if regions_file.trim().is_empty() {
        return Err("--regions must not be empty".to_owned());
    }
    let samples = bounded_usize(&parsed.flags, "FIDUCIA_SAMPLES", 5, 1, 100)?;
    let health_path = value(&parsed.flags, "FIDUCIA_HEALTH_PATH", "/healthz");
    if !health_path.starts_with('/') || health_path.chars().any(char::is_control) {
        return Err("--path must be an absolute HTTP path without control characters".to_owned());
    }
    let timeout_ms = bounded_u64(&parsed.flags, "FIDUCIA_TIMEOUT_MS", 2_000, 1, 60_000)?;
    let warmup = bounded_usize(&parsed.flags, "FIDUCIA_WARMUP", 0, 0, 100)?;
    let only_region = value(&parsed.flags, "FIDUCIA_ONLY_REGION", "");
    let json = parsed
        .flags
        .get("FIDUCIA_JSON")
        .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "yes"));

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

fn value(flags: &std::collections::HashMap<String, String>, name: &str, default: &str) -> String {
    flags
        .get(name)
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

fn bounded_usize(
    flags: &std::collections::HashMap<String, String>,
    name: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, String> {
    let value = flags
        .get(name)
        .map(String::as_str)
        .unwrap_or("")
        .trim();
    let parsed = if value.is_empty() {
        default
    } else {
        value
            .parse::<usize>()
            .map_err(|_| format!("{name} must be an integer"))?
    };
    (min..=max)
        .contains(&parsed)
        .then_some(parsed)
        .ok_or_else(|| format!("{name} must be between {min} and {max}"))
}

fn bounded_u64(
    flags: &std::collections::HashMap<String, String>,
    name: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, String> {
    let value = flags
        .get(name)
        .map(String::as_str)
        .unwrap_or("")
        .trim();
    let parsed = if value.is_empty() {
        default
    } else {
        value
            .parse::<u64>()
            .map_err(|_| format!("{name} must be an integer"))?
    };
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

    #[test]
    fn parses_direct_binary_flags_and_command() {
        let argv = vec![
            "fiducia".to_owned(),
            "region".to_owned(),
            "--samples=7".to_owned(),
            "--timeout=1500".to_owned(),
            "--json".to_owned(),
        ];
        let parsed = parse_cli_args(&argv, &config_path()).expect("valid flags");
        assert_eq!(parsed.command, "region");
        assert_eq!(parsed.samples, 7);
        assert_eq!(parsed.timeout_ms, 1500);
        assert!(parsed.json);
    }

    #[test]
    fn unknown_flags_fail_closed() {
        let argv = vec![
            "fiducia".to_owned(),
            "region".to_owned(),
            "--api-token=must-remain-environment-only".to_owned(),
        ];
        let error = parse_cli_args(&argv, &config_path()).expect_err("unknown flag");
        assert!(error.contains("unknown command-line option"));
    }

    #[test]
    fn unsafe_probe_values_are_rejected() {
        let argv = vec![
            "fiducia".to_owned(),
            "region".to_owned(),
            "--samples=0".to_owned(),
        ];
        assert!(parse_cli_args(&argv, &config_path()).is_err());
    }
}
