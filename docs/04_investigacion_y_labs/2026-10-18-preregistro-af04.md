---
estado: INMUTABLE
categoria_documental: PREREGISTRO
id_afirmacion: AF-04
estado_afirmacion: PREREGISTRADA
fecha_registro: 2026-10-18
---

# Preregistration: robustness of the within-block race test on Base

Author: Keneth Issac Huerta Galindo. First drafted 2026-10-18, revised 2026-10-20 before sealing.

This document fixes the hypotheses, thresholds, sampling rule, analyses, sensitivities and falsification criteria of a robustness study of one earlier result. The revision of 2026-10-20 followed an internal adversarial review and came before any block of the drawn weeks existed on disk. It made the joint reading rule total, fixed which estimate of each test decides, added one sensitivity that never decides, and corrected the account of what the tool enforces; it moved no threshold. I wrote the document before downloading any block of the drawn weeks and before computing any placebo statistic on the fresh weeks or on the 48 weeks of 2025. Placebo-type statistics were printed on one already-open week of the first study, only to check that the code runs; "Evidence observed before this registration" lists them. The study is partly retrospective. I saw the earlier result, its weekly series and several of its sensitivities before writing this (see "Evidence observed before this registration"). Only the tests on 48 freshly drawn weeks decide the joint reading; tests on the 48 weeks already seen are reported and do not decide it.

## Question and scope

- Earlier result: on Base, in 48 weeks of 2025, a bot transaction reverts more often when another entity already made a successful swap earlier in the same block on a pool the transaction touches, at equal entity and equal decile of position in the block. The Mantel-Haenszel risk difference, variant without probes, was +0.0775 (95% CI [+0.0656, +0.0928], bootstrap of whole weeks), positive in 48 of 48 weeks. The same estimator with deciles only, without the entity, was -0.0298.
- Question: does that association behave as a race for pool state ("a swap earlier in the block changes the pool and the later transaction fails because of it"), or does it behave as one of three rival explanations? Does it survive other choices for the universe, the pools, the bot threshold and the entities are defined? Why does the sign flip when the entity is not conditioned on? Is the effect the same in every week?
- Type of claim: every quantity here is an estimate of observed scope, on Base, in the weeks listed below. "Race for state" is an interpretation: arrival time and fee also set the order inside a block, so no test here identifies causation. The tests discriminate between explanations; they do not prove one.
- Population: bot transactions in Base blocks, in the weeks drawn below (fresh weeks, which decide) and in the 48 weeks already seen (concordance and descriptions, which do not decide the joint reading).
- Out of scope: identifying operators; other chains; any claim about why 2024 and 2026 differ (they are described, not explained).

## Definitions

These are the definitions of the earlier study, unchanged unless a test below says so.

- A **probe** is a successful transaction of a bot that emits no logs from foreign contracts and carries at most 1 wei of value. A **revert** is a transaction of a bot with `success = false`. A **bot** is a contract that, within a week, has at least 5 failed attempts with pools and whose transactions are at least 50% failed attempts. **Entities** are the connected components of the contract-sender graph, by week.
- **Universe:** transactions of bot contracts that are not probes (variant "without probes", the one that decides) and whose set of pools P(t) is not empty. P(t) comes from the call trace of the transaction itself (`debug_traceBlockByNumber`, `callTracer`) plus the pools that emit a Swap log in its receipt. Calldata is not used to build P(t).
- **Position:** decile of `tx_index / block length`, where block length counts every transaction of the block.
- **Cell:** (week, entity, decile). **Estimator:** the Mantel-Haenszel risk difference of the revert rate, Δ = Σ w_c d_c / Σ w_c, with d_c = rate(Z=1) − rate(Z=0) and w_c = n₁n₀/(n₁+n₀), over cells with both groups. Cells are nested in weeks, so the pooled estimate is Σ over weeks of the numerators divided by Σ of the weights.
- **X** (exposure of the earlier study): X(t) = 1 if some other entity had a successful swap, at a lower `tx_index` in the same block, on a pool of P(t).
- **Three-outcome rule (R3)** for a quantity with estimate θ̂ and 95% interval [ℓ, u] from the bootstrap of whole weeks (1,000 resamples, seed 20261017), threshold τ = 0.02:
  - **active** if θ̂ ≥ τ and ℓ > 0;
  - **null** if u < τ;
  - **inconclusive** otherwise.
- Thresholds (0.02 and the others below) are copied from the earlier study or fixed here. None was set or moved after seeing a placebo statistic, and none is adjusted afterwards.

