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

use anyhow::{Context, Result};
use base64::Engine;
use rayon::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::cli::{AacDecoder as AacDecoderChoice, AacGap, DablinArgs};
use crate::dablin::audio::AacDecoder;
use crate::dablin::dabplus::process_superframe_inplace;
use crate::dablin::eti::{parse_frame, FsyncState, ETI_FRAME_SIZE};
use crate::dablin::fic::{FicDecoder, ServiceInfo};
use crate::dablin::metadata::MetadataEmitter;
use crate::dablin::msc::{extract_subchannel, SubchannelBuffer};
use crate::dablin::pad::PadDecoder;
use crate::dablin::utils::jsonl::write_jsonl;
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
    emitted_service_label: Option<String>,
    last_dl: Option<String>,
    last_slide_hash: Option<u64>,
    dedup_pad: bool,
}

/// Entry point for `dabctl dablin …`
pub fn run(args: DablinArgs) -> Result<()> {
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

    let running = Arc::new(AtomicBool::new(true));
    {
        let r = Arc::clone(&running);
        ctrlc::set_handler(move || {
            r.store(false, Ordering::Relaxed);
        })
        .expect("Error setting Ctrl+C handler");
    }

    let reader: Box<dyn Read> = if args.input == "-" {
        Box::new(io::stdin())
    } else {
        let f = std::fs::File::open(&args.input)
            .with_context(|| format!("cannot open ETI file: {}", args.input))?;
        Box::new(f)
    };
    let mut reader = BufReader::with_capacity(ETI_FRAME_SIZE * 4, reader);

    if let Some(out_dir) = args.all_services_out.as_deref() {
        return run_all_services(&args, &mut reader, &running, Path::new(out_dir));
    }

    let mut meta: Option<MetadataEmitter> = MetadataEmitter::open().ok();

    let slide_dir = args.slide_dir.as_deref().map(std::path::Path::new);
    if let Some(dir) = slide_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            warn!("Cannot create slide-dir {:?}: {}", dir, e);
        }
    }
    let mut fic = FicDecoder::new();
    let mut selected_scid: Option<u8> = None;
    let mut selected_sid: Option<u32> = None;
    let mut emitted_ensemble_eid: Option<u16> = None;
    let mut emitted_ensemble_label: Option<String> = None;
    let mut emitted_service_sid: Option<u32> = None;
    let mut emitted_service_label: Option<String> = None;

    let mut list_services_frames: u32 = 0;
    let mut aac: Option<AacDecoder> = None;
    let mut subch_buf: Option<SubchannelBuffer> = None;
    let mut pad_decoder = PadDecoder::new();
    let mut fsync_state = FsyncState::new();
    let mut last_dl: Option<String> = None;
    let mut last_slide_hash: Option<u64> = None;
    // FIC freeze: re-parse only on MNSC changes once labels are known.
    let mut fic_stable = false;
    let mut last_mnsc: u16 = 0xFFFF;
    let mut sf_work_buf: Vec<u8> = Vec::new();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut frame_buf = vec![0u8; ETI_FRAME_SIZE];
    let mut frame_count = 0u64;

    loop {
        if !running.load(Ordering::Relaxed) {
            info!("Interrupted, exiting");
            break;
        }

        match reader.read_exact(&mut frame_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                info!("ETI stream ended after {} frames", frame_count);
                break;
            }
            Err(e) => return Err(e).context("ETI read error"),
        }

        let frame = match parse_frame(&frame_buf) {
            Ok(f) => f,
            Err(e) => {
                warn!("ETI parse error frame {}: {}", frame_count, e);
                fsync_state.reset();
                frame_count += 1;
                continue;
            }
        };

        let fsync = [frame_buf[1], frame_buf[2], frame_buf[3]];
        if !fsync_state.check(fsync) {
            warn!("FSYNC mismatch at frame {}, re-syncing", frame_count);
            fsync_state.reset();
            fsync_state.check(fsync);
        }

        frame_count += 1;

        if frame.ficf && !frame.fic.is_empty() {
            let mnsc_changed = frame.mnsc != last_mnsc;
            last_mnsc = frame.mnsc;

            if !fic_stable || mnsc_changed {
                if mnsc_changed && fic_stable {
                    info!("MNSC changed ({:#06x}), re-parsing FIC", frame.mnsc);
                }
                fic.process_fic(frame.fic);

                if !fic_stable {
                    let svc_stable = selected_sid
                        .and_then(|sid| fic.services.iter().find(|s| s.sid == sid))
                        .map(|s| s.label.is_some())
                        .unwrap_or(false);
                    if fic.ensemble.label.is_some() && svc_stable {
                        fic_stable = true;
                        debug!("FIC stable — entering MNSC-watch-only mode");
                    }
                }
            }
        }

        if args.list_services {
            if !fic.services.is_empty() {
                let all_known =
                    fic.ensemble.label.is_some() && fic.services.iter().all(|s| s.label.is_some());
                if all_known || list_services_frames >= 500 {
                    print_services(&fic);
                    return Ok(());
                }
                list_services_frames += 1;
            }
            continue;
        }

        if selected_scid.is_none() && !fic.services.is_empty() {
            let service = select_service(&fic, &args);
            if let Some(svc) = service {
                if let Some(comp) = svc.components.first() {
                    let scid = comp.subch_id;
                    selected_scid = Some(scid);
                    selected_sid = Some(svc.sid);

                    if let Some(stc) = frame.stc.iter().find(|e| e.scid == scid) {
                        let buf = SubchannelBuffer::new(scid, stc.stl);
                        debug!(
                            "Sub-channel SCID={} STL={} ({} bytes/CIF)",
                            scid,
                            stc.stl,
                            buf.cif_bytes()
                        );
                        debug!(
                            "PAD MOT app type for SCID {}: {:?}, SID {:#06x}: {:?}",
                            scid,
                            fic.mot_app_type(scid),
                            svc.sid,
                            fic.mot_app_type_for_sid(svc.sid)
                        );
                        subch_buf = Some(buf);

                        if let Some(m) = meta.as_mut() {
                            let kbps = (u32::from(stc.stl) * 64) / 24;
                            m.emit_bitrate(kbps);
                        }
                    } else {
                        warn!("Sub-channel SCID={} not found in STC", scid);
                    }

                    aac = init_aac_decoder(&args.aac_decoder, &args.aac_gap);
                }
            }
        }

        if let Some(sid) = selected_sid {
            let current_ensemble_label = fic.ensemble.label.clone();

            if let Some(svc) = fic.services.iter().find(|s| s.sid == sid) {
                let current_service_label = svc.label.clone();
                if current_service_label.is_some()
                    && (emitted_service_sid != Some(sid)
                        || emitted_service_label != current_service_label)
                {
                    if emitted_service_sid == Some(sid) && emitted_service_label.is_none() {
                        info!(
                            "Service label resolved: SID={:#06x} label={:?}",
                            sid,
                            current_service_label.as_deref()
                        );
                    }
                    if let Some(m) = meta.as_mut() {
                        m.emit_service(sid, current_service_label.as_deref());
                    }
                    emitted_service_sid = Some(sid);
                    emitted_service_label = current_service_label;
                } else if emitted_service_sid.is_none() {
                    emitted_service_sid = Some(sid);
                }
            }

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

        while buf.len() >= buf.superframe_size() {
            let sf_size = buf.superframe_size();

            let slice = match buf.try_peek_superframe_slice() {
                Some(d) => d,
                None => break,
            };

            if sf_work_buf.len() != sf_size {
                sf_work_buf.resize(sf_size, 0);
            }
            sf_work_buf.copy_from_slice(slice);

            let result = process_superframe_inplace(&mut sf_work_buf);

            if !result.firecode_ok {
                debug!("DAB+ FireCode mismatch – advancing one CIF");
                buf.advance_one_cif();
                continue;
            }

            buf.consume_superframe();

            if result.rs_corrected > 0 {
                debug!("RS corrected {} codewords", result.rs_corrected);
            }

            if let (Some(fmt), Some(aac_dec)) = (result.format.as_ref(), aac.as_mut()) {
                aac_dec.init_format(fmt);
            }

            for au in result.units {
                if let Some(scid) = selected_scid {
                    let mot_app_type = selected_sid
                        .and_then(|sid| fic.mot_app_type_for_sid(sid))
                        .or_else(|| fic.mot_app_type(scid));
                    let pad_events = pad_decoder.process_au(&au.data, mot_app_type);
                    if let Some(dl) = pad_events.dynamic_label {
                        let is_dup = args.dedup_pad && last_dl.as_deref() == Some(dl.as_str());
                        if !is_dup {
                            if let Some(m) = meta.as_mut() {
                                m.emit_dynamic_label(&dl);
                            }
                            last_dl = Some(dl);
                        }
                    }

                    if let Some(slide) = pad_events.slide {
                        use std::collections::hash_map::DefaultHasher;
                        use std::hash::{Hash, Hasher};
                        let mut hasher = DefaultHasher::new();
                        slide.data.hash(&mut hasher);
                        let slide_hash = hasher.finish();
                        let is_dup_slide = args.dedup_pad && last_slide_hash == Some(slide_hash);

                        if !is_dup_slide {
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
                                m.emit_slide(
                                    &slide.content_name,
                                    &slide.content_type,
                                    &data_base64,
                                );
                            }
                            last_slide_hash = Some(slide_hash);
                        }
                    }
                }

                let aac_dec = match aac.as_mut() {
                    Some(d) => d,
                    None => continue,
                };

                match aac_dec.decode(&au) {
                    Some(pcm) => {
                        let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
                        if let Err(e) = out.write_all(&bytes) {
                            if e.kind() == io::ErrorKind::BrokenPipe {
                                info!("stdout closed, exiting");
                                return Ok(());
                            }
                            return Err(e).context("PCM write error");
                        }
                    }
                    None => {
                        debug!("AAC gap: freeze");
                    }
                }
            }
        }
    }

    Ok(())
}

