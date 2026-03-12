use std::{fs, io::Write};

use anyhow::{Context as _, Result, bail};
use bitcoinkernel::{
    ChainType, ChainstateManager, ChainstateManagerBuilder, ContextBuilder, ProcessBlockResult,
};

use crate::{
    config::Config,
    decoder::dummy_decoder::DummyDecoder,
    encoder::{distribution::RobustSoliton, dummy_encoder::DummyEncoder},
    storage::{Storage, file_storage::FileStorage, tmp_file_storage::TmpFileStorage},
    super_block::{EncodableBlock, SUPERBLOCK_SIZE, SuperBlock},
};

pub struct Blockchain {
    config: Config,
    in_chainman: InputChainstateManager,
    out_chainman: OutputChainstateManager,
    encoder: DummyEncoder<RobustSoliton, TmpFileStorage>,
}

impl Blockchain {
    pub fn new(
        config: Config,
        encoder: DummyEncoder<RobustSoliton, TmpFileStorage>,
    ) -> Result<Self> {
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

        Ok(Self {
            config,
            in_chainman,
            out_chainman,
            encoder,
        })
    }

    // TODO separate compressor and decompressor

    pub fn compress(&mut self) -> Result<()> {
        const EPOCHS_COUNT_FILE: &str = "epochs_count.dat";
        const LAST_COMPRESSED_BLOCK_FILE: &str = "last_compressed_block.dat";

        let chain = self.in_chainman.inner.active_chain();
        let chain_height = chain.height() as usize;
        log::info!("Input chain height: {}", chain_height);

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
            let epochs_count_file_path =
                format!("{}/{}", self.config.droplets_dir, EPOCHS_COUNT_FILE);
            let epochs_count =
                fs::read_to_string(epochs_count_file_path).unwrap_or_else(|_| "0".to_string());
            let already_compressed_epochs = epochs_count
                .parse::<usize>()
                .context("parse already_compressed_epochs string")?;

            epoch = already_compressed_epochs;

            let last_block_file_path = format!(
                "{}/{}",
                self.config.droplets_dir, LAST_COMPRESSED_BLOCK_FILE
            );
            let last_compressed_block =
                fs::read_to_string(last_block_file_path).unwrap_or_else(|_| "0".to_string());
            already_compressed_blocks = last_compressed_block
                .parse::<usize>()
                .context("parse last_compressed_block string")?;

            total_processed_blocks = already_compressed_blocks;
            processed_blocks_height = total_processed_blocks;
            previous_total_processed_blocks = total_processed_blocks;

            log::info!(
                "Resuming compression of epoch #{epoch} (last compressed block: {})",
                already_compressed_blocks
            );
        } else {
            // Starting from scratch
            if self.config.epochs_to_encode == 0 {
                log::info!(
                    "Starting compression of the whole blockchain with {} blocks",
                    chain_height
                );
            } else {
                log::info!(
                    "Starting compression of {} epochs, total {} superblocks",
                    self.config.epochs_to_encode,
                    self.config.epochs_to_encode * self.config.super_blocks_per_epoch
                );
            };
        }

        // Start compression
        log::info!(
            "Compressing epoch #{epoch}, starting at block height: {}",
            already_compressed_blocks
        );

        let mut superblock_storage = TmpFileStorage::new()
            .with_context(|| format!("create superblocks storage for epoch {}", epoch))?;
        let mut super_blocks_count = 0;
        let mut superblock = SuperBlock::new();
        let mut epoch_finished = false;

