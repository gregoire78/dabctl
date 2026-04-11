// Protection (EEP + UEP) - converted from eep-protection.cpp + uep-protection.cpp (eti-cmdline)

use crate::pipeline::prot_tables::get_pcodes;
use crate::pipeline::viterbi_handler::ViterbiSpiral;

pub struct EepProtection {
    index_table: Vec<bool>,
    viterbi_block: Vec<i16>,
    viterbi: ViterbiSpiral,
}

#[inline]
fn is_valid_protection_bit_rate(bit_rate: i16) -> bool {
    bit_rate > 0
}

#[inline]
fn depuncture_soft_bits(index_table: &[bool], input: &[i16], block: &mut [i16]) -> usize {
    block.fill(0);
    let mut input_counter = 0usize;
    for (i, dst) in block.iter_mut().enumerate() {
        if i < index_table.len() && index_table[i] && input_counter < input.len() {
            *dst = input[input_counter];
            input_counter += 1;
        }
    }
    input_counter
}

#[cfg(test)]
#[inline]
fn count_marked_positions(index_table: &[bool]) -> usize {
    index_table.iter().filter(|&&b| b).count()
}

impl EepProtection {
    pub fn new(bit_rate: i16, prot_level: i16) -> Self {
        if !is_valid_protection_bit_rate(bit_rate) {
            tracing::warn!(
                bit_rate,
                prot_level,
                "EEP invalid bit_rate, creating empty protection profile"
            );
            return EepProtection {
                index_table: vec![false; 24],
                viterbi_block: vec![0i16; 24],
                viterbi: ViterbiSpiral::new(0),
            };
        }

        let out_size = 24 * bit_rate as usize;
        let mut index_table = vec![false; out_size * 4 + 24];
        let mut viterbi_counter = 0usize;

        let (l1, l2, pi1_idx, pi2_idx);

        if (prot_level & (1 << 2)) == 0 {
            // A profiles
            match prot_level & 0x03 {
                0 => {
                    l1 = 6 * bit_rate as usize / 8 - 3;
                    l2 = 3;
                    pi1_idx = 23;
                    pi2_idx = 22;
                }
                1 => {
                    if bit_rate == 8 {
                        l1 = 5;
                        l2 = 1;
                        pi1_idx = 12;
                        pi2_idx = 11;
                    } else {
                        l1 = 2 * bit_rate as usize / 8 - 3;
                        l2 = 4 * bit_rate as usize / 8 + 3;
                        pi1_idx = 13;
                        pi2_idx = 12;
                    }
                }
                2 => {
                    l1 = 6 * bit_rate as usize / 8 - 3;
                    l2 = 3;
                    pi1_idx = 7;
                    pi2_idx = 6;
                }
                3 => {
                    l1 = 4 * bit_rate as usize / 8 - 3;
                    l2 = 2 * bit_rate as usize / 8 + 3;
                    pi1_idx = 2;
                    pi2_idx = 1;
                }
                _ => unreachable!(),
            }
        } else {
            // B profiles
            match prot_level & 0x03 {
                3 => {
                    l1 = 24 * bit_rate as usize / 32 - 3;
                    l2 = 3;
                    pi1_idx = 1;
                    pi2_idx = 0;
                }
                2 => {
                    l1 = 24 * bit_rate as usize / 32 - 3;
                    l2 = 3;
                    pi1_idx = 3;
                    pi2_idx = 2;
                }
                1 => {
                    l1 = 24 * bit_rate as usize / 32 - 3;
                    l2 = 3;
                    pi1_idx = 5;
                    pi2_idx = 4;
                }
                0 => {
                    l1 = 24 * bit_rate as usize / 32 - 3;
                    l2 = 3;
                    pi1_idx = 9;
                    pi2_idx = 8;
                }
                _ => unreachable!(),
            }
        }

        let pi_x_idx = 7; // PI_8

        for _ in 0..l1 {
            for j in 0..128 {
                if get_pcodes(pi1_idx)[j % 32] != 0 {
                    index_table[viterbi_counter] = true;
                }
                viterbi_counter += 1;
            }
        }
        for _ in 0..l2 {
            for j in 0..128 {
                if get_pcodes(pi2_idx)[j % 32] != 0 {
                    index_table[viterbi_counter] = true;
                }
                viterbi_counter += 1;
            }
        }
        for i in 0..24 {
            if get_pcodes(pi_x_idx)[i] != 0 {
                index_table[viterbi_counter] = true;
            }
            viterbi_counter += 1;
        }

        EepProtection {
            index_table,
            viterbi_block: vec![0i16; out_size * 4 + 24],
            viterbi: ViterbiSpiral::new(out_size),
        }
    }

