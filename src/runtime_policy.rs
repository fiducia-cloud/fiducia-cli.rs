//! Process-level shared CLI runtime policy.

use std::sync::OnceLock;

use ores_clis_core::{CliPolicy, EnvironmentHints, RuntimePolicy, TerminalState};

static RUNTIME: OnceLock<RuntimePolicy> = OnceLock::new();

pub fn install(runtime: RuntimePolicy) {
    let _ = RUNTIME.set(runtime);
}

#[must_use]
pub fn current() -> RuntimePolicy {
    RUNTIME.get().copied().unwrap_or_else(|| {
        CliPolicy::default().resolve(TerminalState::detect(), EnvironmentHints::detect())
    })
}
