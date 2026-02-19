use std::{collections::BTreeMap, fs};

use anyhow::{Context as _, Result};
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::{
    Block, ChainType, ChainstateManager, ChainstateManagerBuilder, ContextBuilder,
    ProcessBlockResult,
};
use rand::seq::SliceRandom;

use crate::{
    config::Config,
    droplet::{Droplet, Neighbor},
    padded_block::PaddedBlock,
};

pub struct Blockchain {
    config: Config,
    in_chainman: InputChainstateManager,
    out_chainman: OutputChainstateManager,
}

impl Blockchain {
    pub fn new(config: Config) -> Result<Self> {
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
        })
    }

    pub fn compress(&self) -> Result<()> {
        let chain = self.in_chainman.inner.active_chain();

        fs::create_dir_all(&self.config.droplets_dir).context("create dir to store droplets")?;

        let num_blocks_to_store = 11;

        // determine max block size for padding
        let mut max_block_size = 0;
        for entry in chain.iter().take(num_blocks_to_store) {
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

        for entry in chain.iter().take(num_blocks_to_store) {
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

            let neighbors = vec![Neighbor::new(entry.height() as usize)];

            // adaptive zero-padding
            let padded_block =
                PaddedBlock::new(block, max_block_size).context("create padded block")?;

            let droplet = Droplet::new(entry.height() as usize, neighbors, padded_block)
                .context("create droplet")?;
            let encoded_droplet = encoding::encode_to_vec(&droplet);
            println!(
                "-> droplet: {}, size: {}, encoded: {} bytes, block: {} bytes",
                droplet.num,
                droplet.data_size,
                encoded_droplet.len(),
                droplet.block_size,
            );

            let droplet_filename = format!("{:06}", droplet.num);
            let droplet_file_path =
                format!("{}/drp{}.dat", self.config.droplets_dir, droplet_filename);
            fs::write(droplet_file_path, encoded_droplet)?;
        }

        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        let droplets_dir = &self.config.droplets_dir;

        let mut droplet_files = fs::read_dir(droplets_dir)
            .with_context(|| format!("read dir {}", droplets_dir))?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()?;

        // sort droplet files by their path, ie. name
        // droplet_files.sort();

        // shuffle droplets to simulate randomness of blocks decoding order
        let mut rng = rand::rng();
        droplet_files.shuffle(&mut rng);

        let mut block_queue = BTreeMap::new();

        for droplet_file_path in droplet_files {
            if !droplet_file_path.is_file() {
                continue;
            }

            let encoded_droplet = fs::read(droplet_file_path.as_path())
                .with_context(|| format!("read droplet file {}", droplet_file_path.display()))?;

            let droplet: Droplet =
                encoding::decode_from_slice(&encoded_droplet).context("decode droplet")?;
            println!(
                "<- reconstructed #{}; neighbors: {:?}, droplet: {} bytes, block: {} bytes",
                droplet.num, droplet.neighbors, droplet.data_size, droplet.block_size
            );

            let block =
                Block::new(droplet.as_block_bytes()).context("new block from droplet bytes")?;

            // add to queue
            block_queue.insert(droplet.num, block);
        }
        println!("-----");

        // process queued blocks
        for (i, block) in block_queue.iter() {
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
