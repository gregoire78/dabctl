use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy)]
struct ProtLevelEntry {
    cu_size: usize,
    prot_level: i16,
    bit_rate: u16,
}

#[derive(Debug, Clone, Copy)]
struct SubChannelInfo {
    start_addr: usize,
    cu_size: usize,
    bit_rate: u16,
    short_form: bool,
    prot_level: i16,
}

#[derive(Debug, Clone, Default)]
struct ServiceRecord {
    sid: u32,
    label: String,
    subch_id: Option<u8>,
    has_primary_audio: bool,
}

#[derive(Debug, Clone)]
pub struct AudioServiceInfo {
    pub sid: u32,
    pub label: String,
    pub subch_id: u8,
    pub start_addr: usize,
    pub cu_size: usize,
    pub bit_rate: u16,
    pub short_form: bool,
    pub prot_level: i16,
}

// Literal but compact FIG service database handling used for CLI service selection.
#[derive(Default)]
pub struct FibDecoder {
    ensemble_id: Option<u16>,
    ensemble_label: Option<String>,
    cif_count: Option<u16>,
    subchannels: HashMap<u8, SubChannelInfo>,
    services: BTreeMap<u32, ServiceRecord>,
}

impl FibDecoder {
    pub fn process_fib(&mut self, fib: &[u8; 32]) {
        let mut processed = 0usize;
        while processed < 30 {
            let fig_header = fib[processed];
            if fig_header == 0xFF {
                break;
            }

            let fig_length = (fig_header & 0x1F) as usize;
            let fig_end = (processed + 1 + fig_length).min(30);
            if fig_end <= processed + 1 {
                break;
            }

            let fig = &fib[processed..fig_end];
            match fig_header >> 5 {
                0 => self.process_fig0(fig),
                1 => self.process_fig1(fig),
                _ => {}
            }

            processed = fig_end;
        }
    }

    pub fn service_count(&self) -> usize {
        self.services.len()
    }

    pub fn ensemble_id(&self) -> Option<u16> {
        self.ensemble_id
    }

    pub fn ensemble_label(&self) -> Option<&str> {
        self.ensemble_label.as_deref()
    }

    pub fn cif_count(&self) -> Option<u16> {
        self.cif_count
    }

    pub fn service_label_for_sid(&self, sid: u32) -> Option<&str> {
        self.services
            .get(&sid)
            .filter(|s| !s.label.is_empty())
            .map(|s| s.label.as_str())
    }

    pub fn selected_audio_service(
        &self,
        sid: u32,
        label: Option<&str>,
    ) -> Option<AudioServiceInfo> {
        if let Some(label) = label {
            if let Some(service) = self.services.values().find(|service| {
                !service.label.is_empty() && service.label.eq_ignore_ascii_case(label)
            }) {
                return self.service_info_for_sid(service.sid);
            }
        }

        self.service_info_for_sid(sid)
    }

    fn service_info_for_sid(&self, sid: u32) -> Option<AudioServiceInfo> {
        let service = self.services.get(&sid)?;
        let subch_id = service.subch_id?;
        let subchannel = self.subchannels.get(&subch_id)?;
        Some(AudioServiceInfo {
            sid,
            label: service.label.clone(),
            subch_id,
            start_addr: subchannel.start_addr,
            cu_size: subchannel.cu_size,
            bit_rate: subchannel.bit_rate,
            short_form: subchannel.short_form,
            prot_level: subchannel.prot_level,
        })
    }

    fn process_fig0(&mut self, fig: &[u8]) {
        if fig.len() < 2 {
            return;
        }

        let extension = fig[1] & 0x1F;
        let pd_flag = (fig[1] >> 5) & 0x01;
        match extension {
            0 => self.process_fig0_ext0(fig),
            1 => self.process_fig0_ext1(fig),
            2 => self.process_fig0_ext2(fig, pd_flag),
            _ => {}
        }
    }