    pub fn deconvolve(&mut self, v: &[i16], out_buffer: &mut [u8]) {
        depuncture_soft_bits(&self.index_table, v, &mut self.viterbi_block);
        self.viterbi.deconvolve(&self.viterbi_block, out_buffer);
    }
}

// UEP protection profile table
struct ProtProfile {
    bit_rate: i16,
    prot_level: i16,
    l1: i16,
    l2: i16,
    l3: i16,
    l4: i16,
    pi1: i16,
    pi2: i16,
    pi3: i16,
    pi4: i16,
}

static PROFILE_TABLE: &[ProtProfile] = &[
    ProtProfile {
        bit_rate: 32,
        prot_level: 5,
        l1: 3,
        l2: 4,
        l3: 17,
        l4: 0,
        pi1: 5,
        pi2: 3,
        pi3: 2,
        pi4: -1,
    },
    ProtProfile {
        bit_rate: 32,
        prot_level: 4,
        l1: 3,
        l2: 3,
        l3: 18,
        l4: 0,
        pi1: 11,
        pi2: 6,
        pi3: 5,
        pi4: -1,
    },
    ProtProfile {
        bit_rate: 32,
        prot_level: 3,
        l1: 3,
        l2: 4,
        l3: 14,
        l4: 3,
        pi1: 15,
        pi2: 9,
        pi3: 6,
        pi4: 8,
    },
    ProtProfile {
        bit_rate: 32,
        prot_level: 2,
        l1: 3,
        l2: 4,
        l3: 14,
        l4: 3,
        pi1: 22,
        pi2: 13,
        pi3: 8,
        pi4: 13,
    },
    ProtProfile {
        bit_rate: 32,
        prot_level: 1,
        l1: 3,
        l2: 5,
        l3: 13,
        l4: 3,
        pi1: 24,
        pi2: 17,
        pi3: 12,
        pi4: 17,
    },
    ProtProfile {
        bit_rate: 48,
        prot_level: 5,
        l1: 4,
        l2: 3,
        l3: 26,
        l4: 3,
        pi1: 5,
        pi2: 4,
        pi3: 2,
        pi4: 3,
    },
    ProtProfile {
        bit_rate: 48,
        prot_level: 4,
        l1: 3,
        l2: 4,
        l3: 26,
        l4: 3,
        pi1: 9,
        pi2: 6,
        pi3: 4,
        pi4: 6,
    },
    ProtProfile {
        bit_rate: 48,
        prot_level: 3,
        l1: 3,
        l2: 4,
        l3: 26,
        l4: 3,
        pi1: 15,
        pi2: 10,
        pi3: 6,
        pi4: 9,
    },
    ProtProfile {
        bit_rate: 48,
        prot_level: 2,
        l1: 3,
        l2: 4,
        l3: 26,
        l4: 3,
        pi1: 24,
        pi2: 14,
        pi3: 8,
        pi4: 15,
    },
    ProtProfile {
        bit_rate: 48,
        prot_level: 1,
        l1: 3,
        l2: 5,
        l3: 25,
        l4: 3,
        pi1: 24,
        pi2: 18,
        pi3: 13,
        pi4: 18,
    },
    ProtProfile {
        bit_rate: 56,
        prot_level: 5,
        l1: 6,
        l2: 10,
        l3: 23,
        l4: 3,
        pi1: 5,
        pi2: 4,
        pi3: 2,
        pi4: 3,
    },
    ProtProfile {
        bit_rate: 56,
        prot_level: 4,
        l1: 6,
        l2: 10,
        l3: 23,
        l4: 3,
        pi1: 9,
        pi2: 6,
        pi3: 4,
        pi4: 5,
    },
    ProtProfile {
        bit_rate: 56,
        prot_level: 3,
        l1: 6,
        l2: 12,
        l3: 21,
        l4: 3,
        pi1: 16,
        pi2: 7,
        pi3: 6,
        pi4: 9,
    },
    ProtProfile {
        bit_rate: 56,
        prot_level: 2,
        l1: 6,
        l2: 10,
        l3: 23,
        l4: 3,
        pi1: 23,
        pi2: 13,
        pi3: 8,
        pi4: 13,
    },
    ProtProfile {
        bit_rate: 64,
        prot_level: 5,
        l1: 6,
        l2: 9,
        l3: 31,
        l4: 2,
        pi1: 5,
        pi2: 3,
        pi3: 2,
        pi4: 3,
    },
    ProtProfile {
        bit_rate: 64,
        prot_level: 4,
        l1: 6,
        l2: 9,
        l3: 33,
        l4: 0,
        pi1: 11,
        pi2: 6,
        pi3: 5,
        pi4: -1,
    },
    ProtProfile {
        bit_rate: 64,
        prot_level: 3,
        l1: 6,
        l2: 12,
        l3: 27,
        l4: 3,
        pi1: 16,
        pi2: 8,
        pi3: 6,
        pi4: 9,
    },
    ProtProfile {
        bit_rate: 64,
        prot_level: 2,
        l1: 6,
        l2: 10,
        l3: 29,
        l4: 3,
        pi1: 23,
        pi2: 13,
        pi3: 8,
        pi4: 13,
    },
    ProtProfile {
        bit_rate: 64,
        prot_level: 1,
        l1: 6,
        l2: 11,
        l3: 28,
        l4: 3,
        pi1: 24,
        pi2: 18,
        pi3: 12,
        pi4: 18,
    },
    ProtProfile {
        bit_rate: 80,
        prot_level: 5,
        l1: 6,
        l2: 10,
        l3: 41,
        l4: 3,
        pi1: 6,
        pi2: 3,
        pi3: 2,
        pi4: 3,
    },
    ProtProfile {
        bit_rate: 80,
        prot_level: 4,
        l1: 6,
        l2: 10,
        l3: 41,
        l4: 3,
        pi1: 11,
        pi2: 6,
        pi3: 5,
        pi4: 6,
    },
    ProtProfile {
        bit_rate: 80,
        prot_level: 3,
        l1: 6,
        l2: 11,
        l3: 40,
        l4: 3,
        pi1: 16,
        pi2: 8,
        pi3: 6,
        pi4: 7,
    },
    ProtProfile {
        bit_rate: 80,
        prot_level: 2,
        l1: 6,
        l2: 10,
        l3: 41,
        l4: 3,
        pi1: 23,
        pi2: 13,
        pi3: 8,
        pi4: 13,
    },
    ProtProfile {
        bit_rate: 80,
        prot_level: 1,
        l1: 6,
        l2: 10,
        l3: 41,
        l4: 3,
        pi1: 24,
        pi2: 7,
        pi3: 12,
        pi4: 18,
    },
    ProtProfile {
        bit_rate: 96,
        prot_level: 5,
        l1: 7,
        l2: 9,
        l3: 53,
        l4: 3,
        pi1: 5,
        pi2: 4,
        pi3: 2,
        pi4: 4,
    },
    ProtProfile {
        bit_rate: 96,
        prot_level: 4,
        l1: 7,
        l2: 10,
        l3: 52,
        l4: 3,
        pi1: 9,
        pi2: 6,
        pi3: 4,
        pi4: 6,
    },
    ProtProfile {
        bit_rate: 96,
        prot_level: 3,
        l1: 6,
        l2: 12,
        l3: 51,
        l4: 3,
        pi1: 16,
        pi2: 9,
        pi3: 6,
        pi4: 10,
    },
    ProtProfile {
        bit_rate: 96,
        prot_level: 2,
        l1: 6,
        l2: 10,
        l3: 53,
        l4: 3,
        pi1: 22,
        pi2: 12,
        pi3: 9,
        pi4: 12,
    },
    ProtProfile {
        bit_rate: 96,
        prot_level: 1,
        l1: 6,
        l2: 13,
        l3: 50,
        l4: 3,
        pi1: 24,
        pi2: 18,
        pi3: 13,
        pi4: 19,
    },
    ProtProfile {
        bit_rate: 112,
        prot_level: 5,
        l1: 14,
        l2: 17,
        l3: 50,
        l4: 3,
        pi1: 5,
        pi2: 4,
        pi3: 2,
        pi4: 5,
    },
    ProtProfile {
        bit_rate: 112,
        prot_level: 4,
        l1: 11,
        l2: 21,
        l3: 49,
        l4: 3,
        pi1: 9,
        pi2: 6,
        pi3: 4,
        pi4: 8,
    },
    ProtProfile {
        bit_rate: 112,
        prot_level: 3,
        l1: 11,
        l2: 23,
        l3: 47,
        l4: 3,
        pi1: 16,
        pi2: 8,
        pi3: 6,
        pi4: 9,
    },
    ProtProfile {
        bit_rate: 112,
        prot_level: 2,
        l1: 11,
        l2: 21,
        l3: 49,
        l4: 3,
        pi1: 23,
        pi2: 12,
        pi3: 9,
        pi4: 14,
    },
    ProtProfile {
        bit_rate: 128,
        prot_level: 5,
        l1: 12,
        l2: 19,
        l3: 62,
        l4: 3,
        pi1: 5,
        pi2: 3,
        pi3: 2,
        pi4: 4,
    },
    ProtProfile {
        bit_rate: 128,
        prot_level: 4,
        l1: 11,
        l2: 21,
        l3: 61,
        l4: 3,
        pi1: 11,
        pi2: 6,
        pi3: 5,
        pi4: 7,
    },
    ProtProfile {
        bit_rate: 128,
        prot_level: 3,
        l1: 11,
        l2: 22,
        l3: 60,
        l4: 3,
        pi1: 16,
        pi2: 9,
        pi3: 6,
        pi4: 10,
    },
    ProtProfile {
        bit_rate: 128,
        prot_level: 2,
        l1: 11,
        l2: 21,
        l3: 61,
        l4: 3,
        pi1: 22,
        pi2: 12,
        pi3: 9,
        pi4: 14,
    },
    ProtProfile {
        bit_rate: 128,
        prot_level: 1,
        l1: 11,
        l2: 20,
        l3: 62,
        l4: 3,
        pi1: 24,
        pi2: 17,
        pi3: 13,
        pi4: 19,
    },
    ProtProfile {
        bit_rate: 160,
        prot_level: 5,
        l1: 11,
        l2: 19,
        l3: 87,
        l4: 3,
        pi1: 5,
        pi2: 4,
        pi3: 2,
        pi4: 4,
    },
    ProtProfile {
        bit_rate: 160,
        prot_level: 4,
        l1: 11,
        l2: 23,
        l3: 83,
        l4: 3,
        pi1: 11,
        pi2: 6,
        pi3: 5,
        pi4: 9,
    },
    ProtProfile {
        bit_rate: 160,
        prot_level: 3,
        l1: 11,
        l2: 24,
        l3: 82,
        l4: 3,
        pi1: 16,
        pi2: 8,
        pi3: 6,
        pi4: 11,
    },
    ProtProfile {
        bit_rate: 160,
        prot_level: 2,
        l1: 11,
        l2: 21,
        l3: 85,
        l4: 3,
        pi1: 22,
        pi2: 11,
        pi3: 9,
        pi4: 13,
    },
    ProtProfile {
        bit_rate: 160,
        prot_level: 1,
        l1: 11,
        l2: 22,
        l3: 84,
        l4: 3,
        pi1: 24,
        pi2: 18,
        pi3: 12,
        pi4: 19,
    },
    ProtProfile {
        bit_rate: 192,
        prot_level: 5,
        l1: 11,
        l2: 20,
        l3: 110,
        l4: 3,
        pi1: 6,
        pi2: 4,
        pi3: 2,
        pi4: 5,
    },
    ProtProfile {
        bit_rate: 192,
        prot_level: 4,
        l1: 11,
        l2: 22,
        l3: 108,
        l4: 3,
        pi1: 10,
        pi2: 6,
        pi3: 4,
        pi4: 9,
    },
    ProtProfile {
        bit_rate: 192,
        prot_level: 3,
        l1: 11,
        l2: 24,
        l3: 106,
        l4: 3,
        pi1: 16,
        pi2: 10,
        pi3: 6,
        pi4: 11,
    },
    ProtProfile {
        bit_rate: 192,
        prot_level: 2,
        l1: 11,
        l2: 20,
        l3: 110,
        l4: 3,
        pi1: 22,
        pi2: 13,
        pi3: 9,
        pi4: 13,
    },
    ProtProfile {
        bit_rate: 192,
        prot_level: 1,
        l1: 11,
        l2: 21,
        l3: 109,
        l4: 3,
        pi1: 24,
        pi2: 20,
        pi3: 13,
        pi4: 24,
    },
    ProtProfile {
        bit_rate: 224,
        prot_level: 5,
        l1: 12,
        l2: 22,
        l3: 131,
        l4: 3,
        pi1: 8,
        pi2: 6,
        pi3: 2,
        pi4: 6,
    },
    ProtProfile {
        bit_rate: 224,
        prot_level: 4,
        l1: 12,
        l2: 26,
        l3: 127,
        l4: 3,
        pi1: 12,
        pi2: 8,
        pi3: 4,
        pi4: 11,
    },
    ProtProfile {
        bit_rate: 224,
        prot_level: 3,
        l1: 11,
        l2: 20,
        l3: 134,
        l4: 3,
        pi1: 16,
        pi2: 10,
        pi3: 7,
        pi4: 9,
    },
    ProtProfile {
        bit_rate: 224,
        prot_level: 2,
        l1: 11,
        l2: 22,
        l3: 132,
        l4: 3,
        pi1: 24,
        pi2: 16,
        pi3: 10,
        pi4: 15,
    },
    ProtProfile {
        bit_rate: 224,
        prot_level: 1,
        l1: 11,
        l2: 24,
        l3: 130,
        l4: 3,
        pi1: 24,
        pi2: 20,
        pi3: 12,
        pi4: 20,
    },
    ProtProfile {
        bit_rate: 256,
        prot_level: 5,
        l1: 11,
        l2: 24,
        l3: 154,
        l4: 3,
        pi1: 6,
        pi2: 5,
        pi3: 2,
        pi4: 5,
    },
    ProtProfile {
        bit_rate: 256,
        prot_level: 4,
        l1: 11,
        l2: 24,
        l3: 154,
        l4: 3,
        pi1: 12,
        pi2: 9,
        pi3: 5,
        pi4: 10,
    },
    ProtProfile {
        bit_rate: 256,
        prot_level: 3,
        l1: 11,
        l2: 27,
        l3: 151,
        l4: 3,
        pi1: 16,
        pi2: 10,
        pi3: 7,
        pi4: 10,
    },
    ProtProfile {
        bit_rate: 256,
        prot_level: 2,
        l1: 11,
        l2: 22,
        l3: 156,
        l4: 3,
        pi1: 24,
        pi2: 14,
        pi3: 10,
        pi4: 13,
    },
    ProtProfile {
        bit_rate: 256,
        prot_level: 1,
        l1: 11,
        l2: 26,
        l3: 152,
        l4: 3,
        pi1: 24,
        pi2: 19,
        pi3: 14,
        pi4: 18,
    },
    ProtProfile {
        bit_rate: 320,
        prot_level: 5,
        l1: 11,
        l2: 26,
        l3: 200,
        l4: 3,
        pi1: 8,
        pi2: 5,
        pi3: 2,
        pi4: 6,
    },
    ProtProfile {
        bit_rate: 320,
        prot_level: 4,
        l1: 11,
        l2: 25,
        l3: 201,
        l4: 3,
        pi1: 13,
        pi2: 9,
        pi3: 5,
        pi4: 10,
    },
    ProtProfile {
        bit_rate: 320,
        prot_level: 2,
        l1: 11,
        l2: 26,
        l3: 200,
        l4: 3,
        pi1: 24,
        pi2: 17,
        pi3: 9,
        pi4: 17,
    },
    ProtProfile {
        bit_rate: 384,
        prot_level: 5,
        l1: 11,
        l2: 27,
        l3: 247,
        l4: 3,
        pi1: 8,
        pi2: 6,
        pi3: 2,
        pi4: 7,
    },
    ProtProfile {
        bit_rate: 384,
        prot_level: 3,
        l1: 11,
        l2: 24,
        l3: 250,
        l4: 3,
        pi1: 16,
        pi2: 9,
        pi3: 7,
        pi4: 10,
    },
    ProtProfile {
        bit_rate: 384,
        prot_level: 1,
        l1: 12,
        l2: 28,
        l3: 245,
        l4: 3,
        pi1: 24,
        pi2: 20,
        pi3: 14,
        pi4: 23,
    },
];

