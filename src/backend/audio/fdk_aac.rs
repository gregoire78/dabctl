#![cfg(feature = "fdk-aac")]

use std::ffi::c_void;

use anyhow::{anyhow, Result};

use super::{mp4processor::build_loas_stream, AacDecoder, StreamParameters};

const TT_MP4_LOAS: u32 = 10;
const AAC_DEC_OK: i32 = 0x0000;
const AAC_DEC_NOT_ENOUGH_BITS: i32 = 0x1002;
const MAX_PCM_SAMPLES: usize = 8192;

type HandleAacDecoder = *mut c_void;

#[repr(C)]
struct CStreamInfo {
    sample_rate: i32,
    frame_size: i32,
    num_channels: i32,
}

#[link(name = "fdk-aac")]
unsafe extern "C" {
    fn aacDecoder_Open(transport_fmt: u32, nr_of_layers: u32) -> HandleAacDecoder;
    fn aacDecoder_Fill(
        self_handle: HandleAacDecoder,
        buffer: *mut *mut u8,
        buffer_size: *const u32,
        bytes_valid: *mut u32,
    ) -> i32;
    fn aacDecoder_DecodeFrame(
        self_handle: HandleAacDecoder,
        pcm: *mut i16,
        time_data_size: i32,
        flags: u32,
    ) -> i32;
    fn aacDecoder_GetStreamInfo(self_handle: HandleAacDecoder) -> *const CStreamInfo;
    fn aacDecoder_Close(self_handle: HandleAacDecoder);
}

pub struct FdkAacDecoder {
    handle: HandleAacDecoder,
}

unsafe impl Send for FdkAacDecoder {}

impl Default for FdkAacDecoder {
    fn default() -> Self {
        let handle = unsafe { aacDecoder_Open(TT_MP4_LOAS, 1) };
        Self { handle }
    }
}

impl Drop for FdkAacDecoder {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { aacDecoder_Close(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

impl AacDecoder for FdkAacDecoder {
    fn decode_access_unit(&mut self, params: &StreamParameters, data: &[u8]) -> Result<Vec<i16>> {
        if self.handle.is_null() || data.is_empty() {
            return Ok(Vec::new());
        }

        let mut loas = build_loas_stream(data.len(), params, data);
        let mut input_ptr = loas.as_mut_ptr();
        let buffer_size = [loas.len() as u32];
        let mut bytes_valid = buffer_size[0];

        let fill_err = unsafe {
            aacDecoder_Fill(
                self.handle,
                &mut input_ptr,
                buffer_size.as_ptr(),
                &mut bytes_valid,
            )
        };
        if fill_err != AAC_DEC_OK {
            return Err(anyhow!("FDK fill error 0x{fill_err:04x}"));
        }

        let mut pcm = vec![0i16; MAX_PCM_SAMPLES];
        let decode_err =
            unsafe { aacDecoder_DecodeFrame(self.handle, pcm.as_mut_ptr(), pcm.len() as i32, 0) };
        if decode_err == AAC_DEC_NOT_ENOUGH_BITS {
            return Ok(Vec::new());
        }
        if decode_err != AAC_DEC_OK {
            return Err(anyhow!("FDK decode error 0x{decode_err:04x}"));
        }

        let info = unsafe { aacDecoder_GetStreamInfo(self.handle) };
        if info.is_null() {
            return Ok(Vec::new());
        }

        let (frame_size, num_channels) = unsafe { ((*info).frame_size, (*info).num_channels) };
        if frame_size <= 0 || num_channels <= 0 {
            return Ok(Vec::new());
        }

        let sample_count = (frame_size as usize).saturating_mul(num_channels as usize);
        let sample_count = sample_count.min(pcm.len());
        pcm.truncate(sample_count);

        if num_channels == 1 {
            let mut stereo = Vec::with_capacity(pcm.len() * 2);
            for sample in pcm {
                stereo.push(sample);
                stereo.push(sample);
            }
            Ok(stereo)
        } else {
            Ok(pcm)
        }
    }
}
