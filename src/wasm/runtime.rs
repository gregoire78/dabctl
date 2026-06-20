//! WebAssembly-oriented runtime scaffolding.
//!
//! This module is intentionally minimal and keeps the native CLI behavior unchanged.
//! It defines the memory-based decode core used by the wasm-bindgen layer
//! implemented in `src/wasm/bindings.rs`.

#![cfg_attr(feature = "wasm-runtime", allow(dead_code))]

use anyhow::{bail, Result};
use serde_json::json;

use crate::cli::DateTimeFormat;
use crate::dablin::audio::latm::LatmPacker;
use crate::dablin::dabplus::{process_superframe_inplace, SuperframeFormat};
use crate::dablin::eti::{parse_frame, EtiFrame, ETI_FRAME_SIZE};
use crate::dablin::fic::{FicDecoder, ServiceInfo};
use crate::dablin::msc::{extract_subchannel, SubchannelBuffer};
use crate::dablin::pad::PadDecoder;
use crate::dablin::shared::{
    audio_codec_label, audio_mode_label, current_subchannel_protection, datetime_mode_from_option,
    encode_slide_base64, hash_bytes, DateTimeMode,
};
use std::collections::BTreeMap;

const OUTPUT_SAMPLE_RATE_HZ: u32 = 48_000;

#[derive(Default)]
struct WasmMetadataState {
    ensemble_emitted: bool,
    service_emitted: bool,
    audio_format: Option<SuperframeFormat>,
    subchannel_protection: Option<String>,
    emitted_time: Option<(String, String, String)>,
    last_dl: Option<String>,
    last_slide_hash: Option<u64>,
}

#[derive(Default)]
struct WasmDecodeSelectionState {
    sid: Option<u32>,
    scid: Option<u8>,
    bitrate_kbps: Option<u32>,
    subch_buf: Option<SubchannelBuffer>,
}

/// Decode options for the WASM memory API, aligned with CLI one-service-out LATM behavior.
#[derive(Debug, Clone, Default)]
pub struct WasmLatmDecodeOptions {
    /// Service ID to decode (hex, e.g. 0xF2F8).
    pub sid: Option<String>,
    /// Select service by label (case-insensitive prefix).
    pub label: Option<String>,
    /// Include slide payload as base64 in metadata events.
    pub slide_base64: bool,
    /// Deduplicate consecutive identical PAD events (DL and slides).
    pub dedup_pad: bool,
    /// Date/time metadata format. None disables time metadata events.
    pub datetime_format: Option<DateTimeFormat>,
}

/// Decode options for the WASM memory API in all-services mode.
#[derive(Debug, Clone, Default)]
pub struct WasmAllServicesDecodeOptions {
    /// Include slide payload as base64 in metadata events.
    pub slide_base64: bool,
    /// Deduplicate consecutive identical PAD events (DL and slides).
    pub dedup_pad: bool,
    /// Date/time metadata format. None disables time metadata events.
    pub datetime_format: Option<DateTimeFormat>,
}

fn push_jsonl_event(metadata_jsonl: &mut Vec<String>, value: serde_json::Value) {
    metadata_jsonl.push(value.to_string());
}

fn emit_subchannel_if_changed(
    fic: &FicDecoder,
    scid: u8,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
) {
    let protection = current_subchannel_protection(fic, scid);
    if protection.is_none() || metadata_state.subchannel_protection == protection {
        return;
    }

    let protection_value = protection.expect("checked above");
    push_jsonl_event(
        metadata_jsonl,
        json!({
            "subchannel": {
                "id": scid,
                "dabplus": fic.is_dabplus(scid),
                "protection": protection_value,
            }
        }),
    );
    metadata_state.subchannel_protection = Some(protection_value);
}

