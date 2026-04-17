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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fig0Header {
    cn: bool,
    oe: bool,
    pd: bool,
    extension: u8,
}

impl Fig0Header {
    fn parse(byte: u8) -> Self {
        Self {
            cn: (byte & 0x80) != 0,
            oe: (byte & 0x40) != 0,
            pd: (byte & 0x20) != 0,
            extension: byte & 0x1F,
        }
    }

    fn should_ignore(&self) -> bool {
        self.cn || self.oe || self.pd
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fig1Header {
    charset: u8,
    oe: bool,
    extension: u8,
}

impl Fig1Header {
    fn parse(byte: u8) -> Self {
        Self {
            charset: byte >> 4,
            oe: (byte & 0x08) != 0,
            extension: byte & 0x07,
        }
    }

    fn is_supported_label_set(&self) -> bool {
        !self.oe && self.charset == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FigEntryHeader {
    fig_type: u8,
    len: usize,
}

impl FigEntryHeader {
    fn parse(byte: u8) -> Self {
        Self {
            fig_type: byte >> 5,
            len: (byte & 0x1F) as usize,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSubchannelEntry {
    subchid: u8,
    start: u16,
    size: u16,
    bitrate: u16,
    protection: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedAudioComponent {
    subchid: u8,
    dab_plus: bool,
    primary: bool,
}

fn parse_subchannel_entry(data: &[u8]) -> Option<(ParsedSubchannelEntry, usize)> {
    if data.len() < 3 {
        return None;
    }

    // ETSI EN 300 401 §6.2.1: FIG 0/1 describes a subchannel with a 6-bit
    // SubChId, start address, and either short- or long-form protection data.
    let subchid = data[0] >> 2;
    let start = ((data[0] & 0x03) as u16) << 8 | data[1] as u16;
    let descriptor = data[2];

    if descriptor & 0x80 != 0 {
        if data.len() < 4 {
            return None;
        }

        let option = (descriptor & 0x70) >> 4;
        let pl = ((descriptor & 0x0C) >> 2) as usize;
        let size = ((descriptor & 0x03) as u16) << 8 | data[3] as u16;

        let (protection, bitrate) = match option {
            0 if pl < 4 => {
                let bitrate = size / EEP_A_SIZE_FACTORS[pl] * 8;
                (format!("EEP {}-A", pl + 1), bitrate)
            }
            1 if pl < 4 => {
                let bitrate = size / EEP_B_SIZE_FACTORS[pl] * 32;
                (format!("EEP {}-B", pl + 1), bitrate)
            }
            _ => return None,
        };

        return Some((
            ParsedSubchannelEntry {
                subchid,
                start,
                size,
                bitrate,
                protection,
            },
            4,
        ));
    }

    let table_index = (descriptor & 0x3F) as usize;
    if table_index >= UEP_TABLE.len() {
        return None;
    }

    let (size, pl, bitrate) = UEP_TABLE[table_index];
    Some((
        ParsedSubchannelEntry {
            subchid,
            start,
            size,
            bitrate,
            protection: format!("UEP {}", pl),
        },
        3,
    ))
}

fn parse_audio_component(data: &[u8]) -> Option<(ParsedAudioComponent, usize)> {
    if data.len() < 2 {
        return None;
    }

    // ETSI EN 300 401 §6.2.2: TMId 00 identifies an MSC stream audio
    // component, with ASCTy describing DAB or DAB+ audio.
    let tmid = data[0] >> 6;
    if tmid != 0b00 {
        return None;
    }

    let ascty = data[0] & 0x3F;
    let subchid = data[1] >> 2;
    let primary = data[1] & 0x02 != 0;
    let ca = data[1] & 0x01 != 0;

    if ca || (ascty != 0 && ascty != 63) {
        return None;
    }

    Some((
        ParsedAudioComponent {
            subchid,
            dab_plus: ascty == 63,
            primary,
        },
        2,
    ))
}

/// FIC decoder state machine
pub struct FicDecoder {
    pub ensemble: Option<EnsembleInfo>,
    pub services: HashMap<u16, ServiceInfo>,
    pub subchannels: HashMap<u8, SubchannelInfo>,
    crc: crate::audio::crc::CrcCalculator,
}

const FIB_SIZE: usize = 32;
const FIB_DATA_SIZE: usize = 30;

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

    fn service_entry_mut(&mut self, sid: u16) -> &mut ServiceInfo {
        self.services.entry(sid).or_insert_with(|| ServiceInfo {
            sid,
            label: String::new(),
            short_label: String::new(),
            primary_subchid: None,
            audio_components: HashMap::new(),
        })
    }

    fn update_audio_service_mapping(
        &mut self,
        sid: u16,
        audio_service: AudioService,
        primary: bool,
    ) {
        let service = self.service_entry_mut(sid);
        service
            .audio_components
            .insert(audio_service.subchid, audio_service);
        if primary {
            service.primary_subchid = Some(audio_service.subchid);
        }
    }

    fn upsert_subchannel(
        &mut self,
        subchid: u8,
        start: u16,
        size: u16,
        bitrate: u16,
        protection: String,
    ) {
        self.subchannels.insert(
            subchid,
            SubchannelInfo {
                start,
                size,
                bitrate,
                protection,
            },
        );
    }

    fn update_ensemble_label(&mut self, eid: u16, label: String, short_label: String) {
        if let Some(ref mut ens) = self.ensemble {
            if ens.eid == eid || ens.eid == 0 {
                ens.eid = eid;
                if !label.is_empty() {
                    ens.label = label;
                }
                if !short_label.is_empty() {
                    ens.short_label = short_label;
                }
            }
        } else {
            self.ensemble = Some(EnsembleInfo {
                eid,
                label,
                short_label,
            });
        }
    }

    /// Process raw FIC data (multiple FIBs of 32 bytes each)
    pub fn process(&mut self, data: &[u8]) {
        if !data.len().is_multiple_of(FIB_SIZE) {
            return;
        }
        for chunk in data.chunks_exact(FIB_SIZE) {
            self.process_fib(chunk);
        }
    }

    fn process_fib(&mut self, data: &[u8]) {
        let crc_stored = (data[FIB_DATA_SIZE] as u16) << 8 | data[FIB_DATA_SIZE + 1] as u16;
        let crc_calced = self.crc.calc(&data[..FIB_DATA_SIZE]);
        if crc_stored != crc_calced {
            return;
        }

        let mut offset = 0usize;
        while offset < FIB_DATA_SIZE && data[offset] != 0xFF {
            let header = FigEntryHeader::parse(data[offset]);
            offset += 1;

            if offset + header.len > FIB_DATA_SIZE {
                break;
            }

            self.dispatch_fig(header.fig_type, &data[offset..offset + header.len]);
            offset += header.len;
        }
    }

    fn dispatch_fig(&mut self, fig_type: u8, data: &[u8]) {
        match fig_type {
            0 => self.process_fig0(data),
            1 => self.process_fig1(data),
            _ => {}
        }
    }

    fn process_fig0(&mut self, data: &[u8]) {
        let Some((&raw_header, field)) = data.split_first() else {
            return;
        };

        let header = Fig0Header::parse(raw_header);

        // ETSI EN 300 401 §6.4 / §8.1.2: ignore next configuration,
        // other-ensemble data, and packet/data-service payload here.
        if header.should_ignore() {
            return;
        }

        match header.extension {
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
        self.update_ensemble_label(eid, String::new(), String::new());
    }

    /// FIG 0/1: Basic sub-channel organization
    fn process_fig0_1(&mut self, data: &[u8]) {
        let mut offset = 0;
        while let Some((entry, consumed)) = parse_subchannel_entry(&data[offset..]) {
            self.upsert_subchannel(
                entry.subchid,
                entry.start,
                entry.size,
                entry.bitrate,
                entry.protection,
            );
            offset += consumed;
            if offset >= data.len() {
                break;
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

                if let Some((component, consumed)) = parse_audio_component(&data[offset..]) {
                    self.update_audio_service_mapping(
                        sid,
                        AudioService {
                            subchid: component.subchid,
                            dab_plus: component.dab_plus,
                        },
                        component.primary,
                    );
                    offset += consumed;
                } else {
                    offset += 2;
                }
            }
        }
    }

    fn process_fig1(&mut self, data: &[u8]) {
        let Some((&raw_header, _)) = data.split_first() else {
            return;
        };

        let header = Fig1Header::parse(raw_header);

        // ETSI EN 300 401 §8.1.13: only current-ensemble EBU Latin labels
        // are supported in this decoder.
        if !header.is_supported_label_set() {
            return;
        }

        match header.extension {
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

        self.update_ensemble_label(eid, label, short_label);
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

        let service = self.service_entry_mut(sid);
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

        // Deterministic fallback: prefer the lowest sub-channel id so service
        // selection remains stable across HashMap iteration order.
        svc.audio_components
            .iter()
            .min_by_key(|(subchid, _)| *subchid)
            .map(|(_, audio)| *audio)
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

    #[test]
    fn test_fig0_header_ignores_non_current_configuration_data() {
        let header = Fig0Header::parse(0xE2);
        assert!(header.should_ignore());
        assert_eq!(header.extension, 0x02);
    }

    #[test]
    fn test_fig1_header_accepts_current_ebu_labels_only() {
        let supported = Fig1Header::parse(0x01);
        assert!(supported.is_supported_label_set());
        assert_eq!(supported.extension, 1);

        let other_ensemble = Fig1Header::parse(0x09);
        assert!(!other_ensemble.is_supported_label_set());

        let unsupported_charset = Fig1Header::parse(0x11);
        assert!(!unsupported_charset.is_supported_label_set());
    }

    #[test]
    fn test_find_service_by_label_is_trimmed_and_case_insensitive() {
        let mut dec = FicDecoder::new();
        dec.services.insert(
            0xF201,
            ServiceInfo {
                sid: 0xF201,
                label: "France Inter".to_string(),
                short_label: "INTER".to_string(),
                primary_subchid: None,
                audio_components: HashMap::new(),
            },
        );

        let service = dec
            .find_service_by_label("  france inter  ")
            .expect("service should be found by normalized label");
        assert_eq!(service.sid, 0xF201);
    }

    #[test]
    fn test_fig0_1_long_form_parses_eep_metadata() {
        let mut dec = FicDecoder::new();

        // SubChId=5, start=0x0123, long form EEP-A level 2, size=64.
        let data = [5 << 2 | 0x01, 0x23, 0x80 | (0 << 4) | (1 << 2), 64];
        dec.process_fig0_1(&data);

        let sub = dec.subchannels.get(&5).expect("subchannel should exist");
        assert_eq!(sub.start, 0x0123);
        assert_eq!(sub.size, 64);
        assert_eq!(sub.bitrate, 64);
        assert_eq!(sub.protection, "EEP 2-A");
    }

    #[test]
    fn test_fig0_1_short_form_uses_uep_table() {
        let mut dec = FicDecoder::new();

        // SubChId=7, start=0x0042, short-form UEP index 0.
        let data = [7 << 2, 0x42, 0x00];
        dec.process_fig0_1(&data);

        let sub = dec.subchannels.get(&7).expect("subchannel should exist");
        assert_eq!(sub.start, 0x0042);
        assert_eq!(sub.size, 16);
        assert_eq!(sub.bitrate, 32);
        assert_eq!(sub.protection, "UEP 5");
    }

    #[test]
    fn test_process_end_to_end_valid_fib_updates_ensemble() {
        let mut dec = FicDecoder::new();
        let mut fib = [0xFFu8; 32];

        // FIG 0/0: 1-byte FIG header plus 4-byte payload.
        fib[0] = 0x05;
        fib[1] = 0x00;
        fib[2] = 0x12;
        fib[3] = 0x34;
        fib[4] = 0x00;
        fib[5] = 0x00;

        let crc = crc16_ccitt().calc(&fib[..30]);
        fib[30] = (crc >> 8) as u8;
        fib[31] = crc as u8;

        dec.process(&fib);
        let ensemble = dec.ensemble.expect("ensemble should be decoded");
        assert_eq!(ensemble.eid, 0x1234);
    }

    #[test]
    fn test_find_audio_service_without_primary_uses_lowest_subchid() {
        let mut dec = FicDecoder::new();
        dec.services.insert(
            0xF201,
            ServiceInfo {
                sid: 0xF201,
                label: "France Inter".to_string(),
                short_label: "INTER".to_string(),
                primary_subchid: None,
                audio_components: {
                    let mut map = HashMap::new();
                    map.insert(
                        9,
                        AudioService {
                            subchid: 9,
                            dab_plus: true,
                        },
                    );
                    map.insert(
                        3,
                        AudioService {
                            subchid: 3,
                            dab_plus: true,
                        },
                    );
                    map
                },
            },
        );

        let selected = dec
            .find_audio_service(0xF201)
            .expect("a fallback service should be selected");
        assert_eq!(selected.subchid, 3);
    }

    #[test]
    fn test_parse_subchannel_entry_long_form_returns_expected_fields() {
        let data = [5 << 2 | 0x01, 0x23, 0x80 | (0 << 4) | (1 << 2), 64];
        let (entry, consumed) =
            parse_subchannel_entry(&data).expect("long-form subchannel should parse");

        assert_eq!(consumed, 4);
        assert_eq!(entry.subchid, 5);
        assert_eq!(entry.start, 0x0123);
        assert_eq!(entry.size, 64);
        assert_eq!(entry.bitrate, 64);
        assert_eq!(entry.protection, "EEP 2-A");
    }

    #[test]
    fn test_parse_audio_component_returns_primary_dab_plus_component() {
        let data = [0x3F, (5 << 2) | 0x02];
        let (component, consumed) =
            parse_audio_component(&data).expect("audio component should parse");

        assert_eq!(consumed, 2);
        assert_eq!(component.subchid, 5);
        assert!(component.dab_plus);
        assert!(component.primary);
    }

    #[test]
    fn test_process_fig0_0_preserves_existing_ensemble_label() {
        let mut dec = FicDecoder::new();
        dec.ensemble = Some(EnsembleInfo {
            eid: 0,
            label: "Ensemble Test".to_string(),
            short_label: "ENS".to_string(),
        });

        dec.process_fig0_0(&[0x12, 0x34, 0x00, 0x00]);

        let ensemble = dec.ensemble.expect("ensemble should stay present");
        assert_eq!(ensemble.eid, 0x1234);
        assert_eq!(ensemble.label, "Ensemble Test");
        assert_eq!(ensemble.short_label, "ENS");
    }
}
