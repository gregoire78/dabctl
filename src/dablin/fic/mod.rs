//! FIC (Fast Information Channel) / FIG (Fast Information Group) parser
//! Reference: ETSI EN 300 401

use encoding_rs::WINDOWS_1252;

use crate::dablin::utils::ebu_latin::ebu_latin_bytes_to_utf8_string;

/// CRC-16 (CCITT, polynomial 0x1021, initial value 0xFFFF) over FIB data.
/// The CRC covers the first 30 bytes; the last 2 bytes are the CRC itself.
pub fn fib_crc_ok(fib: &[u8; 32]) -> bool {
    let computed = crc16_ccitt(&fib[..30]);
    let stored = ((fib[30] as u16) << 8) | fib[31] as u16;
    computed == stored
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
    // ETSI EN 300 401 §5.2.1: CRC is the one's complement of the CRC-CCITT result
    crc ^ 0xFFFF
}

/// Decoded ensemble information (from FIG 0/0)
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct EnsembleInfo {
    pub eid: u16,
    pub label: Option<String>,
    pub lto: i8,
    pub ecc: u8,
}

/// Sub-channel organisation entry (from FIG 0/1)
#[derive(Debug, Clone)]
pub struct SubchannelOrg {
    pub subch_id: u8,
    pub start_addr: u16,
    pub size: u16,     // in CUs
    pub protection: ProtectionProfile,
}

/// Protection profile for a sub-channel
#[derive(Debug, Clone, PartialEq)]
pub enum ProtectionProfile {
    /// Equal Error Protection, profile A, levels 1-4
    EepA(u8),
    /// Equal Error Protection, profile B, levels 1-4
    EepB(u8),
    /// Unequal Error Protection table index
    Uep(u8),
}

/// Service component type
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentType {
    /// DAB audio (audio mode = 0)
    Audio,
    /// DAB+ audio (audio mode ≠ 0) – actually determined by user app FIG 0/13
    DataStream,
    /// Packet mode data
    Packet,
}

/// A single service component (from FIG 0/2)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ServiceComponent {
    pub subch_id: u8,
    pub is_primary: bool,
    pub ctype: ComponentType,
}

/// A decoded service entry (from FIG 0/2 + FIG 1/1)
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub sid: u32,
    pub label: Option<String>,
    pub components: Vec<ServiceComponent>,
}

/// Accumulator: collects FIG data across multiple FIC periods.
/// In DAB, the FIC repeats over many frames so we must merge.
#[derive(Debug, Default)]
pub struct FicDecoder {
    pub ensemble: EnsembleInfo,
    pub subchannels: Vec<SubchannelOrg>,
    pub services: Vec<ServiceInfo>,
    /// SCIDs that carry DAB+ (determined by FIG 0/13 user application)
    pub dabplus_subch_ids: Vec<u8>,
    /// X-PAD app type used for MOT slideshow, per sub-channel.
    pub mot_app_types: Vec<(u8, u8)>,
    /// X-PAD app type used for MOT slideshow, per service SID.
    pub mot_app_types_by_sid: Vec<(u32, u8)>,
    seen_fig0_13_log: bool,
}

