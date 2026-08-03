use anyhow::Result;
use std::io::Write;
use std::path::Path;
use tracing::{debug, info, warn};

use super::init_aac_decoder;
use crate::cli::{AudioOut, OneServiceOutArgs};
use crate::dablin::audio::adts::AdtsPacker;
use crate::dablin::audio::latm::LatmPacker;
use crate::dablin::audio::AacDecoder;
use crate::dablin::dabplus::{AudioUnit, SuperframeFormat};
use crate::dablin::fic::{FicDecoder, ServiceInfo};
use crate::dablin::metadata::MetadataEmitter;
use crate::dablin::msc::SubchannelBuffer;
use crate::dablin::pad::PadDecoder;
use crate::dablin::runner::meta_helpers::{emit_audio_fd3, emit_subchannel_fd3};
use crate::dablin::runner::output::{write_pcm_or_exit, WriteOutcome};
use crate::dablin::runner::pad_helpers::emit_one_service_pad_events;
use crate::dablin::runner::superframe_helpers::{
    apply_superframe_action, classify_superframe_action, decode_next_superframe,
    handle_non_decodable_superframe, maybe_update_emitted_audio_format,
    process_au_with_pad_and_pcm, process_pad_au, resolve_mot_app_type, write_raw_au_or_exit,
    SuperframeAction,
};
use crate::dablin::shared::{current_subchannel_protection, DateTimeMode};

pub(super) struct OneServiceSelectionState {
    pub(super) selected_scid: Option<u8>,
    pub(super) selected_sid: Option<u32>,
    pub(super) selected_bitrate_kbps: Option<u32>,
}

pub(super) struct OneServiceMetadataState {
    pub(super) emitted_ensemble_eid: Option<u16>,
    pub(super) emitted_ensemble_label: Option<String>,
    pub(super) emitted_ensemble_short_label: Option<String>,
    pub(super) emitted_service_sid: Option<u32>,
    pub(super) emitted_service_label: Option<String>,
    pub(super) emitted_time: Option<(String, String, String)>,
    pub(super) emitted_audio_format: Option<SuperframeFormat>,
    pub(super) emitted_subchannel_protection: Option<String>,
}

pub(super) struct OneServicePadState {
    pub(super) last_dl: Option<String>,
    pub(super) last_slide_hash: Option<u64>,
}

pub(super) struct FicTimeRuntime<'a> {
    pub(super) fic_stable: &'a mut bool,
    pub(super) last_mnsc: &'a mut u16,
    pub(super) metadata_state: &'a mut OneServiceMetadataState,
    pub(super) meta: &'a mut Option<MetadataEmitter>,
}

pub(super) struct OneServiceInitRuntime<'a> {
    pub(super) selection: &'a mut OneServiceSelectionState,
    pub(super) metadata_state: &'a mut OneServiceMetadataState,
    pub(super) subch_buf: &'a mut Option<SubchannelBuffer>,
    pub(super) aac: &'a mut Option<AacDecoder>,
    pub(super) meta: &'a mut Option<MetadataEmitter>,
}

pub(super) struct OneServiceSuperframeRuntime<'a, W: Write> {
    pub(super) args: &'a OneServiceOutArgs,
    pub(super) fic: &'a FicDecoder,
    pub(super) selection: &'a OneServiceSelectionState,
    pub(super) buf: &'a mut SubchannelBuffer,
    pub(super) sf_work_buf: &'a mut Vec<u8>,
    pub(super) aac: &'a mut Option<AacDecoder>,
    pub(super) metadata_state: &'a mut OneServiceMetadataState,
    pub(super) meta: &'a mut Option<MetadataEmitter>,
    pub(super) pad_decoder: &'a mut PadDecoder,
    pub(super) slide_dir: Option<&'a Path>,
    pub(super) pad_state: &'a mut OneServicePadState,
    pub(super) latm_packer: &'a mut LatmPacker,
    pub(super) adts_packer: &'a AdtsPacker,
    pub(super) pcm_write_scratch: &'a mut Vec<u8>,
    pub(super) out: &'a mut W,
}

pub(super) fn maybe_process_fic_and_time(
    frame: &crate::dablin::eti::EtiFrame<'_>,
    fic: &mut FicDecoder,
    selection: &OneServiceSelectionState,
    datetime_mode: Option<DateTimeMode<'_>>,
    runtime: &mut FicTimeRuntime<'_>,
) {
    if !frame.ficf || frame.fic.is_empty() {
        return;
    }

    let mnsc_changed = frame.mnsc != *runtime.last_mnsc;
    *runtime.last_mnsc = frame.mnsc;

    if !*runtime.fic_stable || mnsc_changed || datetime_mode.is_some() {
        if mnsc_changed && *runtime.fic_stable {
            info!("MNSC changed ({:#06x}), re-parsing FIC", frame.mnsc);
        }
        fic.process_fic(frame.fic);

        if !*runtime.fic_stable {
            let svc_stable = selection
                .selected_sid
                .and_then(|sid| fic.services.iter().find(|s| s.sid == sid))
                .map(|s| s.label.is_some())
                .unwrap_or(false);
            if fic.ensemble.label.is_some() && svc_stable {
                *runtime.fic_stable = true;
                debug!("FIC stable - entering MNSC-watch-only mode");
            }
        }

        if let Some((use_iso8601_time, use_time_only, custom_datetime_format)) = datetime_mode {
            if let Some(current_time) = fic.current_dab_time_metadata(
                use_iso8601_time,
                use_time_only,
                custom_datetime_format,
            ) {
                if runtime.metadata_state.emitted_time.as_ref() != Some(&current_time) {
                    if let Some(m) = runtime.meta.as_mut() {
                        m.emit_time(&current_time.0, &current_time.1, &current_time.2);
                    }
                    runtime.metadata_state.emitted_time = Some(current_time);
                }
            }
        }
    }
}

