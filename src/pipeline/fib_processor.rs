// FIB processor - converted from fib-processor.cpp (eti-cmdline)

use crate::pipeline::dab_constants::*;

/// Returns `true` when `label` is a plausible human-readable service label.
///
/// A FIC-CRC false positive during sync loss can produce FIG 1 bodies full of
/// random bytes.  After EBU Latin decoding, those bytes may yield Unicode
/// control characters (e.g. U+000C form-feed).  We discard any label that is
/// empty or that contains a control character.  Valid DAB service labels are
/// display strings intended for human consumption (ETSI EN 300 401 §8.1.13).
fn is_valid_label(label: &str) -> bool {
    !label.is_empty() && label.chars().all(|c| !c.is_control())
}

/// Convert an EBU Latin byte to its UTF-8 character (ETSI EN 300 401 Table 1).
///
/// Covers all code points including the 0x00–0x1F special range, the ASCII
/// overrides at 0x24/0x5C/0x5E/0x60, and the 0x7B–0xFF high-byte block.
/// Bytes that map to an empty string (e.g. 0x00) fall back to the raw byte
/// cast to `char` so callers can decide whether to skip them.
fn ebu_to_char(ch: u8) -> char {
    // Delegate to the canonical EBU Latin LUT shared with the audio layer.
    // ETSI EN 300 401 §8.1.13
    let s = crate::audio::ebu_latin::ebu_latin_char_to_utf8_string(ch);
    s.chars().next().unwrap_or(ch as char)
}

// UEP protection level table (ETSI EN 300 401 Page 50)
static PROT_LEVEL: [[i32; 3]; 64] = [
    [16, 5, 32],
    [21, 4, 32],
    [24, 3, 32],
    [29, 2, 32],
    [35, 1, 32],
    [24, 5, 48],
    [29, 4, 48],
    [35, 3, 48],
    [42, 2, 48],
    [52, 1, 48],
    [29, 5, 56],
    [35, 4, 56],
    [42, 3, 56],
    [52, 2, 56],
    [32, 5, 64],
    [42, 4, 64],
    [48, 3, 64],
    [58, 2, 64],
    [70, 1, 64],
    [40, 5, 80],
    [52, 4, 80],
    [58, 3, 80],
    [70, 2, 80],
    [84, 1, 80],
    [48, 5, 96],
    [58, 4, 96],
    [70, 3, 96],
    [84, 2, 96],
    [104, 1, 96],
    [58, 5, 112],
    [70, 4, 112],
    [84, 3, 112],
    [104, 2, 112],
    [64, 5, 128],
    [84, 4, 128],
    [96, 3, 128],
    [116, 2, 128],
    [140, 1, 128],
    [80, 5, 160],
    [104, 4, 160],
    [116, 3, 160],
    [140, 2, 160],
    [168, 1, 160],
    [96, 5, 192],
    [116, 4, 192],
    [140, 3, 192],
    [168, 2, 192],
    [208, 1, 192],
    [116, 5, 224],
    [140, 4, 224],
    [168, 3, 224],
    [208, 2, 224],
    [232, 1, 224],
    [128, 5, 256],
    [168, 4, 256],
    [192, 3, 256],
    [232, 2, 256],
    [280, 1, 256],
    [160, 5, 320],
    [208, 4, 320],
    [280, 2, 320],
    [192, 5, 384],
    [280, 3, 384],
    [416, 1, 384],
];

#[derive(Clone, Default)]
struct SubChannel {
    in_use: bool,
    id: i16,
    start_cu: i16,
    uep_flag: bool,
    uep_index: i16,
    protlev: i16,
    size: i16,
    bitrate: i16,
}

#[derive(Clone, Default)]
struct ServiceLabel {
    has_name: bool,
    label: String,
}

#[derive(Clone, Default)]
struct ServiceId {
    in_use: bool,
    service_id: i32,
    service_label: ServiceLabel,
}

