use clap::{Parser, Subcommand};

/// Configuration object for the application
pub struct Args {
    /// The name of the called command
    pub command: Command,
    pub source_data_dir: String,
    pub output_data_dir: String,
    pub droplets_dir: String,
    // TODO Path to the config file
    //pub config_path: PathBuf,
}

impl Args {
    /// Creates and initializes a new config.
    pub fn parse() -> Result<Self, String> {
        let args = Cli::parse();

        Ok(Self {
            command: args.command,
            source_data_dir: args.source_data_dir,
            output_data_dir: args.output_data_dir,
            droplets_dir: args.droplets_dir,
            //config_path: args.config.unwrap_or(PathBuf::from("config.yaml")),
        })
    }
}

/// Application arguments
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    // TODO Sets a custom config file (default: "config.yaml" )
    // #[arg(short, long, value_name = "FILE")]
    //config: Option<PathBuf>,
    #[arg(short, long, value_name = "FILE")]
    pub source_data_dir: String,
    #[arg(short, long, value_name = "FILE")]
    pub output_data_dir: String,
    #[arg(short, long, value_name = "FILE")]
    pub droplets_dir: String,

    #[command(subcommand)]
    command: Command,
}

/// Application commands
#[derive(Subcommand)]
pub enum Command {
    /// Generate droplets according to chosen `storage reduction ratio`
    #[command(name = "gen")]
    Generate,
    /// Generate enough droplets to successfully restore the blockchain
    #[command(name = "gen-all")]
    GenerateAll,
    /// Attempts to restore the blockchain from droplets
    Restore,
}
