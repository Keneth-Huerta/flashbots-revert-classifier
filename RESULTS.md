# Results

Every number is an estimate over the sampled weeks of 2025 that each section states. Intervals are 95%
bootstrap intervals (resampling whole weeks unless noted), seed 20261017. With 1,000
permutations, `p = 0.001` is the floor: no permutation matched the observed value. These results
say nothing about other years, other chains, or contracts outside each sample's bot universe.

Terms: a **probe** is a successful transaction that emits no external logs and carries at most
1 wei; a **revert** is a transaction that reverted. An **entity** merges bot contracts that share
a sender. **Overlap** is the share of a failed attempt's pools (weighted by how few entities touch
each pool) that another entity also touched in the same block.

## v0.1.0 claim, retracted

Version 0.1.0 grouped attempts by bot and block and called dense groups "contention"; a figure of
about 90% was quoted publicly. That figure measured one bot repeating itself. It did not measure
different bots competing. Under the corrected measure below, the overlap above a within-hour null
is 0.05 to 0.15 instead of 0.9. Run on the AF-01 weeks, the v0.1.0 heuristic still marks 69% to 95%
of failed attempts as "contention", which shows the size of the error.

## AF-01: overlap between entities on Base (4 weeks)

Weeks starting 2025-06-22, 2025-08-10, 2025-09-14 and 2025-12-21, about 800 blocks each, sampled
by UTC hour. The author preregistered and sealed the study in a private repository before opening the
confirmatory blocks (`docs/04_investigacion_y_labs/`). The seal time is the author's own commit
time with no external timestamp, so this seal is weaker evidence than AF-03's.

Statistic: weighted overlap CRw of failed attempts, against a null that reshuffles each entity's
bundles among blocks of the same hour. E = CRw observed minus null mean.

| Week | Entities | Failed attempts with pools | CRw observed [95% CI] | Null | E [95% CI] |
|---|---:|---:|---|---|---|
| S1 2025-06-22 | 116 | 47,289 | 0.765 [0.754, 0.776] | 0.714 | 0.051 [0.046, 0.056] |
| S2 2025-08-10 | 77 | 46,278 | 0.753 [0.738, 0.769] | 0.607 | 0.146 [0.136, 0.157] |
| S3 2025-09-14 | 83 | 112,255 | 0.817 [0.805, 0.830] | 0.712 | 0.106 [0.096, 0.115] |
| S4 2025-12-21 | 84 | 68,219 | 0.657 [0.626, 0.682] | 0.523 | 0.134 [0.123, 0.145] |

Holm-adjusted p = 0.004 in all four weeks.

The result supports this: in these four weeks, failed attempts share more pool weight with another entity
in the same block than a within-hour reshuffle gives. The excess is 0.05 points in June and 0.11
to 0.15 in the other three weeks.

It does not support the following (from the review the author wrote after the run):
- E is an excess of overlap over a null. It is not a fraction of failures caused by competition.
- E mixes block-level synchrony with drift inside the hour (see AF-02, where synchrony is bounded
  at 1.4 points).
- With the binary overlap metric the same data give E of 0.011 to 0.022 (below the 0.05 threshold).
  The author chose the weighted metric on the exploratory half of the same weeks.
- In S3 the result depends on how contracts are merged into entities: without merging E is 0.034;
  ignoring senders that call 10 or more bot contracts it is 0.033.
- A day-level jackknife gives standard errors up to three times the hour bootstrap; in S1 the
  condition E >= 0.05 sits inside the error (0.051 +/- 0.015).

## AF-02: race inside the block and block synchrony on Base (48 weeks)

The 48 weeks of 2025 not used in AF-01, 504 anchor blocks per week, sampled by UTC hour.
The author preregistered and sealed the study privately before the download (commit time only, no
external timestamp).

**H1, race.** Among non-probe transactions from bot contracts, the test compares the revert rate
of those that follow a successful swap by another entity earlier in the block on one of the same
pools against those that do not. The strata are entity and decile of position in the block
(Mantel-Haenszel risk difference).

| Variant | n | Cells | Risk difference [95% CI] | Without entity control [95% CI] |
|---|---:|---:|---|---|
| Without probes (primary) | 313,549 | 8,635 | +0.0775 [+0.0656, +0.0928] | -0.0298 [-0.0501, -0.0110] |
| With probes | 2,083,668 | 16,971 | +0.0089 [+0.0075, +0.0103] | -0.0271 [-0.0395, -0.0159] |

