# Results

All numbers are estimates over the sampled weeks of 2025 stated in each section. Intervals are 95%
bootstrap intervals (resampling whole weeks unless noted), seed 20261017. `p = 0.001` is the floor
with 1,000 permutations: no permutation matched the observed value. Nothing here is a statement
about other years, other chains or contracts outside the bot universe of each sample.

Terms: a **probe** is a successful transaction that emits no external logs and carries at most
1 wei; a **revert** is a transaction that reverted. An **entity** merges bot contracts that share
a sender. **Overlap** is the share of a failed attempt's pools (weighted by how few entities touch
each pool) that another entity also touched in the same block.

## v0.1.0 claim, retracted

Version 0.1.0 grouped attempts by bot and block, called dense groups "contention", and a figure of
about 90% was quoted publicly. That measured one bot repeating itself, not different bots
competing. With the corrected measure below, the overlap above a within-hour null is 0.05 to 0.15,
not 0.9. Running the v0.1.0 heuristic on the AF-01 weeks still marks 69% to 95% of failed
attempts as "contention", which is the size of the error.

## AF-01: overlap between entities on Base (4 weeks)

Weeks starting 2025-06-22, 2025-08-10, 2025-09-14 and 2025-12-21, about 800 blocks each, sampled
by UTC hour. Preregistered and sealed in a private repository before the confirmatory blocks were opened (`docs/04_investigacion_y_labs/`). The seal time is the author's own commit time; it has no external timestamp, so this is weaker evidence than AF-03's.

Statistic: weighted overlap CRw of failed attempts, against a null that reshuffles each entity's
bundles among blocks of the same hour. E = CRw observed minus null mean.

| Week | Entities | Failed attempts with pools | CRw observed [95% CI] | Null | E [95% CI] |
|---|---:|---:|---|---|---|
| S1 2025-06-22 | 116 | 47,289 | 0.765 [0.754, 0.776] | 0.714 | 0.051 [0.046, 0.056] |
| S2 2025-08-10 | 77 | 46,278 | 0.753 [0.738, 0.769] | 0.607 | 0.146 [0.136, 0.157] |
| S3 2025-09-14 | 83 | 112,255 | 0.817 [0.805, 0.830] | 0.712 | 0.106 [0.096, 0.115] |
| S4 2025-12-21 | 84 | 68,219 | 0.657 [0.626, 0.682] | 0.523 | 0.134 [0.123, 0.145] |

Holm-adjusted p = 0.004 in all four weeks.

What it supports: in these four weeks, failed attempts share more pool weight with another entity
in the same block than a within-hour reshuffle gives. The excess is 0.05 points in June and 0.11
to 0.15 in the other three weeks.

What it does not support, and why (from the review written after the run):
- It is not a fraction of failures caused by competition. E is an excess of overlap over a null.
- E mixes block-level synchrony with drift inside the hour (see AF-02, where synchrony is bounded
  at 1.4 points).
- With the binary overlap metric the same data give E of 0.011 to 0.022 (below the 0.05 threshold).
  The weighted metric was chosen on the exploratory half of the same weeks.
- In S3 the result depends on how contracts are merged into entities: without merging E is 0.034;
  ignoring senders that call 10 or more bot contracts it is 0.033.
- A day-level jackknife gives standard errors up to three times the hour bootstrap; in S1 the
  condition E >= 0.05 sits inside the error (0.051 +/- 0.015).

## AF-02: race inside the block and block synchrony on Base (48 weeks)

The 48 weeks of 2025 not used in AF-01, 504 anchor blocks per week, sampled by UTC hour.
Preregistered and sealed before download, privately (commit time only, no external timestamp).

**H1, race.** Among transactions from bot contracts that are not probes, the revert rate was
compared by whether another entity had already made a successful swap earlier in the block on one
of the same pools. Stratified by entity and by decile of position in the block (Mantel-Haenszel
risk difference).

| Variant | n | Cells | Risk difference [95% CI] | Without entity control [95% CI] |
|---|---:|---:|---|---|
| Without probes (primary) | 313,549 | 8,635 | +0.0775 [+0.0656, +0.0928] | -0.0298 [-0.0501, -0.0110] |
| With probes | 2,083,668 | 16,971 | +0.0089 [+0.0075, +0.0103] | -0.0271 [-0.0395, -0.0159] |

