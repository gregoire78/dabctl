//! Main decoding loop for the `dablin` subcommand
//!
//! Pipeline:
//!   ETI source (file / stdin)
//!     → ETI-NI frame parser
//!     → FIC/FIG decoder (ensemble, service discovery)
//!     → MSC sub-channel extractor
//!     → DAB+ super frame assembler
//!     → Reed-Solomon + FireCode
//!     → AAC decoder (faad2 or fdk-aac) OR AAC framing (ADTS/LATM)
//!     → stdout (raw PCM s16le 48 kHz stereo OR raw AAC ADTS/LATM)
//!     → FD 3  (JSONL metadata)

use anyhow::{Context, Result};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

mod all_services_helpers;
mod command_helpers;
pub(crate) mod frame_loop;
mod meta_helpers;
mod one_service_helpers;
mod output;
mod pad_helpers;
mod superframe_helpers;

use crate::cli::{
    AacDecoder as AacDecoderChoice, AacGap, AllServicesOutArgs, AudioOut, DablinCommand,
    ListServicesArgs, OneServiceOutArgs,
};
use crate::dablin::audio::adts::AdtsPacker;
use crate::dablin::audio::latm::LatmPacker;
use crate::dablin::audio::AacDecoder;
use crate::dablin::dabplus::SuperframeFormat;
use crate::dablin::eti::ETI_FRAME_SIZE;
use crate::dablin::fic::FicDecoder;
use crate::dablin::metadata::MetadataEmitter;
use crate::dablin::msc::{extract_subchannel, SubchannelBuffer};
use crate::dablin::pad::PadDecoder;
use crate::dablin::runner::all_services_helpers::{
    discover_all_services_contexts, maybe_emit_all_services_time,
    process_all_services_parallel_context, sync_all_services_labels_and_subchannel,
};
use crate::dablin::runner::command_helpers::{
    init_command_input, run_with_command_input, CommandReader, CommandRuntime,
};
use crate::dablin::runner::frame_loop::{EtiFrameReader, EtiStep, EtiStepStatus};
use crate::dablin::runner::one_service_helpers::{
    maybe_process_fic_and_time, maybe_select_service_and_init, process_one_service_superframes,
    sync_one_service_metadata, FicTimeRuntime, OneServiceInitRuntime, OneServiceMetadataState,
    OneServicePadState, OneServiceSelectionState, OneServiceSuperframeRuntime,
};
use crate::dablin::shared::{datetime_mode_from_option, protection_label};
use crate::dablin::utils::path::sanitize_for_path;
use crate::dablin::utils::wav_writer::WavWriter;

struct ServiceDumpContext {
    sid: u32,
    scid: u8,
    out_dir_rel: String,
    wav: WavWriter,
    meta: BufWriter<std::fs::File>,
    slide_dir: PathBuf,
    subch_buf: SubchannelBuffer,
    aac: Option<AacDecoder>,
    pad_decoder: PadDecoder,
    sf_work_buf: Vec<u8>,
    emitted_ensemble_label: Option<String>,
    emitted_ensemble_short_label: Option<String>,
    emitted_service_label: Option<String>,
    last_dl: Option<String>,
    last_slide_hash: Option<u64>,
    dedup_pad: bool,
    emitted_audio_format: Option<SuperframeFormat>,
    emitted_subchannel_protection: Option<String>,
    bitrate_kbps: u32,
}

const OUTPUT_SAMPLE_RATE_HZ: u32 = 48_000;
const LIST_SERVICES_STABLE_FIC_FRAMES: u32 = 500;

