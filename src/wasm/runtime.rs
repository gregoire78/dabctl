//! WebAssembly-oriented runtime scaffolding.
//!
//! This module is intentionally minimal and keeps the native CLI behavior unchanged.
//! It defines the memory-based decode core used by the wasm-bindgen layer
//! implemented in `src/wasm/bindings.rs`.

#![cfg_attr(feature = "wasm-runtime", allow(dead_code))]

use anyhow::{bail, Result};
use serde_json::json;

use crate::cli::DateTimeFormat;
use crate::dablin::audio::adts::AdtsPacker;
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

#[cfg(feature = "wasm-faad2")]
use crate::cli::AacGap;
#[cfg(feature = "wasm-faad2")]
use crate::dablin::audio::AacDecoder;

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
    slide_base64: bool,
    dedup_pad: bool,
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
            let is_dup = dedup_pad && metadata_state.last_dl.as_deref() == Some(dl.as_str());
            if !is_dup {
                push_jsonl_event(metadata_jsonl, json!({"dl": dl}));
                metadata_state.last_dl = Some(dl);
            }
        }

        if let Some(slide) = pad_events.slide {
            let slide_hash = hash_bytes(&slide.data);
            let is_dup_slide = dedup_pad && metadata_state.last_slide_hash == Some(slide_hash);
            if !is_dup_slide && slide_base64 {
                let data_base64 = encode_slide_base64(&slide.data, slide_base64);
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

/// Per-service metadata-only output in all-services decode mode.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ServiceMetadataDecodeOutput {
    pub sid: u32,
    pub label: Option<String>,
    pub metadata_jsonl: Vec<String>,
}

/// Output container for metadata-only all-services memory decode mode.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AllServicesMetadataDecodeOutput {
    pub services: Vec<ServiceMetadataDecodeOutput>,
}

// ── ADTS output types ──────────────────────────────────────────────────────

/// Output container for a memory-based ADTS decode call.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AdtsDecodeOutput {
    /// Concatenated ADTS-framed bytes.
    pub adts_bytes: Vec<u8>,
    /// Metadata events as JSONL lines.
    pub metadata_jsonl: Vec<String>,
}

/// Per-service memory output in all-services ADTS decode mode.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ServiceAdtsDecodeOutput {
    pub sid: u32,
    pub label: Option<String>,
    pub adts_bytes: Vec<u8>,
    pub metadata_jsonl: Vec<String>,
}

/// Output container for all-services ADTS memory decode mode.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AllServicesAdtsDecodeOutput {
    pub services: Vec<ServiceAdtsDecodeOutput>,
}

// ── FAAD PCM output type ──────────────────────────────────────────────────

/// Output container for a memory-based FAAD (raw PCM) decode call.
/// Emits s16le stereo 48 kHz PCM, identical to the native CLI stdout output.
#[cfg(feature = "wasm-faad2")]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FaadDecodeOutput {
    /// Raw s16le PCM bytes (stdout-equivalent).
    pub pcm_bytes: Vec<u8>,
    /// Metadata events as JSONL lines (fd3-equivalent).
    pub metadata_jsonl: Vec<String>,
}

/// Per-service memory output in all-services FAAD decode mode.
#[cfg(feature = "wasm-faad2")]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ServiceFaadDecodeOutput {
    pub sid: u32,
    pub label: Option<String>,
    pub pcm_bytes: Vec<u8>,
    pub metadata_jsonl: Vec<String>,
}

/// Output container for all-services FAAD memory decode mode.
#[cfg(feature = "wasm-faad2")]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AllServicesFaadDecodeOutput {
    pub services: Vec<ServiceFaadDecodeOutput>,
}

struct WasmAllServicesContext {
    core: AllServicesContextCore<ServiceLatmDecodeOutput>,
    latm_packer: LatmPacker,
}

struct WasmAllServicesAdtsContext {
    core: AllServicesContextCore<ServiceAdtsDecodeOutput>,
    adts_packer: AdtsPacker,
}

struct WasmAllServicesMetadataContext {
    core: AllServicesContextCore<ServiceMetadataDecodeOutput>,
    emitted_any_audio: bool,
}

#[cfg(feature = "wasm-faad2")]
struct WasmAllServicesFaadContext {
    core: AllServicesContextCore<ServiceFaadDecodeOutput>,
    aac_decoder: Option<AacDecoder>,
}

struct AllServicesContextCore<O> {
    sid: u32,
    scid: u8,
    bitrate_kbps: u32,
    metadata_state: WasmMetadataState,
    subch_buf: SubchannelBuffer,
    pad_decoder: PadDecoder,
    sf_work_buf: Vec<u8>,
    out: O,
}