## Rival explanations

The explanation under test is **R-S, race for state**: a swap earlier in the block changes a pool's state, and a later transaction on that pool fails because of it. Only what happens earlier in the order, on a pool of P(t), can act.

- **R-C, common pool shock:** an unobserved movement (price, opportunity) in a pool raises both the swaps of others on that pool (before or after) and the reverts. Order does not matter.
- **R-B, common block shock:** an unobserved shock raises the activity of every pool in the block and the reverts.
- **R-O, residual position:** the order is not random; late transactions revert more and have more earlier swaps, and the decile does not absorb the fine position.
- **R-U, state consumed by any swap:** a subtype of R-S where swaps by non-bot users also count. Its opposite is a race specific to bots.

Predicted sign of each quantity under each explanation ("+" means active, "≈0" means null, "≤0" means null or negative):

| Quantity | R-S | R-C | R-B | R-O |
|---|---|---|---|---|
| Δ (primary) | + | + | + | + |
| Δ_T (swaps by others later in the block) | ≤0 | + | + | ≤0 |
| Δ_P (swaps before, in the pools of a donor transaction) | ≈0 | ≈0 | + | + |
| Δ_don − Δ_P | = Δ_don (+) | = Δ_don (+) | ≈0 | ≈0 |

A temporal placebo that comes out null or negative does not separate R-S from R-O; only an active temporal placebo falsifies R-S.

## Data and sampling

- **Fresh weeks (decide).** Base calendar weeks starting on a Sunday, UTC. Eligible in 2024: 2024-03-17 to 2024-12-29, after the March 2024 Dencun and Ecotone upgrades (42 weeks). Eligible in 2026: 2026-01-04 to 2026-10-04 (40 weeks). For each year the 24 weeks with the lowest `sha256("mev-af04-v1:<year>:<week_start>")` are taken, sorted by date and numbered F24-01 to F24-24 and F26-01 to F26-24. No week overlaps the 48 weeks of 2025 of the earlier study or the four weeks of its first study. The week of 2026-10-11 is not eligible.
  - 2024 (24 of 42): F24-01 2024-03-17, F24-02 2024-03-31, F24-03 2024-04-07, F24-04 2024-04-14, F24-05 2024-04-21, F24-06 2024-05-19, F24-07 2024-05-26, F24-08 2024-06-02, F24-09 2024-06-16, F24-10 2024-06-23, F24-11 2024-07-14, F24-12 2024-07-21, F24-13 2024-08-04, F24-14 2024-08-18, F24-15 2024-08-25, F24-16 2024-09-01, F24-17 2024-09-22, F24-18 2024-09-29, F24-19 2024-10-06, F24-20 2024-10-20, F24-21 2024-11-24, F24-22 2024-12-01, F24-23 2024-12-15, F24-24 2024-12-22.
  - 2026 (24 of 40): F26-01 2026-01-18, F26-02 2026-01-25, F26-03 2026-02-08, F26-04 2026-02-22, F26-05 2026-03-01, F26-06 2026-03-08, F26-07 2026-03-15, F26-08 2026-03-22, F26-09 2026-04-05, F26-10 2026-04-26, F26-11 2026-05-10, F26-12 2026-05-31, F26-13 2026-06-14, F26-14 2026-07-05, F26-15 2026-07-19, F26-16 2026-07-26, F26-17 2026-08-02, F26-18 2026-08-16, F26-19 2026-08-23, F26-20 2026-08-30, F26-21 2026-09-06, F26-22 2026-09-20, F26-23 2026-09-27, F26-24 2026-10-04.
