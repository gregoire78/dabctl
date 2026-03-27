// Protection (EEP + UEP) - converted from eep-protection.cpp + uep-protection.cpp (eti-cmdline)
// Copyright (C) 2013, 2017 Jan van Katwijk - Lazy Chair Computing

use crate::eti_handling::prot_tables::get_pcodes;
use crate::eti_handling::viterbi_handler::ViterbiSpiral;

pub struct EepProtection {
    out_size: usize,
    index_table: Vec<bool>,
    viterbi_block: Vec<i16>,
    viterbi: ViterbiSpiral,
}

impl EepProtection {
    pub fn new(bit_rate: i16, prot_level: i16) -> Self {
        let out_size = 24 * bit_rate as usize;
        let mut index_table = vec![false; out_size * 4 + 24];
        let mut viterbi_counter = 0usize;

        let (l1, l2, pi1_idx, pi2_idx);

        if (prot_level & (1 << 2)) == 0 {
            // A profiles
            match prot_level & 0x03 {
                0 => { l1 = 6 * bit_rate as usize / 8 - 3; l2 = 3; pi1_idx = 23; pi2_idx = 22; }
                1 => {
                    if bit_rate == 8 {
                        l1 = 5; l2 = 1; pi1_idx = 12; pi2_idx = 11;
                    } else {
                        l1 = 2 * bit_rate as usize / 8 - 3; l2 = 4 * bit_rate as usize / 8 + 3;
                        pi1_idx = 13; pi2_idx = 12;
                    }
                }
                2 => { l1 = 6 * bit_rate as usize / 8 - 3; l2 = 3; pi1_idx = 7; pi2_idx = 6; }
                3 => { l1 = 4 * bit_rate as usize / 8 - 3; l2 = 2 * bit_rate as usize / 8 + 3; pi1_idx = 2; pi2_idx = 1; }
                _ => unreachable!(),
            }
        } else {
            // B profiles
            match prot_level & 0x03 {
                3 => { l1 = 24 * bit_rate as usize / 32 - 3; l2 = 3; pi1_idx = 1; pi2_idx = 0; }
                2 => { l1 = 24 * bit_rate as usize / 32 - 3; l2 = 3; pi1_idx = 3; pi2_idx = 2; }
                1 => { l1 = 24 * bit_rate as usize / 32 - 3; l2 = 3; pi1_idx = 5; pi2_idx = 4; }
                0 => { l1 = 24 * bit_rate as usize / 32 - 3; l2 = 3; pi1_idx = 9; pi2_idx = 8; }
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
            out_size,
            index_table,
            viterbi_block: vec![0i16; out_size * 4 + 24],
            viterbi: ViterbiSpiral::new(out_size),
        }
    }

    pub fn deconvolve(&mut self, v: &[i16], out_buffer: &mut [u8]) {
        self.viterbi_block.fill(0);
        let mut input_counter = 0;
        for i in 0..(self.out_size * 4 + 24) {
            if self.index_table[i] {
                if input_counter < v.len() {
                    self.viterbi_block[i] = v[input_counter];
                    input_counter += 1;
                }
            }
        }
        self.viterbi.deconvolve(&self.viterbi_block, out_buffer);
    }
}

// UEP protection profile table
struct ProtProfile {
    bit_rate: i16, prot_level: i16,
    l1: i16, l2: i16, l3: i16, l4: i16,
    pi1: i16, pi2: i16, pi3: i16, pi4: i16,
}

static PROFILE_TABLE: &[ProtProfile] = &[
    ProtProfile{bit_rate:32, prot_level:5, l1:3, l2:4, l3:17, l4:0, pi1:5, pi2:3, pi3:2, pi4:-1},
    ProtProfile{bit_rate:32, prot_level:4, l1:3, l2:3, l3:18, l4:0, pi1:11, pi2:6, pi3:5, pi4:-1},
    ProtProfile{bit_rate:32, prot_level:3, l1:3, l2:4, l3:14, l4:3, pi1:15, pi2:9, pi3:6, pi4:8},
    ProtProfile{bit_rate:32, prot_level:2, l1:3, l2:4, l3:14, l4:3, pi1:22, pi2:13, pi3:8, pi4:13},
    ProtProfile{bit_rate:32, prot_level:1, l1:3, l2:5, l3:13, l4:3, pi1:24, pi2:17, pi3:12, pi4:17},
    ProtProfile{bit_rate:48, prot_level:5, l1:4, l2:3, l3:26, l4:3, pi1:5, pi2:4, pi3:2, pi4:3},
    ProtProfile{bit_rate:48, prot_level:4, l1:3, l2:4, l3:26, l4:3, pi1:9, pi2:6, pi3:4, pi4:6},
    ProtProfile{bit_rate:48, prot_level:3, l1:3, l2:4, l3:26, l4:3, pi1:15, pi2:10, pi3:6, pi4:9},
    ProtProfile{bit_rate:48, prot_level:2, l1:3, l2:4, l3:26, l4:3, pi1:24, pi2:14, pi3:8, pi4:15},
    ProtProfile{bit_rate:48, prot_level:1, l1:3, l2:5, l3:25, l4:3, pi1:24, pi2:18, pi3:13, pi4:18},
    ProtProfile{bit_rate:56, prot_level:5, l1:6, l2:10, l3:23, l4:3, pi1:5, pi2:4, pi3:2, pi4:3},
    ProtProfile{bit_rate:56, prot_level:4, l1:6, l2:10, l3:23, l4:3, pi1:9, pi2:6, pi3:4, pi4:5},
    ProtProfile{bit_rate:56, prot_level:3, l1:6, l2:12, l3:21, l4:3, pi1:16, pi2:7, pi3:6, pi4:9},
    ProtProfile{bit_rate:56, prot_level:2, l1:6, l2:10, l3:23, l4:3, pi1:23, pi2:13, pi3:8, pi4:13},
    ProtProfile{bit_rate:64, prot_level:5, l1:6, l2:9, l3:31, l4:2, pi1:5, pi2:3, pi3:2, pi4:3},
    ProtProfile{bit_rate:64, prot_level:4, l1:6, l2:9, l3:33, l4:0, pi1:11, pi2:6, pi3:5, pi4:-1},
    ProtProfile{bit_rate:64, prot_level:3, l1:6, l2:12, l3:27, l4:3, pi1:16, pi2:8, pi3:6, pi4:9},
    ProtProfile{bit_rate:64, prot_level:2, l1:6, l2:10, l3:29, l4:3, pi1:23, pi2:13, pi3:8, pi4:13},
    ProtProfile{bit_rate:64, prot_level:1, l1:6, l2:11, l3:28, l4:3, pi1:24, pi2:18, pi3:12, pi4:18},
    ProtProfile{bit_rate:80, prot_level:5, l1:6, l2:10, l3:41, l4:3, pi1:6, pi2:3, pi3:2, pi4:3},
    ProtProfile{bit_rate:80, prot_level:4, l1:6, l2:10, l3:41, l4:3, pi1:11, pi2:6, pi3:5, pi4:6},
    ProtProfile{bit_rate:80, prot_level:3, l1:6, l2:11, l3:40, l4:3, pi1:16, pi2:8, pi3:6, pi4:7},
    ProtProfile{bit_rate:80, prot_level:2, l1:6, l2:10, l3:41, l4:3, pi1:23, pi2:13, pi3:8, pi4:13},
    ProtProfile{bit_rate:80, prot_level:1, l1:6, l2:10, l3:41, l4:3, pi1:24, pi2:7, pi3:12, pi4:18},
    ProtProfile{bit_rate:96, prot_level:5, l1:7, l2:9, l3:53, l4:3, pi1:5, pi2:4, pi3:2, pi4:4},
    ProtProfile{bit_rate:96, prot_level:4, l1:7, l2:10, l3:52, l4:3, pi1:9, pi2:6, pi3:4, pi4:6},
    ProtProfile{bit_rate:96, prot_level:3, l1:6, l2:12, l3:51, l4:3, pi1:16, pi2:9, pi3:6, pi4:10},
    ProtProfile{bit_rate:96, prot_level:2, l1:6, l2:10, l3:53, l4:3, pi1:22, pi2:12, pi3:9, pi4:12},
    ProtProfile{bit_rate:96, prot_level:1, l1:6, l2:13, l3:50, l4:3, pi1:24, pi2:18, pi3:13, pi4:19},
    ProtProfile{bit_rate:112, prot_level:5, l1:14, l2:17, l3:50, l4:3, pi1:5, pi2:4, pi3:2, pi4:5},
    ProtProfile{bit_rate:112, prot_level:4, l1:11, l2:21, l3:49, l4:3, pi1:9, pi2:6, pi3:4, pi4:8},
    ProtProfile{bit_rate:112, prot_level:3, l1:11, l2:23, l3:47, l4:3, pi1:16, pi2:8, pi3:6, pi4:9},
    ProtProfile{bit_rate:112, prot_level:2, l1:11, l2:21, l3:49, l4:3, pi1:23, pi2:12, pi3:9, pi4:14},
    ProtProfile{bit_rate:128, prot_level:5, l1:12, l2:19, l3:62, l4:3, pi1:5, pi2:3, pi3:2, pi4:4},
    ProtProfile{bit_rate:128, prot_level:4, l1:11, l2:21, l3:61, l4:3, pi1:11, pi2:6, pi3:5, pi4:7},
    ProtProfile{bit_rate:128, prot_level:3, l1:11, l2:22, l3:60, l4:3, pi1:16, pi2:9, pi3:6, pi4:10},
    ProtProfile{bit_rate:128, prot_level:2, l1:11, l2:21, l3:61, l4:3, pi1:22, pi2:12, pi3:9, pi4:14},
    ProtProfile{bit_rate:128, prot_level:1, l1:11, l2:20, l3:62, l4:3, pi1:24, pi2:17, pi3:13, pi4:19},
    ProtProfile{bit_rate:160, prot_level:5, l1:11, l2:19, l3:87, l4:3, pi1:5, pi2:4, pi3:2, pi4:4},
    ProtProfile{bit_rate:160, prot_level:4, l1:11, l2:23, l3:83, l4:3, pi1:11, pi2:6, pi3:5, pi4:9},
    ProtProfile{bit_rate:160, prot_level:3, l1:11, l2:24, l3:82, l4:3, pi1:16, pi2:8, pi3:6, pi4:11},
    ProtProfile{bit_rate:160, prot_level:2, l1:11, l2:21, l3:85, l4:3, pi1:22, pi2:11, pi3:9, pi4:13},
    ProtProfile{bit_rate:160, prot_level:1, l1:11, l2:22, l3:84, l4:3, pi1:24, pi2:18, pi3:12, pi4:19},
    ProtProfile{bit_rate:192, prot_level:5, l1:11, l2:20, l3:110, l4:3, pi1:6, pi2:4, pi3:2, pi4:5},
    ProtProfile{bit_rate:192, prot_level:4, l1:11, l2:22, l3:108, l4:3, pi1:10, pi2:6, pi3:4, pi4:9},
    ProtProfile{bit_rate:192, prot_level:3, l1:11, l2:24, l3:106, l4:3, pi1:16, pi2:10, pi3:6, pi4:11},
    ProtProfile{bit_rate:192, prot_level:2, l1:11, l2:20, l3:110, l4:3, pi1:22, pi2:13, pi3:9, pi4:13},
    ProtProfile{bit_rate:192, prot_level:1, l1:11, l2:21, l3:109, l4:3, pi1:24, pi2:20, pi3:13, pi4:24},
    ProtProfile{bit_rate:224, prot_level:5, l1:12, l2:22, l3:131, l4:3, pi1:8, pi2:6, pi3:2, pi4:6},
    ProtProfile{bit_rate:224, prot_level:4, l1:12, l2:26, l3:127, l4:3, pi1:12, pi2:8, pi3:4, pi4:11},
    ProtProfile{bit_rate:224, prot_level:3, l1:11, l2:20, l3:134, l4:3, pi1:16, pi2:10, pi3:7, pi4:9},
    ProtProfile{bit_rate:224, prot_level:2, l1:11, l2:22, l3:132, l4:3, pi1:24, pi2:16, pi3:10, pi4:15},
    ProtProfile{bit_rate:224, prot_level:1, l1:11, l2:24, l3:130, l4:3, pi1:24, pi2:20, pi3:12, pi4:20},
    ProtProfile{bit_rate:256, prot_level:5, l1:11, l2:24, l3:154, l4:3, pi1:6, pi2:5, pi3:2, pi4:5},
    ProtProfile{bit_rate:256, prot_level:4, l1:11, l2:24, l3:154, l4:3, pi1:12, pi2:9, pi3:5, pi4:10},
    ProtProfile{bit_rate:256, prot_level:3, l1:11, l2:27, l3:151, l4:3, pi1:16, pi2:10, pi3:7, pi4:10},
    ProtProfile{bit_rate:256, prot_level:2, l1:11, l2:22, l3:156, l4:3, pi1:24, pi2:14, pi3:10, pi4:13},
    ProtProfile{bit_rate:256, prot_level:1, l1:11, l2:26, l3:152, l4:3, pi1:24, pi2:19, pi3:14, pi4:18},
    ProtProfile{bit_rate:320, prot_level:5, l1:11, l2:26, l3:200, l4:3, pi1:8, pi2:5, pi3:2, pi4:6},
    ProtProfile{bit_rate:320, prot_level:4, l1:11, l2:25, l3:201, l4:3, pi1:13, pi2:9, pi3:5, pi4:10},
    ProtProfile{bit_rate:320, prot_level:2, l1:11, l2:26, l3:200, l4:3, pi1:24, pi2:17, pi3:9, pi4:17},
    ProtProfile{bit_rate:384, prot_level:5, l1:11, l2:27, l3:247, l4:3, pi1:8, pi2:6, pi3:2, pi4:7},
    ProtProfile{bit_rate:384, prot_level:3, l1:11, l2:24, l3:250, l4:3, pi1:16, pi2:9, pi3:7, pi4:10},
    ProtProfile{bit_rate:384, prot_level:1, l1:12, l2:28, l3:245, l4:3, pi1:24, pi2:20, pi3:14, pi4:23},
];

fn find_profile(bit_rate: i16, prot_level: i16) -> Option<usize> {
    PROFILE_TABLE.iter().position(|p| p.bit_rate == bit_rate && p.prot_level == prot_level)
}

pub struct UepProtection {
    out_size: usize,
    index_table: Vec<bool>,
    viterbi_block: Vec<i16>,
    viterbi: ViterbiSpiral,
}

impl UepProtection {
    pub fn new(bit_rate: i16, prot_level: i16) -> Self {
        let out_size = 24 * bit_rate as usize;
        let index = find_profile(bit_rate, prot_level).unwrap_or(1);
        let p = &PROFILE_TABLE[index];

        let mut index_table = vec![false; out_size * 4 + 24];
        let mut vc = 0usize;

        let pi1 = get_pcodes((p.pi1 - 1) as usize);
        let pi2 = get_pcodes((p.pi2 - 1) as usize);
        let pi3 = get_pcodes((p.pi3 - 1) as usize);
        let pi4 = if p.pi4 > 0 { Some(get_pcodes((p.pi4 - 1) as usize)) } else { None };
        let pi_x = get_pcodes(7);

        for _ in 0..p.l1 { for j in 0..128 { if pi1[j % 32] != 0 { index_table[vc] = true; } vc += 1; } }
        for _ in 0..p.l2 { for j in 0..128 { if pi2[j % 32] != 0 { index_table[vc] = true; } vc += 1; } }
        for _ in 0..p.l3 { for j in 0..128 { if pi3[j % 32] != 0 { index_table[vc] = true; } vc += 1; } }
        if let Some(pi4) = pi4 {
            for _ in 0..p.l4 { for j in 0..128 { if pi4[j % 32] != 0 { index_table[vc] = true; } vc += 1; } }
        }
        for i in 0..24 { if pi_x[i] != 0 { index_table[vc] = true; } vc += 1; }

        UepProtection {
            out_size,
            index_table,
            viterbi_block: vec![0i16; out_size * 4 + 24],
            viterbi: ViterbiSpiral::new(out_size),
        }
    }

    pub fn deconvolve(&mut self, v: &[i16], out_buffer: &mut [u8]) {
        self.viterbi_block.fill(0);
        let mut input_counter = 0;
        for i in 0..(self.out_size * 4 + 24) {
            if self.index_table[i] {
                if input_counter < v.len() {
                    self.viterbi_block[i] = v[input_counter];
                    input_counter += 1;
                }
            }
        }
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