trait AllServicesLabelSyncContext {
    fn sid(&self) -> u32;
    fn scid(&self) -> u8;
    fn label(&self) -> Option<&str>;
    fn set_label(&mut self, label: String);
    fn metadata_parts_mut(&mut self) -> (&mut WasmMetadataState, &mut Vec<String>);
}

impl AllServicesLabelSyncContext for WasmAllServicesContext {
    fn sid(&self) -> u32 {
        self.core.sid
    }

    fn scid(&self) -> u8 {
        self.core.scid
    }

    fn label(&self) -> Option<&str> {
        self.core.out.label.as_deref()
    }

    fn set_label(&mut self, label: String) {
        self.core.out.label = Some(label);
    }

    fn metadata_parts_mut(&mut self) -> (&mut WasmMetadataState, &mut Vec<String>) {
        (
            &mut self.core.metadata_state,
            &mut self.core.out.metadata_jsonl,
        )
    }
}

impl AllServicesLabelSyncContext for WasmAllServicesAdtsContext {
    fn sid(&self) -> u32 {
        self.core.sid
    }

    fn scid(&self) -> u8 {
        self.core.scid
    }

    fn label(&self) -> Option<&str> {
        self.core.out.label.as_deref()
    }

    fn set_label(&mut self, label: String) {
        self.core.out.label = Some(label);
    }

    fn metadata_parts_mut(&mut self) -> (&mut WasmMetadataState, &mut Vec<String>) {
        (
            &mut self.core.metadata_state,
            &mut self.core.out.metadata_jsonl,
        )
    }
}

impl AllServicesLabelSyncContext for WasmAllServicesMetadataContext {
    fn sid(&self) -> u32 {
        self.core.sid
    }

    fn scid(&self) -> u8 {
        self.core.scid
    }

    fn label(&self) -> Option<&str> {
        self.core.out.label.as_deref()
    }

    fn set_label(&mut self, label: String) {
        self.core.out.label = Some(label);
    }

    fn metadata_parts_mut(&mut self) -> (&mut WasmMetadataState, &mut Vec<String>) {
        (
            &mut self.core.metadata_state,
            &mut self.core.out.metadata_jsonl,
        )
    }
}

#[cfg(feature = "wasm-faad2")]
impl AllServicesLabelSyncContext for WasmAllServicesFaadContext {
    fn sid(&self) -> u32 {
        self.core.sid
    }

    fn scid(&self) -> u8 {
        self.core.scid
    }

    fn label(&self) -> Option<&str> {
        self.core.out.label.as_deref()
    }

    fn set_label(&mut self, label: String) {
        self.core.out.label = Some(label);
    }

    fn metadata_parts_mut(&mut self) -> (&mut WasmMetadataState, &mut Vec<String>) {
        (
            &mut self.core.metadata_state,
            &mut self.core.out.metadata_jsonl,
        )
    }
}

fn update_all_services_label_sync_generic<T: AllServicesLabelSyncContext>(
    fic: &FicDecoder,
    contexts: &mut BTreeMap<u32, T>,
) {
    let labels_by_sid: BTreeMap<u32, &str> = fic
        .services
        .iter()
        .filter_map(|s| s.label.as_deref().map(|label| (s.sid, label)))
        .collect();

    for ctx in contexts.values_mut() {
        {
            let (metadata_state, metadata_jsonl) = ctx.metadata_parts_mut();
            emit_ensemble_if_ready(fic, metadata_state, metadata_jsonl);
        }

        if let Some(label) = labels_by_sid.get(&ctx.sid()) {
            if ctx.label() != Some(label) {
                ctx.set_label((*label).to_owned());
                let sid = ctx.sid();
                let (metadata_state, metadata_jsonl) = ctx.metadata_parts_mut();
                emit_service_if_needed(fic, sid, metadata_state, metadata_jsonl);
            }
        }

        let scid = ctx.scid();
        let (metadata_state, metadata_jsonl) = ctx.metadata_parts_mut();
        emit_subchannel_if_changed(fic, scid, metadata_state, metadata_jsonl);
    }
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
    update_all_services_label_sync_generic(fic, contexts);
}

fn update_all_services_adts_label_sync(
    fic: &FicDecoder,
    contexts: &mut BTreeMap<u32, WasmAllServicesAdtsContext>,
) {
    update_all_services_label_sync_generic(fic, contexts);
}

fn update_all_services_metadata_label_sync(
    fic: &FicDecoder,
    contexts: &mut BTreeMap<u32, WasmAllServicesMetadataContext>,
) {
    update_all_services_label_sync_generic(fic, contexts);
}

