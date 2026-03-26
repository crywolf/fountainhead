use std::{fs::File, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;

/// Configuration object for the application
pub struct Args {
    /// The name of the called command
    pub command: Command,
    pub source_data_dir: String,
    pub output_data_dir: String,
    pub droplets_dir: String,
    pub header_chain_dir: String,
    pub epochs_to_encode: usize,
    pub super_blocks_per_epoch: usize,
    pub storage_reduction_ratio: usize,
}

impl Args {
    /// Creates and initializes a new config.
    pub fn parse() -> Result<Self> {
        let args = Cli::parse();

        let config_path = args.config.unwrap_or(PathBuf::from("config.yaml"));
        let file = File::open(config_path.clone())
            .with_context(|| format!("open config file {}", config_path.display()))?;

        let config: ConfigYaml =
            yaml_serde::from_reader(file).context("parsing yaml config file")?;

        if config.storage_reduction_ratio < 1 {
            anyhow::bail!("Config: compression ratio must be higher than 0");
        }

        Ok(Self {
            command: args.command,
            source_data_dir: config.source_data_dir,
            output_data_dir: config.output_data_dir,
            droplets_dir: config.droplets_dir,
            header_chain_dir: config.header_chain_dir,
            epochs_to_encode: config.epochs_to_encode,
            super_blocks_per_epoch: config.super_blocks_per_epoch,
            storage_reduction_ratio: config.storage_reduction_ratio,
        })
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Sets a custom config file (default: "config.yaml" )
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

/// Application commands
#[derive(Subcommand)]
pub enum Command {
    /// Generate droplets from the input blockchain according to chosen `storage reduction ratio`
    Generate,
    /// Repeat droplet generation until enough droplets to successfully restore the blockchain were created
    #[command(name = "generate-all")]
    GenerateAll,
    /// Generate header-chain from the input blockchain
    #[command(name = "headerchain")]
    HeaderChain,
    /// Attempts to restore the blockchain from droplets
    Restore,
}

#[derive(Deserialize)]
struct ConfigYaml {
    source_data_dir: String,
    output_data_dir: String,
    droplets_dir: String,
    header_chain_dir: String,
    epochs_to_encode: usize,
    super_blocks_per_epoch: usize,
    storage_reduction_ratio: usize,
}
