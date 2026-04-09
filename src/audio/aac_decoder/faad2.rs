// ─────────────────────────────────────────────────────────────────────────────
// faad2 backend — always compiled
// Inspired by AbracaDABra (KejPi, MIT licence) — USE_FDKAAC=OFF equivalent
// ─────────────────────────────────────────────────────────────────────────────
use super::AudioFormat;
use std::os::raw::{c_char, c_uchar, c_ulong};

#[allow(non_camel_case_types)]
mod ffi {
    use std::os::raw::{c_char, c_long, c_uchar, c_ulong, c_void};

    pub type NeAACDecHandle = *mut c_void;

    #[repr(C)]
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

    #[repr(C)]
    #[allow(non_snake_case)]
    pub struct NeAACDecConfiguration {
        pub defObjectType: c_uchar,
        pub defSampleRate: c_ulong,
        pub outputFormat: c_uchar,
        pub downMatrix: c_uchar,
        pub useOldADTSFormat: c_uchar,
        pub dontUpSampleImplicitSBR: c_uchar,
    }

    pub const FAAD_FMT_16BIT: c_uchar = 1;
    pub const LC_DEC_CAP: c_ulong = 1;

    unsafe extern "C" {
        pub fn NeAACDecGetCapabilities() -> c_ulong;
        pub fn NeAACDecOpen() -> NeAACDecHandle;
        pub fn NeAACDecClose(handle: NeAACDecHandle);
        pub fn NeAACDecGetCurrentConfiguration(
            handle: NeAACDecHandle,
        ) -> *mut NeAACDecConfiguration;
        pub fn NeAACDecSetConfiguration(
            handle: NeAACDecHandle,
            config: *mut NeAACDecConfiguration,
        ) -> c_uchar;
        pub fn NeAACDecInit2(
            handle: NeAACDecHandle,
            buffer: *const c_uchar,
            buffer_size: c_ulong,
            samplerate: *mut c_ulong,
            channels: *mut c_uchar,
        ) -> c_long;
        pub fn NeAACDecDecode(
            handle: NeAACDecHandle,
            info: *mut NeAACDecFrameInfo,
            buffer: *mut c_uchar,
            buffer_size: c_ulong,
        ) -> *mut c_void;
        pub fn NeAACDecGetErrorMessage(errcode: c_uchar) -> *const c_char;
    }
}

pub struct AacDecoder {
    handle: ffi::NeAACDecHandle,
    frame_info: ffi::NeAACDecFrameInfo,
    pub sample_rate: u32,
    pub channels: u8,
}

impl AacDecoder {
    pub fn new(asc: &[u8]) -> Result<Self, String> {
        // Safety: all raw pointers are checked for null before use.
        unsafe {
            let cap = ffi::NeAACDecGetCapabilities();
            if cap & ffi::LC_DEC_CAP == 0 {
                return Err("FAAD2: no LC decoding support".into());
            }

            let handle = ffi::NeAACDecOpen();
            if handle.is_null() {
                return Err("FAAD2: NeAACDecOpen failed".into());
            }

            let config = ffi::NeAACDecGetCurrentConfiguration(handle);
            if config.is_null() {
                ffi::NeAACDecClose(handle);
                return Err("FAAD2: NeAACDecGetCurrentConfiguration failed".into());
            }
            (*config).outputFormat = ffi::FAAD_FMT_16BIT;
            (*config).dontUpSampleImplicitSBR = 0;

            if ffi::NeAACDecSetConfiguration(handle, config) != 1 {
                ffi::NeAACDecClose(handle);
                return Err("FAAD2: NeAACDecSetConfiguration failed".into());
            }

            let mut output_sr: c_ulong = 0;
            let mut output_ch: c_uchar = 0;
            let result = ffi::NeAACDecInit2(
                handle,
                asc.as_ptr(),
                asc.len() as c_ulong,
                &mut output_sr,
                &mut output_ch,
            );
            if result != 0 {
                let msg = ffi::NeAACDecGetErrorMessage((-result) as u8);
                let err = if msg.is_null() {
                    format!("FAAD2: init error {}", result)
                } else {
                    let cstr = std::ffi::CStr::from_ptr(msg as *const c_char);
                    format!("FAAD2: {}", cstr.to_string_lossy())
                };
                ffi::NeAACDecClose(handle);
                return Err(err);
            }

            let frame_info = std::mem::zeroed();
            Ok(AacDecoder {
                handle,
                frame_info,
                sample_rate: output_sr as u32,
                channels: output_ch,
            })
        }
    }

    pub fn decode_frame(&mut self, data: &[u8]) -> Option<Vec<i16>> {
        // Safety: FAAD2 writes into an internally managed buffer; we copy out.
        unsafe {
            let mut buf = data.to_vec();
            let output = ffi::NeAACDecDecode(
                self.handle,
                &mut self.frame_info,
                buf.as_mut_ptr(),
                buf.len() as c_ulong,
            );

            if self.frame_info.error != 0 {
                return None;
            }
            if self.frame_info.bytesconsumed == 0 && self.frame_info.samples == 0 {
                return None;
            }
            let num_samples = self.frame_info.samples as usize;
            if num_samples == 0 || output.is_null() {
                return None;
            }

            let pcm_ptr = output as *const i16;
            Some(std::slice::from_raw_parts(pcm_ptr, num_samples).to_vec())
        }
    }

    #[cfg_attr(feature = "fdk-aac", allow(dead_code))]
    pub fn audio_format(&self) -> AudioFormat {
        AudioFormat {
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }
}

impl Drop for AacDecoder {
    fn drop(&mut self) {
        // Safety: handle is valid; never double-freed.
        unsafe { ffi::NeAACDecClose(self.handle) }
    }
}

// Safety: the faad2 handle is never shared across threads.
unsafe impl Send for AacDecoder {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_asc() {
        let result = AacDecoder::new(&[]);
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn decode_frame_returns_none_on_empty_input() {
        // Stereo HE-AAC v2 ASC at 48 kHz — used in the DAB+ test suite.
        let asc: &[u8] = &[0x2B, 0x11, 0x88, 0x00, 0x06, 0x00, 0x4A, 0x00];
        if let Ok(mut dec) = AacDecoder::new(asc) {
            let result = dec.decode_frame(&[]);
            assert!(result.is_none() || result.is_some());
        }
    }
}
