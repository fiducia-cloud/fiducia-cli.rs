//! fiducia — command-line tool for fiducia.cloud.
//!
//!   fiducia regions [--regions <file>]            list the selectable regions
//!   fiducia region  [--regions <file>] [--samples N] [--path P]
//!                                                 probe each region, print the
//!                                                 closest (lowest median latency)
//!
//! Regions come from a JSON array of `{ "name", "url" }` — the same
//! `edge-regions.json` that `fiducia-infra` generates from `topology.toml`. The
//! chosen region's `name` is what a client then passes as `X-Fiducia-Region`.

mod cli_config;
mod flags;

use std::time::{Duration, Instant};

use fiducia_cli::{closest, median, parse_regions, rank, select_regions, RegionLatency};

const USAGE: &str = "\
fiducia — fiducia.cloud CLI

USAGE:
  fiducia regions  [-r <file>] [-o <name>] [-j]
  fiducia region   [-r <file>] [-n <samples>] [-p <path>] [-t <ms>] [-w <n>] [-o <name>] [-j]
                                                            (alias: closest)

OPTIONS (flag | short | env — flags override env; declared in .cli-flags.toml):
  --regions | -r <file>  regions JSON [{name,url}]   FIDUCIA_REGIONS_FILE  (default ./edge-regions.json)
  --samples | -n <n>     probes per region           FIDUCIA_SAMPLES       (default 5; range 1..=100)
  --path    | -p <p>     health path to probe        FIDUCIA_HEALTH_PATH   (default /healthz)
  --timeout | -t <ms>    per-probe timeout (ms)       FIDUCIA_TIMEOUT_MS    (default 2000; max 60000)
  --warmup  | -w <n>     discard first N probes       FIDUCIA_WARMUP        (default 0; max 100)
  --only    | -o <name>  probe only this region       FIDUCIA_ONLY_REGION   (default all)
  --json    | -j         machine-readable JSON output FIDUCIA_JSON          (default off)
";

fn main() {
    let argv = std::env::args().collect::<Vec<_>>();
    if argv
        .iter()
        .skip(1)
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        print!("{USAGE}");
        return;
    }

    let config_path = flags::resolve_config_path().unwrap_or_else(|error| fail_usage(&error));
    let args =
        flags::parse_cli_args(&argv, &config_path).unwrap_or_else(|error| fail_usage(&error));

    let json_txt = match std::fs::read_to_string(&args.regions_file) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("cannot read regions file {}: {error}", args.regions_file);
            std::process::exit(1);
        }
    };
    let regions = match parse_regions(&json_txt) {
        Ok(regions) => regions,
        Err(error) => {
            eprintln!("invalid regions file: {error}");
            std::process::exit(1);
        }
    };
    // --only narrows to a single region (and fails loudly if it matches none).
    let regions = match select_regions(regions, &args.only_region) {
        Ok(regions) => regions,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    match args.command.as_str() {
        "regions" => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&regions).unwrap());
            } else {
                println!("selectable regions ({}):", regions.len());
                for region in &regions {
                    println!("  {:<16} {}", region.name, region.url);
                }
            }
        }
        "region" | "closest" => {
            let agent = ureq::AgentBuilder::new()
                .timeout(Duration::from_millis(args.timeout_ms))
                .build();
            let mut results = Vec::new();
            for region in &regions {
                let target = format!("{}{}", region.url.trim_end_matches('/'), args.health_path);
                // Probe `warmup + samples` times; the first `warmup` calls prime
                // the TCP/TLS connection and are not measured.
                let mut milliseconds = Vec::new();
                for probe in 0..(args.warmup + args.samples) {
                    let started = Instant::now();
                    let ok = agent.get(&target).call().is_ok();
                    if probe >= args.warmup && ok {
                        milliseconds.push(started.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                results.push(RegionLatency {
                    name: region.name.clone(),
                    url: region.url.clone(),
                    median_ms: median(milliseconds.clone()),
                    ok: milliseconds.len(),
                    total: args.samples,
                });
            }
            let ranked = rank(results);
            let nearest = closest(&ranked).map(|candidate| candidate.name.clone());

            if args.json {
                let output = serde_json::json!({
                    "regions": &ranked,
                    "closest": nearest,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!(
                    "{:<16} {:>10}  {:>7}  url",
                    "region", "median ms", "ok/total"
                );
                for region in &ranked {
                    let latency = region
                        .median_ms
                        .map(|milliseconds| format!("{milliseconds:.1}"))
                        .unwrap_or_else(|| "—".into());
                    println!(
                        "{:<16} {:>10}  {:>3}/{:<3}  {}",
                        region.name, latency, region.ok, region.total, region.url
                    );
                }
                match &nearest {
                    Some(name) => {
                        println!("\nclosest: {name}  (pass it as  X-Fiducia-Region: {name})")
                    }
                    None => eprintln!("\nno region was reachable"),
                }
            }
            // Unreachable everywhere is a failure in either output mode.
            if nearest.is_none() {
                std::process::exit(1);
            }
        }
        _ => unreachable!("command was validated by flags::parse_cli_args"),
    }
}

fn fail_usage(error: &str) -> ! {
    eprintln!("fiducia: {error}\n");
    eprint!("{USAGE}");
    std::process::exit(2);
}
