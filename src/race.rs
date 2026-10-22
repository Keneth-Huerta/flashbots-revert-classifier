//! Race test inside the block (P-1; fundamento `contencion-nulos-secundarios.md` §4.6,
//! ADR 004). Mantel-Haenszel risk difference of the revert rate with X = 1 (another entity
//! had a successful swap earlier in the block on a pool of the transaction) minus X = 0,
//! within cells (entity, decile of the position in the block).
//!
//! The Python reference is `scripts/analisis/carrera_tasa.py`; point estimates, cell counts
//! and group sizes agree with it exactly. The bootstrap and permutation draws use another
//! generator, so intervals and p differ from Python by Monte Carlo error only.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde_json::Value;

use crate::contend::Rng;
use crate::extract::{Kind, TxRow, SWAP_TOPICS};
use crate::placebo::{ext_pass, EmptyTx, Ext, ExtOpts, TxExt};
use crate::sample::{block_timestamp_on, Timing, BASE_TIMING};
use crate::system::SystemFilter;

pub const SEED: u64 = 20_261_017;
pub const N_BINS: usize = 10;
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// One bot transaction with pools (non-probe or probe, see [`Tx::probe`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tx {
    pub ent: u32,
    /// Decile (or `--bins` bin) of the position in the block.
    pub bin: u32,
    pub block: u64,
    /// Hour index inside the stratum (bootstrap unit).
    pub hour: u32,
    /// Another entity had a successful swap earlier in the block on a pool of the tx.
    pub x: bool,
    /// The tx is a revert.
    pub y: bool,
    pub probe: bool,
}

#[derive(Debug, Clone)]
pub struct StratumData {
    pub name: String,
    pub txs: Vec<Tx>,
    /// Hours of the stratum with at least one block with raw data (bootstrap draws).
    pub n_hours: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct RaceParams {
    pub bootstrap: usize,
    pub permutations: usize,
    pub seed: u64,
    /// Pooled interval by resampling whole strata (weeks) instead of hours.
    pub week_unit: bool,
    /// Cells (entity, block) instead of (entity, bin).
    pub cell_block: bool,
    pub bins: usize,
}

impl Default for RaceParams {
    fn default() -> Self {
        RaceParams {
            bootstrap: 1000,
            permutations: 1000,
            seed: SEED,
            week_unit: false,
            cell_block: false,
            bins: N_BINS,
        }
    }
}

/// Estimate of one specification (cells with entity, or decile only).
#[derive(Debug, Clone, Copy)]
pub struct Est {
    pub d: f64,
    pub ci: (f64, f64),
    pub p: f64,
    /// Cells with both groups.
    pub cells: usize,
}

#[derive(Debug, Clone)]
pub struct RaceLine {
    pub name: String,
    pub n: usize,
    pub n1: usize,
    pub t0: f64,
    pub t1: f64,
    pub cond: Est,
    pub dec: Est,
}

/// Linear-interpolation percentile (as `numpy.percentile`) of a sorted slice.
pub fn percentile_lin(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let pos = (sorted.len() - 1) as f64 * q / 100.0;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// 95% percentile interval of the finite values, NaN pair if there are none.
pub fn interval(mut v: Vec<f64>) -> (f64, f64) {
    v.retain(|x| !x.is_nan());
    v.sort_by(f64::total_cmp);
    (percentile_lin(&v, 2.5), percentile_lin(&v, 97.5))
}

pub(crate) fn stream(seed: u64, tags: &[u64]) -> Rng {
    let mut st = seed;
    for &t in tags {
        st = Rng::new(st ^ t.wrapping_mul(GAMMA)).next_u64();
    }
    Rng::new(st)
}

fn uniform(r: &mut Rng) -> f64 {
    (r.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

/// Evaluates `job(i)` for `i in 0..n` on all cores; every job seeds its own RNG, so the
/// result does not depend on scheduling.
pub(crate) fn par_map<R: Send, J: Fn(usize) -> R + Sync>(n: usize, job: J) -> Vec<R> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |t| t.get())
        .min(n.max(1));
    let next = AtomicUsize::new(0);
    let mut out: Vec<(usize, R)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut local = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break local;
                        }
                        local.push((i, job(i)));
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker panicked"))
            .collect()
    });
    out.sort_unstable_by_key(|x| x.0);
    out.into_iter().map(|x| x.1).collect()
}

