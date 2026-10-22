//! Placebo exposures, selection tests and week-level estimators of AF-04 (dossier ADR 006,
//! fundamento `carrera-placebos-seleccion.md`). Everything here is opt-in: `race` without
//! the AF-04 options never calls it, and the sealed AF-02/AF-03 outputs do not change.
//!
//! A variant of the race test is a transformation of the strata: the exposure `x` is
//! replaced and extra conditioning (a second indicator, the size of `P`) is folded into the
//! entity id, so the cells (entity, bin) of [`crate::race::analyze`] become the cells
//! (entity, bin, extra) without touching the estimator.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::extract::{Kind, TxRow};
use crate::race::{
    interval, multiplicities, prepare, ratio, read_receipts_pub, stream, swaps_full, RaceParams,
    Spec, StratumData, SwapTx, Tx,
};

/// Options of the AF-04 builder beyond [`crate::race::StratumOpts`].
#[derive(Debug, Clone, Copy)]
pub struct ExtOpts<'a> {
    /// Compute [`Ext`] (alternative exposures, empty-`P` transactions, entity keys).
    pub want_ext: bool,
    /// Minimum share of failed attempts that makes a contract a bot (AF-02: 0.5).
    pub bot_share: Option<f64>,
    /// Seed of the donor choice of the pool placebo.
    pub donor_seed: u64,
    /// Uniswap v4 PoolManager: its Swap logs count as pseudo-pools `v4:<PoolId>`.
    pub v4_pool_manager: Option<&'a str>,
}

impl Default for ExtOpts<'_> {
    fn default() -> Self {
        ExtOpts {
            want_ext: false,
            bot_share: None,
            donor_seed: 1,
            v4_pool_manager: None,
        }
    }
}

/// Alternative exposures of one transaction of the universe (aligned with `StratumData::txs`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TxExt {
    /// A swap of another entity LATER in the block on a pool of the tx (temporal placebo).
    pub x_later: bool,
    /// A swap of the SAME entity (another tx) earlier on a pool of the tx.
    pub x_same: bool,
    /// A swap of a non-bot transaction of a sender outside the entity, earlier, on a pool of the tx.
    pub x_user: bool,
    /// Pool placebo: `X` computed with the pools of the donor; `None` without donor.
    pub x_donor: Option<bool>,
    /// `min(|P|, 3)`.
    pub psize: u8,
    /// `X` with the pools of the contract-week rule; `None` if the contract has no swaps that week.
    pub x_c: Option<bool>,
}

