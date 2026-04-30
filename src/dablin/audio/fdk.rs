//! fdk-aac backend (feature-gated)
//!
//! Wraps libfdk-aac via FFI for AAC/HE-AAC decoding.
//! Only compiled when the `fdk-aac` feature is enabled.

use std::ffi::c_int;

// Minimal fdk-aac FFI surface
#[allow(non_camel_case_types)]
type HANDLE_AACDECODER = *mut std::ffi::c_void;

extern "C" {
    fn aacDecoder_Open(transport_type: c_int, nr_of_layers: c_int) -> HANDLE_AACDECODER;
    fn aacDecoder_Close(handle: HANDLE_AACDECODER);
    fn aacDecoder_ConfigRaw(
        handle: HANDLE_AACDECODER,
        conf: *mut *mut u8,
        length: *mut u32,
    ) -> c_int;
    fn aacDecoder_Fill(
        handle: HANDLE_AACDECODER,
        p_buffer: *mut *mut u8,
        buffer_size: *mut u32,
        bytes_valid: *mut u32,
    ) -> c_int;
    fn aacDecoder_DecodeFrame(
        handle: HANDLE_AACDECODER,
        time_data: *mut i16,
        time_data_size: c_int,
        flags: c_int,
    ) -> c_int;
    fn aacDecoder_GetStreamInfo(handle: HANDLE_AACDECODER) -> *const FdkStreamInfo;
}

/// Subset of CStreamInfo from fdk-aac we need.
#[repr(C)]
struct FdkStreamInfo {
    /// Input sample rate
    pub sample_rate: c_int,
    /// Frame size (samples per channel)
    pub frame_size: c_int,
    /// Number of output channels
    pub num_channels: c_int,
    _padding: [u8; 512],
}

/// Transport type: RAW AAC access units (no ADTS/LOAS headers).
const TT_MP4_RAW: c_int = 0;
/// Maximum output buffer size (2048 samples × 8 channels × 2 bytes)
const MAX_BUF_SAMPLES: usize = 2048 * 8;

pub struct FdkDecoder {
    handle: HANDLE_AACDECODER,
    configured: bool,
}

impl FdkDecoder {
    pub fn new() -> Self {
        let handle = unsafe { aacDecoder_Open(TT_MP4_RAW, 1) };
        if handle.is_null() {
            panic!("fdk-aac: aacDecoder_Open failed");
        }
        FdkDecoder {
            handle,
            configured: false,
        }
    }

    /// Configure RAW decoder with MPEG-4 AudioSpecificConfig.
    pub fn init_with_asc(&mut self, asc: &[u8]) -> bool {
        if self.configured {
            return true;
        }
        if asc.is_empty() {
            return false;
        }

        let mut conf_ptr = asc.as_ptr() as *mut u8;
        let mut conf_len = asc.len() as u32;
        let cfg_err = unsafe { aacDecoder_ConfigRaw(self.handle, &mut conf_ptr, &mut conf_len) };
        if cfg_err != 0 {
            tracing::warn!("fdk-aac: ConfigRaw error {:#x}", cfg_err);
            return false;
        }

        self.configured = true;
        true
    }

    /// Decode one RAW AAC AU, returning s16le PCM samples.
    /// Returns `None` on decode error.
    pub fn decode(&mut self, data: &[u8]) -> Option<Vec<i16>> {
        if !self.configured {
            return None;
        }

        let mut buf_ptr = data.as_ptr() as *mut u8;
        let mut buf_size = data.len() as u32;
        let mut bytes_valid = buf_size;

        let fill_err = unsafe {
            aacDecoder_Fill(
                self.handle,
                &mut buf_ptr,
                &mut buf_size,
                &mut bytes_valid,
            )
        };
        if fill_err != 0 {
            tracing::warn!("fdk-aac: Fill error {:#x}", fill_err);
            return None;
        }

        let mut pcm_buf = vec![0i16; MAX_BUF_SAMPLES];
        let dec_err = unsafe {
            aacDecoder_DecodeFrame(
                self.handle,
                pcm_buf.as_mut_ptr(),
                pcm_buf.len() as c_int,
                0,
            )
        };
        if dec_err != 0 {
            tracing::warn!("fdk-aac: DecodeFrame error {:#x}", dec_err);
            return None;
        }

        let info_ptr = unsafe { aacDecoder_GetStreamInfo(self.handle) };
        if info_ptr.is_null() {
            return None;
        }
        let (frame_size, num_channels) =
            unsafe { ((*info_ptr).frame_size as usize, (*info_ptr).num_channels as usize) };

        let total_samples = frame_size * num_channels;
        if total_samples == 0 || total_samples > pcm_buf.len() {
            return None;
        }

        pcm_buf.truncate(total_samples);
        Some(pcm_buf)
    }
}

impl Drop for FdkDecoder {
    fn drop(&mut self) {
        unsafe { aacDecoder_Close(self.handle) };
    }
}
