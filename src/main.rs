use std::process;

use anyhow::{Context, Result};
use fountainhead::{
    blockchain::{
        compressor::{self, Compressor},
        decompressor::{self, Decompressor},
    },
    cli::{Args, Command},
    encoder::{distribution::RobustSoliton, fountain_encoder::FountainEncoder},
};

/// Number of worker threads for block validation
const WORKER_THREADS: i32 = 8;

fn main() -> Result<()> {
    setup_logging();

    let args = Args::parse().unwrap_or_else(|err| {
        eprintln!("Error: {:?}", err);
        process::exit(1);
    });

    let super_blocks_per_epoch = args.super_blocks_per_epoch;
    let storage_reduction_ratio = args.storage_reduction_ratio;

    let compressor_config = compressor::Config {
        droplets_dir: args.droplets_dir.clone(),
        source_data_dir: args.source_data_dir,
        super_blocks_per_epoch,
        epochs_to_encode: args.epochs_to_encode,
        storage_reduction_ratio,
    };

    let decompressor_config = decompressor::Config {
        droplets_dir: args.droplets_dir,
        super_blocks_per_epoch,
        output_data_dir: args.output_data_dir,
        worker_threads: WORKER_THREADS,
    };

    let degree_distribution =
        RobustSoliton::new(compressor_config.super_blocks_per_epoch, 0.06, 0.01);

    let min_required_droplets_in_epoch = degree_distribution.min_encoded_symbols();
    println!(
        "Number of necessary droplets in each epoch to restore blockchain (compressed using {} superblocks per epoch), is {}",
        compressor_config.super_blocks_per_epoch, min_required_droplets_in_epoch,
    );

    let droplets_produced_in_epoch = super_blocks_per_epoch.div_ceil(storage_reduction_ratio);
    let repetitions_needed = min_required_droplets_in_epoch.div_ceil(droplets_produced_in_epoch);

    //let encoder = fountainhead::encoder::dummy_encoder::DummyEncoder::new(degree_distribution); // TODO
    let encoder = FountainEncoder::new(degree_distribution);
    let mut compressor =
        Compressor::new(compressor_config, encoder).context("create compressor")?;

    match args.command {
        Command::Generate => {
            // Run droplet generation just once
            compressor.generate_droplets()?;
        }
        Command::GenerateAll => {
            // Repeat droplet generation until enough droplets for successful blockchain restoration is created
            for i in 0..repetitions_needed {
                println!("-----------------------------------------------------");
                println!("Round {} of {} needed", i + 1, repetitions_needed);
                compressor.generate_droplets()?;
            }
        }
        Command::Restore => {
            // RESTORE
            let decompressor =
                Decompressor::new(decompressor_config).context("create compressor")?;

            decompressor.restore_blockchain()?;
        }
    }

    Ok(())
}

fn setup_logging() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();
}
