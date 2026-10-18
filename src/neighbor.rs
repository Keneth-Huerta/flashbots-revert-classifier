//! Neighbor bound of block synchrony (P-2; fundamento `descomposicion-exceso.md` Prop. 3,
//! `contencion-nulos-secundarios.md` §4.3, ADR 004).
//!
//! For a failed attempt `f` of entity `v` in an anchor block `b` and a block `b'`,
//! `s(f, b') = sum_{p in S_b'(f)} w_p / sum_{p in P(f)} w_p`, with `S_b'(f)` the pools of
//! `P(f)` touched by any bot transaction of an entity other than `v` in `b'`. The bot
//! universe, the entities and the weights `w_p = 1/n_p` are those of `contend`, computed on
//! the anchor blocks only (neighbors only supply `S_b'`). `C(0)` is the mean of `s(f, b)`
//! and `C(k)` the mean of `s(f, b + k)`; `U_k = C(0) - C(k)` is taken over the attempts of
//! the anchors that have the neighbor at offset `k` (same attempts in both terms).
//!
//! Python cross-check: `scripts/analisis/vecino.py`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::contend::{self, BotRule, Rng};
use crate::extract::{Kind, TxRow};
use crate::race::interval;
use crate::sample::{block_timestamp_on, NeighborBlock, Role, SampledBlock, Timing, BASE_TIMING};
use crate::system::SystemFilter;

pub const OFFSETS: [Role; 3] = [Role::N1, Role::N15, Role::N150];
pub const OFFSET_LABELS: [&str; 3] = ["2s", "30s", "5min"];
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// Reads a sample file. Files without `role` and `anchor` columns (plain `sample`
/// output) give every block the role anchor, with no neighbors.
pub fn read_neighbor_sample(path: &Path) -> Result<Vec<NeighborBlock>> {
    let mut r =
        csv::Reader::from_path(path).with_context(|| format!("no sample at {}", path.display()))?;
    let headers = r.headers()?.clone();
    let col = |name: &str| headers.iter().position(|h| h == name);
    let (role_c, anchor_c) = (col("role"), col("anchor"));
    let mut v = Vec::new();
    for rec in r.records() {
        let rec = rec?;
        let block: u64 = rec[1].parse()?;
        let role = match role_c {
            Some(c) => Role::parse(&rec[c]).with_context(|| format!("bad role {}", &rec[c]))?,
            None => Role::Anchor,
        };
        let anchor = match anchor_c {
            Some(c) => rec[c].parse()?,
            None => block,
        };
        v.push(NeighborBlock {
            block: SampledBlock {
                stratum: rec[0].to_string(),
                block,
                hour_start_ts: rec[2].parse()?,
                exploratory: &rec[3] == "true",
            },
            role,
            anchor,
        });
    }
    Ok(v)
}

#[derive(Debug, Clone, Copy)]
pub struct NeighborParams {
    pub bootstrap: usize,
    pub seed: u64,
    /// Pooled interval by resampling whole strata (weeks) instead of hours.
    pub week_unit: bool,
    /// Skip pairs whose neighbor block has no row in the rows file (not downloaded).
    pub drop_blocks_without_rows: bool,
}

impl Default for NeighborParams {
    fn default() -> Self {
        NeighborParams {
            bootstrap: 1000,
            seed: 20_261_017,
            week_unit: false,
            drop_blocks_without_rows: false,
        }
    }
}

/// Sums of one anchor hour: `[sum s0, attempts]` and, per offset, `[sum s0 matched,
/// sum s_k, pairs]`.
pub type Sums = [f64; 11];

fn add(a: &mut Sums, b: &Sums, m: f64) {
    for (x, y) in a.iter_mut().zip(b) {
        *x += m * y;
    }
}

/// `[C0, (C0|k, C_k, U_k) x 3]` from sums.
pub fn stats(s: &Sums) -> [f64; 10] {
    let q = |n: f64, d: f64| if d > 0.0 { n / d } else { f64::NAN };
    let mut out = [f64::NAN; 10];
    out[0] = q(s[0], s[1]);
    for o in 0..3 {
        let (m0, mk, n) = (s[2 + 3 * o], s[3 + 3 * o], s[4 + 3 * o]);
        out[1 + 3 * o] = q(m0, n);
        out[2 + 3 * o] = q(mk, n);
        out[3 + 3 * o] = q(m0 - mk, n);
    }
    out
}

