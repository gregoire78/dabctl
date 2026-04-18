use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};

use crate::channel;

#[derive(Debug, Clone, Copy, ValueEnum, Eq, PartialEq)]
pub enum AacDecoderKind {
    Faad2,
    FdkAac,
}

#[derive(Debug, Clone, Parser)]
#[command(
    name = "dabctl",
    version,
    about = "Literal DABstar-style CLI receive path scaffold"
)]
pub struct Cli {
    #[arg(short = 'C', long, value_name = "STRING", value_parser = validate_channel)]
    pub channel: String,

    #[arg(short = 's', long, value_name = "HEX", value_parser = parse_sid)]
    pub sid: u32,

    #[arg(
        short = 'G',
        long,
        value_name = "0-100",
        value_parser = parse_gain,
        conflicts_with_all = ["hardware_agc", "driver_agc", "software_agc"]
    )]
    pub gain: Option<u8>,

    #[arg(long, conflicts_with_all = ["gain", "driver_agc"])]
    pub hardware_agc: bool,

    #[arg(long, conflicts_with_all = ["gain", "hardware_agc"])]
    pub driver_agc: bool,

    #[arg(long, conflicts_with = "gain")]
    pub software_agc: bool,

    #[arg(short = 'l', long, value_name = "STRING")]
    pub label: Option<String>,

    #[arg(short = 'S', long, value_name = "PATH")]
    pub slide_dir: Option<PathBuf>,

    #[arg(long)]
    pub slide_base64: bool,

    #[arg(long)]
    pub silent: bool,

    #[arg(long, value_name = "INT", default_value_t = 0)]
    pub device_index: u32,

    #[arg(long, value_enum, default_value_t = AacDecoderKind::Faad2)]
    pub aac_decoder: AacDecoderKind,
}

fn parse_sid(value: &str) -> Result<u32> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        return u32::from_str_radix(hex, 16).map_err(|err| anyhow!("invalid SID '{value}': {err}"));
    }

    value
        .parse::<u32>()
        .map_err(|err| anyhow!("invalid SID '{value}': {err}"))
}

fn parse_gain(value: &str) -> Result<u8> {
    let gain = value
        .parse::<u8>()
        .map_err(|err| anyhow!("invalid gain '{value}': {err}"))?;
    if gain > 100 {
        return Err(anyhow!("gain must be in the range 0..=100"));
    }
    Ok(gain)
}

fn validate_channel(value: &str) -> Result<String> {
    if channel::channel_to_frequency(value).is_some() {
        return Ok(value.to_ascii_uppercase());
    }

    Err(anyhow!("unsupported DAB channel '{value}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_hex_service_id() {
        let cli = Cli::try_parse_from(["dabctl", "-C", "6C", "-s", "0xF2F8"])
            .expect("CLI should parse a hexadecimal SID");
        assert_eq!(cli.sid, 0xF2F8);
    }

    #[test]
    fn rejects_invalid_gain() {
        let err = Cli::try_parse_from(["dabctl", "-C", "6C", "-s", "0xF2F8", "-G", "255"])
            .expect_err("gain above 100 must be rejected");
        let rendered = err.to_string();
        assert!(rendered.contains("gain"));
    }
}
