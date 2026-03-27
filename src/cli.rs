// ==============================================================================
// cli.rs - Command-line interface (refactored)
// ==============================================================================

use clap::Parser;
use crate::types::{DabBand, DabConfig, DabMode};

/// ETI-RTL-SDR: DAB to ETI converter
#[derive(Parser, Debug)]
#[command(name = "eti-rtlsdr-rust")]
#[command(about = "Extract ETI frames from DAB broadcast", long_about = None)]
pub struct CliArgs {
    /// Silent mode - no progress output
    #[arg(short = 'S', long = "silent")]
    pub silent: bool,

    /// DAB channel (e.g., 11C, 5A)
    #[arg(short = 'C', long = "channel", default_value = "11C")]
    pub channel: String,

    /// Gain in percent (0-100)
    #[arg(short = 'G', long = "gain", default_value = "50")]
    pub gain: u32,

    /// Frequency correction (PPM)
    #[arg(long = "ppm", default_value = "0")]
    pub ppm: i32,

    /// Enable autogain
    #[arg(long = "autogain")]
    pub autogain: bool,

    /// Output file (- for stdout)
    #[arg(short = 'O', long = "output", default_value = "-")]
    pub output: String,

    /// Device index
    #[arg(long = "device", default_value = "0")]
    pub device_index: u32,

    /// Band (III or L)
    #[arg(short = 'B', long = "band", default_value = "III")]
    pub band: String,

    /// Dump input to raw file (requires compilation with DUMPING)
    #[arg(short = 'R', long = "raw", value_name = "FILE")]
    pub raw_file: Option<String>,

    /// Milliseconds to wait for time synchronization
    #[arg(short = 'd', long = "wait-sync", default_value = "5000")]
    pub wait_sync_ms: u64,

    /// Seconds to collect ensemble information
    #[arg(short = 'D', long = "collect-time", default_value = "10")]
    pub collect_time_secs: u32,

    /// Number of processing threads
    #[arg(short = 'P', long = "processors", default_value = "6")]
    pub num_processors: usize,
}

impl CliArgs {
    /// Valider et convertir en configuration
    pub fn to_config(&self) -> anyhow::Result<DabConfig> {
        let band = match self.band.to_uppercase().as_str() {
            "III" => DabBand::BandIII,
            "L" => DabBand::BandL,
            _ => return Err(anyhow::anyhow!("Invalid band: {}", self.band)),
        };

        if self.gain > 100 {
            return Err(anyhow::anyhow!("Gain must be 0-100, got {}", self.gain));
        }

        Ok(DabConfig {
            mode: DabMode::ModeI,
            band,
            channel: self.channel.clone(),
            gain_percent: self.gain,
            ppm_correction: self.ppm,
            autogain: self.autogain,
            device_index: self.device_index,
            silent: self.silent,
        })
    }

    /// Afficher les informations de configuration
    pub fn print_config(&self) {
        if self.silent {
            return;
        }

        eprintln!("╔════════════════════════════════════════════╗");
        eprintln!("║   ETI-RTL-SDR-Rust v0.2.0               ║");
        eprintln!("║   DAB to ETI Converter                  ║");
        eprintln!("╚════════════════════════════════════════════╝");
        eprintln!();
        eprintln!("Configuration:");
        eprintln!("  Band:            {}", self.band);
        eprintln!("  Channel:         {}", self.channel);
        eprintln!("  Gain:            {}%", self.gain);
        eprintln!("  PPM Correction:  {}", self.ppm);
        eprintln!("  Autogain:        {}", if self.autogain { "ON" } else { "OFF" });
        eprintln!("  Device:          {}", self.device_index);
        eprintln!("  Processors:      {}", self.num_processors);
        eprintln!("  Output:          {}", self.output);
        eprintln!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_to_config() {
        let args = CliArgs {
            silent: true,
            channel: "11C".to_string(),
            gain: 50,
            ppm: 0,
            autogain: false,
            output: "-".to_string(),
            device_index: 0,
            band: "III".to_string(),
            raw_file: None,
            wait_sync_ms: 5000,
            collect_time_secs: 10,
            num_processors: 6,
        };

        let config = args.to_config().unwrap();
        assert_eq!(config.channel, "11C");
        assert_eq!(config.gain_percent, 50);
    }

    #[test]
    fn test_cli_invalid_band() {
        let args = CliArgs {
            silent: true,
            channel: "11C".to_string(),
            gain: 50,
            ppm: 0,
            autogain: false,
            output: "-".to_string(),
            device_index: 0,
            band: "INVALID".to_string(),
            raw_file: None,
            wait_sync_ms: 5000,
            collect_time_secs: 10,
            num_processors: 6,
        };

        assert!(args.to_config().is_err());
    }
}
