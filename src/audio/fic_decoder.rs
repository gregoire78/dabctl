/// FIC decoder: decodes FIBs to extract service/subchannel mapping.
/// Implements FIG 0/0 (ensemble), FIG 0/1 (subchannel config), FIG 0/2 (service→component),
/// FIG 1/0 (ensemble label), FIG 1/1 (service label).
use crate::audio::crc::crc16_ccitt;
use crate::audio::ebu_latin::ebu_latin_char_to_utf8_string;
use std::collections::HashMap;

/// Audio service type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioService {
    pub subchid: u8,
    pub dab_plus: bool,
}

/// Subchannel configuration from FIG 0/1
#[derive(Debug, Clone)]
pub struct SubchannelInfo {
    pub start: u16,
    pub size: u16,
    pub bitrate: u16,
    pub protection: String,
}

/// Service info assembled from FIG 0/2 + FIG 1/1
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub sid: u16,
    pub label: String,
    pub short_label: String,
    pub primary_subchid: Option<u8>,
    pub audio_components: HashMap<u8, AudioService>,
}

/// Ensemble info from FIG 0/0 + FIG 1/0
#[derive(Debug, Clone)]
pub struct EnsembleInfo {
    pub eid: u16,
    pub label: String,
    pub short_label: String,
}

/// FIC decoder state machine
pub struct FicDecoder {
    pub ensemble: Option<EnsembleInfo>,
    pub services: HashMap<u16, ServiceInfo>,
    pub subchannels: HashMap<u8, SubchannelInfo>,
    crc: crate::audio::crc::CrcCalculator,
}

// EEP-A and EEP-B size factors
const EEP_A_SIZE_FACTORS: [u16; 4] = [12, 8, 6, 4];
const EEP_B_SIZE_FACTORS: [u16; 4] = [27, 21, 18, 15];

// UEP table: [size, protection_level, bitrate] for indices 0..63
static UEP_TABLE: [(u16, u8, u16); 64] = [
    (16, 5, 32),
    (21, 4, 32),
    (24, 3, 32),
    (29, 2, 32),
    (35, 1, 32),
    (24, 5, 48),
    (29, 4, 48),
    (35, 3, 48),
    (42, 2, 48),
    (52, 1, 48),
    (29, 5, 56),
    (35, 4, 56),
    (42, 3, 56),
    (52, 2, 56),
    (32, 5, 64),
    (42, 4, 64),
    (48, 3, 64),
    (58, 2, 64),
    (70, 1, 64),
    (40, 5, 80),
    (52, 4, 80),
    (58, 3, 80),
    (70, 2, 80),
    (84, 1, 80),
    (48, 5, 96),
    (58, 4, 96),
    (70, 3, 96),
    (84, 2, 96),
    (104, 1, 96),
    (58, 5, 112),
    (70, 4, 112),
    (84, 3, 112),
    (104, 2, 112),
    (64, 5, 128),
    (84, 4, 128),
    (96, 3, 128),
    (116, 2, 128),
    (140, 1, 128),
    (80, 5, 160),
    (104, 4, 160),
    (116, 3, 160),
    (140, 2, 160),
    (168, 1, 160),
    (96, 5, 192),
    (116, 4, 192),
    (140, 3, 192),
    (168, 2, 192),
    (208, 1, 192),
    (116, 5, 224),
    (140, 4, 224),
    (168, 3, 224),
    (208, 2, 224),
    (232, 1, 224),
    (128, 5, 256),
    (168, 4, 256),
    (192, 3, 256),
    (232, 2, 256),
    (280, 1, 256),
    (160, 5, 320),
    (208, 4, 320),
    (280, 2, 320),
    (192, 5, 384),
    (280, 3, 384),
    (416, 1, 384),
];

impl Default for FicDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FicDecoder {
    pub fn new() -> Self {
        FicDecoder {
            ensemble: None,
            services: HashMap::new(),
            subchannels: HashMap::new(),
            crc: crc16_ccitt(),
        }
    }

    /// Process raw FIC data (multiple FIBs of 32 bytes each)
    pub fn process(&mut self, data: &[u8]) {
        if !data.len().is_multiple_of(32) {
            return;
        }
        for chunk in data.chunks_exact(32) {
            self.process_fib(chunk);
        }
    }

