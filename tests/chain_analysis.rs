//! `--chain`, `--exclude-system` and `race --max-sender-degree` (AF-03, ADR 005 D1-D2, D6).
//! Library level on synthetic blocks, plus the CLI and the Rust-Python cross-check.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use flate2::write::GzEncoder;
use flate2::Compression;
use mev_revert_classifier::chain::Chain;
use mev_revert_classifier::contend::{self, EntityRule};
use mev_revert_classifier::extract::{Kind, TxRow, SWAP_TOPICS};
use mev_revert_classifier::neighbor::{
    analyze_stratum, analyze_stratum_on, NbOpts, NeighborParams, StratumNb,
};
use mev_revert_classifier::race::{build_stratum, build_stratum_with, StratumOpts};
use mev_revert_classifier::sample::{
    block_timestamp_on, first_block_at_or_after_on, parse_date, NeighborBlock, Role, SampledBlock,
};
use mev_revert_classifier::system::SystemFilter;
use serde_json::json;

const DEPOSIT_FROM: &str = "0xdeaddeaddeaddeaddeaddeaddeaddeaddead0001";
const L1_BLOCK: &str = "0x4200000000000000000000000000000000000015";
const BUILDER: &str = "0xcabba9e7f4b3a885c5aa069f88469ac711dd4acc";
const FLASHBLOCK_NUMBER: &str = "0x3c3a8a41e095c76b03f79f70955fff3b03cf753e";
const BOT_A: &str = "0x00000000000000000000000000000000000000a1";
const BOT_B: &str = "0x00000000000000000000000000000000000000b2";
const BOT_C: &str = "0x00000000000000000000000000000000000000c3";
const EOA_A: &str = "0x0000000000000000000000000000000000000aa1";
const EOA_B: &str = "0x0000000000000000000000000000000000000bb2";
const EOA_C: &str = "0x0000000000000000000000000000000000000cc3";
const RELAYER: &str = "0x0000000000000000000000000000000000000ee5";
const POOL: &str = "0x00000000000000000000000000000000000000f1";

fn tmp(tag: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("chain_{tag}"));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn list(chain: Chain) -> SystemFilter {
    SystemFilter::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("datos/tx_sistema.csv"),
        chain,
    )
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn row(
    block: u64,
    idx: u64,
    seq: &mut u64,
    from: &str,
    to: &str,
    kind: Kind,
    pools: &[&str],
) -> TxRow {
    *seq += 1;
    TxRow {
        block,
        tx_index: idx,
        tx_hash: format!("0x{:064x}", *seq),
        from: from.into(),
        to: to.into(),
        kind,
        gas_used: 1,
        fee_wei: 1,
        swap_logs: 0,
        cyclic: false,
        pools: pools.join(";"),
    }
}

struct Scenario {
    rows: Vec<TxRow>,
    blocks: Vec<u64>,
}

