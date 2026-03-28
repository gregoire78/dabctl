/// AAC decoder using libfaad2 FFI for DAB+ (HE-AAC v2, 960-sample transform)

// FAAD2 FFI bindings
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

    // Output format: 16-bit PCM
    pub const FAAD_FMT_16BIT: c_uchar = 1;

    // Capabilities
    pub const LC_DEC_CAP: c_ulong = 1;

    unsafe extern "C" {
        pub fn NeAACDecGetCapabilities() -> c_ulong;
        pub fn NeAACDecOpen() -> NeAACDecHandle;
        pub fn NeAACDecClose(handle: NeAACDecHandle);
        pub fn NeAACDecGetCurrentConfiguration(handle: NeAACDecHandle) -> *mut NeAACDecConfiguration;
        pub fn NeAACDecSetConfiguration(handle: NeAACDecHandle, config: *mut NeAACDecConfiguration) -> c_uchar;
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

/// AAC decoder wrapper
pub struct AacDecoder {
    handle: ffi::NeAACDecHandle,
    frame_info: ffi::NeAACDecFrameInfo,
    pub sample_rate: u32,
    pub channels: u8,
}

/// Audio format info returned after initialization
#[derive(Debug, Clone)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u8,
}

impl AacDecoder {
    /// Create a new AAC decoder initialized with the given AudioSpecificConfig
    pub fn new(asc: &[u8]) -> Result<Self, String> {
        unsafe {
            let cap = ffi::NeAACDecGetCapabilities();
            if cap & ffi::LC_DEC_CAP == 0 {
                return Err("FAAD2: no LC decoding support".into());
            }

            let handle = ffi::NeAACDecOpen();
            if handle.is_null() {
                return Err("FAAD2: NeAACDecOpen failed".into());
            }

            // Configure
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

            // Init with ASC
            let mut output_sr: std::os::raw::c_ulong = 0;
            let mut output_ch: std::os::raw::c_uchar = 0;
            let result = ffi::NeAACDecInit2(
                handle,
                asc.as_ptr(),
                asc.len() as std::os::raw::c_ulong,
                &mut output_sr,
                &mut output_ch,
            );
            if result != 0 {
                let msg = ffi::NeAACDecGetErrorMessage((-result) as u8);
                let err = if msg.is_null() {
                    format!("FAAD2: init error {}", result)
                } else {
                    let cstr = std::ffi::CStr::from_ptr(msg);
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

    /// Decode one Access Unit. Returns PCM samples (interleaved i16).
    pub fn decode_frame(&mut self, data: &[u8]) -> Option<Vec<i16>> {
        unsafe {
            let mut buf = data.to_vec();
            let output = ffi::NeAACDecDecode(
                self.handle,
                &mut self.frame_info,
                buf.as_mut_ptr(),
                buf.len() as std::os::raw::c_ulong,
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
            let samples = std::slice::from_raw_parts(pcm_ptr, num_samples);
            Some(samples.to_vec())
        }
    }

    /// Get the current audio format
    pub fn audio_format(&self) -> AudioFormat {
        AudioFormat {
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }
}

impl Drop for AacDecoder {
    fn drop(&mut self) {
        unsafe {
            ffi::NeAACDecClose(self.handle);
        }
    }
}

// Safety: FAAD2 handle is not shared between threads
unsafe impl Send for AacDecoder {}