    fn process_fib(&mut self, data: &[u8]) {
        let crc_stored = (data[30] as u16) << 8 | data[31] as u16;
        let crc_calced = self.crc.calc(&data[..30]);
        if crc_stored != crc_calced {
            return;
        }

        let mut offset = 0usize;
        while offset < 30 && data[offset] != 0xFF {
            let fig_type = data[offset] >> 5;
            let fig_len = (data[offset] & 0x1F) as usize;
            offset += 1;

            if offset + fig_len > 30 {
                break;
            }

            match fig_type {
                0 => self.process_fig0(&data[offset..offset + fig_len]),
                1 => self.process_fig1(&data[offset..offset + fig_len]),
                _ => {}
            }

            offset += fig_len;
        }
    }

    fn process_fig0(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let cn = data[0] & 0x80 != 0;
        let oe = data[0] & 0x40 != 0;
        let pd = data[0] & 0x20 != 0;
        let extension = data[0] & 0x1F;

        // Ignore next config, other ensembles, data services
        if cn || oe || pd {
            return;
        }

        let field = &data[1..];
        match extension {
            0 => self.process_fig0_0(field),
            1 => self.process_fig0_1(field),
            2 => self.process_fig0_2(field),
            _ => {}
        }
    }

    /// FIG 0/0: Ensemble information
    fn process_fig0_0(&mut self, data: &[u8]) {
        if data.len() < 4 {
            return;
        }
        let eid = (data[0] as u16) << 8 | data[1] as u16;
        if let Some(ref mut ens) = self.ensemble {
            ens.eid = eid;
        } else {
            self.ensemble = Some(EnsembleInfo {
                eid,
                label: String::new(),
                short_label: String::new(),
            });
        }
    }

    /// FIG 0/1: Basic sub-channel organization
    fn process_fig0_1(&mut self, data: &[u8]) {
        let mut offset = 0;
        while offset + 2 <= data.len() {
            let subchid = data[offset] >> 2;
            let _start_address = ((data[offset] & 0x03) as u16) << 8 | data[offset + 1] as u16;
            offset += 2;

            if offset >= data.len() {
                break;
            }

            let long_form = data[offset] & 0x80 != 0;
            if long_form {
                if offset + 2 > data.len() {
                    break;
                }
                let option = (data[offset] & 0x70) >> 4;
                let pl = ((data[offset] & 0x0C) >> 2) as usize;
                let subch_size = ((data[offset] & 0x03) as u16) << 8 | data[offset + 1] as u16;
                offset += 2;

                let (protection, bitrate) = match option {
                    0 if pl < 4 => {
                        let br = subch_size / EEP_A_SIZE_FACTORS[pl] * 8;
                        (format!("EEP {}-A", pl + 1), br)
                    }
                    1 if pl < 4 => {
                        let br = subch_size / EEP_B_SIZE_FACTORS[pl] * 32;
                        (format!("EEP {}-B", pl + 1), br)
                    }
                    _ => continue,
                };

                self.subchannels.insert(
                    subchid,
                    SubchannelInfo {
                        start: _start_address,
                        size: subch_size,
                        bitrate,
                        protection,
                    },
                );
            } else {
                let table_index = (data[offset] & 0x3F) as usize;
                offset += 1;

                if table_index < UEP_TABLE.len() {
                    let (size, pl, bitrate) = UEP_TABLE[table_index];
                    self.subchannels.insert(
                        subchid,
                        SubchannelInfo {
                            start: _start_address,
                            size,
                            bitrate,
                            protection: format!("UEP {}", pl),
                        },
                    );
                }
            }
        }
    }

