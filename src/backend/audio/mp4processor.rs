use anyhow::{ensure, Result};

use crate::backend::{
    data::{mot::Slide, pad_handler::PadHandler},
    reed_solomon::ReedSolomon,
};

use super::{AacDecoder, StreamParameters};

pub const DEFAULT_DAB_PLUS_BITRATE: u16 = 64;

// DABstar-style DAB+ superframe handler with firecode and Reed-Solomon repair
// before AAC access-unit extraction.
pub struct Mp4Processor {
    pad_handler: PadHandler,
    bit_rate: u16,
    frame_byte_vec: Vec<u8>,
    out_vec: Vec<u8>,
    block_fill_index: usize,
    blocks_in_buffer: usize,
    superframe_sync: i32,
    firecode: FirecodeChecker,
    rs_decoder: ReedSolomon,
    aac_decoder: Box<dyn AacDecoder>,
}

impl Mp4Processor {
    pub fn new(bit_rate: u16, aac_decoder: Box<dyn AacDecoder>) -> Self {
        let rs_dims = usize::from((bit_rate / 8).max(1));
        Self {
            pad_handler: PadHandler::default(),
            bit_rate,
            frame_byte_vec: vec![0; rs_dims * 120],
            out_vec: vec![0; rs_dims * 110],
            block_fill_index: 0,
            blocks_in_buffer: 0,
            superframe_sync: 0,
            firecode: FirecodeChecker::new(),
            rs_decoder: ReedSolomon::new(),
            aac_decoder,
        }
    }

    pub fn add_to_frame(&mut self, soft_bits: &[i8]) -> Result<Vec<i16>> {
        let num_bits = 24usize * usize::from(self.bit_rate);
        if soft_bits.len() < num_bits {
            return Ok(Vec::new());
        }

        let num_bytes = num_bits / 8;
        let base = self.block_fill_index * num_bytes;

        for byte_idx in 0..num_bytes {
            let mut value = 0u8;
            for bit_idx in 0..8 {
                let bit = if soft_bits[byte_idx * 8 + bit_idx] > 0 {
                    1u8
                } else {
                    0u8
                };
                value = (value << 1) | bit;
            }
            self.frame_byte_vec[base + byte_idx] = value;
        }

        self.block_fill_index = (self.block_fill_index + 1) % 5;
        self.blocks_in_buffer = (self.blocks_in_buffer + 1).min(5);

        if self.blocks_in_buffer < 5 {
            return Ok(Vec::new());
        }

        let base = self.block_fill_index * num_bytes;

        if self.superframe_sync == 0 {
            let frame_window = &self.frame_byte_vec[base..base + num_bytes];
            if self.firecode.check(frame_window) {
                self.superframe_sync = 4;
            } else {
                self.blocks_in_buffer = 4;
                let diag = diagnose_firecode_variants(&self.firecode, frame_window);
                tracing::debug!(
                    header = ?&frame_window[..frame_window.len().min(12)],
                    diagnostic = %diag,
                    "DAB+ raw firecode sync check rejected current 5-frame window"
                );
                return Ok(Vec::new());
            }
        }

        self.blocks_in_buffer = 0;
        if self.process_reed_solomon_frame(&self.frame_byte_vec.clone(), base) {
            match self.process_superframe() {
                Ok(pcm) => {
                    self.superframe_sync = 4;
                    Ok(pcm)
                }
                Err(err) => {
                    self.superframe_sync -= 1;
                    if self.superframe_sync == 0 {
                        self.blocks_in_buffer = 4;
                    }
                    tracing::debug!(error = %err, "DAB+ superframe parse rejected current window");
                    Ok(Vec::new())
                }
            }
        } else {
            self.superframe_sync -= 1;
            if self.superframe_sync == 0 {
                self.blocks_in_buffer = 4;
            }
            tracing::debug!("DAB+ firecode/reed-solomon rejected current superframe");
            Ok(Vec::new())
        }
    }

    pub fn last_dynamic_label(&self) -> Option<&str> {
        self.pad_handler.last_dynamic_label()
    }

