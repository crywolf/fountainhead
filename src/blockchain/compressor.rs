use std::fs;

use anyhow::{Context as _, Result};
use bitcoinkernel::{ChainType, ChainstateManagerBuilder, ContextBuilder};

use crate::{
    blockchain::{InputChainstateManager, print_progress},
    encoder::{distribution::RobustSoliton, fountain_encoder::FountainEncoder},
    storage::{Storage, file_storage::FileStorage, tmp_file_storage::TmpFileStorage},
    super_block::{RawBlock, SuperBlock},
};

pub struct Config {
    /// Droplets directory
    pub droplets_dir: String,

    /// Directory containing BTC blockchain data to be compressed
    pub source_data_dir: String,

    /// How many super blocks are produced in an epoch.
    /// An epoch is defined as the time required for the blockchain to grow by `k` blocks (e.g., `k` = 10000).
    /// Here we use super blocks (that contain more concatenated blocks) instead of blocks
    pub super_blocks_per_epoch: usize,

    /// Number of epochs to encode, 0 means encode the whole blockchain
    pub epochs_to_encode: usize,

    /// Compression ratio. For example 10 means 1:10 blockchain disk space savings.
    pub compression_ratio: usize,
}

pub struct Compressor {
    config: Config,
    input_chainman: InputChainstateManager,
    encoder: FountainEncoder<RobustSoliton, TmpFileStorage>,
    //TODO
    //encoder: crate::encoder::dummy_encoder::DummyEncoder<RobustSoliton, TmpFileStorage>,
}

impl Compressor {
    pub fn new(
        config: Config,
        // TODO
        //encoder: crate::encoder::dummy_encoder::DummyEncoder<RobustSoliton, TmpFileStorage>,
        encoder: FountainEncoder<RobustSoliton, TmpFileStorage>,
    ) -> Result<Self> {
        let context = ContextBuilder::new()
            .chain_type(ChainType::Signet)
            .build()?;

        let input_blocks_dir = format!("{}/blocks", &config.source_data_dir);
        let input_chainman = InputChainstateManager::from(
            ChainstateManagerBuilder::new(&context, &config.source_data_dir, &input_blocks_dir)?
                .build()?,
        );

        Ok(Self {
            config,
            input_chainman,
            encoder,
        })
    }

