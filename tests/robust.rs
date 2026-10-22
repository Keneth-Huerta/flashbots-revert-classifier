//! AF-04 descriptions (ADR 006 T7 to T9): exact Kitagawa identity, synthetic Simpson
//! inversion, concentration and leave-one-out, DerSimonian-Laird.

use mev_revert_classifier::contend::Rng;
use mev_revert_classifier::race::{RaceParams, StratumData, Tx};
use mev_revert_classifier::robust::*;

fn params(b: usize) -> RaceParams {
    RaceParams {
        bootstrap: b,
        permutations: 1,
        seed: 5,
        week_unit: true,
        ..RaceParams::default()
    }
}

/// Entities recoded as `(ent * 4 + g) * 2 + z` like `apply_variant` does (g = 3, z = 0).
fn enc(e: u32) -> u32 {
    (e * 4 + 3) * 2
}

fn random_strata(seed: u64, weeks: usize, ents: u32) -> Vec<StratumData> {
    let mut rng = Rng::new(seed);
    (0..weeks)
        .map(|w| {
            let mut txs = Vec::new();
            for _ in 0..1500 {
                let e = rng.below(ents as usize) as u32;
                let bin = rng.below(10) as u32;
                // Some (entity, bin) cells get one group only: X depends on the entity.
                let px = 0.1 + 0.8 * (e as f64 / ents as f64);
                let x = (rng.next_u64() as f64 / u64::MAX as f64) < px;
                let py = 0.2 + 0.08 * e as f64 + 0.03 * bin as f64 + if x { 0.04 } else { 0.0 };
                let y = (rng.next_u64() as f64 / u64::MAX as f64) < py.min(0.95);
                txs.push(Tx {
                    ent: enc(e),
                    bin,
                    block: rng.below(500) as u64,
                    hour: rng.below(8) as u32,
                    x,
                    y,
                    probe: false,
                });
            }
            StratumData {
                name: format!("W{w:02}"),
                txs,
                n_hours: 8,
            }
        })
        .collect()
}

#[test]
fn the_decomposition_identity_is_exact_on_random_data_and_every_resample() {
    for seed in 0..12 {
        let s = random_strata(seed, 6, 3 + (seed % 5) as u32);
        let r = decompose(&s, 0, &params(200));
        assert!(r.obs.resid < 1e-12, "seed {seed}: {}", r.obs.resid);
        assert!(
            r.max_resid < 1e-12,
            "seed {seed}: resample residual {}",
            r.max_resid
        );
        let o = r.obs;
        assert!((o.gap - (o.comp + o.wgt + o.sup)).abs() < 1e-12);
        assert!((o.gap - (o.dec - o.mh)).abs() < 1e-15);
    }
}

#[test]
fn identity_holds_with_sparse_cells_and_single_group_cells() {
    let mut rng = Rng::new(77);
    let txs: Vec<Tx> = (0..300)
        .map(|_| {
            let e = rng.below(40) as u32; // many cells with one group only
            Tx {
                ent: enc(e),
                bin: rng.below(10) as u32,
                block: 0,
                hour: 0,
                x: rng.below(2) == 0,
                y: rng.below(3) == 0,
                probe: false,
            }
        })
        .collect();
    let s = vec![StratumData {
        name: "W00".into(),
        txs,
        n_hours: 1,
    }];
    let r = decompose(&s, 0, &params(100));
    assert!(r.max_resid < 1e-12);
    assert!(
        r.obs.sup.abs() > 0.0,
        "sparse design must have a support term"
    );
}

#[test]
fn synthetic_simpson_reproduces_the_inversion_and_composition_has_the_sign_of_the_gap() {
    // Entity 0: high revert level (0.8), X rare. Entity 1: low level (0.2), X common.
    // Within each entity X raises revert by +0.05; pooled over entities the sign flips.
    let mut rng = Rng::new(1);
    let mut txs = Vec::new();
    for _ in 0..40_000 {
        let e = (rng.below(2)) as u32;
        let px = if e == 0 { 0.1 } else { 0.9 };
        let x = (rng.next_u64() as f64 / u64::MAX as f64) < px;
        let base = if e == 0 { 0.8 } else { 0.2 };
        let y = (rng.next_u64() as f64 / u64::MAX as f64) < base + if x { 0.05 } else { 0.0 };
        txs.push(Tx {
            ent: enc(e),
            bin: 0,
            block: 0,
            hour: 0,
            x,
            y,
            probe: false,
        });
    }
    let s = vec![StratumData {
        name: "W00".into(),
        txs,
        n_hours: 1,
    }];
    let r = decompose(&s, 0, &params(50));
    assert!(
        r.obs.mh > 0.03 && r.obs.dec < 0.0,
        "mh {} dec {}",
        r.obs.mh,
        r.obs.dec
    );
    assert!(r.obs.gap < 0.0 && r.obs.comp < 0.0);
    assert_eq!(r.verdict, "composition");
    assert!(r.obs.sup.abs() < 1e-12, "no single-group cells here");
}

#[test]
fn verdict_is_inconclusive_when_the_gap_is_small() {
    let s = random_strata(3, 4, 1); // one entity: dec == mh up to cells
    let r = decompose(&s, 0, &params(20));
    assert!(r.obs.gap.abs() < 0.02);
    assert_eq!(r.verdict, "inconclusive");
}

#[test]
fn probes_never_enter_the_decomposition() {
    let mut s = random_strata(4, 3, 3);
    let a = decompose(&s, 0, &params(10)).obs;
    for t in s[0].txs.iter_mut().step_by(5) {
        t.probe = true;
    }
    let mut b_only = s.clone();
    b_only[0].txs.retain(|t| !t.probe);
    let b = decompose(&s, 0, &params(10)).obs;
    let c = decompose(&b_only, 0, &params(10)).obs;
    assert_eq!(b.gap.to_bits(), c.gap.to_bits());
    assert_ne!(a.gap.to_bits(), b.gap.to_bits());
}

