use std::collections::{HashMap, HashSet};

use mev_revert_classifier::contend::*;
use mev_revert_classifier::extract::{Kind, TxRow};
use mev_revert_classifier::sample::{first_block_at_or_after, parse_date};

fn row(block: u64, bot: &str, kind: Kind, pools: &[&str]) -> TxRow {
    TxRow {
        block,
        tx_index: 0,
        tx_hash: format!("{block}{bot}{pools:?}"),
        from: format!("eoa-{bot}"),
        to: bot.to_string(),
        kind,
        gas_used: 1,
        fee_wei: 1,
        swap_logs: 0,
        cyclic: false,
        pools: pools.join(";"),
    }
}

fn hours_of_blocks(hours: usize, per_hour: usize) -> Vec<Vec<u64>> {
    let base = first_block_at_or_after(parse_date("2025-08-10").unwrap());
    (0..hours)
        .map(|h| {
            (0..per_hour)
                .map(|i| base + (h as u64) * 1800 + (i as u64) * 300)
                .collect()
        })
        .collect()
}

const P: ContendParams = ContendParams {
    permutations: 199,
    bootstrap: 50,
    bootstrap_permutations: 5,
    seed: 7,
    metric: Metric::Binary,
};

/// Bots act independently: each bot is active in a random subset of blocks and reads
/// random pools from a small set.
fn independent(seed: u64, bots: usize, hours: usize) -> (Vec<TxRow>, Vec<u64>) {
    let mut rng = Rng::new(seed);
    let pools = ["p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7"];
    let mut rows = Vec::new();
    let mut blocks = Vec::new();
    for hour in hours_of_blocks(hours, 6) {
        for &b in &hour {
            blocks.push(b);
            for v in 0..bots {
                if rng.below(2) == 0 {
                    let p = pools[rng.below(pools.len())];
                    rows.push(row(b, &format!("bot{v}"), Kind::Probe, &[p]));
                }
            }
        }
    }
    (rows, blocks)
}

fn analyse(rows: &[TxRow], blocks: &[u64], params: ContendParams) -> ContendResult {
    let rule = BotRule {
        min_failed: 1,
        ..BotRule::default()
    };
    let bots = bot_universe(rows, &rule);
    run(&build(rows, blocks, &bots, &HashSet::new()), params)
}

#[test]
fn null_is_calibrated_without_synchrony() {
    let mut low = 0;
    let trials = 60;
    for s in 0..trials {
        let (rows, blocks) = independent(1000 + s, 4, 12);
        let r = analyse(
            &rows,
            &blocks,
            ContendParams {
                seed: s,
                bootstrap: 0,
                ..P
            },
        );
        if r.p_value < 0.05 {
            low += 1;
        }
    }
    // Expected 3 of 60; allow sampling noise.
    assert!(low <= 9, "{low} of {trials} below 0.05: null too liberal");
}

#[test]
fn seeded_synchrony_is_detected() {
    let (mut rows, mut blocks) = independent(5, 4, 12);
    // Two racers chase a block-specific opportunity in half of the blocks of each hour.
    let mut rng = Rng::new(99);
    for hour in hours_of_blocks(12, 6) {
        for &b in &hour {
            if rng.below(2) == 0 {
                let opp = format!("opp-{b}");
                rows.push(row(b, "racer-a", Kind::Revert, &[&opp]));
                rows.push(row(b, "racer-b", Kind::Revert, &[&opp]));
            }
        }
        blocks.extend(&hour);
    }
    blocks.sort_unstable();
    blocks.dedup();
    let r = analyse(&rows, &blocks, P);
    assert!(r.p_value < 0.01, "p {}", r.p_value);
    assert!(r.excess > 0.05, "E {}", r.excess);
    assert!(r.cr_ci.0 <= r.cr_obs && r.cr_obs <= r.cr_ci.1);
}

