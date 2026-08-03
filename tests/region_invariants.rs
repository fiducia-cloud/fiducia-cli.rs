use fiducia_cli::{closest, median, parse_regions, rank, select_regions, RegionLatency};

fn for_each_permutation<T: Clone>(items: &mut [T], start: usize, visit: &mut impl FnMut(&[T])) {
    if start == items.len() {
        visit(items);
        return;
    }

    for index in start..items.len() {
        items.swap(start, index);
        for_each_permutation(items, start + 1, visit);
        items.swap(start, index);
    }
}

fn latency(name: &str, median_ms: Option<f64>) -> RegionLatency {
    RegionLatency {
        name: name.to_string(),
        url: format!("https://{name}.fiducia.test"),
        median_ms,
        ok: usize::from(median_ms.is_some()),
        total: 1,
    }
}

#[test]
fn median_is_invariant_across_every_input_permutation() {
    let mut samples = [-9.0, -1.0, 0.0, 4.0, 100.0];
    let mut visited = 0usize;

    for_each_permutation(&mut samples, 0, &mut |order| {
        visited += 1;
        assert_eq!(median(order.to_vec()), Some(0.0), "order={order:?}");
    });

    assert_eq!(visited, 120, "5! permutations must be exercised");
}

#[test]
fn rank_is_total_and_deterministic_for_every_input_permutation() {
    let mut regions = [
        latency("slow", Some(90.0)),
        latency("down", None),
        latency("fast", Some(3.0)),
        latency("mid", Some(25.0)),
    ];
    let mut visited = 0usize;

    for_each_permutation(&mut regions, 0, &mut |order| {
        visited += 1;
        let ranked = rank(order.to_vec());
        let names: Vec<&str> = ranked.iter().map(|region| region.name.as_str()).collect();
        assert_eq!(names, ["fast", "mid", "slow", "down"]);
        assert_eq!(closest(&ranked).map(|region| region.name.as_str()), Some("fast"));
    });

    assert_eq!(visited, 24, "4! permutations must be exercised");
}

#[test]
fn region_json_round_trips_unicode_ipv6_and_selection_without_drift() {
    let parsed = parse_regions(
        r#"[
            {"name":" São Paulo ","url":" https://south-america.fiducia.test "},
            {"name":"ipv6-lab","url":"https://[2001:db8::1]:8443"}
        ]"#,
    )
    .expect("valid edge-region document");

    assert_eq!(parsed[0].name, "São Paulo");
    assert_eq!(parsed[0].url, "https://south-america.fiducia.test");
    assert_eq!(parsed[1].url, "https://[2001:db8::1]:8443");

    let encoded = serde_json::to_string(&parsed).expect("serialize normalized regions");
    let reparsed = parse_regions(&encoded).expect("reparse normalized regions");
    assert_eq!(reparsed, parsed, "normalization must be idempotent");

    let selected = select_regions(reparsed, "São Paulo").expect("select unicode region");
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].url, "https://south-america.fiducia.test");
}
