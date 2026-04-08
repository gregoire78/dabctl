/// MOT (Multimedia Object Transfer) manager for DAB slideshow (SLS).
///
/// Reassembles MOT objects from Data Groups, parses headers (core + extension),
/// and emits completed image files (JPEG/PNG).
///
/// Reference: ETSI EN 301 234 (MOT), ETSI TS 101 756 (content types).
use crate::audio::crc::crc16_ccitt;
use std::collections::BTreeMap;

// --- Content types ---

pub const CONTENT_TYPE_IMAGE: u8 = 0x02;
pub const CONTENT_TYPE_MOT_TRANSPORT: u8 = 0x05;

pub const CONTENT_SUB_TYPE_JFIF: u16 = 0x001;
pub const CONTENT_SUB_TYPE_PNG: u16 = 0x003;
pub const CONTENT_SUB_TYPE_HEADER_UPDATE: u16 = 0x000;

const CRC_LEN: usize = 2;

/// A completed MOT file (slideshow image or other object).
#[derive(Debug, Clone)]
pub struct MotFile {
    pub data: Vec<u8>,
    pub body_size: usize,
    pub content_type: i16,
    pub content_sub_type: i16,
    pub content_name: String,
    pub content_name_charset: String,
    pub category_title: String,
    pub click_through_url: String,
    pub trigger_time_now: bool,
}

impl MotFile {
    fn new() -> Self {
        MotFile {
            data: Vec::new(),
            body_size: 0,
            content_type: -1,
            content_sub_type: -1,
            content_name: String::new(),
            content_name_charset: String::new(),
            category_title: String::new(),
            click_through_url: String::new(),
            // ETSI EN 301 234 §6.3.4.1: absence of TriggerTime means immediate display
            trigger_time_now: true,
        }
    }

    /// MIME type based on content_type/sub_type
    pub fn mime_type(&self) -> &'static str {
        if self.content_type == CONTENT_TYPE_IMAGE as i16 {
            match self.content_sub_type as u16 {
                CONTENT_SUB_TYPE_JFIF => "image/jpeg",
                CONTENT_SUB_TYPE_PNG => "image/png",
                _ => "application/octet-stream",
            }
        } else {
            "application/octet-stream"
        }
    }

    /// File extension based on content type
    pub fn extension(&self) -> &'static str {
        if self.content_type == CONTENT_TYPE_IMAGE as i16 {
            match self.content_sub_type as u16 {
                CONTENT_SUB_TYPE_JFIF => "jpg",
                CONTENT_SUB_TYPE_PNG => "png",
                _ => "bin",
            }
        } else {
            "bin"
        }
    }

    /// True if this is a displayable image
    pub fn is_image(&self) -> bool {
        self.content_type == CONTENT_TYPE_IMAGE as i16
            && matches!(
                self.content_sub_type as u16,
                CONTENT_SUB_TYPE_JFIF | CONTENT_SUB_TYPE_PNG
            )
    }
}

/// Collects numbered segments and reassembles them in order.
#[derive(Debug)]
struct MotEntity {
    segments: BTreeMap<usize, Vec<u8>>,
    last_seg_number: Option<usize>,
    total_size: usize,
}

impl MotEntity {
    fn new() -> Self {
        MotEntity {
            segments: BTreeMap::new(),
            last_seg_number: None,
            total_size: 0,
        }
    }

    fn reset(&mut self) {
        self.segments.clear();
        self.last_seg_number = None;
        self.total_size = 0;
    }

    fn add_segment(&mut self, seg_number: usize, last_seg: bool, data: &[u8]) {
        if last_seg {
            self.last_seg_number = Some(seg_number);
        }
        // Don't overwrite existing segments
        if self.segments.contains_key(&seg_number) {
            return;
        }
        self.total_size += data.len();
        self.segments.insert(seg_number, data.to_vec());
    }

    fn is_finished(&self) -> bool {
        let last = match self.last_seg_number {
            Some(n) => n,
            None => return false,
        };
        for i in 0..=last {
            if !self.segments.contains_key(&i) {
                return false;
            }
        }
        true
    }

