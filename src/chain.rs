//! Per-chain constants for the OP-stack chains of the study. Base is the default and its
//! values are the ones the rest of the crate always used.

use clap::ValueEnum;

use crate::sample::Timing;

/// L2 predeploy shared by every OP-stack chain.
pub const WETH: &str = "0x4200000000000000000000000000000000000006";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Chain {
    Base,
    Unichain,
    Op,
}

impl Chain {
    pub const ALL: [Chain; 3] = [Chain::Base, Chain::Unichain, Chain::Op];

    /// Inverse of [`Chain::as_str`] (exact lowercase name).
    pub fn parse(s: &str) -> Option<Chain> {
        Chain::ALL.into_iter().find(|c| c.as_str() == s)
    }

    /// Block clock of the chain.
    pub fn timing(self) -> Timing {
        Timing {
            genesis_ts: self.genesis_ts(),
            block_time: self.block_time(),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Chain::Base => "base",
            Chain::Unichain => "unichain",
            Chain::Op => "op",
        }
    }

    /// Alchemy network slug: `https://<slug>.g.alchemy.com/v2/<key>`.
    pub fn alchemy_network(self) -> &'static str {
        match self {
            Chain::Base => "base-mainnet",
            Chain::Unichain => "unichain-mainnet",
            Chain::Op => "opt-mainnet",
        }
    }

    /// Uniswap v4 singleton PoolManager (lowercase), from the official deployments page.
    pub fn pool_manager(self) -> &'static str {
        match self {
            Chain::Base => "0x498581ff718922c3f8e6a244956af099b2652b2b",
            Chain::Unichain => "0x1f98400000000000000000000000000000000004",
            Chain::Op => "0x9a13f98cb987694c9f086b1f5eb990eea8264ec3",
        }
    }

    pub fn weth(self) -> &'static str {
        WETH
    }

    /// Seconds between blocks.
    pub fn block_time(self) -> i64 {
        match self {
            Chain::Base | Chain::Op => 2,
            Chain::Unichain => 1,
        }
    }

    /// Timestamp `t0` such that block `n` has timestamp `t0 + block_time * n`.
    /// Base: genesis. Unichain: genesis. OP Mainnet: extrapolated to block 0 from the
    /// Bedrock era (valid for blocks after Bedrock, 105_235_063); fitted on the
    /// `blockTimestamp` of the receipts of the chain pilot (EXPL-21), 30 blocks, exact.
    pub fn genesis_ts(self) -> i64 {
        match self {
            Chain::Base => 1_686_789_347,
            Chain::Unichain => 1_730_748_359,
            Chain::Op => 1_475_598_777,
        }
    }
}
