use clap::{Parser, Subcommand, ValueEnum};
use std::str::FromStr;

/// dabctl – digital radio toolkit (ETI/DAB+ decoding)
#[derive(Parser)]
#[command(name = "dabctl", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Decode an ETI stream (DAB/DAB+)
    Dablin {
        #[command(subcommand)]
        command: DablinCommand,
    },
}

/// Subcommands for `dabctl dablin`
#[derive(Subcommand, Debug)]
pub enum DablinCommand {
    /// Decode one DAB/DAB+ service to stdout PCM
    OneServiceOut(OneServiceOutArgs),
    /// Export all DAB+ services into per-service directories
    AllServicesOut(AllServicesOutArgs),
    /// List ensemble services then exit
    ListServices(ListServicesArgs),
}

/// Arguments for `dabctl dablin one-service-out`
#[derive(Parser, Debug)]
pub struct OneServiceOutArgs {
    /// ETI input file or stdin (use `-` for stdin)
    #[arg(short = 'i', long = "input")]
    pub input: String,

    /// Service ID to decode (hex, e.g. 0xF2F8)
    #[arg(short = 's', long = "sid")]
    pub sid: Option<String>,

    /// Select service by label
    #[arg(short = 'l', long = "label")]
    pub label: Option<String>,

    /// AAC decoder backend
    #[arg(long = "aac-decoder", default_value = "faad2")]
    pub aac_decoder: AacDecoder,

    /// Audio output format on stdout
    #[arg(long = "audio-out", default_value = "pcm")]
    pub audio_out: AudioOut,

    /// Behavior on missing/invalid AAC frames
    #[arg(long = "aac-gap", default_value = "freeze")]
    pub aac_gap: AacGap,

    /// Disable stderr logging
    #[arg(long = "silent")]
    pub silent: bool,

    /// Directory to save MOT slideshow images
    #[arg(long = "slide-dir")]
    pub slide_dir: Option<String>,

    /// Include slide data as base64 in FD3 metadata
    #[arg(long = "slide-base64")]
    pub slide_base64: bool,

    /// Deduplicate consecutive identical PAD events (DL and slides)
    #[arg(long = "dedup-pad")]
    pub dedup_pad: bool,

    /// Date/time format for metadata `time` events.
    ///
    /// Omit the flag to disable `time` metadata emission.
    /// Pass `--datetime-format` without a value to use `iso8601`.
    #[arg(long = "datetime-format", num_args = 0..=1, default_missing_value = "iso8601")]
    pub datetime_format: Option<DateTimeFormat>,
}

/// Arguments for `dabctl dablin all-services-out`
#[derive(Parser, Debug)]
pub struct AllServicesOutArgs {
    /// ETI input file or stdin (use `-` for stdin)
    #[arg(short = 'i', long = "input")]
    pub input: String,

    /// Output directory for all-services export
    #[arg(short = 'o', long = "out")]
    pub out_dir: String,

    /// AAC decoder backend
    #[arg(long = "aac-decoder", default_value = "faad2")]
    pub aac_decoder: AacDecoder,

    /// Behavior on missing/invalid AAC frames
    #[arg(long = "aac-gap", default_value = "freeze")]
    pub aac_gap: AacGap,

    /// Disable stderr logging
    #[arg(long = "silent")]
    pub silent: bool,

    /// Include slide data as base64 in metadata files
    #[arg(long = "slide-base64")]
    pub slide_base64: bool,

    /// Deduplicate consecutive identical PAD events (DL and slides)
    #[arg(long = "dedup-pad")]
    pub dedup_pad: bool,

    /// Date/time format for metadata `time` events.
    ///
    /// Omit the flag to disable `time` metadata emission.
    /// Pass `--datetime-format` without a value to use `iso8601`.
    #[arg(long = "datetime-format", num_args = 0..=1, default_missing_value = "iso8601")]
    pub datetime_format: Option<DateTimeFormat>,
}

/// Arguments for `dabctl dablin list-services`
#[derive(Parser, Debug)]
pub struct ListServicesArgs {
    /// ETI input file or stdin (use `-` for stdin)
    #[arg(short = 'i', long = "input")]
    pub input: String,

    /// Disable stderr logging
    #[arg(long = "silent")]
    pub silent: bool,
}

/// AAC decoder backend selection
#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum AacDecoder {
    Faad2,
    #[cfg(feature = "fdk-aac")]
    Fdk,
}

/// Audio output format for one-service-out
#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum AudioOut {
    /// Raw PCM (s16le, 48 kHz, stereo)
    Pcm,
    /// Raw AAC wrapped as ADTS (Audio Data Transport Stream)
    Adts,
    /// Raw AAC wrapped as LATM/LOAS
    Latm,
}

/// Behavior on missing/invalid AAC frames
#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum AacGap {
    /// Preserve legacy behavior: no PCM output on error (default)
    Freeze,
    /// Emit PCM silence to keep stream alive
    Silence,
}