    pub fn take_last_slide(&mut self) -> Option<Slide> {
        self.pad_handler.take_last_slide()
    }

    fn process_superframe(&mut self) -> Result<Vec<i16>> {
        let stream_parameters = parse_stream_parameters(&self.out_vec)?;
        let starts = audio_unit_starts(&self.out_vec, self.bit_rate, &stream_parameters)?;
        tracing::debug!(
            header = ?&self.out_vec[..self.out_vec.len().min(12)],
            dac_rate = stream_parameters.dac_rate,
            sbr_flag = stream_parameters.sbr_flag,
            aac_channel_mode = stream_parameters.aac_channel_mode,
            ps_flag = stream_parameters.ps_flag,
            mpeg_surround = stream_parameters.mpeg_surround,
            au_starts = ?starts,
            "parsed one corrected DAB+ header"
        );
        let mut pcm = Vec::new();
        let mut valid_aus = 0usize;

        for window in starts.windows(2) {
            let start = window[0];
            let end = window[1];
            if end <= start + 2 || end > self.out_vec.len() {
                continue;
            }

            let aac_frame_len = end - start - 2;
            if aac_frame_len > 960 {
                continue;
            }
            if !check_crc_bytes(&self.out_vec[start..], aac_frame_len) {
                tracing::debug!(
                    start,
                    aac_frame_len,
                    "AAC unit CRC failed; still trying decoder"
                );
            } else {
                valid_aus += 1;
            }

            if ((self.out_vec[start] >> 5) & 0x07) == 4 {
                let count = self.out_vec.get(start + 1).copied().unwrap_or(0) as usize;
                let pad_start = start + 2;
                let pad_end = pad_start.saturating_add(count).min(self.out_vec.len());
                if pad_end > pad_start {
                    let payload = &self.out_vec[pad_start..pad_end];
                    self.pad_handler.process_pad(payload)?;
                    self.pad_handler.accept_slide(Slide::new(
                        "pad.bin",
                        "application/octet-stream",
                        payload.to_vec(),
                    ));
                    let _ = self.pad_handler.has_slide();
                }
            }

            match self.aac_decoder.decode_access_unit(
                &stream_parameters,
                &self.out_vec[start..start + aac_frame_len],
            ) {
                Ok(samples) => {
                    pcm.extend_from_slice(&samples);
                }
                Err(err) => {
                    tracing::debug!(error = %err, "dropping one AAC access unit and continuing");
                }
            }
        }

        tracing::debug!(
            valid_aus,
            total_windows = starts.len().saturating_sub(1),
            pcm_samples = pcm.len(),
            "processed one DAB+ superframe"
        );
        Ok(pcm)
    }

    fn process_reed_solomon_frame(&mut self, frame_bytes: &[u8], base: usize) -> bool {
        let rs_dims = usize::from((self.bit_rate / 8).max(1));
        let frame_span = rs_dims * 120;
        let mut had_rs_error = false;
        let mut total_corrections = 0i32;

        for j in 0..rs_dims {
            let mut rs_in = [0u8; 120];
            for (k, byte) in rs_in.iter_mut().enumerate() {
                let idx = (base + j + k * rs_dims) % frame_span;
                *byte = frame_bytes[idx];
            }

            let mut rs_out = [0u8; 110];
            let ler = self.rs_decoder.dec(&rs_in, &mut rs_out, 135);
            if ler < 0 {
                had_rs_error = true;
            } else {
                total_corrections += i32::from(ler);
            }

            for (k, byte) in rs_out.iter().copied().enumerate() {
                self.out_vec[j + k * rs_dims] = byte;
            }
        }

        if had_rs_error {
            tracing::debug!("DAB+ reed-solomon reported an uncorrectable column");
        }
        if total_corrections > 0 {
            tracing::debug!(
                total_corrections,
                "DAB+ reed-solomon corrected superframe symbols"
            );
        }

        self.firecode.check_and_correct_6bits(&mut self.out_vec)
    }
}

