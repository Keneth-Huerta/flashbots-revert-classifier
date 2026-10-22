use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use mev_revert_classifier::chain::Chain;
use mev_revert_classifier::contend::{
    self, BotRule, ContendParams, EntityRule, Metric, NullKind, RunOptions,
};
use mev_revert_classifier::extract::{Kind, TxRow};
use mev_revert_classifier::fetch::{fetch_blocks, sha256_hex, verify_seal, verify_seal_post};
use mev_revert_classifier::rpc::Rpc;
use mev_revert_classifier::sample::{
    block_timestamp_on, parse_date, sample_week_neighbors_on, sample_week_on, Role, SampledBlock,
    Timing,
};
use mev_revert_classifier::system::{self, SystemFilter};
use mev_revert_classifier::{
    analyze, format_gas, is_usable, pct, short_addr, DuneTransaction, Report,
};
use mev_revert_classifier::{neighbor, race};

#[derive(Parser)]
#[command(name = "mev-revert-classifier")]
#[command(version)]
#[command(
    about = "Failed MEV arbitrage attempts on Base: Dune CSV profiling, RPC ingestion and a contention test"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Chain and system-transaction options of the analysis subcommands (AF-03).
#[derive(clap::Args, Clone)]
struct ChainArgs {
    #[arg(
        long,
        value_enum,
        default_value = "base",
        help = "Chain of the rows and the sample: block clock for the hours (Base and OP 2 s, Unichain 1 s) and neighbor offsets (+2/+30/+300 blocks on Unichain)"
    )]
    chain: Chain,
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "true",
        value_name = "BOOL",
        help = "Leave system and builder transactions (--system-list) out before bots, entities and pools are defined; default: off on base, on on unichain and op (ADR 005 D2)"
    )]
    exclude_system: Option<bool>,
    #[arg(
        long,
        default_value = system::DEFAULT_LIST,
        help = "CSV cadena,rol,direccion,fuente of system and builder transactions"
    )]
    system_list: PathBuf,
}

#[derive(Clone, Copy, ValueEnum)]
enum XDefArg {
    PriorOther,
    LaterOther,
    DonorPool,
    PriorSame,
    UserPrior,
    AnyPrior,
}

#[derive(Clone, Copy, ValueEnum)]
enum XAlsoArg {
    None,
    PriorOther,
    UserPrior,
    PriorSame,
    LaterOther,
}

#[derive(Clone, Copy, ValueEnum)]
enum PRuleArg {
    Trace,
    Contract,
    ContractSubset,
    TraceOnContractSubset,
}

/// AF-04 options of `race` (ADR 006): all opt-in; without any of them the output of `race`
/// is the one of v0.7.0.
#[derive(clap::Args)]
struct Af04Args {
    #[arg(
        long,
        value_enum,
        help = "Exposure X: prior_other (AF-02), later_other (temporal placebo), donor_pool (pool placebo), prior_same (entity placebo), user_prior, any_prior"
    )]
    x_def: Option<XDefArg>,
    #[arg(
        long,
        value_enum,
        help = "Second indicator folded into the cells: (entity, bin, Z)"
    )]
    x_also: Option<XAlsoArg>,
    #[arg(
        long,
        value_enum,
        help = "Pools of X: trace (AF-02) or the contract-week rule (10% of the successful swaps of the contract, at most 5)"
    )]
    p_rule: Option<PRuleArg>,
    #[arg(long, help = "Cells (entity, bin, min(|P|, 3))")]
    p_size_strata: bool,
    #[arg(
        long,
        help = "Minimum share of failed attempts of a bot contract (AF-02: 0.5)"
    )]
    bot_share: Option<f64>,
    #[arg(long, default_value = "1", help = "Seed of the donor choice")]
    donor_seed: u64,
    #[arg(
        long,
        help = "Keep only transactions with a donor, whatever the exposure"
    )]
    donor_subset: bool,
    #[arg(
        long,
        help = "Swap logs of the v4 PoolManager count as pseudo-pools v4:<PoolId> (use with rows extracted with --v4-pools)"
    )]
    v4_swap_pools: bool,
    #[arg(
        long,
        help = "Grid of q_R, q_S over the bot transactions with empty P (selection bounds)"
    )]
    missing_grid: bool,
    #[arg(
        long,
        help = "Exact decomposition of the gap between decile-only and (entity, decile)"
    )]
    decompose: bool,
    #[arg(
        long,
        help = "Concentration of the MH weight by entity key and leave-one-out of the K largest keys"
    )]
    loo: Option<usize>,
    #[arg(long, help = "DerSimonian-Laird heterogeneity between strata")]
    het: bool,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Substrings of stratum names: one heterogeneity report per group"
    )]
    het_groups: Vec<String>,
    #[arg(
        long,
        help = "Write variant, stratum, num, den per stratum (input of race-contrast)"
    )]
    emit_weeks: Option<PathBuf>,
}

impl Af04Args {
    fn active(&self) -> bool {
        self.x_def.is_some()
            || self.x_also.is_some()
            || self.p_rule.is_some()
            || self.p_size_strata
            || self.bot_share.is_some()
            || self.donor_subset
            || self.v4_swap_pools
            || self.missing_grid
            || self.decompose
            || self.loo.is_some()
            || self.het
            || !self.het_groups.is_empty()
            || self.emit_weeks.is_some()
    }
}

/// [`ChainArgs`] resolved: clock and filter (`None` when the filter is off).
struct ChainCtx {
    chain: Chain,
    timing: Timing,
    system: Option<SystemFilter>,
}

impl ChainArgs {
    fn resolve(&self) -> Result<ChainCtx> {
        let on = self.exclude_system.unwrap_or(self.chain != Chain::Base);
        let system = if on {
            Some(SystemFilter::load(&self.system_list, self.chain)?)
        } else {
            None
        };
        Ok(ChainCtx {
            chain: self.chain,
            timing: self.chain.timing(),
            system,
        })
    }
}

impl ChainCtx {
    /// Nothing differs from the Base defaults: no extra output line.
    fn is_default(&self) -> bool {
        self.chain == Chain::Base && self.system.is_none()
    }

    fn describe(&self, rows: &[TxRow]) {
        if self.is_default() {
            return;
        }
        println!(
            "chain {} (block time {} s) | system filter {}",
            self.chain.as_str(),
            self.timing.block_time,
            match &self.system {
                None => "off".to_string(),
                Some(f) => format!(
                    "on, {} entries, {} of {} rows excluded",
                    f.len(),
                    f.count(rows),
                    rows.len()
                ),
            }
        );
        if let Some(f) = &self.system {
            for (role, addr, n) in f.counts(rows) {
                if n > 0 {
                    println!("  system {role} {addr}: {n} rows");
                }
            }
        }
    }

