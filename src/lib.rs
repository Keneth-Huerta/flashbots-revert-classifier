pub mod chain;
pub mod contend;
pub mod extract;
pub mod fetch;
pub mod keccak;
pub mod neighbor;
pub mod placebo;
pub mod race;
pub mod robust;
pub mod rpc;
pub mod sample;
pub mod system;

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct DuneTransaction {
    pub week_number: Option<f64>,
    pub week_start: Option<String>,
    pub week_end: Option<String>,
    pub bot_address: String,
    pub tx_hash: String,
    pub block_time: Option<String>,
    pub block_number: Option<u64>,
    pub tx_index: Option<u64>,
    pub spam_classification: Option<String>,
    pub success: Option<String>,
    pub value: Option<String>,
    pub gas_used: Option<u64>,
    pub gas_limit: Option<u64>,
    pub base_fee_eth: Option<f64>,
    pub priority_fee_eth: Option<f64>,
    pub total_gas_fee_eth: Option<f64>,
}

/// Density label of a `(block, bot)` group. It says how many attempts one bot
/// sent in one block; it does not measure competition between bots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Sparse,
    SelfBurst,
}

#[derive(Debug, Clone)]
pub struct BotBlockGroup {
    pub bot: String,
    pub block: u64,
    pub count: usize,
    pub gas_used: u64,
    pub density: Density,
}

#[derive(Debug, Clone)]
pub struct BotShare {
    pub bot: String,
    pub tx: usize,
    pub blocks: usize,
    pub max_per_block: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct DensityStats {
    pub tx: usize,
    pub groups: usize,
    pub unique_bots: usize,
    pub gas_used: u64,
}

impl DensityStats {
    pub fn avg_gas(&self) -> f64 {
        if self.tx == 0 {
            0.0
        } else {
            self.gas_used as f64 / self.tx as f64
        }
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub threshold: usize,
    pub total_rows: usize,
    pub unique_bots: usize,
    pub unique_blocks: usize,
    pub first_block: u64,
    pub last_block: u64,
    pub first_time: Option<String>,
    pub last_time: Option<String>,
    pub probes: usize,
    pub reverts: usize,
    pub other_label: usize,
    pub total_fee_eth: f64,
    pub sparse: DensityStats,
    pub self_burst: DensityStats,
    /// Distinct bots with at least one attempt in a block: (min, median, max).
    pub bots_per_block: (usize, usize, usize),
    pub top_bots: Vec<BotShare>,
    pub top_groups: Vec<BotBlockGroup>,
}

/// Rows without `bot_address` or `block_number` cannot be grouped and are dropped.
pub fn is_usable(tx: &DuneTransaction) -> bool {
    !tx.bot_address.is_empty() && tx.block_number.is_some()
}

pub fn analyze(transactions: &[DuneTransaction], threshold: usize, top_n: usize) -> Report {
    let txs: Vec<&DuneTransaction> = transactions.iter().filter(|t| is_usable(t)).collect();

    let mut groups: HashMap<(u64, &str), (usize, u64)> = HashMap::new();
    let mut bots_in_block: HashMap<u64, HashSet<&str>> = HashMap::new();
    let mut per_bot: HashMap<&str, usize> = HashMap::new();
    let (mut probes, mut reverts, mut other_label) = (0, 0, 0);
    let mut total_fee_eth = 0.0;
    let mut times: Vec<&str> = Vec::new();

    for tx in &txs {
        let block = tx.block_number.unwrap_or_default();
        let bot = tx.bot_address.as_str();
        let entry = groups.entry((block, bot)).or_default();
        entry.0 += 1;
        entry.1 += tx.gas_used.unwrap_or(0);
        bots_in_block.entry(block).or_default().insert(bot);
        *per_bot.entry(bot).or_default() += 1;
        match tx.spam_classification.as_deref() {
            Some("dry_run_probe") => probes += 1,
            Some("reverted_spam") => reverts += 1,
            _ => other_label += 1,
        }
        total_fee_eth += tx.total_gas_fee_eth.unwrap_or(0.0);
        if let Some(t) = tx.block_time.as_deref() {
            times.push(t);
        }
    }

    let mut all_groups: Vec<BotBlockGroup> = groups
        .iter()
        .map(|(&(block, bot), &(count, gas_used))| BotBlockGroup {
            bot: bot.to_string(),
            block,
            count,
            gas_used,
            density: if count >= threshold {
                Density::SelfBurst
            } else {
                Density::Sparse
            },
        })
        .collect();

    let stats_for = |d: Density| {
        let selected: Vec<&BotBlockGroup> = all_groups.iter().filter(|g| g.density == d).collect();
        DensityStats {
            tx: selected.iter().map(|g| g.count).sum(),
            groups: selected.len(),
            unique_bots: selected
                .iter()
                .map(|g| g.bot.as_str())
                .collect::<HashSet<_>>()
                .len(),
            gas_used: selected.iter().map(|g| g.gas_used).sum(),
        }
    };
    let sparse = stats_for(Density::Sparse);
    let self_burst = stats_for(Density::SelfBurst);

    let mut counts: Vec<usize> = bots_in_block.values().map(|s| s.len()).collect();
    counts.sort_unstable();
    let bots_per_block = if counts.is_empty() {
        (0, 0, 0)
    } else {
        (
            counts[0],
            counts[counts.len() / 2],
            counts[counts.len() - 1],
        )
    };

    let mut bot_blocks: HashMap<&str, (usize, usize)> = HashMap::new();
    for (&(_, bot), &(count, _)) in &groups {
        let entry = bot_blocks.entry(bot).or_default();
        entry.0 += 1;
        entry.1 = entry.1.max(count);
    }
    let mut top_bots: Vec<BotShare> = per_bot
        .iter()
        .map(|(&bot, &tx)| {
            let (blocks, max_per_block) = bot_blocks[bot];
            BotShare {
                bot: bot.to_string(),
                tx,
                blocks,
                max_per_block,
            }
        })
        .collect();
    top_bots.sort_by(|a, b| b.tx.cmp(&a.tx).then_with(|| a.bot.cmp(&b.bot)));
    top_bots.truncate(top_n);

    all_groups.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.block.cmp(&b.block))
            .then_with(|| a.bot.cmp(&b.bot))
    });
    let top_groups: Vec<BotBlockGroup> = all_groups
        .into_iter()
        .filter(|g| g.density == Density::SelfBurst)
        .take(top_n)
        .collect();

    times.sort_unstable();
    let blocks: Vec<u64> = bots_in_block.keys().copied().collect();

    Report {
        threshold,
        total_rows: txs.len(),
        unique_bots: per_bot.len(),
        unique_blocks: blocks.len(),
        first_block: blocks.iter().copied().min().unwrap_or(0),
        last_block: blocks.iter().copied().max().unwrap_or(0),
        first_time: times.first().map(|s| s.to_string()),
        last_time: times.last().map(|s| s.to_string()),
        probes,
        reverts,
        other_label,
        total_fee_eth,
        sparse,
        self_burst,
        bots_per_block,
        top_bots,
        top_groups,
    }
}

pub fn format_gas(gas: f64) -> String {
    if gas >= 1_000_000.0 {
        format!("{:.2}M", gas / 1_000_000.0)
    } else if gas >= 1_000.0 {
        format!("{:.1}K", gas / 1_000.0)
    } else {
        format!("{:.0}", gas)
    }
}

pub fn short_addr(addr: &str) -> String {
    if addr.len() > 12 {
        format!("{}...", &addr[..12])
    } else {
        addr.to_string()
    }
}

pub fn pct(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64 * 100.0
    }
}