The difference is positive in 48 of 48 weeks (lower bound above 0 in 41). Preregistered
sensitivities: 50 position bins +0.0741; cells (entity, block) +0.0651; hour bootstrap
+0.0775 [+0.0716, +0.0827]. The primary result met the preregistered criterion (risk difference
at least 0.02 with an interval excluding 0). Without controlling for entity the sign flips
(Simpson's paradox); the cause was not decomposed. With probes the effect is a quarter of the
threshold, so the claim does not extend to probes. A sensitivity excluding senders with 10 or
more bot contracts was not run for this study.

**H2, block synchrony bound.** Overlap in the anchor block against the block 2 s later, 30 s later
and 5 min later, same anchors.

| Distance | Pairs | U_k [95% CI] |
|---|---:|---|
| 2 s | 1,318,574 | +0.0129 [+0.0113, +0.0144] |
| 30 s | 660,536 | +0.0295 [+0.0268, +0.0322] |
| 5 min | 660,536 | +0.0863 [+0.0717, +0.1004] |

U_2s is an upper bound of block-level synchrony, not an estimate of it. Preregistered criterion
(upper limit below 0.02) met: block synchrony is not larger than 2 points on Base in these weeks.
The curve grows with distance, which is consistent with drift over minutes and does not identify
its cause.

Authorised statement: in these 48 weeks, among non-probe transactions and at equal entity and
decile of position, the revert rate is 7.75 points higher (CI 6.56 to 9.28) when another entity
already swapped earlier in the block on one of its pools. Reading that as competition is an
inference; arrival time and fee also set the order inside a block.

## AF-03: the same tests on Unichain and OP Mainnet (12 weeks each)

Preregistration: DOI [10.5281/zenodo.23457231](https://doi.org/10.5281/zenodo.23457231), with an
OpenTimestamps proof. 12 weeks per chain drawn by hash before download; 14,112 blocks per chain.
System and builder transactions are excluded by `datos/tx_sistema.csv`. Intervals have only 12
week-units and are less reliable than AF-02's. Cost of the download: US$0.91.

**H1, race (without probes).** Preregistration requires at least 1,000 cells to affirm or reject.

| Chain | n | Cells | Risk difference [95% CI] | Without entity control |
|---|---:|---:|---|---|
| Unichain | 10,378 | 31 | -0.0450 [-0.1175, +0.0184] | -0.5078 [-0.6132, -0.3424] |
| OP Mainnet | 5,375 | 294 | +0.1390 [+0.0941, +0.1938] | -0.0211 [-0.0980, +0.0570] |

Decision: **inconclusive in both chains** (31 and 294 cells, below 1,000). The OP estimate is
positive in 12 of 12 weeks and is reported as an observation, not as an affirmed result. In
Unichain, 9,246 of 10,378 transactions in the test universe are reverts, but only 65 follow a
successful swap by another entity on their pools, so there is nothing to compare. One contract
receives 80.3% of those reverts. The preregistration has two rules that could both apply to the
Unichain interval (upper limit below 0.02, and fewer than 1,000 cells) and does not order them;
the reading that declares nothing was taken, and that precedence was fixed after seeing the
results.

**H2, synchrony bound.**

| Chain | U_2s [95% CI] | U_30s | U_5min | Pairs at 2 s |
|---|---|---:|---:|---:|
| Unichain | +0.0796 [+0.0561, +0.0933] | +0.0988 | +0.1121 | 19,671 |
| OP Mainnet | +0.0258 [+0.0167, +0.0353] | +0.0230 | +0.0389 | 56,260 |

Both upper limits are above the 0.02 threshold, so no bound is declared; these are upper bounds of
synchrony, not its value. In Unichain U_2s depends on the pool definition: without the Uniswap v4
pseudo-pools it falls to +0.0104 [-0.0008, +0.0195].

**Comparison across chains (descriptive, no claim).**

| Chain | Weeks | Risk difference without probes | Cells | U_2s |
|---|---:|---|---:|---|
| Base (AF-02) | 48 | +0.0775 [+0.0656, +0.0928] | 8,635 | +0.0129 [+0.0113, +0.0144] |
| OP Mainnet (AF-03) | 12 | +0.1390 [+0.0941, +0.1938] | 294 | +0.0258 [+0.0167, +0.0353] |
| Unichain (AF-03) | 12 | -0.0450 [-0.1175, +0.0184] | 31 | +0.0796 [+0.0561, +0.0933] |

Base was run with AF-02's rule (no system-transaction exclusion, no v4 pseudo-pools), so the
comparison is not like for like. The sensitivity that reruns Base with AF-03's rule was not run.

Not claimed: that a state race exists in OP Mainnet or is larger than on Base; that Unichain has
no race; any cause (Flashblocks, fees, chain design); any operator identification.

## Cross-checks

For each study the Rust estimators were reimplemented in Python by the same author on the same
rows (`scripts/analisis/`). Point estimates agree to the printed decimals; confidence intervals
and p-values differ only through the Monte Carlo generator. This is not an independent
reproduction.