    /// The sample's `hour_start_ts` must be the hour of the block on this chain's clock.
    fn check_sample(&self, sample: &[&SampledBlock]) -> Result<()> {
        for b in sample {
            let hour = block_timestamp_on(self.timing, b.block).div_euclid(3600) * 3600;
            if hour != b.hour_start_ts {
                bail!(
                    "sample block {} has hour_start_ts {} but its hour on {} is {}: wrong --chain for this sample",
                    b.block,
                    b.hour_start_ts,
                    self.chain.as_str(),
                    hour
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Split {
    Exploratory,
    Confirmatory,
    All,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum MetricArg {
    Binary,
    Weighted,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum NullArg {
    Primary,
    Active,
}

#[derive(Subcommand)]
enum Command {
    /// Per-bot density profile of a Dune export (v0.2 behaviour)
    Classify {
        #[arg(short, long, help = "Path to Dune-exported CSV file")]
        file: String,
        #[arg(
            short,
            long,
            default_value = "3",
            help = "C_min: attempts by the same bot in the same block >= C_min = self-burst"
        )]
        threshold: usize,
        #[arg(
            long,
            default_value = "10",
            help = "Number of top bots and groups to show"
        )]
        top_n: usize,
    },
    /// Deterministic block sample, stratified by UTC hour
    Sample {
        #[arg(
            long,
            required = true,
            help = "Stratum as NAME=YYYY-MM-DD (week start, UTC); repeatable"
        )]
        stratum: Vec<String>,
        #[arg(long, default_value = "6")]
        per_hour: usize,
        #[arg(long, help = "Exclude an already-seen block range FIRST-LAST")]
        exclude: Option<String>,
        #[arg(
            long,
            help = "Neighbor design (ADR 004): per hour 3 anchors, two with b+1 and one with b+15 and b+150; adds role and anchor columns; --per-hour is ignored"
        )]
        neighbors: bool,
        #[arg(
            long,
            value_enum,
            default_value = "base",
            help = "Chain whose block clock defines the hours (Base and OP 2 s, Unichain 1 s); neighbors keep their wall-clock distance (2 s, 30 s, 5 min)"
        )]
        chain: Chain,
        #[arg(long)]
        out: PathBuf,
    },
    /// Fetch traces and receipts for sampled blocks (needs ALCHEMY_API_KEY unless raw files exist)
    Fetch {
        #[arg(long, help = "CSV from `sample`")]
        blocks: PathBuf,
        #[arg(long, value_enum, default_value = "exploratory")]
        split: Split,
        #[arg(
            long,
            help = "Explicit blocks instead of a sample file (e.g. a pilot on seen blocks)"
        )]
        block: Vec<u64>,
        #[arg(long, help = "Seal file; required to open confirmatory blocks")]
        seal: Option<PathBuf>,
        #[arg(long, default_value = "artefactos/rpc")]
        raw_dir: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "4")]
        workers: usize,
        #[arg(long, help = "Stop after this many blocks")]
        max_blocks: Option<usize>,
        #[arg(long, help = "Never call the RPC; only re-extract raw files on disk")]
        offline: bool,
        #[arg(
            long,
            value_enum,
            default_value = "base",
            help = "Chain: Alchemy network and constants (PoolManager)"
        )]
        chain: Chain,
        #[arg(
            long,
            help = "Detect Uniswap v4 pools: a v4 swap contributes the pseudo-pool v4:<PoolId> and the PoolManager stops being one pool"
        )]
        v4_pools: bool,
        #[arg(
            long,
            help = "Code changed after the seal: require only preregistro and sample intact, list the other changed sealed files as a post-seal deviation"
        )]
        post_seal: bool,
    },
    /// Contention ratio vs. the permutation null, per stratum
    Contend {
        #[arg(long)]
        rows: PathBuf,
        #[arg(long)]
        blocks: PathBuf,
        #[arg(long, value_enum, default_value = "exploratory")]
        split: Split,
        #[arg(long, default_value = "1000")]
        permutations: usize,
        #[arg(long, default_value = "2000")]
        bootstrap: usize,
        #[arg(long, default_value = "50")]
        bootstrap_permutations: usize,
        #[arg(long, default_value = "20261017")]
        seed: u64,
        #[arg(
            long,
            default_value = "0",
            help = "Drop the N pools most read by bots (sensitivity)"
        )]
        exclude_top_pools: usize,
        #[arg(long, help = "Drop a pool address (sensitivity); repeatable")]
        exclude_pool: Vec<String>,
        #[arg(
            long,
            help = "Only bots with a successful cyclic arbitrage (sensitivity)"
        )]
        require_cyclic: bool,
        #[arg(
            long,
            default_value = "0.5",
            help = "Minimum failed share of a contract's transactions to count as a bot"
        )]
        min_failed_share: f64,
        #[arg(
            long,
            help = "Count contracts separately instead of grouping them into entities (sensitivity)"
        )]
        no_entities: bool,
        #[arg(
            long,
            value_enum,
            default_value = "binary",
            help = "Attempt score: binary (§4.4) or weighted by pool specificity (§4.4 bis)"
        )]
        metric: MetricArg,
        #[arg(
            long,
            value_enum,
            default_value = "primary",
            help = "Permutation null: primary (§4.5) or active, permuting only among the entity's active blocks (exploratory, nulos secundarios §4.1)"
        )]
        null: NullArg,
        #[arg(
            long,
            help = "Drop the entity whose representative is ADDR (or that contains this contract); repeatable (sensitivity)"
        )]
        drop_entity: Vec<String>,
        #[arg(
            long,
            default_value = "0",
            help = "Leave-one-out: also rerun without each of the N entities with most failed attempts with pools"
        )]
        loo_top: usize,
        #[arg(
            long,
            value_enum,
            default_value = "pooled",
            help = "CR_w weighting: pooled per attempt, or equal weight per entity (mean of entity means)"
        )]
        entity_weight: EntityWeightArg,
        #[arg(
            long,
            value_enum,
            default_value = "any",
            help = "Merge contracts sharing any sender (any) or at least 2 senders (shared2)"
        )]
        entity_rule: EntityRuleArg,
        #[arg(
            long,
            help = "Ignore senders that call at least K bot contracts when building entities"
        )]
        max_sender_degree: Option<usize>,
        #[arg(
            long,
            default_value = "1",
            value_parser = clap::value_parser!(usize),
            help = "Bootstrap blocks of H contiguous hours (6 or 12) instead of single hours (1)"
        )]
        bootstrap_block_hours: usize,
        #[arg(
            long,
            help = "Also report E with each day left out and the jackknife SE"
        )]
        jackknife_days: bool,
        #[arg(
            long,
            help = "Remove sampled blocks with no row in --rows (not downloaded) instead of keeping them as empty slots"
        )]
        drop_blocks_without_rows: bool,
        #[command(flatten)]
        chain: ChainArgs,
    },
    /// Race test inside the block (P-1): Mantel-Haenszel risk difference of revert rate by prior swap of another entity
    Race {
        #[arg(long)]
        rows: PathBuf,
        #[arg(long)]
        blocks: PathBuf,
        #[arg(long, value_enum, default_value = "exploratory")]
        split: Split,
        #[arg(
            long,
            default_value = "artefactos/rpc/muestra_v1",
            help = "Raw captures (receipts) directory"
        )]
        raw: PathBuf,
        #[arg(long, default_value = "10", help = "Position bins per block")]
        bins: usize,
        #[arg(
            long,
            help = "Cells (entity, block) instead of (entity, bin) (sensitivity)"
        )]
        cell_block: bool,
        #[arg(
            long,
            value_enum,
            default_value = "hour",
            help = "Pooled bootstrap unit: hours within strata, or whole strata (weeks)"
        )]
        boot_unit: BootUnit,
        #[arg(long, default_value = "1000")]
        bootstrap: usize,
        #[arg(long, default_value = "1000")]
        permutations: usize,
        #[arg(long, default_value = "20261017")]
        seed: u64,
        #[arg(long, help = "Only the sin_probes variant")]
        sin_probes_only: bool,
        #[arg(
            long,
            help = "Ignore senders that call at least N distinct bot contracts when building entities (R-ent, N = 10 in AF-02; same rule as `contend --max-sender-degree`; N = 0 ignores every sender, so each contract is its own entity)"
        )]
        max_sender_degree: Option<usize>,
        #[command(flatten)]
        af04: Af04Args,
        #[command(flatten)]
        chain: ChainArgs,
    },
    /// Contrast of two estimands of `race --emit-weeks` files (same strata, same resamples of weeks)
    RaceContrast {
        #[arg(long)]
        a: PathBuf,
        #[arg(long)]
        b: PathBuf,
        #[arg(long, default_value = "sin_probes")]
        variant: String,
        #[arg(long, default_value = "1000")]
        bootstrap: usize,
        #[arg(long, default_value = "20261017")]
        seed: u64,
    },
    /// Neighbor bound of block synchrony (P-2): C(0) - C(k) with the neighbors of a `sample --neighbors` file
    Neighbor {
        #[arg(long)]
        rows: PathBuf,
        #[arg(
            long,
            help = "Sample CSV (with role and anchor columns; plain samples give C(0) only)"
        )]
        blocks: PathBuf,
        #[arg(long, value_enum, default_value = "exploratory")]
        split: Split,
        #[arg(
            long,
            value_enum,
            default_value = "hour",
            help = "Pooled bootstrap unit: hours within strata, or whole strata (weeks)"
        )]
        boot_unit: BootUnit,
        #[arg(long, default_value = "1000")]
        bootstrap: usize,
        #[arg(long, default_value = "20261017")]
        seed: u64,
        #[arg(
            long,
            help = "Skip pairs whose neighbor block has no row in --rows (not downloaded) instead of counting it as empty"
        )]
        drop_blocks_without_rows: bool,
        #[command(flatten)]
        chain: ChainArgs,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BootUnit {
    Hour,
    Week,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum EntityWeightArg {
    Pooled,
    Equal,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum EntityRuleArg {
    Any,
    Shared2,
}

struct ContendOpts<'a> {
    exclude_top: usize,
    exclude_pool: &'a [String],
    rule: BotRule,
    entities: bool,
    entity_rule: EntityRule,
    run: RunOptions,
    drop_entity: &'a [String],
    loo_top: usize,
    drop_blocks_without_rows: bool,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Classify {
            file,
            threshold,
            top_n,
        } => classify(&file, threshold, top_n),
        Command::Sample {
            stratum,
            per_hour,
            exclude,
            neighbors,
            chain,
            out,
        } => sample(&stratum, per_hour, exclude, neighbors, chain, &out),
        Command::Fetch {
            blocks,
            split,
            block,
            seal,
            raw_dir,
            out,
            workers,
            max_blocks,
            offline,
            chain,
            v4_pools,
            post_seal,
        } => fetch(
            &blocks,
            split,
            &block,
            seal,
            &raw_dir,
            &out,
            workers,
            max_blocks,
            FetchOpts {
                offline,
                chain,
                v4_pools,
                post_seal,
            },
        ),
        Command::Race {
            rows,
            blocks,
            split,
            raw,
            bins,
            cell_block,
            boot_unit,
            bootstrap,
            permutations,
            seed,
            sin_probes_only,
            max_sender_degree,
            af04,
            chain,
        } => {
            if bins == 0 {
                bail!("--bins must be at least 1");
            }
            let params = race::RaceParams {
                bootstrap,
                permutations,
                seed,
                week_unit: boot_unit == BootUnit::Week,
                cell_block,
                bins,
            };
            let ctx = chain.resolve()?;
            if af04.active() {
                return run_race_af04(
                    &rows,
                    &blocks,
                    split,
                    &raw,
                    &params,
                    sin_probes_only,
                    max_sender_degree,
                    &af04,
                    &ctx,
                );
            }
            run_race(
                &rows,
                &blocks,
                split,
                &raw,
                &params,
                sin_probes_only,
                max_sender_degree,
                &ctx,
            )
        }
        Command::RaceContrast {
            a,
            b,
            variant,
            bootstrap,
            seed,
        } => run_race_contrast(&a, &b, &variant, bootstrap, seed),
        Command::Neighbor {
            rows,
            blocks,
            split,
            boot_unit,
            bootstrap,
            seed,
            drop_blocks_without_rows,
            chain,
        } => {
            let ctx = chain.resolve()?;
            let params = neighbor::NeighborParams {
                bootstrap,
                seed,
                week_unit: boot_unit == BootUnit::Week,
                drop_blocks_without_rows,
            };
            run_neighbor(&rows, &blocks, split, &params, &ctx)
        }
        Command::Contend {
            rows,
            blocks,
            split,
            permutations,
            bootstrap,
            bootstrap_permutations,
            seed,
            exclude_top_pools,
            exclude_pool,
            require_cyclic,
            min_failed_share,
            no_entities,
            metric,
            null,
            drop_entity,
            loo_top,
            entity_weight,
            entity_rule,
            max_sender_degree,
            bootstrap_block_hours,
            jackknife_days,
            drop_blocks_without_rows,
            chain,
        } => {
            let ctx = chain.resolve()?;
            if !matches!(bootstrap_block_hours, 1 | 6 | 12) {
                anyhow::bail!("--bootstrap-block-hours must be 6 or 12 (default 1: single hours)");
            }
            let params = ContendParams {
                permutations,
                bootstrap,
                bootstrap_permutations,
                seed,
                metric: match metric {
                    MetricArg::Binary => Metric::Binary,
                    MetricArg::Weighted => Metric::Weighted,
                },
            };
            let opts = ContendOpts {
                exclude_top: exclude_top_pools,
                exclude_pool: &exclude_pool,
                rule: BotRule {
                    require_cyclic,
                    min_failed_share,
                    ..BotRule::default()
                },
                entities: !no_entities,
                entity_rule: EntityRule {
                    min_shared: match entity_rule {
                        EntityRuleArg::Any => 1,
                        EntityRuleArg::Shared2 => 2,
                    },
                    max_sender_degree,
                },
                run: RunOptions {
                    equal_entity_weight: matches!(entity_weight, EntityWeightArg::Equal),
                    block_hours: bootstrap_block_hours,
                    jackknife_days,
                },
                drop_entity: &drop_entity,
                loo_top,
                drop_blocks_without_rows,
            };
            let null = match null {
                NullArg::Primary => NullKind::Primary,
                NullArg::Active => NullKind::Active,
            };
            run_contend(&rows, &blocks, split, params, null, &opts, &ctx)
        }
    }
}

