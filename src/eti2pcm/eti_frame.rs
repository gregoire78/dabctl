/// ETI frame parser: extracts FIC data and subchannel data from a 6144-byte ETI frame.
use crate::eti2pcm::crc::crc16_ccitt;

pub const ETI_FRAME_SIZE: usize = 6144;

/// Description of one subchannel stream within an ETI frame (from STC field)
#[derive(Debug, Clone)]
pub struct StreamDescriptor {
    pub scid: u8,       // SubChannel ID (6 bits)
    pub stl: u16,       // Stream length in 64-bit words
}

impl StreamDescriptor {
    /// Data length in bytes
    pub fn byte_len(&self) -> usize {
        self.stl as usize * 8
    }
}

/// Parsed ETI frame header
#[derive(Debug)]
pub struct EtiFrameHeader {
    pub err: u8,
    pub fsync: u32,
    pub ficf: bool,     // FIC flag
    pub nst: u8,        // Number of streams
    pub mid: u8,        // Mode identity (1-4)
    pub fl: u16,        // Frame length in 32-bit words
    pub streams: Vec<StreamDescriptor>,
}

/// Parsed ETI frame with data references
pub struct EtiFrame<'a> {
    pub header: EtiFrameHeader,
    pub fic_data: &'a [u8],
    pub raw: &'a [u8; ETI_FRAME_SIZE],
}

impl<'a> EtiFrame<'a> {
    /// Extract the subchannel data for a given SubChannel ID.
    /// Returns None if the SubChID is not in this frame.
    pub fn subchannel_data(&self, subchid: u8) -> Option<&'a [u8]> {
        let ficl = self.fic_length();
        let data_start = self.data_offset() + ficl * 4;
        let mut offset = data_start;

        for stream in &self.header.streams {
            let byte_len = stream.byte_len();
            if stream.scid == subchid {
                if offset + byte_len <= ETI_FRAME_SIZE {
                    return Some(&self.raw[offset..offset + byte_len]);
                }
                return None;
            }
            offset += byte_len;
        }
        None
    }

    fn fic_length(&self) -> usize {
        if self.header.ficf {
            if self.header.mid == 3 { 32 } else { 24 }
        } else {
            0
        }
    }

    fn data_offset(&self) -> usize {
        // 4 (SYNC+FC) + 4 (FC continued) + nst*4 (STC) + 4 (EOH: 2 MNSC + 2 HCRC)
        // = 8 + nst*4 + 4 ... wait, let's be precise:
        // Byte 0: ERR, 1-3: FSYNC, 4: FCT bits, 5: FICF|NST, 6-7: MID|FL
        // Then 4*NST bytes of STC, then 2 bytes EOH (MNSC), 2 bytes HCRC
        // = 8 + nst*4 + 4
        8 + self.header.nst as usize * 4 + 4
    }
}

