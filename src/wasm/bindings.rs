#![cfg(target_arch = "wasm32")]

use js_sys::Array;
use std::str::FromStr;
use wasm_bindgen::prelude::*;

use crate::cli::DateTimeFormat;
use crate::wasm::runtime::{
    decode_eti_to_adts_all_services_memory, decode_eti_to_adts_all_services_memory_with_options,
    decode_eti_to_adts_memory, decode_eti_to_adts_memory_with_options,
    decode_eti_to_all_services_memory, decode_eti_to_all_services_memory_with_options,
    decode_eti_to_latm_all_services_memory, decode_eti_to_latm_all_services_memory_with_options,
    decode_eti_to_latm_memory, decode_eti_to_latm_memory_with_options, AdtsDecodeOutput,
    AllServicesAdtsDecodeOutput, AllServicesLatmDecodeOutput, AllServicesMetadataDecodeOutput,
    LatmDecodeOutput, ServiceAdtsDecodeOutput, ServiceLatmDecodeOutput,
    ServiceMetadataDecodeOutput, WasmAllServicesDecodeOptions, WasmLatmDecodeOptions,
};

#[cfg(feature = "wasm-faad2")]
use crate::wasm::runtime::{
    decode_eti_to_faad_all_services_memory, decode_eti_to_faad_all_services_memory_with_options,
    decode_eti_to_faad_memory, decode_eti_to_faad_memory_with_options, AllServicesFaadDecodeOutput,
    FaadDecodeOutput, ServiceFaadDecodeOutput,
};

#[wasm_bindgen(js_name = "WasmLatmDecodeOptions")]
pub struct WasmLatmDecodeOptionsJs {
    sid: Option<String>,
    label: Option<String>,
    slide_base64: bool,
    dedup_pad: bool,
    datetime_format: Option<String>,
}

#[wasm_bindgen(js_class = "WasmLatmDecodeOptions")]
impl WasmLatmDecodeOptionsJs {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmLatmDecodeOptionsJs {
        WasmLatmDecodeOptionsJs {
            sid: None,
            label: None,
            slide_base64: false,
            dedup_pad: false,
            datetime_format: None,
        }
    }

    #[wasm_bindgen(setter, js_name = "sid")]
    pub fn set_sid(&mut self, sid: Option<String>) {
        self.sid = sid;
    }

    #[wasm_bindgen(setter, js_name = "label")]
    pub fn set_label(&mut self, label: Option<String>) {
        self.label = label;
    }

    #[wasm_bindgen(setter, js_name = "slideBase64")]
    pub fn set_slide_base64(&mut self, slide_base64: bool) {
        self.slide_base64 = slide_base64;
    }

    #[wasm_bindgen(setter, js_name = "dedupPad")]
    pub fn set_dedup_pad(&mut self, dedup_pad: bool) {
        self.dedup_pad = dedup_pad;
    }

    #[wasm_bindgen(setter, js_name = "datetimeFormat")]
    pub fn set_datetime_format(&mut self, datetime_format: Option<String>) {
        self.datetime_format = datetime_format;
    }
}

impl TryFrom<&WasmLatmDecodeOptionsJs> for WasmLatmDecodeOptions {
    type Error = String;

    fn try_from(value: &WasmLatmDecodeOptionsJs) -> std::result::Result<Self, Self::Error> {
        let datetime_format = match value.datetime_format.as_deref() {
            Some(raw) => Some(DateTimeFormat::from_str(raw)?),
            None => None,
        };

        Ok(Self {
            sid: value.sid.clone(),
            label: value.label.clone(),
            slide_base64: value.slide_base64,
            dedup_pad: value.dedup_pad,
            datetime_format,
        })
    }
}

#[wasm_bindgen(js_name = "WasmLatmDecodeOutput")]
pub struct WasmLatmDecodeOutputJs {
    inner: LatmDecodeOutput,
}

#[wasm_bindgen(js_name = "WasmAllServicesDecodeOptions")]
pub struct WasmAllServicesDecodeOptionsJs {
    slide_base64: bool,
    dedup_pad: bool,
    datetime_format: Option<String>,
}

#[wasm_bindgen(js_name = "WasmEtiSession")]
pub struct WasmEtiSessionJs {
    eti_bytes: Vec<u8>,
}

#[wasm_bindgen(js_class = "WasmAllServicesDecodeOptions")]
impl WasmAllServicesDecodeOptionsJs {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmAllServicesDecodeOptionsJs {
        WasmAllServicesDecodeOptionsJs {
            slide_base64: false,
            dedup_pad: false,
            datetime_format: None,
        }
    }