#[allow(clippy::type_complexity)]
pub struct FibProcessor {
    sub_channels: [SubChannel; 64],
    list_of_services: Vec<ServiceId>,
    cif_count_hi: i16,
    cif_count_lo: i16,
    is_synced: bool,
    // Callbacks
    pub ensemble_name_cb: Option<std::sync::Arc<dyn Fn(&str, u32) + Send + Sync>>,
    pub program_name_cb: Option<std::sync::Arc<dyn Fn(&str, i32) + Send + Sync>>,
}

impl Default for FibProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl FibProcessor {
    pub fn new() -> Self {
        FibProcessor {
            sub_channels: std::array::from_fn(|_| SubChannel::default()),
            list_of_services: vec![ServiceId::default(); 64],
            cif_count_hi: -1,
            cif_count_lo: -1,
            is_synced: false,
            ensemble_name_cb: None,
            program_name_cb: None,
        }
    }

    pub fn process_fib(&mut self, p: &[u8], _fib: u16) {
        let mut processed_bytes: i32 = 0;
        while processed_bytes < 30 {
            let offset = processed_bytes as usize * 8;
            // Need at least 8 bits for the FIG header (type + length).
            if offset + 8 > p.len() {
                break;
            }
            let fig_type = get_bits_3(p, offset);
            // Read the FIG length field before dispatching so we can verify
            // the full FIG body fits within the FIB (ETSI EN 300 401 §5.2.1).
            let length = get_bits_5(p, offset + 3) as i32;
            let fig_end_bit = offset + (length as usize + 1) * 8;
            if fig_end_bit > p.len() {
                // Truncated or corrupted FIG — discard the rest of this FIB.
                break;
            }
            match fig_type {
                0 => self.process_fig0(p, offset),
                1 => self.process_fig1(p, offset),
                7 => return,
                _ => {}
            }
            processed_bytes += length + 1;
        }
    }

    fn process_fig0(&mut self, d: &[u8], base: usize) {
        let extension = get_bits_5(d, base + 8 + 3);
        match extension {
            0 => self.fig0_extension0(d, base),
            1 => self.fig0_extension1(d, base),
            _ => {}
        }
    }

    fn fig0_extension0(&mut self, d: &[u8], base: usize) {
        self.cif_count_hi = (get_bits_5(d, base + 16 + 19) % 20) as i16;
        self.cif_count_lo = (get_bits_8(d, base + 16 + 24) % 250) as i16;
    }

    fn fig0_extension1(&mut self, d: &[u8], base: usize) {
        let mut used: usize = 2;
        let length = get_bits_5(d, base + 3) as usize;
        let pd_bit = get_bits_1(d, base + 8 + 2);
        while used < length.saturating_sub(1) {
            used = self.handle_fig0_ext1(d, base, used, pd_bit as u8);
        }
    }