#[cfg(feature = "wasm-faad2")]
fn update_all_services_faad_label_sync(
    fic: &FicDecoder,
    contexts: &mut BTreeMap<u32, WasmAllServicesFaadContext>,
) {
    update_all_services_label_sync_generic(fic, contexts);
}

fn finalize_latm_all_services_output(
    contexts: BTreeMap<u32, WasmAllServicesContext>,
) -> Result<AllServicesLatmDecodeOutput> {
    let mut out = AllServicesLatmDecodeOutput {
        services: contexts.into_values().map(|ctx| ctx.core.out).collect(),
    };
    out.services.retain(|svc| !svc.latm_bytes.is_empty());
    if out.services.is_empty() {
        bail!("no decodable LATM output produced from ETI input");
    }
    Ok(out)
}

fn finalize_adts_all_services_output(
    contexts: BTreeMap<u32, WasmAllServicesAdtsContext>,
) -> Result<AllServicesAdtsDecodeOutput> {
    let mut out = AllServicesAdtsDecodeOutput {
        services: contexts.into_values().map(|ctx| ctx.core.out).collect(),
    };
    out.services.retain(|svc| !svc.adts_bytes.is_empty());
    if out.services.is_empty() {
        bail!("no decodable ADTS output produced from ETI input");
    }
    Ok(out)
}

fn finalize_metadata_all_services_output(
    contexts: BTreeMap<u32, WasmAllServicesMetadataContext>,
) -> Result<AllServicesMetadataDecodeOutput> {
    let mut out = AllServicesMetadataDecodeOutput {
        services: contexts
            .into_values()
            .filter(|ctx| ctx.emitted_any_audio)
            .map(|ctx| ctx.core.out)
            .collect(),
    };
    out.services.retain(|svc| !svc.metadata_jsonl.is_empty());
    if out.services.is_empty() {
        bail!("no decodable metadata output produced from ETI input");
    }
    Ok(out)
}

#[cfg(feature = "wasm-faad2")]
fn finalize_faad_all_services_output(
    contexts: BTreeMap<u32, WasmAllServicesFaadContext>,
) -> Result<AllServicesFaadDecodeOutput> {
    let mut out = AllServicesFaadDecodeOutput {
        services: contexts.into_values().map(|ctx| ctx.core.out).collect(),
    };
    out.services.retain(|svc| !svc.pcm_bytes.is_empty());
    if out.services.is_empty() {
        bail!("no decodable PCM output produced from ETI input");
    }
    Ok(out)
}

fn decode_eti_to_all_services_memory_with_options_generic<C, O>(
    eti_bytes: &[u8],
    options: &WasmAllServicesDecodeOptions,
    mut make_context: impl FnMut(&FicDecoder, &ServiceInfo, u8, u16, &WasmAllServicesDecodeOptions) -> C,
    mut update_labels: impl FnMut(&FicDecoder, &mut BTreeMap<u32, C>),
    mut process_context: impl FnMut(&FicDecoder, &WasmAllServicesDecodeOptions, &mut C, &[u8]),
    mut context_scid: impl FnMut(&C) -> u8,
    mut emit_time_for_context: impl FnMut(&FicDecoder, Option<DateTimeMode<'_>>, &mut C),
    mut finalize: impl FnMut(BTreeMap<u32, C>) -> Result<O>,
) -> Result<O> {
    if eti_bytes.len() < ETI_FRAME_SIZE {
        bail!("no complete ETI frame in input");
    }

    let mut fic = FicDecoder::new();
    let mut contexts: BTreeMap<u32, C> = BTreeMap::new();
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

            let ctx = make_context(&fic, svc, scid, stc.stl, options);
            contexts.insert(svc.sid, ctx);
        }

        if contexts.is_empty() {
            continue;
        }

        update_labels(&fic, &mut contexts);
        for ctx in contexts.values_mut() {
            emit_time_for_context(&fic, datetime_mode, ctx);

            let scid = context_scid(ctx);
            let Some(cif_data) = extract_subchannel(&frame, scid) else {
                continue;
            };
            process_context(&fic, options, ctx, cif_data);
        }
    }

    if contexts.is_empty() {
        bail!("no DAB+ service discovered in ETI input");
    }

    finalize(contexts)
}