    fn process_fig0_ext0(&mut self, fig: &[u8]) {
        if fig.len() < 7 {
            return;
        }

        let eid = get_bits(fig, 16, 16) as u16;
        let cif_count_hi = get_bits(fig, 35, 5) as u16;
        let cif_count_lo = get_bits(fig, 40, 8) as u16;

        self.ensemble_id = Some(eid);
        self.cif_count = Some(cif_count_hi.saturating_mul(250) + cif_count_lo);
    }

    fn process_fig1(&mut self, fig: &[u8]) {
        if fig.len() < 4 {
            return;
        }

        let extension = fig[1] & 0x07;
        match extension {
            0 => {
                let label = parse_label(&fig[4..]);
                if !label.is_empty() {
                    self.ensemble_label = Some(label);
                }
            }
            1 => {
                let sid = get_bits(fig, 16, 16);
                let label = parse_label(&fig[4..]);
                if !label.is_empty() {
                    let entry = self.services.entry(sid).or_default();
                    entry.sid = sid;
                    entry.label = label;
                }
            }
            _ => {}
        }
    }

    fn process_fig0_ext1(&mut self, fig: &[u8]) {
        let end_bits = fig.len() * 8;
        let mut bit_offset = 16usize;

        while bit_offset + 24 <= end_bits {
            let subch_id = get_bits(fig, bit_offset, 6) as u8;
            let start_addr = get_bits(fig, bit_offset + 6, 10) as usize;
            let short_form = get_bits(fig, bit_offset + 16, 1) == 0;

            if short_form {
                let table_index = get_bits(fig, bit_offset + 18, 6) as usize;
                if let Some(entry) = PROT_LEVEL_TABLE.get(table_index).copied() {
                    self.subchannels.insert(
                        subch_id,
                        SubChannelInfo {
                            start_addr,
                            cu_size: entry.cu_size,
                            bit_rate: entry.bit_rate,
                            short_form: true,
                            prot_level: entry.prot_level,
                        },
                    );
                }
                bit_offset += 24;
            } else {
                if bit_offset + 32 > end_bits {
                    break;
                }
                let option = get_bits(fig, bit_offset + 17, 3) as u8;
                let protection_level = get_bits(fig, bit_offset + 20, 2) as i16;
                let subchannel_size = get_bits(fig, bit_offset + 22, 10) as usize;
                let bit_rate = derive_eep_bit_rate(option, protection_level, subchannel_size);
                self.subchannels.insert(
                    subch_id,
                    SubChannelInfo {
                        start_addr,
                        cu_size: subchannel_size,
                        bit_rate,
                        short_form: false,
                        prot_level: ((option as i16) << 2) | protection_level,
                    },
                );
                bit_offset += 32;
            }
        }
    }

    fn process_fig0_ext2(&mut self, fig: &[u8], pd_flag: u8) {
        let end_bits = fig.len() * 8;
        let mut bit_offset = 16usize;

        while bit_offset + if pd_flag != 0 { 40 } else { 24 } <= end_bits {
            let sid_bits = if pd_flag != 0 { 32 } else { 16 };
            let sid = get_bits(fig, bit_offset, sid_bits);
            bit_offset += sid_bits;
            let num_service_components = get_bits(fig, bit_offset + 4, 4) as usize;
            bit_offset += 8;

            for _ in 0..num_service_components {
                if bit_offset + 16 > end_bits {
                    break;
                }

                let tmid = get_bits(fig, bit_offset, 2) as u8;
                if tmid == 0 {
                    let subch_id = get_bits(fig, bit_offset + 8, 6) as u8;
                    let ps_flag = get_bits(fig, bit_offset + 14, 1) != 0;
                    let entry = self.services.entry(sid).or_default();
                    entry.sid = sid;
                    if ps_flag || entry.subch_id.is_none() || !entry.has_primary_audio {
                        entry.subch_id = Some(subch_id);
                        entry.has_primary_audio = ps_flag;
                    }
                }
                bit_offset += 16;
            }
        }
    }
}

fn get_bits(data: &[u8], bit_offset: usize, bit_length: usize) -> u32 {
    let mut value = 0u32;
    for idx in 0..bit_length {
        let absolute = bit_offset + idx;
        let byte = data.get(absolute / 8).copied().unwrap_or(0);
        let bit = (byte >> (7 - (absolute % 8))) & 1;
        value = (value << 1) | u32::from(bit);
    }
    value
}

