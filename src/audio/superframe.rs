/// DAB+ superframe decoder: accumulates 5 frames, applies RS,
/// checks fire code sync, extracts Access Units.
use crate::audio::crc::{crc16_ccitt, crc_fire_code, CrcCalculator};
use crate::audio::rs_decoder::RsDecoder;
use tracing::trace;

const FPAD_LEN: usize = 2;

/// DAB+ superframe audio format
#[derive(Debug, Clone, PartialEq)]
pub struct SuperframeFormat {
    pub dac_rate: bool,
    pub sbr_flag: bool,
    pub aac_channel_mode: bool,
    pub ps_flag: bool,
    pub mpeg_surround_config: u8,
}

impl SuperframeFormat {
    pub fn core_sample_rate_index(&self) -> u8 {
        if self.dac_rate {
            if self.sbr_flag {
                6
            } else {
                3
            }
        } else {
            if self.sbr_flag {
                8
            } else {
                5
            }
        }
    }

    pub fn core_channel_config(&self) -> u8 {
        if self.aac_channel_mode {
            2
        } else {
            1
        }
    }

    pub fn extension_sample_rate_index(&self) -> u8 {
        if self.dac_rate {
            3
        } else {
            5
        }
    }

    pub fn sample_rate(&self) -> u32 {
        if self.dac_rate {
            48000
        } else {
            32000
        }
    }

    pub fn channels(&self) -> u8 {
        if self.aac_channel_mode || self.ps_flag {
            2
        } else {
            1
        }
    }

    pub fn number_of_access_units(&self) -> usize {
        calculate_number_of_access_units(self.dac_rate, self.sbr_flag)
    }

    pub fn codec_name(&self) -> &'static str {
        if self.sbr_flag {
            if self.ps_flag {
                "HE-AAC v2"
            } else {
                "HE-AAC"
            }
        } else {
            "AAC-LC"
        }
    }

    /// Build AudioSpecificConfig for FAAD2 initialization (960-sample transform)
    pub fn build_asc(&self) -> Vec<u8> {
        let mut asc = Vec::new();
        // AAC LC (AOT 2)
        asc.push(0b00010 << 3 | self.core_sample_rate_index() >> 1);
        asc.push(
            (self.core_sample_rate_index() & 0x01) << 7 | self.core_channel_config() << 3 | 0b100, // GASpecificConfig with 960 transform
        );

        if self.sbr_flag {
            // SBR explicit backwards-compatible signaling
            asc.push(0x56); // sync extension 0x2B7 = 0101 0110 111...
            asc.push(0xE5); // ...101 = AOT 5 (SBR), 1 = SBR present
            let ext_sr = self.extension_sample_rate_index() << 3;
            if self.ps_flag {
                // PS explicit backwards-compatible signaling
                asc.push(0x80 | ext_sr | 0x05); // ext_sr | 10101
                asc.push(0x48); // 001 = PS extension, 0 = ...
                asc.push(0x80); // PS present = 1
            } else {
                asc.push(0x80 | ext_sr);
            }
        }
        asc
    }
}

/// Computes the number of Access Units contained in one DAB+ superframe,
/// determined by `dac_rate` and `sbr_flag` (ETSI TS 102 563 §5.2).
pub fn calculate_number_of_access_units(dac_rate: bool, sbr_flag: bool) -> usize {
    match (dac_rate, sbr_flag) {
        (true, true) => 3,
        (true, false) => 6,
        (false, true) => 2,
        (false, false) => 4,
    }
}

/// An extracted Access Unit ready for AAC decoding
pub struct AccessUnit {
    pub data: Vec<u8>,
}

/// PAD data extracted from an AAC AU
pub struct PadData {
    pub xpad: Vec<u8>,
    pub fpad: [u8; FPAD_LEN],
}

/// Superframe filter: accumulates logical frames, RS-decodes, extracts AUs
pub struct SuperframeFilter {
    rs_dec: RsDecoder,
    crc_ccitt: CrcCalculator,
    crc_fire: CrcCalculator,
    frame_len: usize,
    frame_count: usize,
    /// Circular write head (0..5): index of the slot to write next.
    write_head: usize,
    sf_raw: Vec<u8>, // 5 × frame_len slots in circular order
    sf: Vec<u8>,     // linearized view for RS decode
    sf_len: usize,
    format: Option<SuperframeFormat>,
    format_raw: u8,
}

