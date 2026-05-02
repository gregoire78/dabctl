//! DAB+ super frame processing pipeline
//! Reference: ETSI TS 102 563
//!
//! Pipeline:
//!   raw sub-channel bytes (5 CIFs)
//!     -> FireCode sync check (after RS)
//!     -> RS(120, 110) error correction
//!     -> AU (Audio Unit) extraction
//!     -> AAC decoder

pub mod firecode;
pub mod rs_decoder;

use crate::dablin::dabplus::firecode::check_firecode;
use crate::dablin::dabplus::rs_decoder::rs_decode_superframe;

/// One decoded audio access unit (AU), ready for AAC decoding.
#[derive(Debug)]
pub struct AudioUnit {
    /// Raw AAC bitstream bytes (AU data without length prefix)
    pub data: Vec<u8>,
}

/// DAB+ superframe audio format (parsed from sf[2]).
/// Matches dablin SuperframeFormat.
#[derive(Debug, Clone, PartialEq)]
pub struct SuperframeFormat {
    pub dac_rate: bool,
    pub sbr_flag: bool,
    pub aac_channel_mode: bool,
    pub ps_flag: bool,
    pub mpeg_surround_config: u8,
}

impl SuperframeFormat {
    /// Core sample rate index for AAC (used in AudioSpecificConfig)
    pub fn core_sr_index(&self) -> u8 {
        match (self.dac_rate, self.sbr_flag) {
            (true, true) => 6,   // 24 kHz
            (true, false) => 3,  // 48 kHz
            (false, true) => 8,  // 16 kHz
            (false, false) => 5, // 32 kHz
        }
    }

    /// Core channel config (1=mono, 2=stereo)
    pub fn core_ch_config(&self) -> u8 {
        if self.aac_channel_mode || self.ps_flag {
            2
        } else {
            1
        }
    }

    /// Extension (SBR) sample rate index
    pub fn ext_sr_index(&self) -> u8 {
        if self.dac_rate {
            3
        } else {
            5
        } // 48 or 32 kHz
    }
}

/// Result of processing one DAB+ super frame
#[derive(Debug)]
pub struct SuperframeResult {
    /// Successfully extracted AUs
    pub units: Vec<AudioUnit>,
    /// Number of RS codewords corrected
    pub rs_corrected: usize,
    /// Whether the FireCode check passed
    pub firecode_ok: bool,
    /// Audio format (only valid when firecode_ok = true)
    pub format: Option<SuperframeFormat>,
}

/// Process a DAB+ super frame buffer in-place (5 CIFs of sub-channel data).
///
/// `sf` MUST be a copy owned by the caller — RS correction modifies it in place.
/// Matches dablin's SuperframeFilter::Feed() logic:
///   1. Validate size: sf_len % 120 == 0
///   2. RS decode in-place (subch_index = sf_len / 120)
///   3. FireCode check on RS-decoded bytes
///   4. AU extraction
pub fn process_superframe_inplace(sf: &mut [u8]) -> SuperframeResult {
    let sf_len = sf.len();
    if sf_len == 0 || !sf_len.is_multiple_of(120) {
        tracing::warn!("DAB+ superframe size {} not divisible by 120", sf_len);
        return SuperframeResult {
            units: Vec::new(),
            rs_corrected: 0,
            firecode_ok: false,
            format: None,
        };
    }

    // 1. Reed-Solomon decoding in-place (caller already owns the buffer)
    let rs_corrected = match rs_decode_superframe(sf) {
        Ok(n) => n,
        Err(n) => {
            tracing::debug!("RS: {} uncorrectable codewords, continuing best-effort", n);
            0
        }
    };

    // 2. FireCode check AFTER RS (matches dablin CheckSync())
    let firecode_ok = check_firecode(sf);
    if !firecode_ok {
        tracing::debug!("DAB+ FireCode mismatch after RS - dropping super frame");
        tracing::debug!("  sf[0..11] after RS: {:02X?}", &sf[..11.min(sf.len())]);
        return SuperframeResult {
            units: Vec::new(),
            rs_corrected,
            firecode_ok: false,
            format: None,
        };
    }

    // 3. Extract audio access units
    let format = SuperframeFormat {
        dac_rate: (sf[2] & 0x40) != 0,
        sbr_flag: (sf[2] & 0x20) != 0,
        aac_channel_mode: (sf[2] & 0x10) != 0,
        ps_flag: (sf[2] & 0x08) != 0,
        mpeg_surround_config: sf[2] & 0x07,
    };
    let units = extract_audio_units_dablin(sf, sf_len);

    SuperframeResult {
        units,
        rs_corrected,
        firecode_ok: true,
        format: Some(format),
    }
}

