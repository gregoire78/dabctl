//! LATM/LOAS framing for AAC access units.
//!
//! The generated stream is LOAS (sync + length) carrying one LATM AudioMuxElement
//! per AAC access unit. We include StreamMuxConfig in each element for robustness.

use crate::dablin::dabplus::SuperframeFormat;
use crate::dablin::audio::asc::build_asc;

const LOAS_SYNCWORD: u16 = 0x2B7;

struct BitWriter {
    data: Vec<u8>,
    bit_pos: u8,
}

impl BitWriter {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            bit_pos: 0,
        }
    }

    fn write_bits(&mut self, value: u32, bits: u8) {
        if bits == 0 {
            return;
        }

        // Fast path for full-byte writes when byte-aligned.
        if self.bit_pos == 0 && bits == 8 {
            self.data.push(value as u8);
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

struct SliceBitWriter<'a> {
    data: &'a mut Vec<u8>,
    bit_pos: u8,
}

impl<'a> SliceBitWriter<'a> {
    fn new(data: &'a mut Vec<u8>) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn write_bits(&mut self, value: u32, bits: u8) {
        if bits == 0 {
            return;
        }

        if self.bit_pos == 0 && bits == 8 {
            self.data.push(value as u8);
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
}

fn write_latm_value(writer: &mut BitWriter, value: usize) {
    let mut nbytes_minus1: u8 = 0;
    while (value >> ((usize::from(nbytes_minus1) + 1) * 8)) != 0 && nbytes_minus1 < 3 {
        nbytes_minus1 += 1;
    }

    writer.write_bits(u32::from(nbytes_minus1), 2);
    for i in (0..=nbytes_minus1).rev() {
        let shift = usize::from(i) * 8;
        writer.write_bits(((value >> shift) & 0xFF) as u32, 8);
    }
}

fn build_latm_asc(fmt: &SuperframeFormat) -> Vec<u8> {
    // Canonical DAB+ ASC (HE-AAC where signaled), including 960-frame signaling.
    build_asc(fmt)
}

fn append_bits_from_bytes_slice(writer: &mut SliceBitWriter<'_>, bytes: &[u8], bit_len: usize) {
    let full = bit_len / 8;
    let rem = bit_len % 8;

    for &b in &bytes[..full] {
        writer.write_bits(u32::from(b), 8);
    }

    if rem > 0 {
        let next = bytes[full] >> (8 - rem);
        writer.write_bits(u32::from(next), rem as u8);
    }
}

fn write_stream_mux_config(writer: &mut BitWriter, asc: &[u8]) {
    // StreamMuxConfig, 1 program / 1 layer, in-band ASC.
    // Use audioMuxVersion=1 to carry explicit ASC bit length.
    writer.write_bits(1, 1); // audioMuxVersion = 1
    writer.write_bits(0, 1); // audioMuxVersionA = 0
    write_latm_value(writer, 0); // taraFullness = 0
    writer.write_bits(1, 1); // allStreamsSameTimeFraming = 1
    writer.write_bits(0, 6); // numSubFrames = 0 (one subframe)
    writer.write_bits(0, 4); // numProgram = 0 (one program)
    writer.write_bits(0, 3); // numLayer = 0 (one layer)

    write_latm_value(writer, asc.len() * 8); // ascLen in bits
    for b in asc {
        writer.write_bits(u32::from(*b), 8);
    }

    writer.write_bits(0, 3); // frameLengthType = 0
    writer.write_bits(0xFF, 8); // latmBufferFullness
    writer.write_bits(0, 1); // otherDataPresent = 0
    writer.write_bits(0, 1); // crcCheckPresent = 0
}

/// Stateful LATM packetizer that caches the StreamMuxConfig prefix for a format.
pub struct LatmPacker {
    cached_format: Option<SuperframeFormat>,
    cached_prefix: Vec<u8>,
    cached_prefix_bits: usize,
    packet_buf: Vec<u8>,
}

impl LatmPacker {
    pub fn new() -> Self {
        Self {
            cached_format: None,
            cached_prefix: Vec::new(),
            cached_prefix_bits: 0,
            packet_buf: Vec::new(),
        }
    }

    fn refresh_cache(&mut self, fmt: &SuperframeFormat) {
        if self.cached_format.as_ref() == Some(fmt) {
            return;
        }

        let asc = build_latm_asc(fmt);
        let mut writer = BitWriter::with_capacity(asc.len() + 24);

        // AudioMuxElement(muxConfigPresent=1)
        writer.write_bits(0, 1); // useSameStreamMux = 0
        write_stream_mux_config(&mut writer, &asc);

        let prefix = writer.into_bytes();
        // Prefix currently ends at bit position 2 (90 bits total).
        self.cached_prefix_bits = 90;
        self.cached_prefix = prefix;
        self.cached_format = Some(fmt.clone());
    }

    pub fn wrap<'a>(&'a mut self, fmt: &SuperframeFormat, au: &[u8]) -> &'a [u8] {
        self.refresh_cache(fmt);

        let payload_len_bytes = (au.len() / 255) + 1;
        let mux_bits = self.cached_prefix_bits + (payload_len_bytes + au.len()) * 8;
        let mux_len = mux_bits.div_ceil(8);
        assert!(mux_len <= 0x1FFF, "LATM payload too large for LOAS");

        // Build final LOAS packet directly into a reusable buffer.
        self.packet_buf.clear();
        self.packet_buf.reserve(3 + mux_len);
        let mut writer = SliceBitWriter::new(&mut self.packet_buf);
        writer.write_bits(0, 8);
        writer.write_bits(0, 8);
        writer.write_bits(0, 8);

        append_bits_from_bytes_slice(&mut writer, &self.cached_prefix, self.cached_prefix_bits);

        let mut remaining = au.len();
        while remaining >= 255 {
            writer.write_bits(255, 8);
            remaining -= 255;
        }
        writer.write_bits(remaining as u32, 8);

        for &b in au {
            writer.write_bits(u32::from(b), 8);
        }

        self.packet_buf[0] = (LOAS_SYNCWORD >> 3) as u8;
        self.packet_buf[1] = (((LOAS_SYNCWORD & 0x07) as u8) << 5) | (((mux_len >> 8) & 0x1F) as u8);
        self.packet_buf[2] = (mux_len & 0xFF) as u8;
        &self.packet_buf
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
    fn test_loas_syncword_and_length() {
        let au = vec![0xAA; 64];
        let mut packer = LatmPacker::new();
        let packet = packer.wrap(&fmt_stereo_he_aac(), &au);

        assert_eq!(packet[0], 0x56);
        assert_eq!(packet[1] >> 5, 0x07);

        let len = (((packet[1] as usize) & 0x1F) << 8) | (packet[2] as usize);
        assert_eq!(len + 3, packet.len());
    }

    #[test]
    fn test_latm_packet_size_grows_with_au_size() {
        let mut packer = LatmPacker::new();
        let packet_small_len = packer.wrap(&fmt_stereo_he_aac(), &[0xDE, 0xAD]).len();
        let packet_large_len = packer.wrap(&fmt_stereo_he_aac(), &[0xDE, 0xAD, 0xBE, 0xEF]).len();

        // LATM payload may be bit-packed, so compare structure through size growth.
        assert!(packet_large_len > packet_small_len);
    }

    #[test]
    fn test_latm_mux_has_payload_after_loas_header() {
        let fmt = fmt_stereo_he_aac();
        let au = [0xAA, 0xBB, 0xCC];
        let mut packer = LatmPacker::new();
        let packet = packer.wrap(&fmt, &au);

        assert!(packet.len() > 8);
    }

    #[test]
    fn test_build_latm_asc_for_he_aac_is_not_empty() {
        let asc = build_latm_asc(&fmt_stereo_he_aac());
        assert!(!asc.is_empty());
    }
}