fn emit_ensemble_if_ready(
    fic: &FicDecoder,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
) {
    if metadata_state.ensemble_emitted {
        return;
    }

    let mut ensemble = json!({"eid": format!("{:#06x}", fic.ensemble.eid)});
    if let Some(label) = fic.ensemble.label.as_deref() {
        ensemble["label"] = json!(label);
    }
    if let Some(short_label) = fic.ensemble.short_label.as_deref() {
        ensemble["shortLabel"] = json!(short_label);
    }

    push_jsonl_event(metadata_jsonl, json!({"ensemble": ensemble}));
    metadata_state.ensemble_emitted = true;
}

fn emit_service_if_needed(
    fic: &FicDecoder,
    selected_sid: u32,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
) {
    if metadata_state.service_emitted {
        return;
    }

    let label = fic
        .services
        .iter()
        .find(|s| s.sid == selected_sid)
        .and_then(|s| s.label.as_deref());

    let service = if let Some(lbl) = label {
        json!({"sid": format!("{:#06x}", selected_sid), "label": lbl})
    } else {
        json!({"sid": format!("{:#06x}", selected_sid)})
    };

    push_jsonl_event(metadata_jsonl, json!({"service": service}));
    metadata_state.service_emitted = true;
}

fn emit_audio_if_changed(
    format: &SuperframeFormat,
    bitrate_kbps: Option<u32>,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
) {
    if metadata_state.audio_format.as_ref() == Some(format) {
        return;
    }

    let mut audio = json!({
        "codec": audio_codec_label(format),
        "channels": format.core_ch_config(),
        "mode": audio_mode_label(format),
        "sampleRate": OUTPUT_SAMPLE_RATE_HZ,
        "sbr": format.sbr_flag,
        "ps": format.ps_flag,
    });
    if let Some(kbps) = bitrate_kbps {
        audio["bitrate"] = json!(kbps);
    }

    push_jsonl_event(metadata_jsonl, json!({"audio": audio}));
    metadata_state.audio_format = Some(format.clone());
}

fn select_service<'a>(
    fic: &'a FicDecoder,
    options: &WasmLatmDecodeOptions,
) -> Option<&'a ServiceInfo> {
    if let Some(ref sid_str) = options.sid {
        return fic.find_by_sid(sid_str);
    }
    if let Some(ref label) = options.label {
        return fic.find_by_label(label);
    }
    fic.services.iter().find(|s| !s.components.is_empty())
}

fn first_component_scid(service: &ServiceInfo) -> Option<u8> {
    service.components.first().map(|c| c.subch_id)
}

fn init_subchannel_buffer_from_frame(
    frame: &EtiFrame<'_>,
    scid: u8,
    selection: &mut WasmDecodeSelectionState,
) {
    if let Some(stc) = frame.stc.iter().find(|entry| entry.scid == scid) {
        selection.subch_buf = Some(SubchannelBuffer::new(scid, stc.stl));
        selection.bitrate_kbps = Some((u32::from(stc.stl) * 64) / 24);
    }
}

fn select_service_if_needed(
    fic: &FicDecoder,
    frame: &EtiFrame<'_>,
    options: &WasmLatmDecodeOptions,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
    selection: &mut WasmDecodeSelectionState,
) {
    if selection.scid.is_some() {
        return;
    }

    let Some(service) = select_service(fic, options) else {
        return;
    };
    let Some(scid) = first_component_scid(service) else {
        return;
    };

    let sid = service.sid;
    selection.sid = Some(sid);
    selection.scid = Some(scid);
    emit_service_if_needed(fic, sid, metadata_state, metadata_jsonl);
    emit_subchannel_if_changed(fic, scid, metadata_state, metadata_jsonl);
    init_subchannel_buffer_from_frame(frame, scid, selection);
}