pub(super) fn maybe_select_service_and_init(
    args: &OneServiceOutArgs,
    frame: &crate::dablin::eti::EtiFrame<'_>,
    fic: &FicDecoder,
    runtime: &mut OneServiceInitRuntime<'_>,
) {
    if runtime.selection.selected_scid.is_some() || fic.services.is_empty() {
        return;
    }

    let Some(svc) = select_service(fic, args) else {
        return;
    };
    let Some(comp) = svc.components.first() else {
        return;
    };

    let scid = comp.subch_id;
    runtime.selection.selected_scid = Some(scid);
    runtime.selection.selected_sid = Some(svc.sid);

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
        *runtime.subch_buf = Some(buf);

        let kbps = (u32::from(stc.stl) * 64) / 24;
        runtime.selection.selected_bitrate_kbps = Some(kbps);
        if let Some(m) = runtime.meta.as_mut() {
            runtime.metadata_state.emitted_subchannel_protection =
                emit_subchannel_fd3(m, fic, scid);
        }
    } else {
        warn!("Sub-channel SCID={} not found in STC", scid);
    }

    match args.audio_out {
        AudioOut::Pcm => {
            *runtime.aac = init_aac_decoder(&args.aac_decoder, &args.aac_gap);
        }
        AudioOut::Adts | AudioOut::Latm => {
            // No decoder initialization needed for raw AAC outputs.
        }
    }
}

pub(super) fn sync_one_service_metadata(
    fic: &FicDecoder,
    selection: &OneServiceSelectionState,
    metadata_state: &mut OneServiceMetadataState,
    meta: &mut Option<MetadataEmitter>,
) {
    if let Some(sid) = selection.selected_sid {
        let current_ensemble_label = fic.ensemble.label.as_deref();
        let current_ensemble_short_label = fic.ensemble.short_label.as_deref();

        if let Some(svc) = fic.services.iter().find(|s| s.sid == sid) {
            let current_service_label = svc.label.as_deref();
            if current_service_label.is_some()
                && (metadata_state.emitted_service_sid != Some(sid)
                    || metadata_state.emitted_service_label.as_deref() != current_service_label)
            {
                if metadata_state.emitted_service_sid == Some(sid)
                    && metadata_state.emitted_service_label.is_none()
                {
                    info!(
                        "Service label resolved: SID={:#06x} label={:?}",
                        sid, current_service_label
                    );
                }
                if let Some(m) = meta.as_mut() {
                    m.emit_service(sid, current_service_label);
                }
                metadata_state.emitted_service_sid = Some(sid);
                metadata_state.emitted_service_label = svc.label.clone();
            } else if metadata_state.emitted_service_sid.is_none() {
                metadata_state.emitted_service_sid = Some(sid);
            }
        }

        if current_ensemble_label.is_some()
            && (metadata_state.emitted_ensemble_eid != Some(fic.ensemble.eid)
                || metadata_state.emitted_ensemble_label.as_deref() != current_ensemble_label
                || metadata_state.emitted_ensemble_short_label.as_deref()
                    != current_ensemble_short_label)
        {
            if let Some(m) = meta.as_mut() {
                m.emit_ensemble(
                    fic.ensemble.eid,
                    current_ensemble_label,
                    current_ensemble_short_label,
                );
            }
            metadata_state.emitted_ensemble_eid = Some(fic.ensemble.eid);
            metadata_state.emitted_ensemble_label = fic.ensemble.label.clone();
            metadata_state.emitted_ensemble_short_label = fic.ensemble.short_label.clone();
        }
    }

    if let Some(scid) = selection.selected_scid {
        if let Some(m) = meta.as_mut() {
            let protection = current_subchannel_protection(fic, scid);
            if protection.is_some() && metadata_state.emitted_subchannel_protection != protection {
                metadata_state.emitted_subchannel_protection = emit_subchannel_fd3(m, fic, scid);
            }
        }
    }
}

