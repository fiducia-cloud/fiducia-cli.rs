//! Offline deterministic API/MCP documentation generation through ores-api-docs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use ores_api_docs::{Catalog, PublicationMode, RouteMap, render_docs_publication};
use serde::{Deserialize, Serialize};

use crate::error::CliError;
use crate::flags::CliArgs;
use crate::output::{self, Format, Report};

const CONFIG_FILE: &str = ".api-docs.toml";
const CONFIG_SCHEMA_VERSION: &str = "ores.api-docs.publication-config.v1";

#[derive(Clone, Debug, Deserialize)]
struct DocsConfig {
    schema_version: String,
    route_map: String,
    out_dir: String,
    publication_mode: String,
    producer: String,
}

#[derive(Debug, Serialize)]
struct DocsReport {
    service: String,
    publication_mode: String,
    producer: String,
    contract_sha256: String,
    artifact_count: usize,
    out_dir: String,
}

impl Report for DocsReport {
    fn render_human(&self) -> String {
        return format!(
            "generated {} deterministic API/MCP docs artifacts for {} ({}) in {}",
            self.artifact_count, self.service, self.publication_mode, self.out_dir
        );
    }
}

pub fn run(args: &CliArgs) -> Result<i32, CliError> {
    let project_root = std::env::current_dir()
        .map_err(|error| CliError::runtime(format!("cannot resolve current project directory: {error}")))?;
    let config_path = project_root.join(CONFIG_FILE);
    let config_text = fs::read_to_string(&config_path).map_err(|error| {
        CliError::config(format!(
            "cannot read {}: {error}; add a project-local {}",
            config_path.display(),
            CONFIG_FILE
        ))
    })?;
    let config: DocsConfig = toml::from_str(&config_text)
        .map_err(|error| CliError::config(format!("invalid {}: {error}", config_path.display())))?;
    validate_config(&config)?;

    let route_map_path = resolve_project_path(&project_root, &config.route_map, "route_map")?;
    let out_dir = resolve_project_path(&project_root, &config.out_dir, "out_dir")?;
    let route_map_json = fs::read_to_string(&route_map_path).map_err(|error| {
        CliError::runtime(format!(
            "cannot read route map {}: {error}",
            route_map_path.display()
        ))
    })?;
    let route_map = RouteMap::from_json_str(&route_map_json)
        .map_err(|error| CliError::config(format!("invalid route map: {error}")))?;
    let catalog = Catalog::from_map_with_language(route_map, None)
        .map_err(|error| CliError::config(format!("cannot build api-docs catalog: {error}")))?;
    let mode = publication_mode(&config.publication_mode)?;
    let bundle = render_docs_publication(&catalog, mode, &config.producer)
        .map_err(|error| CliError::config(format!("cannot render api-docs bundle: {error}")))?;

    replace_bundle_tree(&out_dir, &bundle.files)?;

    let report = DocsReport {
        service: bundle.manifest.service.clone(),
        publication_mode: bundle.manifest.publication_mode.as_str().to_owned(),
        producer: bundle.manifest.producer.clone(),
        contract_sha256: bundle.manifest.contract_sha256.clone(),
        artifact_count: bundle.files.len(),
        out_dir: out_dir.display().to_string(),
    };

    return output::emit(&report, Format::from_json_flag(args.json));
}

fn validate_config(config: &DocsConfig) -> Result<(), CliError> {
    if config.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(CliError::config(format!(
            "{} schema_version must be {CONFIG_SCHEMA_VERSION}",
            CONFIG_FILE
        )));
    }
    publication_mode(&config.publication_mode)?;
    validate_relative_path(&config.route_map, "route_map")?;
    validate_relative_path(&config.out_dir, "out_dir")?;
    if config.producer.trim().is_empty() {
        return Err(CliError::config("producer must not be empty"));
    }

    return Ok(());
}

fn publication_mode(value: &str) -> Result<PublicationMode, CliError> {
    match value {
        "publisher_external" => {
            return Ok(PublicationMode::PublisherExternal);
        }
        "consumer_project" => {
            return Ok(PublicationMode::ConsumerProject);
        }
        _ => {
            return Err(CliError::config(
                "publication_mode must be publisher_external or consumer_project",
            ));
        }
    }
}

fn validate_relative_path(configured: &str, name: &str) -> Result<(), CliError> {
    let path = Path::new(configured);
    let has_normal_component = path
        .components()
        .any(|component| matches!(component, Component::Normal(_)));
    let escapes_project = path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });

    if configured.trim().is_empty() || !has_normal_component || escapes_project {
        return Err(CliError::config(format!(
            "{name} must be a non-empty project-relative path without parent traversal"
        )));
    }

    return Ok(());
}

fn resolve_project_path(
    project_root: &Path,
    configured: &str,
    name: &str,
) -> Result<PathBuf, CliError> {
    validate_relative_path(configured, name)?;
    return Ok(project_root.join(configured));
}

fn replace_bundle_tree(
    out_dir: &Path,
    files: &BTreeMap<String, String>,
) -> Result<(), CliError> {
    if out_dir.exists() {
        let metadata = fs::symlink_metadata(out_dir).map_err(|error| {
            CliError::runtime(format!("cannot inspect {}: {error}", out_dir.display()))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(CliError::config(format!(
                "out_dir must not be a symbolic link: {}",
                out_dir.display()
            )));
        }
        if !metadata.is_dir() {
            return Err(CliError::config(format!(
                "out_dir must resolve to a directory: {}",
                out_dir.display()
            )));
        }
        fs::remove_dir_all(out_dir).map_err(|error| {
            CliError::runtime(format!(
                "cannot clear deterministic output directory {}: {error}",
                out_dir.display()
            ))
        })?;
    }

    fs::create_dir_all(out_dir).map_err(|error| {
        CliError::runtime(format!("cannot create {}: {error}", out_dir.display()))
    })?;

    for (relative, content) in files {
        validate_relative_path(relative, "generated artifact path")?;
        let destination = out_dir.join(relative);
        let Some(parent) = destination.parent() else {
            return Err(CliError::runtime(format!(
                "cannot resolve parent directory for {}",
                destination.display()
            )));
        };
        fs::create_dir_all(parent).map_err(|error| {
            CliError::runtime(format!("cannot create {}: {error}", parent.display()))
        })?;
        fs::write(&destination, content).map_err(|error| {
            CliError::runtime(format!("cannot write {}: {error}", destination.display()))
        })?;
    }

    return Ok(());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_modes_are_explicit_and_closed() {
        assert_eq!(
            publication_mode("publisher_external").expect("publisher mode"),
            PublicationMode::PublisherExternal
        );
        assert_eq!(
            publication_mode("consumer_project").expect("consumer mode"),
            PublicationMode::ConsumerProject
        );
        assert!(publication_mode("official").is_err());
    }

    #[test]
    fn project_paths_reject_escape_and_project_root() {
        let root = Path::new("project");
        assert_eq!(
            resolve_project_path(root, "contracts/api.route-map.json", "route_map")
                .expect("relative path"),
            PathBuf::from("project/contracts/api.route-map.json")
        );
        assert!(resolve_project_path(root, "../outside.json", "route_map").is_err());
        assert!(resolve_project_path(root, "/tmp/out", "out_dir").is_err());
        assert!(resolve_project_path(root, ".", "out_dir").is_err());
    }
}
