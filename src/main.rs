use std::{env, fs, process};

use anyhow::{Context, Result};
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::{
    Block, ChainType, ChainstateManagerBuilder, ContextBuilder, ProcessBlockResult,
};
use fountainhead::droplet::Droplet;

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
    let input_blocks_dir = format!("{}/blocks", input_data_dir);

    let output_data_dir = args[3].clone();
    let output_blocks_dir = format!("{}/blocks", output_data_dir);

    let droplets_dir = args[4].clone();

    let context = ContextBuilder::new()
        .chain_type(ChainType::Signet)
        .build()?;

    let in_chainman = ChainstateManagerBuilder::new(&context, &input_data_dir, &input_blocks_dir)?
        .worker_threads(WORKER_THREADS)
        .build()?;

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

    if command == "compress" {
        fs::create_dir_all(&droplets_dir).context("create dir to store droplets")?;

        for entry in chain.iter().take(10) {
            println!(
                ">  Reading block {} at height {}",
                entry.block_hash(),
                entry.height()
            );
            let block = in_chainman
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

            let droplet_file_path = format!("{}/drp{}.dat", droplets_dir, droplet.num);
            fs::write(droplet_file_path, encoded_droplet)?;
        }
    } else if command == "restore" {
        let out_chainman =
            ChainstateManagerBuilder::new(&context, &output_data_dir, &output_blocks_dir)?
                .worker_threads(WORKER_THREADS)
                .build()?;

        let mut droplet_files = fs::read_dir(&droplets_dir)
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

            match out_chainman.process_block(&block) {
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

        println!("-----");

        let out_chain = out_chainman.active_chain();

        // Get the reconstructed tip
        let tip = out_chain.tip();
        println!("Reconstructed chain height: {}", out_chain.height());
        println!("Reconstructed tip hash: {}", tip.block_hash());
        let block = out_chainman.read_block_data(&tip)?;
        println!(
            "Transactions count in the last reconstructed block: {}",
            block.transaction_count()
        );
    }

    Ok(())
}
