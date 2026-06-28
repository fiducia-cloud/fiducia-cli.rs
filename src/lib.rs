//! Library half of `fiducia-cli`: region parsing + latency ranking.
//!
//! Pure and unit-tested; the I/O (probing) lives in `main.rs`. A region is just a
//! `{ name, url }` — the same shape `fiducia-infra` emits to `edge-regions.json`
//! from `topology.toml`, so the selectable region set is generated from one
//! place, never hand-maintained here.

use serde::{Deserialize, Serialize};

/// A selectable region: a public name + the endpoint to probe.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Region {
    pub name: String,
    pub url: String,
}

/// Parse the regions list (the `edge-regions.json` array).
pub fn parse_regions(json: &str) -> Result<Vec<Region>, String> {
    serde_json::from_str::<Vec<Region>>(json).map_err(|e| e.to_string())
}

/// Median of latency samples in ms. `None` if there are no samples.
pub fn median(mut xs: Vec<f64>) -> Option<f64> {
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
    fn closest_none_when_all_unreachable() {
        let ranked = rank(vec![rl("a", None), rl("b", None)]);
        assert!(closest(&ranked).is_none());
    }

    #[test]
    fn generated_interfaces_are_importable() {
        let request = fiducia_interfaces::LockAcquireManyRequest {
            keys: vec!["orders/42".to_string(), "inventory/sku-7".to_string()],
            holder: Some("worker-a".to_string()),
            ttl_ms: Some(30_000),
            wait: Some(false),
        };

        assert_eq!(request.keys.len(), 2);
        assert!(matches!(
            fiducia_interfaces::ProposeErrorReason::NotLeader,
            fiducia_interfaces::ProposeErrorReason::NotLeader
        ));
    }
}