#[derive(Debug, Eq, PartialEq)]
struct ListServiceSnapshot {
    sid: u32,
    label: Option<String>,
    component_subch_ids: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct ListServicesSnapshot {
    ensemble_eid: u16,
    ensemble_label: Option<String>,
    services: Vec<ListServiceSnapshot>,
    dabplus_subch_ids: Vec<u8>,
    subchannel_protection: Vec<(u8, String)>,
}

impl ListServicesSnapshot {
    fn from_fic(fic: &FicDecoder) -> Self {
        let mut services = fic
            .services
            .iter()
            .map(|service| {
                let mut component_subch_ids: Vec<u8> = service
                    .components
                    .iter()
                    .map(|component| component.subch_id)
                    .collect();
                component_subch_ids.sort_unstable();
                ListServiceSnapshot {
                    sid: service.sid,
                    label: service.label.clone(),
                    component_subch_ids,
                }
            })
            .collect::<Vec<_>>();
        services.sort_by_key(|service| service.sid);

        let mut dabplus_subch_ids = fic.dabplus_subch_ids.clone();
        dabplus_subch_ids.sort_unstable();

        let mut subchannel_protection = fic
            .subchannels
            .iter()
            .map(|subchannel| {
                (
                    subchannel.subch_id,
                    protection_label(&subchannel.protection),
                )
            })
            .collect::<Vec<_>>();
        subchannel_protection.sort_by_key(|(subch_id, _)| *subch_id);

        Self {
            ensemble_eid: fic.ensemble.eid,
            ensemble_label: fic.ensemble.label.clone(),
            services,
            dabplus_subch_ids,
            subchannel_protection,
        }
    }
}

struct ListServicesStability {
    inventory: Option<ListServicesSnapshot>,
    unchanged_fic_frames: u32,
}

impl ListServicesStability {
    fn new() -> Self {
        Self {
            inventory: None,
            unchanged_fic_frames: 0,
        }
    }

    fn observe(&mut self, fic: &FicDecoder) -> bool {
        let inventory = ListServicesSnapshot::from_fic(fic);
        if inventory.services.is_empty() {
            self.inventory = None;
            self.unchanged_fic_frames = 0;
            return false;
        }

        if self.inventory.as_ref() == Some(&inventory) {
            self.unchanged_fic_frames = self.unchanged_fic_frames.saturating_add(1);
        } else {
            self.inventory = Some(inventory);
            self.unchanged_fic_frames = 0;
        }

        self.unchanged_fic_frames >= LIST_SERVICES_STABLE_FIC_FRAMES
    }
}

fn should_print_discovered_services(fic: &FicDecoder) -> bool {
    !fic.services.is_empty()
}

/// Initialize the tracing logger on stderr unless `silent` is set.
fn init_logger(silent: bool) {
    if !silent {
        use std::io::IsTerminal;
        let ansi = std::io::stderr().is_terminal();
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_ansi(ansi)
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::INFO.into()),
            )
            .init();
    }
}

/// Register a Ctrl+C handler and return the shared running flag.
#[cfg(not(target_arch = "wasm32"))]
fn setup_ctrlc() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    ctrlc::set_handler(move || {
        r.store(false, Ordering::Relaxed);
    })
    .expect("Error setting Ctrl+C handler");
    running
}

/// WebAssembly builds do not provide POSIX signals, so we keep a static running flag.
#[cfg(target_arch = "wasm32")]
fn setup_ctrlc() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(true))
}

/// Open an ETI input: `-` for stdin, otherwise a file path.
fn open_eti_reader(input: &str) -> Result<BufReader<Box<dyn Read>>> {
    let reader: Box<dyn Read> = if input == "-" {
        Box::new(io::stdin())
    } else {
        let f = std::fs::File::open(input)
            .with_context(|| format!("cannot open ETI file: {}", input))?;
        Box::new(f)
    };
    Ok(BufReader::with_capacity(ETI_FRAME_SIZE * 4, reader))
}

/// Entry point for `dabctl dablin …`
pub fn run(command: DablinCommand) -> Result<()> {
    enforce_latm_only_constraints(&command)?;
    match command {
        DablinCommand::OneServiceOut(args) => run_one_service_cmd(args),
        DablinCommand::AllServicesOut(args) => run_all_services_cmd(args),
        DablinCommand::ListServices(args) => run_list_services_cmd(args),
    }
}

