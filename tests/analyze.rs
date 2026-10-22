use mev_revert_classifier::{analyze, pct, Density, DuneTransaction};

fn tx(bot: &str, block: Option<u64>, label: &str, gas: u64, fee: f64) -> DuneTransaction {
    DuneTransaction {
        week_number: None,
        week_start: None,
        week_end: None,
        bot_address: bot.to_string(),
        tx_hash: format!("0x{bot}{block:?}{gas}"),
        block_time: block.map(|b| format!("2025-12-24 00:00:{:02}.000 UTC", b % 60)),
        block_number: block,
        tx_index: None,
        spam_classification: Some(label.to_string()),
        success: Some((label == "dry_run_probe").to_string()),
        value: None,
        gas_used: Some(gas),
        gas_limit: None,
        base_fee_eth: None,
        priority_fee_eth: None,
        total_gas_fee_eth: Some(fee),
    }
}

fn burst(bot: &str, block: u64, n: usize) -> Vec<DuneTransaction> {
    (0..n)
        .map(|i| tx(bot, Some(block), "dry_run_probe", 100 + i as u64, 0.0))
        .collect()
}

// ERR-04: "unique bots" used to count (block, bot) groups.
#[test]
fn unique_bots_counts_bots_not_groups() {
    let mut data = Vec::new();
    for block in 1..=50 {
        data.extend(burst("a", block, 4));
    }
    let r = analyze(&data, 3, 10);
    assert_eq!(r.unique_bots, 1);
    assert_eq!(r.self_burst.unique_bots, 1);
    assert_eq!(r.self_burst.groups, 50);
}

#[test]
fn threshold_boundary_is_inclusive() {
    let mut data = burst("a", 1, 3);
    data.extend(burst("b", 1, 2));
    let r = analyze(&data, 3, 10);
    assert_eq!(r.self_burst.tx, 3);
    assert_eq!(r.sparse.tx, 2);
    assert_eq!(r.top_groups.len(), 1);
    assert_eq!(r.top_groups[0].density, Density::SelfBurst);
    assert_eq!(r.top_groups[0].bot, "a");
}

#[test]
fn categories_partition_all_usable_rows() {
    let mut data = Vec::new();
    for (i, bot) in ["a", "b", "c", "d"].iter().enumerate() {
        for block in 0..(i as u64 + 3) {
            data.extend(burst(bot, block, (block as usize % 5) + 1));
        }
    }
    for t in [1, 2, 3, 5, 100] {
        let r = analyze(&data, t, 10);
        assert_eq!(r.sparse.tx + r.self_burst.tx, r.total_rows, "threshold {t}");
        assert_eq!(r.probes + r.reverts + r.other_label, r.total_rows);
    }
}

#[test]
fn threshold_one_makes_everything_a_burst_and_huge_makes_nothing() {
    let data = burst("a", 7, 2);
    assert_eq!(analyze(&data, 1, 10).self_burst.tx, 2);
    assert_eq!(analyze(&data, 0, 10).self_burst.tx, 2);
    let r = analyze(&data, usize::MAX, 10);
    assert_eq!(r.self_burst.tx, 0);
    assert!(r.top_groups.is_empty());
}

#[test]
fn rows_without_block_or_bot_are_dropped() {
    let data = vec![
        tx("a", Some(1), "dry_run_probe", 10, 0.0),
        tx("a", None, "dry_run_probe", 10, 0.0),
        tx("", Some(1), "reverted_spam", 10, 0.0),
    ];
    let r = analyze(&data, 3, 10);
    assert_eq!(r.total_rows, 1);
    assert_eq!(r.unique_bots, 1);
    assert_eq!(r.reverts, 0);
}

#[test]
fn outcome_labels_and_fees_are_counted() {
    let data = vec![
        tx("a", Some(1), "dry_run_probe", 10, 0.5),
        tx("a", Some(1), "reverted_spam", 10, 0.25),
        tx("b", Some(2), "something_else", 10, 0.25),
    ];
    let r = analyze(&data, 3, 10);
    assert_eq!((r.probes, r.reverts, r.other_label), (1, 1, 1));
    assert!((r.total_fee_eth - 1.0).abs() < 1e-12);
}

#[test]
fn distinct_bots_per_block_min_median_max() {
    let mut data = burst("a", 1, 1);
    data.extend(burst("a", 2, 5)); // block 2: a, b, c
    data.extend(burst("b", 2, 1));
    data.extend(burst("c", 2, 1));
    data.extend(burst("a", 3, 1)); // block 3: a, b
    data.extend(burst("b", 3, 1));
    let r = analyze(&data, 3, 10);
    assert_eq!(r.bots_per_block, (1, 2, 3));
    assert_eq!(r.unique_blocks, 3);
    assert_eq!((r.first_block, r.last_block), (1, 3));
}

#[test]
fn average_gas_is_per_transaction_not_per_group() {
    let data = vec![
        tx("a", Some(1), "dry_run_probe", 100, 0.0),
        tx("a", Some(1), "dry_run_probe", 200, 0.0),
        tx("a", Some(1), "dry_run_probe", 300, 0.0),
    ];
    let r = analyze(&data, 3, 10);
    assert_eq!(r.self_burst.avg_gas(), 200.0);
    assert_eq!(r.sparse.avg_gas(), 0.0);
}

#[test]
fn top_bots_ranked_by_attempts_with_blocks_and_max() {
    let mut data = burst("small", 1, 1);
    data.extend(burst("big", 1, 2));
    data.extend(burst("big", 2, 6));
    let r = analyze(&data, 3, 1);
    assert_eq!(r.top_bots.len(), 1);
    let b = &r.top_bots[0];
    assert_eq!(
        (b.bot.as_str(), b.tx, b.blocks, b.max_per_block),
        ("big", 8, 2, 6)
    );
}

#[test]
fn ties_are_ordered_deterministically() {
    let mut data = burst("z", 5, 3);
    data.extend(burst("a", 9, 3));
    data.extend(burst("a", 5, 3));
    let r1 = analyze(&data, 3, 10);
    data.reverse();
    let r2 = analyze(&data, 3, 10);
    let key = |r: &mev_revert_classifier::Report| {
        r.top_groups
            .iter()
            .map(|g| (g.block, g.bot.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        key(&r1),
        vec![(5, "a".into()), (5, "z".into()), (9, "a".into())]
    );
    assert_eq!(key(&r1), key(&r2));
}

#[test]
fn empty_input_does_not_panic() {
    let r = analyze(&[], 3, 10);
    assert_eq!(r.total_rows, 0);
    assert_eq!(r.bots_per_block, (0, 0, 0));
    assert_eq!(pct(0, 0), 0.0);
}
