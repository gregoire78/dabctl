use std::collections::BTreeMap;

use encoding_rs::WINDOWS_1252;

use crate::dablin::utils::ebu_latin::ebu_latin_bytes_to_utf8_string;

#[derive(Debug, Clone)]
pub struct PadSlide {
    pub content_name: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct PadEvents {
    pub dynamic_label: Option<String>,
    pub slide: Option<PadSlide>,
}

#[derive(Debug, Clone, Copy)]
struct XpadCi {
    len: usize,
    r#type: u8,
}

#[derive(Debug, Clone)]
struct DlSegment {
    prefix0: u8,
    prefix1: u8,
    chars: Vec<u8>,
}

#[derive(Debug, Default)]
struct DlDecoder {
    segs: BTreeMap<u8, DlSegment>,
    last_toggle: Option<bool>,
    dg_buf: Vec<u8>,
}

impl DlDecoder {
    fn feed(&mut self, start: bool, data: &[u8]) -> Option<String> {
        if start {
            self.dg_buf.clear();
        }

        if !start && self.dg_buf.is_empty() {
            return None;
        }

        self.dg_buf.extend_from_slice(data);
        if self.dg_buf.len() < 4 {
            return None;
        }

        let command = (self.dg_buf[0] & 0x10) != 0;
        let field_len = if command {
            match self.dg_buf[0] & 0x0f {
                0x01 => 0, // remove label command
                0x02 => ((self.dg_buf[1] & 0x0f) as usize) + 1,
                _ => {
                    self.dg_buf.clear();
                    return None;
                }
            }
        } else {
            ((self.dg_buf[0] & 0x0f) as usize) + 1
        };

        // 2-byte header + payload + 2-byte CRC
        let needed = 2 + field_len + 2;
        if self.dg_buf.len() < needed {
            return None;
        }

        let dg = self.dg_buf[..needed].to_vec();
        self.dg_buf.drain(..needed);

        if !crc16_ccitt_ok(&dg) {
            return None;
        }

        if command {
            if (dg[0] & 0x0f) == 0x01 {
                self.segs.clear();
                return Some(String::new());
            }
            // Ignore DL+ command payload in this decoder.
            return None;
        }

        let seg = DlSegment {
            prefix0: dg[0],
            prefix1: dg[1],
            chars: dg[2..2 + field_len].to_vec(),
        };

        let toggle = (seg.prefix0 & 0x80) != 0;
        if let Some(last_toggle) = self.last_toggle {
            if toggle != last_toggle {
                self.segs.clear();
            }
        }
        self.last_toggle = Some(toggle);

        let seg_num = if (seg.prefix0 & 0x40) != 0 {
            0
        } else {
            (seg.prefix1 >> 4) & 0x07
        };

        self.segs.entry(seg_num).or_insert(seg);

        let mut complete_chars = Vec::new();
        let mut found_last = false;
        for i in 0..8u8 {
            let seg_i = self.segs.get(&i)?;
            complete_chars.extend_from_slice(&seg_i.chars);
            if (seg_i.prefix0 & 0x20) != 0 {
                found_last = true;
                break;
            }
        }
        if !found_last {
            return None;
        }

        let charset = self.segs.get(&0).map(|s| s.prefix1 >> 4).unwrap_or(0);

        Some(convert_text_charset(&complete_chars, charset))
    }
}

/// Parse ContentName (parameter 0x0C) from a reassembled MOT Header payload.
/// Layout (ETSI EN 301 234 §5.3 + §6.2.3):
///   Bytes 0-6: core (BodySize 28b, HeaderSize 13b, ContentType 6b, ContentSubType 9b)
///   Bytes 7+:  TLV parameters: PLI(2b)|ParamId(6b) [len] [data…]
fn parse_mot_content_name(header: &[u8]) -> Option<String> {
    if header.len() < 7 {
        return None;
    }
    let mut offset = 7usize;
    while offset < header.len() {
        let pli = header[offset] >> 6;
        let param_id = header[offset] & 0x3F;
        offset += 1;

        let data_len: usize = match pli {
            0b00 => 0,
            0b01 => 1,
            0b10 => 4,
            0b11 => {
                if offset >= header.len() {
                    return None;
                }
                let ext = (header[offset] & 0x80) != 0;
                let lo = (header[offset] & 0x7F) as usize;
                offset += 1;
                if ext {
                    if offset >= header.len() {
                        return None;
                    }
                    let hi = lo;
                    let r = (hi << 8) | header[offset] as usize;
                    offset += 1;
                    r
                } else {
                    lo
                }
            }
            _ => unreachable!(),
        };

        if offset + data_len > header.len() {
            return None;
        }

        if param_id == 0x0C && data_len >= 1 {
            // First byte: charset (high 4 bits) + reserved (low 4 bits)
            let charset = header[offset] >> 4;
            let name_bytes = &header[offset + 1..offset + data_len];
            return Some(convert_text_charset(name_bytes, charset));
        }

        offset += data_len;
    }
    None
}

#[derive(Debug, Default)]
struct MotDecoder {
    // Per-data-group accumulation
    dg_buf: Vec<u8>,
    dgli_len: usize,
    // Per-MOT-object segment reassembly (indexed by seg_number)
    current_transport_id: i32,
    // Body segments (dg_type == 4)
    segments: std::collections::BTreeMap<u16, Vec<u8>>,
    last_seg_number: Option<u16>,
    // Header segments (dg_type == 3) and parsed ContentName
    header_segments: std::collections::BTreeMap<u16, Vec<u8>>,
    header_last_seg: Option<u16>,
    content_name: Option<String>,
}

impl MotDecoder {
    fn new() -> Self {
        Self {
            current_transport_id: -1,
            ..Default::default()
        }
    }