    #[wasm_bindgen(setter, js_name = "slideBase64")]
    pub fn set_slide_base64(&mut self, slide_base64: bool) {
        self.slide_base64 = slide_base64;
    }

    #[wasm_bindgen(setter, js_name = "dedupPad")]
    pub fn set_dedup_pad(&mut self, dedup_pad: bool) {
        self.dedup_pad = dedup_pad;
    }

    #[wasm_bindgen(setter, js_name = "datetimeFormat")]
    pub fn set_datetime_format(&mut self, datetime_format: Option<String>) {
        self.datetime_format = datetime_format;
    }
}

impl TryFrom<&WasmAllServicesDecodeOptionsJs> for WasmAllServicesDecodeOptions {
    type Error = String;

    fn try_from(value: &WasmAllServicesDecodeOptionsJs) -> std::result::Result<Self, Self::Error> {
        let datetime_format = match value.datetime_format.as_deref() {
            Some(raw) => Some(DateTimeFormat::from_str(raw)?),
            None => None,
        };

        Ok(Self {
            slide_base64: value.slide_base64,
            dedup_pad: value.dedup_pad,
            datetime_format,
        })
    }
}

#[wasm_bindgen(js_class = "WasmEtiSession")]
impl WasmEtiSessionJs {
    #[wasm_bindgen(constructor)]
    pub fn new(eti_bytes: &[u8]) -> WasmEtiSessionJs {
        WasmEtiSessionJs {
            eti_bytes: eti_bytes.to_vec(),
        }
    }

