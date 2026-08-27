//! The latency probe loop — the only network I/O behind `fiducia region`.
//!
//! Kept apart from [`crate::regions`] so the ranking rules stay pure and
//! unit-testable, and apart from [`crate::commands`] so the rendering does not
//! have to know how a measurement was taken.

use std::time::{Duration, Instant};

use crate::regions::{median, Region, RegionLatency};

/// How to probe: where to knock, how many times, and how long to wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeSettings {
    /// Absolute health path appended to each region URL (`/healthz`).
    pub health_path: String,
    /// Measured probes per region.
    pub samples: usize,
    /// Unmeasured probes run first, to prime the TCP/TLS connection.
    pub warmup: usize,
    /// Per-probe timeout.
    pub timeout: Duration,
}

/// Measures every region and returns the results ranked by median latency.
///
/// Unreachable regions are not dropped — they come back with `median_ms: None`
/// and sort last, so `--json` consumers can tell "slow" from "down".
pub fn measure(regions: &[Region], settings: &ProbeSettings) -> Vec<RegionLatency> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(settings.timeout))
        .build()
        .into();
    let measured = regions
        .iter()
        .map(|region| measure_one(&agent, region, settings))
        .collect();
    crate::regions::rank(measured)
}

fn measure_one(agent: &ureq::Agent, region: &Region, settings: &ProbeSettings) -> RegionLatency {
    let target = format!(
        "{}{}",
        region.url.trim_end_matches('/'),
        settings.health_path
    );

    let mut milliseconds = Vec::with_capacity(settings.samples);
    for probe in 0..(settings.warmup + settings.samples) {
        let started = Instant::now();
        let reached = agent.get(&target).call().is_ok();
        // The first `warmup` probes pay the connection-setup cost; discarding
        // them is what makes repeated runs comparable.
        if probe >= settings.warmup && reached {
            milliseconds.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }

    RegionLatency {
        name: region.name.clone(),
        url: region.url.clone(),
        ok: milliseconds.len(),
        total: settings.samples,
        median_ms: median(milliseconds),
    }
}