/// Extract audio units from RS-decoded superframe (dablin header format).
///
/// sf[0..1]: FireCode (validated)
/// sf[2]: format byte (dac_rate=bit6, sbr_flag=bit5, ...)
/// sf[3..]: AU start offsets (12-bit packed)
///
/// num_aus and au_start[0] from dac_rate/sbr_flag:
///   (true,  true)  -> 3 AUs, au_start[0]=6
///   (true,  false) -> 6 AUs, au_start[0]=11
///   (false, true)  -> 2 AUs, au_start[0]=5
///   (false, false) -> 4 AUs, au_start[0]=8
fn extract_audio_units_dablin(sf: &[u8], sf_len: usize) -> Vec<AudioUnit> {
    if sf.len() < 11 {
        return Vec::new();
    }

    let dac_rate = (sf[2] & 0x40) != 0;
    let sbr_flag = (sf[2] & 0x20) != 0;

    let (num_aus, au_start_0) = match (dac_rate, sbr_flag) {
        (true, true) => (3usize, 6usize),
        (true, false) => (6usize, 11usize),
        (false, true) => (2usize, 5usize),
        (false, false) => (4usize, 8usize),
    };

    // pseudo-end: sf_len / 120 * 110
    let au_end = sf_len / 120 * 110;

    let mut au_start = vec![0usize; num_aus + 1];
    au_start[0] = au_start_0;
    au_start[num_aus] = au_end;

    if num_aus >= 2 {
        au_start[1] = (sf[3] as usize) << 4 | (sf[4] as usize) >> 4;
    }
    if num_aus >= 3 {
        au_start[2] = ((sf[4] & 0x0F) as usize) << 8 | sf[5] as usize;
    }
    if num_aus >= 4 {
        au_start[3] = (sf[6] as usize) << 4 | (sf[7] as usize) >> 4;
    }
    if num_aus >= 5 {
        au_start[4] = ((sf[7] & 0x0F) as usize) << 8 | sf[8] as usize;
    }
    if num_aus >= 6 {
        au_start[5] = (sf[9] as usize) << 4 | (sf[10] as usize) >> 4;
    }

    // simple plausibility: offsets must be increasing
    for i in 0..num_aus {
        if au_start[i] >= au_start[i + 1] {
            tracing::debug!(
                "AU plausibility failed: au_start[{}]={} >= [{}]={}",
                i,
                au_start[i],
                i + 1,
                au_start[i + 1]
            );
            return Vec::new();
        }
    }

    let mut units = Vec::with_capacity(num_aus);
    for i in 0..num_aus {
        let start = au_start[i];
        let end = au_start[i + 1];
        // each AU ends with 2-byte CRC (ETSI TS 102 563)
        if end > start + 2 && end <= sf.len() {
            units.push(AudioUnit {
                data: sf[start..end - 2].to_vec(),
            });
        }
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_superframe_bad_firecode() {
        // Super frame with size not divisible by 120 → early rejection
        let mut sf = vec![0u8; 121];
        let result = process_superframe_inplace(&mut sf);
        assert!(!result.firecode_ok);
        assert!(result.units.is_empty());
    }

    #[test]
    fn test_process_superframe_valid_firecode() {
        // Build a super frame with correct FireCode
        // All-zero data -> FireCode CRC of bytes[2..11] = 0 -> stored 0x0000 matches
        let mut sf = vec![0u8; 1320]; // 5 x 264 bytes (STL=33)
        sf[0] = 0x00;
        sf[1] = 0x00;
        let result = process_superframe_inplace(&mut sf);
        assert!(result.firecode_ok);
    }
}