/// (sum of w_c d_c, sum of w_c) over cells `[n1, r1, n0, r0]` with both groups.
pub(crate) fn mh_sums(cells: &[[f64; 4]]) -> (f64, f64) {
    let (mut num, mut den) = (0.0, 0.0);
    for &[n1, r1, n0, r0] in cells {
        if n1 > 0.0 && n0 > 0.0 {
            let w = n1 * n0 / (n1 + n0);
            num += w * (r1 / n1 - r0 / n0);
            den += w;
        }
    }
    (num, den)
}

pub(crate) fn ratio(num: f64, den: f64) -> f64 {
    if den > 0.0 {
        num / den
    } else {
        f64::NAN
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Spec {
    /// (entity, bin) or (entity, block).
    Cond,
    /// Bin only.
    Dec,
}

/// Cells with both groups, pruned (a cell with one group never contributes, in any
/// resample or permutation of X within cells).
pub(crate) struct Prepared {
    /// Totals `[n1, r1, n0, r0]` per cell.
    pub(crate) tot: Vec<[f64; 4]>,
    /// Per hour, the `(cell, [n1, r1, n0, r0])` entries.
    pub(crate) by_hour: Vec<Vec<(u32, [f64; 4])>>,
    pub(crate) num: f64,
    pub(crate) den: f64,
}

pub(crate) fn prepare(txs: &[&Tx], n_hours: usize, spec: Spec, p: &RaceParams) -> Prepared {
    let mut ids: HashMap<(u32, u64), usize> = HashMap::new();
    let mut tot: Vec<[f64; 4]> = Vec::new();
    let mut cell_of = Vec::with_capacity(txs.len());
    for t in txs {
        let key = match spec {
            Spec::Dec => (0, t.bin as u64),
            Spec::Cond if p.cell_block => (t.ent, t.block),
            Spec::Cond => (t.ent, t.bin as u64),
        };
        let next = tot.len();
        let c = *ids.entry(key).or_insert(next);
        if c == next {
            tot.push([0.0; 4]);
        }
        cell_of.push(c);
        let (a, b) = if t.x { (0, 1) } else { (2, 3) };
        tot[c][a] += 1.0;
        tot[c][b] += t.y as u8 as f64;
    }
    let ok: Vec<bool> = tot.iter().map(|c| c[0] > 0.0 && c[2] > 0.0).collect();
    let mut dense = vec![u32::MAX; tot.len()];
    let mut kept = Vec::new();
    for (c, &o) in ok.iter().enumerate() {
        if o {
            dense[c] = kept.len() as u32;
            kept.push(tot[c]);
        }
    }
    let mut by_hour: Vec<HashMap<u32, [f64; 4]>> = vec![HashMap::new(); n_hours];
    for (t, &c) in txs.iter().zip(&cell_of) {
        if dense[c] == u32::MAX {
            continue;
        }
        let e = by_hour[t.hour as usize].entry(dense[c]).or_insert([0.0; 4]);
        let (a, b) = if t.x { (0, 1) } else { (2, 3) };
        e[a] += 1.0;
        e[b] += t.y as u8 as f64;
    }
    let by_hour = by_hour
        .into_iter()
        .map(|m| {
            let mut v: Vec<(u32, [f64; 4])> = m.into_iter().collect();
            v.sort_unstable_by_key(|x| x.0);
            v
        })
        .collect();
    let (num, den) = mh_sums(&kept);
    Prepared {
        tot: kept,
        by_hour,
        num,
        den,
    }
}

impl Prepared {
    /// `(num, den)` of one resample given the multiplicity of every hour.
    pub(crate) fn resample(&self, mult: &[u32]) -> (f64, f64) {
        let mut acc = vec![[0.0; 4]; self.tot.len()];
        for (h, &m) in mult.iter().enumerate() {
            if m == 0 {
                continue;
            }
            for &(c, t) in &self.by_hour[h] {
                for k in 0..4 {
                    acc[c as usize][k] += m as f64 * t[k];
                }
            }
        }
        mh_sums(&acc)
    }

    /// `num` after permuting X within every cell: the reverts of the X = 1 group are a
    /// hypergeometric draw (drawn sequentially, so no state is carried between draws).
    fn permuted_num(&self, rng: &mut Rng) -> f64 {
        let mut num = 0.0;
        for &[n1, r1, n0, r0] in &self.tot {
            let (n, r) = (n1 + n0, r1 + r0);
            if r == 0.0 || r == n {
                continue;
            }
            let k = n1.min(n0) as usize;
            let (mut ones, mut left, mut got) = (r, n, 0.0);
            for _ in 0..k {
                if uniform(rng) * left < ones {
                    got += 1.0;
                    ones -= 1.0;
                }
                left -= 1.0;
            }
            let r1s = if n1 <= n0 { got } else { r - got };
            let w = n1 * n0 / n;
            num += w * (r1s / n1 - (r - r1s) / n0);
        }
        num
    }
}

pub(crate) fn multiplicities(n: usize, rng: &mut Rng) -> Vec<u32> {
    let mut m = vec![0u32; n];
    for _ in 0..n {
        m[rng.below(n)] += 1;
    }
    m
}

/// One specification over all strata: per-stratum and pooled estimates.
fn analyze_spec(
    sets: &[Vec<&Tx>],
    hours: &[usize],
    vi: u64,
    spec: Spec,
    p: &RaceParams,
) -> Vec<Est> {
    let sp = (spec == Spec::Dec) as u64;
    let prep: Vec<Prepared> = sets
        .iter()
        .zip(hours)
        .map(|(t, &h)| prepare(t, h, spec, p))
        .collect();
    let s_count = prep.len();
    // Permutations: per stratum and draw, the numerator after permuting X in cells.
    let perm: Vec<Vec<f64>> = prep
        .iter()
        .enumerate()
        .map(|(si, pr)| {
            par_map(p.permutations, |k| {
                pr.permuted_num(&mut stream(p.seed, &[si as u64, vi, sp, 1, k as u64]))
            })
        })
        .collect();
    // Bootstrap: per stratum and replicate, (num, den) resampling its hours.
    let boot: Vec<Vec<(f64, f64)>> = if p.bootstrap == 0 {
        vec![Vec::new(); s_count]
    } else {
        prep.iter()
            .enumerate()
            .map(|(si, pr)| {
                par_map(p.bootstrap, |r| {
                    let mut rng = stream(p.seed, &[si as u64, vi, sp, 2, r as u64]);
                    let m = multiplicities(pr.by_hour.len().max(1), &mut rng);
                    if pr.by_hour.is_empty() {
                        (f64::NAN, 0.0)
                    } else {
                        pr.resample(&m)
                    }
                })
            })
            .collect()
    };
    let est = |obs_num: f64, obs_den: f64, perm_num: Vec<f64>, boots: Vec<f64>, cells: usize| {
        let obs = ratio(obs_num, obs_den);
        let p_value = if obs.is_nan() {
            f64::NAN
        } else {
            let ge = perm_num
                .iter()
                .filter(|&&x| x / obs_den >= obs - 1e-12)
                .count();
            (1 + ge) as f64 / (p.permutations + 1) as f64
        };
        Est {
            d: obs,
            ci: if cells > 0 {
                interval(boots)
            } else {
                (f64::NAN, f64::NAN)
            },
            p: p_value,
            cells,
        }
    };
    let mut out = Vec::new();
    for (si, pr) in prep.iter().enumerate() {
        let boots = boot[si].iter().map(|&(n, d)| ratio(n, d)).collect();
        out.push(est(pr.num, pr.den, perm[si].clone(), boots, pr.tot.len()));
    }
    // Pooled: cells nest in strata, so the pooled estimator is sum(num) / sum(den).
    let den: f64 = prep.iter().map(|x| x.den).sum();
    let num: f64 = prep.iter().map(|x| x.num).sum();
    let perm_pool: Vec<f64> = (0..p.permutations)
        .map(|k| perm.iter().map(|v| v[k]).sum())
        .collect();
    let boots: Vec<f64> = if p.bootstrap == 0 {
        Vec::new()
    } else if p.week_unit {
        (0..p.bootstrap)
            .map(|r| {
                let mut rng = stream(p.seed, &[u64::MAX, vi, sp, 3, r as u64]);
                let m = multiplicities(s_count, &mut rng);
                let (n, d) = prep.iter().zip(&m).fold((0.0, 0.0), |a, (x, &k)| {
                    (a.0 + k as f64 * x.num, a.1 + k as f64 * x.den)
                });
                ratio(n, d)
            })
            .collect()
    } else {
        (0..p.bootstrap)
            .map(|r| {
                let (n, d) = boot
                    .iter()
                    .fold((0.0, 0.0), |a, v| (a.0 + v[r].0, a.1 + v[r].1));
                ratio(n, d)
            })
            .collect()
    };
    let cells = prep.iter().map(|x| x.tot.len()).sum();
    out.push(est(num, den, perm_pool, boots, cells));
    out
}

/// Race test for one variant (`with_probes`: probes stay with y = 0). Returns one line per
/// stratum and, last, the pooled line (name `POOL`). `vi` only seeds the streams.
pub fn analyze(
    strata: &[StratumData],
    vi: u64,
    with_probes: bool,
    p: &RaceParams,
) -> Vec<RaceLine> {
    let sets: Vec<Vec<&Tx>> = strata
        .iter()
        .map(|s| s.txs.iter().filter(|t| with_probes || !t.probe).collect())
        .collect();
    let hours: Vec<usize> = strata.iter().map(|s| s.n_hours).collect();
    let cond = analyze_spec(&sets, &hours, vi, Spec::Cond, p);
    let dec = analyze_spec(&sets, &hours, vi, Spec::Dec, p);
    let raw = |t: &[&Tx]| {
        let n1 = t.iter().filter(|x| x.x).count();
        let rate = |x: bool| {
            let g: Vec<&&Tx> = t.iter().filter(|v| v.x == x).collect();
            if g.is_empty() {
                f64::NAN
            } else {
                g.iter().filter(|v| v.y).count() as f64 / g.len() as f64
            }
        };
        (t.len(), n1, rate(false), rate(true))
    };
    let mut out = Vec::new();
    for (i, s) in strata.iter().enumerate() {
        let (n, n1, t0, t1) = raw(&sets[i]);
        out.push(RaceLine {
            name: s.name.clone(),
            n,
            n1,
            t0,
            t1,
            cond: cond[i],
            dec: dec[i],
        });
    }
    let all: Vec<&Tx> = sets.iter().flatten().copied().collect();
    let (n, n1, t0, t1) = raw(&all);
    out.push(RaceLine {
        name: "POOL".into(),
        n,
        n1,
        t0,
        t1,
        cond: cond[strata.len()],
        dec: dec[strata.len()],
    });
    out
}

fn f4(v: f64, sign: bool) -> String {
    if v.is_nan() {
        "n/a".into()
    } else if sign {
        format!("{v:+.4}")
    } else {
        format!("{v:.4}")
    }
}

pub fn format_line(variant: &str, l: &RaceLine) -> String {
    let (c, d) = (&l.cond, &l.dec);
    let p3 = |p: f64| {
        if p.is_nan() {
            "n/a".to_string()
        } else {
            format!("{p:.3}")
        }
    };
    format!(
        "{:<4} {:<10} n={:>7} nX1={:>6} celdas={:>4} tasa0={} tasa1={} MH(ent,dec)={} [{},{}] p={} | solo-decil({})={} [{},{}] p={}",
        l.name,
        variant,
        l.n,
        l.n1,
        c.cells,
        f4(l.t0, false),
        f4(l.t1, false),
        f4(c.d, true),
        f4(c.ci.0, true),
        f4(c.ci.1, true),
        p3(c.p),
        d.cells,
        f4(d.d, true),
        f4(d.ci.0, true),
        f4(d.ci.1, true),
        p3(d.p),
    )
}

/// Pools with a Swap log in the successful receipts of a block: tx hash -> pools.
pub fn swaps_by_tx(receipts: &Value) -> HashMap<String, HashSet<String>> {
    swaps_by_tx_with(receipts, None)
}

/// Like [`swaps_by_tx`]. With `pool_manager` (Uniswap v4 singleton), a Swap log of the
/// PoolManager contributes the pseudo-pool `v4:<PoolId>` (`topics[1]`, as the extractor
/// writes it with `--v4-pools`) instead of the PoolManager address.
pub fn swaps_by_tx_with(
    receipts: &Value,
    pool_manager: Option<&str>,
) -> HashMap<String, HashSet<String>> {
    swaps_full(receipts, pool_manager)
        .into_iter()
        .map(|s| (s.hash, s.pools))
        .collect()
}

/// A successful transaction with at least one Swap log.
#[derive(Debug, Clone)]
pub struct SwapTx {
    pub idx: u64,
    pub hash: String,
    pub from: String,
    pub pools: HashSet<String>,
}

/// Successful transactions with Swap logs of a block's receipts, in receipt order.
pub fn swaps_full(receipts: &Value, pool_manager: Option<&str>) -> Vec<SwapTx> {
    let mut out = Vec::new();
    for (pos, rc) in receipts.as_array().into_iter().flatten().enumerate() {
        let status = rc
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(status.as_str(), "0x1" | "0x01" | "1") {
            continue;
        }
        let mut pools = HashSet::new();
        for log in rc
            .get("logs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let topic0 = log
                .get("topics")
                .and_then(|t| t.get(0))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            if SWAP_TOPICS.contains(&topic0.as_str()) {
                if let Some(a) = log.get("address").and_then(Value::as_str) {
                    let a = a.to_ascii_lowercase();
                    if pool_manager == Some(a.as_str()) {
                        if let Some(id) = log
                            .get("topics")
                            .and_then(|t| t.get(1))
                            .and_then(Value::as_str)
                        {
                            pools.insert(format!("v4:{}", id.to_ascii_lowercase()));
                        }
                    } else {
                        pools.insert(a);
                    }
                }
            }
        }
        if pools.is_empty() {
            continue;
        }
        if let Some(h) = rc.get("transactionHash").and_then(Value::as_str) {
            let idx = rc
                .get("transactionIndex")
                .and_then(Value::as_str)
                .and_then(|x| u64::from_str_radix(x.trim_start_matches("0x"), 16).ok())
                .unwrap_or(pos as u64);
            out.push(SwapTx {
                idx,
                hash: h.to_ascii_lowercase(),
                from: rc
                    .get("from")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase(),
                pools,
            });
        }
    }
    out
}

pub fn raw_path(dir: &Path, block: u64, kind: &str) -> std::path::PathBuf {
    dir.join(format!("{block}.{kind}.json.gz"))
}

/// A block has raw data when its receipts are on disk: `race` reads only receipts
/// (AF-02 keeps receipts locally and moves traces to DVC; post-seal deviation).
pub fn has_raw(dir: &Path, block: u64) -> bool {
    raw_path(dir, block, "receipts").is_file()
}

pub fn read_receipts_pub(dir: &Path, block: u64) -> Result<Value> {
    read_receipts(dir, block)
}

fn read_receipts(dir: &Path, block: u64) -> Result<Value> {
    let path = raw_path(dir, block, "receipts");
    let f = File::open(&path).with_context(|| format!("no receipts at {}", path.display()))?;
    let v: Value = serde_json::from_reader(BufReader::new(GzDecoder::new(f)))
        .with_context(|| format!("bad receipts json {}", path.display()))?;
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

/// X of every tx of a block: `list` is `(tx_index, entity, pools, success_swap_pools)`
/// sorted by `tx_index`; X = 1 if another entity had a successful swap earlier in the
/// block on one of the tx's pools.
pub fn x_for_block(list: &[(u64, u32, &[String], HashSet<String>)]) -> Vec<bool> {
    let mut wins: HashMap<&str, Vec<(u64, u32)>> = HashMap::new();
    for (idx, ent, _, swaps) in list {
        for p in swaps {
            wins.entry(p.as_str()).or_default().push((*idx, *ent));
        }
    }
    list.iter()
        .map(|(idx, ent, pools, _)| {
            pools.iter().any(|p| {
                wins.get(p.as_str())
                    .is_some_and(|v| v.iter().any(|&(i, e)| i < *idx && e != *ent))
            })
        })
        .collect()
}

/// Builds the transactions of one stratum from its rows (`mine`: only the stratum's
/// blocks), with the bot universe and entities of `contend`. Blocks with no raw data are
/// left out entirely and returned.
pub fn build_stratum(
    name: &str,
    mine: &[TxRow],
    blocks: &[u64],
    raw_dir: &Path,
    bins: usize,
) -> Result<(StratumData, Vec<u64>)> {
    build_stratum_with(name, mine, blocks, raw_dir, bins, &StratumOpts::default())
}

/// Options of [`build_stratum_with`] beyond the sealed AF-02 defaults.
#[derive(Debug, Clone, Copy)]
pub struct StratumOpts<'a> {
    /// Block clock of the chain (only defines the hour units of the bootstrap).
    pub timing: Timing,
    /// Senders that call at least this many distinct bot contracts are ignored when
    /// building entities (as `contend --max-sender-degree`); `None` keeps AF-02.
    pub max_sender_degree: Option<usize>,
    /// System and builder transactions to leave out before bots, entities and pools are
    /// defined. They still count in the block length that scales the position.
    pub system: Option<&'a SystemFilter>,
}

impl Default for StratumOpts<'_> {
    fn default() -> Self {
        StratumOpts {
            timing: BASE_TIMING,
            max_sender_degree: None,
            system: None,
        }
    }
}

/// [`build_stratum`] under [`StratumOpts`]; the defaults reproduce it exactly.
pub fn build_stratum_with(
    name: &str,
    all: &[TxRow],
    blocks: &[u64],
    raw_dir: &Path,
    bins: usize,
    opts: &StratumOpts,
) -> Result<(StratumData, Vec<u64>)> {
    let (d, _, missing) =
        build_stratum_ext(name, all, blocks, raw_dir, bins, opts, &ExtOpts::default())?;
    Ok((d, missing))
}

/// [`build_stratum_with`] under [`ExtOpts`] (AF-04). With `ExtOpts::default()` the
/// [`StratumData`] is the one of `build_stratum_with` and the extension is `None`; with
/// `want_ext` the extension carries, aligned with `txs`, the alternative exposures
/// of the placebo tests, the transactions with empty `P` and the entity keys.
pub fn build_stratum_ext(
    name: &str,
    all: &[TxRow],
    blocks: &[u64],
    raw_dir: &Path,
    bins: usize,
    opts: &StratumOpts,
    eo: &ExtOpts,
) -> Result<(StratumData, Option<Ext>, Vec<u64>)> {
    let filtered: Vec<TxRow>;
    let mine: &[TxRow] = match opts.system {
        Some(f) => {
            filtered = f.apply(all).0;
            &filtered
        }
        None => all,
    };
    let rule = crate::contend::BotRule {
        min_failed_share: eo
            .bot_share
            .unwrap_or(crate::contend::BotRule::default().min_failed_share),
        ..crate::contend::BotRule::default()
    };
    let bots = crate::contend::bot_universe(mine, &rule);
    let entity_of = crate::contend::entities_with(
        mine,
        &bots,
        crate::contend::EntityRule {
            max_sender_degree: opts.max_sender_degree,
            ..crate::contend::EntityRule::default()
        },
    );
    let mut size: HashMap<u64, u64> = HashMap::new();
    // Block length from every row, system transactions included: they occupy positions.
    for r in all {
        let e = size.entry(r.block).or_default();
        *e = (*e).max(r.tx_index + 1);
    }
    let mut missing = Vec::new();
    let have: HashSet<u64> = blocks
        .iter()
        .copied()
        .filter(|&b| {
            let ok = has_raw(raw_dir, b);
            if !ok {
                missing.push(b);
            }
            ok
        })
        .collect();
    let mut by_block: BTreeMap<u64, Vec<&TxRow>> = BTreeMap::new();
    for r in mine {
        if have.contains(&r.block) && entity_of.contains_key(&r.to) && !r.pools.is_empty() {
            by_block.entry(r.block).or_default().push(r);
        }
    }
    let mut pre = if eo.want_ext {
        Some(ext_pass(
            mine, &bots, &entity_of, &have, raw_dir, &size, eo,
        )?)
    } else {
        None
    };
    let mut per_tx: Vec<TxExt> = Vec::new();
    let mut ent_id: HashMap<&str, u32> = HashMap::new();
    let mut hour_id: HashMap<i64, u32> = HashMap::new();
    let mut txs = Vec::new();
    for (&b, list) in by_block.iter_mut() {
        let next = hour_id.len() as u32;
        let hour = *hour_id
            .entry(block_timestamp_on(opts.timing, b) / 3600)
            .or_insert(next);
        list.sort_by_key(|r| r.tx_index);
        let swaps = if list.iter().any(|r| r.kind == Kind::Success) {
            swaps_by_tx_with(&read_receipts(raw_dir, b)?, eo.v4_pool_manager)
        } else {
            HashMap::new()
        };
        let pools: Vec<Vec<String>> = list
            .iter()
            .map(|r| r.pool_list().into_iter().map(String::from).collect())
            .collect();
        let ents: Vec<u32> = list
            .iter()
            .map(|r| {
                let e = entity_of[&r.to].as_str();
                let next = ent_id.len() as u32;
                *ent_id.entry(e).or_insert(next)
            })
            .collect();
        let items: Vec<(u64, u32, &[String], HashSet<String>)> = list
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let sw = if r.kind == Kind::Success {
                    swaps
                        .get(&r.tx_hash.to_ascii_lowercase())
                        .cloned()
                        .unwrap_or_default()
                } else {
                    HashSet::new()
                };
                (r.tx_index, ents[i], pools[i].as_slice(), sw)
            })
            .collect();
        let x = x_for_block(&items);
        for (i, r) in list.iter().enumerate() {
            let q = r.tx_index as f64 / size[&b] as f64 * bins as f64;
            txs.push(Tx {
                ent: ents[i],
                bin: (q as usize).min(bins - 1) as u32,
                block: b,
                hour,
                x: x[i],
                y: r.kind == Kind::Revert,
                probe: r.kind == Kind::Probe,
            });
            if let Some(p) = pre.as_mut() {
                per_tx.push(
                    p.by_hash
                        .remove(&r.tx_hash.to_ascii_lowercase())
                        .unwrap_or_default(),
                );
            }
        }
    }
    let ext = match pre {
        Some(p) => {
            let mut empty = Vec::new();
            for e in &p.empties {
                let next = ent_id.len() as u32;
                let ent = *ent_id.entry(entity_of[&e.to].as_str()).or_insert(next);
                let next = hour_id.len() as u32;
                let hour = *hour_id
                    .entry(block_timestamp_on(opts.timing, e.block) / 3600)
                    .or_insert(next);
                let q = e.tx_index as f64 / size[&e.block] as f64 * bins as f64;
                empty.push(EmptyTx {
                    tx: Tx {
                        ent,
                        bin: (q as usize).min(bins - 1) as u32,
                        block: e.block,
                        hour,
                        x: false,
                        y: e.revert,
                        probe: e.probe,
                    },
                    x_c: e.x_c,
                });
            }
            let mut ent_keys = vec![String::new(); ent_id.len()];
            for (label, &id) in &ent_id {
                ent_keys[id as usize] = p.key_of.get(*label).cloned().unwrap_or_default();
            }
            Some(Ext {
                per_tx,
                empty,
                ent_keys,
            })
        }
        None => None,
    };
    missing.sort_unstable();
    Ok((
        StratumData {
            name: name.to_string(),
            txs,
            n_hours: hour_id.len(),
        },
        ext,
        missing,
    ))
}
