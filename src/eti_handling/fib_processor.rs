// FIB processor - converted from fib-processor.cpp (eti-cmdline)

use crate::dab_constants::*;

/// Convert EBU Latin charset byte to UTF-8 char (EN 300 401, Table 1)
fn ebu_to_char(ch: u8) -> char {
    // EBU Latin differs from ISO 8859-1 in the 0x80..0xFF range
    static EBU_TABLE: [char; 128] = [
        // 0x80..0x8F
        '\u{00E1}', '\u{00E0}', '\u{00E9}', '\u{00E8}', '\u{00ED}', '\u{00EC}', '\u{00F3}', '\u{00F2}',
        '\u{00FA}', '\u{00F9}', '\u{00D1}', '\u{00C7}', '\u{015E}', '\u{00DF}', '\u{00A1}', '\u{0132}',
        // 0x90..0x9F
        '\u{00E2}', '\u{00E4}', '\u{00EA}', '\u{00EB}', '\u{00EE}', '\u{00EF}', '\u{00F4}', '\u{00F6}',
        '\u{00FB}', '\u{00FC}', '\u{00F1}', '\u{00E7}', '\u{015F}', '\u{011F}', '\u{0131}', '\u{0133}',
        // 0xA0..0xAF
        '\u{00AA}', '\u{03B1}', '\u{00A9}', '\u{2030}', '\u{011E}', '\u{011B}', '\u{0148}', '\u{0151}',
        '\u{03C0}', '\u{20AC}', '\u{00A3}', '\u{0024}', '\u{2190}', '\u{2191}', '\u{2192}', '\u{2193}',
        // 0xB0..0xBF
        '\u{00BA}', '\u{00B9}', '\u{00B2}', '\u{00B3}', '\u{00B1}', '\u{0130}', '\u{0144}', '\u{0171}',
        '\u{00B5}', '\u{00BF}', '\u{00F7}', '\u{00B0}', '\u{00BC}', '\u{00BD}', '\u{00BE}', '\u{00A7}',
        // 0xC0..0xCF
        '\u{00C1}', '\u{00C0}', '\u{00C9}', '\u{00C8}', '\u{00CD}', '\u{00CC}', '\u{00D3}', '\u{00D2}',
        '\u{00DA}', '\u{00D9}', '\u{0158}', '\u{010C}', '\u{0160}', '\u{017D}', '\u{00D0}', '\u{013F}',
        // 0xD0..0xDF
        '\u{00C2}', '\u{00C4}', '\u{00CA}', '\u{00CB}', '\u{00CE}', '\u{00CF}', '\u{00D4}', '\u{00D6}',
        '\u{00DB}', '\u{00DC}', '\u{0159}', '\u{010D}', '\u{0161}', '\u{017E}', '\u{0111}', '\u{0140}',
        // 0xE0..0xEF
        '\u{00C3}', '\u{00C5}', '\u{00C6}', '\u{0152}', '\u{0177}', '\u{00DD}', '\u{00D5}', '\u{00D8}',
        '\u{00DE}', '\u{014A}', '\u{0154}', '\u{0106}', '\u{015A}', '\u{0179}', '\u{0166}', '\u{00F0}',
        // 0xF0..0xFF
        '\u{00E3}', '\u{00E5}', '\u{00E6}', '\u{0153}', '\u{0175}', '\u{00FD}', '\u{00F5}', '\u{00F8}',
        '\u{00FE}', '\u{014B}', '\u{0155}', '\u{0107}', '\u{015B}', '\u{017A}', '\u{0167}', '\u{00FF}',
    ];

    if ch < 0x80 {
        ch as char
    } else {
        EBU_TABLE[(ch - 0x80) as usize]
    }
}

// UEP protection level table (ETSI EN 300 401 Page 50)
static PROT_LEVEL: [[i32; 3]; 64] = [
    [16,5,32],[21,4,32],[24,3,32],[29,2,32],[35,1,32],
    [24,5,48],[29,4,48],[35,3,48],[42,2,48],[52,1,48],
    [29,5,56],[35,4,56],[42,3,56],[52,2,56],
    [32,5,64],[42,4,64],[48,3,64],[58,2,64],[70,1,64],
    [40,5,80],[52,4,80],[58,3,80],[70,2,80],[84,1,80],
    [48,5,96],[58,4,96],[70,3,96],[84,2,96],[104,1,96],
    [58,5,112],[70,4,112],[84,3,112],[104,2,112],
    [64,5,128],[84,4,128],[96,3,128],[116,2,128],[140,1,128],
    [80,5,160],[104,4,160],[116,3,160],[140,2,160],[168,1,160],
    [96,5,192],[116,4,192],[140,3,192],[168,2,192],[208,1,192],
    [116,5,224],[140,4,224],[168,3,224],[208,2,224],[232,1,224],
    [128,5,256],[168,4,256],[192,3,256],[232,2,256],[280,1,256],
    [160,5,320],[208,4,320],[280,2,320],
    [192,5,384],[280,3,384],[416,1,384],
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

pub struct FibProcessor {
    sub_channels: [SubChannel; 64],
    list_of_services: Vec<ServiceId>,
    cif_count_hi: i16,
    cif_count_lo: i16,
    is_synced: bool,
    // Callbacks
    pub ensemble_name_cb: Option<Box<dyn Fn(&str, u32) + Send>>,
    pub program_name_cb: Option<Box<dyn Fn(&str, i32) + Send>>,
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
            let fig_type = get_bits_3(p, offset);
            match fig_type {
                0 => self.process_fig0(p, offset),
                1 => self.process_fig1(p, offset),
                7 => return,
                _ => {}
            }
            let length = get_bits_5(p, offset + 3) as i32;
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
            if self.list_of_services[i].in_use && self.list_of_services[i].service_id == service_id {
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

    #[test]
    fn test_ebu_to_char_ascii() {
        assert_eq!(ebu_to_char(b'A'), 'A');
        assert_eq!(ebu_to_char(b' '), ' ');
        assert_eq!(ebu_to_char(b'0'), '0');
    }

    #[test]
    fn test_ebu_to_char_metropolitain() {
        // 0x82 = 'é' in EBU Latin (was printed as \u{82} before fix)
        assert_eq!(ebu_to_char(0x82), 'é');
    }

    #[test]
    fn test_ebu_to_char_accented() {
        assert_eq!(ebu_to_char(0x80), 'á'); // a accent aigu
        assert_eq!(ebu_to_char(0x81), 'à'); // a accent grave
        assert_eq!(ebu_to_char(0x83), 'è'); // e accent grave
        assert_eq!(ebu_to_char(0x90), 'â'); // a circumflex
        assert_eq!(ebu_to_char(0x91), 'ä'); // a umlaut
        assert_eq!(ebu_to_char(0x9B), 'ç'); // c cedilla
    }

    #[test]
    fn test_ebu_to_char_euro() {
        assert_eq!(ebu_to_char(0xA9), '€');
    }
}