/// 8 blocks 900 blocks apart (two Unichain hours). Per block: bot A swaps early (even
/// blocks) or late (odd), bot B reverts at index 4 (or succeeds every third block), bot C
/// reverts at 7, A reverts at 6; in block 0 a relayer calls both A and B. With `system`,
/// the builder sends a probe with pools in every block and a successful swap on the same
/// pool at index 0 (adversarial: it would be a bot and a prior swapper); the L1 deposit
/// sits at index 0 as on the real chains.
fn scenario(first: u64, system: bool) -> Scenario {
    let (mut rows, mut seq) = (Vec::new(), 0u64);
    let blocks: Vec<u64> = (0..8).map(|k| first + 900 * k).collect();
    for (k, &b) in blocks.iter().enumerate() {
        let a_at = if k % 2 == 0 { 1 } else { 9 };
        rows.push(row(b, a_at, &mut seq, EOA_A, BOT_A, Kind::Success, &[POOL]));
        let kind_b = if k % 3 == 0 {
            Kind::Success
        } else {
            Kind::Revert
        };
        rows.push(row(b, 4, &mut seq, EOA_B, BOT_B, kind_b, &[POOL]));
        rows.push(row(b, 6, &mut seq, EOA_A, BOT_A, Kind::Revert, &[POOL]));
        rows.push(row(b, 7, &mut seq, EOA_C, BOT_C, Kind::Revert, &[POOL]));
        rows.push(row(b, 8, &mut seq, EOA_C, BOT_C, Kind::Revert, &[POOL]));
        rows.push(row(b, 3, &mut seq, EOA_B, BOT_B, Kind::Revert, &[POOL]));
        if k == 0 {
            rows.push(row(b, 10, &mut seq, RELAYER, BOT_A, Kind::Revert, &[POOL]));
            rows.push(row(b, 11, &mut seq, RELAYER, BOT_B, Kind::Revert, &[POOL]));
        }
        if system {
            rows.push(row(
                b,
                0,
                &mut seq,
                DEPOSIT_FROM,
                L1_BLOCK,
                Kind::Probe,
                &[],
            ));
            rows.push(row(
                b,
                2,
                &mut seq,
                BUILDER,
                FLASHBLOCK_NUMBER,
                Kind::Success,
                &[POOL],
            ));
            rows.push(row(
                b,
                5,
                &mut seq,
                BUILDER,
                FLASHBLOCK_NUMBER,
                Kind::Probe,
                &[POOL],
            ));
        }
    }
    if system {
        // More failed attempts with pools, past the last real index, and a real tx after
        // them, so that the block length does not depend on the system rows.
        for &b in &blocks {
            rows.push(row(
                b,
                13,
                &mut seq,
                BUILDER,
                FLASHBLOCK_NUMBER,
                Kind::Probe,
                &[POOL],
            ));
            rows.push(row(b, 14, &mut seq, EOA_C, BOT_C, Kind::Revert, &[POOL]));
        }
    }
    Scenario { rows, blocks }
}

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
                let logs = if ok && !r.pools.is_empty() {
                    vec![json!({"address": r.pools, "topics": [SWAP_TOPICS[1]], "data": "0x"})]
                } else {
                    vec![]
                };
                json!({"transactionHash": r.tx_hash, "status": if ok {"0x1"} else {"0x0"},
                       "transactionIndex": format!("0x{:x}", r.tx_index), "logs": logs})
            })
            .collect();
        let f = fs::File::create(dir.join(format!("{b}.receipts.json.gz"))).unwrap();
        let mut gz = GzEncoder::new(f, Compression::fast());
        serde_json::to_writer(&mut gz, &json!({"result": rc})).unwrap();
        gz.finish().unwrap();
    }
}

fn write_rows(path: &Path, rows: &[TxRow]) {
    let mut w = csv::Writer::from_path(path).unwrap();
    for r in rows {
        w.serialize(r).unwrap();
    }
    w.flush().unwrap();
}

fn unichain_first() -> u64 {
    first_block_at_or_after_on(Chain::Unichain.timing(), parse_date("2025-08-10").unwrap())
}

fn hour_start(chain: Chain, b: u64) -> i64 {
    block_timestamp_on(chain.timing(), b).div_euclid(3600) * 3600
}

fn write_plain_sample(path: &Path, chain: Chain, blocks: &[u64]) {
    let mut t = String::from("stratum,block,hour_start_ts,exploratory\n");
    for b in blocks {
        t += &format!("S1,{b},{},false\n", hour_start(chain, *b));
    }
    fs::write(path, t).unwrap();
}

