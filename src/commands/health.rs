//! `fiducia health` — ask one node for its health and status.
//!
//! This is the command that goes through `fiducia-clients` rather than raw
//! HTTP: the org's Rust client owns the request shapes, retry policy, and
//! header contract, so the CLI does not re-implement any of it. The zed
//! dependency in `.zpkg.toml` and the Cargo edge in `Cargo.toml` describe the
//! same edge.

use fiducia_client::FiduciaClient;
use serde::Serialize;
use serde_json::Value;

use crate::error::CliError;
use crate::flags::CliArgs;
use crate::output::{emit, Format, Report};

#[derive(Debug, Serialize)]
pub struct NodeHealth {
    /// The region name when the URL came from the regions file, else `None`.
    pub region: Option<String>,
    pub url: String,
    pub health: Value,
    pub status: Value,
}

impl Report for NodeHealth {
    fn render_human(&self) -> String {
        let label = self.region.as_deref().unwrap_or("(direct url)");
        format!(
            "node    {label}\nurl     {}\nhealth  {}\nstatus  {}",
            self.url,
            compact(&self.health),
            compact(&self.status),
        )
    }
}

pub fn run(args: &CliArgs) -> Result<i32, CliError> {
    let (region, url) = args.resolve_node()?;
    let client = FiduciaClient::new(&url);

    let health = client
        .health()
        .map_err(|error| request_failed("health", &url, &error))?;
    let status = client
        .status()
        .map_err(|error| request_failed("status", &url, &error))?;

    let report = NodeHealth {
        region,
        url,
        health,
        status,
    };
    emit(&report, Format::from_json_flag(args.json))
}

/// One-line JSON so the human table stays a table.
fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".into())
}

/// `fiducia_client::Error` is deliberately not `Display`, and its `Http` variant
/// carries the whole response body. Reporting only the status keeps a failing
/// health check from spilling a body — which may hold tokens or org data — into
/// stderr and CI logs.
fn request_failed(what: &str, url: &str, error: &fiducia_client::Error) -> CliError {
    let detail = match error {
        fiducia_client::Error::Http { status, .. } => format!("HTTP {status}"),
        fiducia_client::Error::Transport(message) => format!("transport error: {message}"),
    };
    CliError::runtime(format!("{what} request to {url} failed: {detail}"))
}
