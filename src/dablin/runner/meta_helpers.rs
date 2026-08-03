use serde_json::json;
use std::io::BufWriter;
use std::path::Path;
use tracing::warn;

use super::OUTPUT_SAMPLE_RATE_HZ;
use crate::dablin::dabplus::SuperframeFormat;
use crate::dablin::fic::FicDecoder;
use crate::dablin::metadata::{AudioMeta, MetadataEmitter};
use crate::dablin::shared::{audio_codec_label, audio_mode_label, current_subchannel_protection};
use crate::dablin::utils::jsonl::write_jsonl;

pub(crate) fn emit_subchannel_fd3(
    meta: &mut MetadataEmitter,
    fic: &FicDecoder,
    scid: u8,
) -> Option<String> {
    let protection = current_subchannel_protection(fic, scid);
    if let Some(ref p) = protection {
        meta.emit_subchannel(scid, Some(p.as_str()), fic.is_dabplus(scid));
    }
    protection
}

pub(crate) fn write_subchannel_jsonl(
    meta: &mut BufWriter<std::fs::File>,
    fic: &FicDecoder,
    scid: u8,
    protection: Option<&str>,
) {
    let Some(protection) = protection else {
        return;
    };
    write_jsonl(
        meta,
        json!({
            "subchannel": {
                "id": scid,
                "dabplus": fic.is_dabplus(scid),
                "protection": protection,
            }
        }),
    );
}

pub(crate) fn emit_audio_fd3(
    meta: &mut MetadataEmitter,
    fmt: &SuperframeFormat,
    bitrate_kbps: Option<u32>,
) {
    meta.emit_audio(AudioMeta {
        codec: audio_codec_label(fmt),
        channels: fmt.core_ch_config(),
        mode: audio_mode_label(fmt),
        sample_rate: OUTPUT_SAMPLE_RATE_HZ,
        bitrate: bitrate_kbps,
        sbr: fmt.sbr_flag,
        ps: fmt.ps_flag,
    });
}

pub(crate) fn write_audio_jsonl(
    meta: &mut BufWriter<std::fs::File>,
    fmt: &SuperframeFormat,
    bitrate_kbps: u32,
) {
    write_jsonl(
        meta,
        json!({
            "audio": {
                "codec": audio_codec_label(fmt),
                "channels": fmt.core_ch_config(),
                "mode": audio_mode_label(fmt),
                "sampleRate": OUTPUT_SAMPLE_RATE_HZ,
                "bitrate": bitrate_kbps,
                "sbr": fmt.sbr_flag,
                "ps": fmt.ps_flag,
            }
        }),
    );
}

pub(crate) fn should_emit_slide_metadata(slide_dir: Option<&Path>, slide_base64: bool) -> bool {
    slide_dir.is_some() || slide_base64
}

/// Save a slide file to disk, logging a warning on failure.
pub(crate) fn save_slide_file(dir: &Path, name: &str, data: &[u8]) {
    let path = dir.join(name);
    if let Err(e) = std::fs::write(&path, data) {
        warn!("Cannot write slide file {:?}: {}", path, e);
    }
}
