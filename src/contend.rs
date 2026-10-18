//! Contention ratio, permutation null and stratified block bootstrap
//! (fundamento `contencion-entre-bots.md` §4.1-§4.6).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::extract::{Kind, TxRow};
use crate::sample::{block_timestamp_on, Timing, BASE_TIMING};

/// SplitMix64: small, seedable, good enough for permutations and resampling.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn from_state(state: u64) -> Self {
        Rng(state)
    }
    pub fn state(&self) -> u64 {
        self.0
    }
    /// Skips `calls` outputs of `next_u64` (the state is a plain counter).
    pub fn advance(&mut self, calls: u64) {
        self.0 = self
            .0
            .wrapping_add(calls.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    }
    /// Uniform integer in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        ((self.next_u64() as u128 * n as u128) >> 64) as usize
    }
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

#[derive(Debug, Clone)]
pub struct BotRule {
    pub min_failed: usize,
    /// Minimum share of the contract's transactions that are failed attempts.
    pub min_failed_share: f64,
    pub require_cyclic: bool,
}

impl Default for BotRule {
    fn default() -> Self {
        BotRule {
            min_failed: 5,
            min_failed_share: 0.5,
            require_cyclic: false,
        }
    }
}

fn is_failed(k: Kind) -> bool {
    matches!(k, Kind::Probe | Kind::Revert)
}

/// Bot universe of §4.1: contracts with at least `min_failed` failed attempts that touch
/// pools and whose traffic is mostly failed attempts (public routers almost always succeed).
pub fn bot_universe(rows: &[TxRow], rule: &BotRule) -> HashSet<String> {
    // (all txs, failed txs, failed txs with pools)
    let mut count: HashMap<&str, (usize, usize, usize)> = HashMap::new();
    let mut cyclic: HashSet<&str> = HashSet::new();
    for r in rows {
        if r.cyclic {
            cyclic.insert(&r.to);
        }
        let e = count.entry(&r.to).or_default();
        e.0 += 1;
        if is_failed(r.kind) {
            e.1 += 1;
            if !r.pools.is_empty() {
                e.2 += 1;
            }
        }
    }
    count
        .into_iter()
        .filter(|(to, (all, failed, with_pools))| {
            *with_pools >= rule.min_failed
                && *failed as f64 >= rule.min_failed_share * *all as f64
                && (!rule.require_cyclic || cyclic.contains(to))
        })
        .map(|(to, _)| to.to_string())
        .collect()
}

/// How bot contracts are merged into entities (fundamento nulos secundarios §4.4 c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityRule {
    /// Distinct senders two contracts must share to be merged (1 = the original rule).
    pub min_shared: usize,
    /// Senders that call at least this many distinct bot contracts are ignored.
    pub max_sender_degree: Option<usize>,
}

impl Default for EntityRule {
    fn default() -> Self {
        EntityRule {
            min_shared: 1,
            max_sender_degree: None,
        }
    }
}

/// Groups bot contracts into entities: contracts that share any sender (`from`) belong
/// to the same operator (union-find over the contract-sender graph). Returns
/// contract -> entity representative (the smallest contract address of the component).
pub fn entities(rows: &[TxRow], bots: &HashSet<String>) -> HashMap<String, String> {
    entities_with(rows, bots, EntityRule::default())
}

