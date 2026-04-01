pub mod compressor;
pub mod decompressor;
pub mod headerchain;
mod leveldb;

use std::io::Write;

use anyhow::{Context as _, Result};
use bitcoinkernel::{BlockHash, ChainstateManager, core::BlockHashExt};

use crate::blockchain::{headerchain::HeaderChainValidator, leveldb::LevelDB};

fn print_progress() {
    if log::log_enabled!(log::Level::Info) {
        print!(".");
        _ = std::io::stdout().flush();
    }
}

struct InputChainstateManager {
    inner: ChainstateManager,
}

impl From<ChainstateManager> for InputChainstateManager {
    fn from(value: ChainstateManager) -> Self {
        Self { inner: value }
    }
}

pub struct OutputChainstateManager {
    inner: ChainstateManager,
    db: LevelDB,
}

impl OutputChainstateManager {
    fn new(chainman: ChainstateManager, output_blocks_dir: &str) -> Result<Self> {
        let db = LevelDB::open(output_blocks_dir).context("header-chain db")?;

        Ok(Self {
            inner: chainman,
            db,
        })
    }
}

impl HeaderChainValidator for OutputChainstateManager {
    /// Returns true if the block is part of the header-chain, and false otherwise.
    /// Also returns false if some block data or header deserializations fail.
    fn validate_presence(&self, block_hash: &[u8; 32]) -> bool {
        let block_hash = if let Ok(block_hash) = BlockHash::new(block_hash) {
            block_hash
        } else {
            return false;
        };

        if self
            .db
            .lookup_header(&block_hash.to_bytes())
            .is_ok_and(|v| v.is_some())
        {
            // valid
            true
        } else {
            //invalid
            false
        }
    }
}

struct HeaderChainstateManager {
    inner: ChainstateManager,
}

impl From<ChainstateManager> for HeaderChainstateManager {
    fn from(value: ChainstateManager) -> Self {
        Self { inner: value }
    }
}
