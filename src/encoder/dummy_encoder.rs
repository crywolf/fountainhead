use std::fmt::Display;

use anyhow::Result;
use rand::Rng;
use rand::distr::Distribution;

use crate::droplet::{Droplet, Neighbor};
use crate::storage::Storage;
use crate::super_block::SuperBlock;

/// Dummy encoder does not encode anything, it just places superblock into a droplet.
/// It is useful only for testing of the surrounding blockchain infrastructure and logic.
pub struct DummyEncoder<D, S>
where
    D: Distribution<usize>,
    S: Storage<usize, SuperBlock>,
{
    /// Number of epoch being encoded
    epoch: usize,
    /// Number of source symbols (ie. blocks per epoch)
    k: usize,
    /// Source data storage
    super_blocks: Option<S>,
    /// What superblock is currently being processed
    position: usize,
    // Not used. Only for trait compatibility.
    _degree_distribution: D,
}

impl<D, S> DummyEncoder<D, S>
where
    D: Distribution<usize>,
    S: Storage<usize, SuperBlock>,
{
    pub fn new(degree_distribution: D) -> Self {
        Self {
            epoch: 0,
            k: 0,
            super_blocks: None,
            position: 0,
            _degree_distribution: degree_distribution,
        }
    }

    /// Initialize the encoder for a new epoch with the given source blocks from that epoch
    pub fn init_epoch(&mut self, epoch: usize, superblock_storage: S, _current_droplet_count: usize)
    where
        S: Storage<usize, SuperBlock>,
    {
        _ = self.super_blocks.take(); // drop storage if present

        self.epoch = epoch;
        self.k = superblock_storage.count();
        self.super_blocks = Some(superblock_storage);
        self.position = 0;
    }

    /// Generate a fake droplet containing next superblock from the superblocks
    pub fn generate_droplet<R: Rng>(&mut self, _rng: &mut R) -> Result<Droplet>
    where
        <S as Storage<usize, SuperBlock>>::Error: Display,
    {
        if let Some(super_blocks) = &self.super_blocks {
            let neighbors = vec![Neighbor::new(self.position)];

            let superblock = super_blocks
                .get(self.position)
                .map_err(|e| anyhow::anyhow!("get superblock from file: {e}"))?;

            let droplet = Droplet::new(self.position, neighbors, superblock);

            self.position += 1;

            Ok(droplet)
        } else {
            anyhow::bail!("Super blocks storage not initialized, have you called init_epoch()?)")
        }
    }

    pub fn truncate_storage(&mut self) -> Result<()> {
        _ = self.super_blocks.take().ok_or(anyhow::anyhow!(
            "Storage is uninitialized, truncation failed"
        ))?;

        Ok(())
    }
}