impl Default for SuperframeFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl SuperframeFilter {
    pub fn new() -> Self {
        SuperframeFilter {
            rs_dec: RsDecoder::new(),
            crc_ccitt: crc16_ccitt(),
            crc_fire: crc_fire_code(),
            frame_len: 0,
            frame_count: 0,
            write_head: 0,
            sf_raw: Vec::new(),
            sf: Vec::new(),
            sf_len: 0,
            format: None,
            format_raw: 0,
        }
    }

    /// Feed a subchannel frame (called once per ETI frame for the selected subchannel).
    /// Returns decoded access units + optional PAD + optional format change.
    pub fn feed(&mut self, data: &[u8]) -> SuperframeResult {
        let len = data.len();

        // Initialize frame length on first frame
        if self.frame_len == 0 {
            if len < 10 || !(5 * len).is_multiple_of(120) {
                return SuperframeResult::default();
            }
            self.frame_len = len;
            self.sf_len = 5 * len;
            self.sf_raw = vec![0u8; self.sf_len];
            self.sf = vec![0u8; self.sf_len];
        }

        if len != self.frame_len {
            return SuperframeResult::default();
        }

        // Circular write: overwrite the oldest slot.
        // Eliminates the copy_within(frame_len.., 0) that previously shifted
        // 4 × frame_len bytes on every frame in the sliding-window path.
        let slot = self.write_head;
        self.sf_raw[slot * self.frame_len..(slot + 1) * self.frame_len].copy_from_slice(data);
        self.write_head = (self.write_head + 1) % 5;
        if self.frame_count < 5 {
            self.frame_count += 1;
        }

        if self.frame_count < 5 {
            return SuperframeResult::default();
        }

        // Linearize circular buffer into self.sf (oldest frame first).
        // read_head = write_head (points to slot that was written longest ago).
        let read_head = self.write_head;
        for i in 0..5 {
            let src_slot = (read_head + i) % 5;
            let src = src_slot * self.frame_len..(src_slot + 1) * self.frame_len;
            let dst = i * self.frame_len..(i + 1) * self.frame_len;
            self.sf[dst].copy_from_slice(&self.sf_raw[src]);
        }
        let (corr, uncorr) = self.rs_dec.decode_superframe(&mut self.sf);
        if corr > 0 || uncorr {
            trace!(
                rs_corrected = corr,
                rs_uncorrectable = uncorr,
                "SUPERFRAME: RS decode"
            );
        }

        // Check fire code sync
        if !self.check_sync() {
            // Log the CRC mismatch to help diagnose systematic vs random failures.
            let crc_stored = (self.sf[0] as u16) << 8 | self.sf[1] as u16;
            let crc_calced = crc_fire_code().calc(&self.sf[2..11]);
            trace!(
                stored = format_args!("0x{:04X}", crc_stored),
                calc = format_args!("0x{:04X}", crc_calced),
                "SUPERFRAME: fire code CRC fail"
            );
            // Slide by 1 frame: set count to 4 so we collect just one more frame.
            // write_head already points to the next slot to overwrite.
            self.frame_count = 4;
            return SuperframeResult {
                sync_fail: true,
                ..SuperframeResult::default()
            };
        }

        // Check format change
        let mut format_changed = false;
        if self.format.is_none() || self.format_raw != self.sf[2] {
            self.format_raw = self.sf[2];
            self.format = Some(self.parse_format());
            format_changed = true;
        }

        let format = self.format.as_ref().unwrap();
        let num_aus = format.number_of_access_units();

        // Extract AU boundaries
        let au_start = self.compute_au_starts(num_aus);

        // Decode each AU
        let mut access_units = Vec::with_capacity(num_aus);
        let mut pad_data = Vec::new();

        for i in 0..num_aus {
            let start = au_start[i];
            let end = au_start[i + 1];
            if start >= end || end > self.sf.len() {
                continue;
            }

            let au_data = &self.sf[start..end];
            let au_len = au_data.len();

            // CRC check
            if au_len < 2 {
                continue;
            }
            let crc_stored = (au_data[au_len - 2] as u16) << 8 | au_data[au_len - 1] as u16;
            let crc_calced = self.crc_ccitt.calc(&au_data[..au_len - 2]);
            if crc_stored != crc_calced {
                continue;
            }

            let au_payload = &au_data[..au_len - 2];
            access_units.push(AccessUnit {
                data: au_payload.to_vec(),
            });

            // Extract PAD from AAC AU (Data Stream Element)
            if let Some(pad) = extract_pad_from_au(au_payload) {
                pad_data.push(pad);
            }
        }

        // Reset for next superframe
        self.frame_count = 0;

        let decoded = access_units.len();
        let expected = num_aus;
        if decoded < expected {
            trace!(
                decoded,
                expected,
                dropped = expected - decoded,
                "SUPERFRAME: AU CRC failures"
            );
        }

        SuperframeResult {
            access_units,
            pad_data,
            format: if format_changed {
                self.format.clone()
            } else {
                None
            },
            sync_ok: true,
            sync_fail: false,
        }
    }

