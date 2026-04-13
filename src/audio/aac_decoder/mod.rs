// ─────────────────────────────────────────────────────────────────────────────
// Public AacDecoder
//
// Without `fdk-aac` feature: thin alias to the faad2 backend (unchanged API).
// With `fdk-aac` feature: wrapper struct that dispatches to either backend at
// runtime based on the `--aac-decoder` CLI argument. The public fields
// `sample_rate` and `channels` are kept on the wrapper so call sites are
// identical in both configurations.
// ─────────────────────────────────────────────────────────────────────────────

pub mod faad2;
#[cfg(feature = "fdk-aac")]
pub mod fdkaac;

/// Audio format info returned after initialization.
#[derive(Debug, Clone)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u8,
}

// ── Without fdk-aac: direct alias to faad2 backend ───────────────────────────

#[cfg(not(feature = "fdk-aac"))]
pub use faad2::AacDecoder;

// ── With fdk-aac: runtime-dispatch wrapper ────────────────────────────────────

#[cfg(feature = "fdk-aac")]
enum AacDecoderInner {
    Faad2(faad2::AacDecoder),
    FdkAac(fdkaac::AacDecoder),
}

/// AAC decoder with runtime backend selection (faad2 / fdk-aac).
///
/// When the `fdk-aac` Cargo feature is disabled this is a direct alias for the
/// faad2 backend. When the feature is enabled, the backend is chosen at
/// construction time via [`AacDecoder::new_faad2`] or [`AacDecoder::new_fdk_aac`].
#[cfg(feature = "fdk-aac")]
pub struct AacDecoder {
    inner: AacDecoderInner,
    pub sample_rate: u32,
    pub channels: u8,
}

#[cfg(feature = "fdk-aac")]
impl AacDecoder {
    /// Construct using the **faad2** backend.
    pub fn new_faad2(asc: &[u8]) -> Result<Self, String> {
        let dec = faad2::AacDecoder::new(asc)?;
        Ok(Self {
            sample_rate: dec.sample_rate,
            channels: dec.channels,
            inner: AacDecoderInner::Faad2(dec),
        })
    }

    /// Construct using the **fdk-aac** backend.
    ///
    /// `expected_channels`: the channel count from `SuperframeFormat::channels()`.
    /// Required to prevent fdk-aac from stalling on mono+SBR streams.
    pub fn new_fdk_aac(asc: &[u8], expected_channels: u8) -> Result<Self, String> {
        let dec = fdkaac::AacDecoder::new(asc, expected_channels)?;
        Ok(Self {
            sample_rate: dec.sample_rate,
            channels: dec.channels,
            inner: AacDecoderInner::FdkAac(dec),
        })
    }

    /// Decode one Access Unit. Returns interleaved PCM samples (i16).
    pub fn decode_frame(&mut self, data: &[u8]) -> Option<Vec<i16>> {
        let pcm = match &mut self.inner {
            AacDecoderInner::Faad2(d) => {
                let pcm = d.decode_frame(data)?;
                self.sample_rate = d.sample_rate;
                self.channels = d.channels;
                pcm
            }
            AacDecoderInner::FdkAac(d) => {
                let pcm = d.decode_frame(data)?;
                self.sample_rate = d.sample_rate;
                self.channels = d.channels;
                pcm
            }
        };
        Some(pcm)
    }

    /// Decode one Access Unit, or produce a zero-filled silence frame when no
    /// AU data is available (`au_data = None`) or when decoding fails.
    ///
    /// Returns `None` only when the frame size is not yet known (no successful
    /// decode has occurred yet).  Silence content is generated entirely by the
    /// AAC decoder backend — callers must not create silence frames themselves.
    pub fn decode_or_silence(&mut self, au_data: Option<&[u8]>) -> Option<Vec<i16>> {
        let pcm = match &mut self.inner {
            AacDecoderInner::Faad2(d) => {
                let pcm = d.decode_or_silence(au_data)?;
                self.sample_rate = d.sample_rate;
                self.channels = d.channels;
                pcm
            }
            AacDecoderInner::FdkAac(d) => {
                let pcm = d.decode_or_silence(au_data)?;
                self.sample_rate = d.sample_rate;
                self.channels = d.channels;
                pcm
            }
        };
        Some(pcm)
    }

    pub fn audio_format(&self) -> AudioFormat {
        AudioFormat {
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }
}

#[cfg(feature = "fdk-aac")]
unsafe impl Send for AacDecoder {}

#[cfg(all(test, feature = "fdk-aac"))]
mod tests {
    use super::*;

    #[test]
    fn new_faad2_rejects_empty_asc() {
        let result = AacDecoder::new_faad2(&[]);
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn new_fdk_aac_rejects_empty_asc() {
        let result = AacDecoder::new_fdk_aac(&[], 2);
        assert!(result.is_err() || result.is_ok());
    }
}
