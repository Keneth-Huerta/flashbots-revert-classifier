# Results

Every number is an estimate over the sampled weeks (2024, 2025 or 2026) that each section states. Intervals are 95%
bootstrap intervals (resampling whole weeks; whole hours in AF-01), seed 20261017. With 1,000
permutations, `p = 0.001` is the floor: no permutation matched the observed value. These results
say nothing about other weeks, other chains, or contracts outside each sample's bot universe.

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
in the same block than a within-hour reshuffle gives. The excess is 0.05 in June and 0.11
to 0.15 in the other three weeks (a difference of proportions).

It does not support the following (from the review the author wrote after the run):
- E is an excess of overlap over a null. It is not a fraction of failures caused by competition.
- E mixes block-level synchrony with drift inside the hour (see AF-02, where the synchrony bound
  is 1.3 or 2.8 points, upper limits 1.4 and 3.4, depending on how Uniswap v4 pools are counted).
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
correlation r = -0.38; this comparison is post hoc). The two studies agree in sign, not in time trend, and the trend was not
modelled. E shrinks with fewer blocks per hour, so these values are not comparable to Study 1's.

**H2, block synchrony bound.** Overlap in the anchor block against the block 2 s later, 30 s later
and 5 min later, same anchors.

| Distance | Pairs | U_k [95% CI] |
|---|---:|---|
| 2 s | 1,318,574 | +0.0129 [+0.0113, +0.0144] |
| 30 s | 660,536 | +0.0295 [+0.0268, +0.0322] |
| 5 min | 660,536 | +0.0863 [+0.0717, +0.1004] |

U_2s is an upper bound of block-level synchrony; it does not estimate it. Under this study's pool
rule (the Uniswap v4 `PoolManager` counts as one pool) the preregistered criterion (upper limit
below 0.02) is met. The curve grows with distance. That fits drift over minutes but does not
identify its cause.

**Correction (2026-10-20).** An earlier version of this file said that "on Base in these weeks,
block synchrony is at most 2 points". That holds only under the pool rule above. AF-04 (test T10)
re-extracted the same traces with v4 pseudo-pools and system transactions excluded:

| Distance | Pairs | U_k [95% CI], v4 pseudo-pools |
|---|---:|---|
| 2 s | 1,309,691 | +0.0276 [+0.0224, +0.0335] |
| 30 s | 655,484 | +0.0479 [+0.0408, +0.0556] |
| 5 min | 655,484 | +0.1039 [+0.0852, +0.1232] |

The upper limit at 2 s is above 0.02, so no 2-point bound is claimed. Excluding system
transactions changes no printed digit; the change comes from telling v4 pools apart. With the
`PoolManager` as one pool, two entities on different v4 pools count as sharing a pool in the anchor
and in the neighbour, which shrinks U_k. The race estimate under the same rule is in AF-04.

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
is not like for like. Under AF-03's rule (AF-04, T10) Base gives a risk difference of +0.0967
[+0.0842, +0.1111] and U_2s of +0.0276 [+0.0224, +0.0335].

Not claimed: that a state race exists in OP Mainnet or is larger than on Base; that Unichain has
no race; any cause (Flashblocks, fees, chain design); any operator identification.

## AF-04: fresh weeks, placebos and robustness on Base

24 weeks of 2024 and 24 of 2026 drawn by hash, none seen before, 504 anchor blocks per week
(24,192 blocks, no download failures). The preregistration was archived on Zenodo
(DOI 10.5281/zenodo.23492680, 2026-10-20 07:29:22 UTC) before the download started (07:29:55 UTC).
Only the fresh weeks decide the joint reading. Each quantity is **active** if the estimate is at
least 0.02 and the interval excludes 0, **null** if the upper limit is below 0.02, and
**inconclusive** otherwise.

**Tests that decide** (non-probe transactions, risk difference by entity and position decile):

| Test | Exposure | Cells | Estimate [95% CI] | Outcome |
|---|---|---:|---|---|
| T0 | prior swap by another entity, own pools | 7,459 | +0.1357 [+0.1137, +0.1579] | active |
| T0, 2024 | same | 1,431 | +0.1843 [+0.1518, +0.2280] | active |
| T0, 2026 | same | 6,028 | +0.1264 [+0.1033, +0.1495] | active |
| T1 | later swap by another entity, own pools | 6,976 | +0.0109 [-0.0011, +0.0222] | inconclusive |
| T2 | prior swap on a donor's pools | 8,262 | +0.0014 [-0.0048, +0.0072] | null |
| T2 | donor subset minus placebo | | +0.1320 [+0.1096, +0.1551] | discriminating |

