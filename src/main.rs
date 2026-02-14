use std::{env, process};

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

    if args.len() < 4 {
        eprintln!(
            "Usage: {} <COMMAND> <path_to_input_dir> <path_to_output_dir>",
            args[0]
        );
        process::exit(1);
    }

    let command = args[1].clone();
    if !["compress", "reconstruct"].contains(&command.as_str()) {
        eprintln!("Unknown command {}", command);
        process::exit(1);
    }

    let input_data_dir = args[2].clone();
    let input_blocks_dir = format!("{}/blocks", input_data_dir);

    let output_data_dir = args[3].clone();
    let output_blocks_dir = format!("{}/blocks", output_data_dir);

    let context = ContextBuilder::new()
        .chain_type(ChainType::Signet)
        .build()?;

    let in_chainman = ChainstateManagerBuilder::new(&context, &input_data_dir, &input_blocks_dir)?
        .worker_threads(WORKER_THREADS)
        .build()?;

    let out_chainman =
        ChainstateManagerBuilder::new(&context, &output_data_dir, &output_blocks_dir)?
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
        for entry in chain.iter().take(5) {
            println!(
                ">  Reading block {} at height {}",
                entry.block_hash(),
                entry.height()
            );
            let block = in_chainman
                .read_block_data(&entry)
                .context("read block data")?;

            let droplet = Droplet::new(entry.height(), block).context("create droplet")?;
            let encoded = encoding::encode_to_vec(&droplet);
            println!(
                "-> droplet: {}, size: {}, encoded: {} bytes",
                droplet.num,
                droplet.size,
                encoded.len()
            );

            let decoded: Droplet =
                encoding::decode_from_slice(&encoded).context("decode droplet")?;
            println!("<- reconstructed: {} bytes", decoded.size);

            let block = Block::new(decoded.as_bytes()).context("new block from droplet bytes")?;

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
    } else {
        todo!()
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

    Ok(())
}
