use std::fs;

use anyhow::{Context as _, Result};
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::{
    Block, ChainType, ChainstateManager, ChainstateManagerBuilder, ContextBuilder,
    ProcessBlockResult,
};

use crate::{config::Config, droplet::Droplet};

pub struct Blockchain {
    config: Config,
    in_chainman: ChainstateManager,
    out_chainman: ChainstateManager,
}

impl Blockchain {
    pub fn new(config: Config) -> Result<Self> {
        let context = ContextBuilder::new()
            .chain_type(ChainType::Signet)
            .build()?;

        let input_blocks_dir = format!("{}/blocks", &config.input_data_dir);
        let in_chainman =
            ChainstateManagerBuilder::new(&context, &config.input_data_dir, &input_blocks_dir)?
                .worker_threads(config.worker_threads)
                .build()?;

        let output_blocks_dir = format!("{}/blocks", &config.output_data_dir);
        let out_chainman =
            ChainstateManagerBuilder::new(&context, &config.output_data_dir, &output_blocks_dir)?
                .worker_threads(config.worker_threads)
                .build()?;

        ///////////////////////

        let chain = in_chainman.active_chain();
        // Get the current tip
        let tip = chain.tip();
        println!("Chain height: {}", chain.height());
        println!("Tip hash: {}", tip.block_hash());
        let block = in_chainman.read_block_data(&tip)?;
        println!(
            "Transactions count in the last block: {}",
            block.transaction_count()
        );

        println!("-----");

        ///////////////////////

        Ok(Self {
            config,
            in_chainman,
            out_chainman,
        })
    }

    pub fn compress(&self) -> Result<()> {
        let chain = self.in_chainman.active_chain();

        fs::create_dir_all(&self.config.droplets_dir).context("create dir to store droplets")?;

        for entry in chain.iter().take(10) {
            println!(
                ">  Reading block {} at height {}",
                entry.block_hash(),
                entry.height()
            );
            let block = self
                .in_chainman
                .read_block_data(&entry)
                .context("read block data")?;

            let droplet = Droplet::new(entry.height(), block).context("create droplet")?;
            let encoded_droplet = encoding::encode_to_vec(&droplet);
            println!(
                "-> droplet: {}, size: {}, encoded: {} bytes",
                droplet.num,
                droplet.size,
                encoded_droplet.len()
            );

            let droplet_file_path = format!("{}/drp{}.dat", self.config.droplets_dir, droplet.num);
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
        droplet_files.sort();

        for droplet_file_path in droplet_files {
            if !droplet_file_path.is_file() {
                continue;
            }

            let encoded_droplet = fs::read(droplet_file_path.as_path())
                .with_context(|| format!("read droplet file {}", droplet_file_path.display()))?;

            let droplet: Droplet =
                encoding::decode_from_slice(&encoded_droplet).context("decode droplet")?;
            println!("<- reconstructed #{}: {} bytes", droplet.num, droplet.size);

            let block = Block::new(droplet.as_bytes()).context("new block from droplet bytes")?;

            match self.out_chainman.process_block(&block) {
                ProcessBlockResult::NewBlock => {
                    println!("<  Reconstructed block validated and written to disk")
                }
                ProcessBlockResult::Duplicate => {
                    println!("<  Reconstructed block already known (valid)")
                }
                ProcessBlockResult::Rejected => {
                    println!("!!! Reconstructed block validation failed !!!")
                }
            }
        }

        ///////////////////////

        println!("-----");

        let out_chain = self.out_chainman.active_chain();

        // Get the reconstructed tip
        let tip = out_chain.tip();
        println!("Reconstructed chain height: {}", out_chain.height());
        println!("Reconstructed tip hash: {}", tip.block_hash());
        let block = self.out_chainman.read_block_data(&tip)?;
        println!(
            "Transactions count in the last reconstructed block: {}",
            block.transaction_count()
        );

        Ok(())
    }
}
