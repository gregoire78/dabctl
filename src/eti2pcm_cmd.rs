// eti2pcm subcommand - ETI → PCM audio decoder (like dablin)
// Reads ETI from stdin or file, decodes selected service to PCM on stdout,
// outputs DAB metadata as JSON on fd 3.

use std::io::{self, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Args;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use eti_rtlsdr_rust::eti2pcm::eti_frame::parse_eti_frame;
use eti_rtlsdr_rust::eti2pcm::eti_reader::EtiReader;
use eti_rtlsdr_rust::eti2pcm::fic_decoder::FicDecoder;
use eti_rtlsdr_rust::eti2pcm::pad_decoder::PadDecoder;
use eti_rtlsdr_rust::eti2pcm::pad_output::PadOutput;
use eti_rtlsdr_rust::eti2pcm::superframe::SuperframeFilter;

#[derive(Args, Debug)]
pub struct Eti2pcmArgs {
    /// Service ID to play (hex, e.g., 0xF201)
    #[arg(short = 's', long = "sid")]
    sid: Option<String>,

    /// Service label to play (matches by name)
    #[arg(short = 'l', long = "label")]
    label: Option<String>,

    /// Play first service found
    #[arg(short = '1', long = "first")]
    first: bool,

    /// PCM output on stdout (required for audio output)
    #[arg(short = 'p', long = "pcm")]
    pcm: bool,

    /// Disable dynamic FIC messages on stderr
    #[arg(short = 'F', long = "disable-dyn-fic")]
    disable_dyn_fic: bool,

    /// Save slideshow images to this directory
    #[arg(short = 'S', long = "slide-dir")]
    slide_dir: Option<String>,

    /// Output slideshow as base64 JSON on fd 3
    #[arg(long = "slide-base64")]
    slide_base64: bool,

    /// ETI input file (default: stdin)
    #[arg()]
    file: Option<String>,
}

pub fn run(args: Eti2pcmArgs) {
    // Logs go to stderr (never stdout, which carries PCM audio)
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(io::stderr)
        .init();

    let running = Arc::new(AtomicBool::new(true));
    let running_ctrlc = running.clone();
    ctrlc::set_handler(move || {
        running_ctrlc.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Parse SID
    let target_sid = args.sid.as_ref().map(|s| parse_sid(s));

    // Open input
    let input: Box<dyn Read> = match &args.file {
        Some(path) => {
            let file = std::fs::File::open(path).unwrap_or_else(|e| {
                error!("Cannot open {}: {}", path, e);
                std::process::exit(1);
            });
            Box::new(BufReader::new(file))
        }
        None => Box::new(BufReader::new(io::stdin())),
    };

    let mut eti_reader = EtiReader::new(input);
    let mut fic_decoder = FicDecoder::new();
    let mut pad_decoder = PadDecoder::new();
    // MOT Slideshow app type is typically 12 (X-PAD CI type for MOT start)
    pad_decoder.set_mot_app_type(12);
    let slide_dir = args.slide_dir.as_ref().map(PathBuf::from);
    let mut pad_output = PadOutput::new(slide_dir, args.slide_base64);
    let mut superframe = SuperframeFilter::new();
    let mut mp2_decoder: Option<eti_rtlsdr_rust::eti2pcm::mp2_decoder::Mp2Decoder> = None;
    let mut aac_decoder: Option<eti_rtlsdr_rust::eti2pcm::aac_decoder::AacDecoder> = None;

    let mut current_subchid: Option<u8> = None;
    let mut is_dab_plus: Option<bool> = None;
    let mut prev_fsync: u32 = 0;
    let mut ensemble_announced = false;
    let mut service_announced = false;

    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    info!("eti2pcm: waiting for ETI frames...");

    while running.load(Ordering::SeqCst) {
        let frame_data = match eti_reader.next_frame() {
            Ok(Some(f)) => f,
            Ok(None) => {
                info!("End of ETI stream");
                break;
            }
            Err(e) => {
                error!("ETI read error: {}", e);
                break;
            }
        };

        // Parse ETI frame
        let frame = match parse_eti_frame(&frame_data) {
            Some(f) => f,
            None => continue,
        };

        // Check alternating FSYNC
        if frame.header.fsync == prev_fsync {
            continue;
        }
        prev_fsync = frame.header.fsync;

        // Process FIC to discover services
        if !frame.fic_data.is_empty() {
            fic_decoder.process(frame.fic_data);
        }

        // Announce ensemble on fd 3
        if !ensemble_announced {
            if let Some(ref ens) = fic_decoder.ensemble {
                if !ens.label.is_empty() {
                    pad_output.write_ensemble(&ens.label, &ens.short_label, ens.eid);
                    info!("Ensemble: {} (0x{:04X})", ens.label.trim(), ens.eid);
                    ensemble_announced = true;
                }
            }
        }

        // Resolve which subchannel to play
        if current_subchid.is_none() {
            let audio_service = if let Some(sid) = target_sid {
                fic_decoder.find_audio_service(sid)
            } else if let Some(ref label) = args.label {
                fic_decoder
                    .find_service_by_label(label)
                    .and_then(|svc| fic_decoder.find_audio_service(svc.sid))
            } else if args.first {
                fic_decoder
                    .services
                    .values()
                    .next()
                    .and_then(|svc| fic_decoder.find_audio_service(svc.sid))
            } else {
                // No service selection → wait
                None
            };

            if let Some(audio) = audio_service {
                current_subchid = Some(audio.subchid);
                is_dab_plus = Some(audio.dab_plus);
                info!(
                    "Playing sub-channel {} ({})",
                    audio.subchid,
                    if audio.dab_plus { "DAB+" } else { "DAB" }
                );

                // Initialize decoder
                if !audio.dab_plus {
                    match eti_rtlsdr_rust::eti2pcm::mp2_decoder::Mp2Decoder::new() {
                        Ok(dec) => mp2_decoder = Some(dec),
                        Err(e) => {
                            error!("Failed to create MP2 decoder: {}", e);
                            std::process::exit(2);
                        }
                    }
                }
                // For DAB+, decoder is initialized after superframe format is known
            }
        }

        // Announce service on fd 3
        if !service_announced {
            if let (Some(sid), Some(_subchid)) = (target_sid, current_subchid) {
                if let Some(svc) = fic_decoder.services.get(&sid) {
                    if !svc.label.is_empty() {
                        pad_output.write_service(&svc.label, &svc.short_label, svc.sid);
                        info!("Service: {} (0x{:04X})", svc.label.trim(), svc.sid);
                        service_announced = true;
                    }
                }
            } else if let Some(ref label) = args.label {
                if current_subchid.is_some() {
                    if let Some(svc) = fic_decoder.find_service_by_label(label) {
                        pad_output.write_service(&svc.label, &svc.short_label, svc.sid);
                        service_announced = true;
                    }
                }
            } else if args.first && current_subchid.is_some() {
                if let Some(svc) = fic_decoder.services.values().next() {
                    pad_output.write_service(&svc.label, &svc.short_label, svc.sid);
                    service_announced = true;
                }
            }
        }

        // Extract subchannel data
        let subchid = match current_subchid {
            Some(id) => id,
            None => continue,
        };

        let subchannel_data = match frame.subchannel_data(subchid) {
            Some(data) => data,
            None => continue,
        };

        // Decode audio based on type
        if is_dab_plus == Some(true) {
            // DAB+ path: superframe → RS → AU → AAC → PCM
            let result = superframe.feed(subchannel_data);

            // Handle format change → init AAC decoder
            if let Some(ref fmt) = result.format {
                let asc = fmt.build_asc();
                info!(
                    "Format: {} {}kHz {}ch",
                    fmt.codec_name(),
                    fmt.sample_rate() / 1000,
                    fmt.channels()
                );
                match eti_rtlsdr_rust::eti2pcm::aac_decoder::AacDecoder::new(&asc) {
                    Ok(dec) => {
                        info!(
                            "AAC decoder: {}Hz {}ch",
                            dec.sample_rate, dec.channels
                        );
                        aac_decoder = Some(dec);
                    }
                    Err(e) => {
                        error!("AAC decoder init failed: {}", e);
                    }
                }
            }

            // Decode AUs
            if let Some(ref mut dec) = aac_decoder {
                for au in &result.access_units {
                    if let Some(pcm) = dec.decode_frame(&au.data) {
                        if args.pcm {
                            write_pcm(&mut stdout_lock, &pcm);
                        }
                    }
                }
            }

            // Process PAD for DLS and slideshow
            for pad in &result.pad_data {
                let pad_result = pad_decoder.process_full(
                    &pad.xpad,
                    pad.xpad.len(),
                    true,
                    &pad.fpad,
                );
                if let Some(ref label) = pad_result.dynamic_label {
                    pad_output.write_dl(&label.text);
                    if !args.disable_dyn_fic {
                        eprintln!("DLS: {}", label.text);
                    }
                }
                if let Some(ref slide) = pad_result.slide {
                    pad_output.write_slide(slide);
                    if !args.disable_dyn_fic {
                        eprintln!(
                            "Slide: {} ({}, {} bytes)",
                            slide.content_name, slide.mime_type(), slide.data.len()
                        );
                    }
                }
            }
        } else if is_dab_plus == Some(false) {
            // DAB (MP2) path
            if let Some(ref mut dec) = mp2_decoder {
                let results = dec.feed(subchannel_data);
                for pcm in results {
                    if args.pcm {
                        write_pcm(&mut stdout_lock, &pcm);
                    }
                }
            }
        }
    }
}

/// Write PCM i16 samples to stdout
fn write_pcm(out: &mut impl Write, samples: &[i16]) {
    // Write as raw little-endian bytes (standard PCM format for ffmpeg)
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(samples.as_ptr() as *const u8, samples.len() * 2)
    };
    let _ = out.write_all(bytes);
    let _ = out.flush();
}

/// Parse a SID string (supports "0xF201" and "F201" hex formats)
fn parse_sid(s: &str) -> u16 {
    let s = s.trim();
    let hex_str = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u16::from_str_radix(hex_str, 16).unwrap_or_else(|_| {
        error!("Invalid SID: {}", s);
        std::process::exit(1);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sid_hex() {
        assert_eq!(parse_sid("0xF201"), 0xF201);
        assert_eq!(parse_sid("0xf2f8"), 0xF2F8);
        assert_eq!(parse_sid("F201"), 0xF201);
    }
}