/// Formatting style for DAB date/time metadata events
#[derive(Debug, Clone, PartialEq)]
pub enum DateTimeFormat {
    /// Keep human-readable display format
    Human,
    /// Emit ISO 8601 strings (UTC with Z, local with offset)
    Iso8601,
    /// Human-readable time-only output (without date)
    TimeHuman,
    /// ISO 8601 time-only output (without date)
    TimeIso8601,
    /// Custom format template (chrono strftime syntax)
    Custom(String),
}

impl FromStr for DateTimeFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_datetime_format(s)
    }
}

fn parse_datetime_format(s: &str) -> Result<DateTimeFormat, String> {
    match s {
        "" => Ok(DateTimeFormat::Iso8601),
        "human" => Ok(DateTimeFormat::Human),
        "iso8601" => Ok(DateTimeFormat::Iso8601),
        "time-human" => Ok(DateTimeFormat::TimeHuman),
        "time-iso8601" => Ok(DateTimeFormat::TimeIso8601),
        other if !other.trim().is_empty() => Ok(DateTimeFormat::Custom(other.to_string())),
        _ => Err("datetime format cannot be empty".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_parse_dablin_basic() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
        ])
        .unwrap();
        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.input, "test.eti");
                assert_eq!(args.sid, Some("0xF2F8".to_string()));
                assert!(!args.silent);
                assert_eq!(args.aac_gap, AacGap::Freeze);
                assert_eq!(args.audio_out, AudioOut::Pcm);
                assert_eq!(args.datetime_format, None);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_silence_gap() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "-",
            "-s",
            "0xF2F8",
            "--aac-gap",
            "silence",
            "--silent",
        ])
        .unwrap();
        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.input, "-");
                assert_eq!(args.aac_gap, AacGap::Silence);
                assert!(args.silent);
                assert_eq!(args.datetime_format, None);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_list_services() {
        let cli =
            Cli::try_parse_from(["dabctl", "dablin", "list-services", "-i", "test.eti"]).unwrap();
        match cli.command {
            Commands::Dablin {
                command: DablinCommand::ListServices(args),
            } => {
                assert_eq!(args.input, "test.eti");
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_by_label() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-l",
            "France Inter",
        ])
        .unwrap();
        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.label, Some("France Inter".to_string()));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_adts_output() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
            "--audio-out",
            "adts",
        ])
        .unwrap();

        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.audio_out, AudioOut::Adts);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_latm_output() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
            "--audio-out",
            "latm",
        ])
        .unwrap();

        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.audio_out, AudioOut::Latm);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_all_services_out() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "all-services-out",
            "-i",
            "test.eti",
            "--out",
            "out",
        ])
        .unwrap();
        match cli.command {
            Commands::Dablin {
                command: DablinCommand::AllServicesOut(args),
            } => {
                assert_eq!(args.out_dir, "out".to_string());
                assert_eq!(args.datetime_format, None);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_datetime_format_iso8601() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
            "--datetime-format",
            "iso8601",
        ])
        .unwrap();

        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.datetime_format, Some(DateTimeFormat::Iso8601));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_datetime_format_time_human() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
            "--datetime-format",
            "time-human",
        ])
        .unwrap();

        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.datetime_format, Some(DateTimeFormat::TimeHuman));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_datetime_format_time_iso8601() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
            "--datetime-format",
            "time-iso8601",
        ])
        .unwrap();

        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.datetime_format, Some(DateTimeFormat::TimeIso8601));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_datetime_format_no_value_defaults_iso8601() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
            "--datetime-format",
        ])
        .unwrap();

        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.datetime_format, Some(DateTimeFormat::Iso8601));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_datetime_format_empty_value_defaults_iso8601() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
            "--datetime-format",
            "",
        ])
        .unwrap();

        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(args.datetime_format, Some(DateTimeFormat::Iso8601));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_rejects_time_only_flag() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
            "--time-only",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_parse_dablin_datetime_format_custom_template() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "one-service-out",
            "-i",
            "test.eti",
            "-s",
            "0xF2F8",
            "--datetime-format",
            "YYYYescape %Y-%m-%dT%H:%M:%S%:zZ",
        ])
        .unwrap();

        match cli.command {
            Commands::Dablin {
                command: DablinCommand::OneServiceOut(args),
            } => {
                assert_eq!(
                    args.datetime_format,
                    Some(DateTimeFormat::Custom(
                        "YYYYescape %Y-%m-%dT%H:%M:%S%:zZ".to_string()
                    ))
                );
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn test_parse_dablin_all_services_out_rejects_sid() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "all-services-out",
            "-i",
            "test.eti",
            "--out",
            "out",
            "-s",
            "0xF2F8",
        ]);
        assert!(cli.is_err());
    }
}