    /// FIG 0/2: Basic service and service component definition
    fn process_fig0_2(&mut self, data: &[u8]) {
        let mut offset = 0;
        while offset + 3 <= data.len() {
            let sid = (data[offset] as u16) << 8 | data[offset + 1] as u16;
            offset += 2;

            let num_comps = (data[offset] & 0x0F) as usize;
            offset += 1;

            for _ in 0..num_comps {
                if offset + 2 > data.len() {
                    return;
                }
                let tmid = data[offset] >> 6;

                if tmid == 0b00 {
                    // MSC stream audio
                    let ascty = data[offset] & 0x3F;
                    let subchid = data[offset + 1] >> 2;
                    let ps = data[offset + 1] & 0x02 != 0;
                    let ca = data[offset + 1] & 0x01 != 0;

                    if !ca && (ascty == 0 || ascty == 63) {
                        let dab_plus = ascty == 63;
                        let audio_service = AudioService { subchid, dab_plus };

                        let service = self.services.entry(sid).or_insert_with(|| ServiceInfo {
                            sid,
                            label: String::new(),
                            short_label: String::new(),
                            primary_subchid: None,
                            audio_components: HashMap::new(),
                        });

                        service.audio_components.insert(subchid, audio_service);
                        if ps {
                            service.primary_subchid = Some(subchid);
                        }
                    }
                }

                offset += 2;
            }
        }
    }

    fn process_fig1(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let charset = data[0] >> 4;
        let oe = (data[0] >> 3) & 0x01;
        let extension = data[0] & 0x07;

        // ETSI EN 300 401 §8.1.13:
        // - OE=1 carries labels for another ensemble and must be ignored.
        // - The current decoder supports charset 0 (EBU Latin) only.
        if oe == 1 || charset != 0 {
            return;
        }

        match extension {
            0 => self.process_fig1_0(data),
            1 => self.process_fig1_1(data),
            _ => {}
        }
    }

    /// FIG 1/0: Ensemble label
    fn process_fig1_0(&mut self, data: &[u8]) {
        // Header(1) + EId(2) + Label(16) + ShortLabelMask(2) = 21
        if data.len() < 21 {
            return;
        }
        let eid = (data[1] as u16) << 8 | data[2] as u16;
        let label = decode_ebu_label(&data[3..19]);
        let short_mask = (data[19] as u16) << 8 | data[20] as u16;
        let short_label = extract_short_label(&data[3..19], short_mask);

        if let Some(ref mut ens) = self.ensemble {
            if ens.eid == eid || ens.eid == 0 {
                ens.eid = eid;
                ens.label = label;
                ens.short_label = short_label;
            }
        } else {
            self.ensemble = Some(EnsembleInfo {
                eid,
                label,
                short_label,
            });
        }
    }

    /// FIG 1/1: Service label
    fn process_fig1_1(&mut self, data: &[u8]) {
        // Header(1) + SId(2) + Label(16) + ShortLabelMask(2) = 21
        if data.len() < 21 {
            return;
        }
        let sid = (data[1] as u16) << 8 | data[2] as u16;
        let label = decode_ebu_label(&data[3..19]);
        let short_mask = (data[19] as u16) << 8 | data[20] as u16;
        let short_label = extract_short_label(&data[3..19], short_mask);

        let service = self.services.entry(sid).or_insert_with(|| ServiceInfo {
            sid,
            label: String::new(),
            short_label: String::new(),
            primary_subchid: None,
            audio_components: HashMap::new(),
        });
        service.label = label;
        service.short_label = short_label;
    }

    /// Find the AudioService for a given SID.
    /// Returns the primary audio component if available.
    pub fn find_audio_service(&self, sid: u16) -> Option<AudioService> {
        let svc = self.services.get(&sid)?;
        if let Some(primary) = svc.primary_subchid {
            return svc.audio_components.get(&primary).copied();
        }
        // Fall back to any audio component
        svc.audio_components.values().next().copied()
    }

    /// Find a service by its label (case-insensitive, trimmed).
    pub fn find_service_by_label(&self, label: &str) -> Option<&ServiceInfo> {
        let needle = label.trim().to_lowercase();
        self.services
            .values()
            .find(|s| s.label.trim().to_lowercase() == needle)
    }
}

/// Decode 16-byte EBU Latin label to UTF-8
fn decode_ebu_label(data: &[u8]) -> String {
    data.iter()
        .take(16)
        .filter(|&&ch| ch != 0)
        .map(|&ch| ebu_latin_char_to_utf8_string(ch))
        .collect()
}

