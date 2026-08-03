use serde_json::json;
use std::io::BufWriter;
use std::path::Path;

use crate::dablin::metadata::MetadataEmitter;
use crate::dablin::pad::PadEvents;
use crate::dablin::runner::meta_helpers::{save_slide_file, should_emit_slide_metadata};
use crate::dablin::shared::{encode_slide_base64, hash_bytes};
use crate::dablin::utils::jsonl::write_jsonl;

fn should_emit_dynamic_label(dl: &str, dedup_pad: bool, last_dl: &Option<String>) -> bool {
    !(dedup_pad && last_dl.as_deref() == Some(dl))
}

fn slide_hash_if_emit(
    slide_data: &[u8],
    dedup_pad: bool,
    last_slide_hash: &Option<u64>,
) -> Option<u64> {
    let slide_hash = hash_bytes(slide_data);
    let is_dup_slide = dedup_pad && *last_slide_hash == Some(slide_hash);
    if is_dup_slide {
        None
    } else {
        Some(slide_hash)
    }
}

pub(crate) fn emit_one_service_pad_events(
    pad_events: PadEvents,
    meta: &mut Option<MetadataEmitter>,
    slide_dir: Option<&Path>,
    slide_base64: bool,
    dedup_pad: bool,
    last_dl: &mut Option<String>,
    last_slide_hash: &mut Option<u64>,
) {
    if let Some(dl) = pad_events.dynamic_label {
        if should_emit_dynamic_label(dl.as_str(), dedup_pad, last_dl) {
            if let Some(m) = meta.as_mut() {
                m.emit_dynamic_label(&dl);
            }
            *last_dl = Some(dl);
        }
    }

    if let Some(slide) = pad_events.slide {
        if let Some(slide_hash) = slide_hash_if_emit(&slide.data, dedup_pad, last_slide_hash) {
            if let Some(dir) = slide_dir {
                save_slide_file(dir, &slide.content_name, &slide.data);
            }
            if should_emit_slide_metadata(slide_dir, slide_base64) {
                if let Some(m) = meta.as_mut() {
                    let data_base64 = encode_slide_base64(&slide.data, slide_base64);
                    m.emit_slide(&slide.content_name, &slide.content_type, &data_base64);
                }
            }
            *last_slide_hash = Some(slide_hash);
        }
    }
}

pub(crate) fn emit_all_services_pad_events(
    pad_events: PadEvents,
    meta: &mut BufWriter<std::fs::File>,
    slide_dir: &Path,
    slide_base64: bool,
    dedup_pad: bool,
    last_dl: &mut Option<String>,
    last_slide_hash: &mut Option<u64>,
) {
    if let Some(dl) = pad_events.dynamic_label {
        if should_emit_dynamic_label(dl.as_str(), dedup_pad, last_dl) {
            write_jsonl(meta, json!({"dl": dl}));
            *last_dl = Some(dl);
        }
    }

    if let Some(slide) = pad_events.slide {
        if let Some(slide_hash) = slide_hash_if_emit(&slide.data, dedup_pad, last_slide_hash) {
            save_slide_file(slide_dir, &slide.content_name, &slide.data);
            let data_base64 = encode_slide_base64(&slide.data, slide_base64);
            write_jsonl(
                meta,
                json!({
                    "slide": {
                        "contentName": slide.content_name,
                        "contentType": slide.content_type,
                        "data": data_base64,
                    }
                }),
            );
            *last_slide_hash = Some(slide_hash);
        }
    }
}
