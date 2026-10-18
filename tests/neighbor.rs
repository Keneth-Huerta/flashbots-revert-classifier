use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use mev_revert_classifier::contend::{self, BotRule, ContendParams, Metric, NullKind, RunOptions};
use mev_revert_classifier::extract::{Kind, TxRow};
use mev_revert_classifier::neighbor::*;
use mev_revert_classifier::sample::{
    block_timestamp, first_block_at_or_after, parse_date, NeighborBlock, Role, SampledBlock,
};

fn row(block: u64, idx: u64, ent: usize, kind: Kind, pools: &[&str]) -> TxRow {
    TxRow {
        block,
        tx_index: idx,
        tx_hash: format!("0x{block:x}{idx:x}{ent}"),
        from: format!("eoa{ent}"),
        to: format!("bot{ent}"),
        kind,
        gas_used: 1,
        fee_wei: 1,
        swap_logs: 0,
        cyclic: false,
        pools: pools.join(";"),
    }
}

const POOLS: [&str; 8] = ["p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7"];

/// Random bot activity in one block: every entity sends 4 reverts and maybe a success.
fn activity(rng: &mut contend::Rng, block: u64) -> Vec<TxRow> {
    let mut v = Vec::new();
    for ent in 0..3 {
        for i in 0..4 {
            let a = POOLS[rng.below(8)];
            let b = POOLS[rng.below(8)];
            let mut ps = vec![a, b];
            ps.sort_unstable();
            ps.dedup();
            v.push(row(block, (ent * 10 + i) as u64, ent, Kind::Revert, &ps));
        }
        if rng.below(2) == 0 {
            v.push(row(
                block,
                90 + ent as u64,
                ent,
                Kind::Success,
                &[POOLS[rng.below(8)]],
            ));
        }
    }
    v
}

#[derive(Clone, Copy, PartialEq)]
enum Neighbors {
    Copies,
    Empty,
    Random,
}

struct Fixture {
    rows: Vec<TxRow>,
    design: Vec<NeighborBlock>,
}

fn nb(stratum: &str, block: u64, role: Role, anchor: u64) -> NeighborBlock {
    NeighborBlock {
        block: SampledBlock {
            stratum: stratum.to_string(),
            block,
            hour_start_ts: block_timestamp(block) / 3600 * 3600,
            exploratory: false,
        },
        role,
        anchor,
    }
}

/// Two strata; every hour has 2 anchors, all with a b+1 neighbor and the first also with
/// b+15 and b+150.
fn fixture(kind: Neighbors) -> Fixture {
    let base = first_block_at_or_after(parse_date("2025-08-10").unwrap());
    let mut rng = contend::Rng::new(42);
    let (mut rows, mut design) = (Vec::new(), Vec::new());
    for (si, stratum) in ["S1", "S2"].iter().enumerate() {
        for hour in 0..6u64 {
            for slot in 0..2u64 {
                let a = base + 100_000 * si as u64 + hour * 1800 + 300 * slot + 10;
                rows.extend(activity(&mut rng, a));
                design.push(nb(stratum, a, Role::Anchor, a));
                let mut offs = vec![Role::N1];
                if slot == 0 {
                    offs.extend([Role::N15, Role::N150]);
                }
                for role in offs {
                    let b = a + role.offset();
                    design.push(nb(stratum, b, role, a));
                    match kind {
                        Neighbors::Empty => {}
                        Neighbors::Copies => rows.extend(
                            rows.clone()
                                .into_iter()
                                .filter(|r| r.block == a)
                                .map(|mut r| {
                                    r.block = b;
                                    r.tx_hash = format!("{}-{b}", r.tx_hash);
                                    r
                                })
                                .collect::<Vec<_>>(),
                        ),
                        Neighbors::Random => rows.extend(activity(&mut rng, b)),
                    }
                }
            }
        }
    }
    Fixture { rows, design }
}

fn tally(f: &Fixture, stratum: &str) -> StratumNb {
    let d: Vec<&NeighborBlock> = f
        .design
        .iter()
        .filter(|n| n.block.stratum == stratum)
        .collect();
    analyze_stratum(stratum, &d, &f.rows, &NeighborParams::default()).unwrap()
}