#[derive(Debug, Clone)]
pub struct StratumNb {
    pub name: String,
    pub anchors: usize,
    /// Anchors with the neighbor of each offset.
    pub anchors_with: [usize; 3],
    /// Neighbor blocks with no row in the rows file, per offset.
    pub empty_neighbors: [usize; 3],
    pub attempts: usize,
    /// Tallies per anchor hour.
    pub hours: Vec<Sums>,
}

impl StratumNb {
    /// Failed-attempt pairs (f, b + offset) of offset index `o`.
    pub fn pairs(&self, o: usize) -> f64 {
        self.hours.iter().map(|h| h[4 + 3 * o]).sum()
    }
    pub fn total(&self) -> Sums {
        let mut t = [0.0; 11];
        for h in &self.hours {
            add(&mut t, h, 1.0);
        }
        t
    }
}

type Index<'a> = HashMap<&'a str, Vec<u32>>;

/// Pool -> entities (dense ids) with a bot transaction on it in the block.
fn pool_index<'a>(rows: &[&'a TxRow], entity_id: &HashMap<&str, u32>) -> Index<'a> {
    let mut idx: Index = HashMap::new();
    for r in rows {
        let Some(&e) = entity_id.get(r.to.as_str()) else {
            continue;
        };
        for p in r.pool_list() {
            let v = idx.entry(p).or_default();
            if !v.contains(&e) {
                v.push(e);
            }
        }
    }
    idx
}

/// `s(f, b')` for an attempt of entity `v` with pools `pools`; `None` if the weights sum
/// to 0.
fn score(pools: &[&str], v: u32, idx: &Index, w: &HashMap<String, f64>) -> Option<f64> {
    let (mut shared, mut all) = (0.0, 0.0);
    for p in pools {
        let wp = w.get(*p).copied().unwrap_or(0.0);
        all += wp;
        if idx.get(*p).is_some_and(|es| es.iter().any(|&e| e != v)) {
            shared += wp;
        }
    }
    if all > 0.0 {
        Some(shared / all)
    } else {
        None
    }
}

/// Tallies of one stratum. `design` lists its blocks (anchors and neighbors); `rows` are
/// all the rows (any block).
pub fn analyze_stratum(
    name: &str,
    design: &[&NeighborBlock],
    rows: &[TxRow],
    params: &NeighborParams,
) -> Result<StratumNb> {
    analyze_stratum_on(name, design, rows, params, &NbOpts::default())
}

/// Chain-dependent options of [`analyze_stratum_on`].
#[derive(Debug, Clone, Copy)]
pub struct NbOpts<'a> {
    pub timing: Timing,
    /// System and builder transactions to leave out of universe, entities and pools. A
    /// block whose rows are all system rows still counts as downloaded (not empty).
    pub system: Option<&'a SystemFilter>,
}

impl Default for NbOpts<'_> {
    fn default() -> Self {
        NbOpts {
            timing: BASE_TIMING,
            system: None,
        }
    }
}