fn process_one_all_services_context(
    fic: &FicDecoder,
    options: &WasmAllServicesDecodeOptions,
    ctx: &mut WasmAllServicesContext,
    cif_data: &[u8],
) {
    let latm_packer = &mut ctx.latm_packer;
    let latm_bytes = &mut ctx.core.out.latm_bytes;

    process_one_all_services_context_generic(
        fic,
        options,
        ctx.core.sid,
        ctx.core.scid,
        ctx.core.bitrate_kbps,
        &mut ctx.core.subch_buf,
        &mut ctx.core.sf_work_buf,
        &mut ctx.core.metadata_state,
        &mut ctx.core.out.metadata_jsonl,
        &mut ctx.core.pad_decoder,
        cif_data,
        move |fmt, units| {
            for au in units {
                let packet = latm_packer.wrap(fmt, &au.data);
                latm_bytes.extend_from_slice(packet);
            }
        },
    );
}

fn process_one_all_services_adts_context(
    fic: &FicDecoder,
    options: &WasmAllServicesDecodeOptions,
    ctx: &mut WasmAllServicesAdtsContext,
    cif_data: &[u8],
) {
    let adts_packer = &mut ctx.adts_packer;
    let adts_bytes = &mut ctx.core.out.adts_bytes;

    process_one_all_services_context_generic(
        fic,
        options,
        ctx.core.sid,
        ctx.core.scid,
        ctx.core.bitrate_kbps,
        &mut ctx.core.subch_buf,
        &mut ctx.core.sf_work_buf,
        &mut ctx.core.metadata_state,
        &mut ctx.core.out.metadata_jsonl,
        &mut ctx.core.pad_decoder,
        cif_data,
        move |fmt, units| {
            for au in units {
                let frame = adts_packer.wrap(fmt, &au.data);
                adts_bytes.extend_from_slice(&frame);
            }
        },
    );
}

fn process_one_all_services_metadata_context(
    fic: &FicDecoder,
    options: &WasmAllServicesDecodeOptions,
    ctx: &mut WasmAllServicesMetadataContext,
    cif_data: &[u8],
) {
    let emitted_any_audio = &mut ctx.emitted_any_audio;
    process_one_all_services_context_generic(
        fic,
        options,
        ctx.core.sid,
        ctx.core.scid,
        ctx.core.bitrate_kbps,
        &mut ctx.core.subch_buf,
        &mut ctx.core.sf_work_buf,
        &mut ctx.core.metadata_state,
        &mut ctx.core.out.metadata_jsonl,
        &mut ctx.core.pad_decoder,
        cif_data,
        move |_fmt, units| {
            if !units.is_empty() {
                *emitted_any_audio = true;
            }
        },
    );
}

#[cfg(feature = "wasm-faad2")]
fn process_one_all_services_faad_context(
    fic: &FicDecoder,
    options: &WasmAllServicesDecodeOptions,
    ctx: &mut WasmAllServicesFaadContext,
    cif_data: &[u8],
) {
    let aac_decoder = &mut ctx.aac_decoder;
    let pcm_bytes = &mut ctx.core.out.pcm_bytes;

    process_one_all_services_context_generic(
        fic,
        options,
        ctx.core.sid,
        ctx.core.scid,
        ctx.core.bitrate_kbps,
        &mut ctx.core.subch_buf,
        &mut ctx.core.sf_work_buf,
        &mut ctx.core.metadata_state,
        &mut ctx.core.out.metadata_jsonl,
        &mut ctx.core.pad_decoder,
        cif_data,
        move |fmt, units| {
            if aac_decoder.is_none() {
                *aac_decoder = AacDecoder::new_faad2(AacGap::Freeze);
            }

            if let Some(ref mut dec) = aac_decoder {
                dec.init_format(fmt);
                for au in units {
                    if let Some(pcm) = dec.decode(&au) {
                        append_pcm_samples_le(pcm_bytes, &pcm);
                    }
                }
            }
        },
    );
}

fn process_one_all_services_context_generic(
    fic: &FicDecoder,
    options: &WasmAllServicesDecodeOptions,
    sid: u32,
    scid: u8,
    bitrate_kbps: u32,
    subch_buf: &mut SubchannelBuffer,
    sf_work_buf: &mut Vec<u8>,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
    pad_decoder: &mut PadDecoder,
    cif_data: &[u8],
    mut emit_units: impl FnMut(&SuperframeFormat, Vec<crate::dablin::dabplus::AudioUnit>),
) {
    subch_buf.push_cif(cif_data);

    while subch_buf.len() >= subch_buf.superframe_size() {
        let sf_size = subch_buf.superframe_size();
        let Some(slice) = subch_buf.try_peek_superframe_slice() else {
            break;
        };

        if sf_work_buf.len() != sf_size {
            sf_work_buf.resize(sf_size, 0);
        }
        sf_work_buf.copy_from_slice(slice);

        let result = process_superframe_inplace(sf_work_buf);
        if !result.firecode_ok {
            subch_buf.advance_one_cif();
            continue;
        }

        subch_buf.consume_superframe();
        if result.rs_over_threshold {
            continue;
        }

        if let Some(fmt) = result.format.as_ref() {
            process_pad_metadata_for_units(
                &result.units,
                fic,
                sid,
                scid,
                options.slide_base64,
                options.dedup_pad,
                metadata_state,
                metadata_jsonl,
                pad_decoder,
            );

            emit_audio_if_changed(fmt, Some(bitrate_kbps), metadata_state, metadata_jsonl);
            emit_units(fmt, result.units);
        }
    }
}

