use std::{fs, io::Write};

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
    storage::{Storage, tmp_file_storage::TmpFileStorage},
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
        let chain = self.in_chainman.inner.active_chain();
        let chain_height = chain.height() as usize;
        log::info!("Input chain height: {}", chain_height);

        let epochs_to_encode = if self.config.epochs_to_encode == 0 {
            log::info!(
                "Starting compression of the whole blockchain with {} blocks",
                chain_height
            );
            usize::MAX
        } else {
            log::info!(
                "Starting compression of {} epochs, total {} superblocks",
                self.config.epochs_to_encode,
                self.config.epochs_to_encode * self.config.super_blocks_per_epoch
            );
            self.config.epochs_to_encode
        };

        let encoder = &mut self.encoder;

        let mut epoch_processed_blocks;
        let mut previous_total_processed_blocks = 0;
        let mut total_processed_blocks = 0;
        let mut processed_blocks_height = 0;
        let mut epoch = 0;
        log::info!("Compressing epoch {epoch}, processed block height: {}", 0);

        let mut superblock_storage = TmpFileStorage::new()
            .with_context(|| format!("create superblocks storage for epoch {}", epoch))?;
        let mut super_blocks_count = 0;
        let mut superblock = SuperBlock::new();
        let mut epoch_finished = false;

        // iterating over all blocks
        for (height, entry) in chain.iter().enumerate() {
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
                // block does not fit => start new superblock
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
                let epoch_dir = format!("{}/epoch{:06}", self.config.droplets_dir, epoch);
                fs::create_dir_all(&epoch_dir).context("create epoch dir to store droplets")?;

                encoder.init_epoch(epoch, superblock_storage);
                let mut rng = rand::rng();

                log::info!(
                    "Generating droplets for epoch {} with {} superblocks containing {} blocks",
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
                    let encoded_droplet = droplet.encode_to_bytes();

                    log::debug!(
                        "-> droplet: {}, superblock size: {}, encoded: {} bytes",
                        droplet_num,
                        droplet_size,
                        encoded_droplet.len(),
                    );

                    let droplet_filename = format!("{:06}", num);
                    let droplet_file_path = format!("{}/drp{}.dat", epoch_dir, droplet_filename);
                    fs::write(droplet_file_path, encoded_droplet)
                        .context("write droplet into a file")?;

                    if num.is_multiple_of(100) {
                        print_progress();
                    }
                }
                println!();

                // get rid of processed superblock files eagerly to save used disk space without delay
                encoder.truncate_storage().context("truncate storage")?;

                processed_blocks_height = height;

                if epoch == epochs_to_encode - 1 {
                    log::debug!("Last requested epoch {} reached, finishing", epoch);
                    break;
                }

                // Start new epoch
                epoch += 1;
                log::info!(
                    "Compressing epoch {epoch}, processed block height: {}",
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
                    "Incomplete epoch {} of {} blocks remains uncompressed, finishing",
                    epoch,
                    total_processed_blocks - processed_blocks_height
                );
            }
        }

        let epochs_count_file_path = format!("{}/epochs.dat", self.config.droplets_dir);
        fs::write(epochs_count_file_path, epoch.to_string())?;

        log::info!("All droplets were successfully created");
        log::info!(
            "Number of compressed blocks (block height): {}",
            processed_blocks_height
        );

        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        let out_chain = self.out_chainman.inner.active_chain();

        let epochs_count_file_path = format!("{}/epochs.dat", self.config.droplets_dir);
        let epochs =
            fs::read_to_string(epochs_count_file_path).context("read epochs count from file")?;
        let epochs = epochs.parse::<usize>().context("parse epochs string")?;

        log::info!(
            "Starting restoration of {} epochs, total {} superblocks",
            epochs,
            epochs * self.config.super_blocks_per_epoch
        );

        for epoch in 0..epochs {
            log::info!("Reconstructed chain height: {}", out_chain.height());
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

            log::info!("Decoding droplets for epoch {epoch}");

            // We need blocks decoded in order, so we iterate from 0 to number of droplets
            for num in 0..droplet_files.len() {
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
                        // All blocks from the droplet were inserted
                        break;
                    } else {
                        // Add another droplet to the decoder
                        if let Some(droplet_file_path) = droplet_files.pop() {
                            if !droplet_file_path.is_file() {
                                bail!("Not a file {}", droplet_file_path.display());
                            }

                            let encoded_droplet = fs::read(droplet_file_path.as_path())
                                .with_context(|| {
                                    format!("read droplet file {}", droplet_file_path.display())
                                })?;

                            let droplet = Droplet::decode_from_bytes(&encoded_droplet)
                                .context("decode droplet from file bytes")?;

                            drop(encoded_droplet);

                            decoder
                                .add_droplet(droplet)
                                .context("add droplet to decoder")?;
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
            // next epoch
            println!();
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
