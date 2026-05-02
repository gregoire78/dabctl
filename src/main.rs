mod cli;
mod dablin;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Dablin(args) => dablin::runner::run(args),
    }
}
