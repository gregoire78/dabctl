/// PAD (Programme Associated Data) decoder for DLS (Dynamic Label Segment)
/// and MOT (Multimedia Object Transfer) slideshow.
/// Decodes F-PAD + X-PAD from DAB+ AU to extract Dynamic Label text and MOT images.
use crate::audio::crc::crc16_ccitt;
use crate::audio::ebu_latin::ebu_latin_char_to_utf8_string;
use crate::audio::mot_decoder::MotDecoder;
use crate::audio::mot_manager::{MotFile, MotManager};
use encoding_rs::{UTF_16BE, WINDOWS_1252};

const MAX_XPAD_LEN: usize = 196;

/// Content Indicator for X-PAD subfield
#[derive(Debug, Clone)]
struct XpadCi {
    len: usize,
    ci_type: u8,
}

impl XpadCi {
    const LENGTHS: [usize; 8] = [4, 6, 8, 12, 16, 24, 32, 48];

    fn from_raw(raw: u8) -> Self {
        let len_index = (raw >> 5) as usize;
        XpadCi {
            len: Self::LENGTHS.get(len_index).copied().unwrap_or(0),
            ci_type: raw & 0x1F,
        }
    }
}

/// A complete Dynamic Label
#[derive(Debug, Clone)]
pub struct DynamicLabel {
    pub text: String,
    pub charset: u8,
}

/// DL segment for reassembly
#[derive(Debug, Clone)]
struct DlSegment {
    data: Vec<u8>,
    seg_num: u8,
    first: bool,
    last: bool,
    toggle: bool,
    charset: u8,
}

/// DL segment reassembler - collects segments into complete label
struct DlReassembler {
    segments: Vec<DlSegment>,
    current_toggle: Option<bool>,
}

impl DlReassembler {
    fn new() -> Self {
        DlReassembler {
            segments: Vec::new(),
            current_toggle: None,
        }
    }

    fn reset(&mut self) {
        self.segments.clear();
        self.current_toggle = None;
    }

    /// Add a segment. Returns true if a complete label is now available.
    fn add_segment(&mut self, seg: DlSegment) -> bool {
        // Toggle change means new label
        if let Some(t) = self.current_toggle {
            if t != seg.toggle {
                self.segments.clear();
            }
        }
        self.current_toggle = Some(seg.toggle);

        // Reject duplicate segments (like dablin)
        let seg_num = seg.seg_num;
        if self.segments.iter().any(|s| s.seg_num == seg_num) {
            return self.check_complete();
        }
        self.segments.push(seg);

        self.check_complete()
    }

    fn check_complete(&self) -> bool {
        // Need a first segment (seg_num=0)
        let has_first = self.segments.iter().any(|s| s.first);
        let has_last = self.segments.iter().any(|s| s.last);
        if !has_first || !has_last {
            return false;
        }

        // Check contiguous segment numbers
        let mut nums: Vec<u8> = self.segments.iter().map(|s| s.seg_num).collect();
        nums.sort();
        nums.dedup();
        let max = *nums.last().unwrap();
        nums.len() == (max + 1) as usize
    }

    fn assemble(&self) -> Option<DynamicLabel> {
        if !self.check_complete() {
            return None;
        }

        let mut sorted = self.segments.clone();
        sorted.sort_by_key(|s| s.seg_num);

        let charset = sorted.first().map(|s| s.charset).unwrap_or(0);
        let mut raw = Vec::new();
        for seg in &sorted {
            raw.extend_from_slice(&seg.data);
        }

        let text = decode_label_text(&raw, charset, false);
        Some(DynamicLabel { text, charset })
    }
}

/// DGLI (Data Group Length Indicator) decoder
struct DgliDecoder {
    data: Vec<u8>,
    len: Option<usize>,
}

impl DgliDecoder {
    fn new() -> Self {
        DgliDecoder {
            data: Vec::new(),
            len: None,
        }
    }

    fn reset(&mut self) {
        self.data.clear();
        self.len = None;
    }

