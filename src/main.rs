use std::{env, process};

use anyhow::Result;
use fountainhead::{
    blockchain::Blockchain,
    config::Config,
    encoder::{distribution::RobustSoliton, dummy_encoder::DummyEncoder},
};

/// Number of worker threads for block validation
const WORKER_THREADS: i32 = 8;

fn main() -> Result<()> {
    setup_logging();

    let args: Vec<String> = env::args().collect();

    if args.len() < 5 {
        eprintln!(
            "Usage: {} <COMMAND> <path_to_input_dir> <path_to_output_dir> <path_to_droplets_dir>",
            args[0]
        );
        process::exit(1);
    }

    let command = args[1].clone();
    if !["compress", "restore"].contains(&command.as_str()) {
        eprintln!("Unknown command {}", command);
        process::exit(1);
    }

    let input_data_dir = args[2].clone();
    let output_data_dir = args[3].clone();
    let droplets_dir = args[4].clone();

    let epochs_to_encode = 0; // 0 means the whole blockchain
    let super_blocks_per_epoch = 1000; // TODO

    let config = Config {
        droplets_dir,
        input_data_dir,
        output_data_dir,
        worker_threads: WORKER_THREADS,
        super_blocks_per_epoch,
        epochs_to_encode,
    };

    let degree_distribution = RobustSoliton::new(config.super_blocks_per_epoch, 0.06, 0.01);
    println!(
        "Number of necessary droplets to restore blockchain compressed with using {} superblocks in epoch is {}",
        config.super_blocks_per_epoch,
        degree_distribution.min_encoded_symbols()
    );
    let encoder = DummyEncoder::new(degree_distribution);

    let mut blockchain = Blockchain::new(config, encoder)?;

    if command == "compress" {
        blockchain.compress()?;
    } else if command == "restore" {
        blockchain.restore()?;
    }

    Ok(())
}

fn setup_logging() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();
}