fn process_pad_metadata_for_units(
    units: &[crate::dablin::dabplus::AudioUnit],
    fic: &FicDecoder,
    selected_sid: u32,
    scid: u8,
    options: &WasmLatmDecodeOptions,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
    pad_decoder: &mut PadDecoder,
) {
    let mot_app_type = fic
        .mot_app_type_for_sid(selected_sid)
        .or_else(|| fic.mot_app_type(scid));

    for au in units {
        let pad_events = pad_decoder.process_au(&au.data, mot_app_type);

        if let Some(dl) = pad_events.dynamic_label {
            let is_dup =
                options.dedup_pad && metadata_state.last_dl.as_deref() == Some(dl.as_str());
            if !is_dup {
                push_jsonl_event(metadata_jsonl, json!({"dl": dl}));
                metadata_state.last_dl = Some(dl);
            }
        }

        if let Some(slide) = pad_events.slide {
            let slide_hash = hash_bytes(&slide.data);
            let is_dup_slide =
                options.dedup_pad && metadata_state.last_slide_hash == Some(slide_hash);
            if !is_dup_slide && options.slide_base64 {
                let data_base64 = encode_slide_base64(&slide.data, options.slide_base64);
                push_jsonl_event(
                    metadata_jsonl,
                    json!({
                        "slide": {
                            "contentName": slide.content_name,
                            "contentType": slide.content_type,
                            "data": data_base64,
                        }
                    }),
                );
                metadata_state.last_slide_hash = Some(slide_hash);
            }
        }
    }
}

fn emit_time_if_changed(
    fic: &FicDecoder,
    options: &WasmLatmDecodeOptions,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
) {
    emit_time_with_mode_if_changed(
        fic,
        datetime_mode_from_option(options.datetime_format.as_ref()),
        metadata_state,
        metadata_jsonl,
    );
}

/// Output container for a memory-based LATM decode call.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LatmDecodeOutput {
    /// Concatenated LATM/LOAS bytes.
    pub latm_bytes: Vec<u8>,
    /// Metadata events as JSONL lines.
    pub metadata_jsonl: Vec<String>,
}

/// Per-service memory output in all-services decode mode.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ServiceLatmDecodeOutput {
    pub sid: u32,
    pub label: Option<String>,
    pub latm_bytes: Vec<u8>,
    pub metadata_jsonl: Vec<String>,
}

/// Output container for all-services memory decode mode.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AllServicesLatmDecodeOutput {
    pub services: Vec<ServiceLatmDecodeOutput>,
}

struct WasmAllServicesContext {
    sid: u32,
    scid: u8,
    bitrate_kbps: u32,
    metadata_state: WasmMetadataState,
    subch_buf: SubchannelBuffer,
    latm_packer: LatmPacker,
    pad_decoder: PadDecoder,
    sf_work_buf: Vec<u8>,
    out: ServiceLatmDecodeOutput,
}

fn emit_time_with_mode_if_changed(
    fic: &FicDecoder,
    mode: Option<DateTimeMode<'_>>,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
) {
    let Some((use_iso8601_time, use_time_only, custom_datetime_format)) = mode else {
        return;
    };

    if let Some(current_time) =
        fic.current_dab_time_metadata(use_iso8601_time, use_time_only, custom_datetime_format)
    {
        if metadata_state.emitted_time.as_ref() != Some(&current_time) {
            push_jsonl_event(
                metadata_jsonl,
                json!({
                    "time": {
                        "utc": current_time.0,
                        "local": current_time.1,
                        "lto": current_time.2,
                    }
                }),
            );
            metadata_state.emitted_time = Some(current_time);
        }
    }
}

fn update_all_services_label_sync(
    fic: &FicDecoder,
    contexts: &mut BTreeMap<u32, WasmAllServicesContext>,
) {
    for ctx in contexts.values_mut() {
        emit_ensemble_if_ready(fic, &mut ctx.metadata_state, &mut ctx.out.metadata_jsonl);

        if let Some(svc) = fic.services.iter().find(|s| s.sid == ctx.sid) {
            if let Some(label) = svc.label.clone() {
                if ctx.out.label.as_deref() != Some(label.as_str()) {
                    ctx.out.label = Some(label);
                    emit_service_if_needed(
                        fic,
                        ctx.sid,
                        &mut ctx.metadata_state,
                        &mut ctx.out.metadata_jsonl,
                    );
                }
            }
        }

        emit_subchannel_if_changed(
            fic,
            ctx.scid,
            &mut ctx.metadata_state,
            &mut ctx.out.metadata_jsonl,
        );
    }
}