fn diagnose_firecode_variants(firecode: &FirecodeChecker, bytes: &[u8]) -> String {
    if bytes.len() < 11 {
        return "too-short".to_string();
    }

    for byte_offset in 0..=bytes.len() - 11 {
        let header = &bytes[byte_offset..byte_offset + 11];
        if byte_offset != 0 && firecode.check(header) {
            return format!("byte-offset-{byte_offset}");
        }

        let reversed: Vec<u8> = header.iter().map(|b| b.reverse_bits()).collect();
        if firecode.check(&reversed) {
            return if byte_offset == 0 {
                "byte-reverse".to_string()
            } else {
                format!("byte-offset-{byte_offset}+byte-reverse")
            };
        }

        let inverted: Vec<u8> = header.iter().map(|b| !b).collect();
        if firecode.check(&inverted) {
            return if byte_offset == 0 {
                "invert".to_string()
            } else {
                format!("byte-offset-{byte_offset}+invert")
            };
        }

        let reversed_inverted: Vec<u8> = header.iter().map(|b| (!b).reverse_bits()).collect();
        if firecode.check(&reversed_inverted) {
            return if byte_offset == 0 {
                "invert+byte-reverse".to_string()
            } else {
                format!("byte-offset-{byte_offset}+invert+byte-reverse")
            };
        }

        for shift in 1..8u8 {
            let left = shift_header_bits(header, shift, false);
            if firecode.check(&left) {
                return if byte_offset == 0 {
                    format!("shift-left-{shift}")
                } else {
                    format!("byte-offset-{byte_offset}+shift-left-{shift}")
                };
            }
            let right = shift_header_bits(header, shift, true);
            if firecode.check(&right) {
                return if byte_offset == 0 {
                    format!("shift-right-{shift}")
                } else {
                    format!("byte-offset-{byte_offset}+shift-right-{shift}")
                };
            }
            let left_inv: Vec<u8> = left.iter().map(|b| !b).collect();
            if firecode.check(&left_inv) {
                return if byte_offset == 0 {
                    format!("shift-left-{shift}+invert")
                } else {
                    format!("byte-offset-{byte_offset}+shift-left-{shift}+invert")
                };
            }
            let right_inv: Vec<u8> = right.iter().map(|b| !b).collect();
            if firecode.check(&right_inv) {
                return if byte_offset == 0 {
                    format!("shift-right-{shift}+invert")
                } else {
                    format!("byte-offset-{byte_offset}+shift-right-{shift}+invert")
                };
            }
        }
    }

    "none".to_string()
}

fn shift_header_bits(header: &[u8], shift: u8, right: bool) -> Vec<u8> {
    let bit_len = header.len() * 8;
    let mut out = vec![0u8; header.len()];
    for dst_bit in 0..bit_len {
        let src_bit = if right {
            dst_bit.checked_sub(usize::from(shift))
        } else {
            let pos = dst_bit + usize::from(shift);
            (pos < bit_len).then_some(pos)
        };
        if let Some(src_bit) = src_bit {
            let src_byte = src_bit / 8;
            let src_off = 7 - (src_bit % 8);
            let bit = (header[src_byte] >> src_off) & 1;
            let dst_byte = dst_bit / 8;
            let dst_off = 7 - (dst_bit % 8);
            out[dst_byte] |= bit << dst_off;
        }
    }
    out
}

