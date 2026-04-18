use anyhow::{ensure, Result};

use crate::backend::data::{mot::Slide, pad_handler::PadHandler};

use super::{AacDecoder, StreamParameters};

pub const DEFAULT_DAB_PLUS_BITRATE: u16 = 64;

// DABstar-style DAB+ superframe handler. Reed-Solomon/firecode are still a future parity step,
// but the AU framing, PAD dispatch, and AAC handoff follow the original execution order.
pub struct Mp4Processor {
    pad_handler: PadHandler,
    bit_rate: u16,
    frame_byte_vec: Vec<u8>,
    block_fill_index: usize,
    blocks_in_buffer: usize,
    aac_decoder: Box<dyn AacDecoder>,
}

impl Mp4Processor {
    pub fn new(bit_rate: u16, aac_decoder: Box<dyn AacDecoder>) -> Self {
        let rs_dims = usize::from((bit_rate / 8).max(1));
        Self {
            pad_handler: PadHandler::default(),
            bit_rate,
            frame_byte_vec: vec![0; rs_dims * 120],
            block_fill_index: 0,
            blocks_in_buffer: 0,
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

        let mut superframe = vec![0u8; num_bytes * 5];
        for block_idx in 0..5 {
            let src_block = (self.block_fill_index + block_idx) % 5;
            let src_start = src_block * num_bytes;
            let dst_start = block_idx * num_bytes;
            superframe[dst_start..dst_start + num_bytes]
                .copy_from_slice(&self.frame_byte_vec[src_start..src_start + num_bytes]);
        }

        self.process_superframe(&superframe)
    }

    pub fn last_dynamic_label(&self) -> Option<&str> {
        self.pad_handler.last_dynamic_label()
    }

    fn process_superframe(&mut self, frame_bytes: &[u8]) -> Result<Vec<i16>> {
        let stream_parameters = parse_stream_parameters(frame_bytes)?;
        let starts = audio_unit_starts(frame_bytes, self.bit_rate, &stream_parameters)?;
        let mut pcm = Vec::new();

        for window in starts.windows(2) {
            let start = window[0];
            let end = window[1];
            if end <= start + 2 || end > frame_bytes.len() {
                continue;
            }

            let aac_frame_len = end - start - 2;
            if aac_frame_len == 0 || aac_frame_len > 960 {
                continue;
            }

            if ((frame_bytes[start] >> 5) & 0x07) == 4 {
                let count = frame_bytes.get(start + 1).copied().unwrap_or(0) as usize;
                let pad_start = start + 2;
                let pad_end = pad_start.saturating_add(count).min(frame_bytes.len());
                if pad_end > pad_start {
                    let payload = &frame_bytes[pad_start..pad_end];
                    self.pad_handler.process_pad(payload)?;
                    self.pad_handler.accept_slide(Slide::new(
                        "pad.bin",
                        "application/octet-stream",
                        payload.to_vec(),
                    ));
                    let _ = self.pad_handler.has_slide();
                }
            }

            let samples = self.aac_decoder.decode_access_unit(
                &stream_parameters,
                &frame_bytes[start..start + aac_frame_len],
            )?;
            pcm.extend_from_slice(&samples);
        }

        Ok(pcm)
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