fn process_one_all_services_context(
    fic: &FicDecoder,
    options: &WasmAllServicesDecodeOptions,
    ctx: &mut WasmAllServicesContext,
    cif_data: &[u8],
) {
    ctx.subch_buf.push_cif(cif_data);

    while ctx.subch_buf.len() >= ctx.subch_buf.superframe_size() {
        let sf_size = ctx.subch_buf.superframe_size();
        let Some(slice) = ctx.subch_buf.try_peek_superframe_slice() else {
            break;
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
        if result.rs_over_threshold {
            continue;
        }

        if let Some(fmt) = result.format.as_ref() {
            process_pad_metadata_for_units(
                &result.units,
                fic,
                ctx.sid,
                ctx.scid,
                &WasmLatmDecodeOptions {
                    slide_base64: options.slide_base64,
                    dedup_pad: options.dedup_pad,
                    ..Default::default()
                },
                &mut ctx.metadata_state,
                &mut ctx.out.metadata_jsonl,
                &mut ctx.pad_decoder,
            );

            emit_audio_if_changed(
                fmt,
                Some(ctx.bitrate_kbps),
                &mut ctx.metadata_state,
                &mut ctx.out.metadata_jsonl,
            );

            for au in result.units {
                let packet = ctx.latm_packer.wrap(fmt, &au.data);
                ctx.out.latm_bytes.extend_from_slice(packet);
            }
        }
    }
}

fn make_all_services_context(
    fic: &FicDecoder,
    svc: &ServiceInfo,
    scid: u8,
    stl: u16,
    options: &WasmAllServicesDecodeOptions,
) -> WasmAllServicesContext {
    let bitrate_kbps = (u32::from(stl) * 64) / 24;
    let mut ctx = WasmAllServicesContext {
        sid: svc.sid,
        scid,
        bitrate_kbps,
        metadata_state: WasmMetadataState::default(),
        subch_buf: SubchannelBuffer::new(scid, stl),
        latm_packer: LatmPacker::new(),
        pad_decoder: PadDecoder::new(),
        sf_work_buf: Vec::new(),
        out: ServiceLatmDecodeOutput {
            sid: svc.sid,
            label: svc.label.clone(),
            latm_bytes: Vec::new(),
            metadata_jsonl: Vec::new(),
        },
    };

    emit_ensemble_if_ready(fic, &mut ctx.metadata_state, &mut ctx.out.metadata_jsonl);
    emit_service_if_needed(
        fic,
        svc.sid,
        &mut ctx.metadata_state,
        &mut ctx.out.metadata_jsonl,
    );
    emit_subchannel_if_changed(
        fic,
        scid,
        &mut ctx.metadata_state,
        &mut ctx.out.metadata_jsonl,
    );
    emit_time_with_mode_if_changed(
        fic,
        datetime_mode_from_option(options.datetime_format.as_ref()),
        &mut ctx.metadata_state,
        &mut ctx.out.metadata_jsonl,
    );
    ctx
}

impl LatmDecodeOutput {
    /// Render stdout bytes as a short hex preview for JavaScript logs.
    pub fn stdout_preview(&self, max_bytes: usize) -> String {
        format_stdout_hex_preview(&self.latm_bytes, max_bytes)
    }

    /// Render FD3 metadata as display-friendly JSONL text.
    pub fn fd3_preview(&self) -> String {
        format_fd3_display(&self.metadata_jsonl)
    }
}

fn clamp_preview_len(max_bytes: usize) -> usize {
    max_bytes.max(1)
}

fn bytes_to_hex_list(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format stdout bytes as a compact hex preview for JavaScript-facing wrappers.
pub fn format_stdout_hex_preview(bytes: &[u8], max_bytes: usize) -> String {
    if bytes.is_empty() {
        return "<empty>".to_string();
    }

    let preview_len = clamp_preview_len(max_bytes);
    let shown = bytes.len().min(preview_len);
    let prefix = bytes_to_hex_list(&bytes[..shown]);
    let remaining = bytes.len() - shown;

    if remaining == 0 {
        prefix
    } else {
        format!("{prefix} ... (+{remaining} bytes)")
    }
}

/// Join FD3 JSONL events for display in JavaScript wrappers.
pub fn format_fd3_display(lines: &[String]) -> String {
    if lines.is_empty() {
        "<empty>".to_string()
    } else {
        lines.join("\n")
    }
}

/// Decode ETI bytes to LATM + fd3-equivalent JSONL metadata in memory
/// using default options.
pub fn decode_eti_to_latm_memory(eti_bytes: &[u8]) -> Result<LatmDecodeOutput> {
    decode_eti_to_latm_memory_with_options(eti_bytes, &WasmLatmDecodeOptions::default())
}

/// Decode ETI bytes to LATM + fd3-equivalent JSONL metadata in memory.
///
/// Behavior follows CLI one-service-out with LATM output, while exposing data
/// as memory buffers suitable for WASM embedding.
pub fn decode_eti_to_latm_memory_with_options(
    eti_bytes: &[u8],
    options: &WasmLatmDecodeOptions,
) -> Result<LatmDecodeOutput> {
    if eti_bytes.len() < ETI_FRAME_SIZE {
        bail!("no complete ETI frame in input");
    }

    let mut out = LatmDecodeOutput::default();
    let mut fic = FicDecoder::new();
    let mut metadata_state = WasmMetadataState::default();
    let mut selection = WasmDecodeSelectionState::default();
    let mut latm_packer = LatmPacker::new();
    let mut pad_decoder = PadDecoder::new();
    let mut sf_work_buf: Vec<u8> = Vec::new();

    for raw in eti_bytes.chunks_exact(ETI_FRAME_SIZE) {
        let Ok(frame) = parse_frame(raw) else {
            continue;
        };

        if frame.ficf && !frame.fic.is_empty() {
            fic.process_fic(frame.fic);
            emit_ensemble_if_ready(&fic, &mut metadata_state, &mut out.metadata_jsonl);
            emit_time_if_changed(&fic, options, &mut metadata_state, &mut out.metadata_jsonl);
        }

        select_service_if_needed(
            &fic,
            &frame,
            options,
            &mut metadata_state,
            &mut out.metadata_jsonl,
            &mut selection,
        );

        let Some(scid) = selection.scid else {
            continue;
        };

        emit_subchannel_if_changed(&fic, scid, &mut metadata_state, &mut out.metadata_jsonl);

        let Some(cif_data) = extract_subchannel(&frame, scid) else {
            continue;
        };

        if selection.subch_buf.is_none() {
            init_subchannel_buffer_from_frame(&frame, scid, &mut selection);
            if selection.subch_buf.is_none() {
                continue;
            }
        }

        let Some(buf) = selection.subch_buf.as_mut() else {
            continue;
        };
        buf.push_cif(cif_data);

        while buf.len() >= buf.superframe_size() {
            let sf_size = buf.superframe_size();
            let Some(slice) = buf.try_peek_superframe_slice() else {
                break;
            };

            if sf_work_buf.len() != sf_size {
                sf_work_buf.resize(sf_size, 0);
            }
            sf_work_buf.copy_from_slice(slice);

            let result = process_superframe_inplace(&mut sf_work_buf);
            if !result.firecode_ok {
                buf.advance_one_cif();
                continue;
            }

            buf.consume_superframe();
            if result.rs_over_threshold {
                continue;
            }

            if let Some(fmt) = result.format.as_ref() {
                if let Some(selected_sid) = selection.sid {
                    process_pad_metadata_for_units(
                        &result.units,
                        &fic,
                        selected_sid,
                        scid,
                        options,
                        &mut metadata_state,
                        &mut out.metadata_jsonl,
                        &mut pad_decoder,
                    );
                }

                emit_audio_if_changed(
                    fmt,
                    selection.bitrate_kbps,
                    &mut metadata_state,
                    &mut out.metadata_jsonl,
                );
            }

            if let Some(fmt) = result.format.as_ref() {
                for au in result.units {
                    let packet = latm_packer.wrap(fmt, &au.data);
                    out.latm_bytes.extend_from_slice(packet);
                }
            }
        }
    }

    if selection.sid.is_none() {
        if options.sid.is_some() || options.label.is_some() {
            bail!("requested service not found in ETI input");
        }
        bail!("no service discovered in ETI input");
    }
    if out.latm_bytes.is_empty() {
        bail!("no decodable LATM output produced from ETI input");
    }

    Ok(out)
}

/// Decode ETI bytes to LATM + fd3-equivalent JSONL metadata for all DAB+ services.
pub fn decode_eti_to_latm_all_services_memory(
    eti_bytes: &[u8],
) -> Result<AllServicesLatmDecodeOutput> {
    decode_eti_to_latm_all_services_memory_with_options(
        eti_bytes,
        &WasmAllServicesDecodeOptions::default(),
    )
}

/// Decode ETI bytes to LATM + fd3-equivalent JSONL metadata for all DAB+ services
/// with explicit options.
pub fn decode_eti_to_latm_all_services_memory_with_options(
    eti_bytes: &[u8],
    options: &WasmAllServicesDecodeOptions,
) -> Result<AllServicesLatmDecodeOutput> {
    if eti_bytes.len() < ETI_FRAME_SIZE {
        bail!("no complete ETI frame in input");
    }

    let mut fic = FicDecoder::new();
    let mut contexts: BTreeMap<u32, WasmAllServicesContext> = BTreeMap::new();
    let datetime_mode = datetime_mode_from_option(options.datetime_format.as_ref());

    for raw in eti_bytes.chunks_exact(ETI_FRAME_SIZE) {
        let Ok(frame) = parse_frame(raw) else {
            continue;
        };

        if frame.ficf && !frame.fic.is_empty() {
            fic.process_fic(frame.fic);
        }

        for svc in &fic.services {
            if svc.components.is_empty() || contexts.contains_key(&svc.sid) {
                continue;
            }
            let scid = svc.components[0].subch_id;
            if !fic.is_dabplus(scid) {
                continue;
            }
            let Some(stc) = frame.stc.iter().find(|entry| entry.scid == scid) else {
                continue;
            };

            let ctx = make_all_services_context(&fic, svc, scid, stc.stl, options);
            contexts.insert(svc.sid, ctx);
        }

        if contexts.is_empty() {
            continue;
        }

        update_all_services_label_sync(&fic, &mut contexts);
        for ctx in contexts.values_mut() {
            emit_time_with_mode_if_changed(
                &fic,
                datetime_mode,
                &mut ctx.metadata_state,
                &mut ctx.out.metadata_jsonl,
            );

            let Some(cif_data) = extract_subchannel(&frame, ctx.scid) else {
                continue;
            };
            process_one_all_services_context(&fic, options, ctx, cif_data);
        }
    }

    if contexts.is_empty() {
        bail!("no DAB+ service discovered in ETI input");
    }

    let mut out = AllServicesLatmDecodeOutput {
        services: contexts.into_values().map(|ctx| ctx.out).collect(),
    };
    out.services.retain(|svc| !svc.latm_bytes.is_empty());
    if out.services.is_empty() {
        bail!("no decodable LATM output produced from ETI input");
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn decode_rejects_input_without_complete_eti_frame() {
        let err = decode_eti_to_latm_memory(&[0u8; 16]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no complete ETI frame"));
    }

    #[test]
    fn decode_fixture_produces_latm_and_fd3_events() {
        let eti = fs::read("test-local/multiplex-t.eti").expect("fixture ETI must exist");
        let out = decode_eti_to_latm_memory(&eti).expect("decode should succeed");

        assert!(!out.latm_bytes.is_empty());
        assert!(!out.metadata_jsonl.is_empty());
        assert!(out
            .metadata_jsonl
            .iter()
            .any(|line| line.contains("\"service\"")));
    }

    #[test]
    fn decode_with_sid_selects_requested_service() {
        let eti = fs::read("test-local/multiplex-t.eti").expect("fixture ETI must exist");
        let options = WasmLatmDecodeOptions {
            sid: Some("0xf201".to_string()),
            ..Default::default()
        };

        let out = decode_eti_to_latm_memory_with_options(&eti, &options)
            .expect("decode with sid should succeed");
        let service_line = out
            .metadata_jsonl
            .iter()
            .find(|line| line.contains("\"service\""))
            .expect("service metadata must be present");

        assert!(service_line.contains("0xf201"));
        assert!(!out.latm_bytes.is_empty());
    }

    #[test]
    fn decode_with_label_selects_requested_service() {
        let eti = fs::read("test-local/multiplex-t.eti").expect("fixture ETI must exist");
        let options = WasmLatmDecodeOptions {
            label: Some("FRANCE INTER".to_string()),
            ..Default::default()
        };

        let out = decode_eti_to_latm_memory_with_options(&eti, &options)
            .expect("decode with label should succeed");
        let service_line = out
            .metadata_jsonl
            .iter()
            .find(|line| line.contains("\"service\""))
            .expect("service metadata must be present");

        assert!(service_line.contains("0xf201"));
    }

    #[test]
    fn decode_with_unknown_sid_fails() {
        let eti = fs::read("test-local/multiplex-t.eti").expect("fixture ETI must exist");
        let options = WasmLatmDecodeOptions {
            sid: Some("0xffff".to_string()),
            ..Default::default()
        };

        let err = decode_eti_to_latm_memory_with_options(&eti, &options).unwrap_err();
        assert!(err.to_string().contains("requested service not found"));
    }

    #[test]
    fn decode_all_services_produces_multiple_service_outputs() {
        let eti = fs::read("test-local/multiplex-t.eti").expect("fixture ETI must exist");
        let out = decode_eti_to_latm_all_services_memory(&eti)
            .expect("all-services decode should succeed");

        assert!(out.services.len() > 1);
        assert!(out.services.iter().all(|svc| !svc.latm_bytes.is_empty()));
        assert!(out.services.iter().all(|svc| svc
            .metadata_jsonl
            .iter()
            .any(|line| line.contains("\"service\""))));
    }

    #[test]
    fn stdout_hex_preview_formats_and_truncates() {
        let bytes = [0x00, 0x7f, 0x80, 0xff, 0x11];
        let preview = format_stdout_hex_preview(&bytes, 3);
        assert_eq!(preview, "00 7f 80 ... (+2 bytes)");
    }

    #[test]
    fn stdout_hex_preview_handles_empty_input() {
        let preview = format_stdout_hex_preview(&[], 8);
        assert_eq!(preview, "<empty>");
    }

    #[test]
    fn fd3_display_joins_lines_without_trailing_newline() {
        let lines = vec![
            r#"{"service":{"sid":"0xf2f8"}}"#.to_string(),
            r#"{"dl":"Artist - Title"}"#.to_string(),
        ];
        let fd3 = format_fd3_display(&lines);
        assert_eq!(
            fd3,
            "{\"service\":{\"sid\":\"0xf2f8\"}}\n{\"dl\":\"Artist - Title\"}"
        );
    }

    #[test]
    fn fd3_display_handles_no_events() {
        let fd3 = format_fd3_display(&[]);
        assert_eq!(fd3, "<empty>");
    }
}