fn enforce_latm_only_constraints(command: &DablinCommand) -> Result<()> {
    #[cfg(any(
        feature = "latm-only",
        feature = "wasm-runtime",
        target_arch = "wasm32"
    ))]
    {
        use anyhow::bail;

        match command {
            DablinCommand::OneServiceOut(args) => {
                if args.audio_out != AudioOut::Latm {
                    bail!("this build is latm-only: use --audio-out latm (or omit --audio-out)");
                }
            }
            DablinCommand::AllServicesOut(_) => {
                bail!("this build is latm-only: all-services-out is not available");
            }
            DablinCommand::ListServices(_) => {}
        }
    }

    #[cfg(not(any(
        feature = "latm-only",
        feature = "wasm-runtime",
        target_arch = "wasm32"
    )))]
    {
        let _ = command;
    }

    Ok(())
}

fn run_one_service_cmd(args: OneServiceOutArgs) -> Result<()> {
    let (running, mut reader) = init_command_input(args.silent, &args.input)?;
    run_one_service(args, &running, &mut reader)
}

fn run_one_service(
    args: OneServiceOutArgs,
    running: &CommandRuntime,
    reader: &mut CommandReader,
) -> Result<()> {
    if (args.audio_out == AudioOut::Adts || args.audio_out == AudioOut::Latm)
        && (args.aac_decoder != AacDecoderChoice::Faad2 || args.aac_gap != AacGap::Freeze)
    {
        warn!("--aac-decoder/--aac-gap are ignored when --audio-out is adts/latm");
    }

    let mut meta: Option<MetadataEmitter> = MetadataEmitter::open().ok();

    let slide_dir = args.slide_dir.as_deref().map(std::path::Path::new);
    if let Some(dir) = slide_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            warn!("Cannot create slide-dir {:?}: {}", dir, e);
        }
    }
    let mut fic = FicDecoder::new();
    let mut selection = OneServiceSelectionState {
        selected_scid: None,
        selected_sid: None,
        selected_bitrate_kbps: None,
    };
    let mut metadata_state = OneServiceMetadataState {
        emitted_ensemble_eid: None,
        emitted_ensemble_label: None,
        emitted_ensemble_short_label: None,
        emitted_service_sid: None,
        emitted_service_label: None,
        emitted_time: None,
        emitted_audio_format: None,
        emitted_subchannel_protection: None,
    };
    let datetime_mode = datetime_mode_from_option(args.datetime_format.as_ref());

    let mut aac: Option<AacDecoder> = None;
    let mut latm_packer = LatmPacker::new();
    let adts_packer = AdtsPacker::new();
    let mut subch_buf: Option<SubchannelBuffer> = None;
    let mut pad_decoder = PadDecoder::new();
    let mut frame_reader = EtiFrameReader::new();
    let mut pad_state = OneServicePadState {
        last_dl: None,
        last_slide_hash: None,
    };
    // FIC freeze: re-parse only on MNSC changes once labels are known.
    let mut fic_stable = false;
    let mut last_mnsc: u16 = 0xFFFF;
    let mut sf_work_buf: Vec<u8> = Vec::new();
    let mut pcm_write_scratch: Vec<u8> = Vec::new();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut frame_buf = vec![0u8; ETI_FRAME_SIZE];

    loop {
        if !running.load(Ordering::Relaxed) {
            info!("Interrupted, exiting");
            break;
        }

        let step: EtiStep = frame_reader.read_step(reader, &mut frame_buf)?;
        let frame = match step.status() {
            EtiStepStatus::Eof => {
                info!(
                    "ETI stream ended after {} frames",
                    frame_reader.frame_count()
                );
                break;
            }
            EtiStepStatus::BadFrame => continue,
            EtiStepStatus::Frame => step
                .into_frame()
                .expect("EtiStepStatus::Frame must carry parsed frame"),
        };

        maybe_process_fic_and_time(
            &frame,
            &mut fic,
            &selection,
            datetime_mode,
            &mut FicTimeRuntime {
                fic_stable: &mut fic_stable,
                last_mnsc: &mut last_mnsc,
                metadata_state: &mut metadata_state,
                meta: &mut meta,
            },
        );

        maybe_select_service_and_init(
            &args,
            &frame,
            &fic,
            &mut OneServiceInitRuntime {
                selection: &mut selection,
                metadata_state: &mut metadata_state,
                subch_buf: &mut subch_buf,
                aac: &mut aac,
                meta: &mut meta,
            },
        );

        sync_one_service_metadata(&fic, &selection, &mut metadata_state, &mut meta);

        let scid = match selection.selected_scid {
            Some(s) => s,
            None => continue,
        };

        let cif_data = match extract_subchannel(&frame, scid) {
            Some(d) => d,
            None => {
                debug!(
                    "Sub-channel {} absent from frame {}",
                    scid,
                    frame_reader.frame_count()
                );
                continue;
            }
        };

        let buf = match subch_buf.as_mut() {
            Some(b) => b,
            None => continue,
        };

        buf.push_cif(cif_data);

        if process_one_service_superframes(&mut OneServiceSuperframeRuntime {
            args: &args,
            fic: &fic,
            selection: &selection,
            buf,
            sf_work_buf: &mut sf_work_buf,
            aac: &mut aac,
            metadata_state: &mut metadata_state,
            meta: &mut meta,
            pad_decoder: &mut pad_decoder,
            slide_dir,
            pad_state: &mut pad_state,
            latm_packer: &mut latm_packer,
            adts_packer: &adts_packer,
            pcm_write_scratch: &mut pcm_write_scratch,
            out: &mut out,
        })? {
            return Ok(());
        }
    }

    Ok(())
}

