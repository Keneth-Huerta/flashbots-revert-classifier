//! Deterministic block sampling, stratified by UTC hour. The plain functions use Base
//! (2 s blocks); the `_on` variants take the chain's [`Timing`] (AF-03).

use std::collections::HashSet;

use sha2::{Digest, Sha256};

/// Base mainnet: block 0 timestamp. Block `n` has timestamp `GENESIS_TS + 2n`.
pub const GENESIS_TS: i64 = 1_686_789_347;
pub const BLOCK_TIME: i64 = 2;
pub const BLOCKS_PER_HOUR: u64 = 1800;

/// Block clock of a chain: block `n` has timestamp `genesis_ts + block_time * n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub genesis_ts: i64,
    pub block_time: i64,
}

pub const BASE_TIMING: Timing = Timing {
    genesis_ts: GENESIS_TS,
    block_time: BLOCK_TIME,
};

pub const SAMPLE_SALT: &str = "mev-contention-v1";
pub const SPLIT_SALT: &str = "explore-v1";
/// A block is exploratory when the first byte of its split hash is below this (52/256 ~ 20%).
pub const EXPLORE_CUTOFF: u8 = 52;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampledBlock {
    pub stratum: String,
    pub block: u64,
    pub hour_start_ts: i64,
    pub exploratory: bool,
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parses `YYYY-MM-DD` into a UTC midnight timestamp.
pub fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

/// First block whose timestamp is >= `ts`.
pub fn first_block_at_or_after(ts: i64) -> u64 {
    first_block_at_or_after_on(BASE_TIMING, ts)
}

pub fn first_block_at_or_after_on(t: Timing, ts: i64) -> u64 {
    let delta = ts - t.genesis_ts;
    if delta <= 0 {
        0
    } else {
        ((delta + t.block_time - 1) / t.block_time) as u64
    }
}

pub fn block_timestamp(block: u64) -> i64 {
    block_timestamp_on(BASE_TIMING, block)
}

pub fn block_timestamp_on(t: Timing, block: u64) -> i64 {
    t.genesis_ts + t.block_time * block as i64
}

fn hash_u64(salt: &str, block: u64) -> u64 {
    let digest = Sha256::digest(format!("{salt}:{block}").as_bytes());
    u64::from_be_bytes(digest[..8].try_into().expect("8 bytes"))
}

pub fn is_exploratory(block: u64) -> bool {
    Sha256::digest(format!("{SPLIT_SALT}:{block}").as_bytes())[0] < EXPLORE_CUTOFF
}

/// Picks `per_hour` blocks with the smallest sample hash in every UTC hour of the
/// 7 days starting at `week_start_ts`, skipping blocks in `exclude` (inclusive range).
pub fn sample_week(
    stratum: &str,
    week_start_ts: i64,
    per_hour: usize,
    exclude: Option<(u64, u64)>,
) -> Vec<SampledBlock> {
    sample_week_on(BASE_TIMING, stratum, week_start_ts, per_hour, exclude)
}

pub fn sample_week_on(
    timing: Timing,
    stratum: &str,
    week_start_ts: i64,
    per_hour: usize,
    exclude: Option<(u64, u64)>,
) -> Vec<SampledBlock> {
    let mut out = Vec::new();
    for hour in 0..(7 * 24) {
        let start_ts = week_start_ts + hour * 3600;
        let lo = first_block_at_or_after_on(timing, start_ts);
        let hi = first_block_at_or_after_on(timing, start_ts + 3600);
        let mut candidates: Vec<(u64, u64)> = (lo..hi)
            .filter(|b| !matches!(exclude, Some((a, z)) if (a..=z).contains(b)))
            .map(|b| (hash_u64(SAMPLE_SALT, b), b))
            .collect();
        candidates.sort_unstable();
        let mut picked: Vec<u64> = candidates.iter().take(per_hour).map(|&(_, b)| b).collect();
        picked.sort_unstable();
        out.extend(picked.into_iter().map(|block| SampledBlock {
            stratum: stratum.to_string(),
            block,
            hour_start_ts: start_ts,
            exploratory: is_exploratory(block),
        }));
    }
    out
}

/// Role of a block in a sample with neighbors (ADR 004, P-2). The neighbors are defined
/// by wall-clock distance (2 s, 30 s, 5 min); the block offset depends on the block time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Anchor,
    /// Neighbor `b + 1` (2 s).
    N1,
    /// Neighbor `b + 15` (30 s).
    N15,
    /// Neighbor `b + 150` (5 min).
    N150,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Anchor => "anchor",
            Role::N1 => "n1",
            Role::N15 => "n15",
            Role::N150 => "n150",
        }
    }
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "anchor" => Some(Role::Anchor),
            "n1" => Some(Role::N1),
            "n15" => Some(Role::N15),
            "n150" => Some(Role::N150),
            _ => None,
        }
    }
    /// Distance in blocks from the anchor on Base (2 s blocks; 0 for the anchor itself).
    pub fn offset(self) -> u64 {
        self.offset_on(BLOCK_TIME)
    }

    /// Distance in seconds from the anchor.
    pub fn offset_secs(self) -> u64 {
        match self {
            Role::Anchor => 0,
            Role::N1 => 2,
            Role::N15 => 30,
            Role::N150 => 300,
        }
    }

    /// Distance in blocks from the anchor for blocks of `block_time` seconds.
    pub fn offset_on(self, block_time: i64) -> u64 {
        self.offset_secs() / block_time as u64
    }
}

