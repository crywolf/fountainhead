use std::collections::VecDeque;

use anyhow::Result;
use rand::Rng;
use rand::distr::Distribution;

use crate::droplet::{Droplet, Neighbor};
use crate::padded_block::PaddedBlock;

/// Dummy encoder does not encode anything, it just places one block into a droplet.
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
    source_blocks: VecDeque<PaddedBlock>,
    /// What block is currently being processed
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
            source_blocks: VecDeque::default(),
            position: 0,
            _degree_distribution: degree_distribution,
        }
    }

    /// Initialize the encoder for a new epoch with the given source blocks from that epoch
    pub fn init_epoch(&mut self, epoch: usize, source_blocks: Vec<PaddedBlock>) {
        self.epoch = epoch;
        self.k = source_blocks.len();
        self.source_blocks = VecDeque::from(source_blocks);
    }

    /// Generate a fake droplet containing next block from the source blocks
    pub fn generate_droplet<R: Rng>(&mut self, _rng: &mut R) -> Result<Droplet> {
        let neighbors = vec![Neighbor::new(self.position)];

        self.position += 1;

        let droplet = Droplet::new(
            self.position,
            neighbors,
            self.source_blocks.pop_front().ok_or(anyhow::anyhow!(
                "Encoding block {} from {} total blocks failed. (No more source blocks for epoch {} left. Have you called init_epoch()?)",
                self.position,
                self.k,
                self.epoch,
            ))?,
        );

        Ok(droplet)
    }
}
