use std::{env, process};

use anyhow::Result;
use fountainhead::{blockchain::Blockchain, config::Config};

/// Number of worker threads for block validation
const WORKER_THREADS: i32 = 8;

fn main() -> Result<()> {
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

    let config = Config {
        droplets_dir,
        input_data_dir,
        output_data_dir,
        worker_threads: WORKER_THREADS,
    };

    let blockchain = Blockchain::new(config)?;

    if command == "compress" {
        blockchain.compress()?;
    } else if command == "restore" {
        blockchain.restore()?;
    }

    Ok(())
}
