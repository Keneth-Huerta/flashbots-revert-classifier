# flashbots-revert-classifier

CLI tool in Rust that ingests Dune Analytics CSV exports of failed MEV transactions and
classifies them into two operational categories: **Blind Spam** and **HF Contention (Elite Colocation)**.

Built as a response to the Flashbots research forum discussion on transaction spam heuristics
(query [7809756](https://dune.com/queries/7809756) by [m1kuwill](https://dune.com/m1kuwill)).

## Methodology

The classifier groups transactions by `(block_number, bot_address)` and applies a configurable
collision threshold `C_min` (default: 3). When `C_min` or more reverts target the same bot
contract within the same block, the cluster is classified as **HF Contention** (high-frequency
competition between sophisticated searchers). Clusters below the threshold are **Blind Spam**
(low-density, untargeted bot activity).

This heuristic is documented in detail at
`docs/07_fundamentos_teoricos/clasificacion-reverts-mev.md`.

## Requirements

- Rust toolchain stable (1.75+)
- CSV file exported from [Dune query 7809756](https://dune.com/queries/7809756)

## Usage

```bash
cargo run --release -- --file dune_export.csv
```

Options:

```
-f, --file <FILE>          Path to Dune-exported CSV file
-t, --threshold <N>        Collision threshold C_min [default: 3]
--top-n <N>                Number of top contested bots to show [default: 10]
```

### Example output

```
mev-revert-classifier v0.1.0
============================================================
File:           dune_export.csv
Rows processed: 150,342
Threshold (C_min): 3

Raw Breakdown (from Dune classification):
  reverted_spam:  120,450
  dry_run_probe:  29,892

============================================================
Heuristic Classification (by bot block collisions)
============================================================
Blind Spam:        89,450 tx (59.5%)  |  12,340 unique bots
HF Contention:     60,892 tx (40.5%)  |     856 unique bots
------------------------------------------------------------
Total:            150,342 tx (100.0%)
============================================================

Average Gas per Tx:
  Blind Spam:      84.2K
  HF Contention:   132.6K

Top 10 Most Contested Bots:
  #1  0x7a250d5630...  (147 tx in block 18432001)
  ...

Processing time: 342ms
```

## Architecture

Written as a single-pass ETL pipeline:

```
CSV (csv + serde) → Vec<Transaction> → HashMap<(block, bot)> → Classifier → Console Report
```

No async runtime. No parallelism. Three dependencies: `csv`, `serde`, `clap`.



MIT
## License

MIT

## Results (Sample Run)

Dune Query 7843588, Base L2, Week 30 (Dec 21-27 2025), 32,000 transactions, 591 blocks:

| C_min | Blind Spam | HF Contention | Time |
|-------|-----------|---------------|------|
| 3 | 2,044 tx (6.4%) | 29,956 tx (93.6%) | 81ms |
| 5 | 2,880 tx (9.0%) | 29,120 tx (91.0%) | 108ms |
| 10 | 9,640 tx (30.1%) | 22,360 tx (69.9%) | 86ms |

One bot (`0x837b57a93d...`) accounts for the entire top 10 with 80-81 transactions per block.
