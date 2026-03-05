use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::droplet::Droplet;

/// Dummy decoder does not decode anything, it just deserializes blocks from dummy droplets.
/// It is useful only for testing of the surrounding blockchain infrastructure and logic.
pub struct DummyDecoder {
    droplet_data_size: usize, // Size of each block data in droplet in bytes
    droplets: Vec<Droplet>,   // Received encoded symbols (droplets)
    recovered_droplets: BTreeMap<usize, Droplet>,
}

impl DummyDecoder {
    /// Create a new decoder
    pub fn new() -> Self {
        Self {
            droplet_data_size: 0,
            droplets: Vec::new(),
            recovered_droplets: BTreeMap::new(),
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

    /// Decodes inserted droplets
    pub fn decode(&mut self) -> Result<()> {
        for _ in 0..self.droplets.len() {
            let droplet = self.droplets.pop().unwrap();

            log::debug!(
                "<- decoded droplet #{}; neighbors: {:?}, droplet data: {} bytes",
                droplet.num,
                droplet.neighbors,
                droplet.data_size(),
            );

            // add to ordered queue
            self.recovered_droplets.insert(droplet.num, droplet);
        }

        Ok(())
    }

    pub fn get_droplet(&mut self, num: usize) -> Option<Droplet> {
        if let Some(first) = self.recovered_droplets.first_entry() {
            if first.key() == &num {
                self.recovered_droplets.remove(&num)
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Default for DummyDecoder {
    fn default() -> Self {
        Self::new()
    }
}
