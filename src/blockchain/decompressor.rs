use anyhow::{Context as _, Result, bail};
use bitcoinkernel::{ChainType, ChainstateManagerBuilder, ContextBuilder, ProcessBlockResult};

use crate::{
    blockchain::{OutputChainstateManager, print_progress},
    decoder::dummy_decoder::DummyDecoder,
    storage::{Storage, file_storage::FileStorage},
};

pub struct Config {
    /// Droplets directory
    pub droplets_dir: String,

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

        for epoch in 0..epochs_count {
            log::info!("Reconstructed chain height: {}", out_chain.height());
            log::info!("Restoring epoch #{epoch}");

            let mut decoder = DummyDecoder::new();

            let droplet_storage = FileStorage::new(&self.config.droplets_dir, epoch)
                .with_context(|| format!("open droplet storage for epoch {}", epoch))?;

            let number_of_droplets = droplet_storage.count();

            log::info!("Decoding {number_of_droplets} droplets for epoch #{epoch}");

            // We need blocks decoded in order, so we iterate from 0 to number of droplets
            for num in 0..number_of_droplets {
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
                        if added_droplets_count < number_of_droplets {
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

        let out_chain = self.output_chainman.inner.active_chain();

        log::info!("Reconstructed chain height: {}", out_chain.height());

        Ok(())
    }
}
