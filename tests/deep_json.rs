use mev_revert_classifier::rpc::parse_json;

fn nested(depth: usize) -> String {
    format!("{}1{}", "[".repeat(depth), "]".repeat(depth))
}

#[test]
fn parses_json_nested_ten_thousand_levels() {
    let text = nested(10_000);
    // The stock parser stops at 128 levels (EXPL-14).
    assert!(serde_json::from_str::<serde_json::Value>(&text).is_err());
    // Deep values also drop recursively, so run on a roomy stack like the fetch workers.
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let v = parse_json(&text).expect("deep json parses");
            assert!(v.is_array());
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn rejects_trailing_garbage_and_truncation() {
    assert!(parse_json("[1] x").is_err());
    assert!(parse_json(&nested(500)[..400]).is_err());
}
