use std::fs;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{anyhow, Result};
use tracing::info;

use crate::channel;
use crate::cli::Cli;
use crate::dab_processor::{DabProcessor, ReceiverConfig};
use crate::metadata::MetadataWriter;
use crate::pcm::PcmOutput;

// Literal Rust translation scaffold of DABstar's DabRadio orchestration.
pub struct DabRadio {
    cli: Cli,
    metadata: MetadataWriter,
    pcm: PcmOutput,
}

impl DabRadio {
    pub fn new(cli: Cli) -> Result<Self> {
        if let Some(path) = &cli.slide_dir {
            fs::create_dir_all(path)?;
        }

        Ok(Self {
            cli,
            metadata: MetadataWriter::from_fd3()?,
            pcm: PcmOutput::stdout(),
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let center_freq_hz = channel::channel_to_frequency(&self.cli.channel)
            .ok_or_else(|| anyhow!("unsupported DAB channel {}", self.cli.channel))?;

        let config = ReceiverConfig::from_cli(&self.cli, center_freq_hz);
        info!(
            channel = %config.channel,
            sid = %format!("0x{:04X}", config.sid),
            "starting DABstar-style receive path"
        );

        let running = Arc::new(AtomicBool::new(true));
        let running_ctrlc = Arc::clone(&running);
        let _ = ctrlc::set_handler(move || {
            running_ctrlc.store(false, Ordering::SeqCst);
        });

        let mut processor = DabProcessor::new(config);
        processor.run(&mut self.metadata, &mut self.pcm, running)
    }
}