fn run_all_services_cmd(args: AllServicesOutArgs) -> Result<()> {
    let (running, mut reader) = init_command_input(args.silent, &args.input)?;

    run_all_services(&args, &mut reader, &running, Path::new(&args.out_dir))
}

fn run_list_services_cmd(args: ListServicesArgs) -> Result<()> {
    run_with_command_input(args.silent, &args.input, |running, reader| {
        run_list_services(reader, running)
    })
}

fn run_list_services(reader: &mut CommandReader, running: &CommandRuntime) -> Result<()> {
    let mut fic = FicDecoder::new();
    let mut frame_reader = EtiFrameReader::new();
    let mut frame_buf = vec![0u8; ETI_FRAME_SIZE];
    let mut stability = ListServicesStability::new();

    loop {
        if !running.load(Ordering::Relaxed) {
            info!("Interrupted, exiting");
            break;
        }

        let step: EtiStep = frame_reader.read_step(reader, &mut frame_buf)?;
        let frame = match step.status() {
            EtiStepStatus::Eof => {
                info!(
                    "ETI stream ended after {} frames",
                    frame_reader.frame_count()
                );
                break;
            }
            EtiStepStatus::BadFrame => continue,
            EtiStepStatus::Frame => step
                .into_frame()
                .expect("EtiStepStatus::Frame must carry parsed frame"),
        };

        if frame.ficf && !frame.fic.is_empty() {
            fic.process_fic(frame.fic);
            if stability.observe(&fic) {
                print_services(&fic);
                return Ok(());
            }
        }
    }

    if should_print_discovered_services(&fic) {
        print_services(&fic);
    }

    Ok(())
}

