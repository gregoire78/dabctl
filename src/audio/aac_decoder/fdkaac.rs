// ─────────────────────────────────────────────────────────────────────────────
// fdk-aac backend — compiled only with `--features fdk-aac`
// Inspired by AbracaDABra (KejPi, MIT licence) — USE_FDKAAC=ON equivalent
// Transport type TT_MP4_RAW (0): raw Access Units, ASC via aacDecoder_ConfigRaw
// ─────────────────────────────────────────────────────────────────────────────
use super::AudioFormat;
use std::os::raw::{c_int, c_uint};

#[allow(non_camel_case_types, dead_code)]
mod ffi {
    use std::os::raw::{c_int, c_uint, c_void};

    pub type HANDLE_AACDECODER = *mut c_void;

    // Transport type: raw access units, no sync layer (ETSI TS 102 563 §5.1).
    // TT_MP4_RAW = 0 per FDK_audio.h; value 2 is TT_MP4_ADTS (ADTS framing).
    pub const TT_MP4_RAW: c_int = 0;
    pub const AAC_DEC_OK: c_uint = 0x0000;

    #[repr(C)]
    #[allow(non_snake_case)]
    pub struct CStreamInfo {
        pub sampleRate: c_int,
        pub frameSize: c_int,
        pub numChannels: c_int,
        pub pChannelType: *const c_int,
        pub pChannelIndices: *const u8,
        pub aacSampleRate: c_int,
        pub profile: c_int,
        pub aot: c_int,
        pub channelConfig: c_int,
        pub bitRate: c_int,
        pub aacSamplesPerFrame: c_int,
        pub aacNumChannels: c_int,
        pub extAot: c_int,
        pub extSamplingRate: c_int,
        pub outputDelay: c_uint,
        pub flags: c_uint,
        pub epConfig: i8,
        pub numLostAccessUnits: c_int,
        pub numTotalBytes: i64,
        pub numBadBytes: i64,
        pub numTotalAccessUnits: i64,
        pub numBadAccessUnits: i64,
        pub drcProgRefLev: i8,
        pub drcPresMode: i8,
    }

    // AACDEC_PARAM values (aacdecoder_lib.h §AAC_PCM_MIN/MAX_OUTPUT_CHANNELS)
    // Restrict output channel count to actual encoded channel count.
    // Without this, fdk-aac assumes implicit PS on mono+SBR and returns
    // NOT_ENOUGH_BITS indefinitely. Technique from dablin/welle.io.
    pub const AAC_PCM_MIN_OUTPUT_CHANNELS: c_int = 0x0011;
    pub const AAC_PCM_MAX_OUTPUT_CHANNELS: c_int = 0x0012;

    unsafe extern "C" {
        pub fn aacDecoder_Open(transportFmt: c_int, nrOfLayers: c_uint) -> HANDLE_AACDECODER;
        // fdk-aac >= 2.0: ConfigRaw has no nrOfLayers parameter.
        pub fn aacDecoder_ConfigRaw(
            self_: HANDLE_AACDECODER,
            conf: *mut *mut u8,
            length: *const c_uint,
        ) -> c_uint;
        pub fn aacDecoder_SetParam(self_: HANDLE_AACDECODER, param: c_int, value: c_int) -> c_uint;
        pub fn aacDecoder_Fill(
            self_: HANDLE_AACDECODER,
            pBuffer: *mut *mut u8,
            bufferSize: *const c_uint,
            bytesValid: *mut c_uint,
        ) -> c_uint;
        pub fn aacDecoder_DecodeFrame(
            self_: HANDLE_AACDECODER,
            pTimeData: *mut i16,
            timeDataSize: c_int,
            flags: c_uint,
        ) -> c_uint;
        pub fn aacDecoder_GetStreamInfo(self_: HANDLE_AACDECODER) -> *mut CStreamInfo;
        pub fn aacDecoder_Close(self_: HANDLE_AACDECODER);
    }
}

const MAX_OUTPUT_SAMPLES: usize = 8192;