/// Bot transaction with empty `P` (left out of the sealed universe), kept for T5.
#[derive(Debug, Clone, Copy)]
pub struct EmptyTx {
    /// `x` is a placeholder (false): the exposure is undefined for these.
    pub tx: Tx,
    pub x_c: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct Ext {
    pub per_tx: Vec<TxExt>,
    pub empty: Vec<EmptyTx>,
    /// Entity key by entity id: the bot contract with most transactions in the entity-week.
    pub ent_keys: Vec<String>,
}

pub struct EmptyRaw {
    pub to: String,
    pub block: u64,
    pub tx_index: u64,
    pub revert: bool,
    pub probe: bool,
    pub x_c: Option<bool>,
}

pub struct PreExt {
    pub by_hash: HashMap<String, TxExt>,
    pub empties: Vec<EmptyRaw>,
    pub key_of: HashMap<String, String>,
}

fn sha_key(seed: u64, block: u64, a: &str, b: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(format!("{seed}:{block}:{a}:{b}").as_bytes());
    h.finalize().into()
}

type Wins<'a> = HashMap<&'a str, Vec<(u64, u32)>>;

fn any_win(wins: &Wins, pools: &[&str], f: impl Fn(u64, u32) -> bool) -> bool {
    pools
        .iter()
        .any(|p| wins.get(p).is_some_and(|v| v.iter().any(|&(i, e)| f(i, e))))
}

/// Pools of the contract-week rule: those with at least 10% of the successful swap
/// incidences of the contract in the week, at most 5, ties by address.
fn contract_pools(counts: &HashMap<(String, String), usize>) -> HashMap<String, Vec<String>> {
    let mut total: HashMap<&str, usize> = HashMap::new();
    for ((c, _), n) in counts {
        *total.entry(c.as_str()).or_default() += n;
    }
    let mut by_c: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for ((c, p), &n) in counts {
        if n as f64 >= 0.10 * total[c.as_str()] as f64 {
            by_c.entry(c.as_str()).or_default().push((n, p.as_str()));
        }
    }
    by_c.into_iter()
        .map(|(c, mut v)| {
            v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
            v.truncate(5);
            (
                c.to_string(),
                v.into_iter().map(|x| x.1.to_string()).collect(),
            )
        })
        .collect()
}

/// Computes the AF-04 extension for the blocks of one stratum (see [`TxExt`]).
pub fn ext_pass(
    mine: &[TxRow],
    bots: &HashSet<String>,
    entity_of: &HashMap<String, String>,
    have: &HashSet<u64>,
    raw_dir: &Path,
    _size: &HashMap<u64, u64>,
    eo: &ExtOpts,
) -> Result<PreExt> {
    // Entity key: the bot contract with most rows in the entity (ties: smallest address).
    let mut rows_of: HashMap<&str, usize> = HashMap::new();
    for r in mine {
        if bots.contains(&r.to) {
            *rows_of.entry(r.to.as_str()).or_default() += 1;
        }
    }
    let mut key_of: HashMap<String, String> = HashMap::new();
    let mut best: HashMap<&str, (usize, &str)> = HashMap::new();
    for (c, &n) in &rows_of {
        if let Some(l) = entity_of.get(*c) {
            let e = best.entry(l.as_str()).or_insert((0, ""));
            if n > e.0 || (n == e.0 && (e.1.is_empty() || *c < e.1)) {
                *e = (n, c);
            }
        }
    }
    for (l, (_, c)) in best {
        key_of.insert(l.to_string(), c.to_string());
    }
    // Senders of every entity (to tell a user swap from a swap of the entity itself).
    let mut senders: HashMap<&str, HashSet<&str>> = HashMap::new();
    for r in mine {
        if let Some(l) = entity_of.get(&r.to) {
            senders
                .entry(l.as_str())
                .or_default()
                .insert(r.from.as_str());
        }
    }
    let bot_hashes: HashSet<String> = mine
        .iter()
        .filter(|r| bots.contains(&r.to))
        .map(|r| r.tx_hash.to_ascii_lowercase())
        .collect();
    let mut by_block: BTreeMap<u64, Vec<&TxRow>> = BTreeMap::new();
    for r in mine {
        if have.contains(&r.block) && entity_of.contains_key(&r.to) {
            by_block.entry(r.block).or_default().push(r);
        }
    }
    // First pass: receipts swaps of the block and contract-week pool counts.
    let mut swaps: BTreeMap<u64, Vec<SwapTx>> = BTreeMap::new();
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for (&b, list) in by_block.iter_mut() {
        list.sort_by_key(|r| r.tx_index);
        let sw = swaps_full(&read_receipts_pub(raw_dir, b)?, eo.v4_pool_manager);
        let by_hash: HashMap<&str, &SwapTx> = sw.iter().map(|s| (s.hash.as_str(), s)).collect();
        for r in list.iter().filter(|r| r.kind == Kind::Success) {
            if let Some(s) = by_hash.get(r.tx_hash.to_ascii_lowercase().as_str()) {
                for p in &s.pools {
                    *counts.entry((r.to.clone(), p.clone())).or_default() += 1;
                }
            }
        }
        swaps.insert(b, sw);
    }
    let pc = contract_pools(&counts);
    let mut out = PreExt {
        by_hash: HashMap::new(),
        empties: Vec::new(),
        key_of,
    };
    for (&b, list) in &by_block {
        let sw = &swaps[&b];
        let by_hash: HashMap<&str, &SwapTx> = sw.iter().map(|s| (s.hash.as_str(), s)).collect();
        let mut lab: HashMap<&str, u32> = HashMap::new();
        for r in list.iter() {
            let n = lab.len() as u32;
            lab.entry(entity_of[&r.to].as_str()).or_insert(n);
        }
        let ent = |r: &TxRow| lab[entity_of[&r.to].as_str()];
        // Wins of the bot transactions: sealed universe (P nonempty) and all bot rows.
        let mut wins_list: Wins = HashMap::new();
        let mut wins_full: Wins = HashMap::new();
        for r in list.iter().filter(|r| r.kind == Kind::Success) {
            if let Some(s) = by_hash.get(r.tx_hash.to_ascii_lowercase().as_str()) {
                for p in &s.pools {
                    wins_full
                        .entry(p.as_str())
                        .or_default()
                        .push((r.tx_index, ent(r)));
                    if !r.pools.is_empty() {
                        wins_list
                            .entry(p.as_str())
                            .or_default()
                            .push((r.tx_index, ent(r)));
                    }
                }
            }
        }
        let mut wins_user: HashMap<&str, Vec<(u64, &str)>> = HashMap::new();
        for s in sw.iter().filter(|s| !bot_hashes.contains(&s.hash)) {
            for p in &s.pools {
                wins_user
                    .entry(p.as_str())
                    .or_default()
                    .push((s.idx, s.from.as_str()));
            }
        }
        let uni: Vec<&&TxRow> = list.iter().filter(|r| !r.pools.is_empty()).collect();
        for r in list.iter() {
            let e = ent(r);
            let i = r.tx_index;
            let label = entity_of[&r.to].as_str();
            let xc = pc.get(&r.to).map(|ps| {
                let ps: Vec<&str> = ps.iter().map(String::as_str).collect();
                any_win(&wins_full, &ps, |j, ej| j < i && ej != e)
            });
            if r.pools.is_empty() {
                out.empties.push(EmptyRaw {
                    to: r.to.clone(),
                    block: b,
                    tx_index: i,
                    revert: r.kind == Kind::Revert,
                    probe: r.kind == Kind::Probe,
                    x_c: xc,
                });
                continue;
            }
            let pools = r.pool_list();
            let x_later = any_win(&wins_list, &pools, |j, ej| j > i && ej != e);
            let x_same = any_win(&wins_list, &pools, |j, ej| j < i && ej == e);
            let x_user = pools.iter().any(|p| {
                wins_user.get(p).is_some_and(|v| {
                    v.iter()
                        .any(|&(j, from)| j < i && !senders[label].contains(from))
                })
            });
            // Donor: same block, universe non-probe, other entity, disjoint pools,
            // |P| closest; ties and final choice by sha256.
            let mine_set: HashSet<&str> = pools.iter().copied().collect();
            let mut best: Option<(usize, [u8; 32], &TxRow)> = None;
            for d in uni.iter().copied().copied() {
                if d.kind == Kind::Probe || ent(d) == e || d.tx_hash == r.tx_hash {
                    continue;
                }
                let dp = d.pool_list();
                if dp.iter().any(|p| mine_set.contains(p)) {
                    continue;
                }
                let gap = dp.len().abs_diff(pools.len());
                let key = sha_key(
                    eo.donor_seed,
                    b,
                    &r.tx_hash.to_ascii_lowercase(),
                    &d.tx_hash.to_ascii_lowercase(),
                );
                if best.as_ref().is_none_or(|bb| (gap, key) < (bb.0, bb.1)) {
                    best = Some((gap, key, d));
                }
            }
            let x_donor = best.map(|(_, _, d)| {
                let dp = d.pool_list();
                any_win(&wins_list, &dp, |j, ej| j < i && ej != e)
            });
            out.by_hash.insert(
                r.tx_hash.to_ascii_lowercase(),
                TxExt {
                    x_later,
                    x_same,
                    x_user,
                    x_donor,
                    psize: pools.len().clamp(1, 3) as u8,
                    x_c: xc,
                },
            );
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- variants

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XDef {
    PriorOther,
    LaterOther,
    DonorPool,
    PriorSame,
    UserPrior,
    AnyPrior,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XAlso {
    None,
    PriorOther,
    UserPrior,
    PriorSame,
    LaterOther,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PRule {
    /// Pools of the transaction trace (sealed rule).
    Trace,
    /// Pools of the contract-week rule, on every bot transaction of a contract with swaps.
    Contract,
    /// Contract rule, only on transactions with nonempty trace `P` (comparison subset).
    ContractSubset,
    /// Trace exposure on the transactions of the contract-rule subset (comparison).
    TraceOnContractSubset,
}

#[derive(Debug, Clone, Copy)]
pub struct Variant {
    pub x_def: XDef,
    pub also: XAlso,
    pub p_rule: PRule,
    pub p_size: bool,
    pub donor_subset: bool,
}

impl Default for Variant {
    fn default() -> Self {
        Variant {
            x_def: XDef::PriorOther,
            also: XAlso::None,
            p_rule: PRule::Trace,
            p_size: false,
            donor_subset: false,
        }
    }
}

impl Variant {
    pub fn is_default(&self) -> bool {
        self.x_def == XDef::PriorOther
            && self.also == XAlso::None
            && self.p_rule == PRule::Trace
            && !self.p_size
            && !self.donor_subset
    }
    pub fn label(&self) -> String {
        format!(
            "x={:?}|also={:?}|p={:?}|psize={}|donor={}",
            self.x_def, self.also, self.p_rule, self.p_size, self.donor_subset
        )
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VariantStats {
    /// Non-probe transactions of the sealed universe before the variant.
    pub universe: usize,
    /// Non-probe transactions kept by the variant.
    pub kept: usize,
}

fn exposure(t: &Tx, e: &TxExt, d: XDef) -> Option<bool> {
    match d {
        XDef::PriorOther => Some(t.x),
        XDef::LaterOther => Some(e.x_later),
        XDef::DonorPool => e.x_donor,
        XDef::PriorSame => Some(e.x_same),
        XDef::UserPrior => Some(e.x_user),
        XDef::AnyPrior => Some(t.x || e.x_user),
    }
}

/// [`apply_variant`] recodes the entity id as `(ent * 4 + g) * 2 + z`; this recovers `ent`.
pub fn orig_ent(e: u32) -> usize {
    (e / 8) as usize
}

/// Applies a variant to the strata. Returns the transformed strata (names, hours and
/// order kept; transactions dropped or recoded) and coverage counts.
pub fn apply_variant(
    strata: &[StratumData],
    exts: &[Ext],
    v: &Variant,
) -> (Vec<StratumData>, VariantStats) {
    let mut stats = VariantStats::default();
    let mut out = Vec::new();
    for (s, ex) in strata.iter().zip(exts) {
        let mut txs = Vec::new();
        let mut push = |t: &Tx, e: &TxExt, x: bool, z: bool| {
            let g = if v.p_size {
                e.psize.max(1) as u32 - 1
            } else {
                3
            };
            let mut t2 = *t;
            t2.x = x;
            t2.ent = (t.ent * 4 + g) * 2 + z as u32;
            txs.push(t2);
        };
        for (t, e) in s.txs.iter().zip(&ex.per_tx) {
            if !t.probe {
                stats.universe += 1;
            }
            if v.donor_subset && e.x_donor.is_none() {
                continue;
            }
            let (x, z) = match v.p_rule {
                PRule::Trace => (exposure(t, e, v.x_def), also(t, e, v.also)),
                PRule::Contract | PRule::ContractSubset => (e.x_c, Some(false)),
                PRule::TraceOnContractSubset => (e.x_c.map(|_| t.x), Some(false)),
            };
            let (Some(x), Some(z)) = (x, z) else {
                continue;
            };
            if !t.probe {
                stats.kept += 1;
            }
            push(t, e, x, z);
        }
        if v.p_rule == PRule::Contract {
            for em in &ex.empty {
                if let Some(x) = em.x_c {
                    if !em.tx.probe {
                        stats.universe += 1;
                        stats.kept += 1;
                    }
                    let e = TxExt {
                        psize: 1,
                        ..TxExt::default()
                    };
                    push(&em.tx, &e, x, false);
                }
            }
        }
        out.push(StratumData {
            name: s.name.clone(),
            txs,
            n_hours: s.n_hours,
        });
    }
    (out, stats)
}

fn also(t: &Tx, e: &TxExt, a: XAlso) -> Option<bool> {
    match a {
        XAlso::None => Some(false),
        XAlso::PriorOther => Some(t.x),
        XAlso::UserPrior => Some(e.x_user),
        XAlso::PriorSame => Some(e.x_same),
        XAlso::LaterOther => Some(e.x_later),
    }
}

// ----------------------------------------------------------- week-level stats

/// `(num, den)` of the (entity, bin) or (entity, block) MH estimator of every stratum.
pub fn week_stats(strata: &[StratumData], with_probes: bool, p: &RaceParams) -> Vec<(f64, f64)> {
    strata
        .iter()
        .map(|s| {
            let t: Vec<&Tx> = s.txs.iter().filter(|t| with_probes || !t.probe).collect();
            let pr = prepare(&t, s.n_hours, Spec::Cond, p);
            (pr.num, pr.den)
        })
        .collect()
}

/// Pooled estimate and 95% interval of the bootstrap of whole strata; the multiplicities
/// are those of `race --boot-unit week` for variant index `vi`.
pub fn week_ci(stats: &[(f64, f64)], vi: u64, p: &RaceParams) -> (f64, (f64, f64)) {
    let obs = ratio(
        stats.iter().map(|x| x.0).sum(),
        stats.iter().map(|x| x.1).sum(),
    );
    (obs, interval(week_boot(stats, vi, p)))
}

pub fn week_boot(stats: &[(f64, f64)], vi: u64, p: &RaceParams) -> Vec<f64> {
    (0..p.bootstrap)
        .map(|r| {
            let mut rng = stream(p.seed, &[u64::MAX, vi, 0, 3, r as u64]);
            let m = multiplicities(stats.len(), &mut rng);
            let (n, d) = stats.iter().zip(&m).fold((0.0, 0.0), |a, (x, &k)| {
                (a.0 + k as f64 * x.0, a.1 + k as f64 * x.1)
            });
            ratio(n, d)
        })
        .collect()
}

/// Contrast `a - b` of two estimands on the same strata, with the interval of the SAME
/// resamples of strata (joint bootstrap). Returns the contrast, its interval, and the two
/// estimates.
pub fn contrast(
    a: &[(f64, f64)],
    b: &[(f64, f64)],
    vi: u64,
    p: &RaceParams,
) -> (f64, (f64, f64), f64, f64) {
    let est = |s: &[(f64, f64)]| ratio(s.iter().map(|x| x.0).sum(), s.iter().map(|x| x.1).sum());
    let (ea, eb) = (est(a), est(b));
    let ba = week_boot(a, vi, p);
    let bb = week_boot(b, vi, p);
    let diff: Vec<f64> = ba.iter().zip(&bb).map(|(x, y)| x - y).collect();
    (ea - eb, interval(diff), ea, eb)
}

// ------------------------------------------------------------ missing grid

#[derive(Debug, Clone)]
pub struct GridPoint {
    pub q_r: f64,
    pub q_s: f64,
    pub d: f64,
}

#[derive(Debug, Clone)]
pub struct GridReport {
    pub points: Vec<GridPoint>,
    pub min: GridPoint,
    pub max: GridPoint,
    /// Smallest grid `q_R` whose least favorable `q_S` gives `d < tau`; `None` if none.
    pub tipping: Option<f64>,
    /// Bootstrap interval (weeks) at the least favorable corner of the grid.
    pub worst_ci: (f64, f64),
    pub n_empty_rev: usize,
    pub n_empty_suc: usize,
}

/// Per stratum, `(num, den)` of the MH estimator when a fraction `q_r` of the empty-`P`
/// reverts and `q_s` of the empty-`P` successes (non-probe) have `X = 1`, within cells (entity, bin).
fn grid_stats(strata: &[StratumData], exts: &[Ext], q_r: f64, q_s: f64) -> Vec<(f64, f64)> {
    strata
        .iter()
        .zip(exts)
        .map(|(s, ex)| {
            let mut cells: BTreeMap<(u32, u32), [f64; 6]> = BTreeMap::new();
            for t in s.txs.iter().filter(|t| !t.probe) {
                let c = cells.entry((t.ent, t.bin)).or_default();
                let (a, b) = if t.x { (0, 1) } else { (2, 3) };
                c[a] += 1.0;
                c[b] += t.y as u8 as f64;
            }
            for e in ex.empty.iter().filter(|e| !e.tx.probe) {
                let c = cells.entry((e.tx.ent, e.tx.bin)).or_default();
                if e.tx.y {
                    c[4] += 1.0;
                } else {
                    c[5] += 1.0;
                }
            }
            let (mut num, mut den) = (0.0, 0.0);
            for c in cells.values() {
                let (mr, ms) = (c[4], c[5]);
                let n1 = c[0] + q_r * mr + q_s * ms;
                let r1 = c[1] + q_r * mr;
                let n0 = c[2] + (1.0 - q_r) * mr + (1.0 - q_s) * ms;
                let r0 = c[3] + (1.0 - q_r) * mr;
                if n1 > 0.0 && n0 > 0.0 {
                    let w = n1 * n0 / (n1 + n0);
                    num += w * (r1 / n1 - r0 / n0);
                    den += w;
                }
            }
            (num, den)
        })
        .collect()
}

/// Missing-data grid of T5a (fundamento §4.2a): estimates on `{0, .1, ..., 1}^2`.
pub fn missing_grid(strata: &[StratumData], exts: &[Ext], tau: f64, p: &RaceParams) -> GridReport {
    let q: Vec<f64> = (0..=10).map(|i| i as f64 / 10.0).collect();
    let mut points = Vec::new();
    for &qr in &q {
        for &qs in &q {
            let st = grid_stats(strata, exts, qr, qs);
            let d = ratio(st.iter().map(|x| x.0).sum(), st.iter().map(|x| x.1).sum());
            points.push(GridPoint {
                q_r: qr,
                q_s: qs,
                d,
            });
        }
    }
    let fin: Vec<&GridPoint> = points.iter().filter(|g| g.d.is_finite()).collect();
    let min = (*fin
        .iter()
        .copied()
        .min_by(|a, b| a.d.total_cmp(&b.d))
        .expect("grid"))
    .clone();
    let max = (*fin
        .iter()
        .copied()
        .max_by(|a, b| a.d.total_cmp(&b.d))
        .expect("grid"))
    .clone();
    let mut tipping = None;
    for &qr in &q {
        let worst = points
            .iter()
            .filter(|g| g.q_r == qr && g.d.is_finite())
            .map(|g| g.d)
            .fold(f64::INFINITY, f64::min);
        if worst < tau {
            tipping = Some(qr);
            break;
        }
    }
    let ws = grid_stats(strata, exts, min.q_r, min.q_s);
    let (_, ci) = week_ci(&ws, 0, p);
    GridReport {
        points,
        min,
        max,
        tipping,
        worst_ci: ci,
        n_empty_rev: exts
            .iter()
            .flat_map(|e| &e.empty)
            .filter(|e| !e.tx.probe && e.tx.y)
            .count(),
        n_empty_suc: exts
            .iter()
            .flat_map(|e| &e.empty)
            .filter(|e| !e.tx.probe && !e.tx.y)
            .count(),
    }
}

/// Reads `variant<TAB>stratum<TAB>num<TAB>den` lines (the `--emit-weeks` file).
pub fn read_weeks(text: &str, variant: &str) -> BTreeMap<String, (f64, f64)> {
    let mut m = BTreeMap::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() == 4 && f[0] == variant {
            if let (Ok(n), Ok(d)) = (f[2].parse::<f64>(), f[3].parse::<f64>()) {
                m.insert(f[1].to_string(), (n, d));
            }
        }
    }
    m
}