/// Like [`entities`], under an alternative [`EntityRule`]. The default rule gives the
/// same partition and representatives as [`entities`].
pub fn entities_with(
    rows: &[TxRow],
    bots: &HashSet<String>,
    rule: EntityRule,
) -> HashMap<String, String> {
    let mut names: Vec<&str> = bots.iter().map(String::as_str).collect();
    names.sort_unstable();
    let index: HashMap<&str, usize> = names.iter().enumerate().map(|(i, &n)| (n, i)).collect();
    let mut parent: Vec<usize> = (0..names.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    fn union(p: &mut [usize], x: usize, y: usize) {
        let (a, b) = (find(p, x), find(p, y));
        if a != b {
            // Keep the smaller index as root so the representative is the smallest address.
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            p[hi] = lo;
        }
    }
    let mut contracts_of: HashMap<&str, Vec<usize>> = HashMap::new();
    for r in rows {
        if let Some(&c) = index.get(r.to.as_str()) {
            contracts_of.entry(&r.from).or_default().push(c);
        }
    }
    let mut pair_count: HashMap<(usize, usize), usize> = HashMap::new();
    for cs in contracts_of.values_mut() {
        cs.sort_unstable();
        cs.dedup();
        if rule.max_sender_degree.is_some_and(|k| cs.len() >= k) {
            continue;
        }
        if rule.min_shared <= 1 {
            for &c in &cs[1..] {
                union(&mut parent, cs[0], c);
            }
        } else {
            for (i, &x) in cs.iter().enumerate() {
                for &y in &cs[i + 1..] {
                    *pair_count.entry((x, y)).or_default() += 1;
                }
            }
        }
    }
    for ((x, y), n) in pair_count {
        if n >= rule.min_shared {
            union(&mut parent, x, y);
        }
    }
    (0..names.len())
        .map(|i| {
            let root = find(&mut parent, i);
            (names[i].to_string(), names[root].to_string())
        })
        .collect()
}

/// Pools touched by the most bot transactions, for the hub-pool sensitivity check.
pub fn top_pools(rows: &[TxRow], bots: &HashSet<String>, n: usize) -> HashSet<String> {
    let mut count: HashMap<&str, usize> = HashMap::new();
    for r in rows.iter().filter(|r| bots.contains(&r.to)) {
        for p in r.pool_list() {
            *count.entry(p).or_default() += 1;
        }
    }
    let mut v: Vec<(&str, usize)> = count.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    v.into_iter().take(n).map(|(p, _)| p.to_string()).collect()
}

#[derive(Debug, Clone)]
pub struct Bundle {
    pub bot: u32,
    /// Union of pools over every transaction of the bot in the block.
    pub pools: Vec<u32>,
    /// Pools of each failed attempt in the bundle.
    pub failed: Vec<Vec<u32>>,
}

#[derive(Debug, Clone, Default)]
pub struct Hour {
    /// One entry per sampled block of the hour (empty blocks included).
    pub blocks: Vec<Vec<Bundle>>,
}

#[derive(Debug, Clone)]
pub struct Dataset {
    pub hours: Vec<Hour>,
    /// Hour (Unix seconds / 3600) of each entry of `hours`.
    pub hour_keys: Vec<i64>,
    pub bots: usize,
    pub failed_by_bots: usize,
    pub failed_with_pools: usize,
    /// $w_p = 1/n_p$ per pool id: $n_p$ is the number of entity-block bundles touching it.
    pub pool_weight: Vec<f64>,
    /// Address of each pool id (same indexing as `pool_weight`).
    pub pool_names: Vec<String>,
}

/// Builds the per-hour bundles. `blocks` lists every sampled block, so blocks with no
/// bot activity still count as slots for the null.
pub fn build(
    rows: &[TxRow],
    blocks: &[u64],
    bots: &HashSet<String>,
    exclude_pools: &HashSet<String>,
) -> Dataset {
    let identity: HashMap<String, String> = bots.iter().map(|b| (b.clone(), b.clone())).collect();
    build_entities(rows, blocks, &identity, exclude_pools)
}

/// Like [`build`], but bundles are formed per entity (`entity_of` maps each bot contract
/// to its entity), so contention is only counted between distinct entities.
pub fn build_entities(
    rows: &[TxRow],
    blocks: &[u64],
    entity_of: &HashMap<String, String>,
    exclude_pools: &HashSet<String>,
) -> Dataset {
    build_entities_on(BASE_TIMING, rows, blocks, entity_of, exclude_pools)
}

/// Like [`build_entities`] with the block clock of another chain (it only defines which
/// blocks share an hour).
pub fn build_entities_on(
    timing: Timing,
    rows: &[TxRow],
    blocks: &[u64],
    entity_of: &HashMap<String, String>,
    exclude_pools: &HashSet<String>,
) -> Dataset {
    let mut bot_id: HashMap<&str, u32> = HashMap::new();
    let mut pool_id: HashMap<&str, u32> = HashMap::new();
    let mut by_block: HashMap<u64, HashMap<u32, Bundle>> = HashMap::new();
    let (mut failed_by_bots, mut failed_with_pools) = (0, 0);

    for r in rows.iter() {
        let Some(entity) = entity_of.get(&r.to) else {
            continue;
        };
        let failed = is_failed(r.kind);
        if failed {
            failed_by_bots += 1;
        }
        let mut pools: Vec<u32> = r
            .pool_list()
            .into_iter()
            .filter(|p| !exclude_pools.contains(*p))
            .map(|p| {
                let next = pool_id.len() as u32;
                *pool_id.entry(p).or_insert(next)
            })
            .collect();
        if pools.is_empty() {
            continue;
        }
        pools.sort_unstable();
        let next = bot_id.len() as u32;
        let bot = *bot_id.entry(entity.as_str()).or_insert(next);
        let bundle = by_block
            .entry(r.block)
            .or_default()
            .entry(bot)
            .or_insert_with(|| Bundle {
                bot,
                pools: Vec::new(),
                failed: Vec::new(),
            });
        bundle.pools.extend(&pools);
        if failed {
            failed_with_pools += 1;
            bundle.failed.push(pools);
        }
    }

    let mut hours: HashMap<i64, Vec<u64>> = HashMap::new();
    for &b in blocks {
        hours
            .entry(block_timestamp_on(timing, b) / 3600)
            .or_default()
            .push(b);
    }
    let mut keys: Vec<i64> = hours.keys().copied().collect();
    keys.sort_unstable();
    let hour_keys = keys.clone();
    let hours = keys
        .into_iter()
        .map(|h| {
            let mut bs = hours.remove(&h).unwrap_or_default();
            bs.sort_unstable();
            Hour {
                blocks: bs
                    .into_iter()
                    .map(|b| {
                        let mut v: Vec<Bundle> = by_block
                            .remove(&b)
                            .map(|m| m.into_values().collect())
                            .unwrap_or_default();
                        for bundle in &mut v {
                            bundle.pools.sort_unstable();
                            bundle.pools.dedup();
                        }
                        v.sort_by_key(|x| x.bot);
                        v
                    })
                    .collect(),
            }
        })
        .collect::<Vec<Hour>>();

    let mut touches = vec![0u32; pool_id.len()];
    for b in hours.iter().flat_map(|h| h.blocks.iter().flatten()) {
        for &p in &b.pools {
            touches[p as usize] += 1;
        }
    }
    let pool_weight = touches
        .into_iter()
        .map(|n| if n == 0 { 0.0 } else { 1.0 / n as f64 })
        .collect();

    let mut pool_names = vec![String::new(); pool_id.len()];
    for (name, &id) in &pool_id {
        pool_names[id as usize] = name.to_string();
    }

    Dataset {
        hours,
        hour_keys,
        bots: bot_id.len(),
        failed_by_bots,
        failed_with_pools,
        pool_weight,
        pool_names,
    }
}

const NONE: u32 = u32::MAX;
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// Bundles flattened into contiguous arrays; pools are dense ids.
struct Flat {
    data: Vec<u32>,
    /// Pool range in `data` of each bundle.
    pools: Vec<(u32, u32)>,
    /// Range in `attempts` of each bundle's failed attempts.
    fails: Vec<(u32, u32)>,
    /// Pool range in `data` of each failed attempt.
    attempts: Vec<(u32, u32)>,
    hours: Vec<FHour>,
    n_pools: usize,
    weight: Vec<f64>,
    metric: Metric,
    null: NullKind,
    /// Entity (dense id) of each bundle.
    bundle_bot: Vec<u32>,
    /// Entities, and whether $CR_w$ is the mean of per-entity means.
    n_ent: usize,
    equal: bool,
}

struct FHour {
    n_blocks: usize,
    n_bots: usize,
    /// Bundle of (bot, block) at `bot * n_blocks + block`, or `NONE`.
    cell: Vec<u32>,
    /// Bundles of each block.
    blocks: Vec<Vec<u32>>,
}

impl Flat {
    fn new(ds: &Dataset, metric: Metric) -> Flat {
        let mut f = Flat {
            data: Vec::new(),
            pools: Vec::new(),
            fails: Vec::new(),
            attempts: Vec::new(),
            hours: Vec::new(),
            n_pools: 0,
            weight: ds.pool_weight.clone(),
            metric,
            null: NullKind::Primary,
            bundle_bot: Vec::new(),
            n_ent: ds.bots,
            equal: false,
        };
        for h in &ds.hours {
            let mut bots: Vec<u32> = h.blocks.iter().flatten().map(|b| b.bot).collect();
            bots.sort_unstable();
            bots.dedup();
            let nb = h.blocks.len();
            let mut cell = vec![NONE; bots.len() * nb];
            let mut blocks = Vec::with_capacity(nb);
            for (i, bs) in h.blocks.iter().enumerate() {
                let mut ids = Vec::with_capacity(bs.len());
                for b in bs {
                    let id = f.pools.len() as u32;
                    let s = f.data.len() as u32;
                    f.data.extend(&b.pools);
                    f.pools.push((s, f.data.len() as u32));
                    f.bundle_bot.push(b.bot);
                    let a0 = f.attempts.len() as u32;
                    for fl in &b.failed {
                        let s = f.data.len() as u32;
                        f.data.extend(fl);
                        f.attempts.push((s, f.data.len() as u32));
                    }
                    f.fails.push((a0, f.attempts.len() as u32));
                    let k = bots.binary_search(&b.bot).expect("bot listed");
                    cell[k * nb + i] = id;
                    ids.push(id);
                }
                blocks.push(ids);
            }
            f.hours.push(FHour {
                n_blocks: nb,
                n_bots: bots.len(),
                cell,
                blocks,
            });
        }
        f.n_pools = f.data.iter().max().map_or(0, |&m| m as usize + 1);
        f
    }
}

struct Scratch {
    counts: Vec<u32>,
    placed: Vec<Vec<u32>>,
    buf: Vec<u32>,
    /// Picked-block positions where the current bot is active (second null).
    pos: Vec<usize>,
    /// (score, attempts) per entity for the equal-weight statistic.
    ent: Vec<(f64, usize)>,
}

impl Scratch {
    fn new(f: &Flat) -> Scratch {
        Scratch {
            counts: vec![0; f.n_pools],
            placed: Vec::new(),
            buf: Vec::new(),
            pos: Vec::new(),
            ent: vec![(0.0, 0); if f.equal { f.n_ent } else { 0 }],
        }
    }
}

/// (contended failed attempts, failed attempts) for the bundles placed in one block.
/// A pool is contended when two bundles of different bots touch it; bundles are one
/// per bot and block and their pools are deduplicated, so that is a count of at least 2.
fn count_block(
    f: &Flat,
    bundles: &[u32],
    counts: &mut [u32],
    mut ent: Option<&mut [(f64, usize)]>,
) -> (f64, usize) {
    for &b in bundles {
        let (s, e) = f.pools[b as usize];
        for &p in &f.data[s as usize..e as usize] {
            counts[p as usize] += 1;
        }
    }
    let (mut hit, mut total) = (0.0, 0);
    for &b in bundles {
        let (a0, a1) = f.fails[b as usize];
        for &(s, e) in &f.attempts[a0 as usize..a1 as usize] {
            total += 1;
            let pools = &f.data[s as usize..e as usize];
            let mut score = 0.0;
            match f.metric {
                Metric::Binary => {
                    if pools.iter().any(|&p| counts[p as usize] >= 2) {
                        score = 1.0;
                    }
                }
                Metric::Weighted => {
                    let (mut shared, mut all) = (0.0, 0.0);
                    for &p in pools {
                        let w = f.weight[p as usize];
                        all += w;
                        if counts[p as usize] >= 2 {
                            shared += w;
                        }
                    }
                    if all > 0.0 {
                        score = shared / all;
                    }
                }
            }
            hit += score;
            if let Some(ent) = ent.as_deref_mut() {
                let e = &mut ent[f.bundle_bot[b as usize] as usize];
                e.0 += score;
                e.1 += 1;
            }
        }
    }
    for &b in bundles {
        let (s, e) = f.pools[b as usize];
        for &p in &f.data[s as usize..e as usize] {
            counts[p as usize] = 0;
        }
    }
    (hit, total)
}

/// Observed (contended, failed) per block of an hour.
fn observed_hour(
    f: &Flat,
    h: &FHour,
    counts: &mut [u32],
    mut ent: Option<&mut [(f64, usize)]>,
) -> Vec<(f64, usize)> {
    h.blocks
        .iter()
        .map(|b| count_block(f, b, counts, ent.as_deref_mut()))
        .collect()
}

/// The statistic: pooled ratio, or the mean of per-entity ratios (entities with no
/// failed attempt in the sample are left out).
fn stat(f: &Flat, hit: f64, total: usize, ent: &[(f64, usize)]) -> f64 {
    if !f.equal {
        return ratio(hit, total);
    }
    let (mut sum, mut n) = (0.0, 0usize);
    for &(h, t) in ent {
        if t > 0 {
            sum += h / t as f64;
            n += 1;
        }
    }
    if n == 0 {
        f64::NAN
    } else {
        sum / n as f64
    }
}

/// Statistic over a list of hour indices (with repeats) from the per-hour tallies.
fn stat_hours(f: &Flat, hours: &[usize], sums: &[(f64, usize)], ents: &[Vec<(f64, usize)>]) -> f64 {
    let (h, t) = hours
        .iter()
        .fold((0.0, 0), |a, &i| (a.0 + sums[i].0, a.1 + sums[i].1));
    if !f.equal {
        return ratio(h, t);
    }
    let mut ent = vec![(0.0, 0usize); f.n_ent];
    for &i in hours {
        for (a, b) in ent.iter_mut().zip(&ents[i]) {
            a.0 += b.0;
            a.1 += b.1;
        }
    }
    stat(f, h, t, &ent)
}

/// Number of bots with a bundle in at least one of the picked blocks.
fn present_bots(h: &FHour, picks: &[usize]) -> usize {
    (0..h.n_bots)
        .filter(|&k| picks.iter().any(|&i| h.cell[k * h.n_blocks + i] != NONE))
        .count()
}

/// RNG draws consumed by one null draw of an hour.
fn null_calls(f: &Flat, h: &FHour, picks: &[usize]) -> u64 {
    if picks.is_empty() {
        return 0;
    }
    match f.null {
        NullKind::Primary => (present_bots(h, picks) * (picks.len() - 1)) as u64,
        // A bot with m active picked blocks consumes m - 1 draws (none if m = 0).
        NullKind::Active => (0..h.n_bots)
            .map(|k| {
                let m = picks
                    .iter()
                    .filter(|&&i| h.cell[k * h.n_blocks + i] != NONE)
                    .count();
                m.saturating_sub(1) as u64
            })
            .sum(),
    }
}

/// One draw of the null for an hour: every bot's bundles are permuted across the
/// hour's (picked) blocks independently of the other bots.
fn null_hour(
    f: &Flat,
    h: &FHour,
    picks: &[usize],
    rng: &mut Rng,
    sc: &mut Scratch,
) -> (f64, usize) {
    let n = picks.len();
    if n == 0 {
        return (0.0, 0);
    }
    if sc.placed.len() < n {
        sc.placed.resize_with(n, Vec::new);
    }
    for p in &mut sc.placed[..n] {
        p.clear();
    }
    for k in 0..h.n_bots {
        sc.buf.clear();
        sc.buf
            .extend(picks.iter().map(|&i| h.cell[k * h.n_blocks + i]));
        if sc.buf.iter().all(|&b| b == NONE) {
            continue;
        }
        if f.null == NullKind::Active {
            // Second null (fundamento nulos secundarios §4.1): the bundles only trade
            // places among the blocks where this bot already has one.
            sc.pos.clear();
            sc.pos.extend((0..n).filter(|&j| sc.buf[j] != NONE));
            sc.buf.retain(|&b| b != NONE);
            rng.shuffle(&mut sc.buf);
            for (&j, &b) in sc.pos.iter().zip(&sc.buf) {
                sc.placed[j].push(b);
            }
            continue;
        }
        rng.shuffle(&mut sc.buf);
        for (j, &b) in sc.buf.iter().enumerate() {
            if b != NONE {
                sc.placed[j].push(b);
            }
        }
    }
    let mut acc = (0.0, 0);
    for p in &sc.placed[..n] {
        let ent = if f.equal { Some(&mut sc.ent[..]) } else { None };
        let (a, b) = count_block(f, p, &mut sc.counts, ent);
        acc = (acc.0 + a, acc.1 + b);
    }
    acc
}

fn ratio(h: f64, t: usize) -> f64 {
    if t == 0 {
        f64::NAN
    } else {
        h / t as f64
    }
}

fn null_draw(f: &Flat, picks: &[Vec<usize>], rng: &mut Rng, sc: &mut Scratch) -> f64 {
    sc.ent.iter_mut().for_each(|e| *e = (0.0, 0));
    let (mut h, mut t) = (0.0, 0);
    for (hr, pk) in f.hours.iter().zip(picks) {
        let (a, b) = null_hour(f, hr, pk, rng, sc);
        h += a;
        t += b;
    }
    stat(f, h, t, &sc.ent)
}

/// Null draw over a list of hour indices (with repeats), each hour with all its blocks.
fn null_draw_hours(
    f: &Flat,
    hours: &[usize],
    identity: &[Vec<usize>],
    rng: &mut Rng,
    sc: &mut Scratch,
) -> f64 {
    sc.ent.iter_mut().for_each(|e| *e = (0.0, 0));
    let (mut h, mut t) = (0.0, 0);
    for &i in hours {
        let (a, b) = null_hour(f, &f.hours[i], &identity[i], rng, sc);
        h += a;
        t += b;
    }
    stat(f, h, t, &sc.ent)
}

/// Evaluates `job(i, scratch)` for `i in 0..n` on all available cores. Each job is
/// independent and seeds its own RNG, so the result does not depend on scheduling.
fn parallel<R, J>(f: &Flat, n: usize, job: J) -> Vec<R>
where
    R: Send,
    J: Fn(usize, &mut Scratch) -> R + Sync,
{
    let threads = std::thread::available_parallelism()
        .map_or(1, |t| t.get())
        .min(n.max(1));
    let next = AtomicUsize::new(0);
    let mut out: Vec<(usize, R)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut sc = Scratch::new(f);
                    let mut local = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break local;
                        }
                        local.push((i, job(i, &mut sc)));
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

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx]
}

