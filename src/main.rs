// main.rs - CLI entry point for dabctl
// RTL-SDR → PCM audio pipeline for DAB/DAB+ radio

mod iq2pcm_cmd;
mod pcm_writer;
mod scan_cmd;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "dabctl",
    about = "DAB/DAB+ radio: RTL-SDR → PCM audio",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Play a DAB+ service: tune a channel and decode audio to stdout (PCM s16le 48kHz stereo)
    Play(iq2pcm_cmd::Iq2pcmArgs),
    /// Scan a DAB channel and list all available ensembles and services
    Scan(scan_cmd::ScanArgs),
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Play(args) => iq2pcm_cmd::run(args),
        Commands::Scan(args) => scan_cmd::run(args),
    }
}
