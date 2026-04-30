mod cli;
mod dablin;

use anyhow::Result;
use cli::{Cli, Commands};
use clap::Parser;

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Dablin(args) => dablin::runner::run(args),
    }
}
