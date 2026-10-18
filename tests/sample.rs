use mev_revert_classifier::sample::*;

#[test]
fn date_to_block_matches_known_base_block() {
    // Dune: block 39872527 has block_time 2025-12-24 00:00:01 UTC.
    let ts = parse_date("2025-12-24").unwrap();
    assert_eq!(first_block_at_or_after(ts), 39_872_527);
    assert_eq!(block_timestamp(39_872_527), ts + 1);
}

#[test]
fn civil_dates_handle_leap_years_and_reject_garbage() {
    assert_eq!(parse_date("1970-01-01"), Some(0));
    assert_eq!(
        parse_date("2024-03-01").unwrap() - parse_date("2024-02-28").unwrap(),
        2 * 86_400
    );
    assert_eq!(
        parse_date("2025-03-01").unwrap() - parse_date("2025-02-28").unwrap(),
        86_400
    );
    for bad in [
        "2025-13-01",
        "2025-00-10",
        "2025-1",
        "x-y-z",
        "2025-01-01-01",
        "",
    ] {
        assert_eq!(parse_date(bad), None, "{bad}");
    }
}

#[test]
fn every_hour_gets_exactly_per_hour_blocks_inside_the_hour() {
    let ts = parse_date("2025-06-22").unwrap();
    let s = sample_week("S1", ts, 6, None);
    assert_eq!(s.len(), 7 * 24 * 6);
    for b in &s {
        let t = block_timestamp(b.block);
        assert!(
            t >= b.hour_start_ts && t < b.hour_start_ts + 3600,
            "block {} outside hour",
            b.block
        );
    }
    let mut blocks: Vec<u64> = s.iter().map(|b| b.block).collect();
    blocks.dedup();
    assert_eq!(blocks.len(), s.len(), "duplicates");
}

#[test]
fn sampling_is_deterministic_and_exclusion_is_respected() {
    let ts = parse_date("2025-12-21").unwrap();
    let ex = (39_872_527, 39_873_117);
    let a = sample_week("S4", ts, 6, Some(ex));
    let b = sample_week("S4", ts, 6, Some(ex));
    assert_eq!(a, b);
    assert!(a.iter().all(|x| x.block < ex.0 || x.block > ex.1));
    // The excluded range spans the first hour of Dec 24; that hour still gets 6 blocks.
    let hour = parse_date("2025-12-24").unwrap();
    assert_eq!(a.iter().filter(|x| x.hour_start_ts == hour).count(), 6);
}

#[test]
fn exploratory_share_is_close_to_twenty_percent() {
    let n = 20_000u64;
    let k = (0..n).filter(|&b| is_exploratory(b)).count() as f64 / n as f64;
    assert!((k - 52.0 / 256.0).abs() < 0.01, "share {k}");
}

fn neighbor_week() -> Vec<NeighborBlock> {
    sample_week_neighbors("S1", parse_date("2025-06-22").unwrap(), None)
}

#[test]
fn neighbor_design_has_three_anchors_and_four_neighbors_per_hour() {
    let s = neighbor_week();
    let count = |r: Role| s.iter().filter(|n| n.role == r).count();
    assert_eq!(count(Role::Anchor), 7 * 24 * 3);
    assert_eq!(count(Role::N1), 7 * 24 * 2);
    assert_eq!(count(Role::N15), 7 * 24);
    assert_eq!(count(Role::N150), 7 * 24);
    for n in &s {
        assert_eq!(n.block.block, n.anchor + n.role.offset());
        assert_eq!(n.block.stratum, "S1");
    }
    // Each hour: anchors inside the hour, the n15/n150 pair on the same (third) anchor.
    for a in s.iter().filter(|n| n.role == Role::Anchor) {
        let t = block_timestamp(a.block.block);
        assert!(t >= a.block.hour_start_ts && t < a.block.hour_start_ts + 3600);
        let has = |r: Role| s.iter().any(|n| n.role == r && n.anchor == a.anchor);
        assert!(has(Role::N1) != has(Role::N15));
        assert_eq!(has(Role::N15), has(Role::N150));
    }
}

#[test]
fn neighbors_never_coincide_with_anchors_other_neighbors_or_exclusions() {
    let base = first_block_at_or_after(parse_date("2025-06-22").unwrap());
    let ex = (base + 3000, base + 3600);
    for exclude in [None, Some(ex)] {
        let s = sample_week_neighbors("S1", parse_date("2025-06-22").unwrap(), exclude);
        let anchors: Vec<u64> = s
            .iter()
            .filter(|n| n.role == Role::Anchor)
            .map(|n| n.block.block)
            .collect();
        let mut all: Vec<u64> = s.iter().map(|n| n.block.block).collect();
        let total = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), total, "a block appears twice");
        for n in s.iter().filter(|n| n.role != Role::Anchor) {
            assert!(!anchors.contains(&n.block.block));
        }
        if let Some((a, z)) = exclude {
            assert!(s.iter().all(|n| n.block.block < a || n.block.block > z));
        }
    }
}

#[test]
fn neighbor_sampling_is_deterministic_and_anchors_follow_the_plain_scheme() {
    assert_eq!(neighbor_week(), neighbor_week());
    // Without clashes the anchors are the 3 smallest-hash blocks of the plain sample.
    let plain = sample_week("S1", parse_date("2025-06-22").unwrap(), 3, None);
    let anchors: Vec<u64> = neighbor_week()
        .iter()
        .filter(|n| n.role == Role::Anchor)
        .map(|n| n.block.block)
        .collect();
    let same = plain.iter().filter(|b| anchors.contains(&b.block)).count();
    assert!(
        same as f64 >= 0.95 * plain.len() as f64,
        "{same} of {}",
        plain.len()
    );
}

#[test]
fn neighbors_inherit_the_split_of_their_anchor() {
    for n in neighbor_week() {
        assert_eq!(n.block.exploratory, is_exploratory(n.anchor));
    }
}

#[test]
fn role_round_trips() {
    for r in [Role::Anchor, Role::N1, Role::N15, Role::N150] {
        assert_eq!(Role::parse(r.as_str()), Some(r));
    }
    assert_eq!(Role::parse("n2"), None);
}

#[test]
fn plain_cli_output_is_byte_identical_to_the_committed_sample() {
    // `sample` without --neighbors must keep reproducing the sealed v1 sample.
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("plain_sample.csv");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_mev-revert-classifier"))
        .args([
            "sample",
            "--stratum",
            "S1=2025-06-22",
            "--stratum",
            "S2=2025-08-10",
            "--stratum",
            "S3=2025-09-14",
            "--stratum",
            "S4=2025-12-21",
            "--exclude",
            "39872527-39873117",
            "--out",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(status.status.success());
    let committed = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/datos/muestra_bloques_v1.csv"
    ));
    if let Ok(committed) = committed {
        assert_eq!(std::fs::read(&out).unwrap(), committed);
    }
}

#[test]
fn neighbors_cli_adds_role_and_anchor_columns() {
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("nb_sample.csv");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_mev-revert-classifier"))
        .args([
            "sample",
            "--neighbors",
            "--stratum",
            "S1=2025-06-22",
            "--out",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(status.status.success());
    let text = std::fs::read_to_string(&out).unwrap();
    assert_eq!(
        text.lines().next().unwrap(),
        "stratum,block,hour_start_ts,exploratory,role,anchor"
    );
    assert_eq!(text.lines().count(), 1 + 7 * 24 * 7);
}
