use clap::{Parser, Subcommand, ValueEnum};

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
    Dablin(DablinArgs),
}

/// Arguments for the `dablin` subcommand
#[derive(Parser, Debug)]
pub struct DablinArgs {
    /// ETI input file or stdin (use `-` for stdin)
    #[arg(short = 'i', long = "input")]
    pub input: String,

    /// Service ID to decode (hex, e.g. 0xF2F8)
    #[arg(short = 's', long = "sid")]
    pub sid: Option<String>,

    /// Select service by label
    #[arg(short = 'l', long = "label")]
    pub label: Option<String>,

    /// List ensemble services then exit
    #[arg(long = "list-services")]
    pub list_services: bool,

    /// AAC decoder backend
    #[arg(long = "aac-decoder", default_value = "faad2")]
    pub aac_decoder: AacDecoder,

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

    /// Export WAV, slides and metadata for all DAB+ services to this directory
    #[arg(
        long = "all-services-out",
        conflicts_with_all = ["sid", "label", "list_services", "slide_dir"]
    )]
    pub all_services_out: Option<String>,

    /// Deduplicate consecutive identical PAD events (DL and slides)
    #[arg(long = "dedup-pad")]
    pub dedup_pad: bool,
}

/// AAC decoder backend selection
#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum AacDecoder {
    Faad2,
    #[cfg(feature = "fdk-aac")]
    Fdk,
}

/// Behavior on missing/invalid AAC frames
#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum AacGap {
    /// Preserve legacy behavior: no PCM output on error (default)
    Freeze,
    /// Emit PCM silence to keep stream alive
    Silence,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_parse_dablin_basic() {
        let cli =
            Cli::try_parse_from(["dabctl", "dablin", "-i", "test.eti", "-s", "0xF2F8"]).unwrap();
        match cli.command {
            Commands::Dablin(args) => {
                assert_eq!(args.input, "test.eti");
                assert_eq!(args.sid, Some("0xF2F8".to_string()));
                assert!(!args.silent);
                assert_eq!(args.aac_gap, AacGap::Freeze);
            }
        }
    }

    #[test]
    fn test_parse_dablin_silence_gap() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
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
            Commands::Dablin(args) => {
                assert_eq!(args.input, "-");
                assert_eq!(args.aac_gap, AacGap::Silence);
                assert!(args.silent);
            }
        }
    }

    #[test]
    fn test_parse_dablin_list_services() {
        let cli =
            Cli::try_parse_from(["dabctl", "dablin", "-i", "test.eti", "--list-services"]).unwrap();
        match cli.command {
            Commands::Dablin(args) => {
                assert!(args.list_services);
            }
        }
    }

    #[test]
    fn test_parse_dablin_by_label() {
        let cli = Cli::try_parse_from(["dabctl", "dablin", "-i", "test.eti", "-l", "France Inter"])
            .unwrap();
        match cli.command {
            Commands::Dablin(args) => {
                assert_eq!(args.label, Some("France Inter".to_string()));
            }
        }
    }

    #[test]
    fn test_parse_dablin_all_services_out() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "-i",
            "test.eti",
            "--all-services-out",
            "out",
        ])
        .unwrap();
        match cli.command {
            Commands::Dablin(args) => {
                assert_eq!(args.all_services_out, Some("out".to_string()));
            }
        }
    }

    #[test]
    fn test_parse_dablin_all_services_out_conflicts_with_sid() {
        let cli = Cli::try_parse_from([
            "dabctl",
            "dablin",
            "-i",
            "test.eti",
            "--all-services-out",
            "out",
            "-s",
            "0xF2F8",
        ]);
        assert!(cli.is_err());
    }
}