    fn size(&self) -> usize {
        self.total_size
    }

    fn get_data(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.total_size);
        if let Some(last) = self.last_seg_number {
            for i in 0..=last {
                if let Some(seg) = self.segments.get(&i) {
                    result.extend_from_slice(seg);
                }
            }
        }
        result
    }
}

/// A single MOT object combining header + body entities.
struct MotObject {
    header: MotEntity,
    body: MotEntity,
    header_received: bool,
    shown: bool,
    result_file: MotFile,
}

impl MotObject {
    fn new() -> Self {
        MotObject {
            header: MotEntity::new(),
            body: MotEntity::new(),
            header_received: false,
            shown: false,
            result_file: MotFile::new(),
        }
    }

    fn add_segment(&mut self, is_header: bool, seg_number: usize, last_seg: bool, data: &[u8]) {
        if is_header {
            self.header.add_segment(seg_number, last_seg, data);
        } else {
            self.body.add_segment(seg_number, last_seg, data);
        }
    }

    fn parse_check_header(&mut self) -> bool {
        let data = self.header.get_data();
        if data.len() < 7 {
            return false;
        }

        let body_size = (data[0] as usize) << 20
            | (data[1] as usize) << 12
            | (data[2] as usize) << 4
            | (data[3] as usize) >> 4;
        let header_size =
            ((data[3] & 0x0F) as usize) << 9 | (data[4] as usize) << 1 | (data[5] as usize) >> 7;
        let content_type = ((data[5] & 0x7F) >> 1) as i16;
        let content_sub_type = (((data[5] & 0x01) as i16) << 8) | data[6] as i16;

        if header_size != self.header.size() {
            return false;
        }

        let header_update = content_type == CONTENT_TYPE_MOT_TRANSPORT as i16
            && content_sub_type == CONTENT_SUB_TYPE_HEADER_UPDATE as i16;

        // Abort if header_received XOR header_update
        if self.header_received != header_update {
            return false;
        }

        if !header_update {
            self.result_file.body_size = body_size;
            self.result_file.content_type = content_type;
            self.result_file.content_sub_type = content_sub_type;
        }

        let old_content_name = self.result_file.content_name.clone();

        // Parse header extension parameters
        let mut offset = 7;
        while offset < data.len() {
            let pli = data[offset] >> 6;
            let param_id = data[offset] & 0x3F;
            offset += 1;

            let data_len = match pli {
                0b00 => 0,
                0b01 => 1,
                0b10 => 4,
                0b11 => {
                    if offset >= data.len() {
                        return false;
                    }
                    let ext = data[offset] & 0x80 != 0;
                    let mut len = (data[offset] & 0x7F) as usize;
                    offset += 1;
                    if ext {
                        if offset >= data.len() {
                            return false;
                        }
                        len = (len << 8) + data[offset] as usize;
                        offset += 1;
                    }
                    len
                }
                _ => unreachable!(),
            };

            if offset + data_len > data.len() {
                return false;
            }

            match param_id {
                0x05 => {
                    // TriggerTime
                    if data_len >= 4 {
                        self.result_file.trigger_time_now = data[offset] & 0x80 == 0;
                    }
                }
                0x0C => {
                    // ContentName
                    if data_len > 0 {
                        let charset_byte = data[offset] >> 4;
                        let name_bytes = &data[offset + 1..offset + data_len];
                        self.result_file.content_name =
                            decode_content_name(name_bytes, charset_byte);
                        self.result_file.content_name_charset = charset_name(charset_byte);
                    }
                }
                0x26 => {
                    // CategoryTitle (already UTF-8)
                    self.result_file.category_title =
                        String::from_utf8_lossy(&data[offset..offset + data_len]).to_string();
                }
                0x27 => {
                    // ClickThroughURL (already UTF-8)
                    self.result_file.click_through_url =
                        String::from_utf8_lossy(&data[offset..offset + data_len]).to_string();
                }
                _ => {}
            }
            offset += data_len;
        }

        if !header_update {
            self.header_received = true;
        } else {
            // For header updates, content name must match
            if self.result_file.content_name != old_content_name {
                return false;
            }
        }

        true
    }

