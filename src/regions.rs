//! Region parsing + latency ranking — the pure half of the CLI.
//!
//! No I/O lives here; probing is in [`crate::probe`] and rendering is in
//! [`crate::commands`]. A region is just a `{ name, url }` — the same shape
//! `fiducia-infra` emits to `edge-regions.json` from `topology.toml`, so the
//! selectable region set is generated from one place, never hand-maintained
//! here.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// A selectable region: a public name + the endpoint to probe.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Region {
    pub name: String,
    pub url: String,
}

/// Parse the regions list (the `edge-regions.json` array).
pub fn parse_regions(json: &str) -> Result<Vec<Region>, String> {
    let mut regions = serde_json::from_str::<Vec<Region>>(json).map_err(|e| e.to_string())?;
    let mut seen = HashSet::new();

    for (index, region) in regions.iter_mut().enumerate() {
        region.name = region.name.trim().to_string();
        region.url = region.url.trim().to_string();

        if region.name.is_empty() {
            return Err(format!("region {index} has an empty name"));
        }

        if region.url.is_empty() {
            return Err(format!("region {} has an empty url", region.name));
        }

        if !(region.url.starts_with("https://") || region.url.starts_with("http://")) {
            return Err(format!(
                "region {} url must start with http(s)",
                region.name
            ));
        }

        if !seen.insert(region.name.clone()) {
            return Err(format!("duplicate region name: {}", region.name));
        }
    }

    Ok(regions)
}

/// Median of latency samples in ms. `None` if there are no samples.
pub fn median(mut xs: Vec<f64>) -> Option<f64> {
    xs.retain(|x| x.is_finite());
    if xs.is_empty() {
        return None;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = xs.len();
    Some(if n % 2 == 1 {
        xs[n / 2]
    } else {
        (xs[n / 2 - 1] + xs[n / 2]) / 2.0
    })
}

/// A region's measured latency. `median_ms == None` means unreachable.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegionLatency {
    pub name: String,
    pub url: String,
    pub median_ms: Option<f64>,
    pub ok: usize,
    pub total: usize,
}