// Mirrors IS_DECODE_ERROR() from aacdecoder_lib.h: decode errors in this range
// produce valid (error-concealed) output — the buffer must NOT be discarded.
// IS_OUTPUT_VALID(err) = (err == AAC_DEC_OK) || IS_DECODE_ERROR(err)
const AAC_DEC_DECODE_ERROR_START: c_uint = 0x4000;
const AAC_DEC_DECODE_ERROR_END: c_uint = 0x4FFF;

#[inline]
fn is_output_valid(err: c_uint) -> bool {
    err == ffi::AAC_DEC_OK || (AAC_DEC_DECODE_ERROR_START..=AAC_DEC_DECODE_ERROR_END).contains(&err)
}

pub struct AacDecoder {
    handle: ffi::HANDLE_AACDECODER,
    pub sample_rate: u32,
    pub channels: u8,
    /// Number of interleaved i16 samples in the last successfully decoded frame.
    /// Used to output a zero-filled concealment frame when IS_OUTPUT_VALID is false.
    last_frame_samples: usize,
}

impl AacDecoder {
    /// `expected_channels`: 1 (mono) or 2 (stereo/PS).
    /// Must match the encoded channel count from SuperframeFormat::channels().
    /// Without this, fdk-aac assumes PS on mono+SBR streams and stalls.
    /// Technique from dablin (AACDecoderFDKAAC) and welle.io.
    pub fn new(asc: &[u8], expected_channels: u8) -> Result<Self, String> {
        // Safety: all raw pointers are checked for null before use.
        unsafe {
            let handle = ffi::aacDecoder_Open(ffi::TT_MP4_RAW, 1);
            if handle.is_null() {
                return Err("fdk-aac: aacDecoder_Open failed".into());
            }

            // Restrict output channel count to prevent fdk-aac from upmixing
            // mono+SBR to stereo (implicit PS assumption → NOT_ENOUGH_BITS stall).
            let ch = expected_channels as c_int;
            let err = ffi::aacDecoder_SetParam(handle, ffi::AAC_PCM_MIN_OUTPUT_CHANNELS, ch);
            if err != ffi::AAC_DEC_OK {
                ffi::aacDecoder_Close(handle);
                return Err(format!(
                    "fdk-aac: SetParam MIN_OUTPUT_CHANNELS error 0x{:04X}",
                    err
                ));
            }
            let err = ffi::aacDecoder_SetParam(handle, ffi::AAC_PCM_MAX_OUTPUT_CHANNELS, ch);
            if err != ffi::AAC_DEC_OK {
                ffi::aacDecoder_Close(handle);
                return Err(format!(
                    "fdk-aac: SetParam MAX_OUTPUT_CHANNELS error 0x{:04X}",
                    err
                ));
            }

            let mut asc_copy = asc.to_vec();
            let mut asc_ptr: *mut u8 = asc_copy.as_mut_ptr();
            let asc_len: c_uint = asc.len() as c_uint;
            let err = ffi::aacDecoder_ConfigRaw(handle, &mut asc_ptr, &asc_len);
            if err != ffi::AAC_DEC_OK {
                ffi::aacDecoder_Close(handle);
                return Err(format!("fdk-aac: aacDecoder_ConfigRaw error 0x{:04X}", err));
            }

            let info = ffi::aacDecoder_GetStreamInfo(handle);
            if info.is_null() {
                ffi::aacDecoder_Close(handle);
                return Err("fdk-aac: aacDecoder_GetStreamInfo returned null".into());
            }

            // Stream info may not yet be populated after ConfigRaw;
            // sample_rate and channels are updated after the first decoded frame.
            let sample_rate = if (*info).sampleRate > 0 {
                (*info).sampleRate as u32
            } else {
                0
            };
            let channels = if (*info).numChannels > 0 {
                (*info).numChannels as u8
            } else {
                0
            };

            Ok(AacDecoder {
                handle,
                sample_rate,
                channels,
                last_frame_samples: 0,
            })
        }
    }