- **Anchor blocks only.** Per week and per UTC hour, the 3 blocks with the lowest `sha256("mev-contention-v1:<block>")`, taken with the neighbor design of the earlier study and keeping only the anchors: 504 blocks per week, 24,192 blocks in total. Base block number is `(timestamp − 1686789347) / 2`. The file of anchors (`muestra_bloques_af04_anclas.csv`) has sha256 `e5a935c531c4b818039c956f4a6606b79de966cc334a5ed1255ccd09a3729c3a`. All rows are marked confirmatory.
- **Weeks already seen.** The 48 anchor weeks of 2025 of the earlier study (`muestra_bloques_af02_anclas.csv`, sha256 `78a2f5338398c0edbef78134ac4ded716f03272163e96b4c64e61d944bb43c9b`, 24,192 blocks) and their neighbor file for the synchrony bound (`muestra_bloques_af02.csv`, sha256 `776f9f9ca9f38d2ec0488dba7f8ade159013ae283103e2fdca542a5a1cf4a958`). Their rows and receipts are the ones produced for the earlier study.
- **Extraction on fresh weeks** uses the rule of the earlier Base study: pools by address (no Uniswap v4 pseudo-pools) and no exclusion of system transactions. Test T10 changes both on the weeks already seen. Uniswap v4 exists on Base in 2026 and not in 2024, so on the 2026 weeks the v4 PoolManager is read as one single pool; the sensitivity S-v4 below repeats part of the analysis with pseudo-pools, and this remains an open risk (see Limits).
- **Chain clock and system list.** Base blocks of 2 s. The system-transaction list (`tx_sistema.csv`, sha256 `f7dfb94922bd6574ffd67e38bbbfdb5431736c8854ed63fca87d6407a836d60e`) is used only in T10.
- Data source: JSON-RPC of a commercial provider (Base mainnet), `debug_traceBlockByNumber` with `callTracer` and `eth_getBlockReceipts`, through the study's command-line tool (`mev-revert-classifier`, version 0.8.0, commit declared at sealing).

## Tests

All tests use `race --boot-unit week --bootstrap 1000 --seed 20261017`, variant without probes (`--sin-probes-only`), and R3 on the pooled estimate. A test is **inconclusive** in addition if its interval contains 0.02, if it has fewer than 1,000 cells with both groups, or if more than 5% of the sampled blocks have no raw data. "Fresh" means the 48 fresh weeks; "Seen" means the 48 weeks of 2025 already seen. In the commands, `<rows>`, `<raw>` and `<anchors>` stand for the extracted rows, the raw captures and the sample of the data set under analysis.

**T0. Gate: the primary test on fresh weeks.**
- Estimand: Δ on Fresh. Command: `race --rows <rows> --raw <raw> --blocks <anchors> --split confirmatory --boot-unit week --bootstrap 1000 --permutations 1000 --seed 20261017 --sin-probes-only`.
- Every rival predicts an active Δ, so T0 only says whether the earlier finding replicates outside the sample seen.
- T0 is also reported by year (F24 and F26). If the two years fall in different classes, the verdict says so; the class of the pooled estimate stays.
- If T0 is null (upper limit < 0.02), the earlier claim is falsified out of sample and T1 to T5 are reported without a reading. If T0 is inconclusive, T1 to T5 are reported as descriptions. Only with T0 active are T1 to T5 read.

**T1. Temporal placebo** (a swap by another entity LATER in the block, on a pool of P(t)).
- Primary estimand: Δ_T, `--x-def later-other`, with 10 position bins and cells (entity, bin). Sensitivities, which never decide: Δ_{T|X}, with cells also split by X (`--x-also prior-other`); 50 position bins (`--bins 50`); cells (entity, block) (`--cell-block`).
- R-S predicts Δ_T null or negative; R-C and R-B predict it active; R-O predicts ≤ 0.
- **Falsifies R-S: Δ_T active.** Reinforces R-S against R-C and R-B: Δ_T null (with the warning that it does not exclude R-O). Null by R3 only asks u < 0.02, so a negative Δ_T whose interval excludes 0 also counts as null for the joint reading; the verdict then states the sign and that a negative value is also compatible with R-O.

**T2. Pool placebo** (the exposure is computed with the pools of a donor transaction: same block, a non-probe bot transaction of another entity, with no pool in common with t, and the closest |P|; ties and the final choice by `sha256("<donor_seed>:<block>:<hash of t>:<hash of candidate>")`, lowest wins; X_P(t) = 1 if some pool of the donor had an earlier swap by an entity other than that of t).
- Primary estimands: Δ_P (`--x-def donor-pool`, donor seed 1) and Δ_don, the primary exposure on the same subset of transactions that have a donor (`--x-def prior-other --donor-subset`); contrast Δ_don − Δ_P with an interval from the same resamples of weeks (`race-contrast` over the files written by `--emit-weeks`). Donor coverage is the share of universe transactions with a donor. Sensitivity, which never decides: donor seeds 2 to 5.
- R-S and R-C predict Δ_P null; R-B and R-O predict it active.
- **Falsifies R-S: Δ_P active.** A pool-specific component exists if the lower limit of Δ_don − Δ_P is ≥ 0.02. For the joint reading T2 takes exactly one of four outcomes, from the primary run:
  - **active:** Δ_P active by R3.
  - **null and discriminating:** Δ_P null by R3 and the lower limit of Δ_don − Δ_P ≥ 0.02. The donor quantity is active, so a null Δ_P is informative.
  - **null and vacuous:** Δ_P null by R3 and the lower limit of Δ_don − Δ_P < 0.02. Then Δ_don itself is not shown to be active on this subset, every explanation predicts about 0 for Δ_P, and the null discriminates nothing.
  - **inconclusive:** Δ_P inconclusive by R3, or donor coverage below 50% of universe transactions (the 50% is an assumption fixed without data, with no prior estimate of coverage behind it), or the general inconclusive conditions above.
