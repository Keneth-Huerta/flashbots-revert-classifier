use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;

#[derive(Parser)]
#[command(name = "mev-revert-classifier")]
#[command(version = "0.1.0")]
#[command(about = "Classifies MEV reverts into Blind Spam and HF Contention")]
struct Args {
    #[arg(short, long, help = "Path to Dune-exported CSV file")]
    file: String,

    #[arg(
        short,
        long,
        default_value = "3",
        help = "Collision threshold C_min: reverts to same bot in same block >= C_min = HF Contention"
    )]
    threshold: usize,

    #[arg(
        long,
        default_value = "10",
        help = "Number of top contested bots to show"
    )]
    top_n: usize,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct DuneTransaction {
    week_number: Option<f64>,
    week_start: Option<String>,
    week_end: Option<String>,
    bot_address: String,
    tx_hash: String,
    block_time: Option<String>,
    block_number: Option<u64>,
    tx_index: Option<u64>,
    spam_classification: Option<String>,
    success: Option<String>,
    value: Option<String>,
    gas_used: Option<u64>,
    gas_limit: Option<u64>,
    base_fee_eth: Option<f64>,
    priority_fee_eth: Option<f64>,
    total_gas_fee_eth: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Classification {
    BlindSpam,
    HFContention,
}

#[derive(Debug, Clone)]
struct BotCluster {
    bot: String,
    block: u64,
    count: usize,
    gas_used: u64,
    reverted_count: usize,
    dry_run_count: usize,
    classification: Classification,
}

struct ClassificationReport {
    total_rows: usize,
    spam_tx_count: usize,
    contention_tx_count: usize,
    spam_bots: usize,
    contention_bots: usize,
    spam_avg_gas: f64,
    contention_avg_gas: f64,
    total_reverted: usize,
    total_dry_run: usize,
    top_contested: Vec<BotCluster>,
    elapsed_ms: u128,
    threshold: usize,
    file: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let start = Instant::now();

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(&args.file)
        .with_context(|| format!("No se pudo abrir {}", &args.file))?;

    let mut transactions: Vec<DuneTransaction> = Vec::new();

    for result in reader.deserialize::<DuneTransaction>() {
        match result {
            Ok(record) => {
                if record.bot_address.is_empty() || record.block_number.is_none() {
                    continue;
                }
                transactions.push(record);
            }
            Err(e) => {
                eprintln!("Warning: skipping malformed row: {}", e);
            }
        }
    }

    if transactions.is_empty() {
        anyhow::bail!("El archivo CSV no contiene filas validas con bot_address y block_number.");
    }

    let total_rows = transactions.len();

    let mut clusters: HashMap<(u64, String), Vec<DuneTransaction>> = HashMap::new();
    for tx in &transactions {
        if let Some(block) = tx.block_number {
            let key = (block, tx.bot_address.clone());
            clusters.entry(key).or_default().push(tx.clone());
        }
    }

    let threshold = args.threshold;

    let mut bot_clusters: Vec<BotCluster> = clusters
        .into_iter()
        .map(|((block, bot), txs)| {
            let count = txs.len();
            let gas: u64 = txs.iter().filter_map(|t| t.gas_used).sum();
            let reverted_count = txs
                .iter()
                .filter(|t| t.spam_classification.as_deref() == Some("reverted_spam"))
                .count();
            let dry_run_count = txs
                .iter()
                .filter(|t| t.spam_classification.as_deref() == Some("dry_run_probe"))
                .count();
            let classification = if count >= threshold {
                Classification::HFContention
            } else {
                Classification::BlindSpam
            };
            BotCluster {
                bot,
                block,
                count,
                gas_used: gas,
                reverted_count,
                dry_run_count,
                classification,
            }
        })
        .collect();

    let spam_tx_count: usize = bot_clusters
        .iter()
        .filter(|c| c.classification == Classification::BlindSpam)
        .map(|c| c.count)
        .sum();

    let contention_tx_count: usize = bot_clusters
        .iter()
        .filter(|c| c.classification == Classification::HFContention)
        .map(|c| c.count)
        .sum();

    let spam_bots: usize = bot_clusters
        .iter()
        .filter(|c| c.classification == Classification::BlindSpam)
        .count();

    let contention_bots: usize = bot_clusters
        .iter()
        .filter(|c| c.classification == Classification::HFContention)
        .count();

    let spam_total_gas: u64 = bot_clusters
        .iter()
        .filter(|c| c.classification == Classification::BlindSpam)
        .map(|c| c.gas_used)
        .sum();

    let contention_total_gas: u64 = bot_clusters
        .iter()
        .filter(|c| c.classification == Classification::HFContention)
        .map(|c| c.gas_used)
        .sum();

    let spam_avg_gas = if spam_tx_count > 0 {
        spam_total_gas as f64 / spam_tx_count as f64
    } else {
        0.0
    };

    let contention_avg_gas = if contention_tx_count > 0 {
        contention_total_gas as f64 / contention_tx_count as f64
    } else {
        0.0
    };

    let total_reverted: usize = bot_clusters.iter().map(|c| c.reverted_count).sum();
    let total_dry_run: usize = bot_clusters.iter().map(|c| c.dry_run_count).sum();

    bot_clusters.sort_by_key(|b| std::cmp::Reverse(b.count));

    let top_contested: Vec<BotCluster> = bot_clusters
        .iter()
        .filter(|c| c.classification == Classification::HFContention)
        .take(args.top_n)
        .cloned()
        .collect();

    let elapsed_ms = start.elapsed().as_millis();

    let report = ClassificationReport {
        total_rows,
        spam_tx_count,
        contention_tx_count,
        spam_bots,
        contention_bots,
        spam_avg_gas,
        contention_avg_gas,
        total_reverted,
        total_dry_run,
        top_contested,
        elapsed_ms,
        threshold,
        file: args.file,
    };

    print_report(&report);

    Ok(())
}

fn print_report(report: &ClassificationReport) {
    let spam_pct = if report.total_rows > 0 {
        (report.spam_tx_count as f64 / report.total_rows as f64) * 100.0
    } else {
        0.0
    };

    let contention_pct = if report.total_rows > 0 {
        (report.contention_tx_count as f64 / report.total_rows as f64) * 100.0
    } else {
        0.0
    };

    println!("mev-revert-classifier v0.1.0");
    println!("============================================================");
    println!("File:           {}", report.file);
    println!("Rows processed: {}", report.total_rows);
    println!("Threshold (C_min): {}", report.threshold);
    println!();
    println!("Raw Breakdown (from Dune classification):");
    println!("  reverted_spam:  {}", report.total_reverted);
    println!("  dry_run_probe:  {}", report.total_dry_run);
    println!();
    println!("============================================================");
    println!("Heuristic Classification (by bot block collisions)");
    println!("============================================================");
    println!(
        "Blind Spam:        {:>8} tx ({:.1}%)  | {:>6} unique bots",
        report.spam_tx_count, spam_pct, report.spam_bots
    );
    println!(
        "HF Contention:     {:>8} tx ({:.1}%)  | {:>6} unique bots",
        report.contention_tx_count, contention_pct, report.contention_bots
    );
    println!("------------------------------------------------------------");
    println!("Total:             {:>8} tx (100.0%)", report.total_rows);
    println!("============================================================");
    println!();
    println!("Average Gas per Tx:");
    println!("  Blind Spam:      {}", format_gas(report.spam_avg_gas));
    println!(
        "  HF Contention:   {}",
        format_gas(report.contention_avg_gas)
    );
    println!();

    if !report.top_contested.is_empty() {
        println!("Top {} Most Contested Bots:", report.top_contested.len());
        for (i, cluster) in report.top_contested.iter().enumerate() {
            let short_addr = if cluster.bot.len() > 12 {
                format!("{}...", &cluster.bot[..12])
            } else {
                cluster.bot.clone()
            };
            println!(
                "  #{:<2} {}  ({} tx in block {})",
                i + 1,
                short_addr,
                cluster.count,
                cluster.block
            );
        }
    }

    println!();
    println!("Processing time: {}ms", report.elapsed_ms);
}

fn format_gas(gas: f64) -> String {
    if gas >= 1_000_000.0 {
        format!("{:.2}M", gas / 1_000_000.0)
    } else if gas >= 1_000.0 {
        format!("{:.1}K", gas / 1_000.0)
    } else {
        format!("{:.0}", gas)
    }
}
