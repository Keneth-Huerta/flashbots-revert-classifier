//! AF-04 placebo exposures (ADR 006 §1.4): builder on hand-made blocks, calibration and
//! power of the placebo contrasts on synthetic strata, missing-data grid, joint contrast.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;
use mev_revert_classifier::contend::Rng;
use mev_revert_classifier::extract::{Kind, TxRow, SWAP_TOPICS};
use mev_revert_classifier::placebo::*;
use mev_revert_classifier::race::*;
use serde_json::json;

const POOLS: [&str; 4] = [
    "0x00000000000000000000000000000000000000f1",
    "0x00000000000000000000000000000000000000f2",
    "0x00000000000000000000000000000000000000f3",
    "0x00000000000000000000000000000000000000f4",
];
const BOTS: [&str; 3] = [
    "0x00000000000000000000000000000000000000a1",
    "0x00000000000000000000000000000000000000b2",
    "0x00000000000000000000000000000000000000c3",
];
const EOAS: [&str; 3] = [
    "0x0000000000000000000000000000000000000aa1",
    "0x0000000000000000000000000000000000000bb2",
    "0x0000000000000000000000000000000000000cc3",
];
const USER_C: &str = "0x00000000000000000000000000000000000000d4";
const USER_S: &str = "0x0000000000000000000000000000000000000dd4";
const V4: &str = "0x498581ff718922c3f8e6a244956af099b2652b2b";

fn tmp(tag: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("placebo_{tag}"));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

struct Mk {
    rows: Vec<TxRow>,
    seq: u64,
}

impl Mk {
    fn new() -> Self {
        Mk {
            rows: vec![],
            seq: 0,
        }
    }
    fn tx(
        &mut self,
        block: u64,
        idx: u64,
        from: &str,
        to: &str,
        kind: Kind,
        pools: &[&str],
    ) -> String {
        self.seq += 1;
        let h = format!("0x{:064x}", self.seq);
        self.rows.push(TxRow {
            block,
            tx_index: idx,
            tx_hash: h.clone(),
            from: from.into(),
            to: to.into(),
            kind,
            gas_used: 1,
            fee_wei: 1,
            swap_logs: 0,
            cyclic: false,
            pools: pools.join(";"),
        });
        h
    }
}

/// Receipts: a success with pools has one Swap log per pool; `from` is carried.
fn write_receipts(dir: &Path, rows: &[TxRow]) {
    let mut blocks: Vec<u64> = rows.iter().map(|r| r.block).collect();
    blocks.sort_unstable();
    blocks.dedup();
    for b in blocks {
        let rc: Vec<_> = rows
            .iter()
            .filter(|r| r.block == b)
            .map(|r| {
                let ok = r.kind == Kind::Success;
                let logs: Vec<_> = if ok {
                    r.pool_list()
                        .iter()
                        .map(|p| json!({"address": p, "topics": [SWAP_TOPICS[1]], "data": "0x"}))
                        .collect()
                } else {
                    vec![]
                };
                json!({"transactionHash": r.tx_hash, "status": if ok {"0x1"} else {"0x0"},
                       "from": r.from, "transactionIndex": format!("0x{:x}", r.tx_index), "logs": logs})
            })
            .collect();
        let f = fs::File::create(dir.join(format!("{b}.receipts.json.gz"))).unwrap();
        let mut gz = GzEncoder::new(f, Compression::fast());
        serde_json::to_writer(&mut gz, &json!({"result": rc})).unwrap();
        gz.finish().unwrap();
    }
}