#[test]
fn neighbors_that_copy_the_anchor_give_u_exactly_zero() {
    let f = fixture(Neighbors::Copies);
    for s in ["S1", "S2"] {
        let t = tally(&f, s);
        let st = stats(&t.total());
        assert!(st[0] > 0.1 && st[0] < 0.99, "C0 {}", st[0]);
        for o in 0..3 {
            assert_eq!(st[3 + 3 * o], 0.0, "U_{o} of {s}");
            assert_eq!(st[1 + 3 * o], st[2 + 3 * o]);
            assert!(t.pairs(o) > 0.0);
        }
    }
}

#[test]
fn empty_neighbors_give_ck_zero_and_u_equal_to_c0_of_the_matched_anchors() {
    let f = fixture(Neighbors::Empty);
    for s in ["S1", "S2"] {
        let t = tally(&f, s);
        let st = stats(&t.total());
        for o in 0..3 {
            assert_eq!(st[2 + 3 * o], 0.0);
            assert_eq!(st[3 + 3 * o], st[1 + 3 * o]);
            assert_eq!(t.empty_neighbors[o], t.anchors_with[o]);
        }
    }
    // With --drop-blocks-without-rows there are no pairs left.
    let d: Vec<&NeighborBlock> = f
        .design
        .iter()
        .filter(|n| n.block.stratum == "S1")
        .collect();
    let p = NeighborParams {
        drop_blocks_without_rows: true,
        ..NeighborParams::default()
    };
    let t = analyze_stratum("S1", &d, &f.rows, &p).unwrap();
    assert_eq!(t.pairs(0), 0.0);
    assert!(stats(&t.total())[3].is_nan());
}

#[test]
fn c0_on_a_plain_sample_equals_the_contend_weighted_ratio() {
    // Parity: with no neighbors, C(0) is CR_w,obs of `contend` (weights and universe of the
    // anchors).
    let f = fixture(Neighbors::Random);
    let anchors: Vec<u64> = f
        .design
        .iter()
        .filter(|n| n.block.stratum == "S1" && n.role == Role::Anchor)
        .map(|n| n.block.block)
        .collect();
    let set: HashSet<u64> = anchors.iter().copied().collect();
    let mine: Vec<TxRow> = f
        .rows
        .iter()
        .filter(|r| set.contains(&r.block))
        .cloned()
        .collect();
    let bots = contend::bot_universe(&mine, &BotRule::default());
    let ents = contend::entities(&mine, &bots);
    let ds = contend::build_entities(&mine, &anchors, &ents, &HashSet::new());
    let params = ContendParams {
        permutations: 1,
        bootstrap: 0,
        bootstrap_permutations: 0,
        seed: 1,
        metric: Metric::Weighted,
    };
    let r = contend::run_ext(&ds, params, NullKind::Primary, &RunOptions::default());
    let plain: Vec<NeighborBlock> = anchors
        .iter()
        .map(|&a| nb("S1", a, Role::Anchor, a))
        .collect();
    let d: Vec<&NeighborBlock> = plain.iter().collect();
    let t = analyze_stratum("S1", &d, &f.rows, &NeighborParams::default()).unwrap();
    let c0 = stats(&t.total())[0];
    assert_eq!(t.attempts, r.failed);
    assert!((c0 - r.cr_obs).abs() < 1e-12, "{c0} vs {}", r.cr_obs);
    // The same anchors with their neighbors in the design give the same C(0).
    let with_nb = tally(&f, "S1");
    assert!((stats(&with_nb.total())[0] - r.cr_obs).abs() < 1e-12);
}

#[test]
fn bootstrap_interval_brackets_the_point_estimate_and_week_unit_runs() {
    let f = fixture(Neighbors::Random);
    let strata = [tally(&f, "S1"), tally(&f, "S2")];
    for week in [false, true] {
        let p = NeighborParams {
            bootstrap: 200,
            week_unit: week,
            ..NeighborParams::default()
        };
        let s = summarize(&strata, &p);
        assert_eq!(s.len(), 3);
        let (pt, ci) = &s[0];
        assert!(ci[0].0 <= pt[0] + 0.05 && pt[0] <= ci[0].1 + 0.05);
        assert!(s[2].0[0].is_finite());
    }
    let a = summarize(
        &strata,
        &NeighborParams {
            bootstrap: 50,
            ..NeighborParams::default()
        },
    );
    let b = summarize(
        &strata,
        &NeighborParams {
            bootstrap: 50,
            ..NeighborParams::default()
        },
    );
    assert_eq!(a[2].1[3].0.to_bits(), b[2].1[3].0.to_bits());
}