- A null Δ_P includes a negative estimate whose interval excludes 0. No rival named here predicts a negative Δ_P; the verdict reports it as an anomaly (the donor rule may correlate with position in the block) and the outcome is unchanged.
- Consequence of coverage: if donor coverage is below 50%, T2 is inconclusive and Reinforced cannot be reached; the class is Weakened if T1 is active and Inconclusive otherwise. That is a limit of the design and not evidence against R-S.

**T3. Entity placebo** (a swap by the SAME entity, another transaction, earlier in the block on a pool of P(t)).
- Estimands: Δ_E (`--x-def prior-same`); conditioned versions Δ_{X|E} (`--x-def prior-other --x-also prior-same`) and Δ_{E|X} (`--x-def prior-same --x-also prior-other`); contrast Δ − Δ_E with joint interval.
- This is not a null under R-S. It separates competition between entities from repetition by one entity.
- Wording, one line per outcome of Δ_E: **null** with the lower limit of Δ − Δ_E ≥ 0.02: the effect is specific to another entity, and the phrase "between bots" stands. **Active:** a previous swap by the same entity also raises the revert rate; the phrase "competition between bots" loses support and "own repetition" gains, and the text says so. **Null** with the lower limit of Δ − Δ_E < 0.02, or **inconclusive:** "between bots" is neither supported nor refuted and the text says it is unresolved. T3 does not change the verdict on Δ.

**T4. Exposure that includes swaps by users** (successful swaps by transactions that are not bot transactions, from senders outside the entity of t).
- Estimands: Δ_{X|U}, the race among bots within cells (entity, decile, X_U) (`--x-def prior-other --x-also user-prior`); Δ_U (`--x-def user-prior`); Δ_all with X_all = max(X, X_U) (`--x-def any-prior`).
- R-S among bots predicts Δ_{X|U} active and Δ_U null. R-U predicts Δ_U active. Wording by outcome of Δ_{X|U}: **active** with Δ_U null, the race among bots stands and swaps by users do not add to it; **active** with Δ_U active, the race among bots stands and swaps by users also raise reverts (R-U gains); **active** with Δ_U inconclusive, the race among bots stands and the role of users is unresolved; **null**, whatever Δ_U is: the earlier effect was an effect of user swaps misclassified in X = 0, and the text says so; **inconclusive:** the wording is not changed and the text says it is unresolved.

**T5. Selection by P(t) and by its size.** Reverts that touch no pool leave the universe (P empty). In a sample of the first study, 13.1% of reverts had empty P against 0.7% of successes, and the mean |P| was 1.64 against 3.09 (diagnostic, not a result); the net sign of the bias is unknown.
- (a) Grid: a fraction q_R of the bot transactions with empty P that are reverts, and a fraction q_S of those that are successes, are assigned X = 1, within cells; Δ(q_R, q_S) over {0, 0.1, …, 1}² (`--missing-grid`). The minimum and maximum over the grid and the tipping point q_R* (smallest q_R for which, at the least favorable q_S, Δ < 0.02) are reported. Because the weight w_c is not linear in the counts, the grid is a practical bound, not an exact one.
- (b) Pools independent of the outcome: for each contract and week, P_c are the pools with at least 10% of that contract's successful swap incidences that week, at most 5, ties by address ascending; the same P_c is used for every transaction of the contract, success, revert or probe, including those with empty trace P (`--p-rule contract`). Comparison on the common subset: `--p-rule contract-subset` and `--p-rule trace-on-contract-subset`. P_c has measurement error and tends towards the null; a null Δ_c is not evidence of absence.
- (c) Control by size of P: cells (entity, decile, min(|P|, 3)) (`--p-size-strata`).
- **Robust to selection** if (b) and (c) are active by R3. **Sensitive** if either is null. Because (b) tends towards the null through measurement error, a null (b) is read as "not robust by this check" and not as evidence of selection; the estimates of `contract-subset` and `trace-on-contract-subset` are printed next to it so that the attenuation is visible. The least favorable corner of (a) is reported with an interval from the bootstrap of weeks at that corner: if it is active by R3 the result is robust to any assignment of empty-P transactions; if not, q_R* is reported and the result is bounded, not robust. The status of T5 is written next to the class of the joint reading and does not change it.