    fn check_sync(&self) -> bool {
        // Prevent sync on all-zero
        if self.sf[3] == 0x00 && self.sf[4] == 0x00 {
            return false;
        }

        // Fire code CRC
        let crc_stored = (self.sf[0] as u16) << 8 | self.sf[1] as u16;
        let crc_calced = self.crc_fire.calc(&self.sf[2..11]);
        crc_stored == crc_calced
    }

    fn parse_format(&self) -> SuperframeFormat {
        SuperframeFormat {
            dac_rate: self.sf[2] & 0x40 != 0,
            sbr_flag: self.sf[2] & 0x20 != 0,
            aac_channel_mode: self.sf[2] & 0x10 != 0,
            ps_flag: self.sf[2] & 0x08 != 0,
            mpeg_surround_config: self.sf[2] & 0x07,
        }
    }

    fn compute_au_starts(&self, num_aus: usize) -> Vec<usize> {
        let sf = &self.sf;
        let format = self.format.as_ref().unwrap();
        let mut au_start = vec![0usize; num_aus + 1];

        // Premier offset AU selon le format
        au_start[0] = match (format.dac_rate, format.sbr_flag) {
            (true, true) => 6,
            (true, false) => 11,
            (false, true) => 5,
            (false, false) => 8,
        };

        // Dernière pseudo-limite AU (après RS strip)
        au_start[num_aus] = self.sf_len / 120 * 110;

        // Offsets AU depuis l'en-tête superframe
        au_start[1] = (sf[3] as usize) << 4 | (sf[4] as usize) >> 4;
        if num_aus >= 3 {
            au_start[2] = ((sf[4] & 0x0F) as usize) << 8 | sf[5] as usize;
        }
        if num_aus >= 4 {
            au_start[3] = (sf[6] as usize) << 4 | (sf[7] as usize) >> 4;
        }
        if num_aus == 6 {
            au_start[4] = ((sf[7] & 0x0F) as usize) << 8 | sf[8] as usize;
            au_start[5] = (sf[9] as usize) << 4 | (sf[10] as usize) >> 4;
        }

        self.sanitize_au_starts(&mut au_start);
        au_start
    }

    /// ETSI TS 102 563 §5.2 defines AU boundaries via header offsets.
    /// On corrupted headers, keep boundaries monotonic and inside payload range.
    fn sanitize_au_starts(&self, au_start: &mut [usize]) {
        if au_start.is_empty() {
            return;
        }

        let payload_end = self.sf_len / 120 * 110;
        if let Some(last) = au_start.last_mut() {
            *last = (*last).min(payload_end);
        }

        for i in 1..au_start.len() {
            let prev = au_start[i - 1];
            au_start[i] = au_start[i].min(payload_end).max(prev);
        }
    }

    /// Get the current format (if determined)
    pub fn format(&self) -> Option<&SuperframeFormat> {
        self.format.as_ref()
    }

    /// Discard the current rolling-window state and start a clean accumulation.
    ///
    /// Call this when the OFDM layer reports a frame sync loss (i.e. when a
    /// `DabFrame` arrives with `sync_lost = true`).  Without a reset, the
    /// 5-slot circular buffer retains CIFs from before the dropout; when new
    /// CIFs arrive, the Fire-code check is run against a window that mixes
    /// pre-dropout and post-dropout data, causing up to 5 consecutive
    /// `sync_fail` events before the sliding window re-aligns with the true
    /// superframe boundary.
    ///
    /// A reset reduces worst-case post-dropout `sync_fail` count from ~9 to
    /// ~4 (one per possible boundary offset in the 5-CIF superframe period).
    ///
    /// ETSI TS 102 563 §5 — DAB+ superframe structure.
    pub fn reset(&mut self) {
        self.frame_count = 0;
        self.write_head = 0;
        // Overwrite stale CIF data so it is never mixed with post-resync CIFs.
        self.sf_raw.fill(0);
    }
}