fn find_profile(bit_rate: i16, prot_level: i16) -> Option<usize> {
    PROFILE_TABLE
        .iter()
        .position(|p| p.bit_rate == bit_rate && p.prot_level == prot_level)
}

#[inline]
fn resolve_uep_profile_index(bit_rate: i16, prot_level: i16) -> Option<usize> {
    if let Some(exact) = find_profile(bit_rate, prot_level) {
        return Some(exact);
    }
    // Keep fallback bitrate-consistent: if the exact protection level is absent,
    // use any profile with the same bitrate.
    PROFILE_TABLE.iter().position(|p| p.bit_rate == bit_rate)
}

pub struct UepProtection {
    index_table: Vec<bool>,
    viterbi_block: Vec<i16>,
    viterbi: ViterbiSpiral,
}

impl UepProtection {
    pub fn new(bit_rate: i16, prot_level: i16) -> Self {
        if !is_valid_protection_bit_rate(bit_rate) {
            tracing::warn!(
                bit_rate,
                prot_level,
                "UEP invalid bit_rate, creating empty protection profile"
            );
            return UepProtection {
                index_table: vec![false; 24],
                viterbi_block: vec![0i16; 24],
                viterbi: ViterbiSpiral::new(0),
            };
        }

        let index = match resolve_uep_profile_index(bit_rate, prot_level) {
            Some(i) => i,
            None => {
                tracing::warn!(
                    bit_rate,
                    prot_level,
                    "UEP bitrate not found in ETSI table, creating empty protection profile"
                );
                return UepProtection {
                    index_table: vec![false; 24],
                    viterbi_block: vec![0i16; 24],
                    viterbi: ViterbiSpiral::new(0),
                };
            }
        };

        if find_profile(bit_rate, prot_level).is_none() {
            // The FIG 0/1 carried an (bit_rate, prot_level) pair that is absent
            // from the UEP profile table (ETSI EN 300 401 §6.2.1, Table 7).
            // Fall back to a profile with the same bitrate.
            tracing::warn!(
                bit_rate,
                prot_level,
                fallback_index = index,
                "UEP profile not found in table, falling back to same-bitrate profile"
            );
        }

        let out_size = 24 * bit_rate as usize;
        let p = &PROFILE_TABLE[index];

        let mut index_table = vec![false; out_size * 4 + 24];
        let mut vc = 0usize;

        let pi1 = get_pcodes((p.pi1 - 1) as usize);
        let pi2 = get_pcodes((p.pi2 - 1) as usize);
        let pi3 = get_pcodes((p.pi3 - 1) as usize);
        let pi4 = if p.pi4 > 0 {
            Some(get_pcodes((p.pi4 - 1) as usize))
        } else {
            None
        };
        let pi_x = get_pcodes(7);

        for _ in 0..p.l1 {
            for j in 0..128 {
                if pi1[j % 32] != 0 {
                    index_table[vc] = true;
                }
                vc += 1;
            }
        }
        for _ in 0..p.l2 {
            for j in 0..128 {
                if pi2[j % 32] != 0 {
                    index_table[vc] = true;
                }
                vc += 1;
            }
        }
        for _ in 0..p.l3 {
            for j in 0..128 {
                if pi3[j % 32] != 0 {
                    index_table[vc] = true;
                }
                vc += 1;
            }
        }
        if let Some(pi4) = pi4 {
            for _ in 0..p.l4 {
                for j in 0..128 {
                    if pi4[j % 32] != 0 {
                        index_table[vc] = true;
                    }
                    vc += 1;
                }
            }
        }
        for &px in pi_x.iter().take(24) {
            if px != 0 {
                index_table[vc] = true;
            }
            vc += 1;
        }

        UepProtection {
            index_table,
            viterbi_block: vec![0i16; out_size * 4 + 24],
            viterbi: ViterbiSpiral::new(out_size),
        }
    }