    fn feed(
        &mut self,
        start: bool,
        data: &[u8],
        dgli_len: usize,
        slide_counter: &mut u64,
    ) -> Option<PadSlide> {
        if start {
            self.dg_buf.clear();
            if dgli_len > 0 {
                self.dgli_len = dgli_len;
            }
        } else if self.dg_buf.is_empty() {
            return None;
        }

        self.dg_buf.extend_from_slice(data);

        if self.dgli_len == 0 || self.dg_buf.len() < self.dgli_len {
            return None;
        }

        let dg = self.dg_buf[..self.dgli_len].to_vec();
        self.dg_buf.drain(..self.dgli_len);

        self.handle_mot_data_group(&dg, slide_counter)
    }

    /// Parses the MSC Data Group structure (ETSI EN 301 234 / EN 300 401 §5.3.3.1),
    /// extracts the payload, and reassembles segments like dablin's MOTManager.
    fn handle_mot_data_group(&mut self, dg: &[u8], slide_counter: &mut u64) -> Option<PadSlide> {
        let mut offset = 0usize;

        // General Data Group Header (2 bytes + optional 2-byte extension)
        if dg.len() < offset + 2 {
            return None;
        }
        let extension_flag = (dg[offset] & 0x80) != 0;
        let crc_flag = (dg[offset] & 0x40) != 0;
        let segment_flag = (dg[offset] & 0x20) != 0;
        let user_access_flag = (dg[offset] & 0x10) != 0;
        let dg_type = dg[offset] & 0x0F;
        offset += 2 + if extension_flag { 2 } else { 0 };

        if !crc_flag || !segment_flag || !user_access_flag {
            return None;
        }
        // dg_type 3 = MOT header, 4 = MOT body; only MOT body carries image bytes
        if dg_type != 3 && dg_type != 4 {
            return None;
        }

        // Session Header
        if dg.len() < offset + 3 {
            return None;
        }
        let last_seg = (dg[offset] & 0x80) != 0;
        let seg_number = (((dg[offset] & 0x7F) as u16) << 8) | dg[offset + 1] as u16;
        let transport_id_flag = (dg[offset + 2] & 0x10) != 0;
        let len_indicator = (dg[offset + 2] & 0x0F) as usize;
        offset += 3;

        if !transport_id_flag || len_indicator < 2 {
            return None;
        }
        if dg.len() < offset + len_indicator {
            return None;
        }
        let transport_id = ((dg[offset] as i32) << 8) | dg[offset + 1] as i32;
        offset += len_indicator;

        // Segmentation Header (2 bytes for MOT)
        if dg.len() < offset + 2 {
            return None;
        }
        let seg_size = (((dg[offset] & 0x1F) as usize) << 8) | dg[offset + 1] as usize;
        offset += 2;

        // Verify announced seg_size matches available data (minus 2-byte CRC)
        let crc_len = 2usize;
        if dg.len() < offset + seg_size + crc_len {
            return None;
        }
        // CRC covers everything before the final 2 bytes
        let crc_end = offset + seg_size + crc_len;
        if !crc16_ccitt_ok(&dg[..crc_end]) {
            return None;
        }

        let payload = dg[offset..offset + seg_size].to_vec();

        // On transport ID change, start a new MOT object
        if transport_id != self.current_transport_id {
            self.current_transport_id = transport_id;
            self.segments.clear();
            self.last_seg_number = None;
            self.header_segments.clear();
            self.header_last_seg = None;
            self.content_name = None;
        }

        if dg_type == 3 {
            // MOT Header – accumulate segments and parse ContentName when complete
            self.header_segments.entry(seg_number).or_insert(payload);
            if last_seg {
                self.header_last_seg = Some(seg_number);
            }
            if let Some(hlast) = self.header_last_seg {
                if (0..=hlast).all(|i| self.header_segments.contains_key(&i)) {
                    let mut header_data: Vec<u8> = Vec::new();
                    for i in 0..=hlast {
                        header_data.extend_from_slice(&self.header_segments[&i]);
                    }
                    self.content_name = parse_mot_content_name(&header_data);
                }
            }
            return None;
        }

        // dg_type == 4: MOT body segment
        // Store segment (duplicates ignored)
        self.segments.entry(seg_number).or_insert(payload);
        if last_seg {
            self.last_seg_number = Some(seg_number);
        }

        // Check if all segments [0..=last] are available
        let last = self.last_seg_number?;
        for i in 0..=last {
            if !self.segments.contains_key(&i) {
                return None;
            }
        }

        // All segments present – assemble into one buffer
        let mut image_data: Vec<u8> = Vec::new();
        for i in 0..=last {
            image_data.extend_from_slice(&self.segments[&i]);
        }

        // Reset body for next object
        self.segments.clear();
        self.last_seg_number = None;

        let saved_content_name = self.content_name.clone();

        // Detect image type from magic bytes
        if let Some((mime, ext, img, _)) = extract_image_from_stream(&image_data) {
            *slide_counter += 1;
            let name = saved_content_name
                .unwrap_or_else(|| format!("slide-{:06}.{}", *slide_counter, ext));
            return Some(PadSlide {
                content_name: name,
                content_type: mime.to_string(),
                data: img,
            });
        }

        // If no image magic found, emit raw bytes
        if !image_data.is_empty() {
            *slide_counter += 1;
            let name =
                saved_content_name.unwrap_or_else(|| format!("slide-{:06}.jpg", *slide_counter));
            return Some(PadSlide {
                content_name: name,
                content_type: "image/jpeg".to_string(),
                data: image_data,
            });
        }

        None
    }
}

pub struct PadDecoder {
    dl: DlDecoder,
    mot: MotDecoder,
    last_xpad_ci: Option<XpadCi>,
    dgli_len: usize,
    dgli_buf: Vec<u8>,
    slide_counter: u64,
}

impl PadDecoder {
    pub fn new() -> Self {
        Self {
            dl: DlDecoder::default(),
            mot: MotDecoder::new(),
            last_xpad_ci: None,
            dgli_len: 0,
            dgli_buf: Vec::new(),
            slide_counter: 0,
        }
    }