/// Block 100 is the one under test; blocks 1..=3 give every bot enough failed attempts
/// with pools to enter the universe.
fn scenario() -> (Mk, Vec<u64>) {
    let mut m = Mk::new();
    let [pa, pb, pc, pd] = POOLS;
    // idx: 0 B revert [pb] | 1 A success [pa] | 2 B revert [pa] | 3 A revert [pa]
    // 4 C success [pb] | 5 A revert [pb] | 6 user swap [pc] | 7 B revert [pc]
    // 8 C revert [pa] | 9 A revert [pd, pc] | 10 B revert [] (empty P)
    m.tx(100, 0, EOAS[1], BOTS[1], Kind::Revert, &[pb]);
    m.tx(100, 1, EOAS[0], BOTS[0], Kind::Success, &[pa]);
    m.tx(100, 2, EOAS[1], BOTS[1], Kind::Revert, &[pa]);
    m.tx(100, 3, EOAS[0], BOTS[0], Kind::Revert, &[pa]);
    m.tx(100, 4, EOAS[2], BOTS[2], Kind::Success, &[pb]);
    m.tx(100, 5, EOAS[0], BOTS[0], Kind::Revert, &[pb]);
    m.tx(100, 6, USER_S, USER_C, Kind::Success, &[pc]);
    m.tx(100, 7, EOAS[1], BOTS[1], Kind::Revert, &[pc]);
    m.tx(100, 8, EOAS[2], BOTS[2], Kind::Revert, &[pa]);
    m.tx(100, 9, EOAS[0], BOTS[0], Kind::Revert, &[pd, pc]);
    m.tx(100, 10, EOAS[1], BOTS[1], Kind::Revert, &[]);
    for b in 1..=3u64 {
        for (k, bot) in BOTS.iter().enumerate() {
            for j in 0..3 {
                m.tx(b, 20 + 3 * k as u64 + j, EOAS[k], bot, Kind::Revert, &[pd]);
            }
        }
    }
    (m, vec![100, 1, 2, 3])
}

fn build(m: &Mk, blocks: &[u64], tag: &str, eo: &ExtOpts) -> (StratumData, Ext) {
    let dir = tmp(tag);
    write_receipts(&dir, &m.rows);
    let (d, e, miss) =
        build_stratum_ext("S", &m.rows, blocks, &dir, 10, &StratumOpts::default(), eo).unwrap();
    assert!(miss.is_empty());
    (d, e.expect("ext"))
}

fn eo() -> ExtOpts<'static> {
    ExtOpts {
        want_ext: true,
        ..ExtOpts::default()
    }
}

/// Exposures of the tx at `idx` of block 100.
fn at(m: &Mk, d: &StratumData, e: &Ext, idx: u64) -> (Tx, TxExt) {
    let mut ordered: Vec<&TxRow> = m
        .rows
        .iter()
        .filter(|r| r.block == 100 && !r.pools.is_empty() && BOTS.contains(&r.to.as_str()))
        .collect();
    ordered.sort_by_key(|r| r.tx_index);
    // txs are ordered by block then index; block 100 is the last (BTreeMap by block).
    let first = d.txs.iter().position(|t| t.block == 100).unwrap();
    let k = ordered.iter().position(|r| r.tx_index == idx).unwrap();
    let k = first + k;
    (d.txs[k], e.per_tx[k])
}

fn user_bot_universe_is_right(d: &StratumData) {
    // 10 txs of block 100 have nonempty P and a bot contract (user swap is not a bot).
    assert_eq!(d.txs.iter().filter(|t| t.block == 100).count(), 9);
}

#[test]
fn builder_exposures_on_a_hand_made_block() {
    let (m, blocks) = scenario();
    let (d, e) = build(&m, &blocks, "hand", &eo());
    user_bot_universe_is_right(&d);
    assert_eq!(e.per_tx.len(), d.txs.len());
    // idx 0: B reverts on pb; C swaps later on pb (other entity) -> later only.
    let (t, x) = at(&m, &d, &e, 0);
    assert!(!t.x && x.x_later && !x.x_same);
    // idx 2: B reverts on pa; A swapped at 1 -> X; no later swap by others on pa.
    let (t, x) = at(&m, &d, &e, 2);
    assert!(t.x && !x.x_later);
    // idx 3: A reverts on pa; own swap at 1 is not "other": X false, same true.
    let (t, x) = at(&m, &d, &e, 3);
    assert!(!t.x && x.x_same && !x.x_later);
    // idx 5: A reverts on pb; C swapped at 4 -> X true; A never swaps on pb: same false.
    let (t, x) = at(&m, &d, &e, 5);
    assert!(t.x && !x.x_same);
    // idx 7: B reverts on pc; only a user swap before: X false, user true.
    let (t, x) = at(&m, &d, &e, 7);
    assert!(!t.x && x.x_user);
    // idx 4: the swapper does not see its own swap as prior or later.
    let (t, x) = at(&m, &d, &e, 4);
    assert!(!t.x && !x.x_later && !x.x_same);
    // idx 6 is a user swap, not in the universe; idx 10 has empty P and is kept apart.
    assert_eq!(e.empty.len(), 1);
    assert_eq!(e.empty[0].tx.block, 100);
    assert!(e.empty[0].tx.y);
}

