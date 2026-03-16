use anyhow::{Context as _, Result, bail};
use bitcoinkernel::{ChainType, ChainstateManagerBuilder, ContextBuilder, ProcessBlockResult};

use crate::{
    blockchain::{OutputChainstateManager, print_progress},
    decoder::fountain_decoder::FountainDecoder,
    storage::{Storage, file_storage::FileStorage},
};

pub struct Config {
    /// Droplets directory
    pub droplets_dir: String,

    /// How many super blocks is one epoch encoded into
    pub super_blocks_per_epoch: usize,

    /// Directory where the restored BTC blockchain should be placed
    pub output_data_dir: String,

    /// Number of worker threads for block validation
    pub worker_threads: i32,
}

pub struct Decompressor {
    config: Config,
    output_chainman: OutputChainstateManager,
}

impl Decompressor {
    pub fn new(config: Config) -> Result<Self> {
        let context = ContextBuilder::new()
            .chain_type(ChainType::Signet)
            .build()?;

        let output_blocks_dir = format!("{}/blocks", &config.output_data_dir);
        let output_chainman = OutputChainstateManager::from(
            ChainstateManagerBuilder::new(&context, &config.output_data_dir, &output_blocks_dir)?
                .worker_threads(config.worker_threads)
                .build()?,
        );

        Ok(Self {
            config,
            output_chainman,
        })
    }

    pub fn restore(&self) -> Result<()> {
        let out_chain = self.output_chainman.inner.active_chain();

        let epochs_count = FileStorage::epoch_count(&self.config.droplets_dir).unwrap_or_default();

        log::info!("Starting restoration of {} epochs", epochs_count,);

        // Iterate over all epochs
        for epoch in 0..epochs_count {
            log::info!("Reconstructed chain height: {}", out_chain.height());
            log::info!("Restoring epoch #{epoch}");

            //let mut decoder = crate::decoder::dummy_decoder::DummyDecoder::new(); // TODO
            let mut decoder = FountainDecoder::new();

            let droplet_storage = FileStorage::new(&self.config.droplets_dir, epoch)
                .with_context(|| format!("open droplet storage for epoch {}", epoch))?;

            let number_of_droplets = droplet_storage.count();
            let number_of_super_blocks = self.config.super_blocks_per_epoch;

            log::info!(
                "Decoding {number_of_super_blocks} superblocks from available {number_of_droplets} droplets for epoch #{epoch}"
            );

            let mut added_droplets_count = 0;

            // Iterate over all available droplets in epoch
            // We need blocks decoded in order, so we iterate from 0 to number of superblocks
            for superblock_num in 0..number_of_super_blocks {
                //dbg!(superblock_num);
                loop {
                    decoder
                        .decode()
                        .context("fountain decoder: recover blocks from droplets")?;

                    if let Some(decoded_droplet) = decoder.get_decoded_droplet(superblock_num) {
                        // Next necessary block was decoded,
                        // insert all its blocks into the blockchain

                        let num = superblock_num;
                        log::info!("- - - Blockchain: Adding superblock {num}");

                        let blocks = decoded_droplet
                            .into_blocks()
                            .context("get blocks from droplet");

                        match blocks {
                            Ok(_) => {}
                            Err(err) => {
                                log::error!("{:?}", err);
                                break;
                            }
                        }
                        let blocks = blocks?;

                        for (i, block) in blocks.into_iter().enumerate() {
                            match self.output_chainman.inner.process_block(&block) {
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

                        log::info!("Add droplet to decoder: {}", added_droplets_count);
                        if added_droplets_count < number_of_droplets {
                            let droplet =
                                droplet_storage.get(added_droplets_count).with_context(|| {
                                    format!("get droplet {} from storage", added_droplets_count)
                                })?;
                            dbg!(droplet.superblock().clone().into_encoded_bytes().len());
                            decoder
                                .add_encoded_droplet(droplet)
                                .context("add droplet to decoder")?;

                            added_droplets_count += 1;
                        } else {
                            // No more droplet files left
                            log::error!(
                                "Used all {added_droplets_count} droplets. No more droplet files left, need more droplets!"
                            );
                            bail!("No more droplet files left, need more droplets!");
                        }
                    }
                }

                if superblock_num.is_multiple_of(50) {
                    print_progress();
                }
            }
            // Next epoch
            println!();

            // Remove used droplet files // TODO
            // droplet_storage
            //     .truncate()
            //     .with_context(|| format!("truncate droplet storage for epoch {}", epoch))?;
        }

        ///////////////////////
        log::info!("All blocks from droplets were successfully restored");

        let out_chain = self.output_chainman.inner.active_chain();

        log::info!("Reconstructed chain height: {}", out_chain.height());

        Ok(())
    }
}
