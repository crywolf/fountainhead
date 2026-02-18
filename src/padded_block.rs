use anyhow::{Context, Result};
use bitcoinkernel::Block;

/// Block data with padding
pub struct PaddedBlock {
    pub block_size: usize,
    pub data: Vec<u8>,
}

impl PaddedBlock {
    pub fn new(block: Block, padding_limit: usize) -> Result<Self> {
        let block_data = block.consensus_encode().context("consensus encode")?;
        let block_size = block_data.len();

        let mut data = block_data;
        let padding = padding_limit - data.len();
        data.append(&mut vec![0u8; padding]);

        Ok(Self { block_size, data })
    }
}
