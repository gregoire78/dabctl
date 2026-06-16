//! LATM/LOAS framing for AAC access units.
//!
//! The generated stream is LOAS (sync + length) carrying one LATM AudioMuxElement
//! per AAC access unit. We include StreamMuxConfig in each element for robustness.

use crate::dablin::audio::asc::build_asc;
use crate::dablin::dabplus::SuperframeFormat;

const LOAS_SYNCWORD: u16 = 0x2B7;

struct BitWriter {
    data: Vec<u8>,
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            bit_pos: 0,
        }
    }

    fn write_bits(&mut self, value: u32, bits: u8) {
        if bits == 0 {
            return;
        }
        for i in (0..bits).rev() {
            let bit = ((value >> i) & 1) as u8;
            if self.bit_pos == 0 {
                self.data.push(0);
            }
            let idx = self.data.len() - 1;
            self.data[idx] |= bit << (7 - self.bit_pos);
            self.bit_pos = (self.bit_pos + 1) % 8;
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.data
    }
}

fn write_stream_mux_config(writer: &mut BitWriter, asc: &[u8]) {
    // StreamMuxConfig, 1 program / 1 layer, in-band ASC.
    writer.write_bits(0, 1); // audioMuxVersion = 0
    writer.write_bits(1, 1); // allStreamsSameTimeFraming = 1
    writer.write_bits(0, 6); // numSubFrames = 0 (one subframe)
    writer.write_bits(0, 4); // numProgram = 0 (one program)
    writer.write_bits(0, 3); // numLayer = 0 (one layer)

    // AudioSpecificConfig() bit length + bytes.
    writer.write_bits(asc.len() as u32, 8);
    for b in asc {
        writer.write_bits(u32::from(*b), 8);
    }

    writer.write_bits(0, 3); // frameLengthType = 0
    writer.write_bits(0xFF, 8); // latmBufferFullness
    writer.write_bits(0, 1); // otherDataPresent = 0
    writer.write_bits(0, 1); // crcCheckPresent = 0
}

fn build_audio_mux_element(fmt: &SuperframeFormat, au: &[u8]) -> Vec<u8> {
    let asc = build_asc(fmt);
    let mut writer = BitWriter::new();

    // AudioMuxElement(muxConfigPresent=1)
    writer.write_bits(0, 1); // useSameStreamMux = 0
    write_stream_mux_config(&mut writer, &asc);

    // PayloadLengthInfo() for frameLengthType=0.
    let mut remaining = au.len();
    while remaining >= 255 {
        writer.write_bits(255, 8);
        remaining -= 255;
    }
    writer.write_bits(remaining as u32, 8);

    // PayloadMux() raw AAC AU bytes.
    for b in au {
        writer.write_bits(u32::from(*b), 8);
    }

    writer.into_bytes()
}

/// Wrap an AAC access unit in a LOAS packet carrying LATM.
pub fn wrap_au_in_latm(fmt: &SuperframeFormat, au: &[u8]) -> Vec<u8> {
    let mux = build_audio_mux_element(fmt, au);
    let mux_len = mux.len();
    assert!(mux_len <= 0x1FFF, "LATM payload too large for LOAS");

    let mut out = Vec::with_capacity(3 + mux_len);
    out.push((LOAS_SYNCWORD >> 3) as u8);
    out.push((((LOAS_SYNCWORD & 0x07) as u8) << 5) | (((mux_len >> 8) & 0x1F) as u8));
    out.push((mux_len & 0xFF) as u8);
    out.extend_from_slice(&mux);
    out
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
    fn test_loas_syncword_and_length() {
        let au = vec![0xAA; 64];
        let packet = wrap_au_in_latm(&fmt_stereo_he_aac(), &au);

        assert_eq!(packet[0], 0x56);
        assert_eq!(packet[1] >> 5, 0x07);

        let len = (((packet[1] as usize) & 0x1F) << 8) | (packet[2] as usize);
        assert_eq!(len + 3, packet.len());
    }

    #[test]
    fn test_latm_packet_size_grows_with_au_size() {
        let packet_small = wrap_au_in_latm(&fmt_stereo_he_aac(), &[0xDE, 0xAD]);
        let packet_large = wrap_au_in_latm(&fmt_stereo_he_aac(), &[0xDE, 0xAD, 0xBE, 0xEF]);

        // LATM payload may be bit-packed, so compare structure through size growth.
        assert!(packet_large.len() > packet_small.len());
    }
}