/// Parse an ETI frame from a 6144-byte buffer.
/// Returns None if the frame is invalid (bad CRC, bad FSYNC, etc.)
pub fn parse_eti_frame(frame: &[u8; ETI_FRAME_SIZE]) -> Option<EtiFrame<'_>> {
    let err = frame[0];
    if err != 0xFF {
        return None;
    }

    let fsync = (frame[1] as u32) << 16 | (frame[2] as u32) << 8 | frame[3] as u32;
    if fsync != 0x073AB6 && fsync != 0xF8C549 {
        return None;
    }

    // Null transmission check
    if frame[4] == 0xFF && frame[5] == 0xFF && frame[6] == 0xFF && frame[7] == 0xFF {
        return None;
    }

    let ficf = frame[5] & 0x80 != 0;
    let nst = frame[5] & 0x7F;
    let mid = (frame[6] & 0x18) >> 3;
    let fl = ((frame[6] & 0x07) as u16) << 8 | frame[7] as u16;

    // Header CRC: over bytes [4..4+4+nst*4+2], CRC at [4+4+nst*4+2..+2]
    let header_crc_data_len = 4 + nst as usize * 4 + 2;
    let crc_offset = 4 + header_crc_data_len;
    if crc_offset + 2 > ETI_FRAME_SIZE {
        return None;
    }
    let header_crc_stored = (frame[crc_offset] as u16) << 8 | frame[crc_offset + 1] as u16;
    let crc = crc16_ccitt();
    let header_crc_calced = crc.calc(&frame[4..4 + header_crc_data_len]);
    if header_crc_stored != header_crc_calced {
        return None;
    }

    // Parse stream descriptors
    let mut streams = Vec::with_capacity(nst as usize);
    for i in 0..nst as usize {
        let base = 8 + i * 4;
        let scid = (frame[base] & 0xFC) >> 2;
        let stl = ((frame[base + 2] & 0x03) as u16) << 8 | frame[base + 3] as u16;
        streams.push(StreamDescriptor { scid, stl });
    }

    let ficl = if ficf { if mid == 3 { 32 } else { 24 } } else { 0 };
    let data_start = 8 + nst as usize * 4 + 4; // after EOH (MNSC + HCRC)

    // MST CRC check
    let mst_data_len = (fl as usize - nst as usize - 1) * 4;
    let mst_crc_offset = data_start + mst_data_len;
    if mst_crc_offset + 2 <= ETI_FRAME_SIZE {
        let mst_crc_stored = (frame[mst_crc_offset] as u16) << 8 | frame[mst_crc_offset + 1] as u16;
        let mst_crc_calced = crc.calc(&frame[data_start..data_start + mst_data_len]);
        if mst_crc_stored != mst_crc_calced {
            return None;
        }
    }

    let fic_data = if ficl > 0 && data_start + ficl * 4 <= ETI_FRAME_SIZE {
        &frame[data_start..data_start + ficl * 4]
    } else {
        &frame[0..0]
    };

    let header = EtiFrameHeader {
        err,
        fsync,
        ficf,
        nst,
        mid,
        fl,
        streams,
    };

    Some(EtiFrame {
        header,
        fic_data,
        raw: frame,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eti2pcm::crc::crc16_ccitt;

    /// Build a minimal valid ETI frame for testing
    fn build_test_frame(nst: u8, mid: u8, streams: &[(u8, u16)]) -> [u8; ETI_FRAME_SIZE] {
        let mut frame = [0u8; ETI_FRAME_SIZE];
        frame[0] = 0xFF; // ERR
        frame[1] = 0x07; // FSYNC0
        frame[2] = 0x3A;
        frame[3] = 0xB6;

        // FC
        frame[4] = 0x00; // FCT
        frame[5] = 0x80 | nst; // FICF=1 | NST
        let ficl: u16 = if mid == 3 { 32 } else { 24 };
        let total_stl: u16 = streams.iter().map(|(_, stl)| stl).sum();
        let fl = nst as u16 + 1 + ficl + total_stl * 2; // in 32-bit words
        frame[6] = (mid << 3) | ((fl >> 8) as u8 & 0x07);
        frame[7] = fl as u8;

        // STC
        for (i, (scid, stl)) in streams.iter().enumerate() {
            let base = 8 + i * 4;
            frame[base] = scid << 2;
            frame[base + 1] = 0;
            frame[base + 2] = ((stl >> 8) & 0x03) as u8;
            frame[base + 3] = *stl as u8;
        }

        // MNSC (2 bytes) - can be zero
        let mnsc_offset = 8 + nst as usize * 4;
        frame[mnsc_offset] = 0;
        frame[mnsc_offset + 1] = 0;

        // Header CRC
        let header_crc_data_len = 4 + nst as usize * 4 + 2;
        let crc = crc16_ccitt();
        let header_crc = crc.calc(&frame[4..4 + header_crc_data_len]);
        let crc_offset = 4 + header_crc_data_len;
        frame[crc_offset] = (header_crc >> 8) as u8;
        frame[crc_offset + 1] = header_crc as u8;

        // MST CRC
        let data_start = 8 + nst as usize * 4 + 4;
        let mst_data_len = (fl as usize - nst as usize - 1) * 4;
        let mst_crc = crc.calc(&frame[data_start..data_start + mst_data_len]);
        let mst_crc_offset = data_start + mst_data_len;
        frame[mst_crc_offset] = (mst_crc >> 8) as u8;
        frame[mst_crc_offset + 1] = mst_crc as u8;

        frame
    }

    #[test]
    fn test_parse_valid_frame() {
        let frame = build_test_frame(1, 1, &[(5, 12)]);
        let parsed = parse_eti_frame(&frame);
        assert!(parsed.is_some());
        let f = parsed.unwrap();
        assert_eq!(f.header.nst, 1);
        assert_eq!(f.header.mid, 1);
        assert!(f.header.ficf);
        assert_eq!(f.header.streams.len(), 1);
        assert_eq!(f.header.streams[0].scid, 5);
        assert_eq!(f.header.streams[0].stl, 12);
    }

    #[test]
    fn test_parse_bad_err() {
        let mut frame = build_test_frame(1, 1, &[(5, 12)]);
        frame[0] = 0x00; // Bad ERR
        assert!(parse_eti_frame(&frame).is_none());
    }

    #[test]
    fn test_parse_bad_fsync() {
        let mut frame = build_test_frame(1, 1, &[(5, 12)]);
        frame[1] = 0x00;
        assert!(parse_eti_frame(&frame).is_none());
    }

    #[test]
    fn test_subchannel_extraction() {
        let mut frame = build_test_frame(2, 1, &[(5, 12), (10, 8)]);
        // Write some identifiable data into subchannel 10's area
        let data_start = 8 + 2 * 4 + 4;
        let ficl = 24 * 4;
        let sc5_len = 12 * 8; // 96 bytes
        let sc10_start = data_start + ficl + sc5_len;
        for i in 0..64 {
            frame[sc10_start + i] = 0xAB;
        }
        // Need to recompute MST CRC since we modified data
        let crc = crc16_ccitt();
        let fl = ((frame[6] & 0x07) as u16) << 8 | frame[7] as u16;
        let mst_data_len = (fl as usize - 2 - 1) * 4;
        let mst_crc = crc.calc(&frame[data_start..data_start + mst_data_len]);
        let mst_crc_offset = data_start + mst_data_len;
        frame[mst_crc_offset] = (mst_crc >> 8) as u8;
        frame[mst_crc_offset + 1] = mst_crc as u8;

        let parsed = parse_eti_frame(&frame).unwrap();
        let data = parsed.subchannel_data(10).unwrap();
        assert_eq!(data.len(), 64);
        assert!(data.iter().all(|&b| b == 0xAB));
    }
}