    pub fn process_au(&mut self, au: &[u8], mot_app_type: Option<u8>) -> PadEvents {
        let mut events = PadEvents::default();
        let (xpad, fpad, exact_xpad_len) =
            extract_pad_from_au(au).unwrap_or_else(|| (Vec::new(), [0x00, 0x00], true));

        // Undo reversed byte order (matches dablin PADDecoder::Process)
        let mut xpad_rev = xpad;
        xpad_rev.reverse();

        let fpad_type = fpad[0] >> 6;
        let xpad_ind = (fpad[0] & 0x30) >> 4;
        let ci_flag = (fpad[1] & 0x02) != 0;

        let prev_xpad_ci = self.last_xpad_ci;
        self.last_xpad_ci = None;

        let ci_lens = [4usize, 6, 8, 12, 16, 24, 32, 48];
        let mut cis: Vec<XpadCi> = Vec::new();
        let mut cis_len = 0usize;

        if fpad_type == 0 {
            if ci_flag {
                match xpad_ind {
                    0b01 => {
                        if xpad_rev.is_empty() {
                            return events;
                        }
                        let t = xpad_rev[0] & 0x1f;
                        if t != 0 {
                            cis_len = 1;
                            cis.push(XpadCi { len: 3, r#type: t });
                        }
                    }
                    0b10 => {
                        for i in 0..4usize {
                            if xpad_rev.len() < i + 1 {
                                return events;
                            }
                            let raw = xpad_rev[i];
                            cis_len += 1;
                            let t = raw & 0x1f;

                            if t == 0 {
                                break;
                            }
                            cis.push(XpadCi {
                                len: ci_lens[(raw >> 5) as usize],
                                r#type: t,
                            });
                        }
                    }
                    _ => {}
                }
            } else if matches!(xpad_ind, 0b01 | 0b10) {
                if let Some(prev) = self.last_xpad_ci {
                    cis.push(prev);
                    cis_len = 0;
                }
            }
        }

        if cis.is_empty() {
            if let Some(prev) = prev_xpad_ci {
                if prev.len <= xpad_rev.len() {
                    cis_len = 0;
                    cis.push(prev);
                } else {
                    return events;
                }
            } else {
                return events;
            }
        }

        let announced_xpad_len = cis_len + cis.iter().map(|ci| ci.len).sum::<usize>();
        if announced_xpad_len > xpad_rev.len() {
            return events;
        }

        let usable_xpad_len = xpad_rev.len();
        if exact_xpad_len && announced_xpad_len < usable_xpad_len {
            // Strict mode like dablin default: discard malformed X-PAD length.
            return events;
        }

        let mot_type = mot_app_type.unwrap_or(12);
        let mut offset = cis_len;
        let mut continued_ci_type: Option<u8> = None;

        for ci in cis {
            if offset + ci.len > usable_xpad_len {
                break;
            }
            let sub = &xpad_rev[offset..offset + ci.len];
            match ci.r#type {
                1 => {
                    if ci_flag {
                        self.dgli_buf.clear();
                    }
                    self.dgli_buf.extend_from_slice(sub);

                    // DGLI Data Group = 2 bytes length + 2 bytes CRC
                    if self.dgli_buf.len() >= 4 && crc16_ccitt_ok(&self.dgli_buf[..4]) {
                        // DGLI: (byte0 & 0x3F) << 8 | byte1  (ETSI EN 300 401 §7.4.2.1)
                        self.dgli_len =
                            ((self.dgli_buf[0] & 0x3F) as usize) << 8 | (self.dgli_buf[1] as usize);
                        self.dgli_buf.clear();
                    }

                    continued_ci_type = Some(1);
                }
                2 | 3 => {
                    if let Some(label) = self.dl.feed(ci.r#type == 2, sub) {
                        events.dynamic_label = Some(label);
                    }
                    continued_ci_type = Some(3);
                }
                t if t == mot_type || t == mot_type.saturating_add(1) => {
                    let dgli_len = self.dgli_len;
                    // DGLI applies to the immediate next DG only.
                    if dgli_len > 0 {
                        self.dgli_len = 0;
                    }
                    if let Some(slide) =
                        self.mot
                            .feed(t == mot_type, sub, dgli_len, &mut self.slide_counter)
                    {
                        events.slide = Some(slide);
                    }
                    continued_ci_type = Some(mot_type.saturating_add(1));
                }
                _ => {
                    continued_ci_type = None;
                }
            }
            offset += ci.len;
        }

        if let Some(t) = continued_ci_type {
            self.last_xpad_ci = Some(XpadCi {
                len: offset,
                r#type: t,
            });
        }
        events
    }
}

fn extract_pad_from_au(au: &[u8]) -> Option<(Vec<u8>, [u8; 2], bool)> {
    // Matches dablin CheckForPAD(): DSE must be the first element.
    if au.len() < 3 || (au[0] >> 5) != 4 {
        return None;
    }

    let mut pad_start = 2usize;
    let mut pad_len = au[1] as usize;
    if pad_len == 255 {
        pad_len += au[2] as usize;
        pad_start += 1;
    }

    if pad_len < 2 || au.len() < pad_start + pad_len {
        return None;
    }

    let xpad_len = pad_len - 2;
    let xpad = au[pad_start..pad_start + xpad_len].to_vec();
    let fpad = [au[pad_start + xpad_len], au[pad_start + xpad_len + 1]];
    Some((xpad, fpad, true))
}

fn convert_text_charset(raw: &[u8], charset: u8) -> String {
    if charset == 0 {
        let s = ebu_latin_bytes_to_utf8_string(raw);
        return s.trim_matches(char::from(0)).trim().to_string();
    }

    if let Ok(s) = std::str::from_utf8(raw) {
        return s.trim_matches(char::from(0)).trim().to_string();
    }

    let (cow, _, _) = WINDOWS_1252.decode(raw);
    cow.trim_matches(char::from(0)).trim().to_string()
}

fn extract_image_from_stream(buf: &[u8]) -> Option<(&'static str, &'static str, Vec<u8>, usize)> {
    // JPEG
    if let Some(start) = buf.windows(2).position(|w| w == [0xff, 0xd8]) {
        if let Some(end_rel) = buf[start + 2..].windows(2).position(|w| w == [0xff, 0xd9]) {
            let end = start + 2 + end_rel + 2;
            return Some(("image/jpeg", "jpg", buf[start..end].to_vec(), end));
        }
    }

    // PNG
    let sig = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if let Some(start) = buf.windows(sig.len()).position(|w| w == sig) {
        let iend = [b'I', b'E', b'N', b'D'];
        if let Some(pos) = buf[start + sig.len()..].windows(4).position(|w| w == iend) {
            let end = start + sig.len() + pos + 8;
            if end <= buf.len() {
                return Some(("image/png", "png", buf[start..end].to_vec(), end));
            }
        }
    }

    None
}

fn crc16_ccitt_ok(data_with_crc: &[u8]) -> bool {
    if data_with_crc.len() < 3 {
        return false;
    }
    let data_len = data_with_crc.len() - 2;
    let mut crc: u16 = 0xFFFF;
    for &byte in &data_with_crc[..data_len] {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    let stored = ((data_with_crc[data_len] as u16) << 8) | data_with_crc[data_len + 1] as u16;
    (crc ^ 0xFFFF) == stored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pad_from_au_none_on_non_dse() {
        let au = [0x00, 0x00, 0x00];
        assert!(extract_pad_from_au(&au).is_none());
    }

    #[test]
    fn test_extract_pad_from_au_basic() {
        let au = [0x80, 0x04, 0x11, 0x22, 0xaa, 0xbb];
        let out = extract_pad_from_au(&au).unwrap();
        assert_eq!(out.0, vec![0x11, 0x22]);
        assert_eq!(out.1, [0xaa, 0xbb]);
    }

    #[test]
    fn test_dl_reassembly_simple() {
        let mut dl = DlDecoder::default();
        // prefix0: first+last, field_len nibble=1 => 2 chars + CRC16
        let mut data = vec![0x61, 0x00, b'O', b'K'];
        let mut crc: u16 = 0xFFFF;
        for &byte in &data {
            crc ^= (byte as u16) << 8;
            for _ in 0..8 {
                if (crc & 0x8000) != 0 {
                    crc = (crc << 1) ^ 0x1021;
                } else {
                    crc <<= 1;
                }
            }
        }
        data.push(((crc ^ 0xFFFF) >> 8) as u8);
        data.push(((crc ^ 0xFFFF) & 0xff) as u8);
        let txt = dl.feed(true, &data).unwrap();
        assert_eq!(txt, "OK");
    }
}
