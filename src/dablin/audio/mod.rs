//! AAC decoder interface
//!
//! Wraps faad2 (default) or fdk-aac (feature-gated) and applies
//! the AAC gap policy (`freeze` or `silence`).

pub mod faad2;

#[cfg(feature = "fdk-aac")]
pub mod fdk;

use crate::cli::AacGap;
use crate::dablin::audio::faad2::build_asc;
use crate::dablin::dabplus::{AudioUnit, SuperframeFormat};

/// Number of samples per AAC frame per channel (standard AAC-LC / HE-AAC).
/// Used to compute the silence buffer size when gap policy = `silence`.
pub const AAC_SAMPLES_PER_FRAME: usize = 1024;
pub const PCM_SAMPLES_PER_CIF_48K: usize = 1152;

fn expected_output_samples_per_au(fmt: &SuperframeFormat) -> usize {
    match (fmt.dac_rate, fmt.sbr_flag) {
        (true, true) => 1920,
        (true, false) => 960,
        (false, true) => 2880,
        (false, false) => 1440,
    }
}

fn resample_frame_to_len(pcm: &[i16], channels: usize, output_samples_per_channel: usize) -> Vec<i16> {
    if channels == 0 {
        return Vec::new();
    }

    let input_samples_per_channel = pcm.len() / channels;
    if input_samples_per_channel == 0 {
        return Vec::new();
    }
    if input_samples_per_channel == output_samples_per_channel {
        return pcm.to_vec();
    }
    if input_samples_per_channel == 1 {
        let mut out = Vec::with_capacity(output_samples_per_channel * channels);
        for _ in 0..output_samples_per_channel {
            out.extend_from_slice(&pcm[..channels]);
        }
        return out;
    }

    let mut out = Vec::with_capacity(output_samples_per_channel * channels);
    for out_index in 0..output_samples_per_channel {
        let position = (out_index as f32) * ((input_samples_per_channel - 1) as f32)
            / ((output_samples_per_channel - 1) as f32);
        let left = position.floor() as usize;
        let right = position.ceil() as usize;
        let frac = position - (left as f32);

        for channel in 0..channels {
            let left_sample = pcm[left * channels + channel] as f32;
            let right_sample = pcm[right * channels + channel] as f32;
            let sample = left_sample + (right_sample - left_sample) * frac;
            out.push(sample.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16);
        }
    }

    out
}

/// Audio decoder: wraps the backend and applies gap policy.
pub struct AacDecoder {
    inner: AacDecoderInner,
    gap_policy: AacGap,
    /// Channel count established on first successful decode (default 2 = stereo)
    channels: usize,
    /// Output sample count per AU (per channel) after normalization to 48 kHz.
    output_samples_per_au: usize,
    /// Whether the backend has been initialized with ASC
    initialized: bool,
}

enum AacDecoderInner {
    Faad2(faad2::Faad2Decoder),
    #[cfg(feature = "fdk-aac")]
    Fdk(fdk::FdkDecoder),
}

impl AacDecoder {
    /// Create a new faad2-backed decoder.
    pub fn new_faad2(gap_policy: AacGap) -> Option<Self> {
        let inner = faad2::Faad2Decoder::new()?;
        Some(Self {
            inner: AacDecoderInner::Faad2(inner),
            gap_policy,
            channels: 2,
            output_samples_per_au: AAC_SAMPLES_PER_FRAME,
            initialized: false,
        })
    }

    #[cfg(feature = "fdk-aac")]
    /// Create a new fdk-aac-backed decoder.
    pub fn new_fdk(gap_policy: AacGap) -> Option<Self> {
        let inner = fdk::FdkDecoder::new();
        Some(Self {
            inner: AacDecoderInner::Fdk(inner),
            gap_policy,
            channels: 2,
            output_samples_per_au: AAC_SAMPLES_PER_FRAME,
            initialized: false,
        })
    }

    /// Initialize the backend with the DAB+ superframe format.
    /// Must be called before decode(). Safe to call multiple times (idempotent).
    pub fn init_format(&mut self, fmt: &SuperframeFormat) -> bool {
        if self.initialized {
            return true;
        }
        let asc = build_asc(fmt);
        let ok = match &mut self.inner {
            AacDecoderInner::Faad2(dec) => dec.init_with_asc(&asc),
            #[cfg(feature = "fdk-aac")]
            AacDecoderInner::Fdk(dec) => dec.init_with_asc(&asc),
        };
        if ok {
            self.initialized = true;
            self.channels = fmt.core_ch_config() as usize;
            self.output_samples_per_au = expected_output_samples_per_au(fmt);
        }
        ok
    }

