/// ETI serializer: converts in-memory `DabFrame` values to 6 144-byte ETI-NI frames.
///
/// This module isolates all ETI binary construction from the OFDM pipeline.
/// Used exclusively by the `iq2eti` sub-command; the `iq2pcm` sub-command skips
/// this module entirely and passes `DabFrame` directly to the audio decoder.
///
/// Reference: ETSI ETS 300 799 §3 — ETI(NI) frame structure.
use std::sync::mpsc;
use std::thread;

use crate::dab_frame::DabFrame;

/// ETI frame size in bytes.  ETSI ETS 300 799 §3.2.
pub const ETI_FRAME_BYTES: usize = 6144;

/// Callback type for writing completed ETI frames.
pub type EtiWriterFn = Box<dyn Fn(&[u8]) + Send>;

/// CRC-16/CCITT lookup table (polynomial 0x1021, initial value 0xFFFF).
/// Used for HCRC and EOF-CRC.  ETSI ETS 300 799 §3.2.
static CRC_TAB_1021: [u16; 256] = [
    0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50a5, 0x60c6, 0x70e7, 0x8108, 0x9129, 0xa14a, 0xb16b,
    0xc18c, 0xd1ad, 0xe1ce, 0xf1ef, 0x1231, 0x0210, 0x3273, 0x2252, 0x52b5, 0x4294, 0x72f7, 0x62d6,
    0x9339, 0x8318, 0xb37b, 0xa35a, 0xd3bd, 0xc39c, 0xf3ff, 0xe3de, 0x2462, 0x3443, 0x0420, 0x1401,
    0x64e6, 0x74c7, 0x44a4, 0x5485, 0xa56a, 0xb54b, 0x8528, 0x9509, 0xe5ee, 0xf5cf, 0xc5ac, 0xd58d,
    0x3653, 0x2672, 0x1611, 0x0630, 0x76d7, 0x66f6, 0x5695, 0x46b4, 0xb75b, 0xa77a, 0x9719, 0x8738,
    0xf7df, 0xe7fe, 0xd79d, 0xc7bc, 0x48c4, 0x58e5, 0x6886, 0x78a7, 0x0840, 0x1861, 0x2802, 0x3823,
    0xc9cc, 0xd9ed, 0xe98e, 0xf9af, 0x8948, 0x9969, 0xa90a, 0xb92b, 0x5af5, 0x4ad4, 0x7ab7, 0x6a96,
    0x1a71, 0x0a50, 0x3a33, 0x2a12, 0xdbfd, 0xcbdc, 0xfbbf, 0xeb9e, 0x9b79, 0x8b58, 0xbb3b, 0xab1a,
    0x6ca6, 0x7c87, 0x4ce4, 0x5cc5, 0x2c22, 0x3c03, 0x0c60, 0x1c41, 0xedae, 0xfd8f, 0xcdec, 0xddcd,
    0xad2a, 0xbd0b, 0x8d68, 0x9d49, 0x7e97, 0x6eb6, 0x5ed5, 0x4ef4, 0x3e13, 0x2e32, 0x1e51, 0x0e70,
    0xff9f, 0xefbe, 0xdfdd, 0xcffc, 0xbf1b, 0xaf3a, 0x9f59, 0x8f78, 0x9188, 0x81a9, 0xb1ca, 0xa1eb,
    0xd10c, 0xc12d, 0xf14e, 0xe16f, 0x1080, 0x00a1, 0x30c2, 0x20e3, 0x5004, 0x4025, 0x7046, 0x6067,
    0x83b9, 0x9398, 0xa3fb, 0xb3da, 0xc33d, 0xd31c, 0xe37f, 0xf35e, 0x02b1, 0x1290, 0x22f3, 0x32d2,
    0x4235, 0x5214, 0x6277, 0x7256, 0xb5ea, 0xa5cb, 0x95a8, 0x8589, 0xf56e, 0xe54f, 0xd52c, 0xc50d,
    0x34e2, 0x24c3, 0x14a0, 0x0481, 0x7466, 0x6447, 0x5424, 0x4405, 0xa7db, 0xb7fa, 0x8799, 0x97b8,
    0xe75f, 0xf77e, 0xc71d, 0xd73c, 0x26d3, 0x36f2, 0x0691, 0x16b0, 0x6657, 0x7676, 0x4615, 0x5634,
    0xd94c, 0xc96d, 0xf90e, 0xe92f, 0x99c8, 0x89e9, 0xb98a, 0xa9ab, 0x5844, 0x4865, 0x7806, 0x6827,
    0x18c0, 0x08e1, 0x3882, 0x28a3, 0xcb7d, 0xdb5c, 0xeb3f, 0xfb1e, 0x8bf9, 0x9bd8, 0xabbb, 0xbb9a,
    0x4a75, 0x5a54, 0x6a37, 0x7a16, 0x0af1, 0x1ad0, 0x2ab3, 0x3a92, 0xfd2e, 0xed0f, 0xdd6c, 0xcd4d,
    0xbdaa, 0xad8b, 0x9de8, 0x8dc9, 0x7c26, 0x6c07, 0x5c64, 0x4c45, 0x3ca2, 0x2c83, 0x1ce0, 0x0cc1,
    0xef1f, 0xff3e, 0xcf5d, 0xdf7c, 0xaf9b, 0xbfba, 0x8fd9, 0x9ff8, 0x6e17, 0x7e36, 0x4e55, 0x5e74,
    0x2e93, 0x3eb2, 0x0ed1, 0x1ef0,
];

