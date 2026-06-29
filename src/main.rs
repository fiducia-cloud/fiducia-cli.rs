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

use std::time::{Duration, Instant};

use fiducia_cli::{closest, median, parse_regions, rank, select_regions, truthy, RegionLatency};

const USAGE: &str = "\
fiducia — fiducia.cloud CLI

USAGE:
  fiducia regions  [-r <file>] [-o <name>] [-j]
  fiducia region   [-r <file>] [-n <samples>] [-p <path>] [-t <ms>] [-w <n>] [-o <name>] [-j]
                                                           (alias: closest)

OPTIONS (flag | short | env — flags override env; declared in .cli-flags.toml):
  --regions | -r <file>  regions JSON [{name,url}]   FIDUCIA_REGIONS_FILE  (default ./edge-regions.json)
  --samples | -n <n>     probes per region           FIDUCIA_SAMPLES       (default 5)
  --path    | -p <p>     health path to probe        FIDUCIA_HEALTH_PATH   (default /healthz)
  --timeout | -t <ms>    per-probe timeout (ms)       FIDUCIA_TIMEOUT_MS    (default 2000)
  --warmup  | -w <n>     discard first N probes       FIDUCIA_WARMUP        (default 0)
  --only    | -o <name>  probe only this region       FIDUCIA_ONLY_REGION   (default all)
  --json    | -j         machine-readable JSON output FIDUCIA_JSON          (default off)
";

fn main() {
    // flags-2-env model (see .cli-flags.toml): defaults <- env <- CLI flags.
    // i.e. read each setting from its declared env var, then let a CLI flag
    // override it (combined = { ...env, ...parsed_cli }).
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cmd = String::from("region");
    let mut regions_file = env_or("FIDUCIA_REGIONS_FILE", "edge-regions.json");
    let mut samples: usize = env_or("FIDUCIA_SAMPLES", "5").parse().unwrap_or(5);
    let mut path = env_or("FIDUCIA_HEALTH_PATH", "/healthz");
    let mut timeout_ms: u64 = env_or("FIDUCIA_TIMEOUT_MS", "2000").parse().unwrap_or(2000);
    let mut warmup: usize = env_or("FIDUCIA_WARMUP", "0").parse().unwrap_or(0);
    let mut only = env_or("FIDUCIA_ONLY_REGION", "");
    let mut json = truthy(&env_or("FIDUCIA_JSON", ""));

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return;
            }
            "--regions" | "-r" => {
                i += 1;
                regions_file = arg(&args, i);
            }
            "--samples" | "-n" => {
                i += 1;
                samples = arg(&args, i).parse().unwrap_or(samples);
            }
            "--path" | "-p" => {
                i += 1;
                path = arg(&args, i);
            }
            "--timeout" | "-t" => {
                i += 1;
                timeout_ms = arg(&args, i).parse().unwrap_or(timeout_ms);
            }
            "--warmup" | "-w" => {
                i += 1;
                warmup = arg(&args, i).parse().unwrap_or(warmup);
            }
            "--only" | "-o" => {
                i += 1;
                only = arg(&args, i);
            }
            "--json" | "-j" => json = true,
            s if !s.starts_with('-') => cmd = s.to_string(),
            other => {
                eprintln!("unknown flag: {other}\n");
                print!("{USAGE}");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    let json_txt = match std::fs::read_to_string(&regions_file) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("cannot read regions file {regions_file}: {e}");
            std::process::exit(1);
        }
    };
    let regions = match parse_regions(&json_txt) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("invalid regions file: {e}");
            std::process::exit(1);
        }
    };
    // --only narrows to a single region (and fails loudly if it matches none).
    let regions = match select_regions(regions, &only) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    match cmd.as_str() {
        "regions" => {
            if json {
                println!("{}", serde_json::to_string_pretty(&regions).unwrap());
            } else {
                println!("selectable regions ({}):", regions.len());
                for r in &regions {
                    println!("  {:<16} {}", r.name, r.url);
                }
            }
        }
        "region" | "closest" => {
            let agent = ureq::AgentBuilder::new()
                .timeout(Duration::from_millis(timeout_ms))
                .build();
            let mut results = Vec::new();
            for r in &regions {
                let target = format!("{}{}", r.url.trim_end_matches('/'), path);
                // Probe `warmup + samples` times; the first `warmup` calls prime
                // the TCP/TLS connection and are not measured.
                let mut ms = Vec::new();
                for n in 0..(warmup + samples) {
                    let t = Instant::now();
                    let ok = agent.get(&target).call().is_ok();
                    if n >= warmup && ok {
                        ms.push(t.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                results.push(RegionLatency {
                    name: r.name.clone(),
                    url: r.url.clone(),
                    median_ms: median(ms.clone()),
                    ok: ms.len(),
                    total: samples,
                });
            }
            let ranked = rank(results);
            let nearest = closest(&ranked).map(|c| c.name.clone());

            if json {
                let out = serde_json::json!({
                    "regions": &ranked,
                    "closest": nearest,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                println!(
                    "{:<16} {:>10}  {:>7}  url",
                    "region", "median ms", "ok/total"
                );
                for r in &ranked {
                    let lat = r
                        .median_ms
                        .map(|m| format!("{m:.1}"))
                        .unwrap_or_else(|| "—".into());
                    println!(
                        "{:<16} {:>10}  {:>3}/{:<3}  {}",
                        r.name, lat, r.ok, r.total, r.url
                    );
                }
                match &nearest {
                    Some(name) => println!(
                        "\nclosest: {name}  (pass it as  X-Fiducia-Region: {name})"
                    ),
                    None => eprintln!("\nno region was reachable"),
                }
            }
            // Unreachable everywhere is a failure in either output mode.
            if nearest.is_none() {
                std::process::exit(1);
            }
        }
        other => {
            eprintln!("unknown command: {other}\n");
            print!("{USAGE}");
            std::process::exit(2);
        }
    }
}

fn arg(args: &[String], i: usize) -> String {
    args.get(i).cloned().unwrap_or_else(|| {
        eprintln!("missing value for flag");
        std::process::exit(2);
    })
}

/// Read an env var (one of the names declared in `.cli-flags.toml`) or fall back
/// to the declared default. CLI flags override these afterward.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