#[test]
fn the_default_stratum_is_the_one_without_the_extension() {
    let (m, blocks) = scenario();
    let dir = tmp("same");
    write_receipts(&dir, &m.rows);
    let (a, _) =
        build_stratum_with("S", &m.rows, &blocks, &dir, 10, &StratumOpts::default()).unwrap();
    let (b, e, _) = build_stratum_ext(
        "S",
        &m.rows,
        &blocks,
        &dir,
        10,
        &StratumOpts::default(),
        &eo(),
    )
    .unwrap();
    assert_eq!(a.txs, b.txs);
    assert!(e.is_some());
    let (_, none, _) = build_stratum_ext(
        "S",
        &m.rows,
        &blocks,
        &dir,
        10,
        &StratumOpts::default(),
        &ExtOpts::default(),
    )
    .unwrap();
    assert!(none.is_none());
}

#[test]
fn donors_are_disjoint_other_entity_and_deterministic() {
    let (m, blocks) = scenario();
    let (d1, e1) = build(&m, &blocks, "donor1", &eo());
    let (_, e2) = build(&m, &blocks, "donor2", &eo());
    assert_eq!(e1.per_tx, e2.per_tx);
    // idx 9 (A, pools pd+pc) overlaps pc with idx 7 (B) and has pa/pb txs of others: a donor
    // exists among non-overlapping ones; idx 2 (B on pa) cannot use A's pa txs.
    let (_, x9) = at(&m, &d1, &e1, 9);
    assert!(x9.x_donor.is_some());
    // Donor seed changes the choice somewhere (5 seeds give a range).
    let mut seen: HashSet<Vec<Option<bool>>> = HashSet::new();
    for seed in 1..=8 {
        let (_, e) = build(
            &m,
            &blocks,
            "donor3",
            &ExtOpts {
                donor_seed: seed,
                ..eo()
            },
        );
        seen.insert(e.per_tx.iter().map(|x| x.x_donor).collect());
    }
    assert!(!seen.is_empty());
}

#[test]
fn a_block_with_one_bot_entity_has_no_donor() {
    let mut m = Mk::new();
    let [pa, pb, ..] = POOLS;
    for b in 1..=3u64 {
        for j in 0..4 {
            m.tx(b, j, EOAS[0], BOTS[0], Kind::Revert, &[pa]);
        }
    }
    m.tx(1, 9, EOAS[0], BOTS[0], Kind::Success, &[pb]);
    let (d, e) = build(&m, &[1, 2, 3], "nodonor", &eo());
    assert!(!d.txs.is_empty());
    assert!(e.per_tx.iter().all(|x| x.x_donor.is_none()));
    let (vs, st) = apply_variant(
        std::slice::from_ref(&d),
        std::slice::from_ref(&e),
        &Variant {
            x_def: XDef::DonorPool,
            ..Variant::default()
        },
    );
    assert!(vs[0].txs.is_empty());
    assert_eq!(st.kept, 0);
}

#[test]
fn contract_rule_uses_swaps_of_the_contract_and_keeps_empty_p() {
    let (m, blocks) = scenario();
    let (d, e) = build(&m, &blocks, "contract", &eo());
    // Bot A swaps on pa (1 incidence of 1): P_A = [pa]; bot C on pb; bot B never swaps.
    let (_, x3) = at(&m, &d, &e, 3);
    assert_eq!(x3.x_c, Some(false)); // A's own swap on pa is not another entity's
    let (_, x2) = at(&m, &d, &e, 2);
    assert_eq!(x2.x_c, None); // B has no successful swaps: left out
    assert!(e.empty[0].x_c.is_none());
    let (vs, st) = apply_variant(
        std::slice::from_ref(&d),
        std::slice::from_ref(&e),
        &Variant {
            p_rule: PRule::Contract,
            ..Variant::default()
        },
    );
    assert_eq!(
        vs[0].txs.len(),
        d.txs
            .iter()
            .zip(&e.per_tx)
            .filter(|(_, x)| x.x_c.is_some())
            .count()
    );
    assert_eq!(st.kept, vs[0].txs.iter().filter(|t| !t.probe).count());
}