/// [`analyze_stratum`] on the block clock of another chain: the neighbor at `+2 s`,
/// `+30 s` or `+5 min` must sit `offset_secs / block_time` blocks after its anchor (Base
/// and OP +1/+15/+150, Unichain +2/+30/+300), and the hour units use that clock.
pub fn analyze_stratum_on(
    name: &str,
    design: &[&NeighborBlock],
    rows: &[TxRow],
    params: &NeighborParams,
    opts: &NbOpts,
) -> Result<StratumNb> {
    let timing = opts.timing;
    let anchors: Vec<u64> = {
        let mut a: Vec<u64> = design
            .iter()
            .filter(|n| n.role == Role::Anchor)
            .map(|n| n.block.block)
            .collect();
        a.sort_unstable();
        a.dedup();
        a
    };
    let anchor_set: HashSet<u64> = anchors.iter().copied().collect();
    let mut neighbor_of: HashMap<(u64, usize), u64> = HashMap::new();
    for n in design.iter().filter(|n| n.role != Role::Anchor) {
        if !anchor_set.contains(&n.anchor) {
            bail!(
                "{name}: neighbor {} has no anchor {}",
                n.block.block,
                n.anchor
            );
        }
        let o = OFFSETS.iter().position(|r| *r == n.role).expect("offset");
        let offset = n.role.offset_on(timing.block_time);
        if n.role.offset_secs() % timing.block_time as u64 != 0
            || n.block.block != n.anchor + offset
        {
            bail!(
                "{name}: block {} is not anchor {} + {} ({} s at {} s per block)",
                n.block.block,
                n.anchor,
                offset,
                n.role.offset_secs(),
                timing.block_time
            );
        }
        neighbor_of.insert((n.anchor, o), n.block.block);
    }
    let needed: HashSet<u64> = anchor_set
        .iter()
        .copied()
        .chain(neighbor_of.values().copied())
        .collect();
    let has_rows: HashSet<u64> = rows
        .iter()
        .map(|r| r.block)
        .filter(|b| needed.contains(b))
        .collect();
    let filtered: Vec<TxRow>;
    let rows: &[TxRow] = match opts.system {
        Some(f) => {
            filtered = f.apply(rows).0;
            &filtered
        }
        None => rows,
    };
    let mut by_block: HashMap<u64, Vec<&TxRow>> = HashMap::new();
    for r in rows.iter().filter(|r| needed.contains(&r.block)) {
        by_block.entry(r.block).or_default().push(r);
    }
    // Universe, entities and weights on the anchor blocks only (as `contend`).
    let mine: Vec<TxRow> = rows
        .iter()
        .filter(|r| anchor_set.contains(&r.block))
        .cloned()
        .collect();
    let bots = contend::bot_universe(&mine, &BotRule::default());
    let entity_of = contend::entities(&mine, &bots);
    let ds = contend::build_entities(&mine, &anchors, &entity_of, &HashSet::new());
    let w: HashMap<String, f64> = ds
        .pool_names
        .iter()
        .cloned()
        .zip(ds.pool_weight.iter().copied())
        .collect();
    let mut entity_id: HashMap<&str, u32> = HashMap::new();
    {
        let mut reps: Vec<&str> = entity_of.values().map(String::as_str).collect();
        reps.sort_unstable();
        reps.dedup();
        let rep_id: HashMap<&str, u32> = reps
            .iter()
            .enumerate()
            .map(|(i, &r)| (r, i as u32))
            .collect();
        for (to, rep) in &entity_of {
            entity_id.insert(to.as_str(), rep_id[rep.as_str()]);
        }
    }
    let no_rows: Vec<&TxRow> = Vec::new();
    let rows_of = |b: u64| by_block.get(&b).unwrap_or(&no_rows);

    let mut hour_ix: HashMap<i64, usize> = HashMap::new();
    let mut hours: Vec<Sums> = Vec::new();
    let mut hour_keys: Vec<i64> = anchors
        .iter()
        .map(|&b| block_timestamp_on(timing, b) / 3600)
        .collect();
    hour_keys.sort_unstable();
    hour_keys.dedup();
    for h in hour_keys {
        hour_ix.insert(h, hours.len());
        hours.push([0.0; 11]);
    }
    let (mut attempts, mut anchors_with) = (0usize, [0usize; 3]);
    let mut empty_neighbors = [0usize; 3];
    for &b in &anchors {
        let rb = rows_of(b);
        let idx0 = pool_index(rb, &entity_id);
        let fails: Vec<(u32, Vec<&str>)> = rb
            .iter()
            .filter(|r| matches!(r.kind, Kind::Probe | Kind::Revert) && !r.pools.is_empty())
            .filter_map(|r| entity_id.get(r.to.as_str()).map(|&e| (e, r.pool_list())))
            .collect();
        let s0: Vec<Option<f64>> = fails
            .iter()
            .map(|(v, ps)| score(ps, *v, &idx0, &w))
            .collect();
        let t = &mut hours[hour_ix[&(block_timestamp_on(timing, b) / 3600)]];
        for s in s0.iter().flatten() {
            t[0] += s;
            t[1] += 1.0;
            attempts += 1;
        }
        for (o, _) in OFFSETS.iter().enumerate() {
            let Some(&nb) = neighbor_of.get(&(b, o)) else {
                continue;
            };
            let rn = rows_of(nb);
            if !has_rows.contains(&nb) {
                empty_neighbors[o] += 1;
                if params.drop_blocks_without_rows {
                    continue;
                }
            }
            anchors_with[o] += 1;
            let idxn = pool_index(rn, &entity_id);
            for ((v, ps), s0) in fails.iter().zip(&s0) {
                let (Some(s0), Some(sk)) = (s0, score(ps, *v, &idxn, &w)) else {
                    continue;
                };
                t[2 + 3 * o] += s0;
                t[3 + 3 * o] += sk;
                t[4 + 3 * o] += 1.0;
            }
        }
    }
    Ok(StratumNb {
        name: name.to_string(),
        anchors: anchors.len(),
        anchors_with,
        empty_neighbors,
        attempts,
        hours,
    })
}