#[test]
fn entity_weeks_split_by_sign_and_threshold() {
    let mut txs = Vec::new();
    // Entity 0: 60 + 60 tx, positive effect. Entity 1: 60 + 60, negative. Entity 2: 10 + 10 (skipped).
    for (e, n, up) in [(0u32, 60usize, true), (1, 60, false), (2, 10, true)] {
        for i in 0..n {
            for x in [true, false] {
                let y = if x == up { i % 2 == 0 } else { i % 4 == 0 };
                txs.push(Tx {
                    ent: enc(e),
                    bin: 0,
                    block: 0,
                    hour: 0,
                    x,
                    y,
                    probe: false,
                });
            }
        }
    }
    let ew = entity_weeks(&[StratumData {
        name: "W".into(),
        txs,
        n_hours: 1,
    }]);
    assert_eq!((ew.pos, ew.neg, ew.zero, ew.skipped), (1, 1, 0, 1));
    assert!(ew.w_pos > 0.0 && ew.w_neg > 0.0 && ew.w_skipped > 0.0);
}

#[test]
fn concentration_and_leave_one_out_by_key() {
    let s = random_strata(9, 5, 4);
    let keys: Vec<Vec<String>> = s
        .iter()
        .map(|_| (0..4).map(|e| format!("k{e}")).collect())
        .collect();
    let c = concentration(&s, &keys);
    assert_eq!(c.top.len(), 4);
    let total: f64 = c.top.iter().map(|x| x.1).sum();
    assert!((total - 1.0).abs() < 1e-12);
    assert!(c.top.windows(2).all(|w| w[0].1 >= w[1].1));
    assert!(c.hhi >= 0.25 - 1e-12 && c.hhi <= 1.0);
    let loo = leave_one_out(&s, &keys, 2, &params(50));
    assert_eq!(loo.len(), 2);
    let dropped = drop_key(&s, &keys, &loo[0].0);
    assert!(dropped[0].txs.len() < s[0].txs.len());
}

#[test]
fn dersimonian_laird_matches_a_hand_computed_example() {
    // k = 3, v = 1 each: FE = mean; Q = sum (y - mean)^2; tau2 = (Q - 2) / (3 - 3/3).
    let est = [(0.0, 1.0), (1.0, 1.0), (4.0, 1.0)];
    let h = dersimonian_laird(&est, &[0.0, 1.0, 2.0]).unwrap();
    let fe: f64 = 5.0 / 3.0;
    let q = (fe * fe) + (1.0 - fe).powi(2) + (4.0 - fe).powi(2);
    assert!((h.fe - fe).abs() < 1e-12 && (h.q - q).abs() < 1e-12);
    let tau2 = (q - 2.0) / 2.0;
    assert!((h.tau2 - tau2).abs() < 1e-12);
    assert!((h.i2 - (q - 2.0) / q).abs() < 1e-12);
    let w = 1.0 / (1.0 + tau2);
    assert!((h.re - (5.0 / 3.0)).abs() < 1e-12); // equal weights
    assert!((h.re_se - (1.0 / (3.0 * w)).sqrt()).abs() < 1e-12);
    assert!((h.slope - 2.0).abs() < 1e-12);
    assert!((h.spearman - 1.0).abs() < 1e-12);
}

#[test]
fn dersimonian_laird_has_zero_heterogeneity_for_identical_estimates_and_floors_tau2() {
    let est = [(0.07, 0.0004), (0.07, 0.0009), (0.07, 0.0001)];
    let h = dersimonian_laird(&est, &[0.0, 1.0, 2.0]).unwrap();
    assert_eq!((h.tau2, h.i2), (0.0, 0.0));
    assert!((h.re - 0.07).abs() < 1e-12);
    assert!(dersimonian_laird(&est[..1], &[0.0]).is_none());
    assert!(dersimonian_laird(&[(0.1, 0.0), (0.2, 0.0)], &[0.0, 1.0]).is_none());
}

#[test]
fn heterogeneity_detects_a_week_trend_in_simulated_weeks() {
    let mut rng = Rng::new(2);
    // 12 weeks with effect decreasing from 0.15 to 0.0.
    let s: Vec<StratumData> = (0..12)
        .map(|w| {
            let eff = 0.15 - 0.0136 * w as f64;
            let mut txs = Vec::new();
            for i in 0..4000 {
                let x = i % 2 == 0;
                let bin = (i / 2 % 10) as u32;
                let py = 0.3 + 0.02 * bin as f64 + if x { eff } else { 0.0 };
                let y = (rng.next_u64() as f64 / u64::MAX as f64) < py;
                txs.push(Tx {
                    ent: enc(0),
                    bin,
                    block: 0,
                    hour: rng.below(10) as u32,
                    x,
                    y,
                    probe: false,
                });
            }
            StratumData {
                name: format!("W{w:02}"),
                txs,
                n_hours: 10,
            }
        })
        .collect();
    let est = week_estimates(&s, 0, &params(200));
    assert_eq!(est.len(), 12);
    let idx: Vec<f64> = (0..12).map(|i| i as f64).collect();
    let h = dersimonian_laird(&est, &idx).unwrap();
    assert!(h.spearman < -0.9 && h.slope < 0.0 && h.i2 > 0.5, "{h:?}");
    // Deterministic.
    let again = week_estimates(&s, 0, &params(200));
    assert_eq!(est[3].1.to_bits(), again[3].1.to_bits());
}