    fn process(&mut self, start: bool, data: &[u8]) {
        if start {
            self.data.clear();
        }
        self.data.extend_from_slice(data);

        // DGLI = 2 bytes + 2 bytes CRC = 4 bytes total
        if self.data.len() >= 4 {
            let crc = crc16_ccitt();
            let crc_stored = (self.data[2] as u16) << 8 | self.data[3] as u16;
            let crc_calced = crc.calc(&self.data[..2]);
            if crc_stored == crc_calced {
                let len = ((self.data[0] as usize) << 8 | self.data[1] as usize) & 0x3FFF;
                tracing::debug!("DGLI received: len={}", len);
                self.len = Some(len);
            } else {
                tracing::debug!("DGLI CRC invalid");
            }
        }
    }

    fn take_len(&mut self) -> usize {
        self.len.take().unwrap_or(0)
    }
}

/// Dynamic Label data group decoder (aligned with dablin's DynamicLabelDecoder).
/// Uses field_len from prefix to determine exact CRC position.
struct DlDataGroupDecoder {
    data: Vec<u8>,
    dg_size_needed: usize,
    crc: crate::audio::crc::CrcCalculator,
}

impl DlDataGroupDecoder {
    const DG_SIZE_MAX: usize = 2 + 16 + 2; // prefix + max_field + CRC

    fn new() -> Self {
        DlDataGroupDecoder {
            data: Vec::with_capacity(Self::DG_SIZE_MAX),
            dg_size_needed: 4, // initial: at least prefix(2) + CRC(2)
            crc: crc16_ccitt(),
        }
    }

    fn reset(&mut self) {
        self.data.clear();
        self.dg_size_needed = 4;
    }

    /// Process a data subfield (like dablin's DataGroup::ProcessDataSubfield + DecodeDataGroup).
    fn process(&mut self, start: bool, data: &[u8]) -> Option<DlSegment> {
        if start {
            self.reset();
        } else {
            // Ignore continuation without prior start
            if self.data.is_empty() {
                return None;
            }
        }

        // Abort if already at needed size
        if self.data.len() >= self.dg_size_needed {
            return None;
        }

        // Append data (up to max buffer)
        let copy_len = (Self::DG_SIZE_MAX - self.data.len()).min(data.len());
        self.data.extend_from_slice(&data[..copy_len]);

        // Wait until we have enough data
        if self.data.len() < self.dg_size_needed {
            return None;
        }

        // Now decode the data group (like dablin's DynamicLabelDecoder::DecodeDataGroup)
        let prefix0 = self.data[0];

        let command = prefix0 & 0x10 != 0;

        let field_len: usize;
        if command {
            match prefix0 & 0x0F {
                0x01 => {
                    // "Remove Label" command — return empty segment to clear label
                    self.reset();
                    return Some(DlSegment {
                        data: Vec::new(),
                        seg_num: 0,
                        first: true,
                        last: true,
                        toggle: prefix0 & 0x80 != 0,
                        charset: 0,
                    });
                }
                0x02 => {
                    // DL Plus command
                    if self.data.len() < 2 {
                        return None;
                    }
                    field_len = (self.data[1] as usize & 0x0F) + 1;
                }
                _ => {
                    // Unknown command, ignore
                    self.reset();
                    return None;
                }
            }
        } else {
            // Segment: field_len = (prefix0 & 0x0F) + 1
            field_len = (prefix0 as usize & 0x0F) + 1;
        }

        let real_len = 2 + field_len; // prefix(2) + field data
        let needed = real_len + 2; // + CRC(2)

        // Update needed size and check
        self.dg_size_needed = needed;
        if self.data.len() < needed {
            return None;
        }

        // CRC check at the correct position (NOT at end of buffer)
        let crc_stored = (self.data[real_len] as u16) << 8 | self.data[real_len + 1] as u16;
        let crc_calced = self.crc.calc(&self.data[..real_len]);
        if crc_stored != crc_calced {
            self.reset();
            return None;
        }

        // Handle command DL Plus — skip for now (just ignore, don't produce segment)
        if command {
            self.reset();
            return None;
        }

        // Parse segment prefix
        let prefix1 = self.data[1];

        let toggle = prefix0 & 0x80 != 0;
        let first = prefix0 & 0x40 != 0;
        let last = prefix0 & 0x20 != 0;

        let seg_num = if first { 0 } else { (prefix1 & 0x70) >> 4 };
        let charset = if first { prefix1 & 0x0F } else { 0 };

        let char_data = &self.data[2..2 + field_len];

        let seg = DlSegment {
            data: char_data.to_vec(),
            seg_num,
            first,
            last,
            toggle,
            charset,
        };

        self.reset();
        Some(seg)
    }
}

