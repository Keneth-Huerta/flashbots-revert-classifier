//! Turns a `callTracer` block trace plus its receipts into one row per transaction,
//! with the pools each transaction touched (fundamento §4.2) and its kind (§4.3).

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::keccak::keccak256;

/// Selectors of pool reads or swaps (fundamento §4.2).
pub const POOL_SELECTORS: &[&str] = &[
    "0x0902f1ac", // getReserves()
    "0x3850c7bd", // slot0()
    "0xe76c01e4", // globalState()
    "0x1a686502", // liquidity()
    "0x022c0d9f", // swap V2
    "0x128acb08", // swap V3
    "0x1e2eaeaf", // extsload(bytes32)
    "0x35fd631a", // extsload(bytes32,uint256)
    "0xdbd035ff", // extsload(bytes32[])
    "0x0dfe1681", // token0()
    "0xd21220a7", // token1()
    "0xf30dba93", // ticks(int24)
    "0x5339c296", // tickBitmap(int16)
    "0x514ea4bf", // positions(bytes32)
];

/// Swap event topics: Uniswap V2, V3 (also Slipstream), V4, Aerodrome, PancakeSwap V3.
pub const SWAP_TOPICS: &[&str] = &[
    "0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822",
    "0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67",
    "0x40e9cecb9f5f1f1c5b9c97dec2917b7ee92e57ba5563708daca94dd84ad7112f",
    "0xb3e2773606abfd36b5bd91394b3a54d1398336c65005baf7bf7a05efeffaf75b",
    "0x19b47279256b2a23a1665c810c8d55a1758940ee09377d4f8d26497a3577dc83",
];

/// Uniswap v4 `Swap(bytes32 indexed id, address indexed sender, int128 amount0,
/// int128 amount1, uint160 sqrtPriceX96, uint128 liquidity, int24 tick, uint24 fee)`,
/// emitted by the PoolManager. `topics[1]` is the pool id.
pub const V4_SWAP_TOPIC: &str =
    "0x40e9cecb9f5f1f1c5b9c97dec2917b7ee92e57ba5563708daca94dd84ad7112f";
/// PoolManager `swap((address,address,uint24,int24,address),(bool,int256,uint160),bytes)`:
/// the pool key (5 static words) starts right after the selector.
pub const V4_SWAP_SELECTOR: &str = "0xf3cd914c";
/// PoolManager `extsload` variants: `(bytes32)`, `(bytes32,uint256)`, `(bytes32[])`.
const EXTSLOAD_SELECTORS: &[&str] = &["0x1e2eaeaf", "0x35fd631a", "0xdbd035ff"];
/// Storage slot of `mapping(PoolId => Pool.State) _pools` in the PoolManager.
const V4_POOLS_SLOT: u8 = 6;
/// Words of the pool state that `extsload` reads by id: slot0, the two fee growth
/// globals and liquidity (`StateLibrary.getSlot0`, `getFeeGrowthGlobals`, `getLiquidity`).
const V4_STATE_WORDS: u8 = 4;
/// Prefix of the pseudo-pool of a v4 pool id in `TxRow::pools`.
pub const V4_PREFIX: &str = "v4:";

pub const TRANSFER_TOPIC: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
pub const BALANCE_OF: &str = "0x70a08231";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Probe,
    Revert,
    Success,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TxRow {
    pub block: u64,
    pub tx_index: u64,
    pub tx_hash: String,
    pub from: String,
    pub to: String,
    pub kind: Kind,
    pub gas_used: u64,
    pub fee_wei: u128,
    pub swap_logs: usize,
    pub cyclic: bool,
    /// Sorted pools joined by `;`: addresses, and with `V4Mode` also `v4:<PoolId>`.
    pub pools: String,
}

impl TxRow {
    pub fn pool_list(&self) -> Vec<&str> {
        if self.pools.is_empty() {
            Vec::new()
        } else {
            self.pools.split(';').collect()
        }
    }
}

