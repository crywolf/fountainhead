use std::collections::BTreeMap;

use anyhow::Result;

use crate::droplet::Droplet;

/// Dummy decoder does not decode anything, it just returns dummy droplets in correct order.
/// It is useful only for testing of the surrounding blockchain infrastructure and decoding logic.
pub struct DummyDecoder {
    /// Received encoded symbols (droplets)
    droplets: Vec<Droplet>,
    /// Decoded droplets (ordered by superblock number)
    recovered_droplets: BTreeMap<usize, Droplet>,
}

impl DummyDecoder {
    /// Create a new decoder
    pub fn new() -> Self {
        Self {
            droplets: Vec::new(),
            recovered_droplets: BTreeMap::new(),
        }
    }

    /// Add a droplet
    pub fn add_droplet(&mut self, droplet: Droplet) -> Result<()> {
        self.droplets.push(droplet);

        Ok(())
    }

    /// Decodes added droplets
    pub fn decode(&mut self) -> Result<()> {
        for _ in 0..self.droplets.len() {
            let droplet = self.droplets.pop().unwrap();

            log::debug!(
                "<- decoded droplet #{}; neighbors: {:?}, droplet data: {} bytes",
                droplet.num,
                droplet.neighbors(),
                droplet.data_size(),
            );

            // add to droplet ordered queue (ordered by superblock number)
            assert_eq!(droplet.neighbors().len(), 1);

            let superblock_num = droplet.neighbors()[0].into();

            self.recovered_droplets.insert(superblock_num, droplet);
        }

        Ok(())
    }

    /// Returns decoded droplet for requested superblock
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