/// Result of processing PAD data
#[derive(Default)]
pub struct PadResult {
    pub dynamic_label: Option<DynamicLabel>,
    pub slide: Option<MotFile>,
}

/// Main PAD decoder (DLS + MOT slideshow)
pub struct PadDecoder {
    dl_decoder: DlDataGroupDecoder,
    dl_reassembler: DlReassembler,
    dgli_decoder: DgliDecoder,
    mot_decoder: MotDecoder,
    mot_manager: MotManager,
    mot_app_type: i8,
    /// Single CI stored for non-CI continuation frames (like dablin's last_xpad_ci).
    /// len = total xpad offset consumed in last CI frame,
    /// ci_type = type of last continuation subfield.
    last_xpad_ci: Option<XpadCi>,
    /// Last emitted DLS text, to avoid repeating the same label
    last_dl_text: String,
    xpad_buf: [u8; MAX_XPAD_LEN],
}

impl Default for PadDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl PadDecoder {
    pub fn new() -> Self {
        PadDecoder {
            dl_decoder: DlDataGroupDecoder::new(),
            dl_reassembler: DlReassembler::new(),
            dgli_decoder: DgliDecoder::new(),
            mot_decoder: MotDecoder::new(),
            mot_manager: MotManager::new(),
            mot_app_type: -1,
            last_xpad_ci: None,
            last_dl_text: String::new(),
            xpad_buf: [0u8; MAX_XPAD_LEN],
        }
    }

    pub fn reset(&mut self) {
        self.dl_decoder.reset();
        self.dl_reassembler.reset();
        self.dgli_decoder.reset();
        self.mot_decoder.reset();
        self.mot_manager.reset();
        self.mot_app_type = -1;
        self.last_xpad_ci = None;
        self.last_dl_text.clear();
    }

    /// Set the MOT application type (from FIC, SLS app type).
    /// Typically 12 for MOT Slideshow.
    pub fn set_mot_app_type(&mut self, app_type: i8) {
        self.mot_app_type = app_type;
    }

    /// Process PAD data (X-PAD + F-PAD). Returns Some(DynamicLabel) when a new label is complete.
    /// For backward compatibility, only returns DynamicLabel.
    /// Use `process_full()` to also get slideshow results.
    pub fn process(
        &mut self,
        xpad_data: &[u8],
        xpad_len: usize,
        _exact_xpad_len: bool,
        fpad_data: &[u8; 2],
    ) -> Option<DynamicLabel> {
        self.process_full(xpad_data, xpad_len, _exact_xpad_len, fpad_data)
            .dynamic_label
    }

