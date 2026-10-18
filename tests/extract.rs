use mev_revert_classifier::extract::*;
use serde_json::json;

const BOT: &str = "0x00000000000000000000000000000000000000b0";
const POOL_A: &str = "0x00000000000000000000000000000000000000a1";
const POOL_B: &str = "0x00000000000000000000000000000000000000a2";
const TOKEN: &str = "0x00000000000000000000000000000000000000c1";

fn call(to: &str, input: &str, calls: serde_json::Value) -> serde_json::Value {
    json!({"type": "CALL", "from": BOT, "to": to, "input": input, "value": "0x0", "calls": calls})
}

fn receipt(hash: &str, idx: u64, status: &str, logs: serde_json::Value) -> serde_json::Value {
    json!({"transactionHash": hash, "transactionIndex": format!("0x{idx:x}"), "status": status,
           "gasUsed": "0x64", "effectiveGasPrice": "0x2", "l1Fee": "0x5", "logs": logs})
}

fn balance_of(addr: &str) -> String {
    format!("0x70a08231{:0>64}", addr.trim_start_matches("0x"))
}

#[test]
fn probe_revert_success_and_pools() {
    let trace = json!([
        // probe: reads POOL_A slot0 and TOKEN.balanceOf(POOL_B), no external logs
        {"txHash": "0x01", "result": {"from": "0xeoa", "to": BOT, "value": "0x0", "input": "0x",
            "calls": [call(POOL_A, "0x3850c7bd", json!([])), call(TOKEN, &balance_of(POOL_B), json!([]))]}},
        // revert touching POOL_B only via a non-pool selector, but POOL_B swaps elsewhere in the block
        {"txHash": "0x02", "result": {"from": "0xeoa", "to": BOT, "value": "0x0", "input": "0x",
            "calls": [call(POOL_B, "0xdeadbeef", json!([]))]}},
        // success that emits a swap from POOL_B; token never touched as a pool
        {"txHash": "0x03", "result": {"from": "0xuser", "to": "0x00000000000000000000000000000000000000d0", "value": "0x0", "input": "0x",
            "calls": [call(POOL_B, "0x128acb08", json!([]))]}},
        // probe that only reads TOKEN.balanceOf(user): no pools
        {"txHash": "0x04", "result": {"from": "0xeoa", "to": BOT, "value": "0x1", "input": "0x",
            "calls": [call(TOKEN, &balance_of("0x00000000000000000000000000000000000000ee"), json!([]))]}}
    ]);
    let receipts = json!([
        receipt(
            "0x01",
            0,
            "0x1",
            json!([{"address": BOT, "topics": ["0x1234"]}])
        ),
        receipt("0x02", 1, "0x0", json!([])),
        receipt(
            "0x03",
            2,
            "0x1",
            json!([{"address": POOL_B, "topics": [SWAP_TOPICS[1]]}])
        ),
        receipt("0x04", 3, "0x1", json!([])),
    ]);
    let rows = extract_block(7, &trace, &receipts).unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows[0].kind,
        Kind::Probe,
        "logs from the bot itself do not count"
    );
    assert_eq!(
        rows[0].pool_list(),
        vec![POOL_A, POOL_B],
        "balanceOf(pool) counts as a read"
    );
    assert_eq!(rows[1].kind, Kind::Revert);
    assert_eq!(
        rows[1].pool_list(),
        vec![POOL_B],
        "swap emitter in the block is a pool"
    );
    assert_eq!(rows[2].kind, Kind::Success);
    assert_eq!(rows[2].swap_logs, 1);
    assert_eq!(rows[3].kind, Kind::Probe, "1 wei still a probe");
    assert!(rows[3].pools.is_empty(), "token is not a pool");
    assert_eq!(rows[0].fee_wei, 100 * 2 + 5);
    assert_eq!(rows[1].tx_index, 1);
}

#[test]
fn value_above_one_wei_or_external_log_is_not_a_probe() {
    let trace = json!([
        {"txHash": "0x01", "result": {"from": "0xeoa", "to": BOT, "value": "0x2", "input": "0x"}},
        {"txHash": "0x02", "result": {"from": "0xeoa", "to": BOT, "value": "0x0", "input": "0x"}}
    ]);
    let receipts = json!([
        receipt("0x01", 0, "0x1", json!([])),
        receipt(
            "0x02",
            1,
            "0x1",
            json!([{"address": TOKEN, "topics": [TRANSFER_TOPIC]}])
        ),
    ]);
    let rows = extract_block(1, &trace, &receipts).unwrap();
    assert_eq!(rows[0].kind, Kind::Success);
    assert_eq!(rows[1].kind, Kind::Success);
}

#[test]
fn cyclic_needs_two_swaps_and_token_round_trip() {
    let pad = |a: &str| format!("0x{:0>64}", a.trim_start_matches("0x"));
    let transfer = |from: &str, to: &str| json!({"address": TOKEN, "topics": [TRANSFER_TOPIC, pad(from), pad(to)]});
    let swap = json!({"address": POOL_A, "topics": [SWAP_TOPICS[0]]});
    let trace = json!([
        {"txHash": "0x01", "result": {"from": "0xeoa", "to": BOT, "value": "0x0", "input": "0x"}},
        {"txHash": "0x02", "result": {"from": "0xeoa", "to": BOT, "value": "0x0", "input": "0x"}}
    ]);
    let receipts = json!([
        receipt(
            "0x01",
            0,
            "0x1",
            json!([
                transfer(BOT, POOL_A),
                swap.clone(),
                swap.clone(),
                transfer(POOL_B, BOT)
            ])
        ),
        receipt(
            "0x02",
            1,
            "0x1",
            json!([transfer(BOT, POOL_A), swap.clone()])
        ),
    ]);
    let rows = extract_block(1, &trace, &receipts).unwrap();
    assert!(rows[0].cyclic);
    assert!(!rows[1].cyclic);
}

#[test]
fn missing_receipt_is_an_error_not_a_silent_drop() {
    let trace = json!([{"txHash": "0x01", "result": {"from": "0xeoa", "to": BOT, "value": "0x0", "input": "0x"}}]);
    assert!(extract_block(1, &trace, &json!([])).is_err());
    assert!(extract_block(1, &json!({}), &json!([])).is_err());
}

#[test]
fn router_passing_user_tokens_is_not_cyclic() {
    let pad = |a: &str| format!("0x{:0>64}", a.trim_start_matches("0x"));
    let user = "0x00000000000000000000000000000000000000e1";
    let router = "0x00000000000000000000000000000000000000f1";
    let token_b = "0x00000000000000000000000000000000000000c2";
    let transfer = |token: &str, from: &str, to: &str| json!({"address": token, "topics": [TRANSFER_TOPIC, pad(from), pad(to)]});
    let swap = json!({"address": POOL_A, "topics": [SWAP_TOPICS[1]]});
    let trace = json!([{"txHash": "0x01", "result": {"from": user, "to": router, "value": "0x0", "input": "0x"}}]);
    // user -> router (A), router -> pool (A), pool -> router (B), router -> user (B)
    let receipts = json!([receipt(
        "0x01",
        0,
        "0x1",
        json!([
            transfer(TOKEN, user, router),
            transfer(TOKEN, router, POOL_A),
            swap.clone(),
            swap.clone(),
            transfer(token_b, POOL_A, router),
            transfer(token_b, router, user)
        ])
    )]);
    let rows = extract_block(1, &trace, &receipts).unwrap();
    assert!(!rows[0].cyclic);
}