    fn handle_fig0_ext1(&mut self, d: &[u8], base: usize, offset: usize, _pd: u8) -> usize {
        let bit_offset = base + offset * 8;
        let sub_ch_id = get_bits_6(d, bit_offset) as usize;
        let start_cu = get_bits(d, bit_offset + 6, 10) as i16;

        self.sub_channels[sub_ch_id].id = sub_ch_id as i16;
        self.sub_channels[sub_ch_id].start_cu = start_cu;
        self.sub_channels[sub_ch_id].uep_flag = get_bits_1(d, bit_offset + 16) == 0;

        if self.sub_channels[sub_ch_id].uep_flag {
            // Short form (UEP)
            let uep_index = get_bits_6(d, bit_offset + 18) as usize;
            self.sub_channels[sub_ch_id].uep_index = uep_index as i16;
            if uep_index < 64 {
                self.sub_channels[sub_ch_id].size = PROT_LEVEL[uep_index][0] as i16;
                self.sub_channels[sub_ch_id].protlev = PROT_LEVEL[uep_index][1] as i16;
                self.sub_channels[sub_ch_id].bitrate = PROT_LEVEL[uep_index][2] as i16;
            }
            self.sub_channels[sub_ch_id].in_use = true;
            return offset + 3; // 24 bits = 3 bytes
        }

        // Long form (EEP)
        let option = get_bits_3(d, bit_offset + 17) as i16;
        let prot_level = get_bits(d, bit_offset + 20, 2) as i16;
        let sub_chan_size = get_bits(d, bit_offset + 22, 10) as i16;
        self.sub_channels[sub_ch_id].size = sub_chan_size;

        if option == 0 {
            // A Level protection
            self.sub_channels[sub_ch_id].protlev = prot_level;
            self.sub_channels[sub_ch_id].bitrate = match prot_level {
                0 => sub_chan_size / 12 * 8,
                1 => sub_chan_size / 8 * 8,
                2 => sub_chan_size / 6 * 8,
                3 => sub_chan_size / 4 * 8,
                _ => 0,
            };
        } else if option == 1 {
            // B Level protection
            self.sub_channels[sub_ch_id].protlev = prot_level + (1 << 2);
            self.sub_channels[sub_ch_id].bitrate = match prot_level {
                0 => sub_chan_size / 27 * 32,
                1 => sub_chan_size / 21 * 32,
                2 => sub_chan_size / 18 * 32,
                3 => sub_chan_size / 15 * 32,
                _ => 0,
            };
        }
        self.sub_channels[sub_ch_id].in_use = true;
        offset + 4 // 32 bits = 4 bytes
    }