/// Rank regions by median latency ascending; unreachable ones (None) sort last.
pub fn rank(mut rs: Vec<RegionLatency>) -> Vec<RegionLatency> {
    rs.sort_by(|a, b| match (a.median_ms, b.median_ms) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    rs
}

/// The closest *reachable* region after ranking, if any.
pub fn closest(ranked: &[RegionLatency]) -> Option<&RegionLatency> {
    ranked.iter().find(|r| r.median_ms.is_some())
}

/// Interpret a `--json`-style boolean env value. Truthy: `1/true/yes/on`
/// (any case); everything else (incl. empty/unset upstream) is false.
pub fn truthy(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Restrict `regions` to the single one named `only` (the `--only` flag). An
/// empty `only` means "no filter" and returns every region. Returns an error
/// string naming `only` when it matches nothing, so the caller can fail loudly.
pub fn select_regions(regions: Vec<Region>, only: &str) -> Result<Vec<Region>, String> {
    if only.is_empty() {
        return Ok(regions);
    }
    let kept: Vec<Region> = regions.into_iter().filter(|r| r.name == only).collect();
    if kept.is_empty() {
        return Err(format!("no region named {only:?}"));
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rl(name: &str, ms: Option<f64>) -> RegionLatency {
        RegionLatency {
            name: name.into(),
            url: "u".into(),
            median_ms: ms,
            ok: 1,
            total: 1,
        }
    }

    #[test]
    fn median_odd_even_empty() {
        assert_eq!(median(vec![]), None);
        assert_eq!(median(vec![5.0]), Some(5.0));
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(vec![1.0, 2.0, 3.0, 4.0]), Some(2.5));
    }

    #[test]
    fn median_ignores_non_finite_samples() {
        assert_eq!(median(vec![f64::NAN, f64::INFINITY]), None);
        assert_eq!(
            median(vec![10.0, f64::NAN, 30.0, f64::NEG_INFINITY]),
            Some(20.0)
        );
    }

    #[test]
    fn parse_regions_reads_edge_format() {
        let regions = parse_regions(
            r#"[{"name":"gcp","url":"https://gcp.lb"},{"name":"aws","url":"https://aws.lb"}]"#,
        )
        .unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].name, "gcp");
        assert!(parse_regions("not json").is_err());
    }

    #[test]
    fn parse_regions_trims_names_and_rejects_duplicates() {
        let regions = parse_regions(r#"[{"name":" us-east ","url":" https://aws.lb "}]"#).unwrap();
        assert_eq!(regions[0].name, "us-east");
        assert_eq!(regions[0].url, "https://aws.lb");

        let err = parse_regions(
            r#"[{"name":"us-east","url":"https://a"},{"name":"us-east","url":"https://b"}]"#,
        )
        .unwrap_err();
        assert!(err.contains("duplicate region name"));
    }

    #[test]
    fn parse_regions_rejects_blank_names_and_non_http_urls() {
        let blank_name = parse_regions(r#"[{"name":" ","url":"https://aws.lb"}]"#).unwrap_err();
        assert!(blank_name.contains("empty name"));

        let blank_url = parse_regions(r#"[{"name":"aws","url":" "}]"#).unwrap_err();
        assert!(blank_url.contains("empty url"));

        let non_http = parse_regions(r#"[{"name":"aws","url":"ssh://aws.lb"}]"#).unwrap_err();
        assert!(non_http.contains("http(s)"));
    }

    #[test]
    fn rank_orders_by_latency_unreachable_last() {
        let ranked = rank(vec![
            rl("far", Some(120.0)),
            rl("down", None),
            rl("near", Some(8.0)),
            rl("mid", Some(40.0)),
        ]);
        assert_eq!(
            ranked.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["near", "mid", "far", "down"]
        );
        assert_eq!(closest(&ranked).unwrap().name, "near");
    }

    #[test]
    fn closest_selection_is_deterministic_for_fixed_latencies() {
        // Three fixed latency profiles, each with a different clearly-nearest
        // region: the pick must be the expected region every time, and
        // repeating the ranking on the same input must never change the
        // answer (ties keep input order — the sort is stable).
        let profiles: [(&str, Vec<RegionLatency>); 3] = [
            (
                "gcp",
                vec![
                    rl("gcp", Some(4.0)),
                    rl("aws", Some(90.0)),
                    rl("hetzner", Some(55.0)),
                ],
            ),
            (
                "aws",
                vec![
                    rl("gcp", Some(80.0)),
                    rl("aws", Some(6.0)),
                    rl("hetzner", Some(70.0)),
                ],
            ),
            (
                "hetzner",
                vec![
                    rl("gcp", Some(60.0)),
                    rl("aws", Some(75.0)),
                    rl("hetzner", Some(3.0)),
                ],
            ),
        ];
        for (expected, latencies) in profiles {
            for _ in 0..3 {
                let ranked = rank(latencies.clone());
                assert_eq!(
                    closest(&ranked).expect("reachable region").name,
                    expected,
                    "profile nearest to {expected} must always pick {expected}"
                );
            }
        }

        // Exact tie: stable sort keeps input order, so the pick is still
        // deterministic rather than platform/run dependent.
        let tied = vec![rl("first", Some(10.0)), rl("second", Some(10.0))];
        for _ in 0..3 {
            assert_eq!(closest(&rank(tied.clone())).unwrap().name, "first");
        }
    }

    #[test]
    fn rank_tolerates_non_finite_medians_without_panicking() {
        // Adversarial latency data must not panic the comparator (the
        // partial_cmp fallback) and must stay deterministic: NaN medians are
        // still "reachable" (Some) so they sort before unreachable (None),
        // and finite medians keep their ascending order.
        let ranked = rank(vec![
            rl("nan", Some(f64::NAN)),
            rl("down", None),
            rl("near", Some(5.0)),
            rl("far", Some(500.0)),
        ]);
        assert_eq!(ranked.len(), 4);
        assert_eq!(
            ranked.last().unwrap().name,
            "down",
            "unreachable regions must always sort last"
        );
        let near_pos = ranked.iter().position(|r| r.name == "near").unwrap();
        let far_pos = ranked.iter().position(|r| r.name == "far").unwrap();
        assert!(near_pos < far_pos, "finite medians must stay ascending");
        // closest() must still return a reachable region, never the None one.
        assert!(closest(&ranked).unwrap().median_ms.is_some());
    }

    #[test]
    fn closest_none_when_all_unreachable() {
        let ranked = rank(vec![rl("a", None), rl("b", None)]);
        assert!(closest(&ranked).is_none());
    }

    #[test]
    fn truthy_accepts_common_yes_values() {
        for y in ["1", "true", "TRUE", "Yes", " on "] {
            assert!(truthy(y), "{y:?} should be truthy");
        }
        for n in ["", "0", "false", "no", "off", "nope"] {
            assert!(!truthy(n), "{n:?} should be falsey");
        }
    }

    #[test]
    fn select_regions_filters_or_errors() {
        let regions = parse_regions(
            r#"[{"name":"gcp","url":"https://gcp.lb"},{"name":"aws","url":"https://aws.lb"}]"#,
        )
        .unwrap();

        // empty `only` => unchanged
        assert_eq!(select_regions(regions.clone(), "").unwrap().len(), 2);

        // matching name => just that one
        let aws = select_regions(regions.clone(), "aws").unwrap();
        assert_eq!(aws.len(), 1);
        assert_eq!(aws[0].name, "aws");

        // no match => error mentioning the name
        let err = select_regions(regions, "nope").unwrap_err();
        assert!(err.contains("nope"));
    }

    #[test]
    fn generated_interfaces_are_importable() {
        let request = fiducia_interfaces::LockAcquireManyRequest {
            keys: vec!["orders/42".to_string(), "inventory/sku-7".to_string()],
            holder: "worker-a".to_string(),
            request_id: Some("interface-contract-attempt".to_string()),
            ttl_ms: Some(30_000),
            wait: Some(true),
            wait_timeout_ms: Some(5_000),
        };

        assert_eq!(request.keys.len(), 2);
        assert_eq!(request.holder, "worker-a");
        assert_eq!(
            request.request_id.as_deref(),
            Some("interface-contract-attempt")
        );
        assert_eq!(request.wait_timeout_ms, Some(5_000));
        assert!(matches!(
            fiducia_interfaces::ProposeErrorReason::NotLeader,
            fiducia_interfaces::ProposeErrorReason::NotLeader
        ));
    }
}
