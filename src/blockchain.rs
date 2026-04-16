pub mod compressor;
pub mod decompressor;
pub mod headerchain;
mod leveldb;

use std::io::Write;

use anyhow::{Context as _, Result};
use bitcoinkernel::{BlockHash, ChainstateManager, core::BlockHashExt};

use crate::{
    blockchain::{headerchain::HeaderChainValidator, leveldb::LevelDB},
    super_block::BlockHashesPair,
};

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
    /// Returns true if the blocks are part of the header-chain in correct order, and false otherwise.
    /// Also returns false if some block data or header deserializations fail.
    fn validate_blocks(&self, block_hashes: &[BlockHashesPair]) -> bool {
        // FIXME: We should also compute and check if Merkle root of the block matches the Merkle root
        // stored in header-chain, but `bitcoinkernel` does not currently provide method to access Merkle root.

        let mut prev_hash = [0; 32];

        for (i, block_hash_pair) in block_hashes.iter().enumerate() {
            let block_hash = if let Ok(block_hash) = BlockHash::new(&block_hash_pair.current()) {
                block_hash
            } else {
                return false;
            };

            if !self
                .db
                .lookup_header(&block_hash.to_bytes())
                .is_ok_and(|v| v.is_some())
            {
                // block is not part of the header-chain
                return false;
            }

            if i > 0 && block_hash_pair.previous() != prev_hash {
                // incorrect predecessor
                return false;
            }

            prev_hash = block_hash_pair.current()
        }

        true
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