/// A sampled block of the neighbor design: an anchor, or the neighbor of `anchor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeighborBlock {
    pub block: SampledBlock,
    pub role: Role,
    /// Anchor block number (the block itself when `role` is `Anchor`).
    pub anchor: u64,
}

/// Neighbor offsets of the anchor in slot `slot` of an hour: slots 0 and 1 get `b + 1`,
/// slot 2 gets `b + 15` and `b + 150`.
const SLOT_ROLES: [&[Role]; 3] = [&[Role::N1], &[Role::N1], &[Role::N15, Role::N150]];

fn hour_start(ts: i64) -> i64 {
    ts.div_euclid(3600) * 3600
}

/// Like [`sample_week`] for the neighbor design: per UTC hour, 3 anchors (the blocks with
/// the smallest sample hash, in hash order), the first two with neighbor `b + 1` and the
/// third with `b + 15` and `b + 150`. A neighbor never coincides with an anchor, with
/// another neighbor or with an excluded block: the offending anchor is replaced by the
/// next candidate of its hour in hash order, so the result is deterministic.
/// Neighbors inherit the `exploratory` flag of their anchor (the split is by anchor).
pub fn sample_week_neighbors(
    stratum: &str,
    week_start_ts: i64,
    exclude: Option<(u64, u64)>,
) -> Vec<NeighborBlock> {
    sample_week_neighbors_on(BASE_TIMING, stratum, week_start_ts, exclude)
}

pub fn sample_week_neighbors_on(
    timing: Timing,
    stratum: &str,
    week_start_ts: i64,
    exclude: Option<(u64, u64)>,
) -> Vec<NeighborBlock> {
    let excluded = |b: u64| matches!(exclude, Some((a, z)) if (a..=z).contains(&b));
    let mut hours: Vec<(i64, Vec<u64>)> = Vec::new();
    for hour in 0..(7 * 24) {
        let start_ts = week_start_ts + hour * 3600;
        let lo = first_block_at_or_after_on(timing, start_ts);
        let hi = first_block_at_or_after_on(timing, start_ts + 3600);
        let mut candidates: Vec<(u64, u64)> = (lo..hi)
            .filter(|&b| !excluded(b))
            .map(|b| (hash_u64(SAMPLE_SALT, b), b))
            .collect();
        candidates.sort_unstable();
        hours.push((start_ts, candidates.into_iter().map(|(_, b)| b).collect()));
    }
    // Provisional anchors: the 3 smallest hashes of every hour.
    let mut anchors: Vec<Vec<u64>> = hours
        .iter()
        .map(|(_, c)| c.iter().take(3).copied().collect())
        .collect();
    let mut anchor_set: HashSet<u64> = anchors.iter().flatten().copied().collect();
    let mut taken: HashSet<u64> = HashSet::new(); // neighbors already placed
    let neighbors_of = |a: u64, slot: usize| -> Vec<u64> {
        SLOT_ROLES[slot]
            .iter()
            .map(|r| a + r.offset_on(timing.block_time))
            .collect()
    };
    for (h, (_, cands)) in hours.iter().enumerate() {
        let mut next = 3; // next unused candidate of the hour
        for slot in 0..anchors[h].len() {
            loop {
                let a = anchors[h][slot];
                let ns = neighbors_of(a, slot);
                let clash = ns.iter().enumerate().any(|(i, n)| {
                    anchor_set.contains(n)
                        || taken.contains(n)
                        || excluded(*n)
                        || ns[..i].contains(n)
                }) || taken.contains(&a);
                if !clash {
                    taken.extend(ns);
                    break;
                }
                let Some(&c) = cands.get(next) else {
                    // No candidate left in the hour: keep the slot out of the sample.
                    anchor_set.remove(&a);
                    anchors[h][slot] = u64::MAX;
                    break;
                };
                next += 1;
                anchor_set.remove(&a);
                anchor_set.insert(c);
                anchors[h][slot] = c;
            }
        }
    }
    let mut out = Vec::new();
    for ((start_ts, _), slots) in hours.iter().zip(&anchors) {
        for (slot, &a) in slots.iter().enumerate() {
            if a == u64::MAX {
                continue;
            }
            let exploratory = is_exploratory(a);
            let mk = |block: u64, role: Role| NeighborBlock {
                block: SampledBlock {
                    stratum: stratum.to_string(),
                    block,
                    hour_start_ts: if role == Role::Anchor {
                        *start_ts
                    } else {
                        hour_start(block_timestamp_on(timing, block))
                    },
                    exploratory,
                },
                role,
                anchor: a,
            };
            out.push(mk(a, Role::Anchor));
            out.extend(
                SLOT_ROLES[slot]
                    .iter()
                    .map(|&r| mk(a + r.offset_on(timing.block_time), r)),
            );
        }
    }
    out
}
