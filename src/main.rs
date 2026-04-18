#![forbid(unsafe_op_in_unsafe_fn)]

mod app;
mod backend;
mod channel;
mod cli;
mod dab_processor;
mod decoder;
mod device;
mod logging;
mod metadata;
mod ofdm;
mod pcm;

use anyhow::Result;
use clap::Parser;

fn main() {
    if let Err(err) = real_main() {
        tracing::error!(error = %err, "dabctl terminated");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let cli = cli::Cli::parse();
    logging::init(cli.silent)?;

    let mut radio = app::DabRadio::new(cli)?;
    radio.run()
}
