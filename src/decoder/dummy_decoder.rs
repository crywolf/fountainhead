use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use bitcoinkernel::Block;

use crate::droplet::Droplet;

/// Dummy decoder does not decode anything, it just deserializes blocks from dummy droplets.
/// It is useful only for testing of the surrounding blockchain infrastructure and logic.
pub struct DummyDecoder {
    droplet_data_size: usize, // Size of each block data in droplet in bytes
    droplets: Vec<Droplet>,   // Received encoded symbols (droplets)
}

impl DummyDecoder {
    /// Create a new decoder
    pub fn new() -> Self {
        Self {
            droplet_data_size: 0,
            droplets: Vec::new(),
        }
    }

    /// Add a droplet
    pub fn add_droplet(&mut self, droplet: Droplet) -> Result<()> {
        if self.droplet_data_size == 0 {
            self.droplet_data_size = droplet.data_size();
        }
        if droplet.data_size() != self.droplet_data_size {
            bail!(
                "Block size mismatch in droplet decoder: expected {}, got {}",
                self.droplet_data_size,
                droplet.data_size()
            );
        }

        self.droplets.push(droplet);

        Ok(())
    }

    /// Consumes decoder and decodes all droplets and put decoded blocks into provided blocks queue (BTreeMap indexed and ordered by droplet number)
    pub fn decode(self, recovered_blocks: &mut BTreeMap<usize, Vec<Block>>) -> Result<()> {
        for (i, droplet) in self.droplets.into_iter().enumerate() {
            if i.is_multiple_of(100) {
                print_dot();
            }

            log::debug!(
                "<- decoded droplet #{}; neighbors: {:?}, droplet data: {} bytes",
                droplet.num,
                droplet.neighbors,
                droplet.data_size(),
            );

            let droplet_num = droplet.num;
            let blocks = droplet.into_blocks().context("get blocks from droplet")?;

            // add to queue
            recovered_blocks.insert(droplet_num, blocks);
        }
        println!();

        Ok(())
    }
}

impl Default for DummyDecoder {
    fn default() -> Self {
        Self::new()
    }
}

fn print_dot() {
    if log::log_enabled!(log::Level::Info) {
        print!(".");
        _ = std::io::Write::flush(&mut std::io::stdout());
    }
}
