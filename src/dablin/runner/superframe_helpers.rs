use crate::cli::AudioOut;
use crate::dablin::audio::adts::AdtsPacker;
use crate::dablin::audio::latm::LatmPacker;
use crate::dablin::audio::AacDecoder;
use crate::dablin::dabplus::{
    process_superframe_inplace, AudioUnit, SuperframeFormat, SuperframeResult,
};
use crate::dablin::fic::FicDecoder;
use crate::dablin::msc::SubchannelBuffer;
use crate::dablin::pad::{PadDecoder, PadEvents};
use crate::dablin::runner::output::{write_adts_or_exit, write_latm_or_exit, WriteOutcome};
use anyhow::Result;
use std::io::Write;
use tracing::debug;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SuperframeAction {
    AdvanceOneCif,
    ConsumeSuperframe,
    DecodeUnits,
}

/// Decode one superframe from the current subchannel buffer without consuming it.
///
/// Returns `None` when no full superframe is available.
pub(crate) fn decode_next_superframe(
    buf: &SubchannelBuffer,
    sf_work_buf: &mut Vec<u8>,
) -> Option<SuperframeResult> {
    let sf_size = buf.superframe_size();
    let slice = buf.try_peek_superframe_slice()?;

    if sf_work_buf.len() != sf_size {
        sf_work_buf.resize(sf_size, 0);
    }
    sf_work_buf.copy_from_slice(slice);

    Some(process_superframe_inplace(sf_work_buf))
}

pub(crate) fn classify_superframe_action(result: &SuperframeResult) -> SuperframeAction {
    if !result.firecode_ok {
        return SuperframeAction::AdvanceOneCif;
    }
    if result.rs_over_threshold {
        return SuperframeAction::ConsumeSuperframe;
    }
    SuperframeAction::DecodeUnits
}

pub(crate) fn apply_superframe_action(buf: &mut SubchannelBuffer, action: SuperframeAction) {
    match action {
        SuperframeAction::AdvanceOneCif => buf.advance_one_cif(),
        SuperframeAction::ConsumeSuperframe | SuperframeAction::DecodeUnits => {
            buf.consume_superframe()
        }
    }
}

pub(crate) fn silence_for_superframe_action(
    aac_dec: &AacDecoder,
    action: SuperframeAction,
) -> Option<Vec<i16>> {
    match action {
        SuperframeAction::AdvanceOneCif => aac_dec.silence_for_missing_cifs(1),
        SuperframeAction::ConsumeSuperframe => aac_dec.silence_for_corrupted_superframe(5),
        SuperframeAction::DecodeUnits => None,
    }
}

pub(crate) fn handle_non_decodable_superframe(
    buf: &mut SubchannelBuffer,
    aac_dec: Option<&AacDecoder>,
    action: SuperframeAction,
    mut on_silence_pcm: impl FnMut(&[i16]) -> Result<bool>,
) -> Result<bool> {
    if action == SuperframeAction::DecodeUnits {
        return Ok(false);
    }

    if let Some(aac_dec) = aac_dec {
        if let Some(pcm) = silence_for_superframe_action(aac_dec, action) {
            if on_silence_pcm(&pcm)? {
                return Ok(true);
            }
        }
    }

    apply_superframe_action(buf, action);
    Ok(false)
}

pub(crate) fn maybe_update_emitted_audio_format(
    emitted_audio_format: &mut Option<SuperframeFormat>,
    current_format: Option<&SuperframeFormat>,
    mut on_format_changed: impl FnMut(&SuperframeFormat),
) {
    let Some(fmt) = current_format else {
        return;
    };

    if emitted_audio_format.as_ref() != Some(fmt) {
        on_format_changed(fmt);
        *emitted_audio_format = Some(fmt.clone());
    }
}

pub(crate) fn resolve_mot_app_type(
    fic: &FicDecoder,
    sid: Option<u32>,
    scid: Option<u8>,
) -> Option<u8> {
    sid.and_then(|sid| fic.mot_app_type_for_sid(sid))
        .or_else(|| scid.and_then(|scid| fic.mot_app_type(scid)))
}

pub(crate) fn decode_pcm_au(
    aac_dec: Option<&mut AacDecoder>,
    au: &AudioUnit,
    log_unexpected_gap: bool,
    mut on_pcm: impl FnMut(&[i16]) -> Result<bool>,
) -> Result<bool> {
    let Some(aac_dec) = aac_dec else {
        return Ok(false);
    };

    match aac_dec.decode(au) {
        Some(pcm) => on_pcm(&pcm),
        None => {
            if log_unexpected_gap {
                // This should not happen with silence policy - silence is generated inside decode().
                debug!("AAC gap: no PCM (unexpected with silence policy)");
            }
            Ok(false)
        }
    }
}

pub(crate) fn process_pad_au(
    pad_decoder: &mut PadDecoder,
    au: &AudioUnit,
    mot_app_type: Option<u8>,
    mut on_pad_events: impl FnMut(PadEvents) -> Result<()>,
) -> Result<()> {
    let pad_events = pad_decoder.process_au(&au.data, mot_app_type);
    on_pad_events(pad_events)
}

pub(crate) fn process_au_with_pad_and_pcm(
    pad_decoder: &mut PadDecoder,
    au: &AudioUnit,
    mot_app_type: Option<u8>,
    mut on_pad_events: impl FnMut(PadEvents) -> Result<()>,
    aac_dec: Option<&mut AacDecoder>,
    log_unexpected_gap: bool,
    on_pcm: impl FnMut(&[i16]) -> Result<bool>,
) -> Result<bool> {
    process_pad_au(pad_decoder, au, mot_app_type, |pad_events| {
        on_pad_events(pad_events)
    })?;
    decode_pcm_au(aac_dec, au, log_unexpected_gap, on_pcm)
}

pub(crate) fn write_raw_au_or_exit<W: Write>(
    audio_out: &AudioOut,
    fmt: Option<&SuperframeFormat>,
    au: &AudioUnit,
    adts_packer: &AdtsPacker,
    latm_packer: &mut LatmPacker,
    out: &mut W,
) -> Result<bool> {
    let Some(fmt) = fmt else {
        return Ok(false);
    };

    match audio_out {
        AudioOut::Adts => {
            let adts_frame = adts_packer.wrap(fmt, &au.data);
            Ok(matches!(
                write_adts_or_exit(out, &adts_frame)?,
                WriteOutcome::Closed
            ))
        }
        AudioOut::Latm => {
            let latm_packet = latm_packer.wrap(fmt, &au.data);
            Ok(matches!(
                write_latm_or_exit(out, latm_packet)?,
                WriteOutcome::Closed
            ))
        }
        AudioOut::Pcm => Ok(false),
    }
}
