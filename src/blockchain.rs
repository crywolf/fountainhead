use std::{collections::BTreeMap, fs};

use anyhow::{Context as _, Result, bail};
use bitcoinkernel::{
    ChainType, ChainstateManager, ChainstateManagerBuilder, ContextBuilder, ProcessBlockResult,
};
use rand::seq::SliceRandom;

use crate::{
    config::Config,
    decoder::dummy_decoder::DummyDecoder,
    droplet::Droplet,
    encoder::{distribution::RobustSoliton, dummy_encoder::DummyEncoder},
    super_block::{DEFAULT_SUPERBLOCK_SIZE, SuperBlock},
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
        log::info!("Initializing blockchain reader");
        log::info!("Active chain height: {}", chain.height());

        ///////////////////////

        Ok(Self {
            config,
            in_chainman,
            out_chainman,
            encoder,
        })
    }

    // TODO separate compressor and decompressor

    pub fn compress(&mut self) -> Result<()> {
        log::info!(
            "Starting compression of {} epochs, total {} blocks",
            self.config.epochs_to_encode,
            self.config.epochs_to_encode * self.config.blocks_per_epoch
        );

        let blocks_per_epoch = self.config.blocks_per_epoch;
        let encoder = &mut self.encoder;

        let chain = self.in_chainman.inner.active_chain();

        for epoch in 0..self.config.epochs_to_encode {
            // iterating over all requested epochs
            log::info!("Compressing epoch {epoch}");

            let epoch_dir = format!("{}/epoch{:06}", self.config.droplets_dir, epoch);
            fs::create_dir_all(&epoch_dir).context("create epoch dir to store droplets")?;

            // first we need to know max block size in epoch for blocks concatenation and padding
            let mut max_block_size_in_epoch = 0;
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
                // Isn't there a better way to get the block size??
                // TODO - make a lookup table?
                let block_size = block.consensus_encode()?.len();
                if block_size > max_block_size_in_epoch {
                    max_block_size_in_epoch = block_size;
                }
            }

            // Superblock size must be at least the size of the largest block in epoch
            // We also need to encode number of blocks in the vector and the total number of bytes, so we add some overhead
            let max_superblock_size =
                std::cmp::max(DEFAULT_SUPERBLOCK_SIZE, max_block_size_in_epoch + 2 * 5 + 1);

            log::info!(
                "Max superblock size: {} | max_block_size_in_epoch: {}",
                max_superblock_size,
                max_block_size_in_epoch
            );

            let mut super_blocks = Vec::new();
            let mut superblock = SuperBlock::new();
            for (height, entry) in chain
                .iter()
                .enumerate()
                .skip(epoch * blocks_per_epoch)
                .take(blocks_per_epoch)
            {
                // iterating over blocks in epoch

                let block = self
                    .in_chainman
                    .inner
                    .read_block_data(&entry)
                    .context("read block data")?;

                // TODO use lookup table
                let block_size = block.consensus_encode()?.len();
                use bitcoin_consensus_encoding::Encoder;
                let size_encoder = bitcoin_consensus_encoding::CompactSizeEncoder::new(block_size);
                let block_size = block_size + size_encoder.current_chunk().len();

                log::debug!(
                    "current superblock len {}, block_size {}",
                    superblock.size(),
                    block_size,
                );
                if superblock.size() + block_size < max_superblock_size {
                    // block fits in superblock => add it
                    log::debug!("  adding block {} to super block", height);
                    superblock
                        .add(block)
                        .context("adding block to super block")?;
                } else {
                    // block does not fit => start new superblock
                    log::debug!(
                        "-- closing current super block with {} blocks",
                        superblock.block_count()
                    );

                    super_blocks.push(superblock);

                    log::debug!(">> starting new super block");
                    superblock = SuperBlock::new();
                    log::debug!("  adding block {} to super block", height);
                    superblock
                        .add(block)
                        .context("adding block to super block")?;
                }
                if (height + 1).is_multiple_of(blocks_per_epoch) {
                    // last block in epoch, add superblock to collection of superblocks
                    log::debug!(
                        "== last block in epoch {} => closing current super block, block {}, total {} superblocks",
                        epoch,
                        height,
                        superblock.block_count()
                    );
                    super_blocks.push(superblock);
                    break;
                }
            }

            // TODO decide what to do with the last (incomplete) epoch
            // assert_eq!(
            //     blocks_per_epoch,
            //     super_blocks.len(),
            //     "Not enough blocks per epoch"
            // );
            let super_blocks_len = super_blocks.len();

            encoder.init_epoch(epoch, super_blocks);
            let mut rng = rand::rng();

            log::info!(
                "Generating droplets for epoch {} with {} superblocks",
                epoch,
                super_blocks_len
            );
            for num in 0..super_blocks_len {
                let droplet = encoder
                    .generate_droplet(&mut rng)
                    .with_context(|| format!("generate droplet {}", num))?;

                let encoded_droplet = droplet.encode_to_bytes();

                log::debug!(
                    "-> droplet: {}, superblock size: {}, encoded: {} bytes",
                    droplet.num,
                    droplet.data_size(),
                    encoded_droplet.len(),
                );

                let droplet_filename = format!("{:06}", num);
                let droplet_file_path = format!("{}/drp{}.dat", epoch_dir, droplet_filename);
                fs::write(droplet_file_path, encoded_droplet)?;
            }
        }

        log::info!("All droplets were successfully created");

        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        for epoch in 0..self.config.epochs_to_encode {
            log::info!("Restoring epoch {epoch}");

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

                let droplet = Droplet::decode_from_bytes(&encoded_droplet)
                    .context("decode droplet from file bytes")?;

                decoder
                    .add_droplet(droplet)
                    .context("add droplet to decoder")?;
            }

            let mut recovered_blocks = BTreeMap::new();

            decoder
                .decode(&mut recovered_blocks)
                .context("fountain decoder: recover blocks from droplets")?;

            // process queued super blocks
            for (num, blocks) in recovered_blocks.iter() {
                for (i, block) in blocks.iter().enumerate() {
                    match self.out_chainman.inner.process_block(block) {
                        ProcessBlockResult::NewBlock => {
                            log::debug!(
                                "<  Droplet #{num}: block #{i:<2} from superblock validated and written to disk"
                            )
                        }
                        ProcessBlockResult::Duplicate => {
                            log::debug!(
                                "<  Droplet #{num}: block #{i:<2} from superblock already known (valid)"
                            )
                        }
                        ProcessBlockResult::Rejected => {
                            log::error!(
                                "!! Droplet #{num}: block #{i:<2} from superblock validation failed!"
                            );
                            bail!(
                                "Droplet #{num}: block #{i:<2} from superblock validation failed!"
                            )
                        }
                    }
                }
            }
        }

        ///////////////////////
        log::info!("All blocks from droplets were successfully restored");

        let out_chain = self.out_chainman.inner.active_chain();

        // Get the reconstructed tip
        let tip = out_chain.tip();
        log::info!("Reconstructed chain height: {}", out_chain.height());
        log::info!("Reconstructed tip hash: {}", tip.block_hash());
        let block = self.out_chainman.inner.read_block_data(&tip)?;
        log::info!(
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