pub(super) fn process_one_service_superframes<W: Write>(
    runtime: &mut OneServiceSuperframeRuntime<'_, W>,
) -> Result<bool> {
    while runtime.buf.len() >= runtime.buf.superframe_size() {
        let result = match decode_next_superframe(runtime.buf, runtime.sf_work_buf) {
            Some(result) => result,
            None => break,
        };

        let action = classify_superframe_action(&result);
        match action {
            SuperframeAction::AdvanceOneCif => {
                debug!("DAB+ FireCode mismatch - advancing one CIF");
            }
            SuperframeAction::ConsumeSuperframe => {
                debug!(
                    "Superframe too corrupted (RS corrected {} codewords) - applying gap policy",
                    result.rs_corrected
                );
            }
            SuperframeAction::DecodeUnits => {}
        }

        if action != SuperframeAction::DecodeUnits {
            let closed = handle_non_decodable_superframe(
                runtime.buf,
                runtime.aac.as_ref(),
                action,
                |pcm| {
                    Ok(matches!(
                        write_pcm_or_exit(runtime.out, pcm, runtime.pcm_write_scratch)?,
                        WriteOutcome::Closed
                    ))
                },
            )?;
            if closed {
                return Ok(true);
            }
            continue;
        }

        apply_superframe_action(runtime.buf, action);

        if result.rs_corrected > 0 {
            debug!("RS corrected {} codewords", result.rs_corrected);
        }

        maybe_update_emitted_audio_format(
            &mut runtime.metadata_state.emitted_audio_format,
            result.format.as_ref(),
            |fmt| {
                if let Some(aac_dec) = runtime.aac.as_mut() {
                    aac_dec.init_format(fmt);
                }
                if let Some(m) = runtime.meta.as_mut() {
                    emit_audio_fd3(m, fmt, runtime.selection.selected_bitrate_kbps);
                }
            },
        );

        let mot_app_type = resolve_mot_app_type(
            runtime.fic,
            runtime.selection.selected_sid,
            runtime.selection.selected_scid,
        );

        match runtime.args.audio_out {
            AudioOut::Pcm => {
                for au in &result.units {
                    if process_one_service_pcm_au(runtime, au, mot_app_type)? {
                        return Ok(true);
                    }
                }
            }
            AudioOut::Adts | AudioOut::Latm => {
                for au in &result.units {
                    if process_one_service_raw_au(
                        runtime,
                        au,
                        mot_app_type,
                        result.format.as_ref(),
                    )? {
                        return Ok(true);
                    }
                }
            }
        }
    }

    Ok(false)
}

fn emit_one_service_pad_for_au<W: Write>(
    runtime: &mut OneServiceSuperframeRuntime<'_, W>,
    au: &AudioUnit,
    mot_app_type: Option<u8>,
) -> Result<()> {
    if runtime.selection.selected_scid.is_none() {
        return Ok(());
    }

    process_pad_au(runtime.pad_decoder, au, mot_app_type, |pad_events| {
        emit_one_service_pad_events(
            pad_events,
            runtime.meta,
            runtime.slide_dir,
            runtime.args.slide_base64,
            runtime.args.dedup_pad,
            &mut runtime.pad_state.last_dl,
            &mut runtime.pad_state.last_slide_hash,
        );
        Ok(())
    })
}

fn process_one_service_pcm_au<W: Write>(
    runtime: &mut OneServiceSuperframeRuntime<'_, W>,
    au: &AudioUnit,
    mot_app_type: Option<u8>,
) -> Result<bool> {
    process_au_with_pad_and_pcm(
        runtime.pad_decoder,
        au,
        mot_app_type,
        |pad_events| {
            if runtime.selection.selected_scid.is_some() {
                emit_one_service_pad_events(
                    pad_events,
                    runtime.meta,
                    runtime.slide_dir,
                    runtime.args.slide_base64,
                    runtime.args.dedup_pad,
                    &mut runtime.pad_state.last_dl,
                    &mut runtime.pad_state.last_slide_hash,
                );
            }
            Ok(())
        },
        runtime.aac.as_mut(),
        true,
        |pcm| {
            Ok(matches!(
                write_pcm_or_exit(runtime.out, pcm, runtime.pcm_write_scratch)?,
                WriteOutcome::Closed
            ))
        },
    )
}

fn process_one_service_raw_au<W: Write>(
    runtime: &mut OneServiceSuperframeRuntime<'_, W>,
    au: &AudioUnit,
    mot_app_type: Option<u8>,
    fmt: Option<&SuperframeFormat>,
) -> Result<bool> {
    emit_one_service_pad_for_au(runtime, au, mot_app_type)?;
    write_raw_au_or_exit(
        &runtime.args.audio_out,
        fmt,
        au,
        runtime.adts_packer,
        runtime.latm_packer,
        runtime.out,
    )
}

fn select_service<'a>(fic: &'a FicDecoder, args: &OneServiceOutArgs) -> Option<&'a ServiceInfo> {
    if let Some(ref sid_str) = args.sid {
        return fic.find_by_sid(sid_str);
    }
    if let Some(ref label) = args.label {
        return fic.find_by_label(label);
    }
    fic.services.iter().find(|s| !s.components.is_empty())
}