**T6. Bot threshold.** Δ with minimum failed share 0.25 and 0.75 (`--bot-share`; the earlier study used 0.5). Stable if both are active, sensitive if either is null, inconclusive otherwise. Decides on the seen weeks (it is a sensitivity of that estimate); reported on Fresh.

**T7. Entity definition.** Δ with `--max-sender-degree 10` (senders calling 10 or more distinct bot contracts are ignored when forming entities), and leave-one-out of the 5 entity keys with the largest Mantel-Haenszel weight (`--loo 5`; the key of an entity-week is its bot contract with the most transactions). Stable if all six are active, sensitive if any is null. The share of the weight of the 5 largest keys and the Herfindahl index are reported. Decides on the seen weeks; reported on Fresh.

**T8. Decomposition of the sign flip** (seen weeks only). Exact identity Δ_dec − Δ_MH = C + W + S: composition between entities (C), weighting (W) and support, the cells with one group only (S); `--decompose`. Let G = Δ_dec − Δ_MH.
- Composition dominates if C/G ≥ 0.5 and the interval of C excludes 0; support dominates if S/G ≥ 0.5 and its interval excludes 0; the same for weighting; otherwise "mixed". Inconclusive if |G| < 0.02. This is descriptive accounting (no cause). If more than one term qualifies, the result is "mixed". Also reported: the number and Mantel-Haenszel weight of entity-weeks (at least 30 transactions per group) with positive, negative and zero effect.

**T9. Heterogeneity between weeks.**
- Per-week estimate and its variance (bootstrap of hours within the week, 1,000 resamples); DerSimonian-Laird τ̂², I², random-effects estimate with its 95% interval (estimate ± 1.96 standard errors), 95% prediction interval, weighted least squares slope and Spearman correlation against the week index (position of the week in date order) (`--het --het-groups F24,F26`).
- Retrospective on the seen weeks: the weekly series, the trend (Spearman −0.64) and the means 0.115 (first 24 weeks) and 0.062 (last 24 weeks) were seen. They are reported and not confirmed.
- **Only prospective prediction (declared now), evaluated only if T0 is active:** the random-effects Δ of the 24 fresh weeks of 2026 is **below 0.115**. Confirmed if the upper limit of its interval is < 0.115; falsified if the lower limit is ≥ 0.115; inconclusive otherwise. If T0 is null or inconclusive the prediction is not evaluated. It is a prediction of low severity: the last 24 weeks of the seen series already averaged 0.062, so it is likely to hold whenever the earlier finding replicates at all. The two fresh years are also described separately, without a prediction for 2024.

**T10. Base with the rule of the later chain study** (seen weeks only). Rows re-extracted offline from the stored traces with Uniswap v4 pseudo-pools (`fetch --offline --chain base --v4-pools`) and system transactions excluded (`--exclude-system true`); then `race --chain base --exclude-system true --v4-swap-pools`. The option `--v4-swap-pools` is new in this version: in the earlier code the pools of a successful swap come from the addresses of Swap logs, so with v4 rows the PoolManager address never matches a `v4:<PoolId>` pseudo-pool and no v4 swap acted as an earlier swap. With the option, a PoolManager Swap log contributes `v4:<topics[1]>`. Without the option the earlier code and its output are unchanged.
- Estimands: Δ_v4, and the contrast Δ_v4 − Δ with joint interval (`race-contrast` on the files of both runs); the synchrony bound U₂ₛ with the same rows (`neighbor --chain base --exclude-system true --boot-unit week`).
- The race **does not change** if Δ_v4 is active and |Δ_v4 − Δ| < 0.02 or its interval contains 0. It **changes** if Δ_v4 is null, or |Δ_v4 − Δ| ≥ 0.02 with an interval that excludes 0. In every other case (Δ_v4 inconclusive without a contrast that excludes 0) the result is inconclusive. U₂ₛ is judged with the earlier rule (upper limit < 0.02).
- T10 changes two things at once (pseudo-pools and the exclusion of system transactions). To attribute a change, `race --chain base --v4-swap-pools` without `--exclude-system` is also run on the same re-extracted rows, which costs nothing; it is reported and does not decide.
- If a published figure of the earlier study moves, that is recorded as an erratum with a new analysis; the earlier document is not edited.

