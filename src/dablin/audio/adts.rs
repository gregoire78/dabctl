//! ADTS (Audio Data Transport Stream) framing for AAC access units.
//!
//! ADTS is the standard container format for AAC, widely supported by tools
//! like FFmpeg. Each frame has a 7-byte header followed by the raw AU data.

use crate::dablin::dabplus::SuperframeFormat;

const ADTS_HEADER_SIZE: usize = 7;

/// Build an ADTS frame header for a given AAC access unit.
///
/// Format: ADTS fixed header (7 bytes, no CRC)
/// Reference: ISO/IEC 13818-7 (MPEG-2 Advanced Audio Coding)
fn build_adts_header(fmt: &SuperframeFormat, au_len: usize) -> [u8; ADTS_HEADER_SIZE] {
    let mut header = [0u8; ADTS_HEADER_SIZE];

    let frame_length = ADTS_HEADER_SIZE + au_len;
    let sr_index = fmt.core_sr_index();
    let ch_config = fmt.core_ch_config();

    // Syncword (12 bits, all 1s) + MPEG version (1 bit, 1=MPEG-4) + Layer (2 bits, 00) + no CRC (1 bit, 1)
    header[0] = 0xFF;
    header[1] = 0xF1; // 1111 0001

    // Profile (2 bits, 01=AAC-LC) + sampling_frequency_index (4 bits) + private (1 bit) + channel_config (3 bits, first bit)
    header[2] = (1 << 6) | (sr_index << 2) | (ch_config >> 2);

    // channel_config (2 remaining bits) + originality (1 bit) + home (1 bit) + copyrighted (1 bit) + copyright_start (1 bit) + frame_length (2 bits, upper)
    header[3] = ((ch_config & 0x03) << 6) | (((frame_length >> 11) & 0x03) as u8);

    // frame_length (8 bits, middle)
    header[4] = ((frame_length >> 3) & 0xFF) as u8;

    // frame_length (3 bits, lower) + buffer_fullness (5 bits, upper)
    header[5] = (((frame_length & 0x07) << 5) | 0x1F) as u8;

    // buffer_fullness (6 bits, lower) + num_raw_data_blocks (2 bits, 00=1 block)
    header[6] = 0xFC;

    header
}

/// Wrap an AAC access unit in an ADTS frame.
pub fn wrap_au_in_adts(fmt: &SuperframeFormat, au: &[u8]) -> Vec<u8> {
    let header = build_adts_header(fmt, au.len());
    let mut frame = Vec::with_capacity(ADTS_HEADER_SIZE + au.len());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(au);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt_stereo_he_aac() -> SuperframeFormat {
        SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        }
    }

    #[test]
    fn test_adts_syncword() {
        let au = vec![0xAA; 100];
        let frame = wrap_au_in_adts(&fmt_stereo_he_aac(), &au);

        // Check syncword (12 bits, all 1s)
        assert_eq!(frame[0], 0xFF);
        assert_eq!(frame[1] & 0xF0, 0xF0);
    }

    #[test]
    fn test_adts_frame_length() {
        let au = vec![0xBB; 200];
        let frame = wrap_au_in_adts(&fmt_stereo_he_aac(), &au);

        // Frame length should be 7 (header) + 200 (AU) = 207
        let length = ((frame[3] as usize & 0x03) << 11)
            | ((frame[4] as usize) << 3)
            | ((frame[5] as usize) >> 5);
        assert_eq!(length, 207);
    }

    #[test]
    fn test_adts_profile_and_sr() {
        let fmt = fmt_stereo_he_aac();
        let au = vec![0xCC; 50];
        let frame = wrap_au_in_adts(&fmt, &au);

        // Profile should be 1 (AAC-LC)
        let profile = (frame[2] >> 6) & 0x03;
        assert_eq!(profile, 1);

        // Sampling rate index should match format
        let sr_idx = (frame[2] >> 2) & 0x0F;
        assert_eq!(sr_idx, fmt.core_sr_index());
    }
}