fn calc_crc(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for byte in data {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    !crc
}

fn check_crc_bytes(msg: &[u8], len: usize) -> bool {
    if msg.len() < len + 2 {
        return false;
    }
    let accumulator = calc_crc(&msg[..len]);
    let crc = (u16::from(msg[len]) << 8) | u16::from(msg[len + 1]);
    (crc ^ accumulator) == 0
}

struct FirecodeChecker {
    crc_table: [u16; 256],
    syndrome_table: Box<[u16; 65536]>,
}

impl FirecodeChecker {
    fn new() -> Self {
        let poly = 0x782Fu16;
        let mut crc_table = [0u16; 256];
        for (idx, entry) in crc_table.iter_mut().enumerate() {
            let mut crc = (idx as u16) << 8;
            for _ in 0..8 {
                if (crc & 0x8000) != 0 {
                    crc = (crc << 1) ^ poly;
                } else {
                    crc <<= 1;
                }
            }
            *entry = crc;
        }

        let mut checker = Self {
            crc_table,
            syndrome_table: Box::new([0u16; 65536]),
        };
        checker.fill_syndrome_table();
        checker
    }

    fn fill_syndrome_table(&mut self) {
        const PATTERN: [u8; 124] = [
            17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 30, 31, 34, 36, 38, 40, 42, 44, 46, 50,
            52, 54, 56, 60, 62, 68, 72, 76, 84, 88, 92, 100, 104, 108, 120, 124, 136, 152, 168,
            184, 200, 216, 248, 33, 35, 37, 39, 41, 43, 45, 49, 51, 53, 55, 57, 59, 61, 63, 66, 70,
            74, 78, 82, 86, 90, 98, 102, 106, 110, 114, 118, 122, 126, 132, 140, 148, 156, 164,
            172, 180, 196, 204, 212, 220, 228, 236, 244, 252, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
            12, 13, 14, 15, 16, 29, 32, 48, 58, 64, 80, 96, 112, 116, 128, 144, 160, 176, 192, 208,
            224, 232, 240,
        ];

        let mut error = [0u8; 11];

        for i in 0..11usize {
            for pattern in PATTERN {
                let bit = i * 8;
                error[i] = pattern;
                let syndrome = self.crc16(&error);
                if self.syndrome_table[usize::from(syndrome)] == 0 {
                    self.syndrome_table[usize::from(syndrome)] =
                        ((bit as u16) << 8) + u16::from(pattern);
                }
                error[i] = 0;
            }
        }

        for i in 0..10usize {
            for pattern in PATTERN.iter().copied().take(45) {
                let bit = i * 8 + 4;
                error[i] = pattern >> 4;
                error[i + 1] = pattern << 4;
                let syndrome = self.crc16(&error);
                if self.syndrome_table[usize::from(syndrome)] == 0 {
                    self.syndrome_table[usize::from(syndrome)] =
                        ((bit as u16) << 8) + u16::from(pattern);
                }
                error[i] = 0;
                error[i + 1] = 0;
            }
        }

        for i in 0..10usize {
            for pattern in PATTERN.iter().copied().skip(45).take(30) {
                let bit = i * 8 + 2;
                error[i] = pattern >> 2;
                error[i + 1] = pattern << 6;
                let syndrome = self.crc16(&error);
                if self.syndrome_table[usize::from(syndrome)] == 0 {
                    self.syndrome_table[usize::from(syndrome)] =
                        ((bit as u16) << 8) + u16::from(pattern);
                }
                error[i] = 0;
                error[i + 1] = 0;
            }
        }

        for i in 0..10usize {
            for pattern in PATTERN.iter().copied().skip(60).take(30) {
                let bit = i * 8 + 6;
                error[i] = pattern >> 6;
                error[i + 1] = pattern << 2;
                let syndrome = self.crc16(&error);
                if self.syndrome_table[usize::from(syndrome)] == 0 {
                    self.syndrome_table[usize::from(syndrome)] =
                        ((bit as u16) << 8) + u16::from(pattern);
                }
                error[i] = 0;
                error[i + 1] = 0;
            }
        }
    }

    fn crc16(&self, bytes: &[u8]) -> u16 {
        if bytes.len() < 11 {
            return 1;
        }

        let mut crc = 0u16;
        for byte in &bytes[2..11] {
            let pos = ((crc >> 8) ^ u16::from(*byte)) as usize;
            crc = (crc << 8) ^ self.crc_table[pos];
        }
        for byte in &bytes[0..2] {
            let pos = ((crc >> 8) ^ u16::from(*byte)) as usize;
            crc = (crc << 8) ^ self.crc_table[pos];
        }
        crc
    }

    fn check(&self, bytes: &[u8]) -> bool {
        bytes.len() >= 11 && self.crc16(bytes) == 0
    }

    fn check_and_correct_6bits(&mut self, bytes: &mut [u8]) -> bool {
        if bytes.len() < 11 {
            return false;
        }

        let syndrome = self.crc16(bytes);
        if syndrome == 0 {
            return true;
        }

        let entry = self.syndrome_table[usize::from(syndrome)];
        let error = (entry & 0x00FF) as u8;
        if error == 0 {
            return false;
        }

        let bit = usize::from(entry >> 8);
        let byte_index = bit / 8;
        let bit_shift = bit % 8;
        if byte_index >= bytes.len() {
            return false;
        }

        bytes[byte_index] ^= error >> bit_shift;
        if byte_index + 1 < bytes.len() && bit_shift != 0 {
            bytes[byte_index + 1] ^= error << (8 - bit_shift);
        }
        true
    }
}

pub fn parse_stream_parameters(frame_bytes: &[u8]) -> Result<StreamParameters> {
    ensure!(frame_bytes.len() >= 3, "superframe is too short");

    let header = frame_bytes[2];
    let dac_rate = (header >> 6) & 0x01;
    let sbr_flag = (header >> 5) & 0x01;
    let aac_channel_mode = (header >> 4) & 0x01;
    let ps_flag = (header >> 3) & 0x01;
    let mpeg_surround = header & 0x07;

    let core_sr_index = if dac_rate != 0 {
        if sbr_flag != 0 {
            6
        } else {
            3
        }
    } else if sbr_flag != 0 {
        8
    } else {
        5
    };

    Ok(StreamParameters {
        dac_rate,
        sbr_flag,
        ps_flag,
        aac_channel_mode,
        mpeg_surround,
        core_ch_config: if aac_channel_mode != 0 { 2 } else { 1 },
        core_sr_index,
        extension_sr_index: if dac_rate != 0 { 3 } else { 5 },
    })
}

pub fn audio_unit_starts(
    frame_bytes: &[u8],
    bit_rate: u16,
    stream_parameters: &StreamParameters,
) -> Result<Vec<usize>> {
    let total = 110usize * usize::from((bit_rate / 8).max(1));
    ensure!(
        frame_bytes.len() >= total,
        "superframe payload is truncated"
    );

    let mut starts = match 2 * stream_parameters.dac_rate + stream_parameters.sbr_flag {
        0 => vec![
            8usize,
            usize::from(frame_bytes[3]) * 16 + usize::from(frame_bytes[4] >> 4),
            usize::from(frame_bytes[4] & 0x0F) * 256 + usize::from(frame_bytes[5]),
            usize::from(frame_bytes[6]) * 16 + usize::from(frame_bytes[7] >> 4),
        ],
        1 => vec![
            5usize,
            usize::from(frame_bytes[3]) * 16 + usize::from(frame_bytes[4] >> 4),
        ],
        2 => vec![
            11usize,
            usize::from(frame_bytes[3]) * 16 + usize::from(frame_bytes[4] >> 4),
            usize::from(frame_bytes[4] & 0x0F) * 256 + usize::from(frame_bytes[5]),
            usize::from(frame_bytes[6]) * 16 + usize::from(frame_bytes[7] >> 4),
            usize::from(frame_bytes[7] & 0x0F) * 256 + usize::from(frame_bytes[8]),
            usize::from(frame_bytes[9]) * 16 + usize::from(frame_bytes[10] >> 4),
        ],
        3 => vec![
            6usize,
            usize::from(frame_bytes[3]) * 16 + usize::from(frame_bytes[4] >> 4),
            usize::from(frame_bytes[4] & 0x0F) * 256 + usize::from(frame_bytes[5]),
        ],
        _ => Vec::new(),
    };

    starts.retain(|&boundary| boundary < total);
    starts.push(total);
    starts.sort_unstable();
    starts.dedup();
    Ok(starts)
}

#[allow(dead_code)]
pub fn build_loas_stream(aac_frame_len: usize, params: &StreamParameters, data: &[u8]) -> Vec<u8> {
    let mut writer = BitWriter::new();
    writer.push_bits(0x2B7, 11);
    writer.push_bits(0, 13);
    writer.push_bits(0, 1);
    writer.push_bits(0, 1);
    writer.push_bits(1, 1);
    writer.push_bits(0, 6);
    writer.push_bits(0, 4);
    writer.push_bits(0, 3);

    if params.sbr_flag != 0 {
        writer.push_bits(0b00101, 5);
        writer.push_bits(u32::from(params.core_sr_index), 4);
        writer.push_bits(u32::from(params.core_ch_config), 4);
        writer.push_bits(u32::from(params.extension_sr_index), 4);
        writer.push_bits(0b00010, 5);
        writer.push_bits(0b100, 3);
    } else {
        writer.push_bits(0b00010, 5);
        writer.push_bits(u32::from(params.core_sr_index), 4);
        writer.push_bits(u32::from(params.core_ch_config), 4);
        writer.push_bits(0b100, 3);
    }

    writer.push_bits(0b000, 3);
    writer.push_bits(0xFF, 8);
    writer.push_bits(0, 1);
    writer.push_bits(0, 1);

    for _ in 0..(aac_frame_len / 255) {
        writer.push_bits(0xFF, 8);
    }
    writer.push_bits((aac_frame_len % 255) as u32, 8);
    writer.push_bytes(&data[..aac_frame_len.min(data.len())]);

    let audio_mux_length_bytes = writer.bytes.len().saturating_sub(3) as u16;
    if writer.bytes.len() >= 3 {
        writer.bytes[1] = 0xE0 | ((audio_mux_length_bytes >> 8) as u8 & 0x1F);
        writer.bytes[2] = (audio_mux_length_bytes & 0xFF) as u8;
    }

    writer.bytes
}

struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_pos: 0,
        }
    }

    fn push_bits(&mut self, value: u32, count: u8) {
        for shift in (0..count).rev() {
            let bit = ((value >> shift) & 1) as u8;
            if self.bit_pos == 0 {
                self.bytes.push(0);
            }
            let idx = self.bytes.len() - 1;
            self.bytes[idx] |= bit << (7 - self.bit_pos);
            self.bit_pos = (self.bit_pos + 1) % 8;
        }
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.push_bits(u32::from(*byte), 8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{audio_unit_starts, build_loas_stream, parse_stream_parameters, StreamParameters};

    #[test]
    fn parses_dab_plus_stream_parameters() {
        let frame = [0x00, 0x00, 0b0111_0000];
        let params = parse_stream_parameters(&frame).expect("stream parameters should parse");
        assert_eq!(params.dac_rate, 1);
        assert_eq!(params.sbr_flag, 1);
        assert_eq!(params.aac_channel_mode, 1);
    }

    #[test]
    fn builds_loas_stream_with_syncword() {
        let params = StreamParameters {
            dac_rate: 1,
            sbr_flag: 0,
            ps_flag: 0,
            aac_channel_mode: 1,
            mpeg_surround: 0,
            core_ch_config: 2,
            core_sr_index: 3,
            extension_sr_index: 3,
        };
        let loas = build_loas_stream(4, &params, &[1, 2, 3, 4]);
        assert!(loas.len() >= 7);
        assert_eq!(loas[0], 0x56);
        assert_eq!(loas[1] >> 5, 0x07);
    }

    #[test]
    fn computes_audio_unit_boundaries() {
        let mut frame = vec![0u8; 110 * (64 / 8) as usize];
        frame[2] = 0;
        frame[3] = 0x01;
        frame[4] = 0x20;
        frame[5] = 0x30;
        frame[6] = 0x02;
        frame[7] = 0x40;

        let params = parse_stream_parameters(&frame).expect("params should parse");
        let starts = audio_unit_starts(&frame, 64, &params).expect("AU starts should parse");
        assert!(starts.len() >= 5);
        assert_eq!(starts[0], 8);
    }
}
