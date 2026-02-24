use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use bitcoinkernel::Block;

use crate::droplet::Droplet;

/// Dummy decoder does not decode anything, it just deserializes blocks from fake droplets.
/// It is useful only for testing of the surrounding blockchain infrastructure and logic.
pub struct DummyDecoder {
    block_data_size: usize, // Size of each block data in droplet in bytes
    droplets: Vec<Droplet>, // Received encoded symbols
}

impl DummyDecoder {
    /// Create a new decoder
    pub fn new() -> Self {
        Self {
            block_data_size: 0,
            droplets: Vec::new(),
        }
    }

    /// Add a droplet
    pub fn add_droplet(&mut self, droplet: Droplet) -> Result<()> {
        if self.block_data_size == 0 {
            self.block_data_size = droplet.data_size;
        }
        if droplet.data_size != self.block_data_size {
            bail!(
                "Block size mismatch: expected {}, got {}",
                self.block_data_size,
                droplet.data_size
            );
        }

        self.droplets.push(droplet);

        Ok(())
    }

    /// Decode all droplets and put them into provided blocks queue (BTreeMap indexed and ordered by block height)
    pub fn decode(&mut self, recovered_blocks: &mut BTreeMap<usize, Block>) -> Result<()> {
        for droplet in &self.droplets {
            println!(
                "<- reconstructed #{}; neighbors: {:?}, droplet: {} bytes, block: {} bytes",
                droplet.num, droplet.neighbors, droplet.data_size, droplet.block_size
            );

            let block =
                Block::new(droplet.as_block_bytes()).context("new block from droplet bytes")?;

            // add to queue
            recovered_blocks.insert(droplet.num, block);
        }

        Ok(())
    }
}

impl Default for DummyDecoder {
    fn default() -> Self {
        Self::new()
    }
}