fn run_all_services(
    args: &AllServicesOutArgs,
    reader: &mut CommandReader,
    running: &CommandRuntime,
    out_root: &Path,
) -> Result<()> {
    std::fs::create_dir_all(out_root)
        .with_context(|| format!("cannot create output directory: {}", out_root.display()))?;

    let mut fic = FicDecoder::new();
    let mut frame_reader = EtiFrameReader::new();
    let mut last_mnsc: u16 = 0xFFFF;
    let mut frame_buf = vec![0u8; ETI_FRAME_SIZE];
    let mut contexts: BTreeMap<u32, ServiceDumpContext> = BTreeMap::new();
    let mut emitted_time: Option<(String, String, String)> = None;
    let datetime_mode = datetime_mode_from_option(args.datetime_format.as_ref());

    loop {
        if !running.load(Ordering::Relaxed) {
            info!("Interrupted, finalizing all service files");
            break;
        }

        let step: EtiStep = frame_reader.read_step(reader, &mut frame_buf)?;
        let frame = match step.status() {
            EtiStepStatus::Eof => {
                info!(
                    "ETI stream ended after {} frames",
                    frame_reader.frame_count()
                );
                break;
            }
            EtiStepStatus::BadFrame => continue,
            EtiStepStatus::Frame => step
                .into_frame()
                .expect("EtiStepStatus::Frame must carry parsed frame"),
        };

        if frame.ficf && !frame.fic.is_empty() {
            let mnsc_changed = frame.mnsc != last_mnsc;
            last_mnsc = frame.mnsc;
            if mnsc_changed {
                debug!("MNSC changed ({:#06x}), re-parsing FIC", frame.mnsc);
            }
            fic.process_fic(frame.fic);
        }

        discover_all_services_contexts(args, &frame, &fic, &mut contexts, out_root, datetime_mode)?;

        // Phase 1 (serial): label/protection/time sync.
        sync_all_services_labels_and_subchannel(&mut contexts, &fic, out_root);
        maybe_emit_all_services_time(&mut contexts, &fic, datetime_mode, &mut emitted_time);

        let slide_base64 = args.slide_base64;

        // Phase 2 (parallel): superframe → AAC → WAV/JSONL, one thread per service.
        let ctxs: Vec<&mut ServiceDumpContext> = contexts.values_mut().collect();
        ctxs.into_par_iter().try_for_each(|ctx| {
            process_all_services_parallel_context(ctx, &frame, &fic, slide_base64)
        })?;
    }

    for ctx in contexts.values_mut() {
        ctx.wav.finalize()?;
        ctx.meta.flush()?;
    }

    Ok(())
}

fn service_dir_name(sid: u32, label: Option<&str>) -> String {
    let sid_hex = format!("{:#06x}", sid);
    let safe_label = sanitize_for_path(label.unwrap_or("no-label"));
    format!("{}-{}", sid_hex, safe_label)
}

fn init_aac_decoder(backend: &AacDecoderChoice, gap: &AacGap) -> Option<AacDecoder> {
    #[cfg(feature = "latm-only")]
    {
        let _ = backend;
        let _ = gap;
        None
    }

    #[cfg(not(feature = "latm-only"))]
    {
        match backend {
            AacDecoderChoice::Faad2 => {
                let dec = AacDecoder::new_faad2(gap.clone());
                if dec.is_none() {
                    warn!("faad2 decoder initialization failed");
                }
                dec
            }
            #[cfg(feature = "fdk-aac")]
            AacDecoderChoice::Fdk => {
                let dec = AacDecoder::new_fdk(gap.clone());
                if dec.is_none() {
                    warn!("fdk-aac decoder initialization failed");
                }
                dec
            }
        }
    }
}

