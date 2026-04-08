// main.rs - CLI entry point for dabctl
// Supports subcommands: iq2eti (RTL-SDR → ETI), eti2pcm (ETI → PCM audio),
// and iq2pcm (RTL-SDR → PCM audio, direct in-memory pipeline)

mod eti2pcm_cmd;
mod iq2eti;
mod iq2pcm_cmd;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "dabctl",
    about = "DAB tools: RTL-SDR → ETI and RTL-SDR/ETI → PCM audio",
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

    /// Decode DAB radio directly to PCM audio (RTL-SDR → PCM, no ETI intermediate)
    Iq2pcm(iq2pcm_cmd::Iq2pcmArgs),
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Iq2eti(args) => iq2eti::run(args),
        Commands::Eti2pcm(args) => eti2pcm_cmd::run(args),
        Commands::Iq2pcm(args) => iq2pcm_cmd::run(args),
    }
}