    pub fn compress(&mut self) -> Result<()> {
        const EPOCHS_COUNT_FILE: &str = "epochs_count.dat";
        const LAST_COMPRESSED_BLOCK_FILE: &str = "last_compressed_block.dat";

        let chain = self.input_chainman.inner.active_chain();
        let chain_height = chain.height() as usize;
        println!("Input chain height: {}", chain_height);

        let epochs_to_encode = if self.config.epochs_to_encode == 0 {
            usize::MAX
        } else {
            self.config.epochs_to_encode
        };

        let encoder = &mut self.encoder;

        let mut epoch_processed_blocks;
        let mut previous_total_processed_blocks = 0;
        let mut total_processed_blocks = 0;
        let mut processed_blocks_height = 0;
        let mut epoch = 0;
        let mut already_compressed_blocks = 0;

        // Determine if we start from scratch or resume interrupted compression
        if FileStorage::epoch_count(&self.config.droplets_dir).unwrap_or_default() > 1 {
            // Resuming interrupted compression

            // TODO put path creation into function
            let epochs_count_file_path =
                format!("{}/{}", self.config.droplets_dir, EPOCHS_COUNT_FILE);
            let epochs_count =
                fs::read_to_string(epochs_count_file_path).unwrap_or_else(|_| "0".to_string());
            let already_compressed_epochs = epochs_count
                .parse::<usize>()
                .context("parse already_compressed_epochs string")?;

            epoch = already_compressed_epochs;

            // TODO put path creation into function
            let last_block_file_path = format!(
                "{}/{}",
                self.config.droplets_dir, LAST_COMPRESSED_BLOCK_FILE
            );
            let last_compressed_block =
                fs::read_to_string(last_block_file_path).unwrap_or_else(|_| "0".to_string());
            already_compressed_blocks = last_compressed_block
                .parse::<usize>()
                .context("parse last_compressed_block string")?;
            //+ 1;

            total_processed_blocks = already_compressed_blocks;
            processed_blocks_height = total_processed_blocks;
            previous_total_processed_blocks = total_processed_blocks;

            println!(
                "Resuming compression of epoch #{epoch} (last compressed block: {})",
                already_compressed_blocks
            );
        } else {
            // Starting from scratch
            if self.config.epochs_to_encode == 0 {
                println!(
                    "Starting compression of the whole blockchain with {} blocks",
                    chain_height
                );
            } else {
                println!(
                    "Starting compression of {} epochs, total {} superblocks",
                    self.config.epochs_to_encode,
                    self.config.epochs_to_encode
                        * self
                            .config
                            .super_blocks_per_epoch
                            .div_ceil(self.config.compression_ratio),
                );
            };
        }

        // Start compression
        println!(
            "Compressing epoch #{epoch}, processed block height: {}",
            already_compressed_blocks
        );

        println!(
            "Constructing superblocks for epoch #{epoch}, starting at block height: {}",
            already_compressed_blocks
        );

        let mut superblock_storage = TmpFileStorage::new()
            .with_context(|| format!("create superblocks storage for epoch {}", epoch))?;
        let mut super_blocks_count = 0;
        let mut superblock = SuperBlock::new(super_blocks_count);
        let mut epoch_finished = false;

        // Iterating over all blocks
        for (height, entry) in chain.iter().enumerate().skip(already_compressed_blocks) {
            let block = self
                .input_chainman
                .inner
                .read_block_data(&entry)
                .context("read block data")?;

            let block = RawBlock::new(&block.consensus_encode().expect("should be valid block"));

            log::debug!(
                "current superblock len {}, block_size {}",
                superblock.size(),
                block.size(),
            );

            if superblock.available_space() >= block.size() {
                // block fits in superblock => add it
                log::debug!("  adding block {} to super block", height);
                superblock
                    .add(block)
                    .context("adding block to super block")?;
            } else {
                // block does not fit in => start new superblock
                log::debug!(
                    "-- closing current super block with {} blocks",
                    superblock.block_count()
                );

                superblock_storage
                    .insert(&super_blocks_count, superblock)
                    .context("insert superblock")?;

                if super_blocks_count.is_multiple_of(20) {
                    print_progress();
                }
                super_blocks_count += 1;

                log::debug!(">> starting new super block");
                superblock = SuperBlock::new(super_blocks_count);
                log::debug!("  adding block {} to super block", height);
                superblock
                    .add(block)
                    .context("adding block to super block")?;
            }

            total_processed_blocks += 1;

            if super_blocks_count == self.config.super_blocks_per_epoch - 1 {
                // Full epoch finished
                epoch_finished = true;
            }

            if epoch_finished {
                println!();
                // Finalize epoch and start a new one
                epoch_processed_blocks = total_processed_blocks - previous_total_processed_blocks;
                previous_total_processed_blocks = total_processed_blocks;

                // Last superblock in epoch, add superblock to collection of superblocks
                log::debug!(
                    "== last superblock in epoch {} => closing current superblock, block {}, total {} superblocks",
                    epoch,
                    height,
                    superblock.block_count()
                );
                log::debug!("  adding block {} to super block", height);

                superblock_storage
                    .insert(&super_blocks_count, superblock)
                    .context("insert superblock")?;

                super_blocks_count += 1;

                // Generate droplets
                let mut droplet_storage = FileStorage::new(&self.config.droplets_dir, epoch)
                    .with_context(|| format!("create droplet storage for epoch {}", epoch))?;

                encoder.init_epoch(epoch, superblock_storage, droplet_storage.count());
                let mut rng = rand::rng();

                // Number of droplets according to compression ratio
                let produced_droplets = super_blocks_count.div_ceil(self.config.compression_ratio);

                println!(
                    "Generating {} droplets for epoch #{} (which consists of {} superblocks, containing {} blocks)",
                    produced_droplets, epoch, super_blocks_count, epoch_processed_blocks,
                );

                for num in 0..produced_droplets {
                    let droplet = encoder
                        .generate_droplet(&mut rng)
                        .with_context(|| format!("generate droplet {}", num))?;

                    let droplet_num = droplet.num;
                    let droplet_size = droplet.data_size();

                    log::debug!(
                        "-> droplet: {}, superblock size: {}",
                        droplet_num,
                        droplet_size,
                    );

                    droplet_storage
                        .insert(&droplet_num, droplet)
                        .context("store droplet")?;

                    if num.is_multiple_of(20) {
                        print_progress();
                    }
                }

                // All droplets for epoch were generated
                println!();

                // Get rid of processed superblock files eagerly to save used disk space without delay
                encoder.truncate_storage().context("truncate storage")?;

                processed_blocks_height = height;

                if epoch == epochs_to_encode - 1 {
                    println!("Last requested epoch #{} reached, finishing", epoch);
                    break;
                } else {
                    // Store last compressed epoch and block to enable resuming interrupted compression
                    let epochs_count_file_path =
                        format!("{}/{}", self.config.droplets_dir, EPOCHS_COUNT_FILE);
                    fs::write(epochs_count_file_path, (epoch + 1).to_string())?;

                    let last_block_file_path = format!(
                        "{}/{}",
                        &self.config.droplets_dir, LAST_COMPRESSED_BLOCK_FILE
                    );
                    fs::write(last_block_file_path, processed_blocks_height.to_string())?;
                }

                // Start new epoch
                epoch += 1;
                println!(
                    "Compressing epoch #{epoch}, processed block height: {}",
                    height
                );
                println!(
                    "Constructing superblocks for epoch #{epoch}, starting at block height: {}",
                    already_compressed_blocks
                );

                super_blocks_count = 0;
                superblock_storage = TmpFileStorage::new()
                    .with_context(|| format!("create superblocks storage for epoch {}", epoch))?;

                superblock = SuperBlock::new(super_blocks_count);
                log::debug!(">> starting new super block");
                epoch_finished = false;
            }

            if height == chain_height {
                println!();
                println!(
                    "Incomplete epoch #{} of {} blocks remains uncompressed, finishing",
                    epoch,
                    total_processed_blocks - processed_blocks_height
                );
                epoch -= 1;
            }
        }

        println!(
            "All droplets in {} epochs were successfully created",
            epoch + 1
        );
        println!(
            "Number of compressed blocks (block height): {}",
            processed_blocks_height
        );

        // TODO put path creation into function
        let epochs_count_file_path = format!("{}/{}", self.config.droplets_dir, EPOCHS_COUNT_FILE);
        _ = fs::remove_file(epochs_count_file_path);

        // TODO put path creation into function
        let last_block_file_path = format!(
            "{}/{}",
            self.config.droplets_dir, LAST_COMPRESSED_BLOCK_FILE
        );
        _ = fs::remove_file(last_block_file_path);

        Ok(())
    }
}
