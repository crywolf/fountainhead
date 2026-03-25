use std::cell::RefCell;

use anyhow::{Context as _, Result};
use bitcoinkernel::{
    BlockHash, BlockHeader, ChainType, ChainstateManagerBuilder, ContextBuilder,
    ProcessBlockHeaderResult, core::BlockHashExt,
};
use rusty_leveldb::{DB, Options};

use crate::blockchain::{HeaderChainstateManager, InputChainstateManager};

pub struct Config {
    /// Header-chain directory // TODO use the output dir
    pub header_chain_dir: String,
}

/// The header-chain manager
pub struct HeaderChain {
    header_chainman: HeaderChainstateManager,
    database: RefCell<DB>,
}

impl HeaderChain {
    pub fn new(config: Config) -> Result<Self> {
        let context = ContextBuilder::new()
            .chain_type(ChainType::Signet)
            .build()
            .context("build context")?;

        let header_blocks_dir = format!("{}/blocks", &config.header_chain_dir);
        let header_chainman = HeaderChainstateManager::from(
            ChainstateManagerBuilder::new(&context, &config.header_chain_dir, &header_blocks_dir)?
                .build()
                .context("build HeaderChainstateManager")?,
        );

        let db_path = std::path::Path::new(&config.header_chain_dir)
            .join("blocks")
            .join("index");

        let options = Options {
            create_if_missing: false,
            ..Default::default()
        };

        let database =
            RefCell::new(DB::open(&db_path, options).context("open header-chain database")?);

        Ok(Self {
            header_chainman,
            database,
        })
    }

    /// Generates header-chain from blockchain. Directory containing BTC blockchain data used to create header-chain
    pub fn generate(&mut self, source_data_dir: &str) -> Result<()> {
        let context = ContextBuilder::new()
            .chain_type(ChainType::Signet)
            .build()
            .context("build context")?;

        let input_blocks_dir = format!("{}/blocks", source_data_dir);
        let input_chainman = InputChainstateManager::from(
            ChainstateManagerBuilder::new(&context, source_data_dir, &input_blocks_dir)?
                .build()
                .context("build InputChainstateManager")?,
        );

        let input_chain = input_chainman.inner.active_chain();
        println!("Input chain height: {}", input_chain.height());

        // Start at header-chain height to prevent re-generation
        let start = if let Some(best_entry) = self.header_chainman.inner.best_entry() {
            best_entry.height() as usize
        } else {
            0
        };

        println!("Starting at height: {}", start);

        for (i, entry) in input_chain.iter().skip(start).enumerate() {
            let header = entry.header().to_owned();

            match self.header_chainman.inner.process_block_header(&header) {
                ProcessBlockHeaderResult::Success(_) => {
                    log::debug!("<  Header #{i} was validated and written to disk")
                }
                ProcessBlockHeaderResult::Failed(_) => {
                    log::error!("!! Header #{i} validation failed!");
                    anyhow::bail!("Header #{i} could not be stored!")
                }
            }

            if i.is_multiple_of(50000) {
                crate::blockchain::print_progress();
            }
        }
        println!();

        if let Some(best_entry) = self.header_chainman.inner.best_entry() {
            println!("Header chain height: {}", best_entry.height());
        } else {
            anyhow::bail!("Header chain creation failed");
        }

        Ok(())
    }

    /// Returns true if the block is part of the header-chain, and false otherwise.
    /// Also returns false if some block data or header deserializations fail.
    pub fn validate_presence(&self, block_hash: &[u8; 32]) -> bool {
        let block_hash = if let Ok(block_hash) = BlockHash::new(block_hash) {
            block_hash
        } else {
            return false;
        };

        if self
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

    fn lookup_header(&self, block_hash: &[u8; 32]) -> Result<Option<BlockHeader>> {
        // LevelDB keys for block index entries follow a specific format
        // The key format is: 'b' + block_hash (in little-endian)
        let mut key = vec![b'b'];
        key.extend_from_slice(block_hash);

        match self.database.borrow_mut().get(&key) {
            Some(bytes) => {
                // First 8 bytes contain some metadata => ignore them
                let header = BlockHeader::new(&bytes[8..]).context("parse header from bytes")?;
                Ok(Some(header))
            }
            None => Ok(None),
        }
    }
}