#[test]
fn v4_swap_logs_become_pseudo_pools() {
    let mut m = Mk::new();
    let id = format!("0x{:064x}", 7);
    let pseudo = format!("v4:{id}");
    for b in 1..=3u64 {
        for j in 0..4 {
            m.tx(b, j, EOAS[1], BOTS[1], Kind::Revert, &[pseudo.as_str()]);
        }
    }
    m.tx(1, 5, EOAS[0], BOTS[0], Kind::Success, &[pseudo.as_str()]);
    m.tx(1, 6, EOAS[1], BOTS[1], Kind::Revert, &[pseudo.as_str()]);
    for b in 1..=3u64 {
        for j in 0..4 {
            m.tx(
                b,
                30 + j,
                EOAS[0],
                BOTS[0],
                Kind::Revert,
                &[pseudo.as_str()],
            );
        }
    }
    let dir = tmp("v4");
    // Receipts: the PoolManager emits the v4 Swap log with the pool id as topics[1].
    for b in 1..=3u64 {
        let rc: Vec<_> = m
            .rows
            .iter()
            .filter(|r| r.block == b)
            .map(|r| {
                let ok = r.kind == Kind::Success;
                let logs = if ok {
                    vec![json!({"address": V4, "topics": [SWAP_TOPICS[2], id], "data": "0x"})]
                } else {
                    vec![]
                };
                json!({"transactionHash": r.tx_hash, "status": if ok {"0x1"} else {"0x0"},
                       "from": r.from, "transactionIndex": format!("0x{:x}", r.tx_index), "logs": logs})
            })
            .collect();
        let f = fs::File::create(dir.join(format!("{b}.receipts.json.gz"))).unwrap();
        let mut gz = GzEncoder::new(f, Compression::fast());
        serde_json::to_writer(&mut gz, &json!({"result": rc})).unwrap();
        gz.finish().unwrap();
    }
    let run = |pm: Option<&str>| {
        let o = ExtOpts {
            want_ext: true,
            v4_pool_manager: pm,
            ..ExtOpts::default()
        };
        let (d, _, _) = build_stratum_ext(
            "S",
            &m.rows,
            &[1, 2, 3],
            &dir,
            10,
            &StratumOpts::default(),
            &o,
        )
        .unwrap();
        d.txs.iter().filter(|t| t.x).count()
    };
    // The sealed definition (receipt addresses) never matches a pseudo-pool: documented gap.
    assert_eq!(run(None), 0);
    assert!(run(Some(V4)) > 0);
}

// ----------------------------------------------------------------- synthetic power

fn params(b: usize) -> RaceParams {
    RaceParams {
        bootstrap: b,
        permutations: 1,
        seed: 3,
        week_unit: true,
        ..RaceParams::default()
    }
}

#[derive(Clone, Copy)]
enum World {
    /// Effect only of the earlier swap X (state race).
    State,
    /// Pool shock: raises Y and the swaps of others before and after in the pool.
    PoolShock,
    /// Block shock: raises Y and every exposure, including the donor's.
    BlockShock,
    /// Effect only of an earlier swap of the same entity.
    OwnSwap,
    /// No effect; exposures random.
    Null,
}

fn world(seed: u64, w: World, weeks: usize) -> (Vec<StratumData>, Vec<Ext>) {
    let mut rng = Rng::new(seed);
    let (mut ss, mut es) = (Vec::new(), Vec::new());
    let u = |r: &mut Rng| r.next_u64() as f64 / u64::MAX as f64;
    for wk in 0..weeks {
        let mut txs = Vec::new();
        let mut per = Vec::new();
        for ent in 0..4u32 {
            for bin in 0..10u32 {
                for _ in 0..40 {
                    let base = 0.3 + 0.05 * ent as f64 + 0.02 * bin as f64;
                    let shock = u(&mut rng) < 0.4;
                    let (x, later, same, donor, y_add): (bool, bool, bool, bool, f64);
                    match w {
                        World::State => {
                            x = u(&mut rng) < 0.4;
                            later = u(&mut rng) < 0.4;
                            donor = u(&mut rng) < 0.4;
                            same = u(&mut rng) < 0.3;
                            y_add = 0.05 * x as u8 as f64;
                        }
                        World::PoolShock => {
                            x = shock && u(&mut rng) < 0.8;
                            later = shock && u(&mut rng) < 0.8;
                            donor = u(&mut rng) < 0.4;
                            same = u(&mut rng) < 0.3;
                            y_add = 0.05 * shock as u8 as f64;
                        }
                        World::BlockShock => {
                            x = shock && u(&mut rng) < 0.8;
                            later = shock && u(&mut rng) < 0.8;
                            donor = shock && u(&mut rng) < 0.8;
                            same = u(&mut rng) < 0.3;
                            y_add = 0.05 * shock as u8 as f64;
                        }
                        World::OwnSwap => {
                            x = u(&mut rng) < 0.4;
                            later = u(&mut rng) < 0.4;
                            donor = u(&mut rng) < 0.4;
                            same = u(&mut rng) < 0.4;
                            y_add = 0.05 * same as u8 as f64;
                        }
                        World::Null => {
                            x = u(&mut rng) < 0.4;
                            later = u(&mut rng) < 0.4;
                            donor = u(&mut rng) < 0.4;
                            same = u(&mut rng) < 0.4;
                            y_add = 0.0;
                        }
                    }
                    let y = u(&mut rng) < base + y_add;
                    txs.push(Tx {
                        ent,
                        bin,
                        block: wk as u64 * 1000 + rng.below(100) as u64,
                        hour: rng.below(10) as u32,
                        x,
                        y,
                        probe: false,
                    });
                    per.push(TxExt {
                        x_later: later,
                        x_same: same,
                        x_user: false,
                        x_donor: Some(donor),
                        psize: 1,
                        x_c: None,
                    });
                }
            }
        }
        ss.push(StratumData {
            name: format!("W{wk:02}"),
            txs,
            n_hours: 10,
        });
        es.push(Ext {
            per_tx: per,
            empty: vec![],
            ent_keys: vec![String::new(); 4],
        });
    }
    (ss, es)
}

