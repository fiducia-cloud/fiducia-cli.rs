//! Human tables versus `--json`.
//!
//! Every command returns a value that knows both renderings, so the `--json`
//! branch is decided once here instead of in each command body.

use std::io;

use ores_clis_core::{
    ColorRole, EmitDisposition, ProtocolEmitter, StreamRole, paint, top_level_io,
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
        if json { Self::Json } else { Self::Human }
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
    let mut output = ProtocolEmitter::new(stdout.lock(), StreamRole::Primary);
    let write = match format {
        Format::Human => {
            let role = if exit_code == 0 { ColorRole::Success } else { ColorRole::Error };
            output.emit_primary_human_line(&paint(
                runtime.color_stdout(),
                role,
                report.render_human(),
            ))
        }
        Format::Json => {
            let encoded = serde_json::to_string(report)?;
            output.emit_primary_machine_record(&encoded)
        }
    };

    match top_level_io(write).map_err(|error| {
        CliError::runtime(format!("could not write command output: {error}"))
    })? {
        EmitDisposition::Written | EmitDisposition::ConsumerClosed => Ok(exit_code),
    }
}