    fn process_fig1(&mut self, d: &[u8], base: usize) {
        let char_set = get_bits_4(d, base + 8);
        let oe = get_bits_1(d, base + 8 + 4);
        let extension = get_bits_3(d, base + 8 + 5);

        if oe == 1 {
            return;
        }

        match extension {
            0 => {
                // Ensemble label
                let sid = get_bits(d, base + 16, 16) as u32;
                if char_set <= 16 {
                    let mut label = String::with_capacity(16);
                    for i in 0..16 {
                        let ch = get_bits_8(d, base + 32 + 8 * i) as u8;
                        if ch != 0 {
                            label.push(ebu_to_char(ch));
                        }
                    }
                    if !is_valid_label(&label) {
                        tracing::debug!(
                            "FIG 1/0: rejected corrupt ensemble label (SId=0x{:04X})",
                            sid
                        );
                        return;
                    }
                    if let Some(ref cb) = self.ensemble_name_cb {
                        cb(&label, sid);
                    }
                    self.is_synced = true;
                }
            }
            1 => {
                // Service label (16-bit SId)
                let sid = get_bits(d, base + 16, 16) as i32;
                if char_set <= 16 {
                    let svc = self.find_service_id(sid);
                    if !self.list_of_services[svc].service_label.has_name {
                        let mut label = String::with_capacity(16);
                        for i in 0..16 {
                            let ch = get_bits_8(d, base + 32 + 8 * i) as u8;
                            if ch != 0 {
                                label.push(ebu_to_char(ch));
                            }
                        }
                        if !is_valid_label(&label) {
                            tracing::debug!(
                                "FIG 1/1: rejected corrupt service label (SId=0x{:04X})",
                                sid
                            );
                            return;
                        }
                        self.list_of_services[svc].service_label.label = label.clone();
                        self.list_of_services[svc].service_label.has_name = true;
                        if let Some(ref cb) = self.program_name_cb {
                            cb(&label, sid);
                        }
                    }
                }
            }
            5 => {
                // Service label (32-bit SId)
                let sid = get_lbits(d, base + 16, 32) as i32;
                if char_set <= 16 {
                    let svc = self.find_service_id(sid);
                    if !self.list_of_services[svc].service_label.has_name {
                        let mut label = String::with_capacity(16);
                        for i in 0..16 {
                            let ch = get_bits_8(d, base + 48 + 8 * i) as u8;
                            if ch != 0 {
                                label.push(ebu_to_char(ch));
                            }
                        }
                        label.push_str(" (data)");
                        if !is_valid_label(&label) {
                            tracing::debug!(
                                "FIG 1/5: rejected corrupt service label (SId=0x{:08X})",
                                sid as u32
                            );
                            return;
                        }
                        self.list_of_services[svc].service_label.label = label.clone();
                        self.list_of_services[svc].service_label.has_name = true;
                        if let Some(ref cb) = self.program_name_cb {
                            cb(&label, sid);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn find_service_id(&mut self, service_id: i32) -> usize {
        // Find existing
        for i in 0..64 {
            if self.list_of_services[i].in_use && self.list_of_services[i].service_id == service_id
            {
                return i;
            }
        }
        // Find free slot
        for i in 0..64 {
            if !self.list_of_services[i].in_use {
                self.list_of_services[i].in_use = true;
                self.list_of_services[i].service_id = service_id;
                self.list_of_services[i].service_label.has_name = false;
                return i;
            }
        }
        0
    }

    pub fn get_channel_info(&self, n: usize) -> ChannelData {
        let sc = &self.sub_channels[n];
        ChannelData {
            in_use: sc.in_use,
            id: sc.id,
            start_cu: sc.start_cu,
            uep_flag: sc.uep_flag,
            protlev: sc.protlev,
            size: sc.size,
            bitrate: sc.bitrate,
        }
    }

    pub fn get_cif_count(&self) -> (i16, i16) {
        (self.cif_count_hi, self.cif_count_lo)
    }

    pub fn clear_ensemble(&mut self) {
        for sc in self.sub_channels.iter_mut() {
            *sc = SubChannel::default();
        }
        for svc in self.list_of_services.iter_mut() {
            *svc = ServiceId::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ── EBU Latin ─────────────────────────────────────────────────────────────

    #[test]
    fn test_ebu_to_char_ascii() {
        assert_eq!(ebu_to_char(b'A'), 'A');
        assert_eq!(ebu_to_char(b' '), ' ');
        assert_eq!(ebu_to_char(b'0'), '0');
    }

    #[test]
    fn test_ebu_to_char_metropolitain() {
        // 0x82 = 'é' in EBU Latin (ETSI EN 300 401 Table 1)
        assert_eq!(ebu_to_char(0x82), 'é');
    }

    #[test]
    fn test_ebu_to_char_accented() {
        assert_eq!(ebu_to_char(0x80), 'á');
        assert_eq!(ebu_to_char(0x81), 'à');
        assert_eq!(ebu_to_char(0x83), 'è');
        assert_eq!(ebu_to_char(0x90), 'â');
        assert_eq!(ebu_to_char(0x91), 'ä');
        assert_eq!(ebu_to_char(0x9B), 'ç');
    }

    #[test]
    fn test_ebu_to_char_euro() {
        assert_eq!(ebu_to_char(0xA9), '€');
    }

    /// Bytes 0x7B–0x7F must yield EBU Latin chars, not raw ASCII punctuation.
    /// Without the delegation fix, these would have returned `{|}~DEL`.
    #[test]
    fn test_ebu_to_char_7b_7f_range() {
        assert_eq!(ebu_to_char(0x7B), '«'); // U+00AB
        assert_eq!(ebu_to_char(0x7D), '»'); // U+00BB
        assert_eq!(ebu_to_char(0x7F), 'Ħ'); // U+0126
    }

    /// ASCII-override bytes must return their EBU Latin counterparts, not ASCII.
    #[test]
    fn test_ebu_to_char_ascii_overrides() {
        assert_eq!(ebu_to_char(0x24), 'ł'); // U+0142 — was '$'
        assert_eq!(ebu_to_char(0x5E), 'Ł'); // U+0141 — was '^'
        assert_eq!(ebu_to_char(0x60), 'Ą'); // U+0104 — was '`'
    }

    // ── process_fib helpers ───────────────────────────────────────────────────

    /// Convert a byte slice into a 256-element bit array (`u8` per bit, MSB first)
    /// as expected by `process_fib` and the `get_bits_*` family.
    fn bytes_to_fib(bytes: &[u8]) -> [u8; 256] {
        let mut fib = [0u8; 256];
        for (byte_idx, &byte) in bytes.iter().enumerate() {
            for bit in 0..8usize {
                fib[byte_idx * 8 + bit] = (byte >> (7 - bit)) & 1;
            }
        }
        fib
    }

    // ── process_fib — FIG end marker (type 7) ─────────────────────────────────

    /// A FIG with type = 7 (ETSI EN 300 401 §8.1.1) signals the end of the FIG
    /// list.  process_fib must return immediately and never invoke any callback.
    #[test]
    fn process_fib_type7_end_marker_returns_early() {
        // Header byte: type=7 (0b111), length=0 → 0b111_00000 = 0xE0
        let fib = bytes_to_fib(&[0xE0]);
        let called = Arc::new(Mutex::new(false));
        let called2 = called.clone();
        let mut p = FibProcessor::new();
        p.ensemble_name_cb = Some(Arc::new(move |_, _| *called2.lock().unwrap() = true));
        p.process_fib(&fib, 0);
        assert!(
            !*called.lock().unwrap(),
            "callback must not fire for FIG type 7"
        );
    }

    // ── process_fib — bounds check (regression for index-out-of-bounds panic) ─

    /// A FIG whose length field claims more bytes than remain in the FIB must
    /// be silently discarded.  Before the fix, this caused an index-out-of-bounds
    /// panic inside `get_bits_*` (ETSI EN 300 401 §5.2.1).
    ///
    /// Layout:
    ///   FIG 0 at offset 0  – type=0, length=0  (8 bits, valid)
    ///   FIG 0 at offset 8  – type=0, length=31 → fig_end_bit = 8+32×8 = 264 > 256 → break
    #[test]
    fn process_fib_oversized_length_does_not_panic() {
        let mut fib = [0u8; 256];
        // Second FIG header starts at bit offset 8.
        // Bits 11-15 are the length field; set all five to 1 → length = 31.
        fib[11] = 1;
        fib[12] = 1;
        fib[13] = 1;
        fib[14] = 1;
        fib[15] = 1;
        // Must not panic.
        FibProcessor::new().process_fib(&fib, 0);
    }

    // ── process_fib — FIG 1/0: ensemble label ────────────────────────────────

    /// A well-formed FIG 1/0 (ensemble label, ETSI EN 300 401 §8.1.13) must
    /// invoke `ensemble_name_cb` with the decoded label and SId.
    #[test]
    fn process_fib_ensemble_label_callback_invoked() {
        // Build FIG 1/0 as real bytes, then convert to bit array.
        //
        // Byte layout (ETSI EN 300 401 §8.1.13):
        //  [0]      header  : type=1 (001), length=21 (10101) → 0x35
        //  [1]      ext hdr : charset=0 (0000), OE=0, extension=0 (000) → 0x00
        //  [2..3]   SId     : 0xF043
        //  [4..19]  label   : "NRJ             " (16 bytes, space-padded, ASCII)
        //  [20..21] char flag: not read by process_fig1 → 0x00, 0x00
        let label_bytes: [u8; 16] = {
            let mut b = [0u8; 16]; // null-pad so get_bits_8 ch==0 check skips them
            b[0] = b'N';
            b[1] = b'R';
            b[2] = b'J';
            b
        };
        let mut raw = [0u8; 22];
        raw[0] = 0x35; // type=1, length=21
        raw[1] = 0x00; // charset=0, OE=0, ext=0
        raw[2] = 0xF0; // SId high byte
        raw[3] = 0x43; // SId low  byte
        raw[4..20].copy_from_slice(&label_bytes);
        // raw[20..21] = 0x00 (flag, padding)

        // Append a FIG end marker (type=7) so the loop stops cleanly.
        let mut fig_bytes = raw.to_vec();
        fig_bytes.push(0xE0); // FIG end marker
        let fib = bytes_to_fib(&fig_bytes);

        let received: Arc<Mutex<Option<(String, u32)>>> = Arc::new(Mutex::new(None));
        let received2 = received.clone();
        let mut p = FibProcessor::new();
        p.ensemble_name_cb = Some(Arc::new(move |name, sid| {
            *received2.lock().unwrap() = Some((name.to_owned(), sid));
        }));
        p.process_fib(&fib, 0);

        let r = received.lock().unwrap();
        let (name, sid) = r.as_ref().expect("ensemble_name_cb was not invoked");
        assert_eq!(name, "NRJ", "decoded label mismatch");
        assert_eq!(*sid, 0xF043, "decoded SId mismatch");
    }

    // ── is_valid_label ────────────────────────────────────────────────────────

    #[test]
    fn valid_label_accepts_normal_ascii() {
        assert!(is_valid_label("NRJ"));
        assert!(is_valid_label("BBC Radio 4"));
    }

    #[test]
    fn valid_label_accepts_ebu_accented() {
        // Characters from the EBU Latin high range are valid
        assert!(is_valid_label("Métropole"));
        assert!(is_valid_label("Ö3"));
    }

    #[test]
    fn valid_label_rejects_empty() {
        assert!(!is_valid_label(""));
    }

    #[test]
    fn valid_label_rejects_control_chars() {
        // \x0c (form feed) is the exact char seen in the corrupt label from the
        // FIC-CRC false-positive event (2026-04-10T19:12:50)
        assert!(!is_valid_label("abc\x0cdef"));
        assert!(!is_valid_label("\x01garbage"));
        assert!(!is_valid_label("\x1f"));
    }

    // ── process_fib — corrupt label rejected ─────────────────────────────────

    /// A FIG 1/1 whose decoded label contains a control character must NOT
    /// invoke `program_name_cb` and must NOT cache the label.
    /// This guards against FIC-CRC false positives during sync loss.
    ///
    /// Note: EBU Latin bytes 0x09 and 0x0C now correctly decode to printable
    /// Unicode characters (Ȋ and Ġ) and are therefore valid label content.
    /// Byte 0x0B maps to an empty string in the EBU Latin table, and falls
    /// back to the raw `\x0B` (vertical tab) via `unwrap_or(ch as char)` —
    /// a true control character that `is_valid_label` rejects.
    #[test]
    fn process_fib_corrupt_label_not_cached() {
        let mut label_bytes = [0u8; 16];
        label_bytes[0] = b'X';
        label_bytes[1] = 0x0B; // 0x0B decodes to '\x0B' (vertical tab — control char)
        label_bytes[2] = b'Z';

        let mut raw = [0u8; 22];
        raw[0] = 0x35; // type=1, length=21
        raw[1] = 0x01; // charset=0, OE=0, ext=1
        raw[2] = 0xAB;
        raw[3] = 0xCD;
        raw[4..20].copy_from_slice(&label_bytes);

        let mut fig_bytes = raw.to_vec();
        fig_bytes.push(0xE0);
        let fib = bytes_to_fib(&fig_bytes);

        let called = Arc::new(Mutex::new(false));
        let called2 = called.clone();
        let mut p = FibProcessor::new();
        p.program_name_cb = Some(Arc::new(move |_, _| *called2.lock().unwrap() = true));
        p.process_fib(&fib, 0);

        assert!(
            !*called.lock().unwrap(),
            "program_name_cb must not fire for a label with control characters"
        );
    }

    /// A FIG 1/5 (32-bit SId service label) whose decoded label contains a
    /// control character must also be rejected.
    #[test]
    fn process_fib_corrupt_label_32bit_sid_not_cached() {
        let mut label_bytes = [0u8; 16];
        label_bytes[0] = b'D';
        label_bytes[1] = 0x0B; // 0x0B decodes to '\x0B' (vertical tab — control char)
        label_bytes[2] = b'B';

        // FIG 1/5 layout (ETSI EN 300 401 §8.1.14.2):
        //  [0]      header  : type=1 (001), length=23 (10111) → 0x37
        //  [1]      ext hdr : charset=0, OE=0, extension=5 (101) → 0x05
        //  [2..5]   SId     : 4 bytes (32-bit)
        //  [6..21]  label   : 16 bytes
        //  [22..23] char flag
        let mut raw = [0u8; 24];
        raw[0] = 0x37; // type=1, length=23
        raw[1] = 0x05; // charset=0, OE=0, ext=5
        raw[2] = 0xB0;
        raw[3] = 0x58;
        raw[4] = 0xE2;
        raw[5] = 0xE3;
        raw[6..22].copy_from_slice(&label_bytes);

        let mut fig_bytes = raw.to_vec();
        fig_bytes.push(0xE0);
        let fib = bytes_to_fib(&fig_bytes);

        let called = Arc::new(Mutex::new(false));
        let called2 = called.clone();
        let mut p = FibProcessor::new();
        p.program_name_cb = Some(Arc::new(move |_, _| *called2.lock().unwrap() = true));
        p.process_fib(&fib, 0);

        assert!(
            !*called.lock().unwrap(),
            "program_name_cb must not fire for a 32-bit SId label with control characters"
        );
    }

    // ── process_fib — FIG 1/1: service label ─────────────────────────────────

    /// A well-formed FIG 1/1 (service label, ETSI EN 300 401 §8.1.14.1) must
    /// invoke `program_name_cb` with the decoded service name and SId.
    #[test]
    fn process_fib_service_label_callback_invoked() {
        // Byte layout (ETSI EN 300 401 §8.1.14.1):
        //  [0]      header  : type=1 (001), length=21 (10101) → 0x35
        //  [1]      ext hdr : charset=0, OE=0, extension=1 (001) → 0x01
        //  [2..3]   SId     : 0xF2F8  (NRJ)
        //  [4..19]  label   : "NRJ             "
        //  [20..21] char flag: 0x00, 0x00
        let label_bytes: [u8; 16] = {
            let mut b = [0u8; 16]; // null-pad so get_bits_8 ch==0 check skips them
            b[0] = b'N';
            b[1] = b'R';
            b[2] = b'J';
            b
        };
        let mut raw = [0u8; 22];
        raw[0] = 0x35; // type=1, length=21
        raw[1] = 0x01; // charset=0, OE=0, ext=1
        raw[2] = 0xF2; // SId high byte
        raw[3] = 0xF8; // SId low  byte
        raw[4..20].copy_from_slice(&label_bytes);

        let mut fig_bytes = raw.to_vec();
        fig_bytes.push(0xE0); // FIG end marker
        let fib = bytes_to_fib(&fig_bytes);

        let received: Arc<Mutex<Option<(String, i32)>>> = Arc::new(Mutex::new(None));
        let received2 = received.clone();
        let mut p = FibProcessor::new();
        p.program_name_cb = Some(Arc::new(move |name, sid| {
            *received2.lock().unwrap() = Some((name.to_owned(), sid));
        }));
        p.process_fib(&fib, 0);

        let r = received.lock().unwrap();
        let (name, sid) = r.as_ref().expect("program_name_cb was not invoked");
        assert_eq!(name, "NRJ", "decoded service name mismatch");
        assert_eq!(*sid, 0xF2F8_u32 as i32, "decoded SId mismatch");
    }
}
