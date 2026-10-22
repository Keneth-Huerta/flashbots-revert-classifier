//! Descriptions of AF-04 on the race test (T7 to T9; fundamento
//! `carrera-placebos-seleccion.md` §4.3 and §4.4): exact Kitagawa decomposition of the
//! reversal between the decile-only and the (entity, decile) estimators, concentration and
//! leave-one-out by entity key, and DerSimonian-Laird heterogeneity between weeks.

use std::collections::BTreeMap;

use crate::contend::Rng;
use crate::placebo::{orig_ent, week_ci, week_stats};
use crate::race::{
    interval, multiplicities, par_map, prepare, ratio, stream, RaceParams, Spec, StratumData, Tx,
};

fn w_of(n1: f64, n0: f64) -> f64 {
    n1 * n0 / (n1 + n0)
}

/// Partial sums of one stratum, all additive over strata:
/// `[N_dec, D_dec, N_mh, D_mh, N_ds, D_ds, N_w, N_c]`.
fn decomp_sums(txs: &[&Tx]) -> [f64; 8] {
    let mut cells: BTreeMap<(u32, u32), [f64; 4]> = BTreeMap::new();
    for t in txs {
        let c = cells.entry((t.ent, t.bin)).or_default();
        let (a, b) = if t.x { (0, 1) } else { (2, 3) };
        c[a] += 1.0;
        c[b] += t.y as u8 as f64;
    }
    let mut dec: BTreeMap<u32, [f64; 4]> = BTreeMap::new();
    let mut sdec: BTreeMap<u32, Vec<[f64; 4]>> = BTreeMap::new();
    let (mut n_mh, mut d_mh) = (0.0, 0.0);
    for (&(_, d), c) in &cells {
        let t = dec.entry(d).or_default();
        for k in 0..4 {
            t[k] += c[k];
        }
        if c[0] > 0.0 && c[2] > 0.0 {
            let w = w_of(c[0], c[2]);
            n_mh += w * (c[1] / c[0] - c[3] / c[2]);
            d_mh += w;
            sdec.entry(d).or_default().push(*c);
        }
    }
    let (mut n_dec, mut d_dec) = (0.0, 0.0);
    for c in dec.values() {
        if c[0] > 0.0 && c[2] > 0.0 {
            let w = w_of(c[0], c[2]);
            n_dec += w * (c[1] / c[0] - c[3] / c[2]);
            d_dec += w;
        }
    }
    let (mut n_ds, mut d_ds, mut n_w, mut n_c) = (0.0, 0.0, 0.0, 0.0);
    for cs in sdec.values() {
        let n1: f64 = cs.iter().map(|c| c[0]).sum();
        let n0: f64 = cs.iter().map(|c| c[2]).sum();
        let r1: f64 = cs.iter().map(|c| c[1]).sum();
        let r0: f64 = cs.iter().map(|c| c[3]).sum();
        let w = w_of(n1, n0);
        n_ds += w * (r1 / n1 - r0 / n0);
        d_ds += w;
        let (mut within, mut comp) = (0.0, 0.0);
        for c in cs {
            let (p1, p0) = (c[0] / n1, c[2] / n0);
            let (rr1, rr0) = (c[1] / c[0], c[3] / c[2]);
            within += (p1 + p0) / 2.0 * (rr1 - rr0);
            comp += (p1 - p0) * (rr1 + rr0) / 2.0;
        }
        n_w += w * within;
        n_c += w * comp;
    }
    [n_dec, d_dec, n_mh, d_mh, n_ds, d_ds, n_w, n_c]
}

#[derive(Debug, Clone, Copy)]
pub struct Decomp {
    pub dec: f64,
    pub mh: f64,
    pub gap: f64,
    /// Composition between entities.
    pub comp: f64,
    /// Weighting.
    pub wgt: f64,
    /// Support (cells with one group).
    pub sup: f64,
    /// `|gap - (comp + wgt + sup)|`.
    pub resid: f64,
}