/// R3: 0 = null (upper < tau), 1 = active, 2 = inconclusive.
fn r3(strata: &[StratumData], p: &RaceParams) -> u8 {
    let (d, (lo, hi)) =
        mev_revert_classifier::placebo::week_ci(&week_stats(strata, false, p), 0, p);
    if d >= 0.02 && lo > 0.0 {
        1
    } else if hi < 0.02 {
        0
    } else {
        2
    }
}

fn read(strata: &[StratumData], exts: &[Ext], def: XDef, p: &RaceParams) -> u8 {
    let v = Variant {
        x_def: def,
        ..Variant::default()
    };
    let (vs, _) = apply_variant(strata, exts, &v);
    r3(&vs, p)
}

#[test]
fn calibration_placebo_estimates_are_centered_at_zero_under_the_null() {
    let p = params(200);
    let (mut sums, trials) = ([0.0f64; 4], 40);
    let mut false_active = 0;
    for t in 0..trials {
        let (s, e) = world(900 + t, World::Null, 8);
        for (k, def) in [
            XDef::LaterOther,
            XDef::DonorPool,
            XDef::PriorSame,
            XDef::PriorOther,
        ]
        .iter()
        .enumerate()
        {
            let (vs, _) = apply_variant(
                &s,
                &e,
                &Variant {
                    x_def: *def,
                    ..Variant::default()
                },
            );
            let (d, (lo, _)) = week_ci(&week_stats(&vs, false, &p), 0, &p);
            sums[k] += d;
            false_active += (d >= 0.02 && lo > 0.0) as usize;
        }
    }
    for (k, s) in sums.iter().enumerate() {
        assert!(
            (s / trials as f64).abs() < 0.004,
            "placebo {k}: mean {}",
            s / trials as f64
        );
    }
    // 160 decisions at nominal 2.5% one-sided per interval end: well under 10%.
    assert!(
        false_active as f64 / (4 * trials) as f64 <= 0.09,
        "false active {false_active}"
    );
}

#[test]
fn power_state_race_is_seen_as_state_race() {
    let p = params(300);
    let (mut prim, mut temporal_ok, mut pool_ok) = (0, 0, 0);
    for t in 0..20 {
        let (s, e) = world(100 + t, World::State, 8);
        prim += (read(&s, &e, XDef::PriorOther, &p) == 1) as usize;
        temporal_ok += (read(&s, &e, XDef::LaterOther, &p) != 1) as usize;
        pool_ok += (read(&s, &e, XDef::DonorPool, &p) != 1) as usize;
    }
    assert!(prim >= 18, "primary detected {prim} of 20");
    assert!(temporal_ok >= 18, "temporal not active {temporal_ok}");
    assert!(pool_ok >= 18, "pool not active {pool_ok}");
}

#[test]
fn power_pool_shock_makes_the_temporal_placebo_active() {
    let p = params(300);
    let n = (0..20)
        .filter(|&t| {
            let (s, e) = world(200 + t, World::PoolShock, 8);
            read(&s, &e, XDef::LaterOther, &p) == 1
        })
        .count();
    assert!(n >= 18, "temporal active in {n} of 20");
}

#[test]
fn power_block_shock_makes_the_pool_placebo_active() {
    let p = params(300);
    let n = (0..20)
        .filter(|&t| {
            let (s, e) = world(300 + t, World::BlockShock, 8);
            read(&s, &e, XDef::DonorPool, &p) == 1
        })
        .count();
    assert!(n >= 18, "pool active in {n} of 20");
}