/// Print the list of discovered services to stderr (for `dablin list-services`).
fn print_services(fic: &FicDecoder) {
    eprintln!(
        "Ensemble: EId={:#06x} label={}",
        fic.ensemble.eid,
        fic.ensemble.label.as_deref().unwrap_or("(no label)")
    );
    for svc in &fic.services {
        let subch_details: Vec<String> = svc
            .components
            .iter()
            .map(|c| {
                let family = if fic.is_dabplus(c.subch_id) {
                    "DAB+"
                } else {
                    "DAB"
                };
                let protection = fic
                    .subchannel_org(c.subch_id)
                    .map(|s| protection_label(&s.protection))
                    .unwrap_or_else(|| "unknown-protection".to_string());
                format!("SCID={} {} {}", c.subch_id, family, protection)
            })
            .collect();
        eprintln!(
            "  SID={:#06x}  label={:?}  components={:?}",
            svc.sid,
            svc.label.as_deref().unwrap_or("(no label)"),
            subch_details,
        );
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "latm-only")]
    use super::enforce_latm_only_constraints;
    use super::meta_helpers::{save_slide_file, should_emit_slide_metadata};
    use super::{service_dir_name, should_print_discovered_services, ListServicesStability};
    #[cfg(feature = "latm-only")]
    use crate::cli::{
        AacDecoder, AacGap, AllServicesOutArgs, AudioOut, DablinCommand, ListServicesArgs,
        OneServiceOutArgs,
    };
    use crate::dablin::fic::{FicDecoder, ServiceInfo};
    use crate::dablin::shared::{
        audio_codec_label, audio_mode_label, current_subchannel_protection, encode_slide_base64,
        hash_bytes, protection_label,
    };
    use std::path::Path;

    #[test]
    fn list_services_waits_for_a_stable_inventory() {
        let mut fic = FicDecoder::new();
        fic.services.push(ServiceInfo {
            sid: 0xf201,
            label: Some("FRANCE INTER".to_string()),
            components: Vec::new(),
        });

        let mut stability = ListServicesStability::new();
        assert!(!stability.observe(&fic));

        for _ in 0..499 {
            assert!(!stability.observe(&fic));
        }
        assert!(stability.observe(&fic));
    }

    #[test]
    fn list_services_resets_stability_when_inventory_grows() {
        let mut fic = FicDecoder::new();
        fic.services.push(ServiceInfo {
            sid: 0xf201,
            label: Some("FRANCE INTER".to_string()),
            components: Vec::new(),
        });

        let mut stability = ListServicesStability::new();
        assert!(!stability.observe(&fic));
        for _ in 0..100 {
            assert!(!stability.observe(&fic));
        }

        fic.services.push(ServiceInfo {
            sid: 0xf202,
            label: Some("FRANCE CULTURE".to_string()),
            components: Vec::new(),
        });
        assert!(!stability.observe(&fic));

        for _ in 0..499 {
            assert!(!stability.observe(&fic));
        }
        assert!(stability.observe(&fic));
    }

    #[test]
    fn list_services_prints_partial_inventory_at_end_of_input() {
        let mut fic = FicDecoder::new();
        assert!(!should_print_discovered_services(&fic));

        fic.services.push(ServiceInfo {
            sid: 0xf201,
            label: None,
            components: Vec::new(),
        });
        assert!(should_print_discovered_services(&fic));
    }

    #[test]
    fn service_dir_name_with_label() {
        let name = service_dir_name(0xf2f8, Some("NRJ"));
        assert_eq!(name, "0xf2f8-NRJ");
    }

    #[test]
    fn service_dir_name_without_label() {
        let name = service_dir_name(0xf2f8, None);
        assert_eq!(name, "0xf2f8-no-label");
    }

    #[test]
    fn service_dir_name_sanitizes_spaces() {
        let name = service_dir_name(0xf211, Some("RTL DAB"));
        assert_eq!(name, "0xf211-RTL_DAB");
    }

    #[test]
    fn service_dir_name_sanitizes_special_chars() {
        let name = service_dir_name(0xf221, Some("RADIO/CLASSIQUE"));
        assert_eq!(name, "0xf221-RADIOCLASSIQUE");
    }

    #[test]
    fn protection_label_formats_eep_a() {
        let label = protection_label(&crate::dablin::fic::ProtectionProfile::EepA(3));
        assert_eq!(label, "EEP-3A");
    }

    #[test]
    fn current_subchannel_protection_none_when_unknown() {
        let fic = crate::dablin::fic::FicDecoder::new();
        assert_eq!(current_subchannel_protection(&fic, 3), None);
    }

    #[test]
    fn audio_codec_label_detects_he_aac_and_v2() {
        let v1 = crate::dablin::dabplus::SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        let v2 = crate::dablin::dabplus::SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: false,
            ps_flag: true,
            mpeg_surround_config: 0,
        };
        assert_eq!(audio_codec_label(&v1), "HE-AAC");
        assert_eq!(audio_codec_label(&v2), "HE-AAC v2");
        assert_eq!(audio_mode_label(&v1), "stereo");
        assert_eq!(audio_mode_label(&v2), "stereo");
    }

    #[test]
    fn hash_bytes_same_input_gives_same_hash() {
        let a = hash_bytes(b"hello");
        let b = hash_bytes(b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_bytes_different_input_gives_different_hash() {
        let a = hash_bytes(b"slide1");
        let b = hash_bytes(b"slide2");
        assert_ne!(a, b);
    }

    #[test]
    fn encode_slide_base64_disabled_returns_empty() {
        let result = encode_slide_base64(b"some data", false);
        assert_eq!(result, "");
    }

    #[test]
    fn encode_slide_base64_enabled_returns_base64() {
        let result = encode_slide_base64(b"hello", true);
        assert_eq!(result, "aGVsbG8=");
    }

    #[test]
    fn should_not_emit_slide_metadata_without_dir_or_base64() {
        assert!(!should_emit_slide_metadata(None, false));
    }

    #[test]
    fn should_emit_slide_metadata_with_dir() {
        assert!(should_emit_slide_metadata(Some(Path::new("slides")), false));
    }

    #[test]
    fn should_emit_slide_metadata_with_base64() {
        assert!(should_emit_slide_metadata(None, true));
    }

    #[test]
    fn save_slide_file_writes_data() {
        let dir = std::env::temp_dir();
        let name = format!("dabctl-test-slide-{}.bin", std::process::id());
        save_slide_file(&dir, &name, b"slide payload");
        let path = dir.join(&name);
        let data = std::fs::read(&path).expect("slide file should exist");
        assert_eq!(data, b"slide payload");
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "latm-only")]
    #[test]
    fn latm_only_rejects_one_service_out_non_latm_audio() {
        let cmd = DablinCommand::OneServiceOut(OneServiceOutArgs {
            input: "test.eti".to_string(),
            sid: Some("0xF2F8".to_string()),
            label: None,
            aac_decoder: AacDecoder::Faad2,
            audio_out: AudioOut::Pcm,
            aac_gap: AacGap::Freeze,
            silent: true,
            slide_dir: None,
            slide_base64: false,
            dedup_pad: false,
            datetime_format: None,
        });

        assert!(enforce_latm_only_constraints(&cmd).is_err());
    }

    #[cfg(feature = "latm-only")]
    #[test]
    fn latm_only_rejects_all_services_out() {
        let cmd = DablinCommand::AllServicesOut(AllServicesOutArgs {
            input: "test.eti".to_string(),
            out_dir: "out".to_string(),
            aac_decoder: AacDecoder::Faad2,
            aac_gap: AacGap::Freeze,
            silent: true,
            slide_base64: false,
            dedup_pad: false,
            datetime_format: None,
        });

        assert!(enforce_latm_only_constraints(&cmd).is_err());
    }

    #[cfg(feature = "latm-only")]
    #[test]
    fn latm_only_accepts_list_services() {
        let cmd = DablinCommand::ListServices(ListServicesArgs {
            input: "test.eti".to_string(),
            silent: true,
        });

        assert!(enforce_latm_only_constraints(&cmd).is_ok());
    }
}

// Note: Integration tests for the full decoding pipeline (ETI → PCM)
// would require complex ETI frame construction and are better handled
// as separate integration tests with real ETI files.
//
// The `rs_over_threshold` flag is tested indirectly through the
// `SuperframeResult` construction in `dabplus::tests`, which ensures
// the flag is properly initialized and propagated.
