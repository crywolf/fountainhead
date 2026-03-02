use std::collections::VecDeque;

use anyhow::Result;
use rand::Rng;
use rand::distr::Distribution;

use crate::droplet::{Droplet, Neighbor};
use crate::super_block::SuperBlock;

/// Dummy encoder does not encode anything, it just places superblock into a droplet.
/// It is useful only for testing of the surrounding blockchain infrastructure and logic.
pub struct DummyEncoder<D>
where
    D: Distribution<usize>,
{
    /// Number of epoch being encoded
    epoch: usize,
    /// Number of source symbols (ie. blocks per epoch)
    k: usize,
    /// Source data
    super_blocks: VecDeque<SuperBlock>,
    /// Maximum size of a superblock in an epoch, used for adaptive padding
    max_superblock_size_in_epoch: usize,
    /// What superblock is currently being processed
    position: usize,
    // Not used. Only for trait compatibility.
    _degree_distribution: D,
}

impl<D> DummyEncoder<D>
where
    D: Distribution<usize>,
{
    pub fn new(degree_distribution: D) -> Self {
        Self {
            epoch: 0,
            k: 0,
            super_blocks: VecDeque::default(),
            max_superblock_size_in_epoch: 0,
            position: 0,
            _degree_distribution: degree_distribution,
        }
    }

    /// Initialize the encoder for a new epoch with the given source blocks from that epoch
    pub fn init_epoch(&mut self, epoch: usize, super_blocks: Vec<SuperBlock>) {
        self.epoch = epoch;
        self.k = super_blocks.len();
        self.super_blocks = VecDeque::from(super_blocks);

        // find max superblock size in the epoch for padding
        for sb in self.super_blocks.iter() {
            let superblock_size = sb.size();
            if superblock_size > self.max_superblock_size_in_epoch {
                self.max_superblock_size_in_epoch = superblock_size;
            }
        }
    }

    /// Generate a fake droplet containing next superblock from the superblocks
    pub fn generate_droplet<R: Rng>(&mut self, _rng: &mut R) -> Result<Droplet> {
        let neighbors = vec![Neighbor::new(self.position)];

        let mut superblock = self.super_blocks.pop_front().ok_or(anyhow::anyhow!(
            "Encoding superblock {} (from {} total) failed. (No more source blocks for epoch {} left. Have you called init_epoch()?)",
            self.position,
            self.k,
            self.epoch,
        ))?;

        // adaptive zero-padding
        superblock.set_padded_size(self.max_superblock_size_in_epoch);

        let droplet = Droplet::new(self.position, neighbors, superblock);

        self.position += 1;

        Ok(droplet)
    }
}