#[test]
fn power_own_swap_effect_is_seen_by_the_entity_placebo() {
    let p = params(300);
    let n = (0..20)
        .filter(|&t| {
            let (s, e) = world(400 + t, World::OwnSwap, 8);
            read(&s, &e, XDef::PriorSame, &p) == 1
        })
        .count();
    assert!(n >= 18, "entity placebo active in {n} of 20");
}

#[test]
fn conditioning_on_a_second_indicator_splits_cells() {
    let (s, e) = world(5, World::State, 3);
    let plain = apply_variant(&s, &e, &Variant::default()).0;
    let cond = apply_variant(
        &s,
        &e,
        &Variant {
            also: XAlso::LaterOther,
            ..Variant::default()
        },
    )
    .0;
    let p = params(0);
    let cells = |v: &[StratumData]| {
        analyze(
            v,
            0,
            false,
            &RaceParams {
                week_unit: false,
                ..p
            },
        )
        .last()
        .unwrap()
        .cond
        .cells
    };
    assert!(cells(&cond) > cells(&plain));
    assert_eq!(plain[0].txs.len(), cond[0].txs.len());
}

#[test]
fn week_ci_matches_analyze_for_the_default_variant() {
    let (s, e) = world(6, World::State, 6);
    let p = RaceParams {
        bootstrap: 100,
        permutations: 10,
        seed: 9,
        week_unit: true,
        ..RaceParams::default()
    };
    let (vs, _) = apply_variant(&s, &e, &Variant::default());
    let line = analyze(&vs, 0, false, &p);
    let (d, ci) = week_ci(&week_stats(&vs, false, &p), 0, &p);
    let pool = line.last().unwrap();
    assert_eq!(d.to_bits(), pool.cond.d.to_bits());
    assert_eq!(ci.0.to_bits(), pool.cond.ci.0.to_bits());
    assert_eq!(ci.1.to_bits(), pool.cond.ci.1.to_bits());
}

#[test]
fn joint_contrast_of_an_estimand_with_itself_is_exactly_zero_and_deterministic() {
    let (s, e) = world(7, World::State, 6);
    let p = params(200);
    let (vs, _) = apply_variant(&s, &e, &Variant::default());
    let st = week_stats(&vs, false, &p);
    let (d, ci, a, b) = contrast(&st, &st, 0, &p);
    assert_eq!((d, ci.0, ci.1), (0.0, 0.0, 0.0));
    assert_eq!(a, b);
    let (s2, _) = apply_variant(
        &s,
        &e,
        &Variant {
            x_def: XDef::PriorSame,
            ..Variant::default()
        },
    );
    let st2 = week_stats(&s2, false, &p);
    let c1 = contrast(&st, &st2, 0, &p);
    let c2 = contrast(&st, &st2, 0, &p);
    assert_eq!(c1.0.to_bits(), c2.0.to_bits());
    assert_eq!(c1.1 .0.to_bits(), c2.1 .0.to_bits());
    assert!(c1.1 .0 < c1.0 && c1.0 < c1.1 .1);
}

#[test]
fn a_stratum_without_both_groups_does_not_break_the_bootstrap() {
    let (mut s, mut e) = world(8, World::State, 4);
    for t in s[1].txs.iter_mut() {
        t.x = false;
    }
    s.push(StratumData {
        name: "EMPTY".into(),
        txs: vec![],
        n_hours: 0,
    });
    e.push(Ext::default());
    let p = params(100);
    let (vs, _) = apply_variant(&s, &e, &Variant::default());
    let (d, ci) = week_ci(&week_stats(&vs, false, &p), 0, &p);
    assert!(d.is_finite() && ci.0.is_finite() && ci.1.is_finite());
}

// ---------------------------------------------------------------------- missing grid

