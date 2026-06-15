//! AAC AudioSpecificConfig builder shared across audio outputs/backends.

use crate::dablin::dabplus::SuperframeFormat;

/// Build the AudioSpecificConfig matching dablin's AACDecoder constructor.
///
/// Reference: dablin AACDecoder::AACDecoder() in dabplus_decoder.cpp
/// Format: AAC-LC with 960-sample transform (GASpecificConfig window = 1)
/// Extended with SBR/PS when applicable.
pub fn build_asc(fmt: &SuperframeFormat) -> Vec<u8> {
    let mut asc = Vec::with_capacity(7);

    // AudioObjectType = 2 (AAC-LC) → 5 bits = 0b00010
    // CoreSrIndex → 4 bits
    // CoreChConfig → 4 bits
    // GASpecificConfig: frameLengthFlag=1 (960), dependsOnCoreCoder=0, extensionFlag=0 → 3 bits
    // Total first two bytes: 00010|xxxx|xxxx|100
    let sr = fmt.core_sr_index();
    let ch = fmt.core_ch_config();

    asc.push(0b00010 << 3 | sr >> 1);
    asc.push((sr & 0x01) << 7 | ch << 3 | 0b100);

    if fmt.sbr_flag {
        // Explicit backwards-compatible SBR signaling
        // syncExtensionType = 0x2B7 (11 bits) → AudioObjectType 5 (SBR) → SBR present
        asc.push(0x56);
        asc.push(0xE5);
        asc.push(0x80 | (fmt.ext_sr_index() << 3));

        if fmt.ps_flag {
            // PS present
            *asc.last_mut().expect("ASC has at least one byte") |= 0x05;
            asc.push(0x48);
            asc.push(0x80);
        }
    }

    asc
}

#[cfg(test)]
mod tests {
    use super::build_asc;
    use crate::dablin::dabplus::SuperframeFormat;

    #[test]
    fn test_build_asc_aac_lc_stereo() {
        let fmt = SuperframeFormat {
            dac_rate: true,
            sbr_flag: false,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };

        let asc = build_asc(&fmt);
        assert_eq!(asc, vec![0x11, 0x94]);
    }

    #[test]
    fn test_build_asc_he_aac() {
        let fmt = SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };

        let asc = build_asc(&fmt);
        assert_eq!(asc, vec![0x13, 0x14, 0x56, 0xE5, 0x98]);
    }

    #[test]
    fn test_build_asc_he_aac_v2_with_ps() {
        let fmt = SuperframeFormat {
            dac_rate: false,
            sbr_flag: true,
            aac_channel_mode: false,
            ps_flag: true,
            mpeg_surround_config: 0,
        };

        let asc = build_asc(&fmt);
        assert_eq!(asc, vec![0x14, 0x14, 0x56, 0xE5, 0xAD, 0x48, 0x80]);
    }
}