fn decomp_of(sums: &[[f64; 8]], mult: &[u32]) -> Decomp {
    let mut a = [0.0; 8];
    for (s, &m) in sums.iter().zip(mult) {
        for k in 0..8 {
            a[k] += m as f64 * s[k];
        }
    }
    let dec = ratio(a[0], a[1]);
    let mh = ratio(a[2], a[3]);
    let ds = ratio(a[4], a[5]);
    let w = ratio(a[6], a[5]);
    let comp = ratio(a[7], a[5]);
    let (sup, wgt, gap) = (dec - ds, w - mh, dec - mh);
    Decomp {
        dec,
        mh,
        gap,
        comp,
        wgt,
        sup,
        resid: (gap - (comp + wgt + sup)).abs(),
    }
}

#[derive(Debug, Clone)]
pub struct DecompReport {
    pub obs: Decomp,
    pub ci_comp: (f64, f64),
    pub ci_wgt: (f64, f64),
    pub ci_sup: (f64, f64),
    pub ci_gap: (f64, f64),
    /// Largest identity residual over the observed data and every resample.
    pub max_resid: f64,
    /// "composition", "support", "weighting" or "mixed" (T8 rule), or "inconclusive" if |G| < 0.02.
    pub verdict: &'static str,
}

/// Exact decomposition of `Delta_dec - Delta_MH` (no probes), with an interval from
/// resampling whole strata (the multiplicities of `--boot-unit week`, variant `vi`).
pub fn decompose(strata: &[StratumData], vi: u64, p: &RaceParams) -> DecompReport {
    let sums: Vec<[f64; 8]> = strata
        .iter()
        .map(|s| decomp_sums(&s.txs.iter().filter(|t| !t.probe).collect::<Vec<_>>()))
        .collect();
    let ones = vec![1u32; strata.len()];
    let obs = decomp_of(&sums, &ones);
    let reps: Vec<Decomp> = (0..p.bootstrap)
        .map(|r| {
            let mut rng = stream(p.seed, &[u64::MAX, vi, 0, 3, r as u64]);
            let m = multiplicities(strata.len(), &mut rng);
            decomp_of(&sums, &m)
        })
        .collect();
    let ci = |f: fn(&Decomp) -> f64| interval(reps.iter().map(f).collect());
    let (ci_comp, ci_wgt, ci_sup, ci_gap) =
        (ci(|d| d.comp), ci(|d| d.wgt), ci(|d| d.sup), ci(|d| d.gap));
    let max_resid = reps.iter().map(|d| d.resid).fold(obs.resid, f64::max);
    let verdict = if obs.gap.abs() < 0.02 {
        "inconclusive"
    } else if obs.comp / obs.gap >= 0.5 && (ci_comp.0 > 0.0 || ci_comp.1 < 0.0) {
        "composition"
    } else if obs.sup / obs.gap >= 0.5 && (ci_sup.0 > 0.0 || ci_sup.1 < 0.0) {
        "support"
    } else if obs.wgt / obs.gap >= 0.5 && (ci_wgt.0 > 0.0 || ci_wgt.1 < 0.0) {
        "weighting"
    } else {
        "mixed"
    };
    DecompReport {
        obs,
        ci_comp,
        ci_wgt,
        ci_sup,
        ci_gap,
        max_resid,
        verdict,
    }
}

#[derive(Debug, Clone)]
pub struct EntityWeek {
    pub pos: usize,
    pub neg: usize,
    pub zero: usize,
    pub w_pos: f64,
    pub w_neg: f64,
    pub w_zero: f64,
    /// Entity-weeks below 30 transactions per group (not classified), and their MH weight.
    pub skipped: usize,
    pub w_skipped: f64,
}

