use std::collections::HashMap;

use anyhow::Result;

use crate::droplet::{Droplet, Neighbor};
use crate::super_block::SuperBlock;

/// Fountain decoder is a peeling decoder for a Luby Transform (LT) code.
/// It decodes a droplet containing requested superblock from a set of LT encoded droplets.
pub struct FountainDecoder {
    /// Received encoded symbols (droplets)
    encoded_droplets: Vec<Droplet>,
    /// Decoded superblocks (indexed by superblock number)
    recovered_super_blocks: HashMap<usize, SuperBlock>,
}

impl FountainDecoder {
    /// Create a new decoder
    pub fn new() -> Self {
        Self {
            encoded_droplets: Vec::new(),
            recovered_super_blocks: HashMap::new(),
        }
    }

    /// Add a droplet
    pub fn add_encoded_droplet(&mut self, droplet: Droplet) -> Result<()> {
        self.encoded_droplets.push(droplet);

        Ok(())
    }

    /// Decodes added droplets
    pub fn decode(&mut self) -> Result<()> {
        // For each droplet, XOR out all its known neighbors
        for droplet in self.encoded_droplets.iter() {
            let unknown_neighbors: Vec<Neighbor> = droplet
                .neighbors()
                .iter()
                .copied()
                .filter(|&neighbor| !self.recovered_super_blocks.contains_key(&neighbor.into()))
                .collect();

            // If exactly one unknown neighbor of the droplet remains
            if unknown_neighbors.len() == 1 {
                let unknown_neighbor = unknown_neighbors[0];
                log::info!("> Unknown_neighbor: {}", unknown_neighbor);

                // XOR out all droplet's known neighbors
                let mut recovered_superblock = droplet.superblock().clone(); // TODO without cloning?
                // log::info!(
                //     "BEFORE XOR drp: {}, sblk: {}, neighbors: {:?}, blkcount: {}, size: {}",
                //     droplet.num,
                //     droplet.superblock().num,
                //     droplet.neighbors(),
                //     droplet.superblock().block_count(),
                //     droplet.superblock().size(),
                // );

                for &neighbor in &droplet.neighbors() {
                    if let Some(known_superblock) =
                        self.recovered_super_blocks.get(&neighbor.into())
                    {
                        recovered_superblock ^= known_superblock;
                    }
                }

                // Add to droplet storage (indexed by superblock number)
                // let recovered_droplet = Droplet::new(
                //     unknown_neighbor.into(),
                //     vec![unknown_neighbor],
                //     recovered_superblock,
                // );
                log::info!(
                    "INSERTING DECODED sblk: {}, blkcount: {}, size: {}, {:?}",
                    recovered_superblock.num,
                    recovered_superblock.block_count(),
                    recovered_superblock.size(),
                    &recovered_superblock.encoded_blocks_bytes[0..18],
                );
                self.recovered_super_blocks
                    .insert(unknown_neighbor.into(), recovered_superblock);
            }
        }

        Ok(())
    }

    /// Returns decoded superblock
    pub fn get_decoded_superblock(&mut self, num: usize) -> Option<SuperBlock> {
        self.recovered_super_blocks.get(&num).cloned()
    }
}

impl Default for FountainDecoder {
    fn default() -> Self {
        Self::new()
    }
}