    fn is_to_be_shown(&mut self) -> bool {
        if self.shown {
            return false;
        }

        if self.header.is_finished() {
            let ok = self.parse_check_header();
            self.header.reset(); // allow header updates
            if !ok {
                return false;
            }
        }

        if !self.header_received {
            tracing::debug!("MOT object: header not yet received");
            return false;
        }
        if !self.body.is_finished() || self.body.size() != self.result_file.body_size {
            tracing::debug!(
                "MOT object: body incomplete ({}/{} bytes, finished={})",
                self.body.size(),
                self.result_file.body_size,
                self.body.is_finished()
            );
            return false;
        }
        if !self.result_file.trigger_time_now {
            tracing::debug!("MOT object: trigger_time not now, skipping");
            return false;
        }

        tracing::debug!(
            "MOT object complete: {} ({} bytes)",
            self.result_file.content_name,
            self.result_file.body_size
        );
        self.result_file.data = self.body.get_data();
        self.shown = true;
        true
    }

    fn current_body_size(&self) -> usize {
        self.body.size()
    }

    fn total_body_size(&self) -> usize {
        self.result_file.body_size
    }
}

/// MOT Manager: routes incoming Data Groups to MOT objects,
/// emits completed files.
pub struct MotManager {
    object: MotObject,
    current_transport_id: i32,
    crc: crate::audio::crc::CrcCalculator,
}

impl Default for MotManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MotManager {
    pub fn new() -> Self {
        MotManager {
            object: MotObject::new(),
            current_transport_id: -1,
            crc: crc16_ccitt(),
        }
    }

    pub fn reset(&mut self) {
        self.object = MotObject::new();
        self.current_transport_id = -1;
    }

    /// Process a completed MOT Data Group. Returns Some(MotFile) if object is complete and displayable.
    pub fn handle_data_group(&mut self, dg: &[u8]) -> (Option<MotFile>, f64) {
        let mut offset = 0;

        // Parse Data Group header
        let dg_type = match self.parse_dg_header(dg, &mut offset) {
            Some(t) => t,
            None => {
                tracing::debug!("MOT DG header parse failed (dg_len={})", dg.len());
                return (None, -1.0);
            }
        };

        // Parse session header
        let (last_seg, seg_number, transport_id) = match self.parse_session_header(dg, &mut offset)
        {
            Some(v) => v,
            None => {
                tracing::debug!("MOT session header parse failed");
                return (None, -1.0);
            }
        };

        // Parse segmentation header
        let seg_size = match self.parse_segmentation_header(dg, &mut offset) {
            Some(s) => s,
            None => {
                tracing::debug!("MOT segmentation header parse failed (dg_type={}, seg_number={}, tid={})", dg_type, seg_number, transport_id);
                return (None, -1.0);
            }
        };

        tracing::debug!("MOT segment: type={} seg={} last={} tid={} size={}", dg_type, seg_number, last_seg, transport_id, seg_size);

        // Reset object on transport ID change
        if self.current_transport_id != transport_id as i32 {
            tracing::debug!("MOT new transport_id={} (was {})", transport_id, self.current_transport_id);
            self.current_transport_id = transport_id as i32;
            self.object = MotObject::new();
        }

        let is_header = dg_type == 3;
        self.object.add_segment(
            is_header,
            seg_number,
            last_seg,
            &dg[offset..offset + seg_size],
        );

        let display = self.object.is_to_be_shown();

        let fraction = if self.object.total_body_size() > 0 {
            self.object.current_body_size() as f64 / self.object.total_body_size() as f64
        } else {
            -1.0
        };

        if display {
            let file = self.object.result_file.clone();
            (Some(file), fraction)
        } else {
            (None, fraction)
        }
    }