#[test]
fn always_present_bots_with_fixed_pools_carry_no_signal() {
    // Documented limit (theory note §5): permuting a bot that is in every block with the
    // same pools changes nothing, so its overlap cannot be told apart from chance.
    let blocks: Vec<u64> = hours_of_blocks(6, 6).concat();
    let rows: Vec<TxRow> = blocks
        .iter()
        .flat_map(|&b| {
            [
                row(b, "x", Kind::Probe, &["p"]),
                row(b, "y", Kind::Probe, &["p"]),
            ]
        })
        .collect();
    let r = analyse(&rows, &blocks, P);
    assert_eq!(r.cr_obs, 1.0);
    assert_eq!(r.null_mean, 1.0);
    assert_eq!(r.excess, 0.0);
}

#[test]
fn one_bot_alone_is_never_in_contention() {
    let blocks: Vec<u64> = hours_of_blocks(2, 6).concat();
    let rows: Vec<TxRow> = blocks
        .iter()
        .flat_map(|&b| (0..10).map(move |_| row(b, "solo", Kind::Probe, &["p"])))
        .collect();
    let r = analyse(&rows, &blocks, P);
    assert_eq!(r.cr_obs, 0.0);
    assert_eq!(r.null_mean, 0.0);
    assert_eq!(r.p_value, 1.0);
}

#[test]
fn contention_counts_successes_of_other_bots_but_not_own() {
    let blocks: Vec<u64> = hours_of_blocks(1, 6).concat();
    let b = blocks[0];
    let rows = vec![
        row(b, "x", Kind::Probe, &["p"]),
        row(b, "x", Kind::Success, &["p"]), // own success: no contention
        row(b, "y", Kind::Success, &["q"]), // other bot, other pool: no contention
    ];
    let bots: HashSet<String> = ["x", "y"].iter().map(|s| s.to_string()).collect();
    let r = run(&build(&rows, &blocks, &bots, &HashSet::new()), P);
    assert_eq!(r.cr_obs, 0.0);
    let rows2 = vec![
        row(b, "x", Kind::Probe, &["p"]),
        row(b, "y", Kind::Success, &["p"]),
    ];
    let r2 = run(&build(&rows2, &blocks, &bots, &HashSet::new()), P);
    assert_eq!(r2.cr_obs, 1.0);
}

#[test]
fn bot_universe_excludes_routers_and_thin_contracts() {
    let mut rows = Vec::new();
    // Router: 10 failed swaps among 100 mostly successful user swaps.
    for i in 0..100 {
        let kind = if i < 10 { Kind::Revert } else { Kind::Success };
        let mut r = row(1, "router", kind, &["p"]);
        r.from = format!("user{i}");
        rows.push(r);
    }
    // Bot rotating many EOAs: 6 failed, 1 success -> still a bot.
    for i in 0..6 {
        let mut r = row(1, "bot", Kind::Probe, &["p"]);
        r.from = format!("eoa{i}");
        rows.push(r);
    }
    rows.push(row(1, "bot", Kind::Success, &["p"]));
    for _ in 0..4 {
        rows.push(row(1, "thin", Kind::Probe, &["p"]));
    }
    for _ in 0..9 {
        rows.push(row(1, "nopool", Kind::Probe, &[]));
    }
    let u = bot_universe(&rows, &BotRule::default());
    assert_eq!(u, HashSet::from(["bot".to_string()]));
    let cyc = bot_universe(
        &rows,
        &BotRule {
            require_cyclic: true,
            ..BotRule::default()
        },
    );
    assert!(cyc.is_empty());
}

#[test]
fn failed_share_boundary_is_inclusive() {
    let mut rows: Vec<TxRow> = (0..5).map(|_| row(1, "b", Kind::Probe, &["p"])).collect();
    rows.extend((0..5).map(|_| row(1, "b", Kind::Success, &["p"])));
    assert_eq!(
        bot_universe(&rows, &BotRule::default()).len(),
        1,
        "5 of 10 = 0.5"
    );
    rows.push(row(1, "b", Kind::Success, &["p"]));
    assert!(
        bot_universe(&rows, &BotRule::default()).is_empty(),
        "5 of 11 < 0.5"
    );
}