fn classify(file: &str, threshold: usize, top_n: usize) -> Result<()> {
    let start = Instant::now();
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(file)
        .with_context(|| format!("No se pudo abrir {}", file))?;
    let mut transactions: Vec<DuneTransaction> = Vec::new();
    let mut skipped = 0usize;
    for result in reader.deserialize::<DuneTransaction>() {
        match result {
            Ok(record) if is_usable(&record) => transactions.push(record),
            Ok(_) => skipped += 1,
            Err(e) => {
                skipped += 1;
                eprintln!("Warning: skipping malformed row: {}", e);
            }
        }
    }
    if transactions.is_empty() {
        bail!("El archivo CSV no contiene filas validas con bot_address y block_number.");
    }
    let report = analyze(&transactions, threshold, top_n);
    print_report(file, skipped, &report);
    println!();
    println!("Processing time: {}ms", start.elapsed().as_millis());
    Ok(())
}

fn parse_range(s: &str) -> Result<(u64, u64)> {
    let (a, b) = s.split_once('-').context("range must be FIRST-LAST")?;
    Ok((a.trim().parse()?, b.trim().parse()?))
}

fn sample(
    strata: &[String],
    per_hour: usize,
    exclude: Option<String>,
    neighbors: bool,
    chain: Chain,
    out: &Path,
) -> Result<()> {
    let exclude = exclude.as_deref().map(parse_range).transpose()?;
    let timing = Timing {
        genesis_ts: chain.genesis_ts(),
        block_time: chain.block_time(),
    };
    if neighbors {
        return sample_neighbors(strata, exclude, timing, out);
    }
    let mut all = Vec::new();
    for s in strata {
        let (name, date) = s
            .split_once('=')
            .context("stratum must be NAME=YYYY-MM-DD")?;
        let ts = parse_date(date).with_context(|| format!("bad date {date}"))?;
        all.extend(sample_week_on(timing, name, ts, per_hour, exclude));
    }
    let mut w = csv::Writer::from_path(out)?;
    w.write_record(["stratum", "block", "hour_start_ts", "exploratory"])?;
    for b in &all {
        w.write_record([
            b.stratum.clone(),
            b.block.to_string(),
            b.hour_start_ts.to_string(),
            b.exploratory.to_string(),
        ])?;
    }
    w.flush()?;
    let explore = all.iter().filter(|b| b.exploratory).count();
    eprintln!(
        "sample: {} blocks ({} exploratory, {} confirmatory) -> {}",
        all.len(),
        explore,
        all.len() - explore,
        out.display()
    );
    Ok(())
}

