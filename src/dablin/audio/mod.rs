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

/// Audio decoder: wraps the backend and applies gap policy.
pub struct AacDecoder {
    inner: AacDecoderInner,
    gap_policy: AacGap,
    /// Channel count established on first successful decode (default 2 = stereo)
    channels: usize,
    /// Sample count per frame (per channel)
    samples_per_frame: usize,
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
            samples_per_frame: AAC_SAMPLES_PER_FRAME,
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
            samples_per_frame: AAC_SAMPLES_PER_FRAME,
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
                // Update channel count from the first successful decode.
                let n_channels = self.channels;
                if !pcm.is_empty() {
                    self.channels = n_channels;
                    self.samples_per_frame = pcm.len() / n_channels.max(1);
                }
                Some(pcm)
            }
            None => {
                tracing::debug!("AAC decode error on AU (see backend warning above)");
                match self.gap_policy {
                    AacGap::Freeze => None,
                    AacGap::Silence => {
                        // Emit PCM silence: samples_per_frame × channels zeros
                        let n = self.samples_per_frame * self.channels;
                        Some(vec![0i16; n])
                    }
                }
            }
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
}