    fn parse_dg_header(&self, dg: &[u8], offset: &mut usize) -> Option<u8> {
        if dg.len() < *offset + 2 {
            return None;
        }

        let extension_flag = dg[*offset] & 0x80 != 0;
        let crc_flag = dg[*offset] & 0x40 != 0;
        let segment_flag = dg[*offset] & 0x20 != 0;
        let user_access_flag = dg[*offset] & 0x10 != 0;
        let dg_type = dg[*offset] & 0x0F;
        *offset += 2 + if extension_flag { 2 } else { 0 };

        if !crc_flag || !segment_flag || !user_access_flag {
            return None;
        }
        // Only accept MOT header (3) or body (4)
        if dg_type != 3 && dg_type != 4 {
            return None;
        }

        Some(dg_type)
    }

    fn parse_session_header(&self, dg: &[u8], offset: &mut usize) -> Option<(bool, usize, usize)> {
        if dg.len() < *offset + 3 {
            return None;
        }

        let last_seg = dg[*offset] & 0x80 != 0;
        let seg_number = ((dg[*offset] & 0x7F) as usize) << 8 | dg[*offset + 1] as usize;
        let transport_id_flag = dg[*offset + 2] & 0x10 != 0;
        let len_indicator = (dg[*offset + 2] & 0x0F) as usize;
        *offset += 3;

        if !transport_id_flag {
            return None;
        }
        if len_indicator < 2 {
            return None;
        }

        if dg.len() < *offset + len_indicator {
            return None;
        }

        let transport_id = (dg[*offset] as usize) << 8 | dg[*offset + 1] as usize;
        *offset += len_indicator;

        Some((last_seg, seg_number, transport_id))
    }

    fn parse_segmentation_header(&self, dg: &[u8], offset: &mut usize) -> Option<usize> {
        if dg.len() < *offset + 2 {
            return None;
        }

        let seg_size = ((dg[*offset] & 0x1F) as usize) << 8 | dg[*offset + 1] as usize;
        *offset += 2;

        // Compare announced vs actual segment size (minus CRC)
        if dg.len() < *offset + seg_size + CRC_LEN {
            return None;
        }

        // CRC check over entire DG (excluding last 2 bytes)
        let crc_offset = *offset + seg_size;
        let crc_stored = (dg[crc_offset] as u16) << 8 | dg[crc_offset + 1] as u16;
        let crc_calced = self.crc.calc(&dg[..crc_offset]);
        if crc_stored != crc_calced {
            return None;
        }

        Some(seg_size)
    }
}

/// Decode content name bytes to UTF-8 string
fn decode_content_name(data: &[u8], charset: u8) -> String {
    match charset {
        0 => {
            // EBU Latin - use simple ASCII fallback
            String::from_utf8_lossy(data).to_string()
        }
        6 | 15 => {
            // UTF-8
            String::from_utf8_lossy(data).to_string()
        }
        _ => String::from_utf8_lossy(data).to_string(),
    }
}

