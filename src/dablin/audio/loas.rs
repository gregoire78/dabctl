//! LOAS/LATM packetizer for raw AAC access units.
//!
//! This module wraps DAB+ audio units into LOAS AudioSyncStream packets
//! so downstream tools (for example ffmpeg `-f latm`) can consume them
//! without PCM decoding.

use crate::dablin::audio::asc::build_asc;
use crate::dablin::dabplus::SuperframeFormat;

const LOAS_SYNCWORD: u32 = 0x2B7;
const LOAS_MAX_MUX_LEN: usize = 0x1FFF;

struct BitWriter {
    out: Vec<u8>,
    current: u8,
    used_bits: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            current: 0,
            used_bits: 0,
        }
    }

    fn write_bit(&mut self, bit: bool) {
        self.current <<= 1;
        if bit {
            self.current |= 1;
        }
        self.used_bits += 1;
        if self.used_bits == 8 {
            self.out.push(self.current);
            self.current = 0;
            self.used_bits = 0;
        }
    }

    fn write_bits(&mut self, value: u64, count: u8) {
        for i in (0..count).rev() {
            let bit = ((value >> i) & 1) != 0;
            self.write_bit(bit);
        }
    }

    fn write_bytes(&mut self, data: &[u8]) {
        for b in data {
            self.write_bits(u64::from(*b), 8);
        }
    }

    /// Align to next byte boundary (ISO 14496-3 byte_alignment())
    fn byte_align(&mut self) {
        if self.used_bits > 0 {
            self.current <<= 8 - self.used_bits;
            self.out.push(self.current);
            self.current = 0;
            self.used_bits = 0;
        }
    }

    fn into_bytes(mut self) -> Vec<u8> {
        if self.used_bits > 0 {
            self.current <<= 8 - self.used_bits;
            self.out.push(self.current);
        }
        self.out
    }
}

fn write_latm_payload_length(w: &mut BitWriter, payload_len: usize) {
    let mut remaining = payload_len;
    while remaining >= 255 {
        w.write_bits(255, 8);
        remaining -= 255;
    }
    w.write_bits(remaining as u64, 8);
}

fn write_stream_mux_config(w: &mut BitWriter, fmt: &SuperframeFormat) {
    let asc = build_asc(fmt);

    // StreamMuxConfig()
    w.write_bit(false); // audioMuxVersion = 0
    w.write_bit(true); // allStreamsSameTimeFraming = 1
    w.write_bits(0, 6); // numSubFrames = 0
    w.write_bits(0, 4); // numProgram = 0 (means 1 program)
    
    // for prog = 0 (single program)
    w.write_bits(0, 3); // numLayer = 0 (means 1 layer)
    
    // for lay = 0 (single layer, prog == 0 && lay == 0)
    // audioSpecificConfig() - first layer of first program
    w.write_bytes(&asc);

    // frameLengthType and related fields for this layer
    w.write_bits(0, 3); // frameLengthType = 0
    w.write_bits(0xFF, 8); // latmBufferFullness
    
    // After all programs/layers
    w.write_bit(false); // otherDataPresent = 0
    w.write_bit(false); // crcCheckPresent = 0
}

fn build_audio_mux_element(
    fmt: &SuperframeFormat,
    au: &[u8],
    use_same_stream_mux: bool,
) -> Vec<u8> {
    let mut w = BitWriter::new();

    w.write_bit(use_same_stream_mux);
    if !use_same_stream_mux {
        write_stream_mux_config(&mut w, fmt);
    }

    write_latm_payload_length(&mut w, au.len());
    w.write_bytes(au);

    // ISO 14496-3: byte_alignment() at end of AudioMuxElement
    w.byte_align();

    w.into_bytes()
}

/// Stateful LOAS packetizer.
///
/// Emits full StreamMuxConfig on first AU and whenever DAB+ format changes.
pub struct LoasPacketizer {
    last_fmt: Option<SuperframeFormat>,
}

impl LoasPacketizer {
    pub fn new() -> Self {
        Self { last_fmt: None }
    }

    pub fn packetize_au(&mut self, fmt: &SuperframeFormat, au: &[u8]) -> Option<Vec<u8>> {
        let use_same_stream_mux = self.last_fmt.as_ref() == Some(fmt);
        let mux = build_audio_mux_element(fmt, au, use_same_stream_mux);
        if mux.len() > LOAS_MAX_MUX_LEN {
            tracing::warn!(
                "LOAS mux element too large: {} bytes (max {})",
                mux.len(),
                LOAS_MAX_MUX_LEN
            );
            return None;
        }

        self.last_fmt = Some(fmt.clone());

        let hdr = (LOAS_SYNCWORD << 13) | (mux.len() as u32 & 0x1FFF);
        let mut out = Vec::with_capacity(3 + mux.len());
        out.push(((hdr >> 16) & 0xFF) as u8);
        out.push(((hdr >> 8) & 0xFF) as u8);
        out.push((hdr & 0xFF) as u8);
        out.extend_from_slice(&mux);
        Some(out)
    }
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
    fn test_loas_header_syncword() {
        let mut p = LoasPacketizer::new();
        let pkt = p
            .packetize_au(&fmt_stereo_he_aac(), &[0x11, 0x22, 0x33])
            .unwrap();
        let hdr = (u32::from(pkt[0]) << 16) | (u32::from(pkt[1]) << 8) | u32::from(pkt[2]);
        let sync = hdr >> 13;
        assert_eq!(sync, LOAS_SYNCWORD);
    }

    #[test]
    fn test_loas_reuses_stream_mux_on_next_au() {
        let mut p = LoasPacketizer::new();
        let fmt = fmt_stereo_he_aac();
        let first = p.packetize_au(&fmt, &[0xAA]).unwrap();
        let second = p.packetize_au(&fmt, &[0xBB]).unwrap();
        assert!(first.len() > second.len());
    }
}
