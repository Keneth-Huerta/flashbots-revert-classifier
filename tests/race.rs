use std::collections::HashSet;

use mev_revert_classifier::contend::Rng;
use mev_revert_classifier::race::*;

fn params(k: usize, b: usize) -> RaceParams {
    RaceParams {
        bootstrap: b,
        permutations: k,
        seed: 11,
        ..RaceParams::default()
    }
}

/// One stratum: `ents` entities x `bins` bins, `per_cell` tx per cell spread over
/// `hours` hours. X is assigned within every cell as a random subset of size ~40%, so it is
/// independent of the cell; Y is Bernoulli(base + effect * X).
fn synthetic(seed: u64, effect: f64) -> StratumData {
    synthetic_n(seed, effect, 24)
}

fn synthetic_n(seed: u64, effect: f64, per_cell: usize) -> StratumData {
    let mut rng = Rng::new(seed);
    let (ents, bins, hours) = (6u32, 10u32, 12usize);
    let mut txs = Vec::new();
    for e in 0..ents {
        for b in 0..bins {
            let base = 0.2 + 0.05 * e as f64 + 0.03 * b as f64; // cell effect, same for X=0/1
            let mut xs: Vec<bool> = (0..per_cell).map(|i| i < per_cell * 2 / 5).collect();
            rng.shuffle(&mut xs);
            for x in xs {
                let p = base + if x { effect } else { 0.0 };
                let u = rng.next_u64() as f64 / u64::MAX as f64;
                txs.push(Tx {
                    ent: e,
                    bin: b,
                    block: rng.below(hours * 5) as u64,
                    hour: rng.below(hours) as u32,
                    x,
                    y: u < p,
                    probe: false,
                });
            }
        }
    }
    StratumData {
        name: "S".into(),
        txs,
        n_hours: hours,
    }
}

#[test]
fn x_needs_a_successful_swap_of_another_entity_earlier_on_a_shared_pool() {
    let p = |s: &str| vec![s.to_string()];
    let (pa, pb) = (p("a"), p("b"));
    let swap = |s: &str| HashSet::from([s.to_string()]);
    let none = HashSet::new();
    let list = vec![
        (1u64, 0u32, pa.as_slice(), swap("a")), // entity 0 swaps on a
        (2, 0, pa.as_slice(), none.clone()),    // same entity: X = 0
        (3, 1, pa.as_slice(), none.clone()),    // other entity, later: X = 1
        (4, 1, pb.as_slice(), none.clone()),    // other pool: X = 0
        (5, 2, pa.as_slice(), swap("a")), // X = 1 (entity 0 swapped on a at index 1); it is also a swap
        (6, 0, pa.as_slice(), none.clone()), // X = 1 (entities 0 and 2 swapped before)
    ];
    let x = x_for_block(&list);
    assert_eq!(x, vec![false, false, true, false, true, true]);
    // A swap that comes later does not count.
    let late = vec![
        (1u64, 1u32, pa.as_slice(), none),
        (2, 0, pa.as_slice(), swap("a")),
    ];
    assert_eq!(x_for_block(&late), vec![false, false]);
}

#[test]
fn mh_matches_a_hand_computed_example() {
    // Cell 1: X=1 {1,0} rate .5, X=0 {0,0,1} rate 1/3 -> d = 1/6, w = 2*3/5.
    // Cell 2: X=1 {1} rate 1, X=0 {1} rate 1 -> d = 0, w = 1/2.
    let mk = |ent, x, y| Tx {
        ent,
        bin: 0,
        block: 0,
        hour: 0,
        x,
        y,
        probe: false,
    };
    let txs = vec![
        mk(0, true, true),
        mk(0, true, false),
        mk(0, false, false),
        mk(0, false, false),
        mk(0, false, true),
        mk(1, true, true),
        mk(1, false, true),
        mk(2, true, true), // cell with one group only: ignored
    ];
    let s = StratumData {
        name: "S".into(),
        txs,
        n_hours: 1,
    };
    let l = &analyze(&[s], 0, false, &params(20, 20))[0];
    let expect = (1.0 / 6.0 * 1.2) / (1.2 + 0.5);
    assert!((l.cond.d - expect).abs() < 1e-12, "{}", l.cond.d);
    assert_eq!(l.cond.cells, 2);
    assert_eq!((l.n, l.n1), (8, 4));
}

