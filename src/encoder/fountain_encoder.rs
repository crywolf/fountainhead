use std::collections::HashSet;

use anyhow::Result;
use rand::distr::Distribution;
use rand::{Rng, RngExt};

use crate::droplet::{Droplet, Neighbor};
use crate::storage::Storage;
use crate::super_block::SuperBlock;

/// Fountain encoder combines more superblocks into one droplet using Luby Transform (LT) Code
pub struct FountainEncoder<D, S>
where
    D: Distribution<usize>,
    S: Storage<usize, SuperBlock>,
{
    /// Number of epoch being encoded
    epoch: usize,
    /// Number of source symbols (ie. superblocks per epoch)
    k: usize,
    /// Source data storage
    super_blocks: Option<S>,
    /// Auto incremented droplet number
    droplet_counter: usize,
    /// Probability distribution on {1, 2, . . . , k} used to sample degrees (ie. counts of superblocks to combine)
    degree_distribution: D,
}

impl<D, S> FountainEncoder<D, S>
where
    D: Distribution<usize>,
    S: Storage<usize, SuperBlock>,
{
    pub fn new(degree_distribution: D) -> Self {
        Self {
            epoch: 0,
            k: 0,
            super_blocks: None,
            droplet_counter: 0,
            degree_distribution,
        }
    }

    /// Initialize the encoder for a new epoch with the given source blocks from that epoch
    pub fn init_epoch(&mut self, epoch: usize, superblock_storage: S, current_droplet_count: usize)
    where
        S: Storage<usize, SuperBlock>,
        S::Error: std::fmt::Debug,
    {
        // TODO - dbg
        for i in 0..superblock_storage.count() {
            let superblock = superblock_storage
                .get(i)
                .expect("Failed to get superblock {} from storage");
            println!(
                "sblk: {}, blkcount: {}, size: {}, blen: {}, {:?}",
                superblock.num,
                superblock.block_count(),
                superblock.size(),
                superblock.bytes_length,
                &superblock.encoded_blocks_bytes[0..18],
            );
        }

        _ = self.super_blocks.take(); // drop storage if present

        self.epoch = epoch;
        self.k = superblock_storage.count();
        self.super_blocks = Some(superblock_storage);
        self.droplet_counter = current_droplet_count;
    }

    /// Generate a droplet containing one or more random superblocks
    pub fn generate_droplet<R: Rng>(&mut self, rng: &mut R) -> Result<Droplet> {
        // To generate a droplet in an epoch, a node first randomly
        // samples a degree d ∈ {1, 2, . . . , k} using the degree distribution
        let degree = self.degree_distribution.sample(rng);

        // Randomly select `degree` superblocks
        let mut neighbors = HashSet::new();

        while neighbors.len() < degree {
            let neighbor = Neighbor::new(rng.random_range(0..self.k));
            if !neighbors.contains(&neighbor) {
                neighbors.insert(neighbor);
            }
        }
        let neighbors: Vec<Neighbor> = neighbors.into_iter().collect();
        assert!(!neighbors.is_empty());

        let storage = &self.super_blocks.as_ref().ok_or(anyhow::anyhow!(
            "Super blocks storage not initialized, have you called init_epoch()?)"
        ))?;

        // XOR the selected superblocks

        // empty superblock
        let mut encoded_superblock = SuperBlock::new(0);

        for neighbor in neighbors.iter() {
            let superblock = storage.get(neighbor.into()).map_err(|_| {
                anyhow::anyhow!("Failed to get superblock {} from storage", neighbor)
            })?;

            encoded_superblock ^= superblock; // XORed_superblock is zero-padded to the biggest of the selected superblocks (neighbor)
        }

        // println!(
        //     "drp: {}, sblk: {},  neighbors:{:?}, blkcount: {}, size:{}",
        //     self.droplet_counter,
        //     encoded_superblock.num,
        //     neighbors,
        //     encoded_superblock.block_count(),
        //     encoded_superblock.size(),
        // );

        // create droplet with the encoded superblock
        let droplet = Droplet::new(self.droplet_counter, neighbors, encoded_superblock);

        self.droplet_counter += 1;

        Ok(droplet)
    }

    pub fn truncate_storage(&mut self) -> Result<()> {
        _ = self.super_blocks.take().ok_or(anyhow::anyhow!(
            "Storage is uninitialized, truncation failed"
        ))?;

        Ok(())
    }
}