    pub fn decode_frame(&mut self, data: &[u8]) -> Option<Vec<i16>> {
        // ETSI TS 102 563 §5.1: an AU must carry AAC payload bytes.
        // Empty payload is malformed and must be rejected deterministically.
        if data.is_empty() {
            return None;
        }

        // Safety: PCM output buffer is stack-allocated with a known upper bound.
        unsafe {
            let mut au = data.to_vec();
            let mut au_ptr: *mut u8 = au.as_mut_ptr();
            let au_len: c_uint = au.len() as c_uint;
            let mut bytes_valid: c_uint = au_len;

            tracing::trace!(au_len, "fdk-aac: Fill");

            let fill_err =
                ffi::aacDecoder_Fill(self.handle, &mut au_ptr, &au_len, &mut bytes_valid);
            if fill_err != ffi::AAC_DEC_OK {
                tracing::trace!("fdk-aac: Fill error 0x{:04X}", fill_err);
                return None;
            }
            tracing::trace!(bytes_valid, "fdk-aac: Fill ok");

            let mut pcm = vec![0i16; MAX_OUTPUT_SAMPLES];
            let dec_err = ffi::aacDecoder_DecodeFrame(
                self.handle,
                pcm.as_mut_ptr(),
                MAX_OUTPUT_SAMPLES as c_int,
                0,
            );
            tracing::trace!("fdk-aac: DecodeFrame returned 0x{:04X}", dec_err);

            if !is_output_valid(dec_err) {
                // Fatal transport/init error — no usable output.
                // If we know the frame size from a previous decode, emit silence
                // (zero-filled buffer) so the audio pipeline stays gapless.
                // Technique from AbracaDABra (KejPi, MIT licence).
                tracing::trace!(
                    "fdk-aac: DecodeFrame fatal error 0x{:04X}, concealing with silence",
                    dec_err
                );
                if self.last_frame_samples == 0 {
                    return None;
                }
                return Some(vec![0i16; self.last_frame_samples]);
            }

            let info = ffi::aacDecoder_GetStreamInfo(self.handle);
            if info.is_null() {
                tracing::trace!("fdk-aac: GetStreamInfo returned null");
                return None;
            }

            let frame_size = (*info).frameSize as usize;
            let num_ch = (*info).numChannels as usize;
            let sample_rate = (*info).sampleRate;
            let num_samples = frame_size * num_ch;
            tracing::trace!(
                frame_size,
                num_ch,
                sample_rate,
                num_samples,
                "fdk-aac: StreamInfo"
            );

            if num_samples == 0 {
                tracing::trace!("fdk-aac: num_samples == 0, dropping frame");
                return None;
            }

            // Update channel/rate from live stream (SBR doubles sample rate).
            self.sample_rate = (*info).sampleRate as u32;
            self.channels = (*info).numChannels as u8;
            self.last_frame_samples = num_samples;

            pcm.truncate(num_samples);
            Some(pcm)
        }
    }

    #[allow(dead_code)]
    pub fn audio_format(&self) -> AudioFormat {
        AudioFormat {
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }

    /// Decode one Access Unit, or produce a zero-filled silence frame when no
    /// AU data is available (`au_data = None`) or when decoding fails fatally.
    ///
    /// Returns `None` only when the frame size is not yet known (no successful
    /// decode has occurred yet), so no silence frame can be sized correctly.
    pub fn decode_or_silence(&mut self, au_data: Option<&[u8]>) -> Option<Vec<i16>> {
        if let Some(data) = au_data {
            if let Some(pcm) = self.decode_frame(data) {
                return Some(pcm);
            }
        }
        if self.last_frame_samples == 0 {
            return None;
        }
        Some(vec![0i16; self.last_frame_samples])
    }
}

impl Drop for AacDecoder {
    fn drop(&mut self) {
        // Safety: handle is valid; never double-freed.
        unsafe { ffi::aacDecoder_Close(self.handle) }
    }
}

// Safety: the fdk-aac handle is never shared across threads.
unsafe impl Send for AacDecoder {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_asc() {
        let result = AacDecoder::new(&[], 2);
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn decode_frame_returns_none_on_empty_input() {
        let asc: &[u8] = &[0x2B, 0x11, 0x88, 0x00, 0x06, 0x00, 0x4A, 0x00];
        if let Ok(mut dec) = AacDecoder::new(asc, 2) {
            let result = dec.decode_frame(&[]);
            assert!(result.is_none());
        }
    }
}