**S-v4. Sensitivity with v4 pseudo-pools on the fresh weeks of 2026** (never decides).
- The 24 fresh weeks of 2026 are extracted with the sealed rule (pools by address), and that extraction is the primary one. For this sensitivity the rows of the same weeks are re-extracted offline from the same raw captures with `fetch --offline --chain base --v4-pools` (no request, no cost), over the sealed sample filtered to the F26 strata, and `race` is run with `--v4-swap-pools` for T0, T1 (Δ_T) and T2 (Δ_P, Δ_don and the contrast), with the commands of those tests.
- It never changes the class of the joint reading. If its class differs from that of the primary, the verdict says so and the class stays as the primary gives it.
- Open risk, declared: on 2026 the primary reads the v4 PoolManager as one single pool. Swaps in different v4 pools then count as swaps in the same pool, which can inflate X, Δ_T and Δ_P, and the donor rule discards pairs of transactions that both touch v4. The size of this effect on the 2026 weeks is not known; S-v4 measures it and does not remove it from the primary. 2024 predates v4 on Base.

### Where each test decides

| Test | Fresh weeks | Seen weeks (retrospective) |
|---|---|---|
| T0 | decides | already evaluated |
| T1 to T5 | decide, by the primary of each test | concordance |
| T6, T7 | reported | decide (sensitivities of the earlier estimate) |
| T8, T9 (description), T10 | none | decide (describe the earlier estimate) |
| S-v4 | reported, never decides | none |
| T9 (prospective prediction) | decides | none |

### Joint reading rule

Fixed before any computation, using only the fresh weeks and only the primary of each test: T0 (Δ), T1 (Δ_T with 10 bins and cells (entity, bin)) and T2 (Δ_P with donor seed 1, with the contrast Δ_don − Δ_P). Sensitivities and variants (50 bins, cells (entity, block), Δ_{T|X}, donor seeds 2 to 5, S-v4, T5 to T7, T10) never decide. If a sensitivity falls in a different class than its primary, the verdict says so and the class stays as the primary gives it.

T0 and T1 each take one of three outcomes (active, null, inconclusive, by R3; a negative placebo with u < 0.02 is null). T2 takes one of four (active, null and discriminating, null and vacuous, inconclusive; defined in T2). The table covers all 3 × 3 × 4 combinations, one class each:

| T0 | T1 (Δ_T) | T2 | Class | Rival named |
|---|---|---|---|---|
| null | any | any | Inconclusive | none; the earlier claim is falsified out of sample (reported under T0) |
| inconclusive | any | any | Inconclusive | none |
| active | active | active | Weakened | R-B |
| active | active | null, discriminating | Weakened | R-C |
| active | active | null, vacuous | Weakened | R-C or R-B, not separated |
| active | active | inconclusive | Weakened | R-C or R-B, not separated |
| active | null | active | Weakened | R-O |
| active | null | null, discriminating | **Reinforced** | none; R-O is not excluded |
| active | null | null, vacuous | Inconclusive | none |
| active | null | inconclusive | Inconclusive | none |
| active | inconclusive | active | Weakened | R-B or R-O, not separated |
| active | inconclusive | null, discriminating | Inconclusive | none |
| active | inconclusive | null, vacuous | Inconclusive | none |
| active | inconclusive | inconclusive | Inconclusive | none |

- **Reinforced:** the reading "race for state" gains support; it remains an inference, not a proof of causation. It requires that the donor quantity be active (the "discriminating" outcome), so a placebo that cannot discriminate never reinforces.
- **Weakened:** the association stays as measured and stops being attributed to state; the rival of the table is named, or none when the sign pattern is incomplete.
- **Inconclusive:** every other case.

T3 and T4 refine the wording ("between bots", "state") without changing the classification. The status of T5 (robust, sensitive, bounded) is written next to the class and does not change it. Each test has its own outcome and the favorable one is not chosen. There is no multiplicity correction: decisions rest on threshold and interval, not on p-values (permutation p-values are descriptive).

## Verification before opening the data