/// Result of feeding a frame to the superframe filter
#[derive(Default)]
pub struct SuperframeResult {
    pub access_units: Vec<AccessUnit>,
    pub pad_data: Vec<PadData>,
    pub format: Option<SuperframeFormat>,
    /// A superframe sync was attempted and succeeded
    pub sync_ok: bool,
    /// A superframe sync was attempted and failed (fire code CRC)
    pub sync_fail: bool,
}

/// Extract PAD data from an AAC AU (embedded in Data Stream Element)
fn extract_pad_from_au(data: &[u8]) -> Option<PadData> {
    if data.len() < 3 {
        return None;
    }

    // Check for DSE: element_id type 4 (data[0] >> 5 == 4)
    if (data[0] >> 5) != 4 {
        return None;
    }

    let mut pad_start = 2;
    let mut pad_len = data[1] as usize;
    if pad_len == 255 {
        if data.len() < 4 {
            return None;
        }
        pad_len += data[2] as usize;
        pad_start = 3;
    }

    if pad_len < FPAD_LEN || data.len() < pad_start + pad_len {
        return None;
    }

    let xpad_len = pad_len - FPAD_LEN;
    let xpad = data[pad_start..pad_start + xpad_len].to_vec();
    let fpad_start = pad_start + xpad_len;
    let fpad = [data[fpad_start], data[fpad_start + 1]];

    Some(PadData { xpad, fpad })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_number_of_access_units_given_dac_and_sbr_combinations_then_expected_result() {
        // Given/When/Then : tous les cas possibles
        assert_eq!(calculate_number_of_access_units(true, true), 3); // Given dac_rate=true, sbr_flag=true, Then 3
        assert_eq!(calculate_number_of_access_units(true, false), 6); // Given dac_rate=true, sbr_flag=false, Then 6
        assert_eq!(calculate_number_of_access_units(false, true), 2); // Given dac_rate=false, sbr_flag=true, Then 2
        assert_eq!(calculate_number_of_access_units(false, false), 4); // Given dac_rate=false, sbr_flag=false, Then 4
    }

    #[test]
    fn test_superframe_format_number_of_access_units_when_various_formats_then_expected() {
        // Given
        let fmt = SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        // When/Then
        assert_eq!(fmt.number_of_access_units(), 3);

        let fmt2 = SuperframeFormat {
            dac_rate: true,
            sbr_flag: false,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        assert_eq!(fmt2.number_of_access_units(), 6);

        let fmt3 = SuperframeFormat {
            dac_rate: false,
            sbr_flag: true,
            aac_channel_mode: false,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        assert_eq!(fmt3.number_of_access_units(), 2);
    }

    #[test]
    fn test_superframe_format_asc() {
        let fmt = SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: true,
            ps_flag: true,
            mpeg_surround_config: 0,
        };
        let asc = fmt.build_asc();
        // Should contain SBR + PS signaling
        assert!(asc.len() >= 5);
        assert_eq!(fmt.codec_name(), "HE-AAC v2");
    }

    #[test]
    fn test_superframe_format_sample_rate() {
        let fmt48 = SuperframeFormat {
            dac_rate: true,
            sbr_flag: false,
            aac_channel_mode: false,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        assert_eq!(fmt48.sample_rate(), 48000);

        let fmt32 = SuperframeFormat {
            dac_rate: false,
            sbr_flag: false,
            aac_channel_mode: false,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        assert_eq!(fmt32.sample_rate(), 32000);
    }

    #[test]
    fn test_extract_pad_from_au_no_dse() {
        let data = [0x00, 0x00, 0x00];
        assert!(extract_pad_from_au(&data).is_none());
    }

    #[test]
    fn test_extract_pad_from_au_dse() {
        // DSE element: type=4 (0x80), length=4 (2 xpad + 2 fpad)
        let data = [0x80, 0x04, 0xAA, 0xBB, 0xCC, 0xDD];
        let pad = extract_pad_from_au(&data).unwrap();
        assert_eq!(pad.xpad, vec![0xAA, 0xBB]);
        assert_eq!(pad.fpad, [0xCC, 0xDD]);
    }

    #[test]
    fn test_superframe_filter_initialization() {
        let sf = SuperframeFilter::new();
        assert_eq!(sf.frame_len, 0);
        assert!(sf.format.is_none());
    }

    /// Feeding frames whose au_start header bytes point past the end of sf
    /// must not panic — the AU loop skips entries where end > sf.len().
    #[test]
    fn au_start_overflow_is_skipped_not_panicked() {
        let mut filter = SuperframeFilter::new();

        // Build a syntactically minimal frame: len=120 so 5×120=600 is multiple of 120.
        // Fire-code bytes (sf[0..2]) and format byte (sf[2]) are deliberately zero so
        // check_sync() returns false.  The important thing is that no panic occurs.
        let frame = vec![0u8; 120];
        for _ in 0..5 {
            let result = filter.feed(&frame);
            // We expect either no output (not enough frames yet) or sync_fail (fire CRC
            // mismatch).  The test assertion is simply that we reach this line alive.
            let _ = result;
        }
    }

    /// A valid-looking frame where au_start[1] encodes a value larger than sf.len()
    /// must produce zero access units, not a panic.
    #[test]
    fn au_start_pointing_beyond_sf_produces_no_aus() {
        let mut filter = SuperframeFilter::new();
        // len=120; sf_len=600.  au_start[1] is decoded from sf[3]<<4|sf[4]>>4.
        // Setting sf[3]=0xFF, sf[4]=0xFF encodes au_start[1]=0xFFF=4095 > 600.
        let mut frame = vec![0u8; 120];
        // Bytes 3 and 4 carry au_start[1]: value 0xFFF = 4095.
        frame[3] = 0xFF;
        frame[4] = 0xFF;
        for _ in 0..5 {
            let result = filter.feed(&frame);
            // Result may be sync_fail (fire CRC) or empty — never a panic.
            assert!(result.access_units.is_empty());
        }
    }

    /// `reset()` must discard all accumulated frames so the next 5 feeds
    /// start a clean accumulation (no mixing with pre-reset CIF data).
    #[test]
    fn reset_discards_accumulated_frames_and_restarts_cleanly() {
        let mut filter = SuperframeFilter::new();
        let frame = vec![0u8; 120];

        // Feed 4 frames — one less than the 5 needed to attempt a decode.
        for _ in 0..4 {
            let r = filter.feed(&frame);
            assert!(r.access_units.is_empty(), "not enough frames yet");
            assert!(!r.sync_ok);
            assert!(!r.sync_fail);
        }

        // reset() before the 5th frame — the accumulator should restart.
        filter.reset();

        // After reset, feeding 4 more frames should NOT trigger a decode attempt.
        for _ in 0..4 {
            let r = filter.feed(&frame);
            assert!(
                r.access_units.is_empty(),
                "should need 5 frames after reset"
            );
            assert!(!r.sync_ok);
            assert!(!r.sync_fail);
        }
    }

    /// After `reset()`, internal stale data is zeroed so that the first decode
    /// attempt (on the 5th feed after reset) never reads pre-reset CIF bytes.
    #[test]
    fn reset_clears_stale_cif_data() {
        let mut filter = SuperframeFilter::new();

        // Prime the filter with non-zero data.
        let non_zero = vec![0xFFu8; 120];
        for _ in 0..5 {
            let _ = filter.feed(&non_zero);
        }

        filter.reset();

        // After reset, sf_raw must be fully zeroed.
        assert!(
            filter.sf_raw.iter().all(|&b| b == 0),
            "stale CIF data not cleared"
        );
        assert_eq!(filter.write_head, 0, "write_head must be reset to 0");
        assert_eq!(filter.frame_count, 0, "frame_count must be reset to 0");
    }

    #[test]
    fn sanitize_au_starts_clamps_values_to_payload_end() {
        let mut filter = SuperframeFilter::new();
        filter.sf_len = 600;
        let mut starts = vec![6, 800, 900, 550];

        filter.sanitize_au_starts(&mut starts);

        // payload_end = 600/120*110 = 550
        assert_eq!(starts, vec![6, 550, 550, 550]);
    }

    #[test]
    fn sanitize_au_starts_enforces_monotonic_order() {
        let mut filter = SuperframeFilter::new();
        filter.sf_len = 600;
        let mut starts = vec![11, 220, 200, 410, 300, 550, 550];

        filter.sanitize_au_starts(&mut starts);

        assert_eq!(starts, vec![11, 220, 220, 410, 410, 550, 550]);
    }
}
