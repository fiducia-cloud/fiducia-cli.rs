//! `fiducia region` (alias `closest`) — probe every region and rank them.
//!
//! The chosen region's `name` is what a client then sends as `X-Fiducia-Region`.

use serde::Serialize;

use crate::error::CliError;
use crate::flags::CliArgs;
use crate::output::{emit, Format, Report};
use crate::regions::{closest, RegionLatency};

#[derive(Debug, Serialize)]
pub struct RegionRanking {
    pub regions: Vec<RegionLatency>,
    /// `None` when no region answered — a real failure, not an empty result.
    pub closest: Option<String>,
}

impl Report for RegionRanking {
    fn render_human(&self) -> String {
        let mut out = format!("{:<16} {:>10}  {:>7}  url", "region", "median ms", "ok/total");
        for region in &self.regions {
            let latency = region
                .median_ms
                .map(|milliseconds| format!("{milliseconds:.1}"))
                .unwrap_or_else(|| "—".into());
            out.push_str(&format!(
                "\n{:<16} {:>10}  {:>3}/{:<3}  {}",
                region.name, latency, region.ok, region.total, region.url
            ));
        }
        match &self.closest {
            Some(name) => out.push_str(&format!(
                "\n\nclosest: {name}  (pass it as  X-Fiducia-Region: {name})"
            )),
            None => out.push_str("\n\nno region was reachable"),
        }
        out
    }

    fn exit_code(&self) -> i32 {
        // "Everything is down" must not look like success to a caller that
        // only checks `$?`, in either output mode.
        i32::from(self.closest.is_none())
    }
}

pub fn run(args: &CliArgs) -> Result<i32, CliError> {
    let regions = args.load_regions()?;
    let ranked = crate::probe::measure(&regions, &args.probe_settings());
    let report = RegionRanking {
        closest: closest(&ranked).map(|candidate| candidate.name.clone()),
        regions: ranked,
    };
    emit(&report, Format::from_json_flag(args.json))
}