/// Sign of the effect inside every entity-week (cells = deciles of the entity-week;
/// at least 30 transactions per group) and the MH weight of the (entity, decile) estimator.
pub fn entity_weeks(strata: &[StratumData]) -> EntityWeek {
    let mut o = EntityWeek {
        pos: 0,
        neg: 0,
        zero: 0,
        w_pos: 0.0,
        w_neg: 0.0,
        w_zero: 0.0,
        skipped: 0,
        w_skipped: 0.0,
    };
    for s in strata {
        let mut ew: BTreeMap<u32, BTreeMap<u32, [f64; 4]>> = BTreeMap::new();
        for t in s.txs.iter().filter(|t| !t.probe) {
            let c = ew.entry(t.ent).or_default().entry(t.bin).or_default();
            let (a, b) = if t.x { (0, 1) } else { (2, 3) };
            c[a] += 1.0;
            c[b] += t.y as u8 as f64;
        }
        for cells in ew.values() {
            let (mut num, mut den, mut n1, mut n0) = (0.0, 0.0, 0.0, 0.0);
            for c in cells.values() {
                n1 += c[0];
                n0 += c[2];
                if c[0] > 0.0 && c[2] > 0.0 {
                    let w = w_of(c[0], c[2]);
                    num += w * (c[1] / c[0] - c[3] / c[2]);
                    den += w;
                }
            }
            if den == 0.0 {
                continue;
            }
            if n1 < 30.0 || n0 < 30.0 {
                o.skipped += 1;
                o.w_skipped += den;
                continue;
            }
            let d = num / den;
            if d.abs() < 1e-12 {
                o.zero += 1;
                o.w_zero += den;
            } else if d > 0.0 {
                o.pos += 1;
                o.w_pos += den;
            } else {
                o.neg += 1;
                o.w_neg += den;
            }
        }
    }
    o
}

#[derive(Debug, Clone)]
pub struct Concentration {
    /// `(key, share of the MH weight)`, descending.
    pub top: Vec<(String, f64)>,
    /// Herfindahl index of the shares of all keys.
    pub hhi: f64,
}

/// Share of the MH weight (cells (entity, bin), no probes) of every entity key.
pub fn concentration(strata: &[StratumData], keys: &[Vec<String>]) -> Concentration {
    let mut by_key: BTreeMap<&str, f64> = BTreeMap::new();
    let mut total = 0.0;
    for (s, k) in strata.iter().zip(keys) {
        let mut cells: BTreeMap<(u32, u32), [f64; 4]> = BTreeMap::new();
        for t in s.txs.iter().filter(|t| !t.probe) {
            let c = cells.entry((t.ent, t.bin)).or_default();
            let (a, b) = if t.x { (0, 1) } else { (2, 3) };
            c[a] += 1.0;
            c[b] += t.y as u8 as f64;
        }
        for (&(e, _), c) in &cells {
            if c[0] > 0.0 && c[2] > 0.0 {
                let w = w_of(c[0], c[2]);
                *by_key.entry(k[orig_ent(e)].as_str()).or_default() += w;
                total += w;
            }
        }
    }
    let mut top: Vec<(String, f64)> = by_key
        .into_iter()
        .map(|(k, w)| (k.to_string(), w / total))
        .collect();
    top.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let hhi = top.iter().map(|x| x.1 * x.1).sum();
    Concentration { top, hhi }
}

/// Strata without the transactions of the entities with key `key`.
pub fn drop_key(strata: &[StratumData], keys: &[Vec<String>], key: &str) -> Vec<StratumData> {
    strata
        .iter()
        .zip(keys)
        .map(|(s, k)| StratumData {
            name: s.name.clone(),
            txs: s
                .txs
                .iter()
                .filter(|t| k[orig_ent(t.ent)] != key)
                .copied()
                .collect(),
            n_hours: s.n_hours,
        })
        .collect()
}

/// Leave-one-out: estimate and interval (weeks) without each of the `k` largest keys.
pub fn leave_one_out(
    strata: &[StratumData],
    keys: &[Vec<String>],
    k: usize,
    p: &RaceParams,
) -> Vec<(String, f64, f64, (f64, f64))> {
    let conc = concentration(strata, keys);
    conc.top
        .iter()
        .take(k)
        .map(|(key, share)| {
            let st = drop_key(strata, keys, key);
            let (d, ci) = week_ci(&week_stats(&st, false, p), 0, p);
            (key.clone(), *share, d, ci)
        })
        .collect()
}

// ---------------------------------------------------------------- heterogeneity

#[derive(Debug, Clone)]
pub struct Het {
    pub k: usize,
    pub fe: f64,
    pub q: f64,
    pub tau2: f64,
    pub i2: f64,
    pub re: f64,
    pub re_se: f64,
    /// 95% confidence interval of the random-effects estimate.
    pub re_ci: (f64, f64),
    /// 95% prediction interval.
    pub pred: (f64, f64),
    /// Weighted least squares slope on the week index (and its standard error), Spearman.
    pub slope: f64,
    pub spearman: f64,
}