#[test]
fn contracts_sharing_a_sender_are_one_entity_and_do_not_contend() {
    let blocks: Vec<u64> = hours_of_blocks(1, 6).concat();
    let b = blocks[0];
    let mut a1 = row(b, "c1", Kind::Probe, &["p"]);
    a1.from = "shared".into();
    let mut a2 = row(b, "c2", Kind::Probe, &["p"]);
    a2.from = "shared".into();
    let mut a3 = row(b, "c3", Kind::Probe, &["q"]);
    a3.from = "other".into();
    let rows = vec![a1, a2, a3];
    let bots: HashSet<String> = ["c1", "c2", "c3"].iter().map(|s| s.to_string()).collect();
    let ent = entities(&rows, &bots);
    assert_eq!(ent["c1"], "c1");
    assert_eq!(ent["c2"], "c1");
    assert_eq!(ent["c3"], "c3");
    // As separate contracts c1 and c2 contend on p; as one entity they do not.
    let sep = run(&build(&rows, &blocks, &bots, &HashSet::new()), P);
    assert!((sep.cr_obs - 2.0 / 3.0).abs() < 1e-12);
    let grouped = run(&build_entities(&rows, &blocks, &ent, &HashSet::new()), P);
    assert_eq!(grouped.cr_obs, 0.0);
    assert_eq!(grouped.bots, 2);
}