- Automatic tests of the tool, run on the code that is sealed: calibration of each placebo exposure (random exposure within cells, 40 trials of 8 weeks, mean estimate within 0.004 of 0 and at most 9% of intervals called active); calibration of the permutation p of the estimator itself (200 trials, share of p ≤ 0.05 inside [0.02, 0.09]); power (an effect of +0.05 seeded only in X detected in at least 18 of 20 trials with the temporal and pool placebos not active; a seeded pool shock makes the temporal placebo active, a seeded block shock makes the pool placebo active, an effect seeded only on own previous swaps is seen by the entity placebo, each in at least 18 of 20); exact identity of the decomposition (error < 10⁻¹² on random data and on every resample) and a synthetic example that reproduces the sign flip; the true estimate with simulated missing data lies inside the range of the grid; the output of `race`, `contend` and `neighbor` without the new options is identical to version 0.7.0 on the data at hand; determinism; a warning on the error stream (and nothing else changed on the standard output) when `race` reads rows that carry v4 pseudo-pools without `--v4-swap-pools`.
- Cross-check: a second implementation in Python, from the same rows and receipts, reproduces the point estimates, counts, rates, the grid of T5(a) and the decomposition terms of T8 exactly (same printed digits) for these variants: the exposures of T0 to T4 (`prior-other`, `later-other`, `donor-pool`, `prior-same`, `user-prior`, `any-prior`, and the second indicators of `--x-also`), the donor subset and donor seeds, the pool rules and size strata of T5, the bot share of T6, the sender degree of T7 and the v4 pool manager of T10. The automatic parity test runs six of these combinations; the others were compared by hand on the smoke week (see the evidence list) and are not in an automatic test. The cross-check does not cover: 50 position bins, cells (entity, block), leave-one-out of entity keys, the heterogeneity statistics of T9 (the DerSimonian-Laird routine is checked in both languages against the same hand-computed example, not end to end), `--exclude-system`, intervals and permutation p-values. Those variants rest on the tests of the Rust code only. Intervals come from another random generator and are not compared. The cross-check is not independent (same author, same rows).
- If calibration, power, identity, regression or cross-check fails, nothing is sealed and no data is downloaded.

## Execution

- **Budget.** 24,192 blocks and 48,384 requests. At about US$0.032 per 1,000 blocks (estimate from the earlier study), about US$0.77 (range 0.7 to 1.0). Hard cap for the study: US$1.00. The download script fetches one week at a time and reads the plan's usage between weeks; before each week it stops if the amount spent since the start plus US$0.02 (an upper bound for one week) would pass US$1.00, and a decision is requested. If the download stops with weeks missing, T0 to T5 are run only if no more than 5% of the 24,192 blocks are missing, which is reported; otherwise they are inconclusive, by the rule below. Disk: about 11 GB for raw captures and rows of the fresh weeks. The weeks already seen need no new requests: their rows and receipts exist, and traces are needed only to re-extract rows for T10.
- **Check before the seal.** Two blocks outside every eligible week (one of 2024-02-01 and one of 2026-10-12) were requested with `debug_traceBlockByNumber` and `eth_getBlockReceipts` to confirm that the provider serves 2024 and 2026 on Base. The provider served both. I touched no block of an eligible week.
- **Stopping rule.** All 48 fresh weeks are downloaded before any test is run on any of them. During the download only coverage and errors are looked at. One primary run per test. If more than 5% of blocks have no raw data after retries, T0 to T5 are inconclusive.
- **Seeds.** Hash rules for sampling and donors; 20261017 for bootstrap and permutations.
- **Expected artifacts.** Raw captures with sha256 in a manifest, rows per week and their union, the outputs of each command, the cross-check, an execution log and a verdict report. Raw JSON-RPC responses are not redistributed; extracted rows, samples and outputs are.
- **Power estimate.** From the interval of the earlier study, the standard deviation of Δ across weeks is about 0.048, so the standard error with k weeks is 0.048/√k: 0.0098 with 24 and 0.0069 with 48. To declare a placebo null (u < 0.02) when its true value is 0, 1.96·SE must be below 0.02: that happens about half the time with 22 weeks and about 80% of the time with 45. With 48 weeks a null placebo is therefore detectable, but not with certainty. Reinforced needs two null placebos and a discriminating contrast: if both placebos were exactly 0 and independent, the chance of reaching it would be at most about 0.7 (0.82 squared), and lower if either placebo is slightly positive or the donor subset has fewer cells. The standard deviation of 0.048 comes from Δ, mixes the trend between weeks, and is not that of the placebos. These are estimates, not measurements. Power is not estimated for T4 to T10.
- **Deviations after the seal** are declared in the verdict, as in the earlier studies.

## Risks and authority