fn write_files(f: &Fixture, dir: &Path, tag: &str) -> (PathBuf, PathBuf) {
    let rows = dir.join(format!("nb_{tag}_rows.csv"));
    let mut w = csv::Writer::from_path(&rows).unwrap();
    for r in &f.rows {
        w.serialize(r).unwrap();
    }
    w.flush().unwrap();
    let blocks = dir.join(format!("nb_{tag}_blocks.csv"));
    let mut text = String::from("stratum,block,hour_start_ts,exploratory,role,anchor\n");
    for n in &f.design {
        text += &format!(
            "{},{},{},{},{},{}\n",
            n.block.stratum,
            n.block.block,
            n.block.hour_start_ts,
            n.block.exploratory,
            n.role.as_str(),
            n.anchor
        );
    }
    std::fs::write(&blocks, text).unwrap();
    (rows, blocks)
}

#[test]
fn rust_and_python_agree_on_the_fixtures() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/analisis/vecino.py");
    let mut compared = 0;
    for (tag, kind) in [
        ("copies", Neighbors::Copies),
        ("empty", Neighbors::Empty),
        ("random", Neighbors::Random),
    ] {
        let f = fixture(kind);
        let (rows, blocks) = write_files(&f, dir, tag);
        let Ok(out) = Command::new("python3")
            .arg(script)
            .args(["--split", "all", "--rows"])
            .arg(&rows)
            .arg("--blocks")
            .arg(&blocks)
            .output()
        else {
            eprintln!("python3 not available: cross-check skipped");
            return;
        };
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        let strata = [tally(&f, "S1"), tally(&f, "S2")];
        let pooled = {
            let mut t = [0.0; 11];
            for s in &strata {
                for (a, b) in t.iter_mut().zip(s.total()) {
                    *a += b;
                }
            }
            t
        };
        let mut expected = Vec::new();
        for (name, sums, n_attempts) in strata
            .iter()
            .map(|s| (s.name.as_str(), s.total(), s.attempts))
            .chain([("POOL", pooled, strata.iter().map(|s| s.attempts).sum())])
        {
            let st = stats(&sums);
            expected.push(format!("{name} intentos={n_attempts} C0={:.10}", st[0]));
            for (o, role) in ["n1", "n15", "n150"].iter().enumerate() {
                let pairs = sums[4 + 3 * o];
                if pairs == 0.0 && name != "POOL" && strata[0].anchors_with[o] == 0 {
                    continue;
                }
                let anchors: usize = match name {
                    "POOL" => strata.iter().map(|s| s.anchors_with[o]).sum(),
                    n => strata.iter().find(|s| s.name == n).unwrap().anchors_with[o],
                };
                expected.push(format!(
                    "{name} {role} anclas={anchors} pares={} C0k={:.10} Ck={:.10} U={:.10}",
                    pairs as u64,
                    st[1 + 3 * o],
                    st[2 + 3 * o],
                    st[3 + 3 * o]
                ));
            }
        }
        let got: Vec<&str> = text.lines().collect();
        assert_eq!(got.len(), expected.len(), "{tag}\n{text}");
        for (g, e) in got.iter().zip(&expected) {
            assert_eq!(*g, e.as_str(), "{tag}");
        }
        compared += 1;
    }
    assert_eq!(compared, 3);
}

#[test]
fn plain_sample_file_reads_as_anchors_without_neighbors() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let p = dir.join("plain_blocks.csv");
    std::fs::write(
        &p,
        "stratum,block,hour_start_ts,exploratory\nS1,100,0,false\nS1,200,0,true\n",
    )
    .unwrap();
    let v = read_neighbor_sample(&p).unwrap();
    assert!(v
        .iter()
        .all(|n| n.role == Role::Anchor && n.anchor == n.block.block));
    assert_eq!(v.len(), 2);
}

#[test]
fn a_neighbor_at_the_wrong_distance_is_rejected() {
    let f = fixture(Neighbors::Random);
    let mut d: Vec<NeighborBlock> = f
        .design
        .iter()
        .filter(|n| n.block.stratum == "S1")
        .cloned()
        .collect();
    let k = d.iter().position(|n| n.role == Role::N1).unwrap();
    d[k].block.block += 1;
    let refs: Vec<&NeighborBlock> = d.iter().collect();
    assert!(analyze_stratum("S1", &refs, &f.rows, &NeighborParams::default()).is_err());
}