#[test]
fn probes_leave_the_main_variant_and_stay_with_y_zero_in_the_other() {
    let mut s = synthetic(1, 0.0);
    for t in s.txs.iter_mut().step_by(7) {
        t.probe = true;
        t.y = false;
    }
    let probes = s.txs.iter().filter(|t| t.probe).count();
    let without = analyze(std::slice::from_ref(&s), 0, false, &params(5, 5));
    let with = analyze(std::slice::from_ref(&s), 1, true, &params(5, 5));
    assert_eq!(without[0].n + probes, with[0].n);
}

#[test]
fn analysis_is_deterministic() {
    let s = synthetic(3, 0.03);
    let a = analyze(std::slice::from_ref(&s), 0, false, &params(50, 50));
    let b = analyze(std::slice::from_ref(&s), 0, false, &params(50, 50));
    assert_eq!(a[0].cond.p.to_bits(), b[0].cond.p.to_bits());
    assert_eq!(a[0].cond.ci.0.to_bits(), b[0].cond.ci.0.to_bits());
    assert_eq!(a[1].cond.ci.1.to_bits(), b[1].cond.ci.1.to_bits());
}

#[test]
fn calibration_x_randomized_within_cells_has_no_effect_and_uniform_p() {
    let trials = 200;
    let (mut false_pos, mut sum) = (0, 0.0);
    let mut ps = Vec::new();
    for t in 0..trials {
        let s = synthetic(1000 + t, 0.0);
        let l = &analyze(&[s], 0, false, &params(199, 0))[0];
        sum += l.cond.d;
        ps.push(l.cond.p);
        false_pos += (l.cond.p <= 0.05) as usize;
    }
    let rate = false_pos as f64 / trials as f64;
    assert!((0.02..=0.09).contains(&rate), "false positive rate {rate}");
    let mean = sum / trials as f64;
    assert!(mean.abs() < 0.005, "mean estimate {mean}");
    // p roughly uniform: the mean is near 1/2 and each quartile holds 15% to 35%.
    let pm = ps.iter().sum::<f64>() / ps.len() as f64;
    assert!((0.42..=0.58).contains(&pm), "mean p {pm}");
    for q in 0..4 {
        let c = ps
            .iter()
            .filter(|&&p| p > q as f64 / 4.0 && p <= (q + 1) as f64 / 4.0)
            .count() as f64
            / trials as f64;
        assert!((0.15..=0.35).contains(&c), "quartile {q}: {c}");
    }
}

#[test]
fn power_a_seeded_effect_of_five_points_is_detected() {
    let mut detected = 0;
    let mut est = 0.0;
    let trials = 20;
    for t in 0..trials {
        let s = synthetic_n(5000 + t, 0.05, 80);
        let l = &analyze(&[s], 0, false, &params(199, 200))[0];
        est += l.cond.d;
        detected += (l.cond.p <= 0.05 && l.cond.ci.0 > 0.0) as usize;
    }
    let mean = est / trials as f64;
    assert!((0.035..=0.065).contains(&mean), "mean estimate {mean}");
    assert!(detected >= 18, "detected {detected} of {trials}");
}

#[test]
fn pooled_week_unit_resamples_whole_strata() {
    let a = synthetic(7, 0.05);
    let mut b = synthetic(8, 0.05);
    b.name = "T".into();
    let hour = analyze(&[a.clone(), b.clone()], 0, false, &params(20, 100));
    let week = analyze(
        &[a, b],
        0,
        false,
        &RaceParams {
            week_unit: true,
            ..params(20, 100)
        },
    );
    let (ph, pw) = (&hour[2], &week[2]);
    assert_eq!(ph.name, "POOL");
    // Same point estimate and permutation p; only the interval changes with the unit.
    assert_eq!(ph.cond.d.to_bits(), pw.cond.d.to_bits());
    assert_eq!(ph.cond.p.to_bits(), pw.cond.p.to_bits());
    assert!(pw.cond.ci.0.is_finite() && pw.cond.ci.0 <= pw.cond.ci.1);
}
