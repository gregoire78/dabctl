use anyhow::{Context, Result};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::BufWriter;
use std::path::Path;
use tracing::{info, warn};

use super::{init_aac_decoder, service_dir_name, ServiceDumpContext};
use crate::cli::AllServicesOutArgs;
use crate::dablin::eti::EtiFrame;
use crate::dablin::fic::FicDecoder;
use crate::dablin::msc::{extract_subchannel, SubchannelBuffer};
use crate::dablin::pad::PadDecoder;
use crate::dablin::runner::meta_helpers::{write_audio_jsonl, write_subchannel_jsonl};
use crate::dablin::runner::pad_helpers::emit_all_services_pad_events;
use crate::dablin::runner::superframe_helpers::{
    apply_superframe_action, classify_superframe_action, decode_next_superframe,
    handle_non_decodable_superframe, maybe_update_emitted_audio_format,
    process_au_with_pad_and_pcm, resolve_mot_app_type, SuperframeAction,
};
use crate::dablin::shared::{current_subchannel_protection, DateTimeMode};
use crate::dablin::utils::jsonl::write_jsonl;
use crate::dablin::utils::wav_writer::WavWriter;

pub(super) fn sync_all_services_labels_and_subchannel(
    contexts: &mut BTreeMap<u32, ServiceDumpContext>,
    fic: &FicDecoder,
    out_root: &Path,
) {
    let current_ensemble_label = fic.ensemble.label.as_deref();
    let current_ensemble_short_label = fic.ensemble.short_label.as_deref();

    for ctx in contexts.values_mut() {
        if let Some(current_ensemble_label) = current_ensemble_label {
            if ctx.emitted_ensemble_label.as_deref() != Some(current_ensemble_label)
                || ctx.emitted_ensemble_short_label.as_deref() != current_ensemble_short_label
            {
                let mut ensemble = json!({
                    "eid": format!("{:#06x}", fic.ensemble.eid),
                    "label": current_ensemble_label,
                });
                if let Some(s) = current_ensemble_short_label {
                    ensemble["shortLabel"] = json!(s);
                }
                write_jsonl(&mut ctx.meta, json!({"ensemble": ensemble}));
                ctx.emitted_ensemble_label = fic.ensemble.label.clone();
                ctx.emitted_ensemble_short_label = fic.ensemble.short_label.clone();
            }
        }

        if let Some(svc) = fic.services.iter().find(|s| s.sid == ctx.sid) {
            if let Some(current_service_label) = svc.label.as_deref() {
                if ctx.emitted_service_label.as_deref() != Some(current_service_label) {
                    let current_service_dir = out_root.join(&ctx.out_dir_rel);
                    let new_service_dir =
                        out_root.join(service_dir_name(ctx.sid, Some(current_service_label)));

                    if new_service_dir != current_service_dir {
                        if let Err(e) = std::fs::rename(&current_service_dir, &new_service_dir) {
                            warn!(
                                "Cannot rename service directory SID={:#06x} from {:?} to {:?}: {}",
                                ctx.sid, current_service_dir, new_service_dir, e
                            );
                        } else {
                            ctx.out_dir_rel = new_service_dir
                                .strip_prefix(out_root)
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|_| new_service_dir.to_string_lossy().to_string());
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
                    ctx.emitted_service_label = Some(current_service_label.to_string());
                }
            }
        }

        let current_protection = current_subchannel_protection(fic, ctx.scid);
        if ctx.emitted_subchannel_protection != current_protection {
            write_subchannel_jsonl(&mut ctx.meta, fic, ctx.scid, current_protection.as_deref());
            ctx.emitted_subchannel_protection = current_protection;
        }
    }
}

pub(super) fn maybe_emit_all_services_time(
    contexts: &mut BTreeMap<u32, ServiceDumpContext>,
    fic: &FicDecoder,
    datetime_mode: Option<DateTimeMode<'_>>,
    emitted_time: &mut Option<(String, String, String)>,
) {
    if let Some((use_iso8601_time, use_time_only, custom_datetime_format)) = datetime_mode {
        if let Some(current_time) =
            fic.current_dab_time_metadata(use_iso8601_time, use_time_only, custom_datetime_format)
        {
            if emitted_time.as_ref() != Some(&current_time) {
                for ctx in contexts.values_mut() {
                    write_jsonl(
                        &mut ctx.meta,
                        json!({
                            "time": {
                                "utc": &current_time.0,
                                "local": &current_time.1,
                                "lto": &current_time.2,
                            }
                        }),
                    );
                }
                *emitted_time = Some(current_time);
            }
        }
    }
}

pub(super) fn discover_all_services_contexts(
    args: &AllServicesOutArgs,
    frame: &EtiFrame<'_>,
    fic: &FicDecoder,
    contexts: &mut BTreeMap<u32, ServiceDumpContext>,
    out_root: &Path,
    datetime_mode: Option<DateTimeMode<'_>>,
) -> Result<()> {
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
        std::fs::create_dir_all(&slides_dir)
            .with_context(|| format!("cannot create slides directory: {}", slides_dir.display()))?;

        let wav = WavWriter::create(&service_dir.join("audio.wav"))?;
        let meta_file = std::fs::File::create(service_dir.join("metadata.jsonl"))
            .with_context(|| format!("cannot create metadata file for SID {:#06x}", svc.sid))?;
        let mut meta = BufWriter::new(meta_file);

        let ensemble_label = fic.ensemble.label.clone();
        let ensemble_short_label = fic.ensemble.short_label.clone();
        if let Some(l) = ensemble_label.as_deref() {
            let mut ensemble = json!({"eid": format!("{:#06x}", fic.ensemble.eid), "label": l});
            if let Some(s) = ensemble_short_label.as_deref() {
                ensemble["shortLabel"] = json!(s);
            }
            write_jsonl(&mut meta, json!({"ensemble": ensemble}));
        }
        if let Some(l) = svc.label.as_deref() {
            write_jsonl(&mut meta, json!({"service": {"sid": sid_hex, "label": l}}));
        }
        if let Some((use_iso8601_time, use_time_only, custom_datetime_format)) = datetime_mode {
            if let Some((utc, local, lto)) = fic.current_dab_time_metadata(
                use_iso8601_time,
                use_time_only,
                custom_datetime_format,
            ) {
                write_jsonl(
                    &mut meta,
                    json!({"time": {"utc": utc, "local": local, "lto": lto}}),
                );
            }
        }
        let kbps = (u32::from(stc.stl) * 64) / 24;
        let protection = current_subchannel_protection(fic, scid);
        write_subchannel_jsonl(&mut meta, fic, scid, protection.as_deref());

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
            emitted_ensemble_short_label: ensemble_short_label,
            emitted_service_label: svc.label.clone(),
            last_dl: None,
            last_slide_hash: None,
            dedup_pad: args.dedup_pad,
            emitted_audio_format: None,
            emitted_subchannel_protection: protection,
            bitrate_kbps: kbps,
        };

        info!(
            "Exporting SID={:#06x} SCID={} into {}",
            svc.sid,
            scid,
            service_dir.display()
        );
        contexts.insert(svc.sid, ctx);
    }

    Ok(())
}