    #[wasm_bindgen(js_name = "decodeAllServices")]
    pub fn decode_all_services(
        &self,
        options: &WasmAllServicesDecodeOptionsJs,
    ) -> std::result::Result<WasmAllServicesDecodeOutputJs, JsValue> {
        let decoded_options = WasmAllServicesDecodeOptions::try_from(options)
            .map_err(|e| JsValue::from_str(&format!("invalid all-services options: {}", e)))?;

        decode_eti_to_all_services_memory_with_options(&self.eti_bytes, &decoded_options)
            .map(|inner| WasmAllServicesDecodeOutputJs { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[cfg(feature = "wasm-faad2")]
    #[wasm_bindgen(js_name = "decodeFaadServiceBySid")]
    pub fn decode_faad_service_by_sid(
        &self,
        sid: &str,
    ) -> std::result::Result<WasmFaadDecodeOutputJs, JsValue> {
        let decoded_options = WasmLatmDecodeOptions {
            sid: Some(sid.to_string()),
            ..Default::default()
        };

        decode_eti_to_faad_memory_with_options(&self.eti_bytes, &decoded_options)
            .map(|inner| WasmFaadDecodeOutputJs { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[cfg(feature = "wasm-faad2")]
    #[wasm_bindgen(js_name = "decodeFaadServiceBySidWithOptions")]
    pub fn decode_faad_service_by_sid_with_options(
        &self,
        sid: &str,
        options: &WasmLatmDecodeOptionsJs,
    ) -> std::result::Result<WasmFaadDecodeOutputJs, JsValue> {
        let mut decoded_options = WasmLatmDecodeOptions::try_from(options)
            .map_err(|e| JsValue::from_str(&format!("invalid wasm decode options: {}", e)))?;
        decoded_options.sid = Some(sid.to_string());

        decode_eti_to_faad_memory_with_options(&self.eti_bytes, &decoded_options)
            .map(|inner| WasmFaadDecodeOutputJs { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

#[wasm_bindgen(js_name = "WasmLatmServiceOutput")]
pub struct WasmLatmServiceOutputJs {
    inner: ServiceLatmDecodeOutput,
}

#[wasm_bindgen(js_class = "WasmLatmServiceOutput")]
impl WasmLatmServiceOutputJs {
    #[wasm_bindgen(getter, js_name = "sid")]
    pub fn sid(&self) -> String {
        format!("{:#06x}", self.inner.sid)
    }

    #[wasm_bindgen(getter, js_name = "label")]
    pub fn label(&self) -> Option<String> {
        self.inner.label.clone()
    }

    #[wasm_bindgen(getter, js_name = "latmBytes")]
    pub fn latm_bytes(&self) -> Vec<u8> {
        self.inner.latm_bytes.clone()
    }

    #[wasm_bindgen(getter, js_name = "metadataJsonl")]
    pub fn metadata_jsonl(&self) -> Array {
        self.inner
            .metadata_jsonl
            .iter()
            .map(|line| JsValue::from_str(line))
            .collect::<Array>()
    }

    #[wasm_bindgen(js_name = "fd3Preview")]
    pub fn fd3_preview(&self) -> String {
        self.inner.metadata_jsonl.join("\n")
    }
}

#[wasm_bindgen(js_name = "WasmAllServicesLatmDecodeOutput")]
pub struct WasmAllServicesLatmDecodeOutputJs {
    inner: AllServicesLatmDecodeOutput,
}

#[wasm_bindgen(js_name = "WasmServiceDecodeOutput")]
pub struct WasmServiceDecodeOutputJs {
    inner: ServiceMetadataDecodeOutput,
}

#[wasm_bindgen(js_class = "WasmServiceDecodeOutput")]
impl WasmServiceDecodeOutputJs {
    #[wasm_bindgen(getter, js_name = "sid")]
    pub fn sid(&self) -> String {
        format!("{:#06x}", self.inner.sid)
    }

    #[wasm_bindgen(getter, js_name = "label")]
    pub fn label(&self) -> Option<String> {
        self.inner.label.clone()
    }

    #[wasm_bindgen(getter, js_name = "metadataJsonl")]
    pub fn metadata_jsonl(&self) -> Array {
        self.inner
            .metadata_jsonl
            .iter()
            .map(|line| JsValue::from_str(line))
            .collect::<Array>()
    }

    #[wasm_bindgen(js_name = "fd3Preview")]
    pub fn fd3_preview(&self) -> String {
        self.inner.metadata_jsonl.join("\n")
    }
}

#[wasm_bindgen(js_name = "WasmAllServicesDecodeOutput")]
pub struct WasmAllServicesDecodeOutputJs {
    inner: AllServicesMetadataDecodeOutput,
}

#[wasm_bindgen(js_class = "WasmAllServicesDecodeOutput")]
impl WasmAllServicesDecodeOutputJs {
    #[wasm_bindgen(getter, js_name = "serviceCount")]
    pub fn service_count(&self) -> usize {
        self.inner.services.len()
    }

    #[wasm_bindgen(js_name = "getService")]
    pub fn get_service(&self, index: usize) -> Option<WasmServiceDecodeOutputJs> {
        self.inner
            .services
            .get(index)
            .cloned()
            .map(|inner| WasmServiceDecodeOutputJs { inner })
    }
}

#[wasm_bindgen(js_class = "WasmAllServicesLatmDecodeOutput")]
impl WasmAllServicesLatmDecodeOutputJs {
    #[wasm_bindgen(getter, js_name = "serviceCount")]
    pub fn service_count(&self) -> usize {
        self.inner.services.len()
    }

    #[wasm_bindgen(js_name = "getService")]
    pub fn get_service(&self, index: usize) -> Option<WasmLatmServiceOutputJs> {
        self.inner
            .services
            .get(index)
            .cloned()
            .map(|inner| WasmLatmServiceOutputJs { inner })
    }
}

#[wasm_bindgen(js_class = "WasmLatmDecodeOutput")]
impl WasmLatmDecodeOutputJs {
    #[wasm_bindgen(getter, js_name = "latmBytes")]
    pub fn latm_bytes(&self) -> Vec<u8> {
        self.inner.latm_bytes.clone()
    }

    #[wasm_bindgen(getter, js_name = "metadataJsonl")]
    pub fn metadata_jsonl(&self) -> Array {
        self.inner
            .metadata_jsonl
            .iter()
            .map(|line| JsValue::from_str(line))
            .collect::<Array>()
    }

    #[wasm_bindgen(js_name = "stdoutPreview")]
    pub fn stdout_preview(&self, max_bytes: usize) -> String {
        self.inner.stdout_preview(max_bytes)
    }

    #[wasm_bindgen(js_name = "fd3Preview")]
    pub fn fd3_preview(&self) -> String {
        self.inner.fd3_preview()
    }
}

#[wasm_bindgen(js_name = "decodeEtiToLatmMemory")]
pub fn decode_eti_to_latm_memory_js(
    eti_bytes: &[u8],
) -> std::result::Result<WasmLatmDecodeOutputJs, JsValue> {
    decode_eti_to_latm_memory(eti_bytes)
        .map(|inner| WasmLatmDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = "decodeEtiToLatmMemoryWithOptions")]
pub fn decode_eti_to_latm_memory_with_options_js(
    eti_bytes: &[u8],
    options: &WasmLatmDecodeOptionsJs,
) -> std::result::Result<WasmLatmDecodeOutputJs, JsValue> {
    let decoded_options = WasmLatmDecodeOptions::try_from(options)
        .map_err(|e| JsValue::from_str(&format!("invalid wasm decode options: {}", e)))?;

    decode_eti_to_latm_memory_with_options(eti_bytes, &decoded_options)
        .map(|inner| WasmLatmDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = "decodeEtiToLatmAllServicesMemory")]
pub fn decode_eti_to_latm_all_services_memory_js(
    eti_bytes: &[u8],
) -> std::result::Result<WasmAllServicesLatmDecodeOutputJs, JsValue> {
    decode_eti_to_latm_all_services_memory(eti_bytes)
        .map(|inner| WasmAllServicesLatmDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = "decodeEtiToLatmAllServicesMemoryWithOptions")]
pub fn decode_eti_to_latm_all_services_memory_with_options_js(
    eti_bytes: &[u8],
    options: &WasmAllServicesDecodeOptionsJs,
) -> std::result::Result<WasmAllServicesLatmDecodeOutputJs, JsValue> {
    let decoded_options = WasmAllServicesDecodeOptions::try_from(options)
        .map_err(|e| JsValue::from_str(&format!("invalid all-services options: {}", e)))?;

    decode_eti_to_latm_all_services_memory_with_options(eti_bytes, &decoded_options)
        .map(|inner| WasmAllServicesLatmDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = "decodeEtiToAllServicesMemory")]
pub fn decode_eti_to_all_services_memory_js(
    eti_bytes: &[u8],
) -> std::result::Result<WasmAllServicesDecodeOutputJs, JsValue> {
    decode_eti_to_all_services_memory(eti_bytes)
        .map(|inner| WasmAllServicesDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = "decodeEtiToAllServicesMemoryWithOptions")]
pub fn decode_eti_to_all_services_memory_with_options_js(
    eti_bytes: &[u8],
    options: &WasmAllServicesDecodeOptionsJs,
) -> std::result::Result<WasmAllServicesDecodeOutputJs, JsValue> {
    let decoded_options = WasmAllServicesDecodeOptions::try_from(options)
        .map_err(|e| JsValue::from_str(&format!("invalid all-services options: {}", e)))?;

    decode_eti_to_all_services_memory_with_options(eti_bytes, &decoded_options)
        .map(|inner| WasmAllServicesDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

// ── ADTS bindings ─────────────────────────────────────────────────────────

#[wasm_bindgen(js_name = "WasmAdtsDecodeOutput")]
pub struct WasmAdtsDecodeOutputJs {
    inner: AdtsDecodeOutput,
}

#[wasm_bindgen(js_class = "WasmAdtsDecodeOutput")]
impl WasmAdtsDecodeOutputJs {
    #[wasm_bindgen(getter, js_name = "adtsBytes")]
    pub fn adts_bytes(&self) -> Vec<u8> {
        self.inner.adts_bytes.clone()
    }

    #[wasm_bindgen(getter, js_name = "metadataJsonl")]
    pub fn metadata_jsonl(&self) -> Array {
        self.inner
            .metadata_jsonl
            .iter()
            .map(|line| JsValue::from_str(line))
            .collect::<Array>()
    }

    #[wasm_bindgen(js_name = "stdoutPreview")]
    pub fn stdout_preview(&self, max_bytes: usize) -> String {
        use crate::wasm::runtime::format_stdout_hex_preview;
        format_stdout_hex_preview(&self.inner.adts_bytes, max_bytes)
    }

    #[wasm_bindgen(js_name = "fd3Preview")]
    pub fn fd3_preview(&self) -> String {
        self.inner.metadata_jsonl.join("\n")
    }
}

#[wasm_bindgen(js_name = "WasmAdtsServiceOutput")]
pub struct WasmAdtsServiceOutputJs {
    inner: ServiceAdtsDecodeOutput,
}

#[wasm_bindgen(js_class = "WasmAdtsServiceOutput")]
impl WasmAdtsServiceOutputJs {
    #[wasm_bindgen(getter, js_name = "sid")]
    pub fn sid(&self) -> String {
        format!("{:#06x}", self.inner.sid)
    }

    #[wasm_bindgen(getter, js_name = "label")]
    pub fn label(&self) -> Option<String> {
        self.inner.label.clone()
    }

    #[wasm_bindgen(getter, js_name = "adtsBytes")]
    pub fn adts_bytes(&self) -> Vec<u8> {
        self.inner.adts_bytes.clone()
    }

    #[wasm_bindgen(getter, js_name = "metadataJsonl")]
    pub fn metadata_jsonl(&self) -> Array {
        self.inner
            .metadata_jsonl
            .iter()
            .map(|line| JsValue::from_str(line))
            .collect::<Array>()
    }

    #[wasm_bindgen(js_name = "fd3Preview")]
    pub fn fd3_preview(&self) -> String {
        self.inner.metadata_jsonl.join("\n")
    }
}

#[wasm_bindgen(js_name = "WasmAllServicesAdtsDecodeOutput")]
pub struct WasmAllServicesAdtsDecodeOutputJs {
    inner: AllServicesAdtsDecodeOutput,
}

#[wasm_bindgen(js_class = "WasmAllServicesAdtsDecodeOutput")]
impl WasmAllServicesAdtsDecodeOutputJs {
    #[wasm_bindgen(getter, js_name = "serviceCount")]
    pub fn service_count(&self) -> usize {
        self.inner.services.len()
    }

    #[wasm_bindgen(js_name = "getService")]
    pub fn get_service(&self, index: usize) -> Option<WasmAdtsServiceOutputJs> {
        self.inner
            .services
            .get(index)
            .cloned()
            .map(|inner| WasmAdtsServiceOutputJs { inner })
    }
}

#[wasm_bindgen(js_name = "decodeEtiToAdtsMemory")]
pub fn decode_eti_to_adts_memory_js(
    eti_bytes: &[u8],
) -> std::result::Result<WasmAdtsDecodeOutputJs, JsValue> {
    decode_eti_to_adts_memory(eti_bytes)
        .map(|inner| WasmAdtsDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = "decodeEtiToAdtsMemoryWithOptions")]
pub fn decode_eti_to_adts_memory_with_options_js(
    eti_bytes: &[u8],
    options: &WasmLatmDecodeOptionsJs,
) -> std::result::Result<WasmAdtsDecodeOutputJs, JsValue> {
    let decoded_options = WasmLatmDecodeOptions::try_from(options)
        .map_err(|e| JsValue::from_str(&format!("invalid wasm decode options: {}", e)))?;

    decode_eti_to_adts_memory_with_options(eti_bytes, &decoded_options)
        .map(|inner| WasmAdtsDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = "decodeEtiToAdtsAllServicesMemory")]
pub fn decode_eti_to_adts_all_services_memory_js(
    eti_bytes: &[u8],
) -> std::result::Result<WasmAllServicesAdtsDecodeOutputJs, JsValue> {
    decode_eti_to_adts_all_services_memory(eti_bytes)
        .map(|inner| WasmAllServicesAdtsDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = "decodeEtiToAdtsAllServicesMemoryWithOptions")]
pub fn decode_eti_to_adts_all_services_memory_with_options_js(
    eti_bytes: &[u8],
    options: &WasmAllServicesDecodeOptionsJs,
) -> std::result::Result<WasmAllServicesAdtsDecodeOutputJs, JsValue> {
    let decoded_options = WasmAllServicesDecodeOptions::try_from(options)
        .map_err(|e| JsValue::from_str(&format!("invalid all-services options: {}", e)))?;

    decode_eti_to_adts_all_services_memory_with_options(eti_bytes, &decoded_options)
        .map(|inner| WasmAllServicesAdtsDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = "dabctlVersion")]
pub fn dabctl_version_js() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ── FAAD (raw PCM) bindings ───────────────────────────────────────────────

#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_name = "WasmFaadDecodeOutput")]
pub struct WasmFaadDecodeOutputJs {
    inner: FaadDecodeOutput,
}

#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_class = "WasmFaadDecodeOutput")]
impl WasmFaadDecodeOutputJs {
    /// Raw s16le PCM bytes (stdout-equivalent, 48 kHz stereo).
    #[wasm_bindgen(getter, js_name = "pcmBytes")]
    pub fn pcm_bytes(&self) -> Vec<u8> {
        self.inner.pcm_bytes.clone()
    }

    #[wasm_bindgen(getter, js_name = "metadataJsonl")]
    pub fn metadata_jsonl(&self) -> Array {
        self.inner
            .metadata_jsonl
            .iter()
            .map(|line| JsValue::from_str(line))
            .collect::<Array>()
    }

    #[wasm_bindgen(js_name = "fd3Preview")]
    pub fn fd3_preview(&self) -> String {
        self.inner.metadata_jsonl.join("\n")
    }
}

#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_name = "WasmFaadServiceOutput")]
pub struct WasmFaadServiceOutputJs {
    inner: ServiceFaadDecodeOutput,
}

#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_class = "WasmFaadServiceOutput")]
impl WasmFaadServiceOutputJs {
    #[wasm_bindgen(getter, js_name = "sid")]
    pub fn sid(&self) -> String {
        format!("{:#06x}", self.inner.sid)
    }

    #[wasm_bindgen(getter, js_name = "label")]
    pub fn label(&self) -> Option<String> {
        self.inner.label.clone()
    }

    #[wasm_bindgen(getter, js_name = "pcmBytes")]
    pub fn pcm_bytes(&self) -> Vec<u8> {
        self.inner.pcm_bytes.clone()
    }

    #[wasm_bindgen(getter, js_name = "metadataJsonl")]
    pub fn metadata_jsonl(&self) -> Array {
        self.inner
            .metadata_jsonl
            .iter()
            .map(|line| JsValue::from_str(line))
            .collect::<Array>()
    }

    #[wasm_bindgen(js_name = "fd3Preview")]
    pub fn fd3_preview(&self) -> String {
        self.inner.metadata_jsonl.join("\n")
    }
}

#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_name = "WasmAllServicesFaadDecodeOutput")]
pub struct WasmAllServicesFaadDecodeOutputJs {
    inner: AllServicesFaadDecodeOutput,
}

#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_class = "WasmAllServicesFaadDecodeOutput")]
impl WasmAllServicesFaadDecodeOutputJs {
    #[wasm_bindgen(getter, js_name = "serviceCount")]
    pub fn service_count(&self) -> usize {
        self.inner.services.len()
    }

    #[wasm_bindgen(js_name = "getService")]
    pub fn get_service(&self, index: usize) -> Option<WasmFaadServiceOutputJs> {
        self.inner
            .services
            .get(index)
            .cloned()
            .map(|inner| WasmFaadServiceOutputJs { inner })
    }
}

/// Decode ETI bytes to raw s16le PCM using the faad2 AAC decoder (default options).
#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_name = "decodeEtiToFaadMemory")]
pub fn decode_eti_to_faad_memory_js(
    eti_bytes: &[u8],
) -> std::result::Result<WasmFaadDecodeOutputJs, JsValue> {
    decode_eti_to_faad_memory(eti_bytes)
        .map(|inner| WasmFaadDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Decode ETI bytes to raw s16le PCM using the faad2 AAC decoder with explicit options.
#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_name = "decodeEtiToFaadMemoryWithOptions")]
pub fn decode_eti_to_faad_memory_with_options_js(
    eti_bytes: &[u8],
    options: &WasmLatmDecodeOptionsJs,
) -> std::result::Result<WasmFaadDecodeOutputJs, JsValue> {
    let decoded_options = WasmLatmDecodeOptions::try_from(options)
        .map_err(|e| JsValue::from_str(&format!("invalid wasm decode options: {}", e)))?;

    decode_eti_to_faad_memory_with_options(eti_bytes, &decoded_options)
        .map(|inner| WasmFaadDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Decode ETI bytes to raw s16le PCM for all DAB+ services using faad2 (default options).
#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_name = "decodeEtiToFaadAllServicesMemory")]
pub fn decode_eti_to_faad_all_services_memory_js(
    eti_bytes: &[u8],
) -> std::result::Result<WasmAllServicesFaadDecodeOutputJs, JsValue> {
    decode_eti_to_faad_all_services_memory(eti_bytes)
        .map(|inner| WasmAllServicesFaadDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Decode ETI bytes to raw s16le PCM for all DAB+ services using faad2 with explicit options.
#[cfg(feature = "wasm-faad2")]
#[wasm_bindgen(js_name = "decodeEtiToFaadAllServicesMemoryWithOptions")]
pub fn decode_eti_to_faad_all_services_memory_with_options_js(
    eti_bytes: &[u8],
    options: &WasmAllServicesDecodeOptionsJs,
) -> std::result::Result<WasmAllServicesFaadDecodeOutputJs, JsValue> {
    let decoded_options = WasmAllServicesDecodeOptions::try_from(options)
        .map_err(|e| JsValue::from_str(&format!("invalid all-services options: {}", e)))?;

    decode_eti_to_faad_all_services_memory_with_options(eti_bytes, &decoded_options)
        .map(|inner| WasmAllServicesFaadDecodeOutputJs { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
