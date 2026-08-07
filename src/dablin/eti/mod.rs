//! ETI-NI frame parser
//! Reference: ETSI EN 300 799
//!
//! Frame structure (6144 bytes for Mode I):
//!   byte 0    : ERR (error status byte)
//!   bytes 1-3 : FSYNC (alternates between two complementary patterns)
//!   bytes 4-7 : FC block (FCT | FICF+NST | FP+MID+FL_hi | FL_lo)
//!   bytes 8.. : STC entries (NST × 4 bytes each)
//!   ..        : EOH (4 bytes: MNSC + HCRC)
//!   ..        : MST (FL × 4 bytes: FIC data + sub-channel streams + padding)
//!   ..        : Frame padding (0x55) to reach 6144 bytes

use arrayvec::ArrayVec;

/// Total ETI-NI frame size in bytes (Mode I)
pub const ETI_FRAME_SIZE: usize = 6144;

/// FIBs per ETI frame per DAB mode (indexed by MID 0-3)
/// One ETI-NI frame = one CIF (24 ms). Each mode carries this many FIBs per CIF.
/// Mode I: 3 FIBs/CIF, Mode II: 3, Mode III: 8, Mode IV: 3
const FIBS_PER_FRAME: [usize; 4] = [3, 3, 8, 3];

/// Sub-channel characterization table entry (STC)
#[derive(Debug, Clone, PartialEq)]
pub struct StcEntry {
    /// Sub-channel ID (0-63)
    pub scid: u8,
    /// Start address in the CIF (Capacity Units)
    pub sad: u16,
    /// Transport protection level
    pub tpl: u8,
    /// Stream length in Capacity Units (1 CU = 64 bits = 8 bytes)
    pub stl: u16,
}

/// Parsed ETI-NI frame (zero-copy: `fic` and `streams` are slices into the raw ETI buffer)
#[derive(Debug)]
pub struct EtiFrame<'a> {
    /// Error byte (0 = no errors)
    #[allow(dead_code)]
    pub err: u8,
    /// Frame count timer (0-249, increments each CIF)
    #[allow(dead_code)]
    pub fct: u8,
    /// FIC present flag
    pub ficf: bool,
    /// Number of sub-channel streams
    #[allow(dead_code)]
    pub nst: u8,
    /// Frame phase (0-6 for Mode I)
    #[allow(dead_code)]
    pub fp: u8,
    /// DAB Mode ID (1 = Mode I, 2 = Mode II, 3 = Mode III, 4 = Mode IV)
    #[allow(dead_code)]
    pub mid: u8,
    /// Frame length (MST in 32-bit words)
    #[allow(dead_code)]
    pub fl: u16,
    /// Sub-channel characterization table (stack-allocated, max 64 entries per spec)
    pub stc: ArrayVec<StcEntry, 64>,
    /// Minor Network Status Change
    pub mnsc: u16,
    /// FIC data (slice into raw ETI buffer — no allocation)
    pub fic: &'a [u8],
    /// Sub-channel stream data slices (no allocation — each points into the raw ETI buffer)
    pub streams: ArrayVec<&'a [u8], 64>,
}

/// Error type for ETI parsing
#[derive(Debug, thiserror::Error)]
pub enum EtiError {
    #[error("frame too short: expected {expected}, got {got}")]
    FrameTooShort { expected: usize, got: usize },
    #[allow(dead_code)]
    #[error("invalid FSYNC bytes: {0:02x} {1:02x} {2:02x}")]
    InvalidFsync(u8, u8, u8),
    #[error("invalid ETI header CRC: expected {expected:04x}, got {got:04x}")]
    HeaderCrcMismatch { expected: u16, got: u16 },
    #[error("unsupported mode: {0}")]
    UnsupportedMode(u8),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// State for FSYNC tracking (alternates each frame)
#[derive(Debug, Default)]
pub struct FsyncState {
    /// Expected FSYNC pattern for next frame, or None = not yet synchronized
    expected: Option<[u8; 3]>,
}

impl FsyncState {
    pub fn new() -> Self {
        Self { expected: None }
    }