#[derive(Debug, Clone, Copy)]
pub struct ContendParams {
    pub permutations: usize,
    pub bootstrap: usize,
    pub bootstrap_permutations: usize,
    pub seed: u64,
    pub metric: Metric,
}

/// Which permutation null the excess is measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullKind {
    /// Primary (§4.5): each entity's bundles move to any block of the hour.
    Primary,
    /// Second null (nulos secundarios §4.1): bundles move only among the blocks of the
    /// hour where the entity is active, so co-presence is kept and pool choice is broken.
    Active,
}

/// Score of one failed attempt (fundamento §4.4 and §4.4 bis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    /// 1 if any pool is shared with another entity in the block.
    Binary,
    /// Share of the attempt's $1/n_p$ weight on pools shared with another entity.
    Weighted,
}

#[derive(Debug, Clone)]
pub struct ContendResult {
    pub blocks: usize,
    pub bots: usize,
    pub failed: usize,
    pub coverage: f64,
    pub cr_obs: f64,
    pub cr_ci: (f64, f64),
    pub null_mean: f64,
    pub null_band: (f64, f64),
    pub excess: f64,
    pub excess_ci: (f64, f64),
    pub p_value: f64,
    /// Share of failed attempts in entity-hours with at least 2 active blocks (the
    /// only ones the second null can move).
    pub active_share: f64,
    /// Entities with a bundle (pools) in the sample.
    pub entities: usize,
    /// E with each Unix day left out (`RunOptions::jackknife_days`), and its jackknife SE.
    pub jackknife: Vec<(i64, f64)>,
    pub jackknife_se: f64,
}