    /// Process PAD data (X-PAD + F-PAD). Returns both DynamicLabel and MOT slideshow.
    pub fn process_full(
        &mut self,
        xpad_data: &[u8],
        xpad_len: usize,
        _exact_xpad_len: bool,
        fpad_data: &[u8; 2],
    ) -> PadResult {
        let mut result = PadResult::default();

        // Reverse X-PAD byte order (DAB+ provides it reversed for compatibility with DAB)
        let used_len = xpad_len.min(MAX_XPAD_LEN);
        for i in 0..used_len {
            self.xpad_buf[i] = xpad_data[xpad_len - 1 - i];
        }

        let fpad_type = fpad_data[0] >> 6;
        let xpad_ind = (fpad_data[0] & 0x30) >> 4;
        let ci_flag = fpad_data[1] & 0x02 != 0;

        let mut cis = Vec::new();
        let mut cis_len: usize = 0;

        if fpad_type == 0b00 {
            if ci_flag {
                match xpad_ind {
                    0b01 => {
                        // Short X-PAD
                        if used_len >= 1 {
                            let ci_type = self.xpad_buf[0] & 0x1F;
                            if ci_type != 0x00 {
                                cis_len = 1;
                                cis.push(XpadCi { len: 3, ci_type });
                            }
                        }
                    }
                    0b10 => {
                        // Variable size X-PAD
                        cis_len = 0;
                        for k in 0..4 {
                            if used_len < k + 1 {
                                break;
                            }
                            let raw = self.xpad_buf[k];
                            cis_len += 1;
                            if raw & 0x1F == 0x00 {
                                break;
                            }
                            cis.push(XpadCi::from_raw(raw));
                        }
                    }
                    _ => {}
                }
                // Store continuation CI for non-CI frames.
                // ETSI EN 300 401 §7.4.3: after a start subfield in a CI frame,
                // subsequent non-CI frames carry the corresponding continuation type.
                // Start types (even) imply continuation types (odd = start + 1):
                //   DL start (2) → DL continuation (3)
                //   MOT start (app_type) → MOT continuation (app_type + 1)
                if !cis.is_empty() {
                    let mut last_continued_type: Option<u8> = None;
                    for ci in &cis {
                        if ci.ci_type == 2 || ci.ci_type == 3 {
                            last_continued_type = Some(3);
                        } else if self.mot_app_type >= 0 {
                            let app_type = self.mot_app_type as u8;
                            if ci.ci_type == app_type || ci.ci_type == app_type + 1 {
                                last_continued_type = Some(app_type + 1);
                            }
                        }
                    }
                    if let Some(cont_type) = last_continued_type {
                        self.last_xpad_ci = Some(XpadCi {
                            len: 0, // not used; non-CI frames use full X-PAD length
                            ci_type: cont_type,
                        });
                    }
                    // If no continuable type found (e.g. DGLI-only CI frame),
                    // keep previous last_xpad_ci unchanged so ongoing MOT/DLS
                    // continuations are not disrupted.
                }
            } else {
                // No CI flag: use single stored continuation CI (like dablin)
                match xpad_ind {
                    0b01 | 0b10 => {
                        if let Some(ref stored_ci) = self.last_xpad_ci {
                            // Non-CI frame: entire X-PAD data is one continuation subfield
                            cis.push(XpadCi {
                                len: used_len,
                                ci_type: stored_ci.ci_type,
                            });
                            cis_len = 0; // no CI bytes present in non-CI frames
                        }
                    }
                    _ => {}
                }
            }
        }

        if cis.is_empty() {
            return result;
        }

        // Process CI subfields
        let mut xpad_offset = cis_len;

        for ci in &cis {
            if xpad_offset + ci.len > used_len {
                tracing::trace!(
                    "X-PAD subfield overflow: offset={} ci_len={} used_len={} ci_type={} ci_flag={}",
                    xpad_offset, ci.len, used_len, ci.ci_type, ci_flag
                );
                break;
            }

            let subfield = &self.xpad_buf[xpad_offset..xpad_offset + ci.len];

            match ci.ci_type {
                1 => {
                    // DGLI - always self-contained, always a start
                    self.dgli_decoder.process(true, subfield);
                }
                2 | 3 => {
                    // Dynamic Label (start=2, continuation=3)
                    let start = ci.ci_type == 2;
                    if let Some(seg) = self.dl_decoder.process(start, subfield) {
                        if self.dl_reassembler.add_segment(seg) {
                            if let Some(dl) = self.dl_reassembler.assemble() {
                                let text = dl.text.trim().to_string();
                                if !text.is_empty() && text != self.last_dl_text {
                                    self.last_dl_text = text;
                                    result.dynamic_label = Some(dl);
                                }
                            }
                        }
                    }
                }
                _ => {
                    // MOT Data Group (app_type = start, app_type+1 = continuation)
                    if self.mot_app_type >= 0 {
                        let app_type = self.mot_app_type as u8;
                        if ci.ci_type == app_type || ci.ci_type == app_type + 1 {
                            let start = ci.ci_type == app_type;

                            // DGLI len is only valid for the immediate next DG
                            if start {
                                let dgli_len = self.dgli_decoder.take_len();
                                tracing::debug!("MOT start subfield (ci_type={}, dgli_len={})", ci.ci_type, dgli_len);
                                self.mot_decoder.set_len(dgli_len);
                            }

                            if self.mot_decoder.process_subfield(start, subfield) {
                                let dg = self.mot_decoder.get_data_group();
                                tracing::debug!("MOT DG complete, forwarding to mot_manager (dg_len={})", dg.len());
                                let (file, _fraction) = self.mot_manager.handle_data_group(&dg);
                                if let Some(f) = file {
                                    if f.is_image() {
                                        result.slide = Some(f);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            xpad_offset += ci.len;
        }

        result
    }
}

/// Decode label bytes to UTF-8 text
fn decode_label_text(raw: &[u8], charset: u8, mot: bool) -> String {
    // Nettoyage des caractères de contrôle
    let cleaned: Vec<u8> = raw
        .iter()
        .copied()
        .filter(|&ch| ch != 0x00 && ch != 0x0A && ch != 0x0B && ch != 0x1F)
        .collect();
    match charset {
        0 => {
            // EBU Latin
            cleaned
                .iter()
                .map(|&ch| ebu_latin_char_to_utf8_string(ch))
                .collect()
        }
        4 if mot => {
            // ISO-8859-1 (MOT) (utilise WINDOWS_1252, compatible pour DAB)
            let (cow, _, _) = WINDOWS_1252.decode(&cleaned);
            cow.into_owned().to_string()
        }
        6 if !mot => {
            // UCS-2BE (DAB only) : décodage direct via encoding_rs (UTF_16BE)
            if !raw.len().is_multiple_of(2) {
                return String::new();
            }
            let (cow, _, had_errors) = UTF_16BE.decode(raw);
            if had_errors {
                String::new()
            } else {
                cow.into_owned().to_string()
            }
        }
        15 => {
            // UTF-8 (no fallback)
            String::from_utf8(cleaned).unwrap_or_else(|_| String::new())
        }
        _ => {
            // Charset non supporté
            tracing::warn!("DL charset={} non supporté, chaîne ignorée", charset);
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xpad_ci_lengths() {
        assert_eq!(XpadCi::LENGTHS, [4, 6, 8, 12, 16, 24, 32, 48]);
    }

    #[test]
    fn test_xpad_ci_from_raw() {
        let ci = XpadCi::from_raw(0x42); // len_index=2(8), type=2(DL start)
        assert_eq!(ci.len, 8);
        assert_eq!(ci.ci_type, 2);
    }

    #[test]
    fn test_dl_reassembler_single_segment() {
        let mut r = DlReassembler::new();
        let seg = DlSegment {
            data: b"Hello".to_vec(),
            seg_num: 0,
            first: true,
            last: true,
            toggle: false,
            charset: 0,
        };
        assert!(r.add_segment(seg));
        let label = r.assemble().unwrap();
        assert_eq!(label.text, "Hello");
    }

    #[test]
    fn test_dl_reassembler_multi_segment() {
        let mut r = DlReassembler::new();
        let seg1 = DlSegment {
            data: b"Hello ".to_vec(),
            seg_num: 0,
            first: true,
            last: false,
            toggle: false,
            charset: 0,
        };
        assert!(!r.add_segment(seg1));

        let seg2 = DlSegment {
            data: b"World".to_vec(),
            seg_num: 1,
            first: false,
            last: true,
            toggle: false,
            charset: 0,
        };
        assert!(r.add_segment(seg2));
        let label = r.assemble().unwrap();
        assert_eq!(label.text, "Hello World");
    }

    #[test]
    fn test_dl_toggle_resets() {
        let mut r = DlReassembler::new();
        let seg1 = DlSegment {
            data: b"Old".to_vec(),
            seg_num: 0,
            first: true,
            last: true,
            toggle: false,
            charset: 0,
        };
        r.add_segment(seg1);

        // New toggle resets
        let seg2 = DlSegment {
            data: b"New".to_vec(),
            seg_num: 0,
            first: true,
            last: true,
            toggle: true,
            charset: 0,
        };
        assert!(r.add_segment(seg2));
        let label = r.assemble().unwrap();
        assert_eq!(label.text, "New");
    }

    #[test]
    fn test_decode_label_ebu() {
        let raw = [b'A', b'B', b'C'];
        let text = decode_label_text(&raw, 0, false);
        assert_eq!(text, "ABC");
    }

    #[test]
    fn test_decode_label_utf8() {
        let raw = "Héllo".as_bytes().to_vec();
        let text = decode_label_text(&raw, 15, false);
        assert_eq!(text, "Héllo");
    }

    #[test]
    fn test_decode_label_ebu_accents() {
        // 0x82 = EBU index 2 = é (U+00E9), 0x83 = EBU index 3 = è (U+00E8)
        let raw = [b'C', b'a', b'f', 0x82];
        let text = decode_label_text(&raw, 0, false);
        assert_eq!(text, "Café");
    }

    #[test]
    fn test_decode_label_iso8859_1() {
        // ISO 8859-1: 0xE9 = é, charset 4
        let raw = b"Cavaill\xe9-Roux";
        let text = decode_label_text(raw, 4, true);
        assert_eq!(text, "Cavaillé-Roux");
    }

    #[test]
    fn test_decode_label_iso8859_1_multiple_accents() {
        // ISO 8859-1: 0xE0=à, 0xE9=é, 0xF4=ô
        let raw = b"M\xe9tropolitain d\xe9j\xe0 pr\xf4t";
        let text = decode_label_text(raw, 4, true);
        assert_eq!(text, "Métropolitain déjà prôt");
    }

    #[test]
    fn test_decode_label_ucs2() {
        // UCS-2 BE: "Café" = 0x0043 0x0061 0x0066 0x00E9
        let raw = [0x00, 0x43, 0x00, 0x61, 0x00, 0x66, 0x00, 0xE9];
        let text = decode_label_text(&raw, 6, false);
        assert_eq!(text, "Café");
    }

    #[test]
    fn test_decode_label_unknown_charset_fallback() {
        // Unknown charset (e.g. 5) : DABlin-style = chaîne vide
        let raw = b"Cavaill\xe9";
        let text = decode_label_text(raw, 5, false);
        assert_eq!(text, "");
    }

    #[test]
    fn test_decode_label_iso8859_1_filters_control_chars() {
        let raw = [b'A', 0x00, b'B', 0x0A, b'C', 0x1F, b'D'];
        let text = decode_label_text(&raw, 4, true);
        assert_eq!(text, "ABCD"); // 0x00, 0x0A, 0x1F filtered out
    }

    #[test]
    fn test_decode_label_utf8_with_ebu_fallback() {
        // Broadcaster claims UTF-8 (charset 15) mais envoie EBU Latin (non valide UTF-8)
        // DABlin-style : chaîne vide
        let raw = b"Cavaill\x82-Roux";
        let text = decode_label_text(raw, 15, false);
        assert_eq!(text, "");
    }

    #[test]
    fn test_decode_label_ebu_oe_ligature() {
        // 0xF3 in EBU Latin = œ (U+0153), must NOT be decoded as ó (ISO 8859-1)
        let raw = b"le c\xF3ur des Balkans";
        let text = decode_label_text(&raw[..], 0, false);
        assert_eq!(text, "le cœur des Balkans");
    }
}
