//! LATM/LOAS framing for AAC access units.
//!
//! The generated stream is LOAS (sync + length) carrying one LATM AudioMuxElement
//! per AAC access unit. We include StreamMuxConfig in each element for robustness.

use crate::dablin::dabplus::SuperframeFormat;
use crate::dablin::audio::asc::build_asc;

const LOAS_SYNCWORD: u16 = 0x2B7;
const LOAS_HEADER_LEN: usize = 3;
const LOAS_MAX_MUX_LEN: usize = 0x1FFF;
const LATM_LEN_BYTE_CONT: usize = 255;

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

    fn bit_len(&self) -> usize {
        if self.bit_pos == 0 {
            self.data.len() * 8
        } else {
            (self.data.len().saturating_sub(1) * 8) + usize::from(self.bit_pos)
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

// Append byte-aligned data into a stream currently at `bit_offset` inside its last byte.
fn append_bytes_with_bit_offset(dst: &mut Vec<u8>, bit_offset: u8, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }

    if bit_offset == 0 {
        dst.extend_from_slice(bytes);
        return;
    }

    let right = bit_offset;
    let left = 8 - right;

    // The destination must already contain a partial byte to complete.
    if dst.is_empty() {
        dst.push(0);
    }

    for &b in bytes {
        let last = dst.len() - 1;
        dst[last] |= b >> right;
        dst.push(b << left);
    }
}

fn payload_length_byte_count(au_len: usize) -> usize {
    (au_len / LATM_LEN_BYTE_CONT) + 1
}

fn write_payload_length_bytes(buf: &mut [u8; 8], mut au_len: usize) -> usize {
    let mut count = 0usize;
    while au_len >= LATM_LEN_BYTE_CONT {
        buf[count] = LATM_LEN_BYTE_CONT as u8;
        count += 1;
        au_len -= LATM_LEN_BYTE_CONT;
    }
    buf[count] = au_len as u8;
    count + 1
}

fn write_loas_header(packet: &mut [u8], mux_len: usize) {
    packet[0] = (LOAS_SYNCWORD >> 3) as u8;
    packet[1] = (((LOAS_SYNCWORD & 0x07) as u8) << 5) | (((mux_len >> 8) & 0x1F) as u8);
    packet[2] = (mux_len & 0xFF) as u8;
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

        let prefix_bits = writer.bit_len();
        let prefix = writer.into_bytes();
        self.cached_prefix_bits = prefix_bits;
        self.cached_prefix = prefix;
        self.cached_format = Some(fmt.clone());
    }

    pub fn wrap<'a>(&'a mut self, fmt: &SuperframeFormat, au: &[u8]) -> &'a [u8] {
        self.refresh_cache(fmt);

        let payload_len_bytes = payload_length_byte_count(au.len());
        let mux_bits = self.cached_prefix_bits + (payload_len_bytes + au.len()) * 8;
        let mux_len = mux_bits.div_ceil(8);
        assert!(mux_len <= LOAS_MAX_MUX_LEN, "LATM payload too large for LOAS");

        // Build final LOAS packet directly into a reusable buffer.
        self.packet_buf.clear();
        self.packet_buf.reserve(LOAS_HEADER_LEN + mux_len);
        self.packet_buf.extend_from_slice(&[0, 0, 0]);

        let prefix_byte_len = self.cached_prefix_bits / 8;
        let prefix_bit_offset = (self.cached_prefix_bits % 8) as u8;

        if prefix_byte_len > 0 {
            self.packet_buf
                .extend_from_slice(&self.cached_prefix[..prefix_byte_len]);
        }
        if prefix_bit_offset > 0 {
            self.packet_buf.push(self.cached_prefix[prefix_byte_len]);
        }

        let mut len_bytes = [0u8; 8];
        let len_count = write_payload_length_bytes(&mut len_bytes, au.len());

        append_bytes_with_bit_offset(&mut self.packet_buf, prefix_bit_offset, &len_bytes[..len_count]);
        append_bytes_with_bit_offset(&mut self.packet_buf, prefix_bit_offset, au);

        let expected_len = LOAS_HEADER_LEN + mux_len;
        if self.packet_buf.len() > expected_len {
            self.packet_buf.truncate(expected_len);
        } else if self.packet_buf.len() < expected_len {
            self.packet_buf.resize(expected_len, 0);
        }

        write_loas_header(&mut self.packet_buf[..LOAS_HEADER_LEN], mux_len);
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