fn run_all_services(
    args: &DablinArgs,
    reader: &mut BufReader<Box<dyn Read>>,
    running: &Arc<AtomicBool>,
    out_root: &Path,
) -> Result<()> {
    std::fs::create_dir_all(out_root)
        .with_context(|| format!("cannot create output directory: {}", out_root.display()))?;

    let global_index_file = std::fs::File::create(out_root.join("global-index.jsonl"))
        .with_context(|| {
            format!(
                "cannot create global index file: {}",
                out_root.join("global-index.jsonl").display()
            )
        })?;
    let mut global_index = BufWriter::new(global_index_file);
    write_jsonl(
        &mut global_index,
        json!({
            "run": {
                "input": args.input.as_str(),
                "aacDecoder": match &args.aac_decoder {
                    AacDecoderChoice::Faad2 => "faad2",
                    #[cfg(feature = "fdk-aac")]
                    AacDecoderChoice::Fdk => "fdk",
                },
                "aacGap": match &args.aac_gap {
                    AacGap::Freeze => "freeze",
                    AacGap::Silence => "silence",
                },
                "slideBase64": args.slide_base64,
            }
        }),
    );

    let mut fic = FicDecoder::new();
    let mut fsync_state = FsyncState::new();
    let mut last_mnsc: u16 = 0xFFFF;
    let mut frame_buf = vec![0u8; ETI_FRAME_SIZE];
    let mut frame_count = 0u64;
    let mut contexts: BTreeMap<u32, ServiceDumpContext> = BTreeMap::new();
    let mut global_emitted_ensemble_label: Option<String> = None;

    write_jsonl(
        &mut global_index,
        json!({"ensemble": {"eid": format!("{:#06x}", fic.ensemble.eid)}}),
    );

    loop {
        if !running.load(Ordering::Relaxed) {
            info!("Interrupted, finalizing all service files");
            break;
        }

        match reader.read_exact(&mut frame_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                info!("ETI stream ended after {} frames", frame_count);
                break;
            }
            Err(e) => return Err(e).context("ETI read error"),
        }

        let frame = match parse_frame(&frame_buf) {
            Ok(f) => f,
            Err(e) => {
                warn!("ETI parse error frame {}: {}", frame_count, e);
                fsync_state.reset();
                frame_count += 1;
                continue;
            }
        };

        let fsync = [frame_buf[1], frame_buf[2], frame_buf[3]];
        if !fsync_state.check(fsync) {
            warn!("FSYNC mismatch at frame {}, re-syncing", frame_count);
            fsync_state.reset();
            fsync_state.check(fsync);
        }

        frame_count += 1;

        if frame.ficf && !frame.fic.is_empty() {
            let mnsc_changed = frame.mnsc != last_mnsc;
            last_mnsc = frame.mnsc;
            if mnsc_changed {
                debug!("MNSC changed ({:#06x}), re-parsing FIC", frame.mnsc);
            }
            fic.process_fic(frame.fic);

            if let Some(current_ensemble_label) = fic.ensemble.label.clone() {
                if global_emitted_ensemble_label.as_deref() != Some(current_ensemble_label.as_str())
                {
                    write_jsonl(
                        &mut global_index,
                        json!({"ensemble": {"eid": format!("{:#06x}", fic.ensemble.eid), "label": current_ensemble_label}}),
                    );
                    global_emitted_ensemble_label = Some(current_ensemble_label);
                }
            }
        }

        for svc in &fic.services {
            if svc.components.is_empty() {
                continue;
            }
            let scid = svc.components[0].subch_id;
            if !fic.is_dabplus(scid) {
                continue;
            }
            if contexts.contains_key(&svc.sid) {
                continue;
            }

            let stc = match frame.stc.iter().find(|e| e.scid == scid) {
                Some(stc) => stc,
                None => continue,
            };

            let sid_hex = format!("{:#06x}", svc.sid);
            let service_dir_name = service_dir_name(svc.sid, svc.label.as_deref());
            let service_dir = out_root.join(service_dir_name);
            let out_dir_rel = service_dir
                .strip_prefix(out_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| service_dir.to_string_lossy().to_string());
            let slides_dir = service_dir.join("slides");
            std::fs::create_dir_all(&slides_dir).with_context(|| {
                format!("cannot create slides directory: {}", slides_dir.display())
            })?;

            let wav = WavWriter::create(&service_dir.join("audio.wav"))?;
            let meta_file = std::fs::File::create(service_dir.join("metadata.jsonl"))
                .with_context(|| format!("cannot create metadata file for SID {:#06x}", svc.sid))?;
            let mut meta = BufWriter::new(meta_file);

            let ensemble_label = fic.ensemble.label.clone();
            if let Some(l) = ensemble_label.as_deref() {
                write_jsonl(
                    &mut meta,
                    json!({"ensemble": {"eid": format!("{:#06x}", fic.ensemble.eid), "label": l}}),
                );
            } else {
                write_jsonl(
                    &mut meta,
                    json!({"ensemble": {"eid": format!("{:#06x}", fic.ensemble.eid)}}),
                );
            }
            if let Some(l) = svc.label.as_deref() {
                write_jsonl(&mut meta, json!({"service": {"sid": sid_hex, "label": l}}));
            } else {
                write_jsonl(&mut meta, json!({"service": {"sid": sid_hex}}));
            }
            let kbps = (u32::from(stc.stl) * 64) / 24;
            write_jsonl(&mut meta, json!({"bitrate": kbps}));
            if let Some(l) = svc.label.as_deref() {
                write_jsonl(
                    &mut global_index,
                    json!({
                        "service": {
                            "sid": sid_hex,
                            "scid": scid,
                            "label": l,
                            "bitrate": kbps,
                            "outDir": out_dir_rel.clone(),
                            "wav": "audio.wav",
                            "metadata": "metadata.jsonl",
                            "slidesDir": "slides",
                        }
                    }),
                );
            } else {
                write_jsonl(
                    &mut global_index,
                    json!({
                        "service": {
                            "sid": sid_hex,
                            "scid": scid,
                            "bitrate": kbps,
                            "outDir": out_dir_rel.clone(),
                            "wav": "audio.wav",
                            "metadata": "metadata.jsonl",
                            "slidesDir": "slides",
                        }
                    }),
                );
            }

            let ctx = ServiceDumpContext {
                sid: svc.sid,
                scid,
                out_dir_rel,
                wav,
                meta,
                slide_dir: slides_dir,
                subch_buf: SubchannelBuffer::new(scid, stc.stl),
                aac: init_aac_decoder(&args.aac_decoder, &args.aac_gap),
                pad_decoder: PadDecoder::new(),
                sf_work_buf: Vec::new(),
                emitted_ensemble_label: ensemble_label,
                emitted_service_label: svc.label.clone(),
                last_dl: None,
                last_slide_hash: None,
                dedup_pad: args.dedup_pad,
            };

            info!(
                "Exporting SID={:#06x} SCID={} into {}",
                svc.sid,
                scid,
                service_dir.display()
            );
            contexts.insert(svc.sid, ctx);
        }

        // Phase 1 (serial): label sync — must stay serial to write global_index.
        for ctx in contexts.values_mut() {
            if let Some(current_ensemble_label) = fic.ensemble.label.clone() {
                if ctx.emitted_ensemble_label.as_deref() != Some(current_ensemble_label.as_str()) {
                    write_jsonl(
                        &mut ctx.meta,
                        json!({"ensemble": {"eid": format!("{:#06x}", fic.ensemble.eid), "label": current_ensemble_label}}),
                    );
                    ctx.emitted_ensemble_label = Some(current_ensemble_label);
                }
            }

            if let Some(svc) = fic.services.iter().find(|s| s.sid == ctx.sid) {
                if let Some(current_service_label) = svc.label.clone() {
                    if ctx.emitted_service_label.as_deref() != Some(current_service_label.as_str())
                    {
                        let current_service_dir = out_root.join(&ctx.out_dir_rel);
                        let new_service_dir = out_root.join(service_dir_name(
                            ctx.sid,
                            Some(current_service_label.as_str()),
                        ));

                        if new_service_dir != current_service_dir {
                            if let Err(e) = std::fs::rename(&current_service_dir, &new_service_dir)
                            {
                                warn!(
                                    "Cannot rename service directory SID={:#06x} from {:?} to {:?}: {}",
                                    ctx.sid,
                                    current_service_dir,
                                    new_service_dir,
                                    e
                                );
                            } else {
                                ctx.out_dir_rel = new_service_dir
                                    .strip_prefix(out_root)
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_else(|_| {
                                        new_service_dir.to_string_lossy().to_string()
                                    });
                                ctx.slide_dir = new_service_dir.join("slides");
                                info!(
                                    "Service label resolved, renamed SID={:#06x} directory to {}",
                                    ctx.sid, ctx.out_dir_rel
                                );
                            }
                        }

                        write_jsonl(
                            &mut ctx.meta,
                            json!({"service": {"sid": format!("{:#06x}", ctx.sid), "label": current_service_label}}),
                        );
                        ctx.emitted_service_label = Some(current_service_label);
                        write_jsonl(
                            &mut global_index,
                            json!({
                                "serviceUpdate": {
                                    "sid": format!("{:#06x}", ctx.sid),
                                    "label": ctx.emitted_service_label.as_deref(),
                                    "outDir": ctx.out_dir_rel,
                                }
                            }),
                        );
                    }
                }
            }
        }

        // Phase 1b: clone CIF slices so the parallel phase needs no frame borrow.
        let mut cif_per_service: HashMap<u32, Vec<u8>> = HashMap::new();
        let mut mot_type_per_service: HashMap<u32, Option<u8>> = HashMap::new();
        for (&sid, ctx) in contexts.iter() {
            if let Some(cif) = extract_subchannel(&frame, ctx.scid) {
                cif_per_service.insert(sid, cif.to_vec());
            }
            let mot = fic
                .mot_app_type_for_sid(sid)
                .or_else(|| fic.mot_app_type(ctx.scid));
            mot_type_per_service.insert(sid, mot);
        }
        let slide_base64 = args.slide_base64;

        // Phase 2 (parallel): superframe → AAC → WAV/JSONL, one thread per service.
        let ctxs: Vec<&mut ServiceDumpContext> = contexts.values_mut().collect();
        ctxs.into_par_iter().try_for_each(|ctx| -> Result<()> {
            let _span = tracing::info_span!("service", sid = format!("{:#06x}", ctx.sid)).entered();
            let Some(cif_data) = cif_per_service.get(&ctx.sid) else {
                return Ok(());
            };
            let mot_app_type = mot_type_per_service.get(&ctx.sid).copied().flatten();

            ctx.subch_buf.push_cif(cif_data);

            while ctx.subch_buf.len() >= ctx.subch_buf.superframe_size() {
                let sf_size = ctx.subch_buf.superframe_size();
                let slice = match ctx.subch_buf.try_peek_superframe_slice() {
                    Some(d) => d,
                    None => break,
                };

                if ctx.sf_work_buf.len() != sf_size {
                    ctx.sf_work_buf.resize(sf_size, 0);
                }
                ctx.sf_work_buf.copy_from_slice(slice);

                let result = process_superframe_inplace(&mut ctx.sf_work_buf);
                if !result.firecode_ok {
                    ctx.subch_buf.advance_one_cif();
                    continue;
                }
                ctx.subch_buf.consume_superframe();

                if let Some(fmt) = result.format.as_ref() {
                    if let Some(aac_dec) = ctx.aac.as_mut() {
                        aac_dec.init_format(fmt);
                    }
                }

                for au in result.units {
                    let pad_events = ctx.pad_decoder.process_au(&au.data, mot_app_type);

                    if let Some(dl) = pad_events.dynamic_label {
                        let is_dup = ctx.dedup_pad && ctx.last_dl.as_deref() == Some(dl.as_str());
                        if !is_dup {
                            write_jsonl(&mut ctx.meta, json!({"dl": dl}));
                            ctx.last_dl = Some(dl);
                        }
                    }

                    if let Some(slide) = pad_events.slide {
                        // Hash the slide data to detect changes
                        use std::collections::hash_map::DefaultHasher;
                        use std::hash::{Hash, Hasher};
                        let mut hasher = DefaultHasher::new();
                        slide.data.hash(&mut hasher);
                        let slide_hash = hasher.finish();

                        let is_dup_slide = ctx.dedup_pad && ctx.last_slide_hash == Some(slide_hash);
                        if !is_dup_slide {
                            let path = ctx.slide_dir.join(&slide.content_name);
                            if let Err(e) = std::fs::write(&path, &slide.data) {
                                warn!("Cannot write slide file {:?}: {}", path, e);
                            }

                            let data_base64 = if slide_base64 {
                                base64::engine::general_purpose::STANDARD.encode(&slide.data)
                            } else {
                                String::new()
                            };
                            write_jsonl(
                                &mut ctx.meta,
                                json!({
                                    "slide": {
                                        "contentName": slide.content_name,
                                        "contentType": slide.content_type,
                                        "data": data_base64,
                                    }
                                }),
                            );
                            ctx.last_slide_hash = Some(slide_hash);
                        }
                    }

                    if let Some(aac_dec) = ctx.aac.as_mut() {
                        if let Some(pcm) = aac_dec.decode(&au) {
                            ctx.wav.write_pcm(&pcm)?;
                        }
                    }
                }
            }
            Ok(())
        })?;
    }

    for ctx in contexts.values_mut() {
        ctx.wav.finalize()?;
        ctx.meta.flush()?;
    }
    write_jsonl(
        &mut global_index,
        json!({"summary": {"services": contexts.len(), "frames": frame_count}}),
    );
    global_index.flush()?;

    Ok(())
}

fn service_dir_name(sid: u32, label: Option<&str>) -> String {
    let sid_hex = format!("{:#06x}", sid);
    let safe_label = sanitize_for_path(label.unwrap_or("no-label"));
    format!("{}-{}", sid_hex, safe_label)
}

fn select_service<'a>(fic: &'a FicDecoder, args: &DablinArgs) -> Option<&'a ServiceInfo> {
    if let Some(ref sid_str) = args.sid {
        return fic.find_by_sid(sid_str);
    }
    if let Some(ref label) = args.label {
        return fic.find_by_label(label);
    }
    fic.services.iter().find(|s| !s.components.is_empty())
}

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
    eprintln!(
        "Ensemble: EId={:#06x} label={}",
        fic.ensemble.eid,
        fic.ensemble.label.as_deref().unwrap_or("(no label)")
    );
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

#[cfg(test)]
mod tests {
    use super::service_dir_name;

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
}