fn hex_u128(v: Option<&Value>) -> u128 {
    v.and_then(Value::as_str)
        .and_then(|s| u128::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0)
}

fn lower(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").to_ascii_lowercase()
}

/// Address in the first 32-byte argument of a call (`balanceOf(address)`).
fn first_arg_address(input: &str) -> Option<String> {
    let arg = input.get(10..74)?;
    if arg.len() == 64 && arg[..24].bytes().all(|c| c == b'0') {
        Some(format!("0x{}", arg[24..].to_ascii_lowercase()))
    } else {
        None
    }
}

/// Uniswap v4 pool detection (fundamento `uniswap-v4-pools.md`). Off by default so the
/// outputs of Base, AF-01 and AF-02 do not change.
#[derive(Debug, Clone, Copy)]
pub struct V4Mode<'a> {
    /// Lowercase address of the chain's PoolManager.
    pub pool_manager: &'a str,
}

type PoolId = [u8; 32];

fn hex32(id: &PoolId) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_hex32(s: &str) -> Option<PoolId> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

/// `PoolId = keccak256(abi.encode(PoolKey))`; the key is the first 160 bytes after the
/// selector of `swap` (a static struct is encoded inline).
fn pool_id_of_swap_input(input: &str) -> Option<PoolId> {
    let key = input.get(10..10 + 5 * 64)?;
    let mut bytes = [0u8; 160];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(key.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(keccak256(&bytes))
}

fn add_small(mut word: [u8; 32], k: u8) -> [u8; 32] {
    let mut carry = k as u16;
    for b in word.iter_mut().rev() {
        let v = *b as u16 + carry;
        *b = v as u8;
        carry = v >> 8;
        if carry == 0 {
            break;
        }
    }
    word
}

/// Storage slots `extsload` uses to read the state of pool `id`:
/// `keccak256(id . POOLS_SLOT) + k`.
fn state_slots(id: &PoolId) -> impl Iterator<Item = [u8; 32]> {
    let mut preimage = [0u8; 64];
    preimage[..32].copy_from_slice(id);
    preimage[63] = V4_POOLS_SLOT;
    let base = keccak256(&preimage);
    (0..V4_STATE_WORDS).map(move |k| add_small(base, k))
}

#[derive(Default)]
struct CallSummary {
    callees: HashSet<String>,
    balance_args: HashSet<String>,
    pool_selector_callees: HashSet<String>,
    /// v4 pool ids of the `swap` calls to the PoolManager (also in reverted calls).
    v4_swaps: HashSet<PoolId>,
}

/// Iterative (explicit stack): traces can nest deeper than the native stack allows.
fn walk(root: &Value, out: &mut CallSummary, v4: Option<V4Mode>) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        walk_node(node, out, &mut stack, v4);
    }
}

/// Words of a call input after the selector, as 64-char hex slices.
fn input_words(input: &str) -> impl Iterator<Item = &str> {
    (10..input.len().saturating_sub(63))
        .step_by(64)
        .filter_map(move |i| input.get(i..i + 64))
}

/// v4 pools a transaction touches beyond its own `swap` calls: a pool id passed as an
/// argument (`StateView.getSlot0(id)`) or a state slot read with `extsload`. Only ids of
/// the block's universe can match.
fn scan_v4_reads(
    root: &Value,
    pm: &str,
    ids: &HashSet<String>,
    slots: &HashMap<String, PoolId>,
) -> HashSet<PoolId> {
    let mut found = HashSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        for call in node
            .get("calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            stack.push(call);
            let input = call.get("input").and_then(Value::as_str).unwrap_or("");
            let is_ext = lower(call.get("to")) == pm
                && input
                    .get(..10)
                    .is_some_and(|s| EXTSLOAD_SELECTORS.contains(&s.to_ascii_lowercase().as_str()));
            for w in input_words(input) {
                let w = w.to_ascii_lowercase();
                if ids.contains(&w) {
                    found.extend(parse_hex32(&w));
                } else if is_ext {
                    found.extend(slots.get(&w).copied());
                }
            }
        }
    }
    found
}