impl FicDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process one ETI frame's worth of FIC data (3 FIBs × 32 bytes).
    pub fn process_fic(&mut self, fic: &[u8]) {
        let n_fibs = fic.len() / 32;
        for fib_i in 0..n_fibs {
            let fib: &[u8; 32] = match fic[fib_i * 32..fib_i * 32 + 32].try_into() {
                Ok(arr) => arr,
                Err(_) => continue,
            };
            if !fib_crc_ok(fib) {
                tracing::debug!("FIB {} CRC error, skipping", fib_i);
                continue;
            }
            self.parse_fib(&fib[..30]);
        }
    }

    fn parse_fib(&mut self, data: &[u8]) {
        let mut offset = 0;
        while offset < data.len() {
            let b = data[offset];
            let fig_type = (b >> 5) & 0x07;
            let fig_len = (b & 0x1f) as usize;
            // End marker
            if fig_type == 7 && fig_len == 31 {
                break;
            }
            if offset + 1 + fig_len > data.len() {
                break;
            }
            let fig_data = &data[offset + 1..offset + 1 + fig_len];
            match fig_type {
                0 => self.parse_fig0(fig_data),
                1 => self.parse_fig1(fig_data),
                _ => {} // Ignore other FIG types
            }
            offset += 1 + fig_len;
        }
    }

    fn parse_fig0(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let ext_byte = data[0];
        let cn = (ext_byte >> 7) & 1;
        let _oe = (ext_byte >> 6) & 1;
        let pd = (ext_byte >> 5) & 1;
        let ext = ext_byte & 0x1f;

        // Only process current configuration (C/N = 0)
        if cn != 0 {
            return;
        }

        let payload = &data[1..];
        match ext {
            0 => self.parse_fig0_0(payload),
            1 => self.parse_fig0_1(payload),
            2 => self.parse_fig0_2(payload, pd),
            13 => {
                if !self.seen_fig0_13_log {
                    self.seen_fig0_13_log = true;
                    tracing::debug!("FIG0/13 seen: pd={} payload_len={}", pd, payload.len());
                }
                self.parse_fig0_13(payload, pd)
            }
            _ => {}
        }
    }

    /// FIG 0/0: Ensemble information
    fn parse_fig0_0(&mut self, data: &[u8]) {
        if data.len() < 4 {
            return;
        }
        self.ensemble.eid = ((data[0] as u16) << 8) | data[1] as u16;
        // Remaining: change flag (2 bits) + Al (1 bit) + CIF count Hi (5) + CIF count Lo (8)
    }

    /// FIG 0/1: Sub-channel organisation
    fn parse_fig0_1(&mut self, data: &[u8]) {
        let mut pos = 0;
        while pos + 3 < data.len() {
            let subch_id = (data[pos] >> 2) & 0x3f;
            let start_addr = (((data[pos] & 0x03) as u16) << 8) | data[pos + 1] as u16;
            let long_form = (data[pos + 2] & 0x80) != 0;

            let (size, protection, advance) = if !long_form {
                // Short form: UEP
                let table_switch = (data[pos + 2] >> 6) & 0x01;
                let table_index = data[pos + 2] & 0x3f;
                if table_switch == 0 {
                    let uep_index = table_index;
                    // Short form size from table (simplified: use table_index as-is)
                    // TODO: proper UEP table lookup
                    let size = (table_index as u16) * 2; // placeholder
                    (size, ProtectionProfile::Uep(uep_index), 3)
                } else {
                    // Reserved
                    (0, ProtectionProfile::Uep(0), 3)
                }
            } else {
                // Long form: EEP
                if pos + 4 > data.len() {
                    break;
                }
                let option = (data[pos + 2] >> 4) & 0x07;
                let _pl = (data[pos + 2] >> 2) & 0x03; // protection level
                let sub_chsz_hi = (data[pos + 2] & 0x03) as u16;
                let sub_chsz_lo = data[pos + 3] as u16;
                let size = (sub_chsz_hi << 8) | sub_chsz_lo;

                let profile_byte = data[pos + 2];
                // bit 6: protection level option (0 = EEP-A, 1 = EEP-B)
                let protection = if option == 0 {
                    let level = ((profile_byte >> 2) & 0x03) + 1;
                    ProtectionProfile::EepA(level)
                } else {
                    let level = ((profile_byte >> 2) & 0x03) + 1;
                    ProtectionProfile::EepB(level)
                };
                (size, protection, 4)
            };

            // Update or insert
            if let Some(sc) = self.subchannels.iter_mut().find(|s| s.subch_id == subch_id) {
                sc.start_addr = start_addr;
                sc.size = size;
                sc.protection = protection;
            } else {
                self.subchannels.push(SubchannelOrg {
                    subch_id,
                    start_addr,
                    size,
                    protection,
                });
            }
            pos += advance;
        }
    }

    /// FIG 0/2: Service organisation
    fn parse_fig0_2(&mut self, data: &[u8], pd: u8) {
        let sid_len = if pd == 0 { 2 } else { 4 };
        let mut pos = 0;
        while pos + sid_len + 1 < data.len() {
            let sid: u32 = if sid_len == 2 {
                ((data[pos] as u32) << 8) | data[pos + 1] as u32
            } else {
                ((data[pos] as u32) << 24)
                    | ((data[pos + 1] as u32) << 16)
                    | ((data[pos + 2] as u32) << 8)
                    | data[pos + 3] as u32
            };
            pos += sid_len;

            let num_comp = (data[pos] & 0x0f) as usize;
            pos += 1;

            let mut components = Vec::new();
            for _ in 0..num_comp {
                if pos + 2 > data.len() {
                    break;
                }
                let tmid = (data[pos] >> 6) & 0x03;

                // Reference: dablin ProcessFIG0_2
                // byte 0: TMId (bits 7-6) + ascty (bits 5-0)
                // byte 1: SubChId (bits 7-2) + PS (bit 1) + CA (bit 0)
                let is_primary = (data[pos + 1] & 0x02) != 0;
                let ca = (data[pos + 1] & 0x01) != 0;

                let (subch_id, ctype, dab_plus) = if tmid == 0 {
                    // MSC stream audio
                    let ascty = data[pos] & 0x3f;
                    let subch_id = data[pos + 1] >> 2;
                    let dab_plus = ascty == 63; // 0=DAB, 63=DAB+
                    (subch_id, ComponentType::Audio, dab_plus)
                } else if tmid == 1 {
                    // MSC stream data
                    let subch_id = data[pos + 1] >> 2;
                    (subch_id, ComponentType::DataStream, false)
                } else {
                    // Packet mode or other
                    let subch_id = data[pos + 1] >> 2;
                    (subch_id, ComponentType::Packet, false)
                };

                if !ca {
                    // Register DAB+ sub-channel from FIG 0/2 (authoritative, per dablin)
                    if dab_plus && !self.dabplus_subch_ids.contains(&subch_id) {
                        self.dabplus_subch_ids.push(subch_id);
                    }
                }

                components.push(ServiceComponent {
                    subch_id,
                    is_primary,
                    ctype,
                });
                pos += 2;
            }

            // Update or insert service
            if let Some(svc) = self.services.iter_mut().find(|s| s.sid == sid) {
                if !components.is_empty() {
                    svc.components = components;
                }
            } else {
                self.services.push(ServiceInfo {
                    sid,
                    label: None,
                    components,
                });
            }
        }
    }

    /// FIG 0/13: User application information (identifies DAB+)
    fn parse_fig0_13(&mut self, data: &[u8], pd: u8) {
        // dablin handles programme services here (16-bit SID).
        // If P/D indicates long IDs, ignore for now.
        if pd != 0 {
            return;
        }

        let mut pos = 0;
        while pos + 3 <= data.len() {
            let sid = ((data[pos] as u32) << 8) | data[pos + 1] as u32;
            pos += 2;

            let _scids = data[pos] >> 4;
            let num_scids_uas = (data[pos] & 0x0f) as usize;
            pos += 1;

            for _ in 0..num_scids_uas {
                if pos + 2 > data.len() {
                    return;
                }

                let ua_type = ((data[pos] as u16) << 3) | ((data[pos + 1] >> 5) as u16);
                let ua_data_len = (data[pos + 1] & 0x1f) as usize;
                pos += 2;

                if pos + ua_data_len > data.len() {
                    return;
                }

                let ua = &data[pos..pos + ua_data_len];
                pos += ua_data_len;

                // UA type 0x002 = MOT slideshow in X-PAD.
                if ua_type == 0x002 {
                    // Fallback defaults from dablin's GetSLSAppType()
                    let mut ca_flag = false;
                    let mut xpad_app_type: u8 = 12;
                    let mut dg_flag = false;
                    let mut dscty: u8 = 60;

                    if ua.len() >= 2 {
                        ca_flag = (ua[0] & 0x80) != 0;
                        xpad_app_type = ua[0] & 0x1f;
                        dg_flag = (ua[1] & 0x80) != 0;
                        dscty = ua[1] & 0x3f;
                    }

                    if !ca_flag && !dg_flag && dscty == 60 {
                        tracing::trace!(
                            "FIG0/13 SLS UA SID={:#06x} xpad_app_type={} ua_len={}",
                            sid,
                            xpad_app_type,
                            ua.len()
                        );
                        if let Some(item) = self.mot_app_types_by_sid.iter_mut().find(|(s, _)| *s == sid) {
                            item.1 = xpad_app_type;
                        } else {
                            self.mot_app_types_by_sid.push((sid, xpad_app_type));
                        }

                        if let Some(svc) = self.services.iter().find(|s| s.sid == sid) {
                            for comp in &svc.components {
                                let subch_id = comp.subch_id;
                                if let Some(item) = self.mot_app_types.iter_mut().find(|(id, _)| *id == subch_id) {
                                    item.1 = xpad_app_type;
                                } else {
                                    self.mot_app_types.push((subch_id, xpad_app_type));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn parse_fig1(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let ext_byte = data[0];
        let _cn = (ext_byte >> 7) & 1;
        let _oe = (ext_byte >> 6) & 1;
        let charset = (ext_byte >> 4) & 0x0f;
        let ext = ext_byte & 0x0f;

        let payload = &data[1..];
        match ext {
            0 => self.parse_fig1_0(payload, charset),
            1 => self.parse_fig1_service_label(payload, charset, false),
            5 => self.parse_fig1_service_label(payload, charset, true),
            _ => {}
        }
    }

    /// FIG 1/0: Ensemble label
    fn parse_fig1_0(&mut self, data: &[u8], charset: u8) {
        if data.len() < 18 {
            return;
        }
        // 2 bytes EId + 16 bytes label + 2 bytes abbreviation mask
        let label_bytes = &data[2..18];
        self.ensemble.label = decode_dab_label(label_bytes, charset);
    }

    /// FIG 1/1 or 1/5: Programme service label (SID 16-bit or 32-bit)
    fn parse_fig1_service_label(&mut self, data: &[u8], charset: u8, long_sid: bool) {
        let sid_len = if long_sid { 4 } else { 2 };
        if data.len() < sid_len + 18 {
            return;
        }
        let sid: u32 = if sid_len == 2 {
            ((data[0] as u32) << 8) | data[1] as u32
        } else {
            ((data[0] as u32) << 24)
                | ((data[1] as u32) << 16)
                | ((data[2] as u32) << 8)
                | data[3] as u32
        };
        let label_bytes = &data[sid_len..sid_len + 16];
        let label = decode_dab_label(label_bytes, charset);
        if let Some(svc) = self.services.iter_mut().find(|s| s.sid == sid) {
            if label.is_some() {
                svc.label = label;
            }
        } else {
            self.services.push(ServiceInfo {
                sid,
                label,
                components: Vec::new(),
            });
        }
    }

    /// Find a service by SID (hex string like "0xF2F8" or decimal)
    pub fn find_by_sid(&self, sid_str: &str) -> Option<&ServiceInfo> {
        let sid = parse_sid(sid_str)?;
        self.services.iter().find(|s| s.sid == sid)
    }

    /// Find a service by label (case-insensitive prefix)
    pub fn find_by_label(&self, label: &str) -> Option<&ServiceInfo> {
        let lower = label.to_lowercase();
        self.services.iter().find(|s| {
            s.label
                .as_deref()
                .map(|l| l.to_lowercase().trim().starts_with(&lower))
                .unwrap_or(false)
        })
    }

    /// Returns true if a given sub-channel carries DAB+ audio
    pub fn is_dabplus(&self, subch_id: u8) -> bool {
        self.dabplus_subch_ids.contains(&subch_id)
    }

    /// Returns X-PAD MOT app type for slideshow on this sub-channel.
    pub fn mot_app_type(&self, subch_id: u8) -> Option<u8> {
        self.mot_app_types
            .iter()
            .find(|(id, _)| *id == subch_id)
            .map(|(_, t)| *t)
    }

    /// Returns X-PAD MOT app type for a service SID.
    pub fn mot_app_type_for_sid(&self, sid: u32) -> Option<u8> {
        self.mot_app_types_by_sid
            .iter()
            .find(|(s, _)| *s == sid)
            .map(|(_, t)| *t)
    }
}

/// Parse a SID from a string ("0xF2F8", "62200", etc.)
pub fn parse_sid(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

/// Decode a 16-byte DAB label to a UTF-8 String.
/// Charset 0 = EBU Latin (maps to Windows-1252 for the printable range).
fn decode_dab_label(bytes: &[u8], charset: u8) -> Option<String> {
    // Trim trailing spaces / padding
    let trimmed: Vec<u8> = bytes
        .iter()
        .cloned()
        .rev()
        .skip_while(|&b| b == 0x20 || b == 0x00)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if trimmed.is_empty() {
        return None;
    }

    // Charset 0 in DAB labels is EBU Latin.
    if charset == 0 {
        let decoded = ebu_latin_bytes_to_utf8_string(&trimmed);
        let clean = decoded.trim();
        if clean.is_empty() {
            return None;
        }
        return Some(clean.to_string());
    }

    // Try UTF-8 first; fall back to Windows-1252 for non-EBU charsets.
    if let Ok(s) = std::str::from_utf8(&trimmed) {
        Some(s.to_string())
    } else {
        let (cow, _, _) = WINDOWS_1252.decode(&trimmed);
        Some(cow.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal FIB with correct CRC.
    fn make_fib(fig_data: &[u8]) -> [u8; 32] {
        let mut fib = [0u8; 32];
        // FIG data must fit in 30 bytes (bytes 0-29), CRC in bytes 30-31
        let len = fig_data.len().min(30);
        fib[..len].copy_from_slice(&fig_data[..len]);
        // End marker at first free slot
        if len < 30 {
            fib[len] = 0xff; // FIG type 7 / length 31 = end marker
        }
        let crc = crc16_ccitt(&fib[..30]);
        fib[30] = (crc >> 8) as u8;
        fib[31] = (crc & 0xff) as u8;
        fib
    }

    #[test]
    fn test_fib_crc_valid() {
        let fib = make_fib(&[0x05, 0x00, 0xf0, 0x43, 0x12, 0xa0]);
        assert!(fib_crc_ok(&fib));
    }

    #[test]
    fn test_fib_crc_invalid() {
        let mut fib = make_fib(&[0x05, 0x00, 0xf0, 0x43]);
        fib[30] ^= 0x01; // corrupt CRC
        assert!(!fib_crc_ok(&fib));
    }

    #[test]
    fn test_parse_fig0_0_ensemble_id() {
        // FIG 0/0: type=0 len=5, ext=0 (C/N=0,OE=0,PD=0), EId=0xF043
        let fig_bytes = [
            0x05u8, // FIG type=0, len=5
            0x00,   // ext byte: C/N=0, OE=0, PD=0, Ext=0 → FIG 0/0
            0xf0, 0x43, // EId = 0xF043
            0x12, 0xa0, // change flag + CIF count
        ];
        let fib = make_fib(&fig_bytes);
        let mut decoder = FicDecoder::new();
        let fic: Vec<u8> = fib.iter().copied().collect();
        decoder.process_fic(&fic);
        assert_eq!(decoder.ensemble.eid, 0xF043);
    }

    #[test]
    fn test_parse_fig0_2_service() {
        // FIG 0/2 (PD=0, 16-bit SID): type=0 len=6, ext=2
        // Service: SID=0xF2F8, 1 component: SCID=1 (sub-ch 1), primary
        let fig_bytes = [
            0x06_u8, // type=0, len=6 (ext byte + SID 2b + num_comp 1b + comp 2b)
            0x02,    // ext=2
            0xf2, 0xf8, // SID = 0xF2F8
            0x01,    // num_comp = 1
            0x00, 0x06, // TMID=0 (stream audio, ascty=0=DAB), subch_id=(0x06>>2)=1, PS=(0x06&0x02)≠0=primary
        ];
        let fib = make_fib(&fig_bytes);
        let mut decoder = FicDecoder::new();
        decoder.process_fic(&fib);
        assert_eq!(decoder.services.len(), 1);
        assert_eq!(decoder.services[0].sid, 0xF2F8);
        assert_eq!(decoder.services[0].components.len(), 1);
        assert_eq!(decoder.services[0].components[0].subch_id, 1);
        assert!(decoder.services[0].components[0].is_primary);
    }

    #[test]
    fn test_find_service_by_sid_hex() {
        let mut decoder = FicDecoder::new();
        decoder.services.push(ServiceInfo {
            sid: 0xF2F8,
            label: Some("Test FM".to_string()),
            components: vec![],
        });
        let svc = decoder.find_by_sid("0xF2F8");
        assert!(svc.is_some());
        assert_eq!(svc.unwrap().sid, 0xF2F8);
    }

    #[test]
    fn test_find_service_by_label() {
        let mut decoder = FicDecoder::new();
        decoder.services.push(ServiceInfo {
            sid: 0xF2F8,
            label: Some("France Inter".to_string()),
            components: vec![],
        });
        let svc = decoder.find_by_label("france");
        assert!(svc.is_some());
        assert_eq!(svc.unwrap().sid, 0xF2F8);
    }

    #[test]
    fn test_parse_sid_hex() {
        assert_eq!(parse_sid("0xF2F8"), Some(0xF2F8));
        assert_eq!(parse_sid("0XF2F8"), Some(0xF2F8));
        assert_eq!(parse_sid("62200"), Some(62200));
        assert_eq!(parse_sid("invalid"), None);
    }

    #[test]
    fn test_fig_end_marker_stops_parsing() {
        // End marker (0xff = type 7, len 31) should terminate parsing
        let fig_bytes = [
            0x05_u8, 0x00, 0xf0, 0x43, 0x12, 0xa0, // FIG 0/0
            0xff,    // end marker
            0x05, 0x02, 0xf2, 0xf8, 0x01, // garbage after end marker (should be ignored)
        ];
        let fib = make_fib(&fig_bytes);
        let mut decoder = FicDecoder::new();
        decoder.process_fic(&fib);
        // Only ensemble should be parsed, no services
        assert_eq!(decoder.ensemble.eid, 0xF043);
        assert_eq!(decoder.services.len(), 0);
    }

    #[test]
    fn test_decode_dab_label_trim() {
        // "NRJ     " (padded with spaces) → "NRJ"
        let mut bytes = [0x20u8; 16];
        bytes[0] = b'N';
        bytes[1] = b'R';
        bytes[2] = b'J';
        let label = decode_dab_label(&bytes, 0).unwrap();
        assert_eq!(label, "NRJ");
    }

    #[test]
    fn test_parse_fig1_5_long_sid_service_label() {
        // FIG 1/5 with 32-bit SID + 16-byte label + 2-byte short label mask
        // Header: type=1, len=23
        let mut fig = vec![0x20 | 23, 0x05]; // FIG1 len=23, ext=5 charset=0
        fig.extend_from_slice(&[0x00, 0x01, 0x23, 0x45]); // SID = 0x00012345

        let mut label = [0x20u8; 16];
        label[..5].copy_from_slice(b"RADIO");
        fig.extend_from_slice(&label);
        fig.extend_from_slice(&[0x00, 0x00]); // abbreviation mask

        let fib = make_fib(&fig);
        let mut decoder = FicDecoder::new();
        decoder.process_fic(&fib);

        let svc = decoder.services.iter().find(|s| s.sid == 0x0001_2345);
        assert!(svc.is_some());
        assert_eq!(svc.and_then(|s| s.label.clone()).as_deref(), Some("RADIO"));
    }
}
