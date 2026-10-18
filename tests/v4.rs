//! Uniswap v4 pool detection and the chain constants. Fixtures come from the Unichain and
//! OP Mainnet pilot: real swap calldata, event topics and slot values.

use mev_revert_classifier::chain::Chain;
use mev_revert_classifier::extract::*;
use mev_revert_classifier::keccak::keccak256;
use mev_revert_classifier::sample::{
    block_timestamp_on, first_block_at_or_after_on, parse_date, sample_week_neighbors_on,
    sample_week_on, Role, Timing,
};
use serde_json::{json, Value};

const PM: &str = "0x1f98400000000000000000000000000000000004";
const BOT: &str = "0x099b1a87ded758e99d5a33a7df5ba9cf3a0907c9";
const EOA: &str = "0xd1b6a445ea0daf810528934fdf044a3a2cc40552";
const OTHER: &str = "0x00000000000000000000000000000000000000b0";
const STATE_VIEW: &str = "0x86e8631a016f9068c3f085faf484ee3f5fdee8f2";
const V3_POOL: &str = "0x00000000000000000000000000000000000000a1";
/// PoolId of the swap below, as emitted in `Swap.topics[1]`.
const POOL_ID: &str = "0x3258f413c7a88cda2fa8709a589d221a80f6574f63df5a5b6774485d8acc39d9";
/// `swap(key, params, hookData)` of Unichain block 61_185_744, tx 0xf302d431.
const SWAP_INPUT: &str = "0xf3cd914c0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000078d782b760474a361dda0af3839290b0ef57ad600000000000000000000000000000000000000000000000000000000000001f4000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffff6a6c1a9cd8f0a00000000000000000000000000000000000000000000346460923d0a2c000000000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000000";

fn call(from: &str, to: &str, input: &str, calls: Value) -> Value {
    json!({"type": "CALL", "from": from, "to": to, "input": input, "value": "0x0", "calls": calls})
}

fn receipt(hash: &str, idx: u64, status: &str, logs: Value) -> Value {
    json!({"transactionHash": hash, "transactionIndex": format!("0x{idx:x}"), "status": status,
           "gasUsed": "0x64", "effectiveGasPrice": "0x2", "l1Fee": "0x0", "logs": logs})
}

fn v4_swap_log(id: &str) -> Value {
    json!({"address": PM, "topics": [V4_SWAP_TOPIC, id,
        "0x000000000000000000000000099b1a87ded758e99d5a33a7df5ba9cf3a0907c9"], "data": "0x"})
}

/// Router -> bot -> PoolManager.unlock -> bot.unlockCallback -> PoolManager.swap.
fn unlock_swap() -> Value {
    let swap = call(BOT, PM, SWAP_INPUT, json!([]));
    let callback = call(PM, BOT, "0x91dd7346", json!([swap]));
    let unlock = call(BOT, PM, "0x48c89491", json!([callback]));
    json!({"type": "CALL", "from": EOA, "to": BOT, "input": "0x6bfd6286", "value": "0x0", "calls": [unlock]})
}

const V4: Option<V4Mode<'static>> = Some(V4Mode { pool_manager: PM });

#[test]
fn keccak256_known_vectors() {
    let hex = |d: [u8; 32]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(keccak256(b"")),
        "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
    );
    assert_eq!(
        hex(keccak256(b"abc")),
        "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
    );
    // The 160-byte pool key of the swap fixture spans two 136-byte blocks: see the pool-id tests.
}

#[test]
fn pool_id_comes_from_the_swap_call_of_a_reverted_transaction() {
    // Logs of a reverted transaction are discarded: only the call carries the pool key.
    let trace = json!([{"txHash": "0x01", "result": unlock_swap()}]);
    let receipts = json!([receipt("0x01", 0, "0x0", json!([]))]);
    let rows = extract_block_with(1, &trace, &receipts, V4).unwrap();
    assert_eq!(rows[0].kind, Kind::Revert);
    assert_eq!(rows[0].pools, format!("v4:{POOL_ID}"));
    // Without the option the PoolManager does not even appear (no pool selector called).
    let rows = extract_block(1, &trace, &receipts).unwrap();
    assert_eq!(rows[0].pools, "");
}