/// Extract short label using 16-bit mask
fn extract_short_label(data: &[u8], mask: u16) -> String {
    let mut result = String::new();
    for i in 0..16 {
        if mask & (1 << (15 - i)) != 0 && i < data.len() && data[i] != 0 {
            result.push_str(&ebu_latin_char_to_utf8_string(data[i]));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ebu_latin_to_utf8_ascii() {
        assert_eq!(
            ebu_latin_char_to_utf8_string(b'A')
                .chars()
                .next()
                .unwrap_or('\0'),
            'A'
        );
        assert_eq!(
            ebu_latin_char_to_utf8_string(b' ')
                .chars()
                .next()
                .unwrap_or('\0'),
            ' '
        );
    }

    #[test]
    fn test_ebu_latin_to_utf8_accented() {
        assert_eq!(
            ebu_latin_char_to_utf8_string(0x82)
                .chars()
                .next()
                .unwrap_or('\0'),
            'é'
        );
        assert_eq!(
            ebu_latin_char_to_utf8_string(0x80)
                .chars()
                .next()
                .unwrap_or('\0'),
            'á'
        );
    }

    #[test]
    fn test_decode_ebu_label() {
        let mut data = [0x20u8; 16];
        data[0] = b'F';
        data[1] = b'r';
        data[2] = b'a';
        data[3] = b'n';
        data[4] = b'c';
        data[5] = b'e';
        let label = decode_ebu_label(&data);
        assert_eq!(label, "France          ");
    }

    #[test]
    fn test_fig0_2_parsing() {
        let mut dec = FicDecoder::new();
        // Construct FIG 0/2 data:
        // SId=0xF201, 1 component, tmid=0b00 (MSC audio), ascty=63 (DAB+), subchid=5, ps=true
        let data = [
            0xF2,
            0x01,            // SId
            0x01,            // 1 component
            0x3F,            // tmid=0b00, ascty=63
            (5 << 2) | 0x02, // subchid=5, ps=1, ca=0
        ];
        dec.process_fig0_2(&data);

        assert!(dec.services.contains_key(&0xF201));
        let svc = &dec.services[&0xF201];
        assert_eq!(svc.primary_subchid, Some(5));
        let audio = svc.audio_components.get(&5).unwrap();
        assert!(audio.dab_plus);
        assert_eq!(audio.subchid, 5);
    }

    #[test]
    fn test_find_audio_service() {
        let mut dec = FicDecoder::new();
        dec.services.insert(
            0xF201,
            ServiceInfo {
                sid: 0xF201,
                label: "France Inter".to_string(),
                short_label: "Fr Inter".to_string(),
                primary_subchid: Some(5),
                audio_components: {
                    let mut map = HashMap::new();
                    map.insert(
                        5,
                        AudioService {
                            subchid: 5,
                            dab_plus: true,
                        },
                    );
                    map
                },
            },
        );

        let audio = dec.find_audio_service(0xF201).unwrap();
        assert_eq!(audio.subchid, 5);
        assert!(audio.dab_plus);
    }

    /// ETSI EN 300 401 §6.2.1: SubChId is a 6-bit field → valid range 0..=63.
    /// subchid is decoded as data[offset+1] >> 2 which yields 0..=63 from a u8,
    /// so 63 is the maximum legitimate value and must be accepted.
    #[test]
    fn test_fig0_2_subchid_63_maximum_valid_is_accepted() {
        let mut dec = FicDecoder::new();
        let data = [
            0xF2,
            0x01,             // SId = 0xF201
            0x01,             // 1 component
            0x3F,             // tmid=0b00, ascty=63 (DAB+)
            (63 << 2) | 0x02, // subchid=63, ps=1, ca=0
        ];
        dec.process_fig0_2(&data);
        let svc = dec.services.get(&0xF201).expect("service must be inserted");
        assert!(
            svc.audio_components.contains_key(&63),
            "subchid 63 must be accepted"
        );
        assert_eq!(svc.primary_subchid, Some(63));
    }

    /// ETSI EN 300 401 §8.1.13: OE=1 means "other ensemble".
    /// We must ignore FIG 1 labels not belonging to the selected ensemble.
    #[test]
    fn test_fig1_1_oe_other_ensemble_is_ignored() {
        let mut dec = FicDecoder::new();

        let mut data = [0u8; 21];
        data[0] = 0x08 | 0x01; // charset=0, OE=1, extension=1 (service label)
        data[1] = 0xF2;
        data[2] = 0x01;
        data[3] = b'T';
        data[4] = b'E';
        data[5] = b'S';
        data[6] = b'T';

        dec.process_fig1(&data);

        assert!(
            !dec.services.contains_key(&0xF201),
            "FIG 1/1 with OE=1 must be ignored"
        );
    }

    /// ETSI EN 300 401 §8.1.13: character set is signaled in FIG 1 header.
    /// This decoder supports charset 0 (EBU Latin) only; other charsets are ignored.
    #[test]
    fn test_fig1_1_non_ebu_charset_is_ignored() {
        let mut dec = FicDecoder::new();

        let mut data = [0u8; 21];
        data[0] = 0x10 | 0x01; // charset=1, OE=0, extension=1
        data[1] = 0xF2;
        data[2] = 0x01;
        data[3] = b'T';
        data[4] = b'E';
        data[5] = b'S';
        data[6] = b'T';

        dec.process_fig1(&data);

        assert!(
            !dec.services.contains_key(&0xF201),
            "FIG 1/1 with non-EBU charset must be ignored"
        );
    }

    /// ETSI EN 300 401 §8.1.14.1 + §6.2.2:
    /// FIG 1/1 label updates must not erase service-component mapping from FIG 0/2.
    #[test]
    fn test_fig1_label_update_preserves_fig0_component_mapping() {
        let mut dec = FicDecoder::new();

        // Seed mapping via FIG 0/2: SId 0xF201 -> primary subchid 5 (DAB+)
        let fig0_2 = [
            0xF2,
            0x01,            // SId
            0x01,            // one component
            0x3F,            // tmid=0b00, ascty=63 (DAB+)
            (5 << 2) | 0x02, // subchid=5, ps=1, ca=0
        ];
        dec.process_fig0_2(&fig0_2);

        // Apply label via FIG 1/1 for the same SId.
        let mut fig1_1 = [0u8; 21];
        fig1_1[0] = 0x01; // charset=0, OE=0, extension=1
        fig1_1[1] = 0xF2;
        fig1_1[2] = 0x01;
        fig1_1[3] = b'T';
        fig1_1[4] = b'E';
        fig1_1[5] = b'S';
        fig1_1[6] = b'T';
        dec.process_fig1(&fig1_1);

        let svc = dec.services.get(&0xF201).expect("service must exist");
        assert_eq!(svc.label, "TEST");
        assert_eq!(svc.primary_subchid, Some(5));
        let audio = svc
            .audio_components
            .get(&5)
            .expect("audio component mapping must be preserved");
        assert!(audio.dab_plus);
        assert_eq!(audio.subchid, 5);
    }

    /// ETSI EN 300 401 §8.1.14.1 + §6.2.2:
    /// If a service is created from FIG 1/1 first, later FIG 0/2 mapping must
    /// attach components without losing existing labels.
    #[test]
    fn test_fig0_mapping_update_preserves_existing_fig1_label() {
        let mut dec = FicDecoder::new();

        // Create service from FIG 1/1 first.
        let mut fig1_1 = [0u8; 21];
        fig1_1[0] = 0x01; // charset=0, OE=0, extension=1
        fig1_1[1] = 0xF2;
        fig1_1[2] = 0x01;
        fig1_1[3] = b'I';
        fig1_1[4] = b'N';
        fig1_1[5] = b'T';
        fig1_1[6] = b'E';
        fig1_1[7] = b'R';
        dec.process_fig1(&fig1_1);

        // Add component mapping later via FIG 0/2.
        let fig0_2 = [
            0xF2,
            0x01,            // SId
            0x01,            // one component
            0x3F,            // tmid=0b00, ascty=63 (DAB+)
            (8 << 2) | 0x02, // subchid=8, ps=1, ca=0
        ];
        dec.process_fig0_2(&fig0_2);

        let svc = dec.services.get(&0xF201).expect("service must exist");
        assert_eq!(svc.label, "INTER");
        assert_eq!(svc.primary_subchid, Some(8));
        let audio = svc
            .audio_components
            .get(&8)
            .expect("audio component mapping must exist after FIG 0/2");
        assert!(audio.dab_plus);
        assert_eq!(audio.subchid, 8);
    }
}