        // iterating over all blocks
        for (height, entry) in chain.iter().enumerate().skip(already_compressed_blocks) {
            let block = self
                .in_chainman
                .inner
                .read_block_data(&entry)
                .context("read block data")?;

            let block = EncodableBlock::new(block);

            log::debug!(
                "current superblock len {}, block_size {}",
                superblock.size(),
                block.size(),
            );

            if superblock.size() + block.size() < SUPERBLOCK_SIZE {
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
                    .insert(super_blocks_count, superblock)
                    .context("insert superblock")?;

                super_blocks_count += 1;
                if super_blocks_count.is_multiple_of(100 - 1) {
                    print_progress();
                }

                log::debug!(">> starting new super block");
                superblock = SuperBlock::new();
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

                // last superblock in epoch, add superblock to collection of superblocks
                log::debug!(
                    "== last superblock in epoch {} => closing current superblock, block {}, total {} superblocks",
                    epoch,
                    height,
                    superblock.block_count()
                );
                log::debug!("  adding block {} to super block", height);

                superblock_storage
                    .insert(super_blocks_count, superblock)
                    .context("insert superblock")?;

                super_blocks_count += 1;

                // Generate droplets
                let mut droplet_storage = FileStorage::new(&self.config.droplets_dir, epoch)
                    .with_context(|| format!("create droplet storage for epoch {}", epoch))?;

                encoder.init_epoch(epoch, superblock_storage);
                let mut rng = rand::rng();

                log::info!(
                    "Generating droplets for epoch #{} with {} superblocks containing {} blocks",
                    epoch,
                    super_blocks_count,
                    epoch_processed_blocks,
                );

                for num in 0..super_blocks_count {
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
                        .insert(droplet_num, droplet)
                        .context("store droplet")?;

                    if num.is_multiple_of(100) {
                        print_progress();
                    }
                }

                // All droplets for epoch were generated
                println!();

                // Get rid of processed superblock files eagerly to save used disk space without delay
                encoder.truncate_storage().context("truncate storage")?;

                processed_blocks_height = height;

                if epoch == epochs_to_encode - 1 {
                    log::info!("Last requested epoch #{} reached, finishing", epoch);
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
                    fs::write(last_block_file_path, height.to_string())?;
                }

                // Start new epoch
                epoch += 1;
                log::info!(
                    "Compressing epoch #{epoch}, processed block height: {}",
                    height
                );

                super_blocks_count = 0;
                superblock_storage = TmpFileStorage::new()
                    .with_context(|| format!("create superblocks storage for epoch {}", epoch))?;

                superblock = SuperBlock::new();
                log::debug!(">> starting new super block");
                epoch_finished = false;
            }

            if height == chain_height {
                println!();
                log::info!(
                    "Incomplete epoch #{} of {} blocks remains uncompressed, finishing",
                    epoch,
                    total_processed_blocks - processed_blocks_height
                );
                epoch -= 1;
            }
        }

        log::info!(
            "All droplets in total {} epochs were successfully created",
            epoch + 1
        );
        log::info!(
            "Number of compressed blocks (block height): {}",
            processed_blocks_height
        );

        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        let out_chain = self.out_chainman.inner.active_chain();

        let epochs_count = FileStorage::epoch_count(&self.config.droplets_dir).unwrap_or_default();

        log::info!(
            "Starting restoration of {} epochs, total {} superblocks",
            epochs_count,
            epochs_count * self.config.super_blocks_per_epoch
        );

        for epoch in 0..=epochs_count {
            log::info!("Reconstructed chain height: {}", out_chain.height());
            log::info!("Restoring epoch #{epoch}");

            let mut decoder = DummyDecoder::new();

            log::info!("Decoding droplets for epoch #{epoch}");

            let droplet_storage = FileStorage::new(&self.config.droplets_dir, epoch)
                .with_context(|| format!("open droplet storage for epoch {}", epoch))?;

            // We need blocks decoded in order, so we iterate from 0 to number of droplets
            for num in 0..droplet_storage.count() {
                let mut added_droplets_count = 0;
                loop {
                    decoder
                        .decode()
                        .context("fountain decoder: recover blocks from droplets")?;

                    if let Some(decoded_droplet) = decoder.get_droplet(num) {
                        // Next necessary block was decoded,
                        // insert all its blocks into the blockchain

                        let num = decoded_droplet.num;

                        let blocks = decoded_droplet
                            .into_blocks()
                            .context("get blocks from droplet")?;

                        for (i, block) in blocks.into_iter().enumerate() {
                            match self.out_chainman.inner.process_block(&block) {
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
                        // All blocks from the droplet were inserted into blockchain
                        break;
                    } else {
                        // Add another droplet to the decoder
                        if added_droplets_count < droplet_storage.count() {
                            let droplet = droplet_storage
                                .get(num)
                                .context("get droplet from storage")?;

                            decoder
                                .add_droplet(droplet)
                                .context("add droplet to decoder")?;

                            added_droplets_count += 1;
                        } else {
                            // No more droplet files left
                            break;
                        }
                    }
                }

                if num.is_multiple_of(50) {
                    print_progress();
                }
            }
            // Next epoch
            println!();

            // Remove used droplet files
            droplet_storage
                .truncate()
                .with_context(|| format!("truncate droplet storage for epoch {}", epoch))?;
        }

        ///////////////////////
        log::info!("All blocks from droplets were successfully restored");

        let out_chain = self.out_chainman.inner.active_chain();

        log::info!("Reconstructed chain height: {}", out_chain.height());

        Ok(())
    }
}

fn print_progress() {
    if log::log_enabled!(log::Level::Info) {
        print!(".");
        _ = std::io::stdout().flush();
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
