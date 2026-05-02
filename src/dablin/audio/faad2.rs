//! faad2 AAC decoder bindings
//! Reference: neaacdec.h, dablin AACDecoderFAAD2

use std::os::raw::{c_int, c_uchar, c_ulong, c_void};

use crate::dablin::dabplus::SuperframeFormat;

#[allow(non_camel_case_types)]
type NeAACDecHandle = *mut c_void;

/// NeAACDecFrameInfo – returned by NeAACDecDecode
#[repr(C)]
#[derive(Debug)]
#[allow(non_snake_case, dead_code)]
pub struct NeAACDecFrameInfo {
    pub bytesconsumed: c_ulong,
    pub samples: c_ulong,
    pub channels: c_uchar,
    pub error: c_uchar,
    pub samplerate: c_ulong,
    pub sbr: c_uchar,
    pub object_type: c_uchar,
    pub header_type: c_uchar,
    pub num_front_channels: c_uchar,
    pub num_side_channels: c_uchar,
    pub num_back_channels: c_uchar,
    pub num_lfe_channels: c_uchar,
    pub channel_position: [c_uchar; 64],
    pub ps: c_uchar,
}

impl Default for NeAACDecFrameInfo {
    fn default() -> Self {
        Self {
            bytesconsumed: 0,
            samples: 0,
            channels: 0,
            error: 0,
            samplerate: 0,
            sbr: 0,
            object_type: 0,
            header_type: 0,
            num_front_channels: 0,
            num_side_channels: 0,
            num_back_channels: 0,
            num_lfe_channels: 0,
            channel_position: [0u8; 64],
            ps: 0,
        }
    }
}

extern "C" {
    fn NeAACDecOpen() -> NeAACDecHandle;
    fn NeAACDecInit2(
        hDecoder: NeAACDecHandle,
        pBuffer: *const c_uchar,
        SizeOfDecoderSpecificInfo: c_ulong,
        samplerate: *mut c_ulong,
        channels: *mut c_uchar,
    ) -> c_int;
    fn NeAACDecDecode(
        hDecoder: NeAACDecHandle,
        hInfo: *mut NeAACDecFrameInfo,
        buffer: *const c_uchar,
        buffer_size: c_ulong,
    ) -> *mut c_void;
    fn NeAACDecClose(hDecoder: NeAACDecHandle);
}

/// Build the AudioSpecificConfig matching dablin's AACDecoder constructor.
///
/// Reference: dablin AACDecoder::AACDecoder() in dabplus_decoder.cpp
/// Format: AAC-LC with 960-sample transform (GASpecificConfig window = 1)
/// Extended with SBR/PS when applicable.
pub fn build_asc(fmt: &SuperframeFormat) -> Vec<u8> {
    let mut asc = Vec::with_capacity(7);

    // AudioObjectType = 2 (AAC-LC) → 5 bits = 0b00010
    // CoreSrIndex → 4 bits
    // CoreChConfig → 4 bits
    // GASpecificConfig: frameLengthFlag=1 (960), dependsOnCoreCoder=0, extensionFlag=0 → 3 bits
    // Total first two bytes: 00010|xxxx|xxxx|100
    let sr = fmt.core_sr_index();
    let ch = fmt.core_ch_config();

    asc.push(0b00010 << 3 | sr >> 1);
    asc.push((sr & 0x01) << 7 | ch << 3 | 0b100);

    if fmt.sbr_flag {
        // Explicit backwards-compatible SBR signaling
        // syncExtensionType = 0x2B7 (11 bits) → AudioObjectType 5 (SBR) → SBR present
        asc.push(0x56);
        asc.push(0xE5);
        asc.push(0x80 | (fmt.ext_sr_index() << 3));

        if fmt.ps_flag {
            // PS present
            *asc.last_mut().unwrap() |= 0x05;
            asc.push(0x48);
            asc.push(0x80);
        }
    }

    asc
}

/// Safe wrapper around a faad2 decoder instance.
pub struct Faad2Decoder {
    handle: NeAACDecHandle,
    initialized: bool,
    /// Sample rate after initialization
    pub sample_rate: u32,
    /// Channel count after initialization
    pub channels: u8,
}

impl Faad2Decoder {
    pub fn new() -> Option<Self> {
        let handle = unsafe { NeAACDecOpen() };
        if handle.is_null() {
            return None;
        }
        Some(Self {
            handle,
            initialized: false,
            sample_rate: 0,
            channels: 0,
        })
    }

    /// Initialize with AudioSpecificConfig built from the superframe format.
    pub fn init_with_asc(&mut self, asc: &[u8]) -> bool {
        let mut sr: c_ulong = 0;
        let mut ch: c_uchar = 0;
        let result = unsafe {
            NeAACDecInit2(
                self.handle,
                asc.as_ptr(),
                asc.len() as c_ulong,
                &mut sr,
                &mut ch,
            )
        };
        if result < 0 {
            return false;
        }
        self.sample_rate = sr as u32;
        self.channels = ch;
        self.initialized = true;
        true
    }

    /// Decode one AU, returning raw s16le PCM samples.
    /// Returns `None` on decode error.
    pub fn decode(&mut self, data: &[u8]) -> Option<Vec<i16>> {
        if !self.initialized {
            return None;
        }
        let mut info = NeAACDecFrameInfo::default();
        let pcm_ptr =
            unsafe { NeAACDecDecode(self.handle, &mut info, data.as_ptr(), data.len() as c_ulong) };
        if info.error != 0 || pcm_ptr.is_null() {
            if info.error != 0 {
                tracing::warn!(
                    "faad2: decode error {} (AU {} bytes)",
                    info.error,
                    data.len()
                );
            }
            return None;
        }
        let n_samples = info.samples as usize;
        if n_samples == 0 {
            return None;
        }
        // PCM is returned as i16 samples
        let pcm_i16: &[i16] =
            unsafe { std::slice::from_raw_parts(pcm_ptr as *const i16, n_samples) };
        Some(pcm_i16.to_vec())
    }
}

impl Drop for Faad2Decoder {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { NeAACDecClose(self.handle) };
        }
    }
}

// Each Faad2Decoder holds its own independent faad2 context handle.
// The handle is never shared between threads; only the owning thread touches it.
// SAFETY: libfaad2 context is not thread-safe when shared, but here each
// ServiceDumpContext owns exactly one instance used from a single Rayon worker.
unsafe impl Send for Faad2Decoder {}
