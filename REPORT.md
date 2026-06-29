---
estado: INMUTABLE
categoria_documental: REPORTE
tipo: "MEV Heuristic Classification: Dune Query 7843588"
fecha_ejecucion: 2026-06-29
hash_commit: "345c7d7"
mercado: "Base (L2)"
hardware: "Intel i7-1165G7 @ 2.80GHz | 19Gi RAM | Linux 7.0.14-zen1-1-zen"
---

# Execution Report: MEV Revert Heuristic Classification, Week 30, Base L2

## 0. Publication Prerequisites

- Clean working tree: yes (commit `345c7d7`).
- `hash_commit` matches `git rev-parse --short HEAD`.
- Conclusive execution with 32,000 rows processed across 3 threshold configurations.

## 1. Test Parameters

| Parameter | Value |
|-----------|-------|
| Dataset | Dune Query 7843588, exported via API (`limit=100000`) |
| Rows processed | 32,000 |
| Unique blocks | 591 (39872527 to 39873117) |
| Weeks | 1 (week 30: 2025-12-21 to 2025-12-27) |
| Unique bots | 74 |
| C_min thresholds tested | 3, 5, 10 |
| Binary | `target/release/mev-revert-classifier` (release build) |
| Classification criteria | Group by `(block_number, bot_address)`; >= C_min -> HF Contention, < C_min -> Blind Spam |

## 2. Hardware Environment (Required)

| Component | Value |
|-----------|-------|
| CPU | 11th Gen Intel Core i7-1165G7 @ 2.80GHz (4 cores, 8 threads) |
| RAM | 19 GiB |
| OS | Linux 7.0.14-zen1-1-zen (Arch) |
| GPU | Intel Iris Xe (unused) |

## 3. Key Results

### 3.1 Classification by Threshold

| C_min | Blind Spam (tx) | Blind Spam (%) | HF Contention (tx) | HF Contention (%) | Spam Bots | Contention Clusters | Time |
|-------|-----------------|----------------|---------------------|-------------------|-----------|---------------------|------|
| 3 | 2,044 | 6.4% | 29,956 | 93.6% | 1,444 | 2,306 | 81ms |
| 5 | 2,880 | 9.0% | 29,120 | 91.0% | 1,701 | 2,049 | 108ms |
| 10 | 9,640 | 30.1% | 22,360 | 69.9% | 2,674 | 1,076 | 86ms |

### 3.2 Average Gas per Category

| C_min | Blind Spam (gas/tx) | HF Contention (gas/tx) |
|-------|---------------------|-------------------------|
| 3 | 135.2K | 88.6K |
| 5 | 122.6K | 88.5K |
| 10 | 102.9K | 86.7K |

### 3.3 Dune Raw Breakdown

| Dune Classification | Count |
|---------------------|-------|
| `dry_run_probe` | 31,157 (97.4%) |
| `reverted_spam` | 843 (2.6%) |

### 3.4 Top Bots by Collision Density

Bot `0x837b57a93d4c0e5be3d4c551730fd7f3b6f7722f` occupies all 10 top positions across all 3 thresholds, with 80-81 transactions per block. No other bot exceeds ~12 tx per block. The other 3 significant bots are `0x2ffd221f8f...`, `0xd9c72e9403...`, and `0x982e4323bc...`, each with 2-12 tx per block.

## 4. Generated Artifacts

- **Raw dataset:** `dataset.json` (32,000 rows, Dune API JSON)
- **Quick test sample:** `sample.csv` (10 rows, same format)
- **CSV conversion:** `python3 -c "import json,csv; d=json.load(open('dataset.json')); w=csv.writer(open('dataset.csv','w')); w.writerow(d['result']['rows'][0].keys()); [w.writerow(r.values()) for r in d['result']['rows']]"`

## 5. Conclusions and Next Steps

**Verdict: SUCCESS.** The classifier processes 32,000 rows in under 110ms, produces consistent ratios across 3 thresholds, and reveals a clear bot traffic concentration pattern on Base L2.

**Key findings:**

1. High-frequency contention dominates traffic: 91-94% of volume for C_min in {3,5}. Even at C_min=10 it remains at 70%.
2. A single bot (`0x837b57a93d...`) takes the entire top 10 with 80+ tx per block. This is an extreme outlier that warrants further investigation: is the bot competing against itself (internal redundancy), or is the query capturing multiple strategies from the same contract?
3. Contention bots use less gas on average (86-88K) than spam bots (102-135K), suggesting gas optimization among elite bots.
4. Only 2.6% of transactions are `reverted_spam` per Dune; 97.4% are `dry_run_probe`. This suggests bots are probing/simulating rather than failing.

**Suggested next steps:**

- Expand dataset to multiple weeks (27-31) to validate seasonality.
- Add calldata similarity heuristic to disambiguate multiple strategies from the same bot.
- Test with Ethereum mainnet data to compare L1 vs L2 contention patterns.
- Publish results in the Flashbots research forum as a reply to m1kuwill's thread.
