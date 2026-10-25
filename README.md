# mev-revert-classifier

[![Data DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.23549452.svg)](https://doi.org/10.5281/zenodo.23549452)
[![Release](https://img.shields.io/github/v/release/Keneth-Huerta/flashbots-revert-classifier)](https://github.com/Keneth-Huerta/flashbots-revert-classifier/releases/tag/v1.0.0)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Rust CLI and analysis scripts for studying **failed arbitrage attempts** (probes and reverts) on
OP-stack chains: Base, Unichain and OP Mainnet. The repository holds the code, the block samples,
the preregistrations and the sealed hashes of four studies:

| Study | Question | Chain | Sample |
|---|---|---|---|
| AF-01 | Do failed attempts of different entities share more pools in the same block than a within-hour permutation null? | Base | 4 weeks of 2025, about 800 blocks per week |
| AF-02 | Inside a block, is the revert rate higher after a successful swap by another entity on the same pools? How much of the overlap is block-level synchrony? | Base | 48 weeks of 2025, 504 anchor blocks per week |
| AF-03 | Same two tests as AF-02 | Unichain, OP Mainnet | 12 weeks of 2025 per chain |
| AF-04 | Does the AF-02 race replicate on weeks not seen before, and does it survive temporal and pool placebos and changes to the bot, entity and pool definitions? | Base | 24 weeks of 2024 and 24 of 2026, 504 anchor blocks per week; reruns on the AF-02 weeks |

[RESULTS.md](RESULTS.md) has the results. In brief: most of the overlap that v0.1.0 of this tool
called "contention" is one bot repeating itself, and the measurable evidence of a race inside the
block is much smaller (see Errata 2026-10-25). AF-03 reached no decision on the race test in either chain. AF-04 finds the race
again in 2024 and 2026, with a joint reading of the placebos that is inconclusive.

Errata 2026-10-25: that comparison sets an excess of overlap over a null against 0.9, which are not
the same kind of quantity; the study gives no replacement for the 90%. See the Errata section of
[RESULTS.md](RESULTS.md).

## Definitions used here

- **Probe** (`dry_run_probe` in the Dune query): a *successful* transaction that emits no external
  logs and carries at most 1 wei. It read state and stopped without swapping.
- **Revert**: a transaction that reverted during execution.
- **Entity**: the set of bot contracts that share a sender. It approximates an operator. The merge
  is a stated limitation and identifies no one.

## What this does not measure

Counting attempts per bot and block tells you how often *one* bot repeats itself. Version 0.1.0
labeled dense groups "HF Contention", and a ~90% figure from it was quoted on the Flashblocks forum
thread that started this work. The label was wrong. The v0.1.0 `REPORT.md` is gone from the tree
(git history keeps it), and [RESULTS.md](RESULTS.md) replaces it.

## Requirements

- Rust stable (the author produced the published numbers with rustc 1.99.1)
- Python 3 with `numpy` for `scripts/analisis/` (cross-checks of the Rust estimators)
- To re-download chain data: an Alchemy API key with archive and `debug_traceBlockByNumber`
  access, exported as `ALCHEMY_API_KEY` (or in a `.env` file, which is git-ignored)

## Build and test

```bash
cargo build --release
cargo test
python3 -I -m unittest discover scripts/tests
```

## Quick start: per-bot density profile of a Dune export (v0.2 behaviour)

```bash
cargo run --release -- classify --file sample.csv
```

`dataset.csv` is the 32,000-row export of Dune query 7843588 (a fork of Fei Wu and Burak Oz's
query 7809756) used in the first report: 591 consecutive Base blocks, about 20 minutes on
2025-12-24. It describes those 20 minutes only.

## Reproducing the studies

The repository does **not** include the raw chain captures (traces and receipts, many GB). You
re-download them from an archive RPC. The extracted rows of the four studies, with the sha256 of every
raw capture, are on Zenodo: **DOI [10.5281/zenodo.23549452](https://doi.org/10.5281/zenodo.23549452)**.
With them you can skip `fetch` for the counting analyses; `race` and `neighbor` still need the receipts.
The block samples are deterministic and live in `datos/`;
the `*-sello-*.sha256` files under `docs/04_investigacion_y_labs/` hold their sha256. Cost on Alchemy pay-as-you-go: about US$0.9 for AF-03; AF-02
needs a few dollars.

Each confirmatory step checks a seal file (`sha256sum` format) before it opens any block. The
preregistrations and block samples match the sealed bytes, so the gate passes with `--post-seal`,
which tolerates changes to the other sealed files (see "About the seals" at the end of this section).

Set `B=target/release/mev-revert-classifier` and run from the repository root.

### AF-01 (Base, 4 weeks)

```bash
S=docs/04_investigacion_y_labs/2026-10-17-sello-contencion.sha256
$B fetch --blocks datos/muestra_bloques_v1.csv --split confirmatory --seal $S --post-seal \
  --raw-dir artefactos/rpc/muestra_v1 --out artefactos/confirmatorio/rows.csv --workers 8
$B contend --rows artefactos/confirmatorio/rows.csv --blocks datos/muestra_bloques_v1.csv \
  --split confirmatory --metric weighted
python3 -I scripts/analisis/check_contencion.py          # independent Python recount of bots and entities
```

### AF-02 (Base, 48 weeks)

```bash
S=docs/04_investigacion_y_labs/2026-10-18-sello-af02.sha256
python3 scripts/ingesta/descarga_af02.py --sin-dvc     # week by week; resumable
# then concatenate artefactos/af02/filas/*.csv (one header) into artefactos/af02/rows.csv
$B race --rows artefactos/af02/rows.csv --raw artefactos/rpc_af02 \
  --blocks datos/muestra_bloques_af02_anclas.csv --split confirmatory --boot-unit week
$B neighbor --rows artefactos/af02/rows.csv --blocks datos/muestra_bloques_af02.csv \
  --split confirmatory --boot-unit week
python3 scripts/analisis/carrera_tasa.py --help        # Python cross-check of `race`
python3 scripts/analisis/vecino.py --help              # Python cross-check of `neighbor`
```

`descarga_af02.py` writes one rows file per week; read its docstring first.

### AF-03 (Unichain and OP Mainnet, 12 weeks each)

The author preregistered this study and deposited it on Zenodo before downloading any block of the
drawn weeks: **DOI [10.5281/zenodo.23457231](https://doi.org/10.5281/zenodo.23457231)**. The sealed
hash list `2026-10-18-sello-af03.sha256` and the preregistration each have an OpenTimestamps proof
(`.ots`) next to them.

```bash
S=docs/04_investigacion_y_labs/2026-10-18-sello-af03.sha256
python3 scripts/analisis/sorteo_semanas_af03.py 2025-02-12   # the hash-based week draw (prints --stratum args)
for c in op unichain; do
  $B fetch --chain $c --v4-pools --blocks datos/muestra_bloques_af03_$c.csv --split confirmatory \
    --seal $S --post-seal --raw-dir artefactos/rpc_af03/$c --out artefactos/af03/filas_$c.csv
  $B race --chain $c --rows artefactos/af03/filas_$c.csv --raw artefactos/rpc_af03/$c \
    --blocks datos/muestra_bloques_af03_${c}_anclas.csv --split confirmatory \
    --boot-unit week --bootstrap 1000 --permutations 1000 --seed 20261017
  $B neighbor --chain $c --rows artefactos/af03/filas_$c.csv \
    --blocks datos/muestra_bloques_af03_$c.csv --split confirmatory \
    --boot-unit week --bootstrap 1000 --seed 20261017
done
scripts/analisis/sens_af03.sh unichain                    # preregistered sensitivities
```

`scripts/ingesta/descarga_af03.sh` runs the download loop for both chains. The list
`datos/tx_sistema.csv` excludes system and builder transactions.

### AF-04 (Base, 48 new weeks and the AF-02 weeks)

Preregistration on Zenodo before the download:
**DOI [10.5281/zenodo.23492680](https://doi.org/10.5281/zenodo.23492680)**, with OpenTimestamps
proofs next to the preregistration and to `2026-10-18-sello-af04.sha256`.

```bash
scripts/ingesta/descarga_af04.sh              # 48 new weeks, anchors only, one week at a time
bash scripts/analisis/correr_af04_frescas.sh  # T0 to T7, T9 and S-v4 on the new weeks
bash scripts/analisis/correr_af04_vistas.sh   # T6 to T9 and the concordance runs on the AF-02 weeks
bash scripts/analisis/correr_af04_vistas.sh T10
```

The scripts chain the commands of the preregistration and save each output under `artefactos/af04/`. The AF-02 runs need the AF-02 rows
and receipts in `artefactos/af02/` and `artefactos/rpc_af02/`; T10 also needs the AF-02 traces.

### About the seals

The OpenTimestamps proofs and the Zenodo record fix the AF-03 preregistration and the hash list
as they were on 2026-10-18. The seal files also list internal design notes that this repository does **not publish**: ADR 005 in the AF-03
seal (sha256 `5bae3de071a35a87b26726f385f1a43443648bb9bd3ce97102e3997b95538498`), ADR 004 in the AF-02
seal (`1c30c82edeff4bc382ae4c3c85ee7c8898744decb7165d05c9a71f1583d61d48`), and ADR 002 and 003 in the
AF-01 seal (`79790f6d3b3fffdabb2bfe24d9f05ecf08b28b7a1a2e851dc27c19af8db94168`,
`901720ee72659e9a95630061cb6880e2f4a3282eead448182ac3937de2774ac4`). On this release,
`sha256sum -c` on the AF-04 seal reports 45 OK and 1 missing, ADR 006
(`e2fa50ad7adf34189be4570d0f014daab56d90d14a9db5b4e1fc664ef8ff1127`); that is the only expected
failure. On the AF-03 seal it reports 22 OK, ADR 005 missing, and 7 code files changed (`Cargo.toml`,
`Cargo.lock`, `src/lib.rs`, `src/main.rs`, `src/race.rs`, `scripts/analisis/carrera_tasa.py`,
`scripts/analisis/orden_swap.py`): the CLI moved to v0.8.0 for AF-04 and the race code was fixed to read
Uniswap v4 swaps as pools, a correction the results file reports for AF-03.

The code listed in the AF-01, AF-02 and AF-03 seals changed afterwards through declared post-seal
deviations, so `sha256sum -c` reports those files as changed and the unpublished notes as missing.
Their preregistrations and block samples are intact, and those are what `--post-seal` checks (it
tolerates missing non-preregistration files).

## Repository layout

```
src/                 Rust library and CLI (classify, sample, fetch, contend, race, neighbor)
tests/               Rust integration tests (synthetic data, no network)
datos/               block samples, anchor files and the system-transaction list
scripts/analisis/    Python re-implementations and descriptive analyses
scripts/ingesta/     chain downloads
scripts/tests/       tests of the Python scripts
docs/04_investigacion_y_labs/   preregistrations, seal files, OpenTimestamps proofs
docs/07_fundamentos_teoricos/   estimator derivations
```

## Limits

The data are observational. Arrival time and fee set the order of transactions inside a block,
along with state, so a higher revert rate after another entity's swap does not prove competition.
The author infers entities from shared senders. The sampled weeks are from 2025 (AF-01 to AF-03) and
from 2024 and 2026 (AF-04).

## Data sources

Traces (`debug_traceBlockByNumber` with `callTracer`) and block receipts come from the Alchemy
archive RPC for Base, Unichain and OP Mainnet. `dataset.csv` comes from Dune (query 7843588, a fork
of Fei Wu and Burak Oz's query 7809756). The rows on Zenodo were extracted from those captures.

## License and citation

MIT, see [LICENSE](LICENSE); the Zenodo dataset is CC BY 4.0. To cite the software, the dataset or
the preregistrations, use [CITATION.cff](CITATION.cff) (GitHub shows a "Cite this repository" button).

Author: Keneth Issac Huerta Galindo, Escuela Superior de Computo (ESCOM), Instituto Politecnico
Nacional. ORCID [0009-0004-9084-2937](https://orcid.org/0009-0004-9084-2937).

Preprint with the full write-up: [10.5281/zenodo.23610471](https://doi.org/10.5281/zenodo.23610471).