/// Per stratum, the estimate and the variance of an hour bootstrap within the stratum.
pub fn week_estimates(strata: &[StratumData], vi: u64, p: &RaceParams) -> Vec<(f64, f64)> {
    strata
        .iter()
        .enumerate()
        .filter_map(|(si, s)| {
            let t: Vec<&Tx> = s.txs.iter().filter(|t| !t.probe).collect();
            let pr = prepare(&t, s.n_hours, Spec::Cond, p);
            if pr.den <= 0.0 || pr.by_hour.is_empty() {
                return None;
            }
            let reps: Vec<f64> = par_map(p.bootstrap, |r| {
                let mut rng: Rng = stream(p.seed, &[si as u64, vi, 0, 2, r as u64]);
                let m = multiplicities(pr.by_hour.len(), &mut rng);
                let (n, d) = pr.resample(&m);
                ratio(n, d)
            })
            .into_iter()
            .filter(|x| x.is_finite())
            .collect();
            if reps.len() < 2 {
                return None;
            }
            let mean = reps.iter().sum::<f64>() / reps.len() as f64;
            let var =
                reps.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (reps.len() - 1) as f64;
            Some((pr.num / pr.den, var))
        })
        .collect()
}

fn ranks(v: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|&a, &b| v[a].total_cmp(&v[b]));
    let mut r = vec![0.0; v.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && v[idx[j + 1]] == v[idx[i]] {
            j += 1;
        }
        for &q in &idx[i..=j] {
            r[q] = (i + j) as f64 / 2.0 + 1.0;
        }
        i = j + 1;
    }
    r
}

pub fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let vb: f64 = b.iter().map(|y| (y - mb).powi(2)).sum();
    cov / (va * vb).sqrt()
}

/// DerSimonian-Laird on `(estimate, variance)` per week in week order; `index` is the
/// week position used for the trend.
pub fn dersimonian_laird(est: &[(f64, f64)], index: &[f64]) -> Option<Het> {
    let est_v: Vec<(f64, f64, f64)> = est
        .iter()
        .zip(index)
        .filter(|(e, _)| e.1 > 0.0)
        .map(|(e, &i)| (e.0, e.1, i))
        .collect();
    let k = est_v.len();
    if k < 2 {
        return None;
    }
    let s1: f64 = est_v.iter().map(|x| 1.0 / x.1).sum();
    let s2: f64 = est_v.iter().map(|x| 1.0 / (x.1 * x.1)).sum();
    let fe = est_v.iter().map(|x| x.0 / x.1).sum::<f64>() / s1;
    let q: f64 = est_v.iter().map(|x| (x.0 - fe).powi(2) / x.1).sum();
    let df = (k - 1) as f64;
    let tau2 = ((q - df) / (s1 - s2 / s1)).max(0.0);
    let i2 = if q > 0.0 {
        ((q - df) / q).max(0.0)
    } else {
        0.0
    };
    let sw: f64 = est_v.iter().map(|x| 1.0 / (x.1 + tau2)).sum();
    let re = est_v.iter().map(|x| x.0 / (x.1 + tau2)).sum::<f64>() / sw;
    let re_var = 1.0 / sw;
    let re_se = re_var.sqrt();
    let half = 1.96 * (tau2 + re_var).sqrt();
    let (xs, ys): (Vec<f64>, Vec<f64>) = est_v.iter().map(|x| (x.2, x.0)).unzip();
    let w: Vec<f64> = est_v.iter().map(|x| 1.0 / x.1).collect();
    let sw1: f64 = w.iter().sum();
    let mx = xs.iter().zip(&w).map(|(x, w)| x * w).sum::<f64>() / sw1;
    let my = ys.iter().zip(&w).map(|(y, w)| y * w).sum::<f64>() / sw1;
    let sxy: f64 = xs
        .iter()
        .zip(&ys)
        .zip(&w)
        .map(|((x, y), w)| w * (x - mx) * (y - my))
        .sum();
    let sxx: f64 = xs.iter().zip(&w).map(|(x, w)| w * (x - mx).powi(2)).sum();
    Some(Het {
        k,
        fe,
        q,
        tau2,
        i2,
        re,
        re_se,
        re_ci: (re - 1.96 * re_se, re + 1.96 * re_se),
        pred: (re - half, re + half),
        slope: sxy / sxx,
        spearman: pearson(&ranks(&xs), &ranks(&ys)),
    })
}
