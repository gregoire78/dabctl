//! faad2 AAC decoder bindings
//! Reference: neaacdec.h, dablin AACDecoderFAAD2

use std::os::raw::{c_int, c_uchar, c_ulong, c_void};

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
    /// Follows dablin behavior: logs error warnings but returns PCM if samples > 0.
    /// Only returns None if both bytesconsumed and samples are zero.
    pub fn decode(&mut self, data: &[u8]) -> Option<Vec<i16>> {
        if !self.initialized {
            return None;
        }
        let mut info = NeAACDecFrameInfo::default();
        let pcm_ptr =
            unsafe { NeAACDecDecode(self.handle, &mut info, data.as_ptr(), data.len() as c_ulong) };

        // Log error warning but don't abort
        if info.error != 0 {
            tracing::warn!(
                "faad2: decode error {} (AU {} bytes, samples: {}, consumed: {})",
                info.error,
                data.len(),
                info.samples,
                info.bytesconsumed
            );
        }

        // Abort only if both bytesconsumed and samples are zero (matches dablin behavior)
        if info.bytesconsumed == 0 && info.samples == 0 {
            return None;
        }

        let n_samples = info.samples as usize;
        if n_samples == 0 || pcm_ptr.is_null() {
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

// SAFETY: libfaad2 decoder handles are not thread-safe when shared, but each
// Faad2Decoder instance is owned by a single thread and never aliased.
unsafe impl Send for Faad2Decoder {}
