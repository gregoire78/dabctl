// main.rs - CLI entry point for dabctl
// RTL-SDR → PCM audio pipeline for DAB/DAB+ radio

mod iq2pcm_cmd;
mod pcm_writer;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "dabctl",
    about = "DAB/DAB+ radio: RTL-SDR → PCM audio",
    version
)]
struct Cli {
    #[command(flatten)]
    args: iq2pcm_cmd::Iq2pcmArgs,
}

fn main() {
    let cli = Cli::parse();
    iq2pcm_cmd::run(cli.args);
}
