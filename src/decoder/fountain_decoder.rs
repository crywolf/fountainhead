use std::collections::HashMap;

use anyhow::Result;

use crate::droplet::{Droplet, Neighbor};

/// Fountain decoder is a peeling decoder for a Luby Transform (LT) code.
/// It decodes a droplet containing requested superblock from a set of LT encoded droplets.
pub struct FountainDecoder {
    /// Received encoded symbols (droplets)
    droplets: Vec<Droplet>,
    /// Decoded droplets (ordered by superblock number)
    recovered_droplets: HashMap<usize, Droplet>,
}

impl FountainDecoder {
    /// Create a new decoder
    pub fn new() -> Self {
        Self {
            droplets: Vec::new(),
            recovered_droplets: HashMap::new(),
        }
    }

    /// Add a droplet
    pub fn add_encoded_droplet(&mut self, droplet: Droplet) -> Result<()> {
        self.droplets.push(droplet);

        Ok(())
    }

    /// Decodes added droplets
    pub fn decode(&mut self) -> Result<()> {
        // For each droplet, XOR out all its known neighbors
        for droplet in self.droplets.iter() {
            let unknown_neighbors: Vec<Neighbor> = droplet
                .neighbors()
                .iter()
                .copied()
                .filter(|&neighbor| !self.recovered_droplets.contains_key(&neighbor.into()))
                .collect();

            // If exactly one unknown neighbor of the droplet remains
            if unknown_neighbors.len() == 1 {
                let unknown_neighbor = unknown_neighbors[0];
                log::info!("> Unknown_neighbor: {}", unknown_neighbor);

                // XOR out all droplet's known neighbors
                let mut recovered_superblock = droplet.superblock().clone(); // TODO without cloning
                log::info!(
                    "BEFORE XOR drp: {}, sblk: {}, neighbors: {:?}, blkcount: {}, size: {}",
                    droplet.num,
                    droplet.superblock().num,
                    droplet.neighbors(),
                    droplet.superblock().block_count(),
                    droplet.superblock().size(),
                );

                for &neighbor in &droplet.neighbors() {
                    if let Some(known_droplet) = self.recovered_droplets.get(&neighbor.into()) {
                        recovered_superblock ^= known_droplet.superblock().clone(); // TODO without cloning
                    }
                }

                // Add to droplet storage (indexed by superblock number)
                let recovered_droplet = Droplet::new(
                    unknown_neighbor.into(),
                    vec![unknown_neighbor],
                    recovered_superblock,
                );
                log::info!(
                    "INSERTING DECODED drp: {}, sblk: {}, neighbors: {:?}, blkcount: {}, size: {}",
                    recovered_droplet.num,
                    recovered_droplet.superblock().num,
                    recovered_droplet.neighbors(),
                    recovered_droplet.superblock().block_count(),
                    recovered_droplet.superblock().size(),
                );
                self.recovered_droplets
                    .insert(unknown_neighbor.into(), recovered_droplet);
            }
        }

        Ok(())
    }

    /// Returns decoded droplet for requested superblock
    pub fn get_decoded_droplet(&mut self, num: usize) -> Option<Droplet> {
        self.recovered_droplets.get(&num).cloned()
    }
}

impl Default for FountainDecoder {
    fn default() -> Self {
        Self::new()
    }
}