The difference is positive in 48 of 48 weeks (lower bound above 0 in 41). Preregistered
sensitivities: 50 position bins +0.0741; cells (entity, block) +0.0651; hour bootstrap
+0.0775 [+0.0716, +0.0827]. The primary result met the preregistered criterion (risk difference
at least 0.02 with an interval excluding 0). Without controlling for entity the sign flips
(Simpson's paradox); the author did not decompose the cause. With probes the effect (+0.0089) is less than half of the
0.02 threshold, so the claim does not extend to probes. The author did not run a sensitivity that
excludes senders with 10 or more bot contracts for this study.

The preregistered sensitivity that reruns the Study 1 overlap on the Study 2 anchors (`contend`,
3 anchors per hour instead of 6) gives an excess E between 0.013 and 0.136 by week (mean 0.061),
below 0.05 in 26 of 48 weeks. The mean is 0.030 over the first 24 sampled weeks and 0.092 over the
last 24, while the weekly risk difference above moves the other way (mean 0.115 against 0.062; weekly
correlation r = -0.38). The two studies agree in sign, not in time trend, and the trend was not
modelled. E shrinks with fewer blocks per hour, so these values are not comparable to Study 1's.

**H2, block synchrony bound.** Overlap in the anchor block against the block 2 s later, 30 s later
and 5 min later, same anchors.

| Distance | Pairs | U_k [95% CI] |
|---|---:|---|
| 2 s | 1,318,574 | +0.0129 [+0.0113, +0.0144] |
| 30 s | 660,536 | +0.0295 [+0.0268, +0.0322] |
| 5 min | 660,536 | +0.0863 [+0.0717, +0.1004] |

U_2s is an upper bound of block-level synchrony; it does not estimate it. The preregistered
criterion (upper limit below 0.02) is met: on Base in these weeks, block synchrony is at most 2
points. The curve grows with distance. That fits drift over minutes but does not identify its
cause.

Authorised statement: in these 48 weeks, among non-probe transactions and at equal entity and
decile of position, the revert rate is 7.75 points higher (CI 6.56 to 9.28) when another entity
already swapped earlier in the block on one of its pools. Reading that as competition is an
inference; arrival time and fee also set the order inside a block.

## AF-03: the same tests on Unichain and OP Mainnet (12 weeks each)

Preregistration: DOI [10.5281/zenodo.23457231](https://doi.org/10.5281/zenodo.23457231), with an
OpenTimestamps proof. The 12 weeks per chain were drawn by hash before download; 14,112 blocks per
chain. `datos/tx_sistema.csv` excludes system and builder transactions. The intervals rest on only
12 week-units and are less reliable than AF-02's. The download cost US$0.91.

**H1, race (without probes).** The preregistration requires at least 1,000 cells to affirm or reject.

| Chain | n | Cells | Risk difference [95% CI] | Without entity control |
|---|---:|---:|---|---|
| Unichain | 10,378 | 169 | -0.0791 [-0.1482, +0.0088] | -0.1193 [-0.2351, +0.0540] |
| OP Mainnet | 5,375 | 301 | +0.1396 [+0.0986, +0.1887] | -0.0206 [-0.0921, +0.0509] |

Decision: **inconclusive in both chains** (169 and 301 cells, below 1,000). The OP estimate is
positive in 12 of 12 weeks; this report gives it as an observation and does not count it as an
affirmed result. In
Unichain, 9,246 of 10,378 transactions in the test universe are reverts, and 761 of them (7.3%)
follow a successful swap by another entity on their pools; in OP the share is 865 of 5,375
(16.1%). That leaves 169 cells with both groups, far from 1,000. One contract
receives 80.3% of those reverts. The preregistration has two rules that could both apply to the
Unichain interval (upper limit below 0.02, and fewer than 1,000 cells) and does not say which comes
first. The upper limit is +0.0088, below 0.02, so the falsification rule would apply on its own. The
author took the reading that declares nothing and fixed that precedence after seeing the results.
Three sensitivities that do not decide put the upper limit at -0.0092 (50 position bins), -0.0425
(hour bootstrap) and +0.0725 (cells by entity and block). The lack of a decision rests on the cell
count, which is the preregistered rule.

*Correction, 2026-10-20.* An earlier version of this file reported 31 cells and only 65 Unichain
transactions after another entity's swap (OP: 294 cells, risk difference +0.1390 [+0.0941, +0.1938]).
The analysis code did not read Uniswap v4 swaps as pools, so it missed most prior swaps. The
definition of the test did not change. The tables above use the corrected code on the same rows;
the decisions are the same, and H2 is unaffected.

**H2, synchrony bound.**

| Chain | U_2s [95% CI] | U_30s | U_5min | Pairs at 2 s |
|---|---|---:|---:|---:|
| Unichain | +0.0796 [+0.0561, +0.0933] | +0.0988 | +0.1121 | 19,671 |
| OP Mainnet | +0.0258 [+0.0167, +0.0353] | +0.0230 | +0.0389 | 56,260 |

Both upper limits are above the 0.02 threshold, so the study declares no bound. These numbers
are upper bounds of synchrony and do not give its value. In Unichain, U_2s depends on the pool definition: without the Uniswap v4
pseudo-pools it falls to +0.0104 [-0.0008, +0.0195].

**Comparison across chains (descriptive, no claim).**

| Chain | Weeks | Risk difference without probes | Cells | U_2s |
|---|---:|---|---:|---|
| Base (AF-02) | 48 | +0.0775 [+0.0656, +0.0928] | 8,635 | +0.0129 [+0.0113, +0.0144] |
| OP Mainnet (AF-03) | 12 | +0.1396 [+0.0986, +0.1887] | 301 | +0.0258 [+0.0167, +0.0353] |
| Unichain (AF-03) | 12 | -0.0791 [-0.1482, +0.0088] | 169 | +0.0796 [+0.0561, +0.0933] |

Base used AF-02's rule (no system-transaction exclusion, no v4 pseudo-pools), so the comparison
is not like for like. The author did not run the sensitivity that reruns Base with AF-03's rule.

Not claimed: that a state race exists in OP Mainnet or is larger than on Base; that Unichain has
no race; any cause (Flashblocks, fees, chain design); any operator identification.

## Cross-checks

For Study 1 the Python check reproduces the bot and entity universe, the observed overlap, the exact
mean of the null, and the excess E (for example 0.0512, 0.1458, 0.1056 and 0.1337 against 0.0510,
0.1460, 0.1060 and 0.1340 from the Rust null). For Studies 2 and 3 it reproduces the per-week
estimates and the neighbour curve. Intervals, p-values, pooled estimates and sensitivity tables
come from the Rust implementation only. The same author wrote both on the same rows
(`scripts/analisis/`); this is not an independent reproduction.