fn append_pcm_samples_le(dst: &mut Vec<u8>, pcm: &[i16]) {
    #[cfg(target_endian = "little")]
    {
        let byte_len = std::mem::size_of_val(pcm);
        dst.reserve(byte_len);
        // Safe because i16 has no invalid bit patterns and we only reinterpret as bytes.
        let bytes = unsafe { std::slice::from_raw_parts(pcm.as_ptr() as *const u8, byte_len) };
        dst.extend_from_slice(bytes);
    }

    #[cfg(not(target_endian = "little"))]
    {
        dst.reserve(pcm.len() * 2);
        for sample in pcm {
            dst.extend_from_slice(&sample.to_le_bytes());
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
        core: AllServicesContextCore {
            sid: svc.sid,
            scid,
            bitrate_kbps,
            metadata_state: WasmMetadataState::default(),
            subch_buf: SubchannelBuffer::new(scid, stl),
            pad_decoder: PadDecoder::new(),
            sf_work_buf: Vec::new(),
            out: ServiceLatmDecodeOutput {
                sid: svc.sid,
                label: svc.label.clone(),
                latm_bytes: Vec::new(),
                metadata_jsonl: Vec::new(),
            },
        },
        latm_packer: LatmPacker::new(),
    };

    emit_initial_all_services_metadata(
        fic,
        svc.sid,
        scid,
        options,
        &mut ctx.core.metadata_state,
        &mut ctx.core.out.metadata_jsonl,
    );
    ctx
}

fn make_all_services_adts_context(
    fic: &FicDecoder,
    svc: &ServiceInfo,
    scid: u8,
    stl: u16,
    options: &WasmAllServicesDecodeOptions,
) -> WasmAllServicesAdtsContext {
    let bitrate_kbps = (u32::from(stl) * 64) / 24;
    let mut ctx = WasmAllServicesAdtsContext {
        core: AllServicesContextCore {
            sid: svc.sid,
            scid,
            bitrate_kbps,
            metadata_state: WasmMetadataState::default(),
            subch_buf: SubchannelBuffer::new(scid, stl),
            pad_decoder: PadDecoder::new(),
            sf_work_buf: Vec::new(),
            out: ServiceAdtsDecodeOutput {
                sid: svc.sid,
                label: svc.label.clone(),
                adts_bytes: Vec::new(),
                metadata_jsonl: Vec::new(),
            },
        },
        adts_packer: AdtsPacker::new(),
    };

    emit_initial_all_services_metadata(
        fic,
        svc.sid,
        scid,
        options,
        &mut ctx.core.metadata_state,
        &mut ctx.core.out.metadata_jsonl,
    );
    ctx
}

fn make_all_services_metadata_context(
    fic: &FicDecoder,
    svc: &ServiceInfo,
    scid: u8,
    stl: u16,
    options: &WasmAllServicesDecodeOptions,
) -> WasmAllServicesMetadataContext {
    let bitrate_kbps = (u32::from(stl) * 64) / 24;
    let mut ctx = WasmAllServicesMetadataContext {
        core: AllServicesContextCore {
            sid: svc.sid,
            scid,
            bitrate_kbps,
            metadata_state: WasmMetadataState::default(),
            subch_buf: SubchannelBuffer::new(scid, stl),
            pad_decoder: PadDecoder::new(),
            sf_work_buf: Vec::new(),
            out: ServiceMetadataDecodeOutput {
                sid: svc.sid,
                label: svc.label.clone(),
                metadata_jsonl: Vec::new(),
            },
        },
        emitted_any_audio: false,
    };

    emit_initial_all_services_metadata(
        fic,
        svc.sid,
        scid,
        options,
        &mut ctx.core.metadata_state,
        &mut ctx.core.out.metadata_jsonl,
    );
    ctx
}

#[cfg(feature = "wasm-faad2")]
fn make_all_services_faad_context(
    fic: &FicDecoder,
    svc: &ServiceInfo,
    scid: u8,
    stl: u16,
    options: &WasmAllServicesDecodeOptions,
) -> WasmAllServicesFaadContext {
    let bitrate_kbps = (u32::from(stl) * 64) / 24;
    let mut ctx = WasmAllServicesFaadContext {
        core: AllServicesContextCore {
            sid: svc.sid,
            scid,
            bitrate_kbps,
            metadata_state: WasmMetadataState::default(),
            subch_buf: SubchannelBuffer::new(scid, stl),
            pad_decoder: PadDecoder::new(),
            sf_work_buf: Vec::new(),
            out: ServiceFaadDecodeOutput {
                sid: svc.sid,
                label: svc.label.clone(),
                pcm_bytes: Vec::new(),
                metadata_jsonl: Vec::new(),
            },
        },
        aac_decoder: None,
    };

    emit_initial_all_services_metadata(
        fic,
        svc.sid,
        scid,
        options,
        &mut ctx.core.metadata_state,
        &mut ctx.core.out.metadata_jsonl,
    );
    ctx
}

fn emit_initial_all_services_metadata(
    fic: &FicDecoder,
    sid: u32,
    scid: u8,
    options: &WasmAllServicesDecodeOptions,
    metadata_state: &mut WasmMetadataState,
    metadata_jsonl: &mut Vec<String>,
) {
    emit_ensemble_if_ready(fic, metadata_state, metadata_jsonl);
    emit_service_if_needed(fic, sid, metadata_state, metadata_jsonl);
    emit_subchannel_if_changed(fic, scid, metadata_state, metadata_jsonl);
    emit_time_with_mode_if_changed(
        fic,
        datetime_mode_from_option(options.datetime_format.as_ref()),
        metadata_state,
        metadata_jsonl,
    );
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
                        options.slide_base64,
                        options.dedup_pad,
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
    decode_eti_to_all_services_memory_with_options_generic(
        eti_bytes,
        options,
        make_all_services_context,
        update_all_services_label_sync,
        process_one_all_services_context,
        |ctx| ctx.core.scid,
        |fic, datetime_mode, ctx| {
            emit_time_with_mode_if_changed(
                fic,
                datetime_mode,
                &mut ctx.core.metadata_state,
                &mut ctx.core.out.metadata_jsonl,
            );
        },
        finalize_latm_all_services_output,
    )
}

/// Decode ETI bytes to fd3-equivalent JSONL metadata for all DAB+ services.
pub fn decode_eti_to_all_services_memory(
    eti_bytes: &[u8],
) -> Result<AllServicesMetadataDecodeOutput> {
    decode_eti_to_all_services_memory_with_options(
        eti_bytes,
        &WasmAllServicesDecodeOptions::default(),
    )
}

/// Decode ETI bytes to fd3-equivalent JSONL metadata for all DAB+ services
/// with explicit options.
pub fn decode_eti_to_all_services_memory_with_options(
    eti_bytes: &[u8],
    options: &WasmAllServicesDecodeOptions,
) -> Result<AllServicesMetadataDecodeOutput> {
    decode_eti_to_all_services_memory_with_options_generic(
        eti_bytes,
        options,
        make_all_services_metadata_context,
        update_all_services_metadata_label_sync,
        process_one_all_services_metadata_context,
        |ctx| ctx.core.scid,
        |fic, datetime_mode, ctx| {
            emit_time_with_mode_if_changed(
                fic,
                datetime_mode,
                &mut ctx.core.metadata_state,
                &mut ctx.core.out.metadata_jsonl,
            );
        },
        finalize_metadata_all_services_output,
    )
}

// ── ADTS single-service decode ────────────────────────────────────────────

/// Decode ETI bytes to ADTS + fd3-equivalent JSONL metadata in memory
/// using default options.
pub fn decode_eti_to_adts_memory(eti_bytes: &[u8]) -> Result<AdtsDecodeOutput> {
    decode_eti_to_adts_memory_with_options(eti_bytes, &WasmLatmDecodeOptions::default())
}

/// Decode ETI bytes to ADTS + fd3-equivalent JSONL metadata in memory.
///
/// Same pipeline as LATM but each AAC access unit is wrapped in an ADTS header.
pub fn decode_eti_to_adts_memory_with_options(
    eti_bytes: &[u8],
    options: &WasmLatmDecodeOptions,
) -> Result<AdtsDecodeOutput> {
    if eti_bytes.len() < ETI_FRAME_SIZE {
        bail!("no complete ETI frame in input");
    }

    let mut out = AdtsDecodeOutput::default();
    let mut fic = FicDecoder::new();
    let mut metadata_state = WasmMetadataState::default();
    let mut selection = WasmDecodeSelectionState::default();
    let adts_packer = AdtsPacker::new();
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
                        options.slide_base64,
                        options.dedup_pad,
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

                for au in result.units {
                    let adts_frame = adts_packer.wrap(fmt, &au.data);
                    out.adts_bytes.extend_from_slice(&adts_frame);
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
    if out.adts_bytes.is_empty() {
        bail!("no decodable ADTS output produced from ETI input");
    }

    Ok(out)
}

// ── ADTS all-services decode ──────────────────────────────────────────────

/// Decode ETI bytes to ADTS + fd3-equivalent JSONL metadata for all DAB+ services.
pub fn decode_eti_to_adts_all_services_memory(
    eti_bytes: &[u8],
) -> Result<AllServicesAdtsDecodeOutput> {
    decode_eti_to_adts_all_services_memory_with_options(
        eti_bytes,
        &WasmAllServicesDecodeOptions::default(),
    )
}

/// Decode ETI bytes to ADTS + fd3-equivalent JSONL metadata for all DAB+ services
/// with explicit options.
pub fn decode_eti_to_adts_all_services_memory_with_options(
    eti_bytes: &[u8],
    options: &WasmAllServicesDecodeOptions,
) -> Result<AllServicesAdtsDecodeOutput> {
    decode_eti_to_all_services_memory_with_options_generic(
        eti_bytes,
        options,
        make_all_services_adts_context,
        update_all_services_adts_label_sync,
        process_one_all_services_adts_context,
        |ctx| ctx.core.scid,
        |fic, datetime_mode, ctx| {
            emit_time_with_mode_if_changed(
                fic,
                datetime_mode,
                &mut ctx.core.metadata_state,
                &mut ctx.core.out.metadata_jsonl,
            );
        },
        finalize_adts_all_services_output,
    )
}

// ── FAAD single-service decode ────────────────────────────────────────────

/// Decode ETI bytes to raw s16le PCM using the faad2 AAC decoder.
/// Uses default options (first available service, no metadata extras).
#[cfg(feature = "wasm-faad2")]
pub fn decode_eti_to_faad_memory(eti_bytes: &[u8]) -> Result<FaadDecodeOutput> {
    decode_eti_to_faad_memory_with_options(eti_bytes, &WasmLatmDecodeOptions::default())
}

/// Decode ETI bytes to raw s16le PCM using the faad2 AAC decoder.
///
/// Output mirrors the native `one-service-out --audio-out pcm` stdout contract:
/// s16le, 48 kHz, stereo, no framing.
#[cfg(feature = "wasm-faad2")]
pub fn decode_eti_to_faad_memory_with_options(
    eti_bytes: &[u8],
    options: &WasmLatmDecodeOptions,
) -> Result<FaadDecodeOutput> {
    if eti_bytes.len() < ETI_FRAME_SIZE {
        bail!("no complete ETI frame in input");
    }

    let mut out = FaadDecodeOutput::default();
    let mut fic = FicDecoder::new();
    let mut metadata_state = WasmMetadataState::default();
    let mut selection = WasmDecodeSelectionState::default();
    let mut aac_decoder: Option<AacDecoder> = None;
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
                        options.slide_base64,
                        options.dedup_pad,
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

                if aac_decoder.is_none() {
                    aac_decoder = AacDecoder::new_faad2(AacGap::Freeze);
                }
                if let Some(ref mut dec) = aac_decoder {
                    dec.init_format(fmt);
                    for au in result.units {
                        if let Some(pcm) = dec.decode(&au) {
                            append_pcm_samples_le(&mut out.pcm_bytes, &pcm);
                        }
                    }
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
    if out.pcm_bytes.is_empty() {
        bail!("no decodable PCM output produced from ETI input");
    }

    Ok(out)
}

/// Decode ETI bytes to raw s16le PCM + fd3-equivalent JSONL metadata for all DAB+ services.
#[cfg(feature = "wasm-faad2")]
pub fn decode_eti_to_faad_all_services_memory(
    eti_bytes: &[u8],
) -> Result<AllServicesFaadDecodeOutput> {
    decode_eti_to_faad_all_services_memory_with_options(
        eti_bytes,
        &WasmAllServicesDecodeOptions::default(),
    )
}

/// Decode ETI bytes to raw s16le PCM + fd3-equivalent JSONL metadata for all DAB+ services
/// with explicit options.
#[cfg(feature = "wasm-faad2")]
pub fn decode_eti_to_faad_all_services_memory_with_options(
    eti_bytes: &[u8],
    options: &WasmAllServicesDecodeOptions,
) -> Result<AllServicesFaadDecodeOutput> {
    decode_eti_to_all_services_memory_with_options_generic(
        eti_bytes,
        options,
        make_all_services_faad_context,
        update_all_services_faad_label_sync,
        process_one_all_services_faad_context,
        |ctx| ctx.core.scid,
        |fic, datetime_mode, ctx| {
            emit_time_with_mode_if_changed(
                fic,
                datetime_mode,
                &mut ctx.core.metadata_state,
                &mut ctx.core.out.metadata_jsonl,
            );
        },
        finalize_faad_all_services_output,
    )
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
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
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
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
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
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
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
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
        let options = WasmLatmDecodeOptions {
            sid: Some("0xffff".to_string()),
            ..Default::default()
        };

        let err = decode_eti_to_latm_memory_with_options(&eti, &options).unwrap_err();
        assert!(err.to_string().contains("requested service not found"));
    }

    #[test]
    fn decode_all_services_produces_multiple_service_outputs() {
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
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

    // ── ADTS tests ─────────────────────────────────────────────────────────

    #[test]
    fn decode_adts_fixture_produces_adts_and_fd3_events() {
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
        let out = decode_eti_to_adts_memory(&eti).expect("decode should succeed");

        assert!(!out.adts_bytes.is_empty());
        assert!(!out.metadata_jsonl.is_empty());
        assert!(out
            .metadata_jsonl
            .iter()
            .any(|line| line.contains("\"service\"")));
    }

    #[test]
    fn decode_adts_with_sid_selects_requested_service() {
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
        let options = WasmLatmDecodeOptions {
            sid: Some("0xf201".to_string()),
            ..Default::default()
        };

        let out = decode_eti_to_adts_memory_with_options(&eti, &options)
            .expect("decode with sid should succeed");
        let service_line = out
            .metadata_jsonl
            .iter()
            .find(|line| line.contains("\"service\""))
            .expect("service metadata must be present");

        assert!(service_line.contains("0xf201"));
        assert!(!out.adts_bytes.is_empty());
    }

    #[test]
    fn decode_adts_with_label_selects_requested_service() {
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
        let options = WasmLatmDecodeOptions {
            label: Some("FRANCE INTER".to_string()),
            ..Default::default()
        };

        let out = decode_eti_to_adts_memory_with_options(&eti, &options)
            .expect("decode with label should succeed");
        let service_line = out
            .metadata_jsonl
            .iter()
            .find(|line| line.contains("\"service\""))
            .expect("service metadata must be present");

        assert!(service_line.contains("0xf201"));
    }

    #[test]
    fn decode_adts_with_unknown_sid_fails() {
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
        let options = WasmLatmDecodeOptions {
            sid: Some("0xffff".to_string()),
            ..Default::default()
        };

        let err = decode_eti_to_adts_memory_with_options(&eti, &options).unwrap_err();
        assert!(err.to_string().contains("requested service not found"));
    }

    #[test]
    fn decode_adts_all_services_produces_multiple_service_outputs() {
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
        let out = decode_eti_to_adts_all_services_memory(&eti)
            .expect("all-services decode should succeed");

        assert!(out.services.len() > 1);
        assert!(out.services.iter().all(|svc| !svc.adts_bytes.is_empty()));
        assert!(out.services.iter().all(|svc| svc
            .metadata_jsonl
            .iter()
            .any(|line| line.contains("\"service\""))));
    }

    #[cfg(feature = "wasm-faad2")]
    #[test]
    fn decode_faad_fixture_produces_pcm_and_fd3_events() {
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
        let out = decode_eti_to_faad_memory(&eti).expect("decode should succeed");

        assert!(!out.pcm_bytes.is_empty());
        assert!(!out.metadata_jsonl.is_empty());
        assert!(out
            .metadata_jsonl
            .iter()
            .any(|line| line.contains("\"service\"")));
    }

    #[cfg(feature = "wasm-faad2")]
    #[test]
    fn decode_faad_all_services_produces_multiple_service_outputs() {
        let eti = fs::read("test-local/multiplex.eti").expect("fixture ETI must exist");
        let out = decode_eti_to_faad_all_services_memory(&eti)
            .expect("all-services decode should succeed");

        assert!(out.services.len() > 1);
        assert!(out.services.iter().all(|svc| !svc.pcm_bytes.is_empty()));
        assert!(out.services.iter().all(|svc| svc
            .metadata_jsonl
            .iter()
            .any(|line| line.contains("\"service\""))));
    }
}