/// Charset name string
fn charset_name(charset: u8) -> String {
    match charset {
        0 => "EBU Latin".into(),
        6 | 15 => "UTF-8".into(),
        4 => "ISO 8859-1".into(),
        _ => format!("charset-{}", charset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mot_file_defaults() {
        let f = MotFile::new();
        assert_eq!(f.content_type, -1);
        assert_eq!(f.content_sub_type, -1);
        assert!(f.data.is_empty());
        assert!(!f.is_image());
    }

    #[test]
    fn test_mot_file_mime_type() {
        let mut f = MotFile::new();
        f.content_type = CONTENT_TYPE_IMAGE as i16;
        f.content_sub_type = CONTENT_SUB_TYPE_JFIF as i16;
        assert_eq!(f.mime_type(), "image/jpeg");
        assert_eq!(f.extension(), "jpg");
        assert!(f.is_image());

        f.content_sub_type = CONTENT_SUB_TYPE_PNG as i16;
        assert_eq!(f.mime_type(), "image/png");
        assert_eq!(f.extension(), "png");
        assert!(f.is_image());
    }

    #[test]
    fn test_mot_entity_basic() {
        let mut entity = MotEntity::new();
        entity.add_segment(0, false, b"Hello ");
        entity.add_segment(1, true, b"World");
        assert!(entity.is_finished());
        assert_eq!(entity.get_data(), b"Hello World");
        assert_eq!(entity.size(), 11);
    }

    #[test]
    fn test_mot_entity_no_overwrite() {
        let mut entity = MotEntity::new();
        entity.add_segment(0, true, b"First");
        entity.add_segment(0, true, b"Second");
        assert_eq!(entity.get_data(), b"First");
    }

    #[test]
    fn test_mot_entity_incomplete() {
        let mut entity = MotEntity::new();
        entity.add_segment(0, false, b"A");
        entity.add_segment(2, true, b"C");
        // Missing segment 1
        assert!(!entity.is_finished());
    }

    #[test]
    fn test_mot_entity_reset() {
        let mut entity = MotEntity::new();
        entity.add_segment(0, true, b"Data");
        assert!(entity.is_finished());
        entity.reset();
        assert!(!entity.is_finished());
        assert_eq!(entity.size(), 0);
    }

    #[test]
    fn test_decode_content_name() {
        assert_eq!(decode_content_name(b"logo.jpg", 0), "logo.jpg");
        assert_eq!(decode_content_name(b"slide.png", 15), "slide.png");
    }

    #[test]
    fn test_charset_name() {
        assert_eq!(charset_name(0), "EBU Latin");
        assert_eq!(charset_name(15), "UTF-8");
        assert_eq!(charset_name(4), "ISO 8859-1");
    }

    /// Build a minimal valid MOT Data Group for testing
    fn build_mot_dg(
        dg_type: u8,
        seg_number: usize,
        last_seg: bool,
        transport_id: u16,
        segment_data: &[u8],
    ) -> Vec<u8> {
        let crc_calc = crc16_ccitt();
        let mut dg = Vec::new();

        // Data Group header: CRC=1, segment=1, user_access=1
        let flags: u8 = 0x40 | 0x20 | 0x10 | (dg_type & 0x0F);
        dg.push(flags);
        dg.push(0x00); // continuity index + repetition indicator

        // Session header
        let seg_hi = if last_seg { 0x80 } else { 0x00 } | ((seg_number >> 8) & 0x7F) as u8;
        let seg_lo = (seg_number & 0xFF) as u8;
        dg.push(seg_hi);
        dg.push(seg_lo);
        // transport_id_flag=1, len_indicator=2
        dg.push(0x10 | 0x02);
        dg.push((transport_id >> 8) as u8);
        dg.push((transport_id & 0xFF) as u8);

        // Segmentation header
        let seg_size = segment_data.len();
        dg.push(((seg_size >> 8) & 0x1F) as u8);
        dg.push((seg_size & 0xFF) as u8);

        // Segment data
        dg.extend_from_slice(segment_data);

        // CRC over everything
        let crc = crc_calc.calc(&dg);
        dg.push((crc >> 8) as u8);
        dg.push((crc & 0xFF) as u8);

        dg
    }

    /// Build a minimal MOT header entity for testing
    fn build_mot_header(
        body_size: usize,
        content_type: u8,
        content_sub_type: u16,
        trigger_now: bool,
    ) -> Vec<u8> {
        let mut hdr = Vec::new();

        // HeaderCore: 7 bytes
        // body_size (28 bits), header_size (13 bits), content_type (6 bits), content_sub_type (9 bits)
        let header_ext_len = if trigger_now { 5 } else { 0 }; // TriggerTime: 1 byte PLI+paramID + 4 bytes data
        let header_size = 7 + header_ext_len;

        hdr.push((body_size >> 20) as u8);
        hdr.push((body_size >> 12) as u8);
        hdr.push((body_size >> 4) as u8);
        hdr.push(((body_size & 0x0F) << 4 | (header_size >> 9) & 0x0F) as u8);
        hdr.push((header_size >> 1) as u8);
        hdr.push(
            ((header_size & 0x01) << 7) as u8
                | ((content_type & 0x3F) << 1)
                | ((content_sub_type >> 8) & 0x01) as u8,
        );
        hdr.push((content_sub_type & 0xFF) as u8);

        if trigger_now {
            // PLI=0b10 (4 bytes), param_id=0x05
            hdr.push(0x80 | 0x05); // pli=10, param_id=0x05
            hdr.push(0x00); // Now: bit 7 = 0
            hdr.push(0x00);
            hdr.push(0x00);
            hdr.push(0x00);
        }

        hdr
    }

    #[test]
    fn test_mot_manager_complete_object() {
        let mut mgr = MotManager::new();

        // Build a small JPEG-like body
        let body_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let body_size = body_data.len();

        // Build MOT header
        let header_raw =
            build_mot_header(body_size, CONTENT_TYPE_IMAGE, CONTENT_SUB_TYPE_JFIF, true);

        // Send header as DG type 3
        let dg_header = build_mot_dg(3, 0, true, 1, &header_raw);
        let (result, _) = mgr.handle_data_group(&dg_header);
        assert!(result.is_none());

        // Send body as DG type 4
        let dg_body = build_mot_dg(4, 0, true, 1, &body_data);
        let (result, _) = mgr.handle_data_group(&dg_body);

        assert!(result.is_some());
        let file = result.unwrap();
        assert!(file.is_image());
        assert_eq!(file.data, body_data);
        assert_eq!(file.mime_type(), "image/jpeg");
        assert!(file.trigger_time_now);
    }

    #[test]
    fn test_mot_manager_multi_segment_body() {
        let mut mgr = MotManager::new();

        let body_part1 = vec![0xFF, 0xD8, 0xFF];
        let body_part2 = vec![0xE0, 0x00, 0x10];
        let body_size = body_part1.len() + body_part2.len();

        let header_raw =
            build_mot_header(body_size, CONTENT_TYPE_IMAGE, CONTENT_SUB_TYPE_PNG, true);

        let dg_header = build_mot_dg(3, 0, true, 42, &header_raw);
        let (result, _) = mgr.handle_data_group(&dg_header);
        assert!(result.is_none());

        let dg_body1 = build_mot_dg(4, 0, false, 42, &body_part1);
        let (result, _) = mgr.handle_data_group(&dg_body1);
        assert!(result.is_none());

        let dg_body2 = build_mot_dg(4, 1, true, 42, &body_part2);
        let (result, _) = mgr.handle_data_group(&dg_body2);

        assert!(result.is_some());
        let file = result.unwrap();
        assert_eq!(file.data, [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]);
        assert!(file.is_image());
        assert_eq!(file.mime_type(), "image/png");
    }

    #[test]
    fn test_mot_manager_transport_id_change_resets() {
        let mut mgr = MotManager::new();

        let body_data = vec![0x01, 0x02];
        let header_raw = build_mot_header(
            body_data.len(),
            CONTENT_TYPE_IMAGE,
            CONTENT_SUB_TYPE_JFIF,
            true,
        );

        // Send header for transport_id=1
        let dg1 = build_mot_dg(3, 0, true, 1, &header_raw);
        mgr.handle_data_group(&dg1);

        // Send header for transport_id=2 (new object)
        let dg2 = build_mot_dg(3, 0, true, 2, &header_raw);
        mgr.handle_data_group(&dg2);

        // Send body for transport_id=2
        let dg3 = build_mot_dg(4, 0, true, 2, &body_data);
        let (result, _) = mgr.handle_data_group(&dg3);

        assert!(result.is_some());
    }

    #[test]
    fn test_mot_manager_bad_crc_rejected() {
        let mut mgr = MotManager::new();

        let mut dg = build_mot_dg(3, 0, true, 1, &[0x01]);
        // Corrupt CRC
        let len = dg.len();
        dg[len - 1] ^= 0xFF;

        let (result, _) = mgr.handle_data_group(&dg);
        assert!(result.is_none());
    }
}
