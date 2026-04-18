use std::ffi::CStr;
use std::os::raw::{c_uchar, c_void};

use anyhow::{anyhow, Result};

use super::{AacDecoder, StreamParameters};

const FAAD_FMT_16BIT: u8 = 1;

#[repr(C)]
struct NeAACDecConfiguration {
    def_object_type: c_uchar,
    def_sample_rate: u64,
    output_format: c_uchar,
    down_matrix: c_uchar,
    use_old_adts_format: c_uchar,
    dont_upsample_implicit_sbr: c_uchar,
}

#[repr(C)]
struct NeAACDecFrameInfo {
    bytesconsumed: u64,
    samples: u64,
    channels: c_uchar,
    error: c_uchar,
    samplerate: u64,
    sbr: c_uchar,
    object_type: c_uchar,
    header_type: c_uchar,
    num_front_channels: c_uchar,
    num_side_channels: c_uchar,
    num_back_channels: c_uchar,
    num_lfe_channels: c_uchar,
    channel_position: [c_uchar; 64],
    ps: c_uchar,
}

type NeAACDecHandle = *mut c_void;

#[link(name = "faad")]
unsafe extern "C" {
    fn NeAACDecOpen() -> NeAACDecHandle;
    fn NeAACDecClose(hDecoder: NeAACDecHandle);
    fn NeAACDecGetCurrentConfiguration(hDecoder: NeAACDecHandle) -> *mut NeAACDecConfiguration;
    fn NeAACDecSetConfiguration(
        hDecoder: NeAACDecHandle,
        config: *mut NeAACDecConfiguration,
    ) -> c_uchar;
    fn NeAACDecInit2(
        hDecoder: NeAACDecHandle,
        pBuffer: *mut c_uchar,
        SizeOfDecoderSpecificInfo: u64,
        samplerate: *mut u64,
        channels: *mut c_uchar,
    ) -> i8;
    fn NeAACDecDecode(
        hDecoder: NeAACDecHandle,
        hInfo: *mut NeAACDecFrameInfo,
        buffer: *mut c_uchar,
        buffer_size: u64,
    ) -> *mut c_void;
    fn NeAACDecGetErrorMessage(errcode: c_uchar) -> *mut i8;
}

pub struct FaadDecoder {
    handle: NeAACDecHandle,
    initialized: bool,
}

// The decoder handle is only touched from the owning receive thread.
unsafe impl Send for FaadDecoder {}

impl Default for FaadDecoder {
    fn default() -> Self {
        let handle = unsafe { NeAACDecOpen() };
        if !handle.is_null() {
            let conf = unsafe { NeAACDecGetCurrentConfiguration(handle) };
            if !conf.is_null() {
                unsafe {
                    (*conf).output_format = FAAD_FMT_16BIT;
                    (*conf).dont_upsample_implicit_sbr = 0;
                    let _ = NeAACDecSetConfiguration(handle, conf);
                }
            }
        }

        Self {
            handle,
            initialized: false,
        }
    }
}

impl AacDecoder for FaadDecoder {
    fn decode_access_unit(&mut self, params: &StreamParameters, data: &[u8]) -> Result<Vec<i16>> {
        if self.handle.is_null() || data.is_empty() {
            return Ok(Vec::new());
        }

        let _ps_used = params.ps_flag != 0;

        if !self.initialized {
            let mut asc = build_audio_specific_config(params);
            let mut sample_rate = 0u64;
            let mut channels = 0u8;
            let result = unsafe {
                NeAACDecInit2(
                    self.handle,
                    asc.as_mut_ptr(),
                    asc.len() as u64,
                    &mut sample_rate,
                    &mut channels,
                )
            };
            if result < 0 {
                return Err(anyhow!("NeAACDecInit2 failed"));
            }
            self.initialized = true;
        }

        let mut frame_info = NeAACDecFrameInfo {
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
            channel_position: [0; 64],
            ps: 0,
        };

        let out = unsafe {
            NeAACDecDecode(
                self.handle,
                &mut frame_info,
                data.as_ptr().cast_mut(),
                data.len() as u64,
            )
        };

        if frame_info.error != 0 {
            let msg_ptr = unsafe { NeAACDecGetErrorMessage(frame_info.error) };
            let message = if msg_ptr.is_null() {
                "FAAD decode error".to_string()
            } else {
                unsafe { CStr::from_ptr(msg_ptr) }
                    .to_string_lossy()
                    .into_owned()
            };
            return Err(anyhow!(message));
        }

        if out.is_null() || frame_info.samples == 0 {
            return Ok(Vec::new());
        }

        let samples =
            unsafe { std::slice::from_raw_parts(out.cast::<i16>(), frame_info.samples as usize) };

        if frame_info.channels == 1 {
            let mut stereo = Vec::with_capacity(samples.len() * 2);
            for sample in samples {
                stereo.push(*sample);
                stereo.push(*sample);
            }
            Ok(stereo)
        } else {
            Ok(samples.to_vec())
        }
    }
}

impl Drop for FaadDecoder {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { NeAACDecClose(self.handle) };
        }
    }
}

fn build_audio_specific_config(params: &StreamParameters) -> [u8; 2] {
    let core_sr_index = params.core_sr_index;
    let core_ch_config = match params.mpeg_surround {
        0 => {
            if params.aac_channel_mode != 0 {
                2
            } else {
                1
            }
        }
        1 => 6,
        2 => 7,
        _ => 2,
    };

    [
        (0b00010 << 3) | (core_sr_index >> 1),
        ((core_sr_index & 0x01) << 7) | (core_ch_config << 3) | 0b100,
    ]
}