fn crc16(data: &[u8], initial: u16) -> u16 {
    let mut crc = initial;
    for &b in data {
        let idx = (((b as u16) ^ (crc >> 8)) & 0xFF) as usize;
        crc = CRC_TAB_1021[idx] ^ (crc << 8);
    }
    crc
}

/// Serializer: runs on its own thread, drains a `DabFrame` channel, writes ETI bytes.
pub struct EtiSerializer {
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl EtiSerializer {
    /// Spawn the serializer thread.  It will run until the sender side of `rx` is dropped.
    pub fn new(rx: mpsc::Receiver<DabFrame>, writer: EtiWriterFn) -> Self {
        let handle = thread::spawn(move || {
            // Pre-allocate the ETI output buffer once for the lifetime of the thread.
            // ETSI ETS 300 799 §3.2: fixed frame size of 6 144 bytes.
            let mut buf = Box::new([0u8; ETI_FRAME_BYTES]);
            for frame in rx {
                serialize_frame(&frame, &mut buf);
                writer(&*buf);
            }
        });
        EtiSerializer {
            thread_handle: Some(handle),
        }
    }

    /// Wait for the serializer thread to finish (call after dropping the sender).
    pub fn join(mut self) {
        if let Some(h) = self.thread_handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for EtiSerializer {
    fn drop(&mut self) {
        if let Some(h) = self.thread_handle.take() {
            let _ = h.join();
        }
    }
}

/// Build a complete ETI-NI frame from a `DabFrame`.
///
/// Layout (ETSI ETS 300 799 §3.2):
/// ```text
/// [0]        ERR  (0xFF = no error)
/// [1..3]     FSYNC (alternates on cif_count_lo parity)
/// [4]        FCT  (cif_count_lo)
/// [5]        FICF | NST
/// [6..7]     FP | MID | FL
/// [8..8+NST*4-1]  STC entries (4 bytes each)
/// [8+NST*4..+1]   MNSC (0xFFFF)
/// [+2..+3]   HCRC
/// [data_off..data_off+ficl*4]  FIC data
/// [..fp-1]   MST (subchannel payloads)
/// [fp..fp+3] EOF (CRC + RFU + TIST)
/// [..6143]   padding 0x55
/// ```
pub fn serialize_frame(frame: &DabFrame, buf: &mut [u8; ETI_FRAME_BYTES]) {
    buf.fill(0x55); // padding

    let cif_lo = frame.cif_count_lo;
    let cif_hi = frame.cif_count_hi;

    // ── SYNC ───────────────────────────────────────────────────────────────────
    // ERR — ETSI ETS 300 799 §3.2.1
    buf[0] = 0xFF;

    // FSYNC alternates on cif_count_lo LSB — ETSI ETS 300 799 §3.2.1
    if (cif_lo & 1) != 0 {
        buf[1] = 0xF8;
        buf[2] = 0xC5;
        buf[3] = 0x49;
    } else {
        buf[1] = 0x07;
        buf[2] = 0x3A;
        buf[3] = 0xB6;
    }

    // ── FC ────────────────────────────────────────────────────────────────────
    // FCT (Frame Counter) — ETSI ETS 300 799 §3.2.2
    buf[4] = cif_lo;

    let nst = frame.subchannels.len() as u8;
    // FL = NST + 1 (EOH) + 24 (FIC words Mode I) + sum(STL) words — ETS 300 799 §3.2.2
    let fl_mst: u16 = frame
        .subchannels
        .iter()
        .map(|s| (s.descriptor.bitrate * 3) / 4)
        .sum();
    let fl: u16 = nst as u16 + 1 + 24 + fl_mst;

    // FICF=1 | NST
    buf[5] = 0x80 | nst;

    // FP (Frame Phase = (cif_hi*250+cif_lo) % 8) | MID=1 (Mode I) | FL[10:8]
    let fp_val = (((cif_hi as u32 * 250) + cif_lo as u32) % 8) as u8;
    buf[6] = (fp_val << 5) | (0x01 << 3) | ((fl >> 8) as u8 & 0x07);
    buf[7] = (fl & 0xFF) as u8;

    // ── STC entries (4 bytes each) ────────────────────────────────────────────
    // ETSI ETS 300 799 §3.2.2
    let mut fp_ofs: usize = 8;
    for s in &frame.subchannels {
        let d = &s.descriptor;
        let sad = d.start_cu;
        let tpl: u8 = if d.uep_flag {
            0x10 | (d.protlev.saturating_sub(1))
        } else {
            0x20 | d.protlev
        };
        let stl: u16 = (d.bitrate * 3) / 8;

        buf[fp_ofs] = (s.subchid << 2) | ((sad >> 8) as u8 & 0x03);
        fp_ofs += 1;
        buf[fp_ofs] = (sad & 0xFF) as u8;
        fp_ofs += 1;
        buf[fp_ofs] = (tpl << 2) | ((stl >> 8) as u8 & 0x03);
        fp_ofs += 1;
        buf[fp_ofs] = (stl & 0xFF) as u8;
        fp_ofs += 1;
    }

    // ── EOH: MNSC + HCRC ─────────────────────────────────────────────────────
    // ETSI ETS 300 799 §3.2.3
    buf[fp_ofs] = 0xFF; // MNSC
    fp_ofs += 1;
    buf[fp_ofs] = 0xFF;
    fp_ofs += 1;

    // HCRC over FC..MNSC (bytes 4..fp_ofs)
    let hcrc = !crc16(&buf[4..fp_ofs], 0xFFFF);
    buf[fp_ofs] = ((hcrc >> 8) & 0xFF) as u8;
    fp_ofs += 1;
    buf[fp_ofs] = (hcrc & 0xFF) as u8;
    fp_ofs += 1;

    let data_start = fp_ofs;

    // ── FIC data (96 bytes, Mode I = 24 × 32-bit words) ─────────────────────
    // ETSI EN 300 401 §3.2.1, ETS 300 799 §3.2.4
    buf[fp_ofs..fp_ofs + 96].copy_from_slice(frame.fic_data.as_ref());
    fp_ofs += 96;

    // ── MST: subchannel payloads ──────────────────────────────────────────────
    for s in &frame.subchannels {
        let len = s.data.len();
        if fp_ofs + len <= ETI_FRAME_BYTES - 8 {
            buf[fp_ofs..fp_ofs + len].copy_from_slice(&s.data);
            fp_ofs += len;
        }
    }

    // ── EOF: CRC over FIC+MST, RFU, TIST ────────────────────────────────────
    // ETSI ETS 300 799 §3.2.5
    let eof_crc = !crc16(&buf[data_start..fp_ofs], 0xFFFF);
    buf[fp_ofs] = ((eof_crc >> 8) & 0xFF) as u8;
    fp_ofs += 1;
    buf[fp_ofs] = (eof_crc & 0xFF) as u8;
    fp_ofs += 1;
    // RFU
    buf[fp_ofs] = 0xFF;
    fp_ofs += 1;
    buf[fp_ofs] = 0xFF;
    fp_ofs += 1;
    // TIST (all 0xFF = unknown)
    buf[fp_ofs] = 0xFF;
    fp_ofs += 1;
    buf[fp_ofs] = 0xFF;
    fp_ofs += 1;
    buf[fp_ofs] = 0xFF;
    fp_ofs += 1;
    buf[fp_ofs] = 0xFF;
}

// ────────────────────────────────────────────────────────────────────────────

// Keep the original EtiWriterFn type alias; re-export Arc for convenience.
pub use std::sync::Arc as _Arc;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dab_frame::SubchannelDescriptor;
    use crate::eti2pcm::eti_frame::parse_eti_frame;
    use crate::eti2pcm::eti_reader::EtiReader;
    use std::io::Cursor;
    use std::sync::Arc;

    /// Build a minimal `DabFrame` with a single subchannel of `payload_len` bytes.
    /// payload_len must be a multiple of 3 (bitrate = payload_len / 3 kb/s).
    fn make_frame(cif_lo: u8, payload_len: usize) -> DabFrame {
        let mut frame = DabFrame::new([0xABu8; 96], 0, cif_lo);
        let data: Arc<[u8]> = Arc::from(vec![0x5Au8; payload_len].as_slice());
        // bitrate (kb/s) such that bitrate * 3 = payload_len bytes per ETI frame.
        // ETSI ETS 300 799 §3: payload bytes = bitrate * 24ms = bitrate * 3 bytes.
        let bitrate = (payload_len as u16) / 3;
        let desc = SubchannelDescriptor {
            start_cu: 0,
            uep_flag: false,
            protlev: 1,
            bitrate,
        };
        frame.push_subchannel(0, data, desc);
        frame
    }

    // ── ERR byte ─────────────────────────────────────────────────────────────

    #[test]
    fn err_byte_is_0xff() {
        // ETSI ETS 300 799 §3.2.1: ERR = 0xFF means no error
        let frame = DabFrame::new([0u8; 96], 0, 0);
        let mut buf = [0u8; ETI_FRAME_BYTES];
        serialize_frame(&frame, &mut buf);
        assert_eq!(buf[0], 0xFF);
    }

    // ── FSYNC alternation ─────────────────────────────────────────────────────

    #[test]
    fn fsync_alternates_on_cif_count_lo_parity() {
        // ETSI ETS 300 799 §3.2.1
        let mut buf = [0u8; ETI_FRAME_BYTES];

        let frame0 = DabFrame::new([0u8; 96], 0, 0); // even → 0x073AB6
        serialize_frame(&frame0, &mut buf);
        assert_eq!(
            &buf[1..4],
            &[0x07, 0x3A, 0xB6],
            "even cif_lo must use FSYNC0"
        );

        let frame1 = DabFrame::new([0u8; 96], 0, 1); // odd → 0xF8C549
        serialize_frame(&frame1, &mut buf);
        assert_eq!(
            &buf[1..4],
            &[0xF8, 0xC5, 0x49],
            "odd cif_lo must use FSYNC1"
        );
    }

    // ── FCT ───────────────────────────────────────────────────────────────────

    #[test]
    fn fct_matches_cif_count_lo() {
        let frame = DabFrame::new([0u8; 96], 0, 77);
        let mut buf = [0u8; ETI_FRAME_BYTES];
        serialize_frame(&frame, &mut buf);
        assert_eq!(buf[4], 77, "FCT must equal cif_count_lo");
    }

    // ── HCRC ──────────────────────────────────────────────────────────────────

    #[test]
    fn hcrc_is_valid() {
        // ETSI ETS 300 799 §3.2.3: HCRC computed over FC..MNSC
        let frame = make_frame(4, 192);
        let mut buf = [0u8; ETI_FRAME_BYTES];
        serialize_frame(&frame, &mut buf);

        // Locate HCRC: byte 8 + NST*4 + 2 (after MNSC)
        let nst = frame.subchannels.len();
        let mnsc_end = 8 + nst * 4 + 2;
        let stored_hcrc = ((buf[mnsc_end] as u16) << 8) | buf[mnsc_end + 1] as u16;

        let computed = !crc16(&buf[4..mnsc_end], 0xFFFF);
        assert_eq!(stored_hcrc, computed, "HCRC mismatch");
    }

    // ── NST / FICF ────────────────────────────────────────────────────────────

    #[test]
    fn ficf_set_and_nst_matches_subchannels() {
        let frame = make_frame(0, 192);
        let mut buf = [0u8; ETI_FRAME_BYTES];
        serialize_frame(&frame, &mut buf);

        assert_eq!(buf[5] >> 7, 1, "FICF must be set");
        assert_eq!(buf[5] & 0x7F, 1, "NST must be 1 for one subchannel");
    }

    // ── FIC data ──────────────────────────────────────────────────────────────

    #[test]
    fn fic_data_copied_into_frame() {
        let mut fic = [0u8; 96];
        for (i, b) in fic.iter_mut().enumerate() {
            *b = (i & 0xFF) as u8;
        }
        let frame = DabFrame::new(fic, 0, 2);
        let mut buf = [0u8; ETI_FRAME_BYTES];
        serialize_frame(&frame, &mut buf);

        // data_start = 8 + NST*4 + 4 (MNSC + HCRC)
        let data_start = 8 + 0 * 4 + 4; // NST=0 for this frame
        assert_eq!(&buf[data_start..data_start + 96], fic.as_ref());
    }

    // ── Round-trip: parse_eti_frame ───────────────────────────────────────────

    #[test]
    fn round_trip_parse_eti_frame_succeeds() {
        // Bytes produced by serialize_frame must be parsable by parse_eti_frame
        let frame = make_frame(6, 192);
        let mut buf = [0u8; ETI_FRAME_BYTES];
        serialize_frame(&frame, &mut buf);

        let parsed = parse_eti_frame(&buf);
        assert!(
            parsed.is_some(),
            "parse_eti_frame must succeed on serialized frame"
        );
        let parsed = parsed.unwrap();
        assert_eq!(parsed.header.nst, 1);
        assert_eq!(parsed.header.ficf, true);
        assert_eq!(parsed.fic_data.len(), 96);
    }

    // ── Round-trip: EtiReader ─────────────────────────────────────────────────

    #[test]
    fn round_trip_eti_reader_finds_fsync() {
        // Two consecutive frames must be found by EtiReader (alternating FSYNC)
        let frame0 = make_frame(0, 192);
        let frame1 = make_frame(1, 192);
        let mut stream = vec![0u8; ETI_FRAME_BYTES * 2];

        let mut buf = [0u8; ETI_FRAME_BYTES];
        serialize_frame(&frame0, &mut buf);
        stream[..ETI_FRAME_BYTES].copy_from_slice(&buf);
        serialize_frame(&frame1, &mut buf);
        stream[ETI_FRAME_BYTES..].copy_from_slice(&buf);

        let mut reader = EtiReader::new(Cursor::new(stream));
        assert!(
            reader.next_frame().unwrap().is_some(),
            "first frame not found"
        );
        assert!(
            reader.next_frame().unwrap().is_some(),
            "second frame not found"
        );
    }

    // ── Thread: EtiSerializer ─────────────────────────────────────────────────

    #[test]
    fn serializer_thread_writes_frames_in_order() {
        let (tx, rx) = mpsc::sync_channel::<DabFrame>(4);
        let written: Arc<std::sync::Mutex<Vec<u8>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let written_clone = written.clone();

        let ser = EtiSerializer::new(
            rx,
            Box::new(move |data: &[u8]| {
                written_clone.lock().unwrap().push(data[4]); // collect FCT values
            }),
        );

        for i in 0u8..4 {
            tx.send(DabFrame::new([0u8; 96], 0, i)).unwrap();
        }
        drop(tx);
        ser.join();

        let fcts = written.lock().unwrap().clone();
        assert_eq!(fcts, vec![0u8, 1, 2, 3], "frames must be written in order");
    }
}