#[test]
fn successful_unlock_swap_agrees_with_the_event_and_is_not_a_probe() {
    let trace = json!([{"txHash": "0x01", "result": unlock_swap()}]);
    let receipts = json!([receipt("0x01", 0, "0x1", json!([v4_swap_log(POOL_ID)]))]);
    let rows = extract_block_with(1, &trace, &receipts, V4).unwrap();
    assert_eq!(
        rows[0].kind,
        Kind::Success,
        "PoolManager logs are external logs"
    );
    assert_eq!(rows[0].swap_logs, 1);
    assert_eq!(
        rows[0].pools,
        format!("v4:{POOL_ID}"),
        "call and event give one id"
    );
    // Default behavior: the PoolManager is the pool, as before.
    let rows = extract_block(1, &trace, &receipts).unwrap();
    assert_eq!(rows[0].pools, PM);
}

#[test]
fn unlock_without_logs_is_a_probe_and_shares_the_pool_with_other_swaps() {
    // A bot that unlocks, reads the state through `extsload` of the swapped pool and
    // returns without swapping: probe, and the pool is the one swapped elsewhere in the block.
    let mut slot = keccak256(&{
        let mut p = [0u8; 64];
        p[..32].copy_from_slice(&hex32(POOL_ID));
        p[63] = 6;
        p
    });
    slot[31] = slot[31].wrapping_add(3); // liquidity word; no carry in this fixture
    let slot_hex: String = slot.iter().map(|b| format!("{b:02x}")).collect();
    let read = call(OTHER, PM, &format!("0x1e2eaeaf{slot_hex}"), json!([]));
    let probe = json!({"type": "CALL", "from": EOA, "to": OTHER, "input": "0x", "value": "0x0",
        "calls": [call(OTHER, PM, "0x48c89491", json!([call(PM, OTHER, "0x91dd7346", json!([read]))]))]});
    let trace = json!([
        {"txHash": "0x01", "result": unlock_swap()},
        {"txHash": "0x02", "result": probe}
    ]);
    let receipts = json!([
        receipt("0x01", 0, "0x1", json!([v4_swap_log(POOL_ID)])),
        receipt("0x02", 1, "0x1", json!([]))
    ]);
    let rows = extract_block_with(1, &trace, &receipts, V4).unwrap();
    assert_eq!(rows[1].kind, Kind::Probe);
    assert_eq!(rows[1].pools, format!("v4:{POOL_ID}"));
    // Default behavior: one shared pseudo-pool for the whole PoolManager.
    let rows = extract_block(1, &trace, &receipts).unwrap();
    assert_eq!(rows[1].pools, PM);
}

#[test]
fn pool_id_argument_of_a_state_view_call_counts() {
    let get_slot0 = call(
        OTHER,
        STATE_VIEW,
        &format!("0xc815641c{}", &POOL_ID[2..]),
        json!([]),
    );
    let probe = json!({"type": "CALL", "from": EOA, "to": OTHER, "input": "0x", "value": "0x0",
        "calls": [get_slot0, call(OTHER, V3_POOL, "0x3850c7bd", json!([]))]});
    let trace = json!([
        {"txHash": "0x01", "result": unlock_swap()},
        {"txHash": "0x02", "result": probe}
    ]);
    let receipts = json!([
        receipt("0x01", 0, "0x1", json!([v4_swap_log(POOL_ID)])),
        receipt("0x02", 1, "0x1", json!([]))
    ]);
    let rows = extract_block_with(1, &trace, &receipts, V4).unwrap();
    assert_eq!(rows[1].kind, Kind::Probe);
    // address pool and v4 pool side by side, sorted
    assert_eq!(
        rows[1].pool_list(),
        vec![V3_POOL, &format!("v4:{POOL_ID}")[..]]
    );
}

#[test]
fn flash_loan_without_swap_touches_no_pool_in_v4_mode() {
    // take/sync/settle without a swap uses the PoolManager as a flash-loan source.
    let take = call(OTHER, PM, "0x0b0d9c09", json!([]));
    let flash = json!({"type": "CALL", "from": EOA, "to": OTHER, "input": "0x", "value": "0x0",
        "calls": [call(OTHER, PM, "0x48c89491", json!([call(PM, OTHER, "0x91dd7346", json!([take]))]))]});
    let trace = json!([{"txHash": "0x01", "result": flash}]);
    let receipts = json!([receipt("0x01", 0, "0x1", json!([]))]);
    let rows = extract_block_with(1, &trace, &receipts, V4).unwrap();
    assert_eq!(rows[0].kind, Kind::Probe);
    assert!(rows[0].pools.is_empty());
}

