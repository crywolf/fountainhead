use std::collections::BTreeMap;

use anyhow::Result;

use crate::{droplet::Droplet, super_block::SuperBlock};

/// Dummy decoder does not decode anything, it just returns superblock from dummy droplet in correct order.
/// It is useful only for testing of the surrounding blockchain infrastructure and decoding logic.
pub struct DummyDecoder {
    /// Received encoded symbols (droplets)
    droplets: Vec<Droplet>,
    /// Decoded superblocks (ordered by superblock number)
    recovered_super_blocks: BTreeMap<usize, SuperBlock>,
}

impl DummyDecoder {
    /// Create a new decoder
    pub fn new() -> Result<Self> {
        Ok(Self {
            droplets: Vec::new(),
            recovered_super_blocks: BTreeMap::new(),
        })
    }

    /// Add a droplet
    pub fn add_encoded_droplet(&mut self, droplet: Droplet) -> Result<()> {
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

            self.recovered_super_blocks
                .insert(superblock_num, droplet.into_xored_superblock());
        }

        Ok(())
    }

    /// Returns decoded superblock
    pub fn get_decoded_superblock(&mut self, num: usize) -> Option<SuperBlock> {
        if let Some(first) = self.recovered_super_blocks.first_entry() {
            if first.key() == &num {
                self.recovered_super_blocks.remove(&num)
            } else {
                None
            }
        } else {
            None
        }
    }
}