fn parse_label(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(16)
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                ' '
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn derive_eep_bit_rate(option: u8, protection_level: i16, subchannel_size: usize) -> u16 {
    match (option, protection_level) {
        (0, 0) => ((subchannel_size / 12) * 8) as u16,
        (0, 1) => subchannel_size as u16,
        (0, 2) => ((subchannel_size / 6) * 8) as u16,
        (0, 3) => ((subchannel_size / 4) * 8) as u16,
        (1, 0) => ((subchannel_size / 27) * 32) as u16,
        (1, 1) => ((subchannel_size / 21) * 32) as u16,
        (1, 2) => ((subchannel_size / 18) * 32) as u16,
        (1, 3) => ((subchannel_size / 15) * 32) as u16,
        _ => 64,
    }
}

const PROT_LEVEL_TABLE: [ProtLevelEntry; 64] = [
    ProtLevelEntry {
        cu_size: 16,
        prot_level: 5,
        bit_rate: 32,
    },
    ProtLevelEntry {
        cu_size: 21,
        prot_level: 4,
        bit_rate: 32,
    },
    ProtLevelEntry {
        cu_size: 24,
        prot_level: 3,
        bit_rate: 32,
    },
    ProtLevelEntry {
        cu_size: 29,
        prot_level: 2,
        bit_rate: 32,
    },
    ProtLevelEntry {
        cu_size: 35,
        prot_level: 1,
        bit_rate: 32,
    },
    ProtLevelEntry {
        cu_size: 24,
        prot_level: 5,
        bit_rate: 48,
    },
    ProtLevelEntry {
        cu_size: 29,
        prot_level: 4,
        bit_rate: 48,
    },
    ProtLevelEntry {
        cu_size: 35,
        prot_level: 3,
        bit_rate: 48,
    },
    ProtLevelEntry {
        cu_size: 42,
        prot_level: 2,
        bit_rate: 48,
    },
    ProtLevelEntry {
        cu_size: 52,
        prot_level: 1,
        bit_rate: 48,
    },
    ProtLevelEntry {
        cu_size: 29,
        prot_level: 5,
        bit_rate: 56,
    },
    ProtLevelEntry {
        cu_size: 35,
        prot_level: 4,
        bit_rate: 56,
    },
    ProtLevelEntry {
        cu_size: 42,
        prot_level: 3,
        bit_rate: 56,
    },
    ProtLevelEntry {
        cu_size: 52,
        prot_level: 2,
        bit_rate: 56,
    },
    ProtLevelEntry {
        cu_size: 32,
        prot_level: 5,
        bit_rate: 64,
    },
    ProtLevelEntry {
        cu_size: 42,
        prot_level: 4,
        bit_rate: 64,
    },
    ProtLevelEntry {
        cu_size: 48,
        prot_level: 3,
        bit_rate: 64,
    },
    ProtLevelEntry {
        cu_size: 58,
        prot_level: 2,
        bit_rate: 64,
    },
    ProtLevelEntry {
        cu_size: 70,
        prot_level: 1,
        bit_rate: 64,
    },
    ProtLevelEntry {
        cu_size: 40,
        prot_level: 5,
        bit_rate: 80,
    },
    ProtLevelEntry {
        cu_size: 52,
        prot_level: 4,
        bit_rate: 80,
    },
    ProtLevelEntry {
        cu_size: 58,
        prot_level: 3,
        bit_rate: 80,
    },
    ProtLevelEntry {
        cu_size: 70,
        prot_level: 2,
        bit_rate: 80,
    },
    ProtLevelEntry {
        cu_size: 84,
        prot_level: 1,
        bit_rate: 80,
    },
    ProtLevelEntry {
        cu_size: 48,
        prot_level: 5,
        bit_rate: 96,
    },
    ProtLevelEntry {
        cu_size: 58,
        prot_level: 4,
        bit_rate: 96,
    },
    ProtLevelEntry {
        cu_size: 70,
        prot_level: 3,
        bit_rate: 96,
    },
    ProtLevelEntry {
        cu_size: 84,
        prot_level: 2,
        bit_rate: 96,
    },
    ProtLevelEntry {
        cu_size: 104,
        prot_level: 1,
        bit_rate: 96,
    },
    ProtLevelEntry {
        cu_size: 58,
        prot_level: 5,
        bit_rate: 112,
    },
    ProtLevelEntry {
        cu_size: 70,
        prot_level: 4,
        bit_rate: 112,
    },
    ProtLevelEntry {
        cu_size: 84,
        prot_level: 3,
        bit_rate: 112,
    },
    ProtLevelEntry {
        cu_size: 104,
        prot_level: 2,
        bit_rate: 112,
    },
    ProtLevelEntry {
        cu_size: 64,
        prot_level: 5,
        bit_rate: 128,
    },
    ProtLevelEntry {
        cu_size: 84,
        prot_level: 4,
        bit_rate: 128,
    },
    ProtLevelEntry {
        cu_size: 96,
        prot_level: 3,
        bit_rate: 128,
    },
    ProtLevelEntry {
        cu_size: 116,
        prot_level: 2,
        bit_rate: 128,
    },
    ProtLevelEntry {
        cu_size: 140,
        prot_level: 1,
        bit_rate: 128,
    },
    ProtLevelEntry {
        cu_size: 80,
        prot_level: 5,
        bit_rate: 160,
    },
    ProtLevelEntry {
        cu_size: 104,
        prot_level: 4,
        bit_rate: 160,
    },
    ProtLevelEntry {
        cu_size: 116,
        prot_level: 3,
        bit_rate: 160,
    },
    ProtLevelEntry {
        cu_size: 140,
        prot_level: 2,
        bit_rate: 160,
    },
    ProtLevelEntry {
        cu_size: 168,
        prot_level: 1,
        bit_rate: 160,
    },
    ProtLevelEntry {
        cu_size: 96,
        prot_level: 5,
        bit_rate: 192,
    },
    ProtLevelEntry {
        cu_size: 116,
        prot_level: 4,
        bit_rate: 192,
    },
    ProtLevelEntry {
        cu_size: 140,
        prot_level: 3,
        bit_rate: 192,
    },
    ProtLevelEntry {
        cu_size: 168,
        prot_level: 2,
        bit_rate: 192,
    },
    ProtLevelEntry {
        cu_size: 208,
        prot_level: 1,
        bit_rate: 192,
    },
    ProtLevelEntry {
        cu_size: 116,
        prot_level: 5,
        bit_rate: 224,
    },
    ProtLevelEntry {
        cu_size: 140,
        prot_level: 4,
        bit_rate: 224,
    },
    ProtLevelEntry {
        cu_size: 168,
        prot_level: 3,
        bit_rate: 224,
    },
    ProtLevelEntry {
        cu_size: 208,
        prot_level: 2,
        bit_rate: 224,
    },
    ProtLevelEntry {
        cu_size: 232,
        prot_level: 1,
        bit_rate: 224,
    },
    ProtLevelEntry {
        cu_size: 128,
        prot_level: 5,
        bit_rate: 256,
    },
    ProtLevelEntry {
        cu_size: 168,
        prot_level: 4,
        bit_rate: 256,
    },
    ProtLevelEntry {
        cu_size: 192,
        prot_level: 3,
        bit_rate: 256,
    },
    ProtLevelEntry {
        cu_size: 232,
        prot_level: 2,
        bit_rate: 256,
    },
    ProtLevelEntry {
        cu_size: 280,
        prot_level: 1,
        bit_rate: 256,
    },
    ProtLevelEntry {
        cu_size: 160,
        prot_level: 5,
        bit_rate: 320,
    },
    ProtLevelEntry {
        cu_size: 208,
        prot_level: 4,
        bit_rate: 320,
    },
    ProtLevelEntry {
        cu_size: 280,
        prot_level: 2,
        bit_rate: 320,
    },
    ProtLevelEntry {
        cu_size: 192,
        prot_level: 5,
        bit_rate: 384,
    },
    ProtLevelEntry {
        cu_size: 280,
        prot_level: 3,
        bit_rate: 384,
    },
    ProtLevelEntry {
        cu_size: 416,
        prot_level: 1,
        bit_rate: 384,
    },
];

#[cfg(test)]
mod tests {
    use super::{derive_eep_bit_rate, FibDecoder, ServiceRecord, SubChannelInfo};

    fn set_bits(buf: &mut [u8], bit_offset: usize, bit_len: usize, value: u32) {
        for idx in 0..bit_len {
            let src_bit = (value >> (bit_len - 1 - idx)) & 1;
            let absolute = bit_offset + idx;
            let byte_idx = absolute / 8;
            let bit_in_byte = 7 - (absolute % 8);
            if src_bit != 0 {
                buf[byte_idx] |= 1 << bit_in_byte;
            } else {
                buf[byte_idx] &= !(1 << bit_in_byte);
            }
        }
    }

    #[test]
    fn derives_expected_eep_a_bitrates() {
        assert_eq!(derive_eep_bit_rate(0, 2, 84), 112);
        assert_eq!(derive_eep_bit_rate(0, 3, 32), 64);
    }

    #[test]
    fn parses_fig0_ensemble_information_and_cif_count() {
        let mut fib = [0xFFu8; 32];
        fib[0] = 0x06;
        fib[1] = 0x00;
        fib[2] = 0x12;
        fib[3] = 0x34;
        fib[4] = 0x07;
        fib[5] = 0x2A;
        fib[6] = 0x00;

        let mut decoder = FibDecoder::default();
        decoder.process_fib(&fib);

        assert_eq!(decoder.ensemble_id(), Some(0x1234));
        assert_eq!(decoder.cif_count(), Some(7 * 250 + 0x2A));
    }

    #[test]
    fn empty_decoder_has_no_selected_service() {
        let decoder = FibDecoder::default();
        assert!(decoder.selected_audio_service(0xF2F8, None).is_none());
    }

    #[test]
    fn requested_sid_does_not_fall_back_to_another_service() {
        let mut decoder = FibDecoder::default();
        decoder.subchannels.insert(
            3,
            SubChannelInfo {
                start_addr: 100,
                cu_size: 66,
                bit_rate: 88,
                short_form: false,
                prot_level: 2,
            },
        );
        decoder.services.insert(
            0x1234,
            ServiceRecord {
                sid: 0x1234,
                label: "Other".to_string(),
                subch_id: Some(3),
                has_primary_audio: true,
            },
        );

        assert!(decoder.selected_audio_service(0xF2F8, None).is_none());
    }

    #[test]
    fn fig0_ext2_prefers_primary_audio_component() {
        let mut decoder = FibDecoder::default();
        decoder.subchannels.insert(
            4,
            SubChannelInfo {
                start_addr: 528,
                cu_size: 66,
                bit_rate: 88,
                short_form: false,
                prot_level: 2,
            },
        );
        decoder.subchannels.insert(
            5,
            SubChannelInfo {
                start_addr: 600,
                cu_size: 66,
                bit_rate: 88,
                short_form: false,
                prot_level: 2,
            },
        );

        let mut fig = [0u8; 9];
        set_bits(&mut fig, 16, 16, 0xF2F8);
        set_bits(&mut fig, 36, 4, 2);

        set_bits(&mut fig, 40, 2, 0);
        set_bits(&mut fig, 48, 6, 5);
        set_bits(&mut fig, 54, 1, 0);

        set_bits(&mut fig, 56, 2, 0);
        set_bits(&mut fig, 64, 6, 4);
        set_bits(&mut fig, 70, 1, 1);

        decoder.process_fig0_ext2(&fig, 0);

        let info = decoder
            .selected_audio_service(0xF2F8, None)
            .expect("primary audio service should be selected");
        assert_eq!(info.subch_id, 4);
        assert_eq!(info.start_addr, 528);
    }
}