#[test]
fn slot_of_a_real_extsload_matches_the_pool_state_slot() {
    // Unichain tx 0xfed70c37: swap of pool 0x04b7dd.. and `extsload(0x1078fcb2..)` just before.
    let id = "0x04b7dd024db64cfbe325191c818266e4776918cd9eaf021c26949a859e654b16";
    let observed = "1078fcb22b237a8f66b7e9c34bf49605a6c92519aee3ed30fed21a3c0800e37b";
    let mut pre = [0u8; 64];
    pre[..32].copy_from_slice(&hex32(id));
    pre[63] = 6;
    let base = keccak256(&pre);
    let hex = |d: [u8; 32]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(hex(base), observed, "slot0 word is the base slot");
}

fn hex32(s: &str) -> [u8; 32] {
    let s = s.trim_start_matches("0x");
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
    }
    out
}

#[test]
fn chain_constants() {
    assert_eq!(Chain::Base.alchemy_network(), "base-mainnet");
    assert_eq!(Chain::Unichain.alchemy_network(), "unichain-mainnet");
    assert_eq!(Chain::Op.alchemy_network(), "opt-mainnet");
    assert_eq!(Chain::Unichain.pool_manager(), PM);
    assert_eq!(
        Chain::Op.pool_manager(),
        "0x9a13f98cb987694c9f086b1f5eb990eea8264ec3"
    );
    assert_eq!(
        Chain::Base.pool_manager(),
        "0x498581ff718922c3f8e6a244956af099b2652b2b"
    );
    for c in [Chain::Base, Chain::Unichain, Chain::Op] {
        assert_eq!(c.pool_manager(), c.pool_manager().to_ascii_lowercase());
        assert_eq!(c.weth(), "0x4200000000000000000000000000000000000006");
    }
}

fn timing(c: Chain) -> Timing {
    Timing {
        genesis_ts: c.genesis_ts(),
        block_time: c.block_time(),
    }
}

#[test]
fn chain_clocks_match_pilot_receipts() {
    // blockTimestamp of the receipts of the chain pilot.
    assert_eq!(
        block_timestamp_on(timing(Chain::Unichain), 0x3a59ed0),
        0x6acebe97
    );
    assert_eq!(block_timestamp_on(timing(Chain::Op), 0x96f286f), 0x6ad22a97);
}

#[test]
fn base_timing_is_the_historical_default() {
    let t = parse_date("2025-06-22").unwrap();
    let base = timing(Chain::Base);
    assert_eq!(
        first_block_at_or_after_on(base, t),
        mev_revert_classifier::sample::first_block_at_or_after(t)
    );
    assert_eq!(
        sample_week_on(base, "S1", t, 6, None),
        mev_revert_classifier::sample::sample_week("S1", t, 6, None)
    );
    assert_eq!(
        sample_week_neighbors_on(base, "S1", t, None),
        mev_revert_classifier::sample::sample_week_neighbors("S1", t, None)
    );
}

#[test]
fn unichain_samples_by_clock_time_with_scaled_neighbor_offsets() {
    let t = timing(Chain::Unichain);
    let start = parse_date("2026-10-04").unwrap();
    let plain = sample_week_on(t, "S", start, 4, None);
    assert_eq!(plain.len(), 7 * 24 * 4);
    for b in &plain {
        let ts = block_timestamp_on(t, b.block);
        assert!(ts >= b.hour_start_ts && ts < b.hour_start_ts + 3600);
    }
    let nb = sample_week_neighbors_on(t, "S", start, None);
    let mut anchors = std::collections::HashMap::new();
    for n in nb.iter().filter(|n| n.role == Role::Anchor) {
        anchors.insert(n.anchor, n.block.block);
    }
    for n in nb.iter().filter(|n| n.role != Role::Anchor) {
        assert_eq!(n.block.block, n.anchor + n.role.offset_secs());
        assert_eq!(
            block_timestamp_on(t, n.block.block) - block_timestamp_on(t, n.anchor),
            n.role.offset_secs() as i64
        );
    }
    assert_eq!(Role::N15.offset_on(1), 30);
    assert_eq!(Role::N150.offset_on(1), 300);
    assert_eq!(Role::N150.offset_on(2), Role::N150.offset());
}