Joint reading, by the table fixed in the preregistration: **inconclusive**. The race replicates
out of sample and in each year, in sign and class; the size differs (the 2024 interval lies above
the 2025 one, and 2026 holds most of the cells). A prior swap on pools the transaction does not touch changes
nothing, which a shock to the whole block or residual position would not give. A later swap on
its own pools adds 1.1 points with an upper limit of 2.2, so the test cannot set a race for state
apart from a shock common to the pool.

Other results on the fresh weeks (they refine the wording and do not change the class):
- A prior swap by the **same** entity lowers the revert rate: -0.1182 [-0.1444, -0.0886]. The
  author did not predict it and does not explain it.
- Swaps by users who are not bots raise the rate more than swaps by other bots (+0.2224 against
  +0.1357; in 2025 +0.1188 against +0.0775); the race among bots holds within cells split by user
  swaps (+0.1412 [+0.1186, +0.1651]).
- Selection by the pool set is not ruled out. Pools fixed per contract from its successful swaps,
  which do not depend on the outcome but pull toward zero through measurement error, give +0.0299
  [+0.0149, +0.0453]; the interval contains 0.02, so this check is inconclusive, and in 2025 it is
  null (-0.0043 [-0.0134, +0.0037]). Strata by pool-set size give +0.1310; the least favourable
  assignment of transactions with no pools, over a grid that is a practical bound, gives +0.0957
  [+0.0693, +0.1276].
- Prediction fixed in advance (2026 random-effects estimate below 0.115): inconclusive, +0.1321
  [+0.1105, +0.1537].
- With v4 pseudo-pools on the 2026 weeks every deciding test keeps its class.

**On the 48 weeks of 2025 (AF-02):**
- Bot threshold: the race falls as the definition tightens, +0.2327 with 0.25 failed share,
  +0.0775 with 0.5 and +0.0082 [-0.0029, +0.0193], null, with 0.75. The result holds for the bot
  definition used here and not for a stricter one.
- Entity definition: stable. Ignoring senders that call 10 or more bot contracts gives +0.0775;
  leaving out each of the five largest entities gives +0.0729 to +0.0998.
- Sign reversal without entity control: composition between entities accounts for 52% of it.
- AF-03's pool rule: the race is +0.0967 [+0.0842, +0.1111], a change of +0.0192; by the
  preregistered rule it does not change. The synchrony bound does change (see the correction in
  AF-02).

Authorised statement: in 48 fresh weeks of Base (2024 and 2026), among non-probe transactions of
bot contracts and at equal entity and decile of position, the revert rate is 13.6 points higher
(CI 11.4 to 15.8) when another entity already swapped earlier in the block on one of its pools. A
prior swap on pools the transaction does not touch changes nothing (+0.1, CI -0.5 to +0.7). A later
swap on its pools adds 1.1 points (CI -0.1 to +2.2) that cannot be called zero.

Not claimed: that the race for state is reinforced; that a shock common to the pool is ruled out;
any cause; why 2024, 2025 and 2026 differ.

## Cross-checks

For Study 1 the Python check reproduces the bot and entity universe, the observed overlap, the exact
mean of the null, and the excess E (for example 0.0512, 0.1458, 0.1056 and 0.1337 against 0.0510,
0.1460, 0.1060 and 0.1340 from the Rust null). For Studies 2 and 3 it reproduces the per-week
estimates and the neighbour curve. Intervals, p-values, pooled estimates and sensitivity tables
come from the Rust implementation only. For AF-04 the Python check reproduces counts, rates and
point estimates, per week and pooled, of 23 variants on the fresh weeks and the 121 points of the
selection grid; it does not cover 50 position bins, entity-block cells, intervals, p-values, leave-one-out,
heterogeneity or the system-transaction filter. The decomposition terms (T8) and the v4 pool reading were
compared with Python by hand on one week before the seal; on the full data they rest on the Rust code. The same author wrote both on the same rows
(`scripts/analisis/`); this is not an independent reproduction.

## Data

The extracted rows behind every section, with the sha256 of each raw capture, are on Zenodo:
DOI [10.5281/zenodo.23549452](https://doi.org/10.5281/zenodo.23549452) (CC BY 4.0). Each
uncompressed file has the hash of the rows the primary analyses above read. The re-extractions with
Uniswap v4 pools counted separately (S-v4 and T10 in AF-04) are not included.
