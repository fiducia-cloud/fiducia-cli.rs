//! `fiducia regions` — list the selectable regions without probing them.

use serde::Serialize;

use crate::error::CliError;
use crate::flags::CliArgs;
use crate::output::{emit, Format, Report};
use crate::regions::Region;

#[derive(Debug, Serialize)]
pub struct RegionList {
    pub regions: Vec<Region>,
}

impl Report for RegionList {
    fn render_human(&self) -> String {
        let mut out = format!("selectable regions ({}):", self.regions.len());
        for region in &self.regions {
            out.push_str(&format!("\n  {:<16} {}", region.name, region.url));
        }
        out
    }
}

pub fn run(args: &CliArgs) -> Result<i32, CliError> {
    let regions = args.load_regions()?;
    emit(&RegionList { regions }, Format::from_json_flag(args.json))
}