- Ethics and privacy: public on-chain data, bot contracts; published tables have at least 5 transactions per cell.
- Licenses: terms of the RPC provider.
- Conflicts of interest: the author presents results of this study in a talk and in a preprint. I will report the result whatever it is, including a falsification of the earlier claim.
- Limits: on the 2026 fresh weeks the primary reads the v4 PoolManager as one pool, and the size of the resulting distortion of T0, T1 and T2 is an open risk that S-v4 measures but does not remove. No test identifies causation. The bootstrap of 48 weeks is approximate. Entity keys are followed across weeks by the contract with the most transactions and are approximate if an entity rotates contracts. The grid of T5 is a practical bound. The decomposition of T8 depends on the reference point (the midpoint is symmetric and exact, not unique). Two years with different block regimes are described, not explained.

## Sealing

- Responsible: Keneth Issac Huerta Galindo.
- Version or hash: the sha256 of this file is registered with a third party before any data is downloaded (see the last section), together with the sha256 of the sample, the system list, the tool's source and lockfile, the test code, the analysis scripts, the cross-check script and the download script. The header of the seal declares the commit, the compiler version and the tool version; these three are declared and the tool does not check them.
- What the code checks, and what it does not:
  - `fetch` recomputes the sha256 of every file listed in the seal and refuses to open the sampled blocks if any differs. The option `--post-seal` exists: it tolerates changed code but still requires this file and the sample to be intact. The download script does not use it; any use would be declared as a deviation in the verdict.
  - `fetch` does not check that the block file it receives is the sealed sample; the download script checks the sha256 of the sample itself and passes `fetch` per-week files filtered from it.
  - Only `fetch` has this gate. `race`, `neighbor` and `race-contrast` run without any seal, so the rule "no placebo statistic before the external registration" is a commitment of the author that the tool does not enforce.
  - The download script refuses to run unless a receipt file of the external deposit exists and contains a DOI and the sha256 of this file. The receipt is created after the deposit and is not part of the seal.
- **Evidence observed before this registration:**
  - The complete result of the earlier study on Base (48 weeks of 2025): the primary estimate, the weekly series, the sensitivities with 50 position bins (+0.0741) and with cells (entity, block) (+0.0651), the sign flip without entity (−0.0298), the trend with the week index and the means of the first and last 24 weeks.
  - Diagnostics on the sample of the first study (other dates, not the weeks of this one): the shares of empty P, mean |P| and the 8.8% of non-probe rows that touch the Uniswap v4 PoolManager.
  - The results of a later study on two other chains, including that adopting v4 pseudo-pools moved the synchrony bound there from +0.0796 to +0.0104. That study was corrected on 2026-10-20 after a defect in how `race` read v4 swaps: on one chain the count of exposed transactions went from 65 to 761 once v4 swap logs were read as pseudo-pools. The defect and its correction motivated the option `--v4-swap-pools`, the shape of T10 and the sensitivity S-v4; the primary estimates of that study with the correction were printed (they are not placebos of this study).
  - Two internal reviews of the draft that reports the earlier result, which proposed several of these tests.
  - An internal adversarial review of the draft of this document (2026-10-20), which proposed the revisions described at the top. It looked at the text, the code and the earlier results; it did not touch any fresh block.
  - Placebo-type statistics printed before this registration, listed in full. The only data was one already-open week of the first study (week of 2025-12-21, 1,176 blocks: 504 anchors and 672 neighbors), which lies outside the 48 weeks of 2025 and outside the fresh weeks. (1) With version 0.6.0 of the tool, in the first study, only the primary Δ of that week was printed; no placebo. (2) During development and during the internal review of this document, the options of version 0.8.0 of `race` (the exposures of T1 to T4, the checks of T5, the bot share and entity variants of T6 and T7, the decomposition of T8, the heterogeneity statistics of T9 and the v4 option of T10) were run on that same week, with small numbers of resamples, to check that every command runs and that both implementations agree. Estimates, counts and rough intervals were printed; none was saved in the repository or recorded. No threshold in this document was set or moved after seeing them, and I cannot show that I did not remember them.
  - Synthetic data only otherwise. No block, receipt or trace of the 48 fresh weeks has been downloaded, and no placebo statistic has been computed on the 48 weeks of 2025 or on any fresh block.

## Publication and external registration

- Will be published or cited outside the private repository: yes.
- Publishable form: yes.
- External registration, to be filled BEFORE opening the reserved evidence. This file is registered with this section blank, so that its sha256 is the one deposited; the deposit data are recorded outside this file and linked to that hash.
  - Receipt for the download script: a file with the DOI and this file's sha256, which the script requires before it makes any request.
  - Service:
  - Identifier (URL or DOI):
  - Date and time UTC of registration:
  - sha256 of the publishable file:
- Retroactive case: not applicable.
