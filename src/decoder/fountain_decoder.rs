use std::collections::HashSet;
use std::fmt::Display;

use anyhow::{Context, Result};

use crate::blockchain::{headerchain::HeaderChainValidator, print_progress};
use crate::droplet::{Droplet, Neighbor};
use crate::storage::Storage;
use crate::storage::tmp_file_storage::TmpFileStorage;
use crate::super_block::SuperBlock;

/// Fountain decoder is a peeling decoder for a Luby Transform (LT) code.
/// It decodes a droplet containing requested superblock from a set of LT encoded droplets.
pub struct FountainDecoder<'a, S, H>
where
    S: Storage<usize, Droplet>,
    H: HeaderChainValidator + ?Sized,
{
    /// Storage of received encoded symbols (droplets)
    droplet_storage: S,
    /// Decoded superblocks (indexed by superblock number)
    recovered_super_blocks: TmpFileStorage,
    /// Already decoded neighbors
    known_neighbors: HashSet<usize>,
    /// Header-chain validator used to reject invalid ("murky" or "opaque") droplets
    header_chain: &'a H,
}

impl<'a, S, H> FountainDecoder<'a, S, H>
where
    S: Storage<usize, Droplet>,
    S::Error: Display,
    H: HeaderChainValidator + ?Sized,
{
    /// Create a new decoder
    pub fn new(
        droplet_storage: S,
        header_chain: &'a H,
        super_blocks_per_epoch: usize,
    ) -> Result<Self> {
        Ok(Self {
            droplet_storage,
            recovered_super_blocks: TmpFileStorage::new()
                .context("FountainDecoder: Failed to create superblocks storage")?,
            known_neighbors: HashSet::with_capacity(super_blocks_per_epoch),
            header_chain,
        })
    }

    /// Returns decoded superblock
    pub fn get_decoded_superblock(&mut self, num: usize) -> Result<Option<SuperBlock>> {
        let mut droplets_to_skip = Vec::with_capacity(200); // droplets that we do not need anymore

        let decoded_superblock = self.recovered_super_blocks.get(&num).with_context(|| {
            format!(
                "FountainDecoder: Failed to get recovered superblock {} from storage",
                num
            )
        })?;

        if decoded_superblock.is_some() {
            return Ok(decoded_superblock);
        }

        for _ in 0..self.droplet_storage.count() {
            // For each droplet, XOR out all its known neighbors
            for i in 0..self.droplet_storage.count() {
                if droplets_to_skip.contains(&i) {
                    // skip invalid or already decoded droplets
                    continue;
                }

                let droplet = self
                .droplet_storage
                .get(&i)
                .map_err(|e| {
                    anyhow::anyhow!(
                        "FountainDecoder: Failed to get droplet {} from storage: {}",
                        i,
                        e
                    )
                })?
                .ok_or(anyhow::anyhow!(
                    "FountainDecoder: Failed to get droplet {} from storage, droplet is missing",
                    i
                ))?;

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

                    let block_hashes = match unknown_superblock
                        .block_hashes()
                        .context("Error getting block hashes from superblock")
                    {
                        Ok(hashes) => hashes,
                        Err(e) => {
                            // Do not return error here, but rather mark the droplet invalid
                            log::warn!("Invalid superblock in droplet {}: {}", droplet.num, e);
                            // store the number of invalid droplet
                            droplets_to_skip.push(i);
                            break;
                        }
                    };

                    // Check if contained blocks are part of the header-chain
                    if !self.header_chain.validate_blocks(&block_hashes) {
                        log::warn!("Invalid superblock in droplet {}", droplet.num);
                        // store the number of invalid droplet
                        droplets_to_skip.push(i);
                        break;
                    }

                    // Return it if it is the superblock that caller asked for
                    let returned_superblock = if num == unknown_neighbor.into() {
                        Some(unknown_superblock.clone())
                    } else {
                        None
                    };

                    // Superblock is valid, store it
                    self.recovered_super_blocks
                        .insert(&unknown_neighbor.into(), unknown_superblock)
                        .context("insert recovered superblock to storage")?;

                    self.known_neighbors.insert(unknown_neighbor.into());

                    print_progress();

                    // store the number of decoded droplet
                    droplets_to_skip.push(i);

                    // Return it if it is the superblock that caller asked for
                    if returned_superblock.is_some() {
                        return Ok(returned_superblock);
                    }
                }
            }
        }

        // We have not have the requested decoded superblock
        Ok(None)
    }
}
