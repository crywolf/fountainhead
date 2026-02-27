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
            position: 0,
            _degree_distribution: degree_distribution,
        }
    }

    /// Initialize the encoder for a new epoch with the given source blocks from that epoch
    pub fn init_epoch(&mut self, epoch: usize, super_blocks: Vec<SuperBlock>) {
        self.epoch = epoch;
        self.k = super_blocks.len();
        self.super_blocks = VecDeque::from(super_blocks);
    }

    /// Generate a fake droplet containing next block from the source blocks
    pub fn generate_droplet<R: Rng>(&mut self, _rng: &mut R) -> Result<Droplet> {
        let neighbors = vec![Neighbor::new(self.position)];

        let droplet = Droplet::new(
            self.position,
            neighbors,
            self.super_blocks.pop_front().ok_or(anyhow::anyhow!(
                "Encoding superblock {} (from {} total) failed. (No more source blocks for epoch {} left. Have you called init_epoch()?)",
                self.position,
                self.k,
                self.epoch,
            ))?,
        );

        self.position += 1;

        Ok(droplet)
    }
}