fn sample_neighbors(
    strata: &[String],
    exclude: Option<(u64, u64)>,
    timing: Timing,
    out: &Path,
) -> Result<()> {
    let mut all = Vec::new();
    for s in strata {
        let (name, date) = s
            .split_once('=')
            .context("stratum must be NAME=YYYY-MM-DD")?;
        let ts = parse_date(date).with_context(|| format!("bad date {date}"))?;
        all.extend(sample_week_neighbors_on(timing, name, ts, exclude));
    }
    let mut w = csv::Writer::from_path(out)?;
    w.write_record([
        "stratum",
        "block",
        "hour_start_ts",
        "exploratory",
        "role",
        "anchor",
    ])?;
    for n in &all {
        w.write_record([
            n.block.stratum.clone(),
            n.block.block.to_string(),
            n.block.hour_start_ts.to_string(),
            n.block.exploratory.to_string(),
            n.role.as_str().to_string(),
            n.anchor.to_string(),
        ])?;
    }
    w.flush()?;
    let anchors = all.iter().filter(|n| n.role == Role::Anchor).count();
    let explore = all
        .iter()
        .filter(|n| n.role == Role::Anchor && n.block.exploratory)
        .count();
    eprintln!(
        "sample: {} anchors ({} exploratory, {} confirmatory) and {} neighbors -> {}",
        anchors,
        explore,
        anchors - explore,
        all.len() - anchors,
        out.display()
    );
    Ok(())
}

fn read_sample(path: &Path) -> Result<Vec<SampledBlock>> {
    let mut r =
        csv::Reader::from_path(path).with_context(|| format!("no sample at {}", path.display()))?;
    let mut v = Vec::new();
    for rec in r.records() {
        let rec = rec?;
        v.push(SampledBlock {
            stratum: rec[0].to_string(),
            block: rec[1].parse()?,
            hour_start_ts: rec[2].parse()?,
            exploratory: &rec[3] == "true",
        });
    }
    Ok(v)
}

fn selected(sample: &[SampledBlock], split: Split) -> Vec<&SampledBlock> {
    sample
        .iter()
        .filter(|b| match split {
            Split::Exploratory => b.exploratory,
            Split::Confirmatory => !b.exploratory,
            Split::All => true,
        })
        .collect()
}

fn load_api_key() -> Result<String> {
    if let Ok(k) = std::env::var("ALCHEMY_API_KEY") {
        if !k.is_empty() {
            return Ok(k);
        }
    }
    let text = fs::read_to_string(".env").context("ALCHEMY_API_KEY not set and no .env")?;
    text.lines()
        .find_map(|l| {
            l.strip_prefix("ALCHEMY_API_KEY=")
                .map(|v| v.trim().to_string())
        })
        .filter(|k| !k.is_empty())
        .context("ALCHEMY_API_KEY missing in .env")
}

struct FetchOpts {
    offline: bool,
    chain: Chain,
    v4_pools: bool,
    post_seal: bool,
}

#[allow(clippy::too_many_arguments)]
fn fetch(
    blocks_csv: &Path,
    split: Split,
    explicit: &[u64],
    seal: Option<PathBuf>,
    raw_dir: &Path,
    out: &Path,
    workers: usize,
    max_blocks: Option<usize>,
    opts: FetchOpts,
) -> Result<()> {
    let FetchOpts {
        offline,
        chain,
        v4_pools,
        post_seal,
    } = opts;
    let mut blocks: Vec<u64> = if explicit.is_empty() {
        let sample = read_sample(blocks_csv)?;
        if split != Split::Exploratory {
            let seal =
                seal.context("confirmatory blocks need --seal (preregistro sellado, [PRE-02])")?;
            if post_seal {
                let changed = verify_seal_post(&seal, Path::new("."))?;
                if changed.is_empty() {
                    eprintln!("seal: ok ({})", seal.display());
                } else {
                    eprintln!(
                        "seal: AVISO desviacion post-sello: preregistro y muestra intactos, pero cambiaron {} archivos sellados:",
                        changed.len()
                    );
                    for p in &changed {
                        eprintln!("  {p}");
                    }
                }
            } else {
                verify_seal(&seal, Path::new("."))?;
                eprintln!("seal: ok ({})", seal.display());
            }
        }
        selected(&sample, split).iter().map(|b| b.block).collect()
    } else {
        explicit.to_vec()
    };
    if let Some(n) = max_blocks {
        blocks.truncate(n);
    }
    let rpc = if offline {
        None
    } else {
        Some(Rpc::alchemy(chain, &load_api_key()?))
    };
    let start = Instant::now();
    let outcome = fetch_blocks(
        rpc,
        raw_dir,
        &blocks,
        workers,
        v4_pools.then(|| chain.pool_manager()),
    )?;

    let mut w = csv::Writer::from_path(out)?;
    for r in &outcome.rows {
        w.serialize(r)?;
    }
    w.flush()?;
    let manifest = out.with_extension("manifest.csv");
    let mut m = csv::Writer::from_path(&manifest)?;
    m.write_record(["block", "kind", "sha256"])?;
    for (b, k, h) in &outcome.manifest {
        m.write_record([b.to_string(), k.clone(), h.clone()])?;
    }
    m.flush()?;
    let rows_hash = sha256_hex(&fs::read(out)?);
    eprintln!(
        "fetch: {} blocks, {} rows, {} RPC calls, {} failures, {:.1}s; rows sha256 {}",
        blocks.len(),
        outcome.rows.len(),
        outcome.fetched_calls,
        outcome.failures.len(),
        start.elapsed().as_secs_f64(),
        rows_hash
    );
    for (b, e) in outcome.failures.iter().take(10) {
        eprintln!("  failed block {b}: {e}");
    }
    Ok(())
}