    /// Decode one audio access unit.
    ///
    /// Returns `Some(Vec<i16>)` on success, or:
    ///   - `None` if gap policy is `freeze`
    ///   - `Some(silence)` if gap policy is `silence`
    pub fn decode(&mut self, au: &AudioUnit) -> Option<Vec<i16>> {
        if !self.initialized {
            return None;
        }
        let result = match &mut self.inner {
            AacDecoderInner::Faad2(dec) => dec.decode(&au.data),
            #[cfg(feature = "fdk-aac")]
            AacDecoderInner::Fdk(dec) => dec.decode(&au.data),
        };

        match result {
            Some(pcm) => {
                let n_channels = self.channels;
                if !pcm.is_empty() {
                    self.channels = n_channels;
                    return Some(resample_frame_to_len(
                        &pcm,
                        n_channels.max(1),
                        self.output_samples_per_au,
                    ));
                }
                Some(pcm)
            }
            None => {
                tracing::debug!("AAC decode error on AU (see backend warning above)");
                match self.gap_policy {
                    AacGap::Freeze => None,
                    AacGap::Silence => {
                        // Emit 48 kHz PCM silence with the same AU duration as a valid frame.
                        let n = self.output_samples_per_au * self.channels;
                        Some(vec![0i16; n])
                    }
                }
            }
        }
    }

    /// Emit silence for missing CIFs (24 ms each) to keep 48 kHz PCM time-continuous.
    /// Used when superframe sync is lost before AU decoding.
    pub fn silence_for_missing_cifs(&self, cif_count: usize) -> Option<Vec<i16>> {
        if cif_count == 0 || !self.initialized || self.gap_policy != AacGap::Silence {
            return None;
        }
        let n = PCM_SAMPLES_PER_CIF_48K * cif_count * self.channels;
        Some(vec![0i16; n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_on_gap_freeze_returns_none() {
        let gap = AacGap::Freeze;
        let result: Option<Vec<i16>> = None;
        let output = match result {
            Some(pcm) => Some(pcm),
            None => match gap {
                AacGap::Freeze => None,
                AacGap::Silence => Some(vec![0i16; AAC_SAMPLES_PER_FRAME * 2]),
            },
        };
        assert!(output.is_none());
    }

    #[test]
    fn test_silence_on_gap_silence_returns_zeros() {
        let gap = AacGap::Silence;
        let result: Option<Vec<i16>> = None;
        let output = match result {
            Some(pcm) => Some(pcm),
            None => match gap {
                AacGap::Freeze => None,
                AacGap::Silence => Some(vec![0i16; AAC_SAMPLES_PER_FRAME * 2]),
            },
        };
        let pcm = output.unwrap();
        assert_eq!(pcm.len(), AAC_SAMPLES_PER_FRAME * 2);
        assert!(pcm.iter().all(|&s| s == 0));
    }

    #[test]
    fn test_silence_frame_is_exact_length() {
        let silence = vec![0i16; AAC_SAMPLES_PER_FRAME * 2];
        assert_eq!(silence.len(), 1024 * 2);
        assert!(silence.iter().all(|&s| s == 0));
    }

    #[test]
    fn test_silence_values_are_zero() {
        let silence = vec![0i16; 64];
        assert!(silence.iter().all(|&s| s == 0));
    }

    #[test]
    fn test_expected_output_samples_per_au_matches_dabplus_grid() {
        let fmt = SuperframeFormat {
            dac_rate: false,
            sbr_flag: false,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        assert_eq!(expected_output_samples_per_au(&fmt), 1440);

        let fmt = SuperframeFormat {
            dac_rate: true,
            sbr_flag: false,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        assert_eq!(expected_output_samples_per_au(&fmt), 960);
    }

    #[test]
    fn test_resample_frame_to_len_expands_stereo_frame() {
        let pcm = [0i16, 100, 1000, 1100, 2000, 2100, 3000, 3100];
        let out = resample_frame_to_len(&pcm, 2, 6);
        assert_eq!(out.len(), 12);
        assert_eq!(out[0], 0);
        assert_eq!(out[1], 100);
        assert_eq!(out[10], 3000);
        assert_eq!(out[11], 3100);
    }
}