fn multiplicities(n: usize, rng: &mut Rng) -> Vec<f64> {
    let mut m = vec![0.0; n];
    for _ in 0..n {
        m[rng.below(n)] += 1.0;
    }
    m
}

fn stream(seed: u64, tags: &[u64]) -> Rng {
    let mut st = seed;
    for &t in tags {
        st = Rng::new(st ^ t.wrapping_mul(GAMMA)).next_u64();
    }
    Rng::new(st)
}

/// Point statistics and their 95% intervals.
pub type Summary = ([f64; 10], [(f64, f64); 10]);

/// Point statistics and 95% intervals (same layout as [`stats`]) per stratum and pooled
/// (last entry). Bootstrap of anchor hours within each stratum; the pooled interval
/// resamples hours within strata, or whole strata with `week_unit`.
pub fn summarize(strata: &[StratumNb], p: &NeighborParams) -> Vec<Summary> {
    let totals: Vec<Sums> = strata.iter().map(StratumNb::total).collect();
    let mut pooled = [0.0; 11];
    for t in &totals {
        add(&mut pooled, t, 1.0);
    }
    // reps[s][r]: sums of stratum s in replicate r.
    let reps: Vec<Vec<Sums>> = strata
        .iter()
        .enumerate()
        .map(|(si, s)| {
            (0..p.bootstrap)
                .map(|r| {
                    let mut rng = stream(p.seed, &[si as u64, 1, r as u64]);
                    let m = multiplicities(s.hours.len(), &mut rng);
                    let mut acc = [0.0; 11];
                    for (h, &k) in s.hours.iter().zip(&m) {
                        add(&mut acc, h, k);
                    }
                    acc
                })
                .collect()
        })
        .collect();
    let ci_of = |draws: Vec<[f64; 10]>| -> [(f64, f64); 10] {
        let mut out = [(f64::NAN, f64::NAN); 10];
        for (k, o) in out.iter_mut().enumerate() {
            *o = interval(draws.iter().map(|d| d[k]).collect());
        }
        out
    };
    let mut out: Vec<Summary> = Vec::new();
    for (si, t) in totals.iter().enumerate() {
        let draws = reps[si].iter().map(stats).collect();
        out.push((stats(t), ci_of(draws)));
    }
    let draws: Vec<[f64; 10]> = (0..p.bootstrap)
        .map(|r| {
            let mut acc = [0.0; 11];
            if p.week_unit {
                let mut rng = stream(p.seed, &[u64::MAX, 2, r as u64]);
                let m = multiplicities(totals.len(), &mut rng);
                for (t, &k) in totals.iter().zip(&m) {
                    add(&mut acc, t, k);
                }
            } else {
                for rs in &reps {
                    add(&mut acc, &rs[r], 1.0);
                }
            }
            stats(&acc)
        })
        .collect();
    out.push((stats(&pooled), ci_of(draws)));
    out
}