#[test]
fn the_true_estimate_with_hidden_p_lies_in_the_grid_range() {
    for seed in 0..10 {
        let (s, _) = world(500 + seed, World::State, 4);
        let mut rng = Rng::new(seed);
        // Hide 15% of the transactions of each stratum as "empty P" (exposure unknown).
        let (mut shown, mut exts) = (Vec::new(), Vec::new());
        for st in &s {
            let mut keep = Vec::new();
            let mut empty = Vec::new();
            for t in &st.txs {
                if rng.below(100) < 15 {
                    empty.push(EmptyTx { tx: *t, x_c: None });
                } else {
                    keep.push(*t);
                }
            }
            exts.push(Ext {
                per_tx: vec![TxExt::default(); keep.len()],
                empty,
                ent_keys: vec![],
            });
            shown.push(StratumData {
                name: st.name.clone(),
                txs: keep,
                n_hours: st.n_hours,
            });
        }
        let p = params(20);
        let g = missing_grid(&shown, &exts, 0.02, &p);
        let truth = week_stats(&s, false, &p);
        let truth = truth.iter().map(|x| x.0).sum::<f64>() / truth.iter().map(|x| x.1).sum::<f64>();
        assert!(
            g.min.d - 1e-9 <= truth && truth <= g.max.d + 1e-9,
            "{} not in [{}, {}]",
            truth,
            g.min.d,
            g.max.d
        );
        assert_eq!(g.points.len(), 121);
    }
}

#[test]
fn grid_corner_without_missing_equals_the_plain_estimate() {
    let (s, _) = world(11, World::State, 3);
    let exts: Vec<Ext> = s
        .iter()
        .map(|x| Ext {
            per_tx: vec![TxExt::default(); x.txs.len()],
            ..Ext::default()
        })
        .collect();
    let p = params(10);
    let g = missing_grid(&s, &exts, 0.02, &p);
    let plain = week_stats(&s, false, &p);
    let plain = plain.iter().map(|x| x.0).sum::<f64>() / plain.iter().map(|x| x.1).sum::<f64>();
    assert!(g.points.iter().all(|x| (x.d - plain).abs() < 1e-12));
    assert!(g.tipping.is_none());
}

// ------------------------------------------------------------------------- CLI

fn cli(args: &[&str]) -> String {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_mev-revert-classifier"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn the_af04_path_with_the_default_variant_gives_the_sealed_numbers() {
    let (m, blocks) = scenario();
    let dir = tmp("cli");
    write_receipts(&dir, &m.rows);
    let rows = dir.join("rows.csv");
    let mut w = csv::Writer::from_path(&rows).unwrap();
    for r in &m.rows {
        w.serialize(r).unwrap();
    }
    w.flush().unwrap();
    let mut s = String::from("stratum,block,hour_start_ts,exploratory\n");
    for b in &blocks {
        let ts = mev_revert_classifier::sample::block_timestamp_on(
            mev_revert_classifier::sample::BASE_TIMING,
            *b,
        );
        s += &format!("S1,{b},{},false\n", ts.div_euclid(3600) * 3600);
    }
    let sample = dir.join("sample.csv");
    fs::write(&sample, s).unwrap();
    let base = |extra: &[&str]| {
        let mut a = vec![
            "race",
            "--rows",
            rows.to_str().unwrap(),
            "--blocks",
            sample.to_str().unwrap(),
            "--split",
            "all",
            "--raw",
            dir.to_str().unwrap(),
            "--bootstrap",
            "50",
            "--permutations",
            "50",
            "--boot-unit",
            "week",
            "--sin-probes-only",
        ];
        a.extend_from_slice(extra);
        cli(&a)
    };
    let sealed = base(&[]);
    let af04 = base(&["--x-def", "prior-other"]);
    let lines = |t: &str| {
        t.lines()
            .filter(|l| l.starts_with("S1") || l.starts_with("POOL"))
            .map(String::from)
            .collect::<Vec<_>>()
    };
    assert!(!lines(&sealed).is_empty());
    assert_eq!(lines(&sealed), lines(&af04));
    assert!(af04.contains("af04 variant"));
    assert!(!sealed.contains("af04"));
    // Contrast file round trip: a contrast of a variant with itself is zero.
    let w = dir.join("weeks.tsv");
    base(&["--emit-weeks", w.to_str().unwrap()]);
    let c = cli(&[
        "race-contrast",
        "--a",
        w.to_str().unwrap(),
        "--b",
        w.to_str().unwrap(),
        "--bootstrap",
        "50",
    ]);
    assert!(c.contains("a-b="), "{c}"); // one tiny stratum: the estimate may be undefined
}

