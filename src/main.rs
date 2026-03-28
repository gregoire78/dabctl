// main.rs - CLI entry point for eti-rtlsdr-rust
// Supports subcommands: iq2eti (RTL-SDR → ETI) and eti2pcm (ETI → PCM audio)

mod iq2eti;
mod eti2pcm_cmd;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "eti-rtlsdr-rust",
    about = "DAB ETI tools: RTL-SDR → ETI and ETI → PCM audio",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate ETI stream from RTL-SDR (IQ → ETI)
    Iq2eti(iq2eti::Iq2etiArgs),

    /// Decode ETI stream to PCM audio (like dablin)
    Eti2pcm(eti2pcm_cmd::Eti2pcmArgs),
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Iq2eti(args) => iq2eti::run(args),
        Commands::Eti2pcm(args) => eti2pcm_cmd::run(args),
    }
}