fn without_system(rows: &[TxRow]) -> Vec<TxRow> {
    rows.iter()
        .filter(|r| {
            ![DEPOSIT_FROM, BUILDER].contains(&r.from.as_str())
                && ![L1_BLOCK, FLASHBLOCK_NUMBER].contains(&r.to.as_str())
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------- system list

#[test]
fn the_shipped_list_covers_the_dossier_entries_per_chain() {
    let dep = row(1, 0, &mut 0, DEPOSIT_FROM, L1_BLOCK, Kind::Probe, &[]);
    let builder = row(
        1,
        1,
        &mut 0,
        BUILDER,
        FLASHBLOCK_NUMBER,
        Kind::Probe,
        &[POOL],
    );
    let policy = row(
        1,
        2,
        &mut 0,
        BUILDER,
        "0xd44f9d1331659f417a3e22c9e29529d498b66a29",
        Kind::Probe,
        &[],
    );
    let user = row(1, 3, &mut 0, EOA_A, BOT_A, Kind::Revert, &[POOL]);
    for chain in Chain::ALL {
        let f = list(chain);
        assert!(f.matches(&dep), "{chain:?} deposit");
        assert!(!f.matches(&user), "{chain:?} user");
        let on_unichain = chain == Chain::Unichain;
        assert_eq!(f.matches(&builder), on_unichain, "{chain:?} builder");
        assert_eq!(f.matches(&policy), on_unichain, "{chain:?} policy");
    }
    assert_eq!(list(Chain::Unichain).len(), 6);
    assert_eq!(list(Chain::Op).len(), 2);
    assert_eq!(list(Chain::Base).len(), 2);
}

#[test]
fn a_bad_list_is_rejected_instead_of_silently_weakening_the_rule() {
    let h = "cadena,rol,direccion,fuente\n";
    let ok = "0x4200000000000000000000000000000000000015";
    for (bad, why) in [
        (format!("{h}arbitrum,destino,{ok},x\n"), "unknown chain"),
        (format!("{h}base,origen,{ok},x\n"), "unknown role"),
        (format!("{h}base,destino,0x42,x\n"), "short address"),
        (
            format!("{h}base,destino,0xzz00000000000000000000000000000000000015,x\n"),
            "not hex",
        ),
        (
            "cadena,rol,fuente\nbase,destino,x\n".to_string(),
            "missing column",
        ),
    ] {
        assert!(SystemFilter::parse(&bad, Chain::Base).is_err(), "{why}");
    }
    // Comments and upper-case addresses are fine; matching ignores case.
    let f = SystemFilter::parse(
        &format!(
            "{h}# nota\nbase,destino,{},x\n",
            ok.to_uppercase().replace("0X", "0x")
        ),
        Chain::Base,
    )
    .unwrap();
    assert_eq!(f.len(), 1);
    assert!(f.matches(&row(1, 0, &mut 0, EOA_A, ok, Kind::Probe, &[])));
    // An entry of another chain is checked but not applied.
    let other = SystemFilter::parse(&format!("{h}unichain,destino,{ok},x\n"), Chain::Base).unwrap();
    assert!(other.is_empty());
    assert!(SystemFilter::load(Path::new("no/such/list.csv"), Chain::Base).is_err());
    assert_eq!(Chain::parse("op"), Some(Chain::Op));
    assert_eq!(Chain::parse("OP"), None);
    assert_eq!(Chain::parse("arbitrum"), None);
}

// ----------------------------------------------------------------------- race

#[test]
fn a_system_tx_inside_a_raced_block_changes_nothing_once_excluded() {
    let first = unichain_first();
    let s = scenario(first, true);
    let dir = tmp("race_sys");
    write_receipts(&dir, &s.rows);
    let timing = Chain::Unichain.timing();
    let f = list(Chain::Unichain);
    let run = |rows: &[TxRow], system: Option<&SystemFilter>| {
        build_stratum_with(
            "S1",
            rows,
            &s.blocks,
            &dir,
            10,
            &StratumOpts {
                timing,
                max_sender_degree: None,
                system,
            },
        )
        .unwrap()
        .0
    };
    let unfiltered = run(&s.rows, None);
    let filtered = run(&s.rows, Some(&f));
    let clean = run(&without_system(&s.rows), None);
    // Excluded = the same population as if the system rows had never existed.
    assert_eq!(filtered.txs, clean.txs);
    assert_eq!(filtered.n_hours, clean.n_hours);
    // Not excluded, the builder is a bot, adds transactions and makes X = 1 for others.
    assert!(unfiltered.txs.len() > filtered.txs.len());
    let x1 = |d: &mev_revert_classifier::race::StratumData| d.txs.iter().filter(|t| t.x).count();
    assert!(x1(&unfiltered) > x1(&filtered));
    // The filter drops rows; it never invents some.
    assert!(filtered.txs.iter().all(|t| clean.txs.contains(t)));
}

#[test]
fn system_rows_still_count_in_the_block_length_that_scales_the_position() {
    let first = unichain_first();
    let mut seq = 0;
    let mut rows = Vec::new();
    let blocks: Vec<u64> = (0..6).map(|k| first + 10 * k).collect();
    for &b in &blocks {
        rows.push(row(b, 0, &mut seq, EOA_A, BOT_A, Kind::Success, &[POOL]));
        rows.push(row(b, 8, &mut seq, EOA_B, BOT_B, Kind::Revert, &[POOL]));
        rows.push(row(b, 5, &mut seq, EOA_B, BOT_B, Kind::Revert, &[POOL]));
        rows.push(row(b, 3, &mut seq, EOA_A, BOT_A, Kind::Revert, &[POOL]));
        rows.push(row(
            b,
            9,
            &mut seq,
            BUILDER,
            FLASHBLOCK_NUMBER,
            Kind::Probe,
            &[],
        ));
    }
    let dir = tmp("race_len");
    write_receipts(&dir, &rows);
    let f = list(Chain::Unichain);
    let opts = StratumOpts {
        timing: Chain::Unichain.timing(),
        max_sender_degree: None,
        system: Some(&f),
    };
    let d = build_stratum_with("S", &rows, &blocks, &dir, 100, &opts)
        .unwrap()
        .0;
    // Block length 10 (index 9 is the builder), so index 8 -> bin 80, not 8/9 -> 88.
    let max_bin = d.txs.iter().map(|t| t.bin).max().unwrap();
    assert_eq!(max_bin, 80);
    assert!(d.txs.iter().all(|t| t.bin != 88));
}

#[test]
fn default_options_reproduce_the_sealed_build_stratum() {
    let first = unichain_first();
    let s = scenario(first, false);
    let dir = tmp("race_default");
    write_receipts(&dir, &s.rows);
    let a = build_stratum("S", &s.rows, &s.blocks, &dir, 10).unwrap();
    let b = build_stratum_with("S", &s.rows, &s.blocks, &dir, 10, &StratumOpts::default()).unwrap();
    assert_eq!(a.0.txs, b.0.txs);
    assert_eq!(a.0.n_hours, b.0.n_hours);
    assert_eq!(a.1, b.1);
    // The block clock only moves the hour units: Base (2 s) puts them in other hours.
    let uni = build_stratum_with(
        "S",
        &s.rows,
        &s.blocks,
        &dir,
        10,
        &StratumOpts {
            timing: Chain::Unichain.timing(),
            ..StratumOpts::default()
        },
    )
    .unwrap()
    .0;
    assert_eq!(uni.txs.len(), a.0.txs.len());
    assert_ne!(uni.n_hours, a.0.n_hours);
}

#[test]
fn max_sender_degree_splits_entities_joined_by_a_relayer() {
    let first = unichain_first();
    let s = scenario(first, false);
    let dir = tmp("race_deg");
    write_receipts(&dir, &s.rows);
    let run = |deg: Option<usize>| {
        build_stratum_with(
            "S",
            &s.rows,
            &s.blocks,
            &dir,
            10,
            &StratumOpts {
                max_sender_degree: deg,
                ..StratumOpts::default()
            },
        )
        .unwrap()
        .0
    };
    let ents = |d: &mev_revert_classifier::race::StratumData| {
        d.txs.iter().map(|t| t.ent).collect::<HashSet<_>>().len()
    };
    let x1 = |d: &mev_revert_classifier::race::StratumData| d.txs.iter().filter(|t| t.x).count();
    let none = run(None);
    assert_eq!(ents(&none), 2, "A and B merge through the relayer; C apart");
    // The relayer calls 2 bot contracts: degree 2 is ignored, degree 3 is not.
    let d2 = run(Some(2));
    assert_eq!(ents(&d2), 3);
    assert!(x1(&d2) > x1(&none), "separate entities race each other");
    assert_eq!(run(Some(3)).txs, none.txs);
    assert_eq!(run(Some(1_000)).txs, none.txs);
    // N = 0 ignores every sender: each contract is its own entity (same here as N = 2).
    assert_eq!(run(Some(0)).txs, d2.txs);
}

#[test]
fn the_degree_rule_is_the_one_of_contend() {
    let s = scenario(unichain_first(), false);
    let bots = contend::bot_universe(&s.rows, &contend::BotRule::default());
    for k in [0usize, 1, 2, 3] {
        let e = contend::entities_with(
            &s.rows,
            &bots,
            EntityRule {
                max_sender_degree: Some(k),
                ..EntityRule::default()
            },
        );
        let n: HashSet<&String> = e.values().collect();
        assert_eq!(n.len(), if k >= 3 { 2 } else { 3 }, "k={k}");
    }
}

// ------------------------------------------------------------------- neighbor

fn nb_fixture(chain: Chain, system: bool) -> (Vec<TxRow>, Vec<NeighborBlock>) {
    let first = first_block_at_or_after_on(chain.timing(), parse_date("2025-08-10").unwrap());
    let (mut rows, mut design, mut seq) = (Vec::new(), Vec::new(), 0u64);
    let mk = |b: u64, role: Role, anchor: u64| NeighborBlock {
        block: SampledBlock {
            stratum: "S1".into(),
            block: b,
            hour_start_ts: hour_start(chain, b),
            exploratory: false,
        },
        role,
        anchor,
    };
    let bt = chain.block_time();
    for k in 0..8u64 {
        let a = first + 2_000 * k + 10;
        design.push(mk(a, Role::Anchor, a));
        let mut roles = vec![Role::N1];
        if k % 2 == 0 {
            roles.extend([Role::N15, Role::N150]);
        }
        for role in &roles {
            design.push(mk(a + role.offset_on(bt), *role, a));
        }
        for b in std::iter::once(a).chain(roles.iter().map(|r| a + r.offset_on(bt))) {
            let shared = if (b + k) % 3 == 0 { POOL } else { "0xaaaa" };
            rows.push(row(
                b,
                1,
                &mut seq,
                EOA_A,
                BOT_A,
                Kind::Revert,
                &[POOL, shared],
            ));
            rows.push(row(b, 2, &mut seq, EOA_B, BOT_B, Kind::Revert, &[shared]));
            rows.push(row(b, 3, &mut seq, EOA_C, BOT_C, Kind::Success, &[POOL]));
            rows.push(row(b, 4, &mut seq, EOA_C, BOT_C, Kind::Revert, &["0xbbbb"]));
            if system {
                rows.push(row(
                    b,
                    0,
                    &mut seq,
                    DEPOSIT_FROM,
                    L1_BLOCK,
                    Kind::Probe,
                    &[],
                ));
                rows.push(row(
                    b,
                    5,
                    &mut seq,
                    BUILDER,
                    FLASHBLOCK_NUMBER,
                    Kind::Probe,
                    &[POOL],
                ));
                rows.push(row(
                    b,
                    6,
                    &mut seq,
                    BUILDER,
                    FLASHBLOCK_NUMBER,
                    Kind::Probe,
                    &[POOL],
                ));
            }
        }
    }
    (rows, design)
}

fn tally(
    chain: Chain,
    rows: &[TxRow],
    design: &[NeighborBlock],
    system: Option<&SystemFilter>,
) -> StratumNb {
    let d: Vec<&NeighborBlock> = design.iter().collect();
    analyze_stratum_on(
        "S1",
        &d,
        rows,
        &NeighborParams::default(),
        &NbOpts {
            timing: chain.timing(),
            system,
        },
    )
    .unwrap()
}

#[test]
fn unichain_neighbors_sit_two_thirty_and_three_hundred_blocks_away() {
    for (chain, offs) in [
        (Chain::Base, [1, 15, 150]),
        (Chain::Op, [1, 15, 150]),
        (Chain::Unichain, [2, 30, 300]),
    ] {
        let (_, design) = nb_fixture(chain, false);
        for n in &design {
            if n.role != Role::Anchor {
                let o = [Role::N1, Role::N15, Role::N150]
                    .iter()
                    .position(|r| *r == n.role)
                    .unwrap();
                assert_eq!(n.block.block, n.anchor + offs[o], "{chain:?}");
            }
        }
    }
}

#[test]
fn a_design_built_for_another_clock_is_rejected() {
    let (rows, uni) = nb_fixture(Chain::Unichain, false);
    let d: Vec<&NeighborBlock> = uni.iter().collect();
    // Unichain offsets (+2/+30/+300) are not Base offsets (+1/+15/+150).
    assert!(analyze_stratum("S1", &d, &rows, &NeighborParams::default()).is_err());
    let (rows, base) = nb_fixture(Chain::Base, false);
    let d: Vec<&NeighborBlock> = base.iter().collect();
    assert!(analyze_stratum_on(
        "S1",
        &d,
        &rows,
        &NeighborParams::default(),
        &NbOpts {
            timing: Chain::Unichain.timing(),
            system: None
        }
    )
    .is_err());
    // OP shares the Base clock: its design is accepted under either.
    let (rows, op) = nb_fixture(Chain::Op, false);
    let d: Vec<&NeighborBlock> = op.iter().collect();
    assert!(analyze_stratum_on(
        "S1",
        &d,
        &rows,
        &NeighborParams::default(),
        &NbOpts {
            timing: Chain::Op.timing(),
            system: None
        }
    )
    .is_ok());
}

#[test]
fn excluding_system_rows_gives_the_tallies_of_a_world_without_them() {
    let (rows, design) = nb_fixture(Chain::Unichain, true);
    let f = list(Chain::Unichain);
    let filtered = tally(Chain::Unichain, &rows, &design, Some(&f));
    let clean = tally(Chain::Unichain, &without_system(&rows), &design, None);
    let unfiltered = tally(Chain::Unichain, &rows, &design, None);
    assert_eq!(filtered.attempts, clean.attempts);
    assert_eq!(filtered.hours, clean.hours);
    assert_eq!(filtered.empty_neighbors, clean.empty_neighbors);
    // Unfiltered the builder is a bot whose probes share POOL with everyone.
    assert!(unfiltered.attempts > filtered.attempts);
}

#[test]
fn a_neighbor_with_only_system_rows_is_downloaded_not_empty() {
    let (mut rows, design) = nb_fixture(Chain::Unichain, true);
    let victim = design
        .iter()
        .find(|n| n.role == Role::N1)
        .unwrap()
        .block
        .block;
    rows.retain(|r| r.block != victim || [DEPOSIT_FROM, BUILDER].contains(&r.from.as_str()));
    let f = list(Chain::Unichain);
    let t = tally(Chain::Unichain, &rows, &design, Some(&f));
    assert_eq!(t.empty_neighbors, [0, 0, 0]);
    // Without any row at all it is empty.
    rows.retain(|r| r.block != victim);
    assert_eq!(
        tally(Chain::Unichain, &rows, &design, Some(&f)).empty_neighbors[0],
        1
    );
}

#[test]
fn contend_hours_follow_the_block_clock() {
    let first = unichain_first();
    let s = scenario(first, false);
    let bots = contend::bot_universe(&s.rows, &contend::BotRule::default());
    let ent = contend::entities(&s.rows, &bots);
    let none = HashSet::new();
    let uni = contend::build_entities_on(Chain::Unichain.timing(), &s.rows, &s.blocks, &ent, &none);
    let base = contend::build_entities(&s.rows, &s.blocks, &ent, &none);
    // 900-block steps from an hour boundary: 4 blocks per Unichain hour; the Base clock
    // (2 s) would cut the same blocks differently.
    assert_eq!(uni.hours.len(), 2);
    assert_ne!(base.hours.len(), uni.hours.len());
    assert_eq!(
        uni.hour_keys[0],
        block_timestamp_on(Chain::Unichain.timing(), first) / 3600
    );
}

// ------------------------------------------------------------------------ CLI

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mev-revert-classifier"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

struct Files {
    rows: PathBuf,
    blocks: PathBuf,
    raw: PathBuf,
}

fn cli_files(tag: &str, chain: Chain, system: bool) -> Files {
    let dir = tmp(tag);
    let first = first_block_at_or_after_on(chain.timing(), parse_date("2025-08-10").unwrap());
    let s = scenario(first, system);
    write_receipts(&dir, &s.rows);
    let (rows, blocks) = (dir.join("rows.csv"), dir.join("blocks.csv"));
    write_rows(&rows, &s.rows);
    write_plain_sample(&blocks, chain, &s.blocks);
    Files {
        rows,
        blocks,
        raw: dir,
    }
}

fn race_args<'a>(f: &'a Files, extra: &[&'a str]) -> Vec<&'a str> {
    let mut v = vec![
        "race",
        "--rows",
        f.rows.to_str().unwrap(),
        "--blocks",
        f.blocks.to_str().unwrap(),
        "--raw",
        f.raw.to_str().unwrap(),
        "--split",
        "confirmatory",
        "--bootstrap",
        "20",
        "--permutations",
        "20",
        "--sin-probes-only",
    ];
    v.extend(extra);
    v
}

#[test]
fn an_unknown_chain_is_refused_by_every_analysis_subcommand() {
    let f = cli_files("cli_unknown", Chain::Unichain, false);
    for sub in ["race", "contend", "neighbor"] {
        let o = cli(&[
            sub,
            "--rows",
            f.rows.to_str().unwrap(),
            "--blocks",
            f.blocks.to_str().unwrap(),
            "--chain",
            "arbitrum",
        ]);
        assert!(!o.status.success(), "{sub}");
        let t = text(&o);
        assert!(
            t.contains("invalid value") && t.contains("base"),
            "{sub}: {t}"
        );
    }
}

#[test]
fn the_sample_must_match_the_chain_clock() {
    // A Unichain sample read as Base: hour_start_ts is not the Base hour of the blocks.
    let f = cli_files("cli_clock", Chain::Unichain, false);
    for sub in ["race", "contend", "neighbor"] {
        let mut a = if sub == "race" {
            race_args(&f, &[])
        } else {
            vec![
                sub,
                "--rows",
                f.rows.to_str().unwrap(),
                "--blocks",
                f.blocks.to_str().unwrap(),
                "--split",
                "confirmatory",
            ]
        };
        let o = cli(&a);
        assert!(!o.status.success(), "{sub} as base");
        assert!(text(&o).contains("wrong --chain"), "{sub}: {}", text(&o));
        a.extend(["--chain", "unichain", "--exclude-system=false"]);
        let o = cli(&a);
        assert!(o.status.success(), "{sub} as unichain: {}", text(&o));
    }
}

#[test]
fn defaults_per_chain_and_the_explicit_switch() {
    let f = cli_files("cli_default", Chain::Unichain, true);
    // Unichain: the filter is on by default and reports what it dropped.
    let on = text(&cli(&race_args(&f, &["--chain", "unichain"])));
    assert!(
        on.contains("chain unichain (block time 1 s) | system filter on, 6 entries"),
        "{on}"
    );
    assert!(
        on.contains("system remitente 0xcabba9e7f4b3a885c5aa069f88469ac711dd4acc"),
        "{on}"
    );
    // Explicitly off: no builder bot is excluded and the line says so.
    let off = text(&cli(&race_args(
        &f,
        &["--chain", "unichain", "--exclude-system=false"],
    )));
    assert!(off.contains("system filter off"), "{off}");
    assert_ne!(on, off);
    // Base default prints no chain line at all (output unchanged); on request it does.
    let fb = cli_files("cli_base", Chain::Base, true);
    let base = text(&cli(&race_args(&fb, &[])));
    assert!(
        !base.contains("chain base") && !base.contains("system"),
        "{base}"
    );
    let base_on = text(&cli(&race_args(&fb, &["--exclude-system"])));
    assert!(
        base_on.contains("chain base (block time 2 s) | system filter on"),
        "{base_on}"
    );
    // A missing list is an error when the filter is on, harmless when it is off.
    let o = cli(&race_args(
        &f,
        &["--chain", "unichain", "--system-list", "nope.csv"],
    ));
    assert!(
        !o.status.success() && text(&o).contains("no system list"),
        "{}",
        text(&o)
    );
    assert!(cli(&race_args(
        &f,
        &[
            "--chain",
            "unichain",
            "--system-list",
            "nope.csv",
            "--exclude-system=false"
        ]
    ))
    .status
    .success());
}

#[test]
fn race_max_sender_degree_zero_and_large_through_the_cli() {
    let f = cli_files("cli_deg", Chain::Base, false);
    let plain = text(&cli(&race_args(&f, &[])));
    let big = text(&cli(&race_args(&f, &["--max-sender-degree", "1000"])));
    let zero = text(&cli(&race_args(&f, &["--max-sender-degree", "0"])));
    let two = text(&cli(&race_args(&f, &["--max-sender-degree", "2"])));
    assert!(big.contains("senders with degree >= 1000 ignored"), "{big}");
    // Same estimates as the default (a header line is the only difference).
    let strip = |t: &str| {
        t.lines()
            .filter(|l| !l.starts_with("entities:") && !l.starts_with("rows sha256 "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let body = |t: &str| {
        strip(t)
            .lines()
            .filter(|l| l.starts_with("POOL") || l.starts_with("S1"))
            .map(String::from)
            .collect::<Vec<_>>()
    };
    assert_eq!(body(&plain), body(&big));
    assert_eq!(body(&zero), body(&two));
    assert_ne!(body(&plain), body(&zero));
    assert!(
        cli(&race_args(&f, &["--max-sender-degree", "-1"]))
            .status
            .code()
            != Some(0)
    );
    assert!(
        cli(&race_args(&f, &["--max-sender-degree", "x"]))
            .status
            .code()
            != Some(0)
    );
}

// ------------------------------------------------------------ Rust vs Python

fn python(script: &str, args: &[&str]) -> Option<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/analisis")
        .join(script);
    let out = Command::new("python3")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg(path)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        let e = String::from_utf8_lossy(&out.stderr);
        if e.contains("No module named") {
            eprintln!("python deps missing: cross-check skipped");
            return None;
        }
        panic!("{script} failed: {e}");
    }
    Some(String::from_utf8(out.stdout).unwrap())
}

/// Fields of a race line that do not depend on the random draws.
fn race_point(line: &str) -> Vec<String> {
    let mut s = line.to_string();
    while s.contains("= ") {
        s = s.replace("= ", "=");
    }
    s.split_whitespace()
        .filter(|t| {
            [
                "n=",
                "nX1=",
                "celdas=",
                "tasa0=",
                "tasa1=",
                "MH(ent,dec)=",
                "solo-decil(",
            ]
            .iter()
            .any(|p| t.starts_with(p))
        })
        .map(String::from)
        .collect::<Vec<_>>()
        .into_iter()
        .chain(s.split_whitespace().take(2).map(String::from))
        .collect()
}

#[test]
fn race_agrees_with_python_under_chain_system_filter_and_degree() {
    for (chain, name) in [(Chain::Unichain, "unichain"), (Chain::Base, "base")] {
        let f = cli_files(&format!("py_race_{name}"), chain, true);
        for deg in [None, Some("2"), Some("0")] {
            for system in ["--exclude-system=true", "--exclude-system=false"] {
                let mut extra = vec!["--chain", name, system];
                if let Some(d) = deg {
                    extra.extend(["--max-sender-degree", d]);
                }
                let rust = text(&cli(&race_args(&f, &extra)));
                let mut pargs = vec![
                    "--rows",
                    f.rows.to_str().unwrap(),
                    "--blocks",
                    f.blocks.to_str().unwrap(),
                    "--raw",
                    f.raw.to_str().unwrap(),
                    "--split",
                    "confirmatory",
                    "--sin-probes",
                ];
                pargs.extend(&extra);
                let Some(py) = python("carrera_tasa.py", &pargs) else {
                    return;
                };
                let pick = |t: &str| {
                    t.lines()
                        .filter(|l| l.starts_with("S1 "))
                        .map(race_point)
                        .collect::<Vec<_>>()
                };
                let (r, p) = (pick(&rust), pick(&py));
                assert!(!r.is_empty(), "{rust}");
                assert_eq!(r, p, "{name} {system} deg={deg:?}\nRUST\n{rust}\nPY\n{py}");
            }
        }
    }
}

#[test]
fn neighbor_agrees_with_python_under_chain_and_system_filter() {
    for chain in [Chain::Unichain, Chain::Base] {
        let name = chain.as_str();
        let (rows, design) = nb_fixture(chain, true);
        let dir = tmp(&format!("py_nb_{name}"));
        let (rp, bp) = (dir.join("rows.csv"), dir.join("blocks.csv"));
        write_rows(&rp, &rows);
        let mut t = String::from("stratum,block,hour_start_ts,exploratory,role,anchor\n");
        for n in &design {
            t += &format!(
                "S1,{},{},false,{},{}\n",
                n.block.block,
                n.block.hour_start_ts,
                n.role.as_str(),
                n.anchor
            );
        }
        fs::write(&bp, t).unwrap();
        for system in ["--exclude-system=true", "--exclude-system=false"] {
            let args = [
                "--rows",
                rp.to_str().unwrap(),
                "--blocks",
                bp.to_str().unwrap(),
                "--split",
                "all",
                "--chain",
                name,
                system,
            ];
            let Some(py) = python("vecino.py", &args) else {
                return;
            };
            let f = (system == "--exclude-system=true").then(|| list(chain));
            let st = tally(chain, &rows, &design, f.as_ref());
            let tot = mev_revert_classifier::neighbor::stats(&st.total());
            let num = |line: &str, key: &str| -> f64 {
                line.split_whitespace()
                    .find_map(|t| t.strip_prefix(key))
                    .unwrap()
                    .parse()
                    .unwrap()
            };
            let c0_line = py
                .lines()
                .find(|l| l.starts_with("POOL intentos="))
                .unwrap();
            assert_eq!(
                num(c0_line, "intentos=") as usize,
                st.attempts,
                "{name} {system}"
            );
            assert!(
                (num(c0_line, "C0=") - tot[0]).abs() < 1e-9,
                "{name} {system}"
            );
            for (o, role) in ["n1", "n15", "n150"].iter().enumerate() {
                let l = py
                    .lines()
                    .find(|l| l.starts_with(&format!("POOL {role} ")))
                    .unwrap();
                assert_eq!(
                    num(l, "anclas=") as usize,
                    st.anchors_with[o],
                    "{name} {role}"
                );
                assert_eq!(num(l, "pares="), st.pairs(o), "{name} {role}");
                assert!(
                    (num(l, "U=") - tot[3 + 3 * o]).abs() < 1e-9,
                    "{name} {role} {system}\n{py}"
                );
                assert!(
                    (num(l, "Ck=") - tot[2 + 3 * o]).abs() < 1e-9,
                    "{name} {role}"
                );
            }
        }
    }
}