#[test]
fn race_warns_on_stderr_about_v4_rows_without_the_swap_option() {
    let (m, blocks) = scenario();
    let dir = tmp("v4warn");
    write_receipts(&dir, &m.rows);
    let rows = dir.join("rows.csv");
    let mut w = csv::Writer::from_path(&rows).unwrap();
    for (i, r) in m.rows.iter().enumerate() {
        let mut r = r.clone();
        if i == 0 {
            r.pools = format!("v4:0x{:064x}", 7);
        }
        w.serialize(&r).unwrap();
    }
    w.flush().unwrap();
    let mut s = String::from("stratum,block,hour_start_ts,exploratory\n");
    for b in &blocks {
        let ts = mev_revert_classifier::sample::block_timestamp_on(
            mev_revert_classifier::sample::BASE_TIMING,
            *b,
        );
        s += &format!("S1,{b},{},false\n", ts.div_euclid(3600) * 3600);
    }
    let sample = dir.join("sample.csv");
    fs::write(&sample, s).unwrap();
    let run = |extra: &[&str]| {
        let mut a = vec![
            "race",
            "--rows",
            rows.to_str().unwrap(),
            "--blocks",
            sample.to_str().unwrap(),
            "--split",
            "all",
            "--raw",
            dir.to_str().unwrap(),
            "--bootstrap",
            "20",
            "--permutations",
            "20",
            "--boot-unit",
            "week",
            "--sin-probes-only",
        ];
        a.extend_from_slice(extra);
        std::process::Command::new(env!("CARGO_BIN_EXE_mev-revert-classifier"))
            .args(&a)
            .output()
            .unwrap()
    };
    let plain = run(&[]);
    assert!(plain.status.success());
    let err = String::from_utf8_lossy(&plain.stderr);
    assert!(err.contains("v4-swap-pools"), "{err}");
    assert!(!String::from_utf8_lossy(&plain.stdout).contains("warning"));
    // Same rows with the option: no warning.
    let with = run(&["--v4-swap-pools"]);
    assert!(with.status.success());
    assert!(!String::from_utf8_lossy(&with.stderr).contains("v4-swap-pools is not set"));
    // Rows without v4 pools: silent.
    let (m2, _) = scenario();
    assert!(m2.rows.iter().all(|r| !r.pools.contains("v4:")));
}

#[test]
fn python_parity_on_the_hand_made_scenario() {
    let (m, blocks) = scenario();
    let dir = tmp("py");
    write_receipts(&dir, &m.rows);
    let rows = dir.join("rows.csv");
    let mut w = csv::Writer::from_path(&rows).unwrap();
    for r in &m.rows {
        w.serialize(r).unwrap();
    }
    w.flush().unwrap();
    let mut t = String::from("stratum,block,hour_start_ts,exploratory\n");
    for b in &blocks {
        let ts = mev_revert_classifier::sample::block_timestamp_on(
            mev_revert_classifier::sample::BASE_TIMING,
            *b,
        );
        t += &format!("S1,{b},{},false\n", ts.div_euclid(3600) * 3600);
    }
    let sample = dir.join("sample.csv");
    fs::write(&sample, t).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/analisis/robustez_af04.py");
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let cases: [(&[&str], &[&str]); 6] = [
        (&[], &[]),
        (&["--x-def", "later-other"], &["--x-def", "later_other"]),
        (&["--x-def", "donor-pool"], &["--x-def", "donor_pool"]),
        (&["--x-def", "prior-same"], &["--x-def", "prior_same"]),
        (
            &["--x-def", "user-prior", "--x-also", "prior-other"],
            &["--x-def", "user_prior", "--x-also", "prior_other"],
        ),
        (
            &["--p-rule", "contract", "--p-size-strata"],
            &["--p-rule", "contract", "--p-size-strata"],
        ),
    ];
    for (i, (rust, py)) in cases.iter().enumerate() {
        let mut a = vec![
            "race",
            "--rows",
            rows.to_str().unwrap(),
            "--blocks",
            sample.to_str().unwrap(),
            "--split",
            "confirmatory",
            "--raw",
            dir.to_str().unwrap(),
            "--bootstrap",
            "5",
            "--permutations",
            "5",
            "--sin-probes-only",
        ];
        a.extend_from_slice(rust);
        let out = cli(&a);
        let f = dir.join(format!("rust{i}.txt"));
        fs::write(&f, out).unwrap();
        let o = std::process::Command::new("python3")
            .arg("-I")
            .arg(&script)
            .args([
                "--rows",
                rows.to_str().unwrap(),
                "--blocks",
                sample.to_str().unwrap(),
            ])
            .args(["--split", "confirmatory", "--raw", dir.to_str().unwrap()])
            .args(*py)
            .args(["--comparar", f.to_str().unwrap()])
            .output()
            .unwrap();
        let so = String::from_utf8_lossy(&o.stdout);
        assert!(
            o.status.success(),
            "case {i}: {so}{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(so.contains("PARIDAD OK"), "case {i}: {so}");
    }
}
