//! Human tables versus `--json`.
//!
//! Every command returns a value that knows both renderings, so the `--json`
//! branch is decided once here instead of in each command body.

use std::io;

use ores_clis_core::{
    paint, top_level_io, ColorRole, EmitDisposition, LogLevel, ProtocolEmitter, StreamRole,
};
use serde::Serialize;

use crate::error::CliError;
use crate::runtime_policy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Human,
    Json,
}

impl Format {
    pub fn from_json_flag(json: bool) -> Self {
        if json {
            Self::Json
        } else {
            Self::Human
        }
    }
}

pub trait Report: Serialize {
    fn render_human(&self) -> String;

    fn exit_code(&self) -> i32 {
        0
    }
}

pub fn emit<R: Report>(report: &R, format: Format) -> Result<i32, CliError> {
    let runtime = runtime_policy::current();
    let exit_code = report.exit_code();
    let stdout = io::stdout();
    let write = match format {
        Format::Human => {
            let role = if exit_code == 0 {
                ColorRole::Success
            } else {
                ColorRole::Error
            };
            ProtocolEmitter::new(stdout.lock(), StreamRole::Primary).emit_primary_human_line(
                &paint(runtime.color_stdout(), role, report.render_human()),
            )
        }
        Format::Json => {
            let encoded = serde_json::to_string(report)?;
            ProtocolEmitter::new(stdout.lock(), StreamRole::Primary)
                .emit_primary_machine_record(&encoded)
        }
    };

    match top_level_io(write)
        .map_err(|error| CliError::runtime(format!("could not write command output: {error}")))?
    {
        EmitDisposition::Written | EmitDisposition::ConsumerClosed => Ok(exit_code),
    }
}

pub fn emit_informational(value: &str) -> Result<(), CliError> {
    let stdout = io::stdout();
    match top_level_io(
        ProtocolEmitter::new(stdout.lock(), StreamRole::Primary)
            .emit_primary_human_line(value.trim_end_matches('\n')),
    )
    .map_err(|error| CliError::runtime(format!("could not write informational output: {error}")))?
    {
        EmitDisposition::Written | EmitDisposition::ConsumerClosed => Ok(()),
    }
}

pub fn emit_error(value: impl std::fmt::Display) {
    let runtime = runtime_policy::current();
    if !runtime.allows_log(LogLevel::Error) {
        return;
    }
    let stderr = io::stderr();
    let line = paint(runtime.color_stderr(), ColorRole::Error, value);
    let _ = top_level_io(
        ProtocolEmitter::new(stderr.lock(), StreamRole::Diagnostics).emit_diagnostic_line(&line),
    );
}

pub fn emit_diagnostic_text(value: &str) {
    let runtime = runtime_policy::current();
    if !runtime.allows_log(LogLevel::Error) {
        return;
    }
    let stderr = io::stderr();
    let _ = top_level_io(
        ProtocolEmitter::new(stderr.lock(), StreamRole::Diagnostics)
            .emit_diagnostic_line(value.trim_end_matches('\n')),
    );
}