    /// Validate and update FSYNC, returns true if valid
    pub fn check(&mut self, fsync: [u8; 3]) -> bool {
        match self.expected {
            None => {
                // Accept first FSYNC, set complement as next expected
                self.expected = Some([!fsync[0], !fsync[1], !fsync[2]]);
                true
            }
            Some(expected) => {
                if fsync == expected {
                    // Toggle to the complement for the next frame
                    self.expected = Some([!fsync[0], !fsync[1], !fsync[2]]);
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.expected = None;
    }
}

/// Parse a single 6144-byte ETI-NI frame.
///
/// Returns an `EtiFrame<'_>` whose `fic` and `streams` fields are zero-copy
/// slices into `raw` — no heap allocation is performed.
pub fn parse_frame(raw: &[u8]) -> Result<EtiFrame<'_>, EtiError> {
    if raw.len() < ETI_FRAME_SIZE {
        return Err(EtiError::FrameTooShort {
            expected: ETI_FRAME_SIZE,
            got: raw.len(),
        });
    }

    let err = raw[0];
    let _fsync = [raw[1], raw[2], raw[3]];

    // FC block
    let fct = raw[4];
    let ficf = (raw[5] >> 7) != 0;
    let nst = raw[5] & 0x7f;
    let fp = (raw[6] >> 5) & 0x07;
    let mid = (raw[6] >> 3) & 0x03;

    if mid == 0 || mid > 4 {
        return Err(EtiError::UnsupportedMode(mid));
    }

    let fl_hi = (raw[6] & 0x07) as u16;
    let fl = (fl_hi << 8) | raw[7] as u16;

    // Parse STC entries (stack-allocated — no heap allocation)
    let stc_start = 8usize;
    let stc_bytes = nst as usize * 4;
    let mut stc = ArrayVec::<StcEntry, 64>::new();
    for i in 0..nst as usize {
        if stc.is_full() {
            break;
        } // safety: DAB spec max = 64 SCIDs
        let base = stc_start + i * 4;
        let b = &raw[base..base + 4];
        let scid = b[0] >> 2;
        let sad = (((b[0] & 0x03) as u16) << 8) | b[1] as u16;
        let tpl = b[2] >> 2;
        let stl = (((b[2] & 0x03) as u16) << 8) | b[3] as u16;
        stc.push(StcEntry {
            scid,
            sad,
            tpl,
            stl,
        });
    }

    // EOH: MNSC (2 bytes) + HCRC (2 bytes)
    let eoh_start = stc_start + stc_bytes;
    let mnsc = ((raw[eoh_start] as u16) << 8) | raw[eoh_start + 1] as u16;
    let header_crc_stored = ((raw[eoh_start + 2] as u16) << 8) | raw[eoh_start + 3] as u16;
    let header_crc_calced = crc16_ccitt(&raw[4..eoh_start + 2]);
    if header_crc_stored != header_crc_calced {
        return Err(EtiError::HeaderCrcMismatch {
            expected: header_crc_calced,
            got: header_crc_stored,
        });
    }

    // MST starts after EOH
    let mst_start = eoh_start + 4;
    // FIC data — zero-copy slice into raw
    let fic_fibs = if ficf {
        FIBS_PER_FRAME[mid as usize - 1]
    } else {
        0
    };
    let fic_size = fic_fibs * 32;
    let fic: &[u8] = if ficf && mst_start + fic_size <= raw.len() {
        &raw[mst_start..mst_start + fic_size]
    } else {
        &[] // &'static [u8] coerces to &'a [u8] via covariance
    };

    // Sub-channel stream data — zero-copy slices into raw (no per-stream allocation)
    let mut streams = ArrayVec::<&[u8], 64>::new();
    let mut stream_offset = mst_start + fic_size;
    for entry in &stc {
        if streams.is_full() {
            break;
        }
        let stream_size = entry.stl as usize * 8;
        let end = stream_offset + stream_size;
        let stream: &[u8] = if end <= raw.len() {
            &raw[stream_offset..end]
        } else {
            &[]
        };
        streams.push(stream);
        stream_offset = end;
    }

    Ok(EtiFrame {
        err,
        fct,
        ficf,
        nst,
        fp,
        mid,
        fl,
        stc,
        mnsc,
        fic,
        streams,
    })
}

fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc ^ 0xFFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid ETI-NI frame for testing.
    /// Mode I, 1 sub-channel, FICF=1, NST=1, STL=8 CUs
    fn make_test_frame(fct: u8, fsync: [u8; 3]) -> Vec<u8> {
        let mut frame = vec![0u8; ETI_FRAME_SIZE];
        // ERR
        frame[0] = 0xff;
        // FSYNC
        frame[1] = fsync[0];
        frame[2] = fsync[1];
        frame[3] = fsync[2];
        // FC: FCT=fct, FICF=1, NST=1, FP=0, MID=1 (Mode I), FL
        let nst: u8 = 1;
        let ficf: bool = true;
        let mid: u8 = 1;
        let fic_size = 3 * 32; // 96 bytes
        let stream_size = 8 * 8; // STL=8, 64 bytes
        let mst_size = fic_size + stream_size;
        let fl = (mst_size / 4) as u16; // 40
        frame[4] = fct;
        frame[5] = ((ficf as u8) << 7) | (nst & 0x7f);
        frame[6] = ((mid & 0x03) << 3) | ((fl >> 8) as u8 & 0x07);
        frame[7] = (fl & 0xff) as u8;
        // STC[0]: SCID=3, SAD=0, TPL=0x22, STL=8
        let scid: u8 = 3;
        let sad: u16 = 0;
        let tpl: u8 = 0x22;
        let stl: u16 = 8;
        frame[8] = (scid << 2) | ((sad >> 8) as u8 & 0x03);
        frame[9] = (sad & 0xff) as u8;
        frame[10] = (tpl << 2) | ((stl >> 8) as u8 & 0x03);
        frame[11] = (stl & 0xff) as u8;
        // EOH
        let eoh_start = 8 + nst as usize * 4;
        frame[eoh_start] = 0xff; // MNSC hi
        frame[eoh_start + 1] = 0xff; // MNSC lo
        frame[eoh_start + 2] = 0x00; // HCRC hi (not checked)
        frame[eoh_start + 3] = 0x00; // HCRC lo
                                     // Fill FIC with known pattern
        let mst_start = eoh_start + 4;
        for i in 0..fic_size {
            frame[mst_start + i] = (i & 0xff) as u8;
        }
        // Fill stream with known pattern
        for i in 0..stream_size {
            frame[mst_start + fic_size + i] = 0xA5;
        }
        let header_crc = crc16_ccitt(&frame[4..eoh_start + 2]);
        frame[eoh_start + 2] = (header_crc >> 8) as u8;
        frame[eoh_start + 3] = (header_crc & 0xff) as u8;
        frame
    }

    #[test]
    fn test_parse_frame_header() {
        let frame = make_test_frame(42, [0x07, 0x3a, 0xb6]);
        let parsed = parse_frame(&frame).unwrap();
        assert_eq!(parsed.err, 0xff);
        assert_eq!(parsed.fct, 42);
        assert!(parsed.ficf);
        assert_eq!(parsed.nst, 1);
        assert_eq!(parsed.mid, 1); // Mode I
        assert_eq!(parsed.fl, 40); // (96 + 64) / 4 = 40
    }

    #[test]
    fn test_parse_stc_entry() {
        let frame = make_test_frame(0, [0x07, 0x3a, 0xb6]);
        let parsed = parse_frame(&frame).unwrap();
        assert_eq!(parsed.stc.len(), 1);
        let entry = &parsed.stc[0];
        assert_eq!(entry.scid, 3);
        assert_eq!(entry.sad, 0);
        assert_eq!(entry.tpl, 0x22);
        assert_eq!(entry.stl, 8);
    }

    #[test]
    fn test_parse_fic_data() {
        let frame = make_test_frame(0, [0x07, 0x3a, 0xb6]);
        let parsed = parse_frame(&frame).unwrap();
        assert_eq!(parsed.fic.len(), 96); // 3 FIBs × 32 bytes (Mode I)
                                          // FIC is filled with sequential bytes 0..95
        for (i, &b) in parsed.fic.iter().enumerate() {
            assert_eq!(b, (i & 0xff) as u8);
        }
    }

    #[test]
    fn test_parse_stream_data() {
        let frame = make_test_frame(0, [0x07, 0x3a, 0xb6]);
        let parsed = parse_frame(&frame).unwrap();
        assert_eq!(parsed.streams.len(), 1);
        assert_eq!(parsed.streams[0].len(), 64); // STL=8 × 8 bytes
        assert!(parsed.streams[0].iter().all(|&b| b == 0xA5));
    }

    #[test]
    fn test_parse_mnsc() {
        let frame = make_test_frame(0, [0x07, 0x3a, 0xb6]);
        let parsed = parse_frame(&frame).unwrap();
        assert_eq!(parsed.mnsc, 0xFFFF);
    }

    #[test]
    fn test_frame_too_short() {
        let short = vec![0u8; 100];
        let result = parse_frame(&short);
        assert!(matches!(result, Err(EtiError::FrameTooShort { .. })));
    }

    #[test]
    fn test_real_eti_frame_header() {
        // Test against known first frame from test-local/multiplex.eti
        // Frame 0: FCT=160, FICF=1, NST=12, MID=1, FL=829
        // STC[0]: SCID=1, SAD=0, TPL=0x22, STL=33
        let raw = [
            0xff_u8, 0x07, 0x3a, 0xb6, // ERR + FSYNC
            0xa0, 0x8c, 0x8b, 0x3d, // FC
            0x04, 0x00, 0x88, 0x21, // STC[0]
        ];
        // Parse just the FC part manually
        let fct = raw[4];
        let ficf = (raw[5] >> 7) != 0;
        let nst = raw[5] & 0x7f;
        let mid = (raw[6] >> 3) & 0x03;
        let fl = (((raw[6] & 0x07) as u16) << 8) | raw[7] as u16;
        assert_eq!(fct, 160);
        assert!(ficf);
        assert_eq!(nst, 12);
        assert_eq!(mid, 1);
        assert_eq!(fl, 829);

        // STC[0]
        let b = &raw[8..12];
        let scid = b[0] >> 2;
        let sad = (((b[0] & 0x03) as u16) << 8) | b[1] as u16;
        let tpl = b[2] >> 2;
        let stl = (((b[2] & 0x03) as u16) << 8) | b[3] as u16;
        assert_eq!(scid, 1);
        assert_eq!(sad, 0);
        assert_eq!(tpl, 0x22);
        assert_eq!(stl, 33);
    }

    #[test]
    fn test_fsync_alternates() {
        let mut state = FsyncState::new();
        let fsync_a = [0x07u8, 0x3a, 0xb6];
        let fsync_b = [0xf8u8, 0xc5, 0x49]; // bitwise NOT of fsync_a

        // First frame: any FSYNC accepted
        assert!(state.check(fsync_a));
        // Second frame must be the complement
        assert!(state.check(fsync_b));
        // Third frame must be back to original
        assert!(state.check(fsync_a));
        // Wrong pattern should fail
        assert!(!state.check(fsync_a)); // expected fsync_b
    }
}