/// ERR-12 guard: rows extracted with `--v4-pools` carry `v4:<PoolId>` pseudo-pools, and reading
/// them without `--v4-swap-pools` makes no v4 swap count as an earlier swap. Warns on STDERR only,
/// so stdout stays byte-identical.
fn warn_v4_rows(rows: &[TxRow], v4_swap_pools: bool) {
    if v4_swap_pools {
        return;
    }
    let n = rows.iter().filter(|r| r.pools.contains("v4:")).count();
    if n > 0 {
        eprintln!(
            "warning: {n} rows carry v4:<PoolId> pseudo-pools but --v4-swap-pools is not set; \
             Swap logs of the PoolManager will not count as earlier swaps (ERR-12). \
             Pass --v4-swap-pools to read them as pseudo-pools."
        );
    }
}

fn read_rows(path: &Path) -> Result<Vec<TxRow>> {
    let mut r =
        csv::Reader::from_path(path).with_context(|| format!("no rows at {}", path.display()))?;
    r.deserialize().map(|x| x.map_err(Into::into)).collect()
}

fn run_contend(
    rows_path: &Path,
    blocks_csv: &Path,
    split: Split,
    params: ContendParams,
    null: NullKind,
    opts: &ContendOpts,
    ctx: &ChainCtx,
) -> Result<()> {
    let rows = read_rows(rows_path)?;
    let sample = read_sample(blocks_csv)?;
    let mut strata: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    ctx.check_sample(&selected(&sample, split))?;
    for b in selected(&sample, split) {
        strata.entry(&b.stratum).or_default().push(b.block);
    }
    let rule = &opts.rule;
    let (exclude_top, exclude_pool) = (opts.exclude_top, opts.exclude_pool);
    println!(
        "mev-revert-classifier v{} contend",
        env!("CARGO_PKG_VERSION")
    );
    println!("rows sha256 {}", sha256_hex(&fs::read(rows_path)?));
    println!(
        "metric {:?} | permutations {} | bootstrap {} x {} | seed {} | exclude top pools {} | extra excluded {} | require cyclic {} | min failed share {} | entities {}",
        params.metric, params.permutations, params.bootstrap, params.bootstrap_permutations, params.seed, exclude_top, exclude_pool.len(), rule.require_cyclic, rule.min_failed_share, opts.entities
    );
    let extra = opts.drop_entity.len()
        + opts.loo_top
        + opts.drop_blocks_without_rows as usize
        + (opts.run.equal_entity_weight
            || opts.run.block_hours > 1
            || opts.run.jackknife_days
            || opts.entity_rule != EntityRule::default()) as usize;
    if extra > 0 {
        println!(
            "variants: entity weight {} | entity rule min shared {} max sender degree {:?} | bootstrap block hours {} | jackknife days {} | drop entities {:?} | loo top {} | drop blocks without rows {}",
            if opts.run.equal_entity_weight { "equal" } else { "pooled" },
            opts.entity_rule.min_shared,
            opts.entity_rule.max_sender_degree,
            opts.run.block_hours,
            opts.run.jackknife_days,
            opts.drop_entity,
            opts.loo_top,
            opts.drop_blocks_without_rows
        );
    }
    ctx.describe(&rows);
    // Blocks with rows are counted before the filter: a block with only system rows is
    // downloaded, not empty.
    let rows_blocks: HashSet<u64> = rows.iter().map(|r| r.block).collect();
    let rows = match &ctx.system {
        Some(f) => f.apply(&rows).0,
        None => rows,
    };
    let mut results = Vec::new();
    let mut loo_tables = Vec::new();
    for (name, blocks) in &strata {
        let blocks: Vec<u64> = if opts.drop_blocks_without_rows {
            let kept: Vec<u64> = blocks
                .iter()
                .copied()
                .filter(|b| rows_blocks.contains(b))
                .collect();
            println!(
                "{name}: dropped {} of {} blocks without rows",
                blocks.len() - kept.len(),
                blocks.len()
            );
            kept
        } else {
            blocks.clone()
        };
        let blocks = &blocks;
        let set: HashSet<u64> = blocks.iter().copied().collect();
        let mine: Vec<TxRow> = rows
            .iter()
            .filter(|r| set.contains(&r.block))
            .cloned()
            .collect();
        let bots = contend::bot_universe(&mine, rule);
        let mut excluded = contend::top_pools(&mine, &bots, exclude_top);
        excluded.extend(exclude_pool.iter().map(|p| p.to_ascii_lowercase()));
        let mut entity_of = if opts.entities {
            contend::entities_with(&mine, &bots, opts.entity_rule)
        } else {
            bots.iter().map(|b| (b.clone(), b.clone())).collect()
        };
        if opts.entity_rule != EntityRule::default() {
            let n: HashSet<&String> = entity_of.values().collect();
            println!(
                "{name}: {} bot contracts -> {} entities",
                entity_of.len(),
                n.len()
            );
        }
        for addr in opts.drop_entity {
            let addr = addr.to_ascii_lowercase();
            let rep = entity_of
                .get(&addr)
                .cloned()
                .or_else(|| entity_of.values().find(|v| **v == addr).cloned());
            match rep {
                Some(rep) => {
                    entity_of.retain(|_, v| *v != rep);
                    println!("{name}: dropped entity {rep}");
                }
                None => println!("{name}: --drop-entity {addr} is not a bot entity here"),
            }
        }
        let analyse = |entity_of: &HashMap<String, String>| {
            let ds = contend::build_entities_on(ctx.timing, &mine, blocks, entity_of, &excluded);
            contend::run_ext(&ds, params, null, &opts.run)
        };
        results.push((name.to_string(), analyse(&entity_of)));
        if opts.loo_top > 0 {
            // Failed attempts with pools (after pool exclusions) per entity.
            let mut failed: HashMap<&str, usize> = HashMap::new();
            for r in &mine {
                let Some(e) = entity_of.get(&r.to) else {
                    continue;
                };
                if matches!(r.kind, Kind::Probe | Kind::Revert)
                    && r.pool_list().iter().any(|p| !excluded.contains(*p))
                {
                    *failed.entry(e).or_default() += 1;
                }
            }
            let all: usize = failed.values().sum();
            let mut top: Vec<(&str, usize)> = failed.into_iter().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            let mut table = Vec::new();
            for (rep, n) in top.into_iter().take(opts.loo_top) {
                let mut without = entity_of.clone();
                without.retain(|_, v| v != rep);
                let r = analyse(&without);
                table.push((rep.to_string(), n as f64 / all as f64, r));
            }
            loo_tables.push((name.to_string(), table));
        }
    }
    let adjusted = contend::holm(&results.iter().map(|(_, r)| r.p_value).collect::<Vec<_>>());
    if null == NullKind::Active {
        // Header line only for the second null, so the default output is unchanged.
        println!("null active (permutation among the entity's active blocks of the hour)");
    }
    println!();
    println!(
        "{:<8} {:>6} {:>5} {:>7} {:>6} {:>17} {:>7} {:>17} {:>7} {:>17} {:>8} {:>8}",
        "stratum",
        "blocks",
        "bots",
        "failed",
        "cover",
        "CR_obs [95% CI]",
        "null",
        "null 95% band",
        "E",
        "E [95% CI]",
        "p",
        "p_holm"
    );
    for ((name, r), ph) in results.iter().zip(adjusted) {
        println!(
            "{:<8} {:>6} {:>5} {:>7} {:>6.3} {:>6.3} [{:.3},{:.3}] {:>7.3} [{:.3},{:.3}] {:>7.3} [{:.3},{:.3}] {:>8.4} {:>8.4}",
            name, r.blocks, r.bots, r.failed, r.coverage, r.cr_obs, r.cr_ci.0, r.cr_ci.1, r.null_mean,
            r.null_band.0, r.null_band.1, r.excess, r.excess_ci.0, r.excess_ci.1, r.p_value, ph
        );
    }
    if opts.run.jackknife_days {
        println!();
        println!("jackknife by day (E with the day left out):");
        for (name, r) in &results {
            let cells: Vec<String> = r
                .jackknife
                .iter()
                .map(|(d, e)| format!("{}={:.4}", civil_date(*d), e))
                .collect();
            println!("{:<8} SE {:.4} | {}", name, r.jackknife_se, cells.join(" "));
        }
    }
    for (name, table) in &loo_tables {
        println!();
        println!("leave-one-out, stratum {name}:");
        println!(
            "{:<44} {:>13} {:>8} {:>17}",
            "entidad_quitada", "share_fallidas", "E", "IC"
        );
        for (rep, share, r) in table {
            println!(
                "{:<44} {:>13.3} {:>8.4} [{:.4},{:.4}]",
                rep, share, r.excess, r.excess_ci.0, r.excess_ci.1
            );
        }
    }
    if null == NullKind::Active {
        println!();
        println!("share of failed attempts in entity-hours with >= 2 active blocks (movable by the null):");
        for (name, r) in &results {
            println!("{:<8} {:>6.3}", name, r.active_share);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_race(
    rows_path: &Path,
    blocks_csv: &Path,
    split: Split,
    raw: &Path,
    params: &race::RaceParams,
    sin_probes_only: bool,
    max_sender_degree: Option<usize>,
    ctx: &ChainCtx,
) -> Result<()> {
    let rows = read_rows(rows_path)?;
    warn_v4_rows(&rows, false);
    let sample = read_sample(blocks_csv)?;
    let mut strata: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    ctx.check_sample(&selected(&sample, split))?;
    for b in selected(&sample, split) {
        strata.entry(&b.stratum).or_default().push(b.block);
    }
    println!("mev-revert-classifier v{} race", env!("CARGO_PKG_VERSION"));
    println!("rows sha256 {}", sha256_hex(&fs::read(rows_path)?));
    println!(
        "# seed={} B={} K={} boot-unit={} bins={} cells={}; one-sided p (MH > 0); POOL = cells nested in strata, intervals by {}",
        params.seed,
        params.bootstrap,
        params.permutations,
        if params.week_unit { "week" } else { "hour" },
        params.bins,
        if params.cell_block { "entity x block" } else { "entity x bin" },
        if params.week_unit { "whole strata" } else { "hours within strata" },
    );
    ctx.describe(&rows);
    if let Some(k) = max_sender_degree {
        println!("entities: senders with degree >= {k} ignored (max sender degree {k})");
    }
    let opts = race::StratumOpts {
        timing: ctx.timing,
        max_sender_degree,
        system: ctx.system.as_ref(),
    };
    let mut data = Vec::new();
    let mut missing = 0usize;
    for (name, blocks) in &strata {
        let set: HashSet<u64> = blocks.iter().copied().collect();
        let mine: Vec<TxRow> = rows
            .iter()
            .filter(|r| set.contains(&r.block))
            .cloned()
            .collect();
        let (d, miss) = race::build_stratum_with(name, &mine, blocks, raw, params.bins, &opts)?;
        missing += miss.len();
        data.push(d);
    }
    let variants: &[(&str, bool)] = if sin_probes_only {
        &[("sin_probes", false)]
    } else {
        &[("sin_probes", false), ("con_probes", true)]
    };
    for (vi, (label, with_probes)) in variants.iter().enumerate() {
        for line in race::analyze(&data, vi as u64, *with_probes, params) {
            if line.n > 0 {
                println!("{}", race::format_line(label, &line));
            }
        }
    }
    println!("bloques sin crudo: {missing}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_race_af04(
    rows_path: &Path,
    blocks_csv: &Path,
    split: Split,
    raw: &Path,
    params: &race::RaceParams,
    sin_probes_only: bool,
    max_sender_degree: Option<usize>,
    a: &Af04Args,
    ctx: &ChainCtx,
) -> Result<()> {
    use mev_revert_classifier::placebo::{self as pl, PRule, XAlso, XDef};
    use mev_revert_classifier::robust;
    let rows = read_rows(rows_path)?;
    warn_v4_rows(&rows, a.v4_swap_pools);
    let sample = read_sample(blocks_csv)?;
    let mut strata: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    ctx.check_sample(&selected(&sample, split))?;
    for b in selected(&sample, split) {
        strata.entry(&b.stratum).or_default().push(b.block);
    }
    println!("mev-revert-classifier v{} race", env!("CARGO_PKG_VERSION"));
    println!("rows sha256 {}", sha256_hex(&fs::read(rows_path)?));
    println!(
        "# seed={} B={} K={} boot-unit={} bins={} cells={}; one-sided p (MH > 0); POOL = cells nested in strata, intervals by {}",
        params.seed,
        params.bootstrap,
        params.permutations,
        if params.week_unit { "week" } else { "hour" },
        params.bins,
        if params.cell_block { "entity x block" } else { "entity x bin" },
        if params.week_unit { "whole strata" } else { "hours within strata" },
    );
    ctx.describe(&rows);
    if let Some(k) = max_sender_degree {
        println!("entities: senders with degree >= {k} ignored (max sender degree {k})");
    }
    let variant = pl::Variant {
        x_def: match a.x_def {
            None | Some(XDefArg::PriorOther) => XDef::PriorOther,
            Some(XDefArg::LaterOther) => XDef::LaterOther,
            Some(XDefArg::DonorPool) => XDef::DonorPool,
            Some(XDefArg::PriorSame) => XDef::PriorSame,
            Some(XDefArg::UserPrior) => XDef::UserPrior,
            Some(XDefArg::AnyPrior) => XDef::AnyPrior,
        },
        also: match a.x_also {
            None | Some(XAlsoArg::None) => XAlso::None,
            Some(XAlsoArg::PriorOther) => XAlso::PriorOther,
            Some(XAlsoArg::UserPrior) => XAlso::UserPrior,
            Some(XAlsoArg::PriorSame) => XAlso::PriorSame,
            Some(XAlsoArg::LaterOther) => XAlso::LaterOther,
        },
        p_rule: match a.p_rule {
            None | Some(PRuleArg::Trace) => PRule::Trace,
            Some(PRuleArg::Contract) => PRule::Contract,
            Some(PRuleArg::ContractSubset) => PRule::ContractSubset,
            Some(PRuleArg::TraceOnContractSubset) => PRule::TraceOnContractSubset,
        },
        p_size: a.p_size_strata,
        donor_subset: a.donor_subset,
    };
    println!(
        "af04 variant: {} bot-share={} donor-seed={} v4-swap-pools={}",
        variant.label(),
        a.bot_share.map_or("0.5".to_string(), |v| v.to_string()),
        a.donor_seed,
        a.v4_swap_pools
    );
    let opts = race::StratumOpts {
        timing: ctx.timing,
        max_sender_degree,
        system: ctx.system.as_ref(),
    };
    let eo = pl::ExtOpts {
        want_ext: true,
        bot_share: a.bot_share,
        donor_seed: a.donor_seed,
        v4_pool_manager: a.v4_swap_pools.then(|| ctx.chain.pool_manager()),
    };
    let (mut data, mut exts, mut missing) = (Vec::new(), Vec::new(), 0usize);
    for (name, blocks) in &strata {
        let set: HashSet<u64> = blocks.iter().copied().collect();
        let mine: Vec<TxRow> = rows
            .iter()
            .filter(|r| set.contains(&r.block))
            .cloned()
            .collect();
        let (d, e, miss) =
            race::build_stratum_ext(name, &mine, blocks, raw, params.bins, &opts, &eo)?;
        missing += miss.len();
        data.push(d);
        exts.push(e.expect("ext requested"));
    }
    let (vdata, vs) = pl::apply_variant(&data, &exts, &variant);
    println!(
        "af04 coverage: universe(non-probe)={} kept={} share={:.4} empty-P bot tx (non-probe): reverts={} successes={}",
        vs.universe,
        vs.kept,
        if vs.universe > 0 { vs.kept as f64 / vs.universe as f64 } else { f64::NAN },
        exts.iter().flat_map(|e| &e.empty).filter(|e| !e.tx.probe && e.tx.y).count(),
        exts.iter().flat_map(|e| &e.empty).filter(|e| !e.tx.probe && !e.tx.y).count(),
    );
    let label = if variant.is_default() {
        "sin_probes".to_string()
    } else {
        "sin_probes*".to_string()
    };
    let variants: &[(&str, bool)] = if sin_probes_only {
        &[("sin_probes", false)]
    } else {
        &[("sin_probes", false), ("con_probes", true)]
    };
    for (vi, (l, with_probes)) in variants.iter().enumerate() {
        let l = if *l == "sin_probes" {
            label.as_str()
        } else {
            l
        };
        for line in race::analyze(&vdata, vi as u64, *with_probes, params) {
            if line.n > 0 {
                println!("{}", race::format_line(l, &line));
            }
        }
    }
    println!("bloques sin crudo: {missing}");
    let f = |v: f64| {
        if v.is_nan() {
            "n/a".to_string()
        } else {
            format!("{v:+.4}")
        }
    };
    if let Some(path) = &a.emit_weeks {
        let st = pl::week_stats(&vdata, false, params);
        let mut out = String::new();
        for (s, (n, d)) in vdata.iter().zip(&st) {
            out.push_str(&format!("sin_probes\t{}\t{:?}\t{:?}\n", s.name, n, d));
        }
        fs::write(path, out)?;
        println!("af04 weeks written: {}", path.display());
    }
    if a.missing_grid {
        let g = pl::missing_grid(&data, &exts, 0.02, params);
        println!("af04 missing-grid (no probes; empty-P bot tx: reverts={} successes={}); rows q_R, columns q_S = 0.0..1.0", g.n_empty_rev, g.n_empty_suc);
        for (i, row) in g.points.chunks(11).enumerate() {
            println!(
                "af04 grid q_R={:.1} {}",
                i as f64 / 10.0,
                row.iter().map(|p| f(p.d)).collect::<Vec<_>>().join(" ")
            );
        }
        println!(
            "af04 grid min={} at q_R={:.1} q_S={:.1} [{},{}] max={} at q_R={:.1} q_S={:.1} tipping_q_R={}",
            f(g.min.d), g.min.q_r, g.min.q_s, f(g.worst_ci.0), f(g.worst_ci.1),
            f(g.max.d), g.max.q_r, g.max.q_s,
            g.tipping.map_or("none".to_string(), |q| format!("{q:.1}")),
        );
    }
    if a.decompose {
        let r = robust::decompose(&vdata, 0, params);
        let o = &r.obs;
        println!(
            "af04 decompose: dec={} mh={} gap={} comp={} [{},{}] wgt={} [{},{}] sup={} [{},{}] max_residual={:.3e} verdict={}",
            f(o.dec), f(o.mh), f(o.gap), f(o.comp), f(r.ci_comp.0), f(r.ci_comp.1),
            f(o.wgt), f(r.ci_wgt.0), f(r.ci_wgt.1), f(o.sup), f(r.ci_sup.0), f(r.ci_sup.1),
            r.max_resid, r.verdict
        );
        let ew = robust::entity_weeks(&vdata);
        println!(
            "af04 entity-weeks (>=30 tx per group): positive={} negative={} zero={} skipped={}; MH weight positive={:.4} negative={:.4} zero={:.4} skipped={:.4}",
            ew.pos, ew.neg, ew.zero, ew.skipped,
            ew.w_pos, ew.w_neg, ew.w_zero, ew.w_skipped
        );
    }
    if a.loo.is_some() || a.decompose {
        let keys: Vec<Vec<String>> = exts.iter().map(|e| e.ent_keys.clone()).collect();
        let conc = robust::concentration(&vdata, &keys);
        println!(
            "af04 concentration: keys={} hhi={:.4} top5={}",
            conc.top.len(),
            conc.hhi,
            conc.top
                .iter()
                .take(5)
                .map(|(k, s)| format!("{k}:{s:.4}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        if let Some(k) = a.loo {
            for (key, share, d, ci) in robust::leave_one_out(&vdata, &keys, k, params) {
                println!(
                    "af04 loo without {key} (weight share {share:.4}): {} [{},{}]",
                    f(d),
                    f(ci.0),
                    f(ci.1)
                );
            }
        }
    }
    if a.het || !a.het_groups.is_empty() {
        let mut groups: Vec<(String, Vec<usize>)> =
            vec![("ALL".to_string(), (0..vdata.len()).collect())];
        for g in &a.het_groups {
            groups.push((
                g.clone(),
                (0..vdata.len())
                    .filter(|&i| vdata[i].name.contains(g.as_str()))
                    .collect(),
            ));
        }
        for (g, idx) in groups {
            let sub: Vec<_> = idx.iter().map(|&i| vdata[i].clone()).collect();
            let est = robust::week_estimates(&sub, 0, params);
            let index: Vec<f64> = (0..est.len()).map(|i| i as f64).collect();
            match robust::dersimonian_laird(&est, &index) {
                Some(h) => println!(
                    "af04 het[{g}] k={} fe={} Q={:.3} tau2={:.6} I2={:.3} re={} se={:.4} ci=[{},{}] pred=[{},{}] slope={:+.6} spearman={:+.3}",
                    h.k, f(h.fe), h.q, h.tau2, h.i2, f(h.re), h.re_se, f(h.re_ci.0), f(h.re_ci.1), f(h.pred.0), f(h.pred.1), h.slope, h.spearman
                ),
                None => println!("af04 het[{g}] n/a (fewer than 2 strata)"),
            }
        }
    }
    Ok(())
}

fn run_race_contrast(a: &Path, b: &Path, variant: &str, bootstrap: usize, seed: u64) -> Result<()> {
    use mev_revert_classifier::placebo as pl;
    let wa = pl::read_weeks(&fs::read_to_string(a)?, variant);
    let wb = pl::read_weeks(&fs::read_to_string(b)?, variant);
    let names: std::collections::BTreeSet<&String> = wa.keys().chain(wb.keys()).collect();
    let get = |m: &BTreeMap<String, (f64, f64)>| -> Vec<(f64, f64)> {
        names
            .iter()
            .map(|n| m.get(*n).copied().unwrap_or((0.0, 0.0)))
            .collect()
    };
    let p = race::RaceParams {
        bootstrap,
        seed,
        ..race::RaceParams::default()
    };
    let (diff, ci, ea, eb) = pl::contrast(&get(&wa), &get(&wb), 0, &p);
    println!(
        "mev-revert-classifier v{} race-contrast",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "# a={} b={} strata={} (union) B={} seed={}; joint bootstrap of whole strata",
        a.display(),
        b.display(),
        names.len(),
        bootstrap,
        seed
    );
    println!(
        "a={ea:+.4} b={eb:+.4} a-b={diff:+.4} [{:+.4},{:+.4}]",
        ci.0, ci.1
    );
    Ok(())
}

fn run_neighbor(
    rows_path: &Path,
    blocks_csv: &Path,
    split: Split,
    params: &neighbor::NeighborParams,
    ctx: &ChainCtx,
) -> Result<()> {
    let rows = read_rows(rows_path)?;
    let design = neighbor::read_neighbor_sample(blocks_csv)?;
    ctx.check_sample(&design.iter().map(|n| &n.block).collect::<Vec<_>>())?;
    let mut strata: BTreeMap<&str, Vec<&mev_revert_classifier::sample::NeighborBlock>> =
        BTreeMap::new();
    for n in &design {
        let keep = match split {
            Split::Exploratory => n.block.exploratory,
            Split::Confirmatory => !n.block.exploratory,
            Split::All => true,
        };
        if keep {
            strata.entry(&n.block.stratum).or_default().push(n);
        }
    }
    println!(
        "mev-revert-classifier v{} neighbor",
        env!("CARGO_PKG_VERSION")
    );
    println!("rows sha256 {}", sha256_hex(&fs::read(rows_path)?));
    println!(
        "# seed={} B={} boot-unit={} drop blocks without rows {}; universe, entities and weights on anchor blocks only",
        params.seed,
        params.bootstrap,
        if params.week_unit { "week" } else { "hour" },
        params.drop_blocks_without_rows
    );
    ctx.describe(&rows);
    let nb_opts = neighbor::NbOpts {
        timing: ctx.timing,
        system: ctx.system.as_ref(),
    };
    let mut results = Vec::new();
    for (name, d) in &strata {
        results.push(neighbor::analyze_stratum_on(
            name, d, &rows, params, &nb_opts,
        )?);
    }
    let summary = neighbor::summarize(&results, params);
    let fmt = |v: f64| {
        if v.is_nan() {
            "n/a".to_string()
        } else {
            format!("{v:.4}")
        }
    };
    let fmt_s = |v: f64| {
        if v.is_nan() {
            "n/a".to_string()
        } else {
            format!("{v:+.4}")
        }
    };
    println!();
    println!(
        "{:<6} {:>7} {:>6} {:>8} {:>7} {:>17}",
        "stratum", "anchors", "hours", "attempts", "C(0)", "C(0) [95% CI]"
    );
    let pooled_n = results.len();
    for (i, (st, ci)) in summary.iter().enumerate() {
        let (name, anchors, hours, attempts) = if i < pooled_n {
            let r = &results[i];
            (r.name.as_str(), r.anchors, r.hours.len(), r.attempts)
        } else {
            (
                "POOL",
                results.iter().map(|r| r.anchors).sum(),
                results.iter().map(|r| r.hours.len()).sum(),
                results.iter().map(|r| r.attempts).sum(),
            )
        };
        println!(
            "{:<6} {:>7} {:>6} {:>8} {} [{},{}]",
            name,
            anchors,
            hours,
            attempts,
            fmt(st[0]),
            fmt(ci[0].0),
            fmt(ci[0].1)
        );
    }
    println!();
    println!(
        "{:<6} {:>6} {:>7} {:>7} {:>8} {:>8} {:>17} {:>9} {:>19}",
        "stratum",
        "offset",
        "anchors",
        "pairs",
        "C(0)|k",
        "C(k)",
        "C(k) [95% CI]",
        "U_k",
        "U_k [95% CI]"
    );
    for (i, (st, ci)) in summary.iter().enumerate() {
        for (o, label) in neighbor::OFFSET_LABELS.iter().enumerate() {
            let (name, anchors, pairs, empty) = if i < pooled_n {
                let r = &results[i];
                (
                    r.name.as_str(),
                    r.anchors_with[o],
                    r.pairs(o),
                    r.empty_neighbors[o],
                )
            } else {
                (
                    "POOL",
                    results.iter().map(|r| r.anchors_with[o]).sum(),
                    results.iter().map(|r| r.pairs(o)).sum(),
                    results.iter().map(|r| r.empty_neighbors[o]).sum(),
                )
            };
            if anchors == 0 {
                continue;
            }
            println!(
                "{:<6} {:>6} {:>7} {:>7} {:>8} {:>8} [{},{}] {:>9} [{},{}]  (neighbors with no rows: {})",
                name,
                label,
                anchors,
                pairs as u64,
                fmt(st[1 + 3 * o]),
                fmt(st[2 + 3 * o]),
                fmt(ci[2 + 3 * o].0),
                fmt(ci[2 + 3 * o].1),
                fmt_s(st[3 + 3 * o]),
                fmt_s(ci[3 + 3 * o].0),
                fmt_s(ci[3 + 3 * o].1),
                empty
            );
        }
    }
    Ok(())
}

/// Date (UTC) of a day count since the Unix epoch.
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02}")
}

fn print_report(file: &str, skipped: usize, r: &Report) {
    let line = "=".repeat(64);
    println!("mev-revert-classifier v{}", env!("CARGO_PKG_VERSION"));
    println!("{line}");
    println!("File:            {}", file);
    println!("Rows processed:  {} ({} skipped)", r.total_rows, skipped);
    println!(
        "Blocks:          {} ({} to {})",
        r.unique_blocks, r.first_block, r.last_block
    );
    if let (Some(a), Some(b)) = (&r.first_time, &r.last_time) {
        println!("Time window:     {} to {}", a, b);
    }
    println!("Unique bots:     {}", r.unique_bots);
    println!("Total fees:      {:.6} ETH", r.total_fee_eth);
    println!();
    println!("Outcome (Dune label):");
    println!(
        "  Probes (success, no swap): {:>8} ({:.1}%)",
        r.probes,
        pct(r.probes, r.total_rows)
    );
    println!(
        "  Reverts (failed):          {:>8} ({:.1}%)",
        r.reverts,
        pct(r.reverts, r.total_rows)
    );
    if r.other_label > 0 {
        println!("  Other or missing label:    {:>8}", r.other_label);
    }
    println!();
    println!("{line}");
    println!("Per-bot density in a block (C_min = {})", r.threshold);
    println!("Measures repetition by ONE bot, not competition between bots.");
    println!("{line}");
    for (name, s) in [
        ("Sparse (< C_min)", &r.sparse),
        ("Self-burst (>= C_min)", &r.self_burst),
    ] {
        println!(
            "{:<22} {:>8} tx ({:>5.1}%) | {:>6} bot-block groups | {:>4} bots | avg gas {}",
            name,
            s.tx,
            pct(s.tx, r.total_rows),
            s.groups,
            s.unique_bots,
            format_gas(s.avg_gas())
        );
    }
    println!();
    let (min, med, max) = r.bots_per_block;
    println!(
        "Distinct bots per block: min {} | median {} | max {}",
        min, med, max
    );
    println!();
    println!("Top {} bots by attempts:", r.top_bots.len());
    for (i, b) in r.top_bots.iter().enumerate() {
        println!(
            "  #{:<2} {}  {:>7} tx ({:>5.1}%) in {:>5} blocks, max {} per block",
            i + 1,
            short_addr(&b.bot),
            b.tx,
            pct(b.tx, r.total_rows),
            b.blocks,
            b.max_per_block
        );
    }
    if !r.top_groups.is_empty() {
        println!();
        println!("Top {} self-burst groups:", r.top_groups.len());
        for (i, g) in r.top_groups.iter().enumerate() {
            println!(
                "  #{:<2} {}  ({} tx in block {})",
                i + 1,
                short_addr(&g.bot),
                g.count,
                g.block
            );
        }
    }
}
