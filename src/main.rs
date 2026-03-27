// ==============================================================================
// main.rs - CLI binary (refactored with clean code architecture)
// ==============================================================================

use anyhow::Result;
use clap::Parser;
use eti_rtlsdr_rust::cli::CliArgs;
use eti_rtlsdr_rust::callbacks::{CallbackHub, EtiWriter};
use eti_rtlsdr_rust::eti_pipeline::EtiPipeline;
use std::io::{self, Write};
use std::sync::Arc;

/// EtiWriter implementation that writes ETI frames to stdout
struct StdoutEtiWriter;

impl EtiWriter for StdoutEtiWriter {
    fn write_eti_frame(&self, data: &[u8]) -> anyhow::Result<()> {
        io::stdout().write_all(data)?;
        io::stdout().flush()?;
        Ok(())
    }
}

fn main() -> Result<()> {
    // Parse command-line arguments
    let args = CliArgs::parse();

    // Display configuration (unless silent mode)
    args.print_config();

    // Convert CLI args to DabConfig
    let config = args.to_config()?;

    // Create callback hub with ETI writer
    let eti_writer = Arc::new(StdoutEtiWriter);
    let callbacks = CallbackHub::new().with_eti_writer(eti_writer);

    // Create ETI pipeline
    let pipeline = EtiPipeline::new(config, callbacks)?;

    if !args.silent {
        eprintln!("✅ Pipeline initialized successfully");
        eprintln!("📊 Ready to process IQ samples from RTL-SDR device");
        eprintln!("🎯 Channel: {}, Gain: {}%, PPM: {}", 
                  args.channel, args.gain, args.ppm);
    }

    // Signal sync (in production, would detect from OFDM sync)
    pipeline.signal_sync();

    if !args.silent {
        eprintln!("⏳ Waiting for input stream...");
        eprintln!("💡 Tip: Use 'eti-rtlsdr-rust -h' for all options");
    }

    // Note: In a production implementation:
    // 1. Create IQ source (RTL-SDR or file replay)
    // 2. Read IQ samples in chunks
    // 3. Call pipeline.process_iq_block() for each chunk
    // 4. ETI frames are written via EtiWriter callback
    //
    // For now, this demonstrates the refactored CLI architecture.

    if !args.silent {
        eprintln!("✨ ETI-RTL-SDR Rust pipeline is operational");
    }

    Ok(())
}