#[test]
fn entity_grouping_is_transitive() {
    let mk = |to: &str, from: &str| {
        let mut r = row(1, to, Kind::Probe, &["p"]);
        r.from = from.into();
        r
    };
    let rows = vec![
        mk("c3", "e1"),
        mk("c2", "e1"),
        mk("c2", "e2"),
        mk("c1", "e2"),
        mk("c4", "e9"),
    ];
    let bots: HashSet<String> = ["c1", "c2", "c3", "c4"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let ent = entities(&rows, &bots);
    assert_eq!(
        (ent["c1"].as_str(), ent["c2"].as_str(), ent["c3"].as_str()),
        ("c1", "c1", "c1")
    );
    assert_eq!(ent["c4"], "c4");
}

#[test]
fn excluded_pools_remove_overlap() {
    let blocks: Vec<u64> = hours_of_blocks(1, 6).concat();
    let rows = vec![
        row(blocks[0], "x", Kind::Probe, &["hub"]),
        row(blocks[0], "y", Kind::Probe, &["hub"]),
    ];
    let bots: HashSet<String> = ["x", "y"].iter().map(|s| s.to_string()).collect();
    let ex = HashSet::from(["hub".to_string()]);
    let r = run(&build(&rows, &blocks, &bots, &ex), P);
    assert!(r.cr_obs.is_nan(), "no failed attempt keeps pools");
    assert_eq!(top_pools(&rows, &bots, 1), ex);
}

#[test]
fn holm_matches_hand_computation() {
    let adj = holm(&[0.01, 0.04, 0.03, 0.20]);
    let want = [0.04, 0.09, 0.09, 0.20];
    for (a, w) in adj.iter().zip(want) {
        assert!((a - w).abs() < 1e-12, "{adj:?}");
    }
}

#[test]
fn same_seed_same_result() {
    let (rows, blocks) = independent(3, 5, 6);
    let a = analyse(&rows, &blocks, P);
    let b = analyse(&rows, &blocks, P);
    assert_eq!(a.p_value, b.p_value);
    assert_eq!(a.cr_ci, b.cr_ci);
}

/// Dataset with varied pool sets, empty blocks and mixed outcomes, for exact pinning.
fn varied(seed: u64) -> (Vec<TxRow>, Vec<u64>) {
    let mut rng = Rng::new(seed);
    let pools = ["p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9"];
    let mut rows = Vec::new();
    let mut blocks = Vec::new();
    for hour in hours_of_blocks(5, 6) {
        for &b in &hour {
            blocks.push(b);
            for v in 0..7 {
                for _ in 0..rng.below(3) {
                    let k = match rng.below(3) {
                        0 => Kind::Probe,
                        1 => Kind::Revert,
                        _ => Kind::Success,
                    };
                    let mut ps: Vec<&str> = (0..1 + rng.below(3))
                        .map(|_| pools[rng.below(pools.len())])
                        .collect();
                    ps.sort_unstable();
                    ps.dedup();
                    rows.push(row(b, &format!("bot{v}"), k, &ps));
                }
            }
        }
    }
    (rows, blocks)
}

#[test]
fn run_output_is_pinned_for_fixed_seed() {
    let (rows, blocks) = varied(42);
    let r = analyse(
        &rows,
        &blocks,
        ContendParams {
            permutations: 300,
            bootstrap: 120,
            bootstrap_permutations: 7,
            seed: 20261017,
            metric: Metric::Binary,
        },
    );
    let got = [
        r.coverage,
        r.cr_obs,
        r.cr_ci.0,
        r.cr_ci.1,
        r.null_mean,
        r.null_band.0,
        r.null_band.1,
        r.excess,
        r.excess_ci.0,
        r.excess_ci.1,
        r.p_value,
    ]
    .map(f64::to_bits);
    assert_eq!((r.blocks, r.bots, r.failed), (30, 7, 136));
    // Point estimates, null and p are pinned to the original sequential code. The four CI
    // values were re-pinned on 2026-10-17 when the bootstrap moved to whole-hour clusters.
    let want: [u64; 11] = [
        0x3ff0000000000000,
        0x3fec000000000000,
        0x3fea15833a15833a,
        0x3fed77b654b82c34,
        0x3feb36699cd0033a,
        0x3fe92d2d2d2d2d2d,
        0x3fecf0f0f0f0f0f1,
        0x3f9932cc65ff98c0,
        0xbf9595488b176180,
        0x3fb0dbf2833dfd68,
        0x3fd028d2ec70440a,
    ];
    assert_eq!(got, want);
}

#[test]
fn hour_bootstrap_interval_contains_point_estimates() {
    // Regression for the block-level bootstrap, whose E interval could exclude E itself.
    for seed in 0..5 {
        let (rows, blocks) = independent(200 + seed, 5, 24);
        let r = analyse(
            &rows,
            &blocks,
            ContendParams {
                permutations: 99,
                bootstrap: 300,
                bootstrap_permutations: 20,
                seed,
                metric: Metric::Binary,
            },
        );
        assert!(
            r.cr_ci.0 <= r.cr_obs && r.cr_obs <= r.cr_ci.1,
            "seed {seed}: CR {r:?}"
        );
        assert!(
            r.excess_ci.0 <= r.excess && r.excess <= r.excess_ci.1,
            "seed {seed}: E {r:?}"
        );
    }
}

const W: ContendParams = ContendParams {
    permutations: 199,
    bootstrap: 50,
    bootstrap_permutations: 5,
    seed: 7,
    metric: Metric::Weighted,
};

#[test]
fn weighted_score_downweights_hub_pools() {
    let blocks: Vec<u64> = hours_of_blocks(1, 6).concat();
    // "hub" is touched by every bundle (n = 4 -> w = 1/4); "rare" by two bundles (w = 1/2).
    let rows = vec![
        row(blocks[0], "x", Kind::Probe, &["hub", "rare"]),
        row(blocks[0], "y", Kind::Success, &["hub"]),
        row(blocks[1], "z", Kind::Success, &["hub", "rare"]),
        row(blocks[2], "y", Kind::Success, &["hub"]),
    ];
    let bots: HashSet<String> = ["x", "y", "z"].iter().map(|s| s.to_string()).collect();
    let ds = build(&rows, &blocks, &bots, &HashSet::new());
    let binary = run(&ds, P);
    let weighted = run(&ds, W);
    assert_eq!(binary.cr_obs, 1.0, "shares the hub with y");
    // Only the hub is shared in block 0: (1/4) / (1/4 + 1/2) = 1/3.
    assert!(
        (weighted.cr_obs - 1.0 / 3.0).abs() < 1e-12,
        "{}",
        weighted.cr_obs
    );
}

#[test]
fn weighted_null_is_calibrated_without_synchrony() {
    let mut low = 0;
    let trials = 60;
    for s in 0..trials {
        let (rows, blocks) = independent(5000 + s, 4, 12);
        let r = analyse(
            &rows,
            &blocks,
            ContendParams {
                seed: s,
                bootstrap: 0,
                ..W
            },
        );
        if r.p_value < 0.05 {
            low += 1;
        }
    }
    assert!(
        low <= 9,
        "{low} of {trials} below 0.05: weighted null too liberal"
    );
}

#[test]
fn weighted_detects_seeded_synchrony_behind_hub_pools() {
    let (mut rows, mut blocks) = independent(5, 4, 12);
    let mut rng = Rng::new(99);
    for hour in hours_of_blocks(12, 6) {
        for &b in &hour {
            if rng.below(2) == 0 {
                let opp = format!("opp-{b}");
                // Racers also read the hub every time, as real scanners do.
                rows.push(row(b, "racer-a", Kind::Revert, &["p0", &opp]));
                rows.push(row(b, "racer-b", Kind::Revert, &["p0", &opp]));
            }
        }
        blocks.extend(&hour);
    }
    blocks.sort_unstable();
    blocks.dedup();
    let r = analyse(&rows, &blocks, W);
    assert!(r.p_value < 0.01, "p {}", r.p_value);
    assert!(r.excess > 0.0);
}

/// Block-level common shock: in a random half of the blocks of each hour a group of
/// entities is active together, but each reads random pools independently of the block
/// (co-presence without shared pool choice). With `shared`, they all read the block's own
/// opportunity pool instead.
fn shocked(seed: u64, shared: bool) -> (Vec<TxRow>, Vec<u64>) {
    let mut rng = Rng::new(seed);
    let pools = ["p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7"];
    let mut rows = Vec::new();
    let mut blocks = Vec::new();
    for hour in hours_of_blocks(12, 6) {
        for &b in &hour {
            blocks.push(b);
            if rng.below(2) == 0 {
                for v in 0..4 {
                    if rng.below(5) != 0 {
                        let p = if shared {
                            format!("opp-{b}")
                        } else {
                            pools[rng.below(pools.len())].to_string()
                        };
                        rows.push(row(b, &format!("bot{v}"), Kind::Revert, &[&p]));
                    }
                }
            }
        }
    }
    (rows, blocks)
}

fn analyse_null(
    rows: &[TxRow],
    blocks: &[u64],
    params: ContendParams,
    null: NullKind,
) -> ContendResult {
    let rule = BotRule {
        min_failed: 1,
        ..BotRule::default()
    };
    let bots = bot_universe(rows, &rule);
    run_with(&build(rows, blocks, &bots, &HashSet::new()), params, null)
}

#[test]
fn active_null_is_calibrated_under_copresence_without_shared_pools() {
    let (mut low_active, mut low_primary) = (0, 0);
    let trials = 200;
    for s in 0..trials {
        let (rows, blocks) = shocked(7000 + s, false);
        let p = ContendParams {
            seed: s,
            bootstrap: 0,
            ..W
        };
        if analyse_null(&rows, &blocks, p, NullKind::Active).p_value < 0.05 {
            low_active += 1;
        }
        if analyse_null(&rows, &blocks, p, NullKind::Primary).p_value < 0.05 {
            low_primary += 1;
        }
    }
    let rate = low_active as f64 / trials as f64;
    assert!(
        (0.03..=0.07).contains(&rate),
        "active null rejects {low_active} of {trials}"
    );
    // The primary null cannot tell co-presence from shared choice, so it over-rejects.
    assert!(
        low_primary as f64 / trials as f64 > 0.5,
        "primary null rejects only {low_primary} of {trials}"
    );
}

#[test]
fn active_null_detects_shared_block_specific_pools() {
    let (rows, blocks) = shocked(11, true);
    let r = analyse_null(&rows, &blocks, W, NullKind::Active);
    assert!(r.p_value < 0.01, "p {}", r.p_value);
    assert!(r.excess > 0.1, "E {}", r.excess);
    assert!(r.cr_ci.0 <= r.cr_obs && r.cr_obs <= r.cr_ci.1);
}

#[test]
fn primary_through_run_with_is_run_and_active_is_seed_deterministic() {
    let (rows, blocks) = varied(42);
    let a = analyse_null(&rows, &blocks, P, NullKind::Primary);
    let b = analyse(&rows, &blocks, P);
    assert_eq!(a.null_mean.to_bits(), b.null_mean.to_bits());
    assert_eq!(a.excess_ci.0.to_bits(), b.excess_ci.0.to_bits());
    let c = analyse_null(&rows, &blocks, P, NullKind::Active);
    let d = analyse_null(&rows, &blocks, P, NullKind::Active);
    assert_eq!(c.p_value, d.p_value);
    assert_eq!(c.null_mean.to_bits(), d.null_mean.to_bits());
    assert_eq!(c.excess_ci.1.to_bits(), d.excess_ci.1.to_bits());
}

#[test]
fn single_active_block_entities_are_fixed_by_the_active_null() {
    // Each entity has one bundle per hour: nothing to permute, so the null equals the
    // observation, E = 0, and no failed attempt is movable.
    let blocks: Vec<u64> = hours_of_blocks(4, 6).concat();
    let mut rows = Vec::new();
    for hour in hours_of_blocks(4, 6) {
        rows.push(row(hour[0], "x", Kind::Revert, &["p"]));
        rows.push(row(hour[0], "y", Kind::Revert, &["p"]));
        rows.push(row(hour[3], "z", Kind::Revert, &["q"]));
    }
    let r = analyse_null(&rows, &blocks, P, NullKind::Active);
    assert!(r.excess.abs() < 1e-12, "E {}", r.excess);
    assert_eq!(r.active_share, 0.0);
    let (rows2, blocks2) = shocked(3, true);
    let share = analyse_null(&rows2, &blocks2, P, NullKind::Active).active_share;
    assert!(share > 0.5 && share <= 1.0, "share {share}");
}

// ---- v0.5.0: sensitivity options (nulos secundarios §4.4, §4.5) ----

/// `analyse` with explicit entities (rule) and run options.
fn analyse_ext(
    rows: &[TxRow],
    blocks: &[u64],
    params: ContendParams,
    rule: EntityRule,
    opts: &RunOptions,
) -> ContendResult {
    let bot_rule = BotRule {
        min_failed: 1,
        ..BotRule::default()
    };
    let bots = bot_universe(rows, &bot_rule);
    let entity_of = entities_with(rows, &bots, rule);
    run_ext(
        &build_entities(rows, blocks, &entity_of, &HashSet::new()),
        params,
        NullKind::Primary,
        opts,
    )
}

fn row_from(from: &str, block: u64, bot: &str) -> TxRow {
    TxRow {
        from: from.to_string(),
        ..row(block, bot, Kind::Probe, &["p"])
    }
}

#[test]
fn default_options_reproduce_run_with_bit_for_bit() {
    let (rows, blocks) = varied(42);
    let a = analyse(&rows, &blocks, P);
    let b = analyse_ext(
        &rows,
        &blocks,
        P,
        EntityRule::default(),
        &RunOptions::default(),
    );
    assert_eq!(a.cr_obs.to_bits(), b.cr_obs.to_bits());
    assert_eq!(a.null_mean.to_bits(), b.null_mean.to_bits());
    assert_eq!(a.excess_ci.0.to_bits(), b.excess_ci.0.to_bits());
    assert_eq!(a.excess_ci.1.to_bits(), b.excess_ci.1.to_bits());
    assert_eq!(a.p_value.to_bits(), b.p_value.to_bits());
    assert!(b.jackknife.is_empty());
}

#[test]
fn entities_with_default_rule_matches_entities() {
    let (rows, _) = varied(42);
    let bots = bot_universe(
        &rows,
        &BotRule {
            min_failed: 1,
            ..BotRule::default()
        },
    );
    assert_eq!(
        entities(&rows, &bots),
        entities_with(&rows, &bots, EntityRule::default())
    );
}

#[test]
fn shared2_needs_two_senders_and_degree_cap_ignores_relayers() {
    let b = 1;
    let bots: HashSet<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    // a-b share one sender (s1); a-c share two (s1, s2); s1 calls three contracts.
    let rows = vec![
        row_from("s1", b, "a"),
        row_from("s1", b, "b"),
        row_from("s1", b, "c"),
        row_from("s2", b, "a"),
        row_from("s2", b, "c"),
    ];
    let count = |m: &HashMap<String, String>| m.values().collect::<HashSet<_>>().len();
    let any = entities_with(&rows, &bots, EntityRule::default());
    assert_eq!(count(&any), 1);
    let two = entities_with(
        &rows,
        &bots,
        EntityRule {
            min_shared: 2,
            max_sender_degree: None,
        },
    );
    assert_eq!(count(&two), 2);
    assert_eq!(two["a"], two["c"]);
    assert_ne!(two["a"], two["b"]);
    // Dropping s1 (degree 3 >= 3) leaves only s2, which links a and c.
    let capped = entities_with(
        &rows,
        &bots,
        EntityRule {
            min_shared: 1,
            max_sender_degree: Some(3),
        },
    );
    assert_eq!(count(&capped), 2);
    assert_eq!(capped["a"], capped["c"]);
    // A cap of 2 removes both senders: every contract is its own entity.
    let none = entities_with(
        &rows,
        &bots,
        EntityRule {
            min_shared: 1,
            max_sender_degree: Some(2),
        },
    );
    assert_eq!(count(&none), 3);
}

#[test]
fn equal_weight_is_mean_of_entity_means() {
    // One block: x has 3 attempts (all contended with y), y has 1 (contended), z has 1
    // (alone). Pooled = 4/5 over attempts... per entity: x = 1, y = 1, z = 0 -> 2/3.
    let blocks: Vec<u64> = hours_of_blocks(1, 6).concat();
    let rows = vec![
        row(blocks[0], "x", Kind::Probe, &["p"]),
        row(blocks[0], "x", Kind::Probe, &["p"]),
        row(blocks[0], "x", Kind::Probe, &["p"]),
        row(blocks[0], "y", Kind::Probe, &["p"]),
        row(blocks[0], "z", Kind::Probe, &["q"]),
    ];
    let bots: HashSet<String> = ["x", "y", "z"].iter().map(|s| s.to_string()).collect();
    let ds = build(&rows, &blocks, &bots, &HashSet::new());
    let pooled = run_ext(&ds, P, NullKind::Primary, &RunOptions::default());
    let equal = run_ext(
        &ds,
        P,
        NullKind::Primary,
        &RunOptions {
            equal_entity_weight: true,
            ..RunOptions::default()
        },
    );
    assert!((pooled.cr_obs - 0.8).abs() < 1e-12, "{}", pooled.cr_obs);
    assert!((equal.cr_obs - 2.0 / 3.0).abs() < 1e-12, "{}", equal.cr_obs);
    assert_eq!(equal.entities, 3);
    // The null uses the same formula: its mean is a mean of per-entity means too.
    assert!((0.0..=1.0).contains(&equal.null_mean));
}

#[test]
fn equal_weight_still_detects_seeded_synchrony() {
    let (mut rows, mut blocks) = independent(5, 4, 12);
    let mut rng = Rng::new(99);
    for hour in hours_of_blocks(12, 6) {
        for &b in &hour {
            if rng.below(2) == 0 {
                let opp = format!("opp-{b}");
                rows.push(row(b, "racer-a", Kind::Revert, &[&opp]));
                rows.push(row(b, "racer-b", Kind::Revert, &[&opp]));
            }
        }
        blocks.extend(&hour);
    }
    blocks.sort_unstable();
    blocks.dedup();
    let r = analyse_ext(
        &rows,
        &blocks,
        W,
        EntityRule::default(),
        &RunOptions {
            equal_entity_weight: true,
            ..RunOptions::default()
        },
    );
    assert!(r.p_value < 0.01, "p {}", r.p_value);
    assert!(r.excess > 0.0 && r.excess_ci.0.is_finite());
}

#[test]
fn block_bootstrap_and_jackknife_are_deterministic_and_cover_the_days() {
    // 48 hourly buckets = 2 days.
    let base = first_block_at_or_after(parse_date("2025-08-10").unwrap());
    let mut rng = Rng::new(3);
    let (mut rows, mut blocks) = (Vec::new(), Vec::new());
    for h in 0..48u64 {
        for i in 0..4u64 {
            let b = base + h * 1800 + i * 300;
            blocks.push(b);
            for v in 0..3 {
                if rng.below(2) == 0 {
                    let p = format!("p{}", rng.below(4));
                    rows.push(row(b, &format!("bot{v}"), Kind::Probe, &[&p]));
                }
            }
        }
    }
    let opts = RunOptions {
        block_hours: 6,
        jackknife_days: true,
        ..RunOptions::default()
    };
    let a = analyse_ext(&rows, &blocks, P, EntityRule::default(), &opts);
    let b = analyse_ext(&rows, &blocks, P, EntityRule::default(), &opts);
    assert_eq!(a.excess_ci.0.to_bits(), b.excess_ci.0.to_bits());
    assert_eq!(a.jackknife, b.jackknife);
    assert!(
        a.jackknife.len() >= 2 && a.jackknife.len() <= 3,
        "{:?}",
        a.jackknife
    );
    assert!(a.jackknife_se.is_finite() && a.jackknife_se >= 0.0);
    let plain = analyse(&rows, &blocks, P);
    assert_eq!(plain.cr_obs.to_bits(), a.cr_obs.to_bits());
    assert!(a.excess_ci.0 <= a.excess_ci.1);
}

#[test]
fn weighted_null_calibration_over_300_trials() {
    // Fast: 99 permutations and no bootstrap.
    // The earlier test only bounded the count at 9/60; this one demands the rate sit
    // in [0.02, 0.08] around the nominal 0.05 (300 trials: SE ~ 0.013).
    let trials = 300;
    let mut low = 0;
    for s in 0..trials {
        let (rows, blocks) = independent(20_000 + s, 4, 12);
        let p = ContendParams {
            seed: s,
            permutations: 99,
            bootstrap: 0,
            ..W
        };
        if analyse(&rows, &blocks, p).p_value < 0.05 {
            low += 1;
        }
    }
    let rate = low as f64 / trials as f64;
    assert!(
        (0.02..=0.08).contains(&rate),
        "{low} of {trials} below 0.05 (rate {rate})"
    );
}

/// Pool popularity that drifts smoothly through the hour and is common to all entities,
/// with every entity drawing independently given the block (no block-level synchrony).
fn drifting(seed: u64) -> (Vec<TxRow>, Vec<u64>) {
    let mut rng = Rng::new(seed);
    let n_pools = 8usize;
    let mut rows = Vec::new();
    let mut blocks = Vec::new();
    for hour in hours_of_blocks(12, 6) {
        let phase = rng.below(n_pools) as f64;
        for (i, &b) in hour.iter().enumerate() {
            blocks.push(b);
            let centre = phase + i as f64 * 1.5;
            let w: Vec<f64> = (0..n_pools)
                .map(|k| {
                    let d = (k as f64 - centre).rem_euclid(n_pools as f64);
                    let d = d.min(n_pools as f64 - d);
                    (-d * d / (2.0 * 0.8 * 0.8)).exp()
                })
                .collect();
            let total: f64 = w.iter().sum();
            for v in 0..4 {
                if rng.below(2) == 0 {
                    let mut u = rng.below(1_000_000) as f64 / 1_000_000.0 * total;
                    let mut k = 0;
                    while k + 1 < n_pools && u > w[k] {
                        u -= w[k];
                        k += 1;
                    }
                    rows.push(row(
                        b,
                        &format!("bot{v}"),
                        Kind::Revert,
                        &[&format!("p{k}")],
                    ));
                }
            }
        }
    }
    (rows, blocks)
}

#[test]
#[ignore = "documents the V1 vulnerability (pool-popularity drift); does not assert"]
fn primary_null_under_pool_popularity_drift_is_reported() {
    let trials = 300;
    let mut low = 0;
    for s in 0..trials {
        let (rows, blocks) = drifting(30_000 + s);
        let p = ContendParams {
            seed: s,
            permutations: 99,
            bootstrap: 0,
            ..W
        };
        if analyse(&rows, &blocks, p).p_value < 0.05 {
            low += 1;
        }
    }
    // Run with `cargo test --release -- --ignored --nocapture`. Measured 2026-10-17 (v0.5.0):
    // 300 of 300 (rate 1.000) with a strong drift (Gaussian bump of sd 0.8 pools moving 1.5
    // pools per block over 8 pools): the primary null is badly inflated under common drift.
    eprintln!(
        "drift: primary null rejects {low} of {trials} at p < 0.05 (rate {:.3})",
        low as f64 / trials as f64
    );
}
