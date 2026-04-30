//! Main decoding loop for the `dablin` subcommand
//!
//! Pipeline:
//!   ETI source (file / stdin)
//!     → ETI-NI frame parser
//!     → FIC/FIG decoder (ensemble, service discovery)
//!     → MSC sub-channel extractor
//!     → DAB+ super frame assembler
//!     → Reed-Solomon + FireCode
//!     → AAC decoder (faad2 or fdk-aac)
//!     → stdout (raw PCM s16le 48 kHz stereo)
//!     → FD 3  (JSONL metadata)

use std::io::{self, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use anyhow::{Context, Result};
use tracing::{debug, info, warn};
use base64::Engine;

use crate::cli::{AacDecoder as AacDecoderChoice, AacGap, DablinArgs};
use crate::dablin::audio::AacDecoder;
use crate::dablin::dabplus::process_superframe;
use crate::dablin::eti::{parse_frame, FsyncState, ETI_FRAME_SIZE};
use crate::dablin::fic::{FicDecoder, ServiceInfo};
use crate::dablin::metadata::MetadataEmitter;
use crate::dablin::msc::{extract_subchannel, SubchannelBuffer};
use crate::dablin::pad::PadDecoder;

/// Entry point for `dabctl dablin …`
pub fn run(args: DablinArgs) -> Result<()> {
    // ── Logging ──────────────────────────────────────────────────────────────
    if !args.silent {
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

    // ── Ctrl+C handler ───────────────────────────────────────────────────────
    let running = Arc::new(AtomicBool::new(true));
    {
        let r = Arc::clone(&running);
        ctrlc::set_handler(move || {
            r.store(false, Ordering::Relaxed);
        })
        .expect("Error setting Ctrl+C handler");
    }

    // ── Open input ───────────────────────────────────────────────────────────
    let reader: Box<dyn Read> = if args.input == "-" {
        Box::new(io::stdin())
    } else {
        let f = std::fs::File::open(&args.input)
            .with_context(|| format!("cannot open ETI file: {}", args.input))?;
        Box::new(f)
    };
    let mut reader = BufReader::with_capacity(ETI_FRAME_SIZE * 4, reader);

    // ── Open metadata channel (FD 3) ─────────────────────────────────────────
    // FD 3 may not be open in test environments, so we use an Option.
    let mut meta: Option<MetadataEmitter> = MetadataEmitter::open().ok();

    // ── Slide output configuration ────────────────────────────────────────────
    let slide_dir = args.slide_dir.as_deref().map(std::path::Path::new);
    if let Some(dir) = slide_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            warn!("Cannot create slide-dir {:?}: {}", dir, e);
        }
    }
    let _slide_base64 = args.slide_base64; // reserved for MOT PAD decoding

    // ── FIC decoder ──────────────────────────────────────────────────────────
    let mut fic = FicDecoder::new();
    let mut selected_scid: Option<u8> = None;
    let mut selected_sid: Option<u32> = None;
    let mut emitted_ensemble_eid: Option<u16> = None;
    let mut emitted_ensemble_label: Option<String> = None;
    let mut emitted_service_sid: Option<u32> = None;
    let mut emitted_service_label: Option<String> = None;

    // ── AAC decoder ──────────────────────────────────────────────────────────
    let mut aac: Option<AacDecoder> = None;

    // ── Sub-channel buffer ───────────────────────────────────────────────────
    let mut subch_buf: Option<SubchannelBuffer> = None;

    // ── PAD decoder ──────────────────────────────────────────────────────────
    let mut pad_decoder = PadDecoder::new();

    // ── FSYNC state ──────────────────────────────────────────────────────────
    let mut fsync_state = FsyncState::new();

    // ── PCM stdout ───────────────────────────────────────────────────────────
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // ── ETI frame read loop ──────────────────────────────────────────────────
    let mut frame_buf = vec![0u8; ETI_FRAME_SIZE];
    let mut frame_count = 0u64;

    loop {
        // Exit gracefully on Ctrl+C
        if !running.load(Ordering::Relaxed) {
            info!("Interrupted, exiting");
            break;
        }

        // Read one ETI-NI frame (6144 bytes)
        match reader.read_exact(&mut frame_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                info!("ETI stream ended after {} frames", frame_count);
                break;
            }
            Err(e) => return Err(e).context("ETI read error"),
        }

        // Parse frame header
        let frame = match parse_frame(&frame_buf) {
            Ok(f) => f,
            Err(e) => {
                warn!("ETI parse error frame {}: {}", frame_count, e);
                fsync_state.reset();
                frame_count += 1;
                continue;
            }
        };

        // FSYNC validation
        let fsync = [frame_buf[1], frame_buf[2], frame_buf[3]];
        if !fsync_state.check(fsync) {
            warn!("FSYNC mismatch at frame {}, re-syncing", frame_count);
            fsync_state.reset();
            fsync_state.check(fsync); // accept as new reference
        }

        frame_count += 1;

        // ── FIC processing ────────────────────────────────────────────────────
        if frame.ficf && !frame.fic.is_empty() {
            fic.process_fic(&frame.fic);
        }

        // ── Service selection (deferred until FIC is populated) ───────────────
        if selected_scid.is_none() && !fic.services.is_empty() {
            let service = select_service(&fic, &args);
            match service {
                Some(svc) => {
                    if let Some(comp) = svc.components.first() {
                        let scid = comp.subch_id;
                        selected_scid = Some(scid);
                        selected_sid = Some(svc.sid);

                        // Find STL for the selected sub-channel
                        if let Some(stc) = frame.stc.iter().find(|e| e.scid == scid) {
                            let buf = SubchannelBuffer::new(scid, stc.stl);
                            debug!("Sub-channel SCID={} STL={} ({} bytes/CIF)", scid, stc.stl, buf.cif_bytes());
                            debug!(
                                "PAD MOT app type for SCID {}: {:?}, SID {:#06x}: {:?}",
                                scid,
                                fic.mot_app_type(scid),
                                svc.sid,
                                fic.mot_app_type_for_sid(svc.sid)
                            );
                            subch_buf = Some(buf);

                            // Approximate service bitrate from sub-channel size:
                            // 1 CIF every 24 ms, 1 CU = 64 bits per CIF.
                            // kbps = floor(STL * 64 / 24)
                            if let Some(m) = meta.as_mut() {
                                let kbps = (u32::from(stc.stl) * 64) / 24;
                                m.emit_bitrate(kbps);
                            }
                        } else {
                            warn!("Sub-channel SCID={} not found in STC", scid);
                        }

                        // Initialize AAC decoder
                        aac = init_aac_decoder(&args.aac_decoder, &args.aac_gap);
                    }
                }
                None if args.list_services => {
                    // Already printed services, will exit below
                }
                None => {
                    // Not yet found, keep accumulating FIC
                }
            }

            // Handle --list-services
            if args.list_services && !fic.services.is_empty() {
                print_services(&fic);
                return Ok(());
            }
        }

        // ── Metadata refresh (labels can arrive after initial service selection) ──
        if let Some(sid) = selected_sid {
            let current_ensemble_label = fic.ensemble.label.clone();

            // Emit service only once the label is known
            if let Some(svc) = fic.services.iter().find(|s| s.sid == sid) {
                let current_service_label = svc.label.clone();
                if current_service_label.is_some()
                    && (emitted_service_sid != Some(sid) || emitted_service_label != current_service_label)
                {
                    if emitted_service_sid == Some(sid) && emitted_service_label.is_none() {
                        info!("Service label resolved: SID={:#06x} label={:?}", sid, current_service_label.as_deref());
                    }
                    if let Some(m) = meta.as_mut() {
                        m.emit_service(sid, current_service_label.as_deref());
                    }
                    emitted_service_sid = Some(sid);
                    emitted_service_label = current_service_label;
                } else if emitted_service_sid.is_none() {
                    // Track that we've seen this service even without label yet
                    emitted_service_sid = Some(sid);
                }
            }

            // Emit ensemble only once the label is known
            if current_ensemble_label.is_some()
                && (emitted_ensemble_eid != Some(fic.ensemble.eid)
                    || emitted_ensemble_label != current_ensemble_label)
            {
                if let Some(m) = meta.as_mut() {
                    m.emit_ensemble(fic.ensemble.eid, current_ensemble_label.as_deref());
                }
                emitted_ensemble_eid = Some(fic.ensemble.eid);
                emitted_ensemble_label = current_ensemble_label;
            }
        }

        // ── MSC extraction ───────────────────────────────────────────────────
        let scid = match selected_scid {
            Some(s) => s,
            None => continue,
        };

        let cif_data = match extract_subchannel(&frame, scid) {
            Some(d) => d,
            None => {
                debug!("Sub-channel {} absent from frame {}", scid, frame_count);
                continue;
            }
        };

        let buf = match subch_buf.as_mut() {
            Some(b) => b,
            None => continue,
        };

        buf.push_cif(cif_data);

        // ── DAB+ super frame decoding ─────────────────────────────────────────
        // Use sliding window like dablin: if FireCode fails, advance one CIF
        while buf.buffer_len() >= buf.superframe_size() {
            let sf_data = match buf.try_peek_superframe() {
                Some(d) => d,
                None => break,
            };
            let result = process_superframe(&sf_data);

            if !result.firecode_ok {
                debug!("DAB+ FireCode mismatch – advancing one CIF");
                buf.advance_one_cif();
                continue;
            }

            // Valid sync – consume the full superframe
            buf.try_pop_superframe();

            if result.rs_corrected > 0 {
                debug!("RS corrected {} codewords", result.rs_corrected);
            }

            // Initialize AAC decoder with format on first valid superframe
            if let (Some(fmt), Some(aac_dec)) = (result.format.as_ref(), aac.as_mut()) {
                aac_dec.init_format(fmt);
            }

            // Decode each AU
            for au in result.units {
                // Try extracting PAD events from untouched AU data first.
                if let Some(scid) = selected_scid {
                    let mot_app_type = selected_sid
                        .and_then(|sid| fic.mot_app_type_for_sid(sid))
                        .or_else(|| fic.mot_app_type(scid));
                    let pad_events = pad_decoder.process_au(&au.data, mot_app_type);
                    if let Some(dl) = pad_events.dynamic_label {
                        if let Some(m) = meta.as_mut() {
                            m.emit_dynamic_label(&dl);
                        }
                    }

                    if let Some(slide) = pad_events.slide {
                        if let Some(dir) = slide_dir {
                            let path = dir.join(&slide.content_name);
                            if let Err(e) = std::fs::write(&path, &slide.data) {
                                warn!("Cannot write slide file {:?}: {}", path, e);
                            }
                        }

                        if let Some(m) = meta.as_mut() {
                            let data_base64 = if args.slide_base64 {
                                base64::engine::general_purpose::STANDARD.encode(&slide.data)
                            } else {
                                String::new()
                            };
                            m.emit_slide(&slide.content_name, &slide.content_type, &data_base64);
                        }
                    }
                }

                let aac_dec = match aac.as_mut() {
                    Some(d) => d,
                    None => continue,
                };

                match aac_dec.decode(&au) {
                    Some(pcm) => {
                        // Write raw s16le PCM to stdout
                        let bytes: &[u8] = unsafe {
                            std::slice::from_raw_parts(
                                pcm.as_ptr() as *const u8,
                                pcm.len() * 2,
                            )
                        };
                        if let Err(e) = out.write_all(bytes) {
                            if e.kind() == io::ErrorKind::BrokenPipe {
                                info!("stdout closed, exiting");
                                return Ok(());
                            }
                            return Err(e).context("PCM write error");
                        }
                    }
                    None => {
                        // Freeze mode: no output on error
                        debug!("AAC gap: freeze (no PCM output)");
                    }
                }
            }
        }
    }

    Ok(())
}

