use std::{collections::BTreeMap, fs};

use anyhow::{Context as _, Result};
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::{
    ChainType, ChainstateManager, ChainstateManagerBuilder, ContextBuilder, ProcessBlockResult,
};
use rand::seq::SliceRandom;

use crate::{
    config::Config,
    decoder::dummy_decoder::DummyDecoder,
    droplet::Droplet,
    encoder::{distribution::RobustSoliton, dummy_encoder::DummyEncoder},
    padded_block::PaddedBlock,
};

pub struct Blockchain {
    config: Config,
    in_chainman: InputChainstateManager,
    out_chainman: OutputChainstateManager,
    encoder: DummyEncoder<RobustSoliton>,
}

impl Blockchain {
    pub fn new(config: Config, encoder: DummyEncoder<RobustSoliton>) -> Result<Self> {
        let context = ContextBuilder::new()
            .chain_type(ChainType::Signet)
            .build()?;

        let input_blocks_dir = format!("{}/blocks", &config.input_data_dir);
        let in_chainman = InputChainstateManager::from(
            ChainstateManagerBuilder::new(&context, &config.input_data_dir, &input_blocks_dir)?
                .worker_threads(config.worker_threads)
                .build()?,
        );

        let output_blocks_dir = format!("{}/blocks", &config.output_data_dir);
        let out_chainman = OutputChainstateManager::from(
            ChainstateManagerBuilder::new(&context, &config.output_data_dir, &output_blocks_dir)?
                .worker_threads(config.worker_threads)
                .build()?,
        );

        ///////////////////////

        let chain = in_chainman.inner.active_chain();
        println!("Initializing blockchain reader");
        println!("Active chain height: {}", chain.height());

        println!("-----");

        ///////////////////////

        Ok(Self {
            config,
            in_chainman,
            out_chainman,
            encoder,
        })
    }

    pub fn compress(&mut self) -> Result<()> {
        let blocks_per_epoch = self.config.blocks_per_epoch;
        let encoder = &mut self.encoder;

        let chain = self.in_chainman.inner.active_chain();

        for epoch in 0..self.config.epochs_to_encode {
            println!("--- EPOCH {epoch} ---");
            let epoch_dir = format!("{}/epoch{:06}", self.config.droplets_dir, epoch);

            fs::create_dir_all(&epoch_dir).context("create epoch dir to store droplets")?;

            // determine max block size for padding
            let mut max_block_size = 0;
            for entry in chain
                .iter()
                .skip(epoch * blocks_per_epoch)
                .take(blocks_per_epoch)
            {
                let block = self
                    .in_chainman
                    .inner
                    .read_block_data(&entry)
                    .context("read block data")?;
                let block_size = block.consensus_encode()?.len();
                if block_size > max_block_size {
                    max_block_size = block_size;
                }
            }

            let mut source_blocks = Vec::new();
            for entry in chain
                .iter()
                .skip(epoch * blocks_per_epoch)
                .take(blocks_per_epoch)
            {
                println!(
                    ">  Reading block {} at height {}",
                    entry.block_hash(),
                    entry.height()
                );
                let block = self
                    .in_chainman
                    .inner
                    .read_block_data(&entry)
                    .context("read block data")?;

                // adaptive zero-padding
                let padded_block =
                    PaddedBlock::new(block, max_block_size).context("create padded block")?;

                source_blocks.push(padded_block);
            }

            // TODO decide what to do with the last (incomplete) epoch
            assert_eq!(
                blocks_per_epoch,
                source_blocks.len(),
                "Not enough blocks per epoch"
            );

            encoder.init_epoch(epoch, source_blocks);
            let mut rng = rand::rng();

            for num in 0..blocks_per_epoch {
                let droplet = encoder
                    .generate_droplet(&mut rng)
                    .with_context(|| format!("generate droplet {}", num))?;

                let encoded_droplet = encoding::encode_to_vec(&droplet);

                println!(
                    "-> droplet: {}, size: {}, encoded: {} bytes, block: {} bytes",
                    droplet.num,
                    droplet.data_size,
                    encoded_droplet.len(),
                    droplet.block_size,
                );

                let droplet_filename = format!("{:06}", droplet.num);
                let droplet_file_path = format!("{}/drp{}.dat", epoch_dir, droplet_filename);
                fs::write(droplet_file_path, encoded_droplet)?;
            }
        }

        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        for epoch in 0..self.config.epochs_to_encode {
            println!("--- EPOCH {epoch} ---");
            let epoch_dir = format!("{}/epoch{:06}", self.config.droplets_dir, epoch);

            let mut droplet_files = fs::read_dir(&epoch_dir)
                .with_context(|| format!("read dir {}", epoch_dir))?
                .map(|res| res.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()?;

            // sort droplet files by their path, ie. name
            // droplet_files.sort();

            // shuffle droplets to simulate randomness of blocks decoding order
            let mut rng = rand::rng();
            droplet_files.shuffle(&mut rng);

            let mut decoder = DummyDecoder::new();

            for droplet_file_path in droplet_files.iter() {
                if !droplet_file_path.is_file() {
                    continue;
                }

                let encoded_droplet = fs::read(droplet_file_path.as_path()).with_context(|| {
                    format!("read droplet file {}", droplet_file_path.display())
                })?;

                let droplet: Droplet = encoding::decode_from_slice(&encoded_droplet)
                    .context("decode droplet from file bytes")?;

                decoder.add_droplet(droplet)?;
            }

            let mut recovered_blocks = BTreeMap::new();

            decoder
                .decode(&mut recovered_blocks)
                .context("decode droplet")?;

            println!("-----");

            // process queued blocks
            for (i, block) in recovered_blocks.iter() {
                match self.out_chainman.inner.process_block(block) {
                    ProcessBlockResult::NewBlock => {
                        println!("<  Reconstructed block {i} validated and written to disk")
                    }
                    ProcessBlockResult::Duplicate => {
                        println!("<  Reconstructed block {i} already known (valid)")
                    }
                    ProcessBlockResult::Rejected => {
                        println!("!!! Reconstructed block {i} validation failed !!!")
                    }
                }
            }
        }

        ///////////////////////

        println!("-----");

        let out_chain = self.out_chainman.inner.active_chain();

        // Get the reconstructed tip
        let tip = out_chain.tip();
        println!("Reconstructed chain height: {}", out_chain.height());
        println!("Reconstructed tip hash: {}", tip.block_hash());
        let block = self.out_chainman.inner.read_block_data(&tip)?;
        println!(
            "Transactions count in the last reconstructed block: {}",
            block.transaction_count()
        );

        Ok(())
    }
}

pub struct InputChainstateManager {
    inner: ChainstateManager,
}

impl From<ChainstateManager> for InputChainstateManager {
    fn from(value: ChainstateManager) -> Self {
        Self { inner: value }
    }
}

pub struct OutputChainstateManager {
    inner: ChainstateManager,
}

impl From<ChainstateManager> for OutputChainstateManager {
    fn from(value: ChainstateManager) -> Self {
        Self { inner: value }
    }
}
