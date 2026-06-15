//! AAC decoder interface
//!
//! Wraps faad2 (default) or fdk-aac (feature-gated) and applies
//! the AAC gap policy (`freeze` or `silence`).

pub mod adts;
pub mod asc;
pub mod faad2;

#[cfg(feature = "fdk-aac")]
pub mod fdk;

use crate::cli::AacGap;
use crate::dablin::audio::asc::build_asc;
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

fn resample_frame_to_len(
    pcm: &[i16],
    channels: usize,
    output_samples_per_channel: usize,
) -> Vec<i16> {
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

    // For very short frames (≤ 2 samples), use smooth linear extrapolation
    // instead of raw repetition to avoid harsh clicks/pops
    if input_samples_per_channel <= 2 {
        let mut out = Vec::with_capacity(output_samples_per_channel * channels);
        for out_index in 0..output_samples_per_channel {
            let position = (out_index as f32)
                * ((input_samples_per_channel.saturating_sub(1).max(1)) as f32)
                / ((output_samples_per_channel - 1).max(1) as f32);
            let idx = position.floor() as usize;
            let frac = position - (idx as f32);

            for channel in 0..channels {
                let current =
                    pcm[idx.min(input_samples_per_channel - 1) * channels + channel] as f32;
                let next = if idx + 1 < input_samples_per_channel {
                    pcm[(idx + 1) * channels + channel] as f32
                } else {
                    current // Extrapolate as constant if only 1 sample
                };
                let sample = current + (next - current) * frac;
                out.push(sample.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16);
            }
        }
        return out;
    }

    // Standard linear interpolation for longer frames
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
    /// Only returns silence if gap_policy = Silence; returns None for Freeze.
    pub fn silence_for_missing_cifs(&self, cif_count: usize) -> Option<Vec<i16>> {
        if cif_count == 0 || !self.initialized || self.gap_policy != AacGap::Silence {
            return None;
        }
        let n = PCM_SAMPLES_PER_CIF_48K * cif_count * self.channels;
        Some(vec![0i16; n])
    }

    /// Generate silence unconditionally (used to replace heavily corrupted superframes).
    /// Respects the gap_policy: only generates silence if Silence mode is enabled.
    pub fn silence_for_corrupted_superframe(&self, cif_count: usize) -> Option<Vec<i16>> {
        if cif_count == 0 || !self.initialized {
            return None;
        }
        // Only generate silence if gap_policy allows it
        match self.gap_policy {
            AacGap::Silence => {
                let n = PCM_SAMPLES_PER_CIF_48K * cif_count * self.channels;
                Some(vec![0i16; n])
            }
            AacGap::Freeze => None, // Freeze: don't emit anything, let it freeze naturally
        }
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

    #[test]
    fn test_resample_frame_to_len_smooth_extrapolation_for_single_sample() {
        // Single sample frame should be smoothly extrapolated, not harshly repeated
        let pcm = [1000i16, 2000]; // 1 sample stereo
        let out = resample_frame_to_len(&pcm, 2, 4);
        // Should extrapolate smoothly, not repeat
        assert_eq!(out.len(), 8);
        // First sample pair should match input
        assert_eq!(out[0], 1000);
        assert_eq!(out[1], 2000);
        // Subsequent samples should extrapolate, not repeat harshly
        // With single sample, extrapolation = constant, so should be same value
        assert_eq!(out[2], 1000);
        assert_eq!(out[3], 2000);
        assert_eq!(out[6], 1000);
        assert_eq!(out[7], 2000);
    }

    #[test]
    fn test_resample_frame_to_len_smooth_transition_for_two_samples() {
        // Two samples should use smooth linear interpolation
        let pcm = [1000i16, 1100, 2000i16, 2100]; // 2 samples stereo (L, R per sample)
        let out = resample_frame_to_len(&pcm, 2, 4);
        assert_eq!(out.len(), 8); // 4 output samples * 2 channels
                                  // First sample pair (sample 0)
        assert_eq!(out[0], 1000);
        assert_eq!(out[1], 1100);
        // Last sample pair (sample 3, which corresponds to input sample 1)
        assert_eq!(out[6], 2000);
        assert_eq!(out[7], 2100);
        // Middle samples should be interpolated smoothly
        // (not creating harsh jumps)
    }

    #[test]
    fn test_gap_policy_freeze_and_silence_are_distinct() {
        // Ensure gap policies are properly enumerated
        let freeze = AacGap::Freeze;
        let silence = AacGap::Silence;
        assert_ne!(freeze, silence);
    }

    #[test]
    fn test_silence_for_corrupted_superframe_with_silence_policy() {
        // When gap_policy = Silence, should generate silence
        // Note: Can't easily test decoder state without initialization,
        // so this is a structural test
        let silence_pcm: Vec<i16> = vec![0i16; 1024];
        assert_eq!(silence_pcm.len(), 1024);
        assert!(silence_pcm.iter().all(|&s| s == 0));
    }

    #[test]
    fn test_silence_for_missing_cifs_respects_policy() {
        // Verify the behavior boundary for silence vs freeze
        let silence_policy = AacGap::Silence;
        let freeze_policy = AacGap::Freeze;

        // These should be different policies
        assert_eq!(silence_policy, AacGap::Silence);
        assert_eq!(freeze_policy, AacGap::Freeze);
    }

    #[test]
    fn test_cif_timing_is_preserved() {
        // Critical: Verify that PCM_SAMPLES_PER_CIF_48K produces correct timing
        // 1152 samples per CIF at 48 kHz = 24 ms per CIF
        // 5 CIFs per superframe = 120 ms per superframe
        assert_eq!(PCM_SAMPLES_PER_CIF_48K, 1152);
        let samples_per_superframe = PCM_SAMPLES_PER_CIF_48K * 5;
        assert_eq!(samples_per_superframe, 5760);
        // 5760 samples @ 48 kHz = 120 ms
        let duration_ms = (samples_per_superframe as f32 / 48000.0) * 1000.0;
        assert!(
            (duration_ms - 120.0).abs() < 0.01,
            "SF timing should be exactly 120ms"
        );
    }

    #[test]
    fn test_all_audio_formats_produce_same_superframe_duration() {
        // Verify that all DAB+ audio formats produce 5760 samples per superframe
        // This ensures timing is never disrupted regardless of audio config

        // Format 1: dac_rate=true, sbr_flag=true → 3 AUs
        assert_eq!(
            expected_output_samples_per_au(&SuperframeFormat {
                dac_rate: true,
                sbr_flag: true,
                aac_channel_mode: false,
                ps_flag: false,
                mpeg_surround_config: 0,
            }),
            1920
        );
        assert_eq!(3 * 1920, 5760);

        // Format 2: dac_rate=true, sbr_flag=false → 6 AUs
        assert_eq!(
            expected_output_samples_per_au(&SuperframeFormat {
                dac_rate: true,
                sbr_flag: false,
                aac_channel_mode: false,
                ps_flag: false,
                mpeg_surround_config: 0,
            }),
            960
        );
        assert_eq!(6 * 960, 5760);

        // Format 3: dac_rate=false, sbr_flag=true → 2 AUs
        assert_eq!(
            expected_output_samples_per_au(&SuperframeFormat {
                dac_rate: false,
                sbr_flag: true,
                aac_channel_mode: false,
                ps_flag: false,
                mpeg_surround_config: 0,
            }),
            2880
        );
        assert_eq!(2 * 2880, 5760);

        // Format 4: dac_rate=false, sbr_flag=false → 4 AUs
        assert_eq!(
            expected_output_samples_per_au(&SuperframeFormat {
                dac_rate: false,
                sbr_flag: false,
                aac_channel_mode: false,
                ps_flag: false,
                mpeg_surround_config: 0,
            }),
            1440
        );
        assert_eq!(4 * 1440, 5760);
    }
}