/// Select a service based on CLI arguments.
fn select_service<'a>(fic: &'a FicDecoder, args: &DablinArgs) -> Option<&'a ServiceInfo> {
    if let Some(ref sid_str) = args.sid {
        return fic.find_by_sid(sid_str);
    }
    if let Some(ref label) = args.label {
        return fic.find_by_label(label);
    }
    // Default: first service with components
    fic.services.iter().find(|s| !s.components.is_empty())
}

/// Initialize the AAC decoder backend.
fn init_aac_decoder(backend: &AacDecoderChoice, gap: &AacGap) -> Option<AacDecoder> {
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

/// Print the list of discovered services to stderr (for --list-services).
fn print_services(fic: &FicDecoder) {
    eprintln!("Ensemble: EId={:#06x} label={:?}", fic.ensemble.eid, fic.ensemble.label);
    for svc in &fic.services {
        let subch_ids: Vec<u8> = svc.components.iter().map(|c| c.subch_id).collect();
        let dabplus_marks: Vec<&str> = subch_ids
            .iter()
            .map(|&id| if fic.is_dabplus(id) { "DAB+" } else { "DAB" })
            .collect();
        eprintln!(
            "  SID={:#06x}  label={:?}  sub-ch={:?}  type={:?}",
            svc.sid,
            svc.label.as_deref().unwrap_or("(no label)"),
            subch_ids,
            dabplus_marks,
        );
    }
}
