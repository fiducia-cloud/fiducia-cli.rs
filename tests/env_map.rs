use fiducia_cli::{env_value, get_env_map, EnvMap};

const PROCESS_PROBE: &str = "FIDUCIA_CLI_INTEGRATION_PROCESS_PROBE";

#[test]
fn later_duplicate_overrides_win_deterministically() {
    let result = get_env_map(
        EnvMap::from([("FIDUCIA_SAMPLES".into(), "3".into())]),
        [
            ("FIDUCIA_SAMPLES".into(), "5".into()),
            ("FIDUCIA_SAMPLES".into(), "8".into()),
        ],
    );

    assert_eq!(env_value(&result, "FIDUCIA_SAMPLES"), Some("8"));
}

#[test]
fn merge_preserves_unrelated_values_and_btree_order() {
    let original = EnvMap::from([
        ("Z_LAST".into(), "z".into()),
        ("A_FIRST".into(), "a".into()),
    ]);
    let result = get_env_map(original.clone(), [("M_MIDDLE".into(), "m".into())]);

    assert_eq!(
        original.len(),
        2,
        "the caller-owned source snapshot is unchanged"
    );
    assert_eq!(
        result.keys().map(String::as_str).collect::<Vec<_>>(),
        ["A_FIRST", "M_MIDDLE", "Z_LAST"],
    );
}

#[test]
fn env_value_trims_unicode_whitespace_without_changing_storage() {
    let env = EnvMap::from([("FIDUCIA_REGION".into(), "\u{2003}sa-east-1\u{2003}".into())]);

    assert_eq!(env_value(&env, "FIDUCIA_REGION"), Some("sa-east-1"));
    assert_eq!(
        env.get("FIDUCIA_REGION").map(String::as_str),
        Some("\u{2003}sa-east-1\u{2003}"),
    );
}

#[test]
fn env_value_distinguishes_missing_from_present_nonempty_values() {
    let env = EnvMap::from([("FIDUCIA_JSON".into(), "1".into())]);

    assert_eq!(env_value(&env, "FIDUCIA_JSON"), Some("1"));
    assert_eq!(env_value(&env, "FIDUCIA_MISSING"), None);
}

#[test]
fn empty_override_masks_a_previous_value() {
    let result = get_env_map(
        EnvMap::from([(
            "FIDUCIA_NODE_URL".into(),
            "https://node.example.test".into(),
        )]),
        [("FIDUCIA_NODE_URL".into(), " \t ".into())],
    );

    assert_eq!(env_value(&result, "FIDUCIA_NODE_URL"), None);
    assert_eq!(
        result.get("FIDUCIA_NODE_URL").map(String::as_str),
        Some(" \t "),
    );
}

#[test]
fn pure_merge_does_not_touch_the_process_environment() {
    let before = std::env::var_os(PROCESS_PROBE);
    let result = get_env_map(
        EnvMap::new(),
        [(PROCESS_PROBE.into(), "snapshot-only".into())],
    );

    assert_eq!(env_value(&result, PROCESS_PROBE), Some("snapshot-only"));
    assert_eq!(std::env::var_os(PROCESS_PROBE), before);
}