pub(super) fn process_all_services_parallel_context(
    ctx: &mut ServiceDumpContext,
    frame: &EtiFrame<'_>,
    fic: &FicDecoder,
    slide_base64: bool,
) -> Result<()> {
    let _span = tracing::info_span!("service", sid = format!("{:#06x}", ctx.sid)).entered();

    let Some(cif_data) = extract_subchannel(frame, ctx.scid) else {
        return Ok(());
    };
    let mot_app_type = resolve_mot_app_type(fic, Some(ctx.sid), Some(ctx.scid));

    ctx.subch_buf.push_cif(cif_data);

    while ctx.subch_buf.len() >= ctx.subch_buf.superframe_size() {
        let result = match decode_next_superframe(&ctx.subch_buf, &mut ctx.sf_work_buf) {
            Some(result) => result,
            None => break,
        };
        let action = classify_superframe_action(&result);
        if action != SuperframeAction::DecodeUnits {
            handle_non_decodable_superframe(&mut ctx.subch_buf, ctx.aac.as_ref(), action, |pcm| {
                ctx.wav.write_pcm(pcm)?;
                Ok(false)
            })?;
            continue;
        }

        apply_superframe_action(&mut ctx.subch_buf, action);

        maybe_update_emitted_audio_format(
            &mut ctx.emitted_audio_format,
            result.format.as_ref(),
            |fmt| {
                if let Some(aac_dec) = ctx.aac.as_mut() {
                    aac_dec.init_format(fmt);
                }
                write_audio_jsonl(&mut ctx.meta, fmt, ctx.bitrate_kbps);
            },
        );

        for au in &result.units {
            process_all_services_au(ctx, au, mot_app_type, slide_base64)?;
        }
    }

    Ok(())
}

fn process_all_services_au(
    ctx: &mut ServiceDumpContext,
    au: &crate::dablin::dabplus::AudioUnit,
    mot_app_type: Option<u8>,
    slide_base64: bool,
) -> Result<()> {
    let _ = process_au_with_pad_and_pcm(
        &mut ctx.pad_decoder,
        au,
        mot_app_type,
        |pad_events| {
            emit_all_services_pad_events(
                pad_events,
                &mut ctx.meta,
                &ctx.slide_dir,
                slide_base64,
                ctx.dedup_pad,
                &mut ctx.last_dl,
                &mut ctx.last_slide_hash,
            );
            Ok(())
        },
        ctx.aac.as_mut(),
        false,
        |pcm| {
            ctx.wav.write_pcm(pcm)?;
            Ok(false)
        },
    )?;

    Ok(())
}
