use std::collections::HashSet;

use anyhow::{Context, Result};

use crate::blockchain::headerchain::HeaderChain;
use crate::droplet::{Droplet, Neighbor};
use crate::storage::Storage;
use crate::storage::tmp_file_storage::TmpFileStorage;
use crate::super_block::SuperBlock;

/// Fountain decoder is a peeling decoder for a Luby Transform (LT) code.
/// It decodes a droplet containing requested superblock from a set of LT encoded droplets.
pub struct FountainDecoder<'a> {
    /// Received encoded symbols (droplets)
    encoded_droplets: Vec<Droplet>,
    /// Decoded superblocks (indexed by superblock number)
    recovered_super_blocks: TmpFileStorage,
    /// Already decoded neighbors
    known_neighbors: HashSet<usize>,
    /// Header-chain manager used to reject invalid ("murky" or "opaque") droplets
    header_chain: &'a HeaderChain,
}

impl<'a> FountainDecoder<'a> {
    /// Create a new decoder
    pub fn new(header_chain: &'a HeaderChain) -> Result<Self> {
        Ok(Self {
            encoded_droplets: Vec::new(),
            recovered_super_blocks: TmpFileStorage::new()
                .context("FountainDecoder: Failed to create superblocks storage")?,
            known_neighbors: HashSet::new(),
            header_chain,
        })
    }

    /// Add a droplet
    pub fn add_encoded_droplet(&mut self, droplet: Droplet) -> Result<()> {
        self.encoded_droplets.push(droplet);

        Ok(())
    }

    /// Decodes added droplets
    pub fn decode(&mut self) -> Result<()> {
        let mut invalid_droplets = Vec::new();

        // For each droplet, XOR out all its known neighbors
        for (position, droplet) in self.encoded_droplets.iter().enumerate() {
            let unknown_neighbors: Vec<Neighbor> = droplet
                .neighbors()
                .iter()
                .copied()
                .filter(|&neighbor| !self.known_neighbors.contains(&neighbor.into()))
                .collect();

            // If exactly one unknown neighbor of the droplet remains
            if unknown_neighbors.len() == 1 {
                let unknown_neighbor = unknown_neighbors[0];

                // XOR out all superblock's known neighbors
                let mut unknown_superblock = droplet.xored_superblock().clone();

                for &neighbor in &droplet.neighbors() {
                    if let Some(known_neighbor) = self
                        .recovered_super_blocks
                        .get(&neighbor.into())
                        .with_context(|| {
                            format!(
                                "FountainDecoder: Failed to get known neighbor {} from storage",
                                neighbor
                            )
                        })?
                    {
                        unknown_superblock ^= known_neighbor;
                    }
                }

                // Check if all the blocks are valid, ie. that they correspond to the longest header-chain
                // if not => reject the whole droplet
                let mut valid_super_block = true;

                let block_hashes = match unknown_superblock
                    .block_hashes()
                    .context("get block hashes from superblock")
                {
                    Ok(hashes) => hashes,
                    Err(_) => {
                        // Do not return error here, but rather mark the droplet invalid
                        // store the droplet position for removal
                        invalid_droplets.push(position);
                        valid_super_block = false;
                        log::warn!("Invalid droplet {}", droplet.num);
                        vec![]
                    }
                };

                for block_hash in block_hashes {
                    // Look up a block in the header-chain using its hash
                    if !self.header_chain.validate_presence(&block_hash) {
                        // invalid block
                        // store the droplet position for removal
                        invalid_droplets.push(position);
                        valid_super_block = false;
                        log::warn!("Invalid droplet {}", droplet.num);
                        break;
                    }
                }

                if !valid_super_block {
                    continue;
                }

                // Valid superblock

                self.recovered_super_blocks
                    .insert(&unknown_neighbor.into(), unknown_superblock)
                    .context("insert recovered superblock to storage")?;

                self.known_neighbors.insert(unknown_neighbor.into());
            }
        }

        // Remove invalid droplets
        for i in invalid_droplets.into_iter().rev() {
            if self.encoded_droplets.len() == 1 {
                self.encoded_droplets.remove(i);
            } else {
                self.encoded_droplets.swap_remove(i);
            }
        }

        Ok(())
    }

    /// Returns decoded superblock
    pub fn get_decoded_superblock(&mut self, num: usize) -> Result<Option<SuperBlock>> {
        self.recovered_super_blocks.get(&num).with_context(|| {
            format!(
                "FountainDecoder: Failed to get recovered superblock {} from storage",
                num
            )
        })
    }
}