fn walk_node<'a>(
    node: &'a Value,
    out: &mut CallSummary,
    stack: &mut Vec<&'a Value>,
    v4: Option<V4Mode>,
) {
    let Some(calls) = node.get("calls").and_then(Value::as_array) else {
        return;
    };
    for call in calls {
        let to = lower(call.get("to"));
        let input = call.get("input").and_then(Value::as_str).unwrap_or("");
        let selector = input.get(..10).unwrap_or("").to_ascii_lowercase();
        if let Some(v4) = v4 {
            if to == v4.pool_manager && selector == V4_SWAP_SELECTOR {
                out.v4_swaps.extend(pool_id_of_swap_input(input));
            }
        }
        if !to.is_empty() {
            if POOL_SELECTORS.contains(&selector.as_str()) {
                out.pool_selector_callees.insert(to.clone());
            }
            out.callees.insert(to);
        }
        if selector == BALANCE_OF {
            if let Some(a) = first_arg_address(input) {
                out.balance_args.insert(a);
            }
        }
        stack.push(call);
    }
}

pub fn extract_block(block: u64, trace: &Value, receipts: &Value) -> Result<Vec<TxRow>> {
    extract_block_with(block, trace, receipts, None)
}

/// Like [`extract_block`]. With `v4`, the PoolManager stops being one address-pool and
/// each v4 pool becomes the pseudo-pool `v4:0x<PoolId>`.
pub fn extract_block_with(
    block: u64,
    trace: &Value,
    receipts: &Value,
    v4: Option<V4Mode>,
) -> Result<Vec<TxRow>> {
    let traces = trace
        .as_array()
        .ok_or_else(|| anyhow!("block {block}: trace is not an array"))?;
    let receipts = receipts
        .as_array()
        .ok_or_else(|| anyhow!("block {block}: receipts is not an array"))?;
    let by_hash: HashMap<String, &Value> = receipts
        .iter()
        .map(|r| (lower(r.get("transactionHash")), r))
        .collect();

    let mut summaries = Vec::with_capacity(traces.len());
    let mut universe: HashSet<String> = HashSet::new();
    for item in traces {
        let hash = lower(item.get("txHash"));
        let root = item
            .get("result")
            .with_context(|| format!("block {block}: trace without result for {hash}"))?;
        let mut summary = CallSummary::default();
        walk(root, &mut summary, v4);
        universe.extend(summary.pool_selector_callees.iter().cloned());
        summaries.push((hash, root, summary));
    }
    for r in receipts {
        for log in r
            .get("logs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let topic0 = lower(log.get("topics").and_then(|t| t.get(0)));
            if SWAP_TOPICS.contains(&topic0.as_str()) {
                universe.insert(lower(log.get("address")));
            }
        }
    }
    // v4 universe of the block: pools swapped (in calls, also reverted, or in logs).
    let mut v4_by_tx: HashMap<String, HashSet<PoolId>> = HashMap::new();
    let mut v4_scan: Option<(HashSet<String>, HashMap<String, PoolId>)> = None;
    if let Some(v4) = v4 {
        universe.remove(v4.pool_manager);
        let mut v4_ids: HashSet<PoolId> = HashSet::new();
        for (hash, _, summary) in &summaries {
            v4_ids.extend(summary.v4_swaps.iter().copied());
            v4_by_tx
                .entry(hash.clone())
                .or_default()
                .extend(summary.v4_swaps.iter().copied());
        }
        for r in receipts {
            let hash = lower(r.get("transactionHash"));
            for log in r
                .get("logs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let topics = log.get("topics").and_then(Value::as_array);
                if lower(log.get("address")) == v4.pool_manager
                    && lower(topics.and_then(|t| t.first())) == V4_SWAP_TOPIC
                {
                    if let Some(id) = parse_hex32(&lower(topics.and_then(|t| t.get(1)))) {
                        v4_ids.insert(id);
                        v4_by_tx.entry(hash.clone()).or_default().insert(id);
                    }
                }
            }
        }
        let ids: HashSet<String> = v4_ids.iter().map(hex32).collect();
        let slots: HashMap<String, PoolId> = v4_ids
            .iter()
            .flat_map(|id| state_slots(id).map(move |s| (hex32(&s), *id)))
            .collect();
        v4_scan = Some((ids, slots));
    }

    let mut rows = Vec::with_capacity(summaries.len());
    for (hash, root, summary) in summaries {
        let Some(receipt) = by_hash.get(&hash) else {
            bail!("block {block}: no receipt for {hash}");
        };
        let to = lower(root.get("to"));
        let logs: Vec<&Value> = receipt
            .get("logs")
            .and_then(Value::as_array)
            .map(|v| v.iter().collect())
            .unwrap_or_default();
        let external_logs = logs.iter().any(|l| lower(l.get("address")) != to);
        let status = hex_u128(receipt.get("status"));
        let value = hex_u128(root.get("value"));
        let kind = if status == 0 {
            Kind::Revert
        } else if !external_logs && value <= 1 {
            Kind::Probe
        } else {
            Kind::Success
        };

        let mut swap_logs = 0;
        let (mut sent, mut received) = (HashSet::new(), HashSet::new());
        for l in &logs {
            let topics = l.get("topics").and_then(Value::as_array);
            let t = |i: usize| lower(topics.and_then(|t| t.get(i)));
            let topic0 = t(0);
            if SWAP_TOPICS.contains(&topic0.as_str()) {
                swap_logs += 1;
            } else if topic0 == TRANSFER_TOPIC && to.len() == 42 {
                let token = lower(l.get("address"));
                let padded = |s: String| s.get(26..).map(|a| format!("0x{a}"));
                let (src, dst) = (padded(t(1)), padded(t(2)));
                // Transfers with the transaction sender are user flows through a router,
                // not legs of an arbitrage cycle.
                let sender = Some(lower(root.get("from")));
                if src.as_deref() == Some(&to) && dst != sender {
                    sent.insert(token.clone());
                }
                if dst.as_deref() == Some(&to) && src != sender {
                    received.insert(token);
                }
            }
        }
        let cyclic = kind == Kind::Success && swap_logs >= 2 && !sent.is_disjoint(&received);

        let mut pools: BTreeSet<String> = summary
            .callees
            .iter()
            .chain(summary.balance_args.iter())
            .filter(|a| universe.contains(*a) && **a != to)
            .cloned()
            .collect();
        if let (Some(v4), Some((ids, slots))) = (v4, &v4_scan) {
            let mut touched = v4_by_tx.remove(&hash).unwrap_or_default();
            if !ids.is_empty() {
                touched.extend(scan_v4_reads(root, v4.pool_manager, ids, slots));
            }
            pools.extend(
                touched
                    .iter()
                    .map(|id| format!("{V4_PREFIX}0x{}", hex32(id))),
            );
        }
        let gas_used = hex_u128(receipt.get("gasUsed")) as u64;
        let fee_wei = (gas_used as u128) * hex_u128(receipt.get("effectiveGasPrice"))
            + hex_u128(receipt.get("l1Fee"));

        rows.push(TxRow {
            block,
            tx_index: hex_u128(receipt.get("transactionIndex")) as u64,
            tx_hash: hash,
            from: lower(root.get("from")),
            to,
            kind,
            gas_used,
            fee_wei,
            swap_logs,
            cyclic,
            pools: pools.into_iter().collect::<Vec<_>>().join(";"),
        });
    }
    Ok(rows)
}