/// The SplitMix64 stream is a counter, so any draw can start at its exact position in
/// the sequential stream: results are bit-identical to a single-threaded run.
pub fn run(ds: &Dataset, params: ContendParams) -> ContendResult {
    run_with(ds, params, NullKind::Primary)
}

/// Like [`run`], against the chosen null. `NullKind::Primary` is bit-identical to `run`.
pub fn run_with(ds: &Dataset, params: ContendParams, null_kind: NullKind) -> ContendResult {
    run_ext(ds, params, null_kind, &RunOptions::default())
}

/// Optional variants, all off by default (the default result is unchanged bit for bit).
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    /// $CR_w$ as the mean of per-entity means (each entity weighs the same).
    pub equal_entity_weight: bool,
    /// Bootstrap of blocks of this many contiguous hours (0 or 1: single hours).
    pub block_hours: usize,
    /// Also compute E with each of the days left out (fundamento §4.5).
    pub jackknife_days: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            equal_entity_weight: false,
            block_hours: 1,
            jackknife_days: false,
        }
    }
}

/// Like [`run_with`], with the variants of [`RunOptions`].
pub fn run_ext(
    ds: &Dataset,
    params: ContendParams,
    null_kind: NullKind,
    opts: &RunOptions,
) -> ContendResult {
    let mut f = Flat::new(ds, params.metric);
    f.null = null_kind;
    f.equal = opts.equal_entity_weight;
    let mut sc0 = Scratch::new(&f);
    let mut per_hour_ent: Vec<Vec<(f64, usize)>> = if f.equal {
        vec![vec![(0.0, 0); f.n_ent]; f.hours.len()]
    } else {
        Vec::new()
    };
    let per_hour: Vec<Vec<(f64, usize)>> = f
        .hours
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let ent = if f.equal {
                Some(&mut per_hour_ent[i][..])
            } else {
                None
            };
            observed_hour(&f, h, &mut sc0.counts, ent)
        })
        .collect();
    let (hit, total) = per_hour
        .iter()
        .flatten()
        .fold((0.0, 0), |a, &(h, t)| (a.0 + h, a.1 + t));
    let all_hours: Vec<usize> = (0..f.hours.len()).collect();
    let per_hour_sum: Vec<(f64, usize)> = per_hour
        .iter()
        .map(|v| v.iter().fold((0.0, 0), |a, &(h, t)| (a.0 + h, a.1 + t)))
        .collect();
    let cr_obs = if f.equal {
        stat_hours(&f, &all_hours, &per_hour_sum, &per_hour_ent)
    } else {
        ratio(hit, total)
    };

    let mut rng = Rng::new(params.seed);
    let identity: Vec<Vec<usize>> = f.hours.iter().map(|h| (0..h.n_blocks).collect()).collect();
    let calls: u64 = f
        .hours
        .iter()
        .zip(&identity)
        .map(|(h, p)| null_calls(&f, h, p))
        .sum();
    let start = rng.state();
    let mut null: Vec<f64> = parallel(&f, params.permutations, |d, sc| {
        let mut r =
            Rng::from_state(start.wrapping_add((d as u64).wrapping_mul(calls).wrapping_mul(GAMMA)));
        null_draw(&f, &identity, &mut r, sc)
    });
    rng.advance(params.permutations as u64 * calls);
    let ge = null.iter().filter(|&&x| x >= cr_obs).count();
    let p_value = (1 + ge) as f64 / (params.permutations + 1) as f64;
    let null_mean = null.iter().sum::<f64>() / null.len().max(1) as f64;
    null.sort_by(f64::total_cmp);

    // Cluster bootstrap over whole hours (fundamento §4.6): each replicate resamples the
    // week's hours with replacement and keeps every hour intact, so the within-hour
    // permutation null stays well defined (no block appears twice inside an hour).
    let hour_calls: Vec<u64> = f
        .hours
        .iter()
        .zip(&identity)
        .map(|(h, p)| null_calls(&f, h, p))
        .collect();
    let n_hours = f.hours.len();
    let mut boot_cr = Vec::with_capacity(params.bootstrap);
    let mut reps: Vec<(Vec<usize>, u64)> = Vec::with_capacity(params.bootstrap);
    for _ in 0..params.bootstrap {
        if n_hours == 0 {
            break;
        }
        let hours: Vec<usize> = if opts.block_hours <= 1 {
            (0..n_hours).map(|_| rng.below(n_hours)).collect()
        } else {
            // Moving blocks of contiguous hours, cut to the sample's number of hours.
            let len = opts.block_hours.min(n_hours);
            let mut v = Vec::with_capacity(n_hours + len);
            while v.len() < n_hours {
                let start = rng.below(n_hours - len + 1);
                v.extend(start..start + len);
            }
            v.truncate(n_hours);
            v
        };
        boot_cr.push(stat_hours(&f, &hours, &per_hour_sum, &per_hour_ent));
        let start = rng.state();
        let c: u64 = hours.iter().map(|&i| hour_calls[i]).sum();
        rng.advance(params.bootstrap_permutations as u64 * c);
        reps.push((hours, start));
    }
    let mut boot_e = Vec::new();
    if params.bootstrap_permutations > 0 && !reps.is_empty() {
        boot_e = parallel(&f, reps.len(), |r, sc| {
            let (hours, start) = &reps[r];
            let mut rg = Rng::from_state(*start);
            let m = (0..params.bootstrap_permutations)
                .map(|_| null_draw_hours(&f, hours, &identity, &mut rg, sc))
                .sum::<f64>()
                / params.bootstrap_permutations as f64;
            boot_cr[r] - m
        });
    }
    boot_cr.sort_by(f64::total_cmp);
    boot_e.sort_by(f64::total_cmp);

    // Jackknife by day (§4.5): E with each Unix day left out, same null and statistic.
    let mut jackknife = Vec::new();
    let mut jackknife_se = f64::NAN;
    if opts.jackknife_days {
        let day = |i: usize| ds.hour_keys[i].div_euclid(24);
        let mut days: Vec<i64> = (0..n_hours).map(day).collect();
        days.sort_unstable();
        days.dedup();
        for (k, &d) in days.iter().enumerate() {
            let kept: Vec<usize> = (0..n_hours).filter(|&i| day(i) != d).collect();
            let obs = stat_hours(&f, &kept, &per_hour_sum, &per_hour_ent);
            let c: u64 = kept.iter().map(|&i| hour_calls[i]).sum();
            let start = Rng::new(params.seed ^ (k as u64 + 1).wrapping_mul(GAMMA)).state();
            let draws = parallel(&f, params.permutations, |j, sc| {
                let mut r = Rng::from_state(
                    start.wrapping_add((j as u64).wrapping_mul(c).wrapping_mul(GAMMA)),
                );
                null_draw_hours(&f, &kept, &identity, &mut r, sc)
            });
            let m = draws.iter().sum::<f64>() / draws.len().max(1) as f64;
            jackknife.push((d, obs - m));
        }
        let k = jackknife.len() as f64;
        if jackknife.len() >= 2 {
            let mean = jackknife.iter().map(|x| x.1).sum::<f64>() / k;
            let ss: f64 = jackknife.iter().map(|x| (x.1 - mean).powi(2)).sum();
            jackknife_se = ((k - 1.0) / k * ss).sqrt();
        }
    }

    ContendResult {
        blocks: ds.hours.iter().map(|h| h.blocks.len()).sum(),
        bots: ds.bots,
        failed: total,
        coverage: ratio(ds.failed_with_pools as f64, ds.failed_by_bots),
        cr_obs,
        cr_ci: (percentile(&boot_cr, 0.025), percentile(&boot_cr, 0.975)),
        null_mean,
        null_band: (percentile(&null, 0.025), percentile(&null, 0.975)),
        excess: cr_obs - null_mean,
        excess_ci: (percentile(&boot_e, 0.025), percentile(&boot_e, 0.975)),
        p_value,
        active_share: active_share(&f),
        entities: ds.bots,
        jackknife,
        jackknife_se,
    }
}

/// Share of failed attempts that sit in entity-hours with at least 2 active blocks.
fn active_share(f: &Flat) -> f64 {
    let (mut movable, mut all) = (0usize, 0usize);
    for h in &f.hours {
        for k in 0..h.n_bots {
            let cells: Vec<u32> = (0..h.n_blocks)
                .map(|i| h.cell[k * h.n_blocks + i])
                .filter(|&c| c != NONE)
                .collect();
            let n: usize = cells
                .iter()
                .map(|&c| (f.fails[c as usize].1 - f.fails[c as usize].0) as usize)
                .sum();
            all += n;
            if cells.len() >= 2 {
                movable += n;
            }
        }
    }
    ratio(movable as f64, all)
}

/// Holm step-down adjustment; returns adjusted p-values in input order.
pub fn holm(p: &[f64]) -> Vec<f64> {
    let m = p.len();
    let mut idx: Vec<usize> = (0..m).collect();
    idx.sort_by(|&a, &b| p[a].total_cmp(&p[b]));
    let mut out = vec![0.0; m];
    let mut running: f64 = 0.0;
    for (rank, &i) in idx.iter().enumerate() {
        running = running.max(((m - rank) as f64 * p[i]).min(1.0));
        out[i] = running;
    }
    out
}