    pub fn deconvolve(&mut self, v: &[i16], out_buffer: &mut [u8]) {
        depuncture_soft_bits(&self.index_table, v, &mut self.viterbi_block);
        self.viterbi.deconvolve(&self.viterbi_block, out_buffer);
    }
}

pub enum Protection {
    Eep(EepProtection),
    Uep(UepProtection),
}

impl Protection {
    pub fn deconvolve(&mut self, v: &[i16], out_buffer: &mut [u8]) {
        match self {
            Protection::Eep(p) => p.deconvolve(v, out_buffer),
            Protection::Uep(p) => p.deconvolve(v, out_buffer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn count_marked_positions_counts_true_entries() {
        let table = [true, false, true, true, false, false, true];
        assert_eq!(count_marked_positions(&table), 4);
    }

    #[test]
    fn resolve_uep_profile_index_known_pair() {
        let idx = resolve_uep_profile_index(32, 4);
        assert_eq!(idx, Some(1), "(32,4) must map to profile index 1");
    }

    #[test]
    fn resolve_uep_profile_index_unknown_pair_falls_back_to_same_bitrate() {
        let idx = resolve_uep_profile_index(999, 99);
        assert_eq!(idx, None, "unknown bitrate must return None");
    }

    #[test]
    fn resolve_uep_profile_index_unknown_level_with_known_bitrate() {
        // 64 kbps exists in table, but level 99 does not.
        let idx = resolve_uep_profile_index(64, 99).expect("must find same-bitrate fallback");
        assert_eq!(PROFILE_TABLE[idx].bit_rate, 64);
    }

    #[test]
    fn uep_profile_pairs_are_unique() {
        // ETSI EN 300 401 §6.2.1 profile table must not contain duplicate
        // (bit_rate, prot_level) keys.
        let mut seen = HashSet::<(i16, i16)>::new();
        for p in PROFILE_TABLE {
            let key = (p.bit_rate, p.prot_level);
            assert!(seen.insert(key), "duplicate UEP profile key: {key:?}");
        }
    }

    #[test]
    fn uep_profile_fields_have_valid_ranges() {
        // Sanity ranges expected by constructor loops and get_pcodes lookup.
        for p in PROFILE_TABLE {
            assert!(p.bit_rate > 0, "bit_rate must be positive");
            assert!(p.prot_level >= 1 && p.prot_level <= 5);

            assert!(p.l1 >= 0 && p.l2 >= 0 && p.l3 >= 0 && p.l4 >= 0);
            assert!(p.l1 + p.l2 + p.l3 + p.l4 > 0, "all L segments are zero");

            assert!((1..=24).contains(&p.pi1));
            assert!((1..=24).contains(&p.pi2));
            assert!((1..=24).contains(&p.pi3));
            if p.pi4 != -1 {
                assert!((1..=24).contains(&p.pi4));
            }
        }
    }

    #[test]
    fn eep_creation_a1() {
        let _p = EepProtection::new(64, 0);
    }

    #[test]
    fn eep_index_table_has_valid_coverage() {
        let p = EepProtection::new(64, 0);
        assert_eq!(p.index_table.len(), 24 * 64 * 4 + 24);
        let marked = count_marked_positions(&p.index_table);
        assert!(marked > 0, "EEP index table must mark some positions");
        assert!(
            marked <= p.index_table.len(),
            "marked positions cannot exceed table length"
        );
    }

    #[test]
    fn eep_creation_a2() {
        let _p = EepProtection::new(128, 1);
    }

    #[test]
    fn eep_creation_a3() {
        let _p = EepProtection::new(64, 2);
    }

    #[test]
    fn eep_creation_a4() {
        let _p = EepProtection::new(64, 3);
    }

    #[test]
    fn eep_creation_b_profiles() {
        for level in 4..8 {
            let _p = EepProtection::new(64, level);
        }
    }

    #[test]
    fn eep_deconvolve_zero_input() {
        let mut p = EepProtection::new(64, 0);
        let in_size = 24 * 64;
        let input = vec![0i16; in_size * 4 + 24];
        let mut output = vec![0u8; in_size];
        p.deconvolve(&input, &mut output);
        assert!(output.iter().all(|&b| b == 0 || b == 1));
    }

    #[test]
    fn protection_enum_eep() {
        let mut prot = Protection::Eep(EepProtection::new(64, 0));
        let in_size = 24 * 64;
        let input = vec![0i16; in_size * 4 + 24];
        let mut output = vec![0u8; in_size];
        prot.deconvolve(&input, &mut output);
    }

    /// UepProtection::new() with an unknown (bit_rate, prot_level) pair must not
    /// panic. Per ETSI EN 300 401 §6.2.1 the profile table is finite; a malformed
    /// FIG 0/1 may carry an unseen combination.  The constructor must fall back
    /// to a safe default rather than indexing the table at position 1 blindly or
    /// panicking when the search returns None.
    #[test]
    fn uep_unknown_profile_does_not_panic() {
        // bit_rate=999, prot_level=99 — not in PROFILE_TABLE
        let _p = UepProtection::new(999, 99);
    }

    #[test]
    fn uep_index_table_has_valid_coverage_for_known_profile() {
        let p = UepProtection::new(64, 4);
        assert_eq!(p.index_table.len(), 24 * 64 * 4 + 24);
        let marked = count_marked_positions(&p.index_table);
        assert!(marked > 0, "UEP index table must mark some positions");
        assert!(
            marked <= p.index_table.len(),
            "marked positions cannot exceed table length"
        );
    }

    /// The fallback profile used when the requested (bit_rate, prot_level) is absent
    /// must allow deconvolution to complete without panic.
    /// (Previously unwrap_or(1) was used, which is index 1 and silently wrong;
    /// unwrap_or(0) is still arbitrary but at least the intent is explicit and logged.)
    #[test]
    fn uep_unknown_profile_deconvolve_does_not_panic() {
        let mut p = UepProtection::new(32, 99); // prot_level=99 absent from table
                                                // out_size = 24 * 32 = 768; input must be out_size*4+24 i16s
        let out_size = 24 * 32;
        let input = vec![0i16; out_size * 4 + 24];
        let mut output = vec![0u8; out_size];
        p.deconvolve(&input, &mut output); // must not panic
    }

    #[test]
    fn uep_unknown_bitrate_creates_empty_profile() {
        let mut p = UepProtection::new(999, 99);
        let input = vec![0i16; 24];
        let mut output = Vec::<u8>::new();
        p.deconvolve(&input, &mut output);
        assert!(output.is_empty(), "unknown bitrate must emit no bits");
        assert_eq!(count_marked_positions(&p.index_table), 0);
    }

    #[test]
    fn eep_invalid_bit_rate_creates_empty_profile() {
        let mut p = EepProtection::new(0, 0);
        let input = vec![0i16; 24];
        let mut output = Vec::<u8>::new();
        p.deconvolve(&input, &mut output);
        assert!(output.is_empty(), "empty EEP profile must emit no bits");
        assert_eq!(count_marked_positions(&p.index_table), 0);
    }

    #[test]
    fn uep_invalid_bit_rate_creates_empty_profile() {
        let mut p = UepProtection::new(0, 0);
        let input = vec![0i16; 24];
        let mut output = Vec::<u8>::new();
        p.deconvolve(&input, &mut output);
        assert!(output.is_empty(), "empty UEP profile must emit no bits");
        assert_eq!(count_marked_positions(&p.index_table), 0);
    }

    #[test]
    fn depuncture_consumes_only_marked_positions() {
        let table = [true, false, true, true, false];
        let input = [10i16, 20, 30];
        let mut block = [0i16; 5];
        let consumed = depuncture_soft_bits(&table, &input, &mut block);
        assert_eq!(consumed, 3);
        assert_eq!(block, [10, 0, 20, 30, 0]);
    }

    #[test]
    fn depuncture_truncated_input_leaves_remaining_zero() {
        let table = [true, true, true, true];
        let input = [7i16, 8];
        let mut block = [99i16; 4];
        let consumed = depuncture_soft_bits(&table, &input, &mut block);
        assert_eq!(consumed, 2);
        assert_eq!(block, [7, 8, 0, 0]);
    }
}
