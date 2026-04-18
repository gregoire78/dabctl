use anyhow::{bail, Result};

use crate::decoder::viterbi::{
    build_prbs_bits, viterbi_decode_rate_1_4, viterbi_decode_rate_1_4_diag,
};

#[derive(Debug, Clone)]
pub struct BackendDeconvolver {
    bit_rate: u16,
    viterbi_block_len: usize,
    viterbi_mapping: Vec<usize>,
    prbs: Vec<u8>,
}

impl BackendDeconvolver {
    pub fn new(bit_rate: u16, short_form: bool, prot_level: i16) -> Result<Self> {
        let frame_bits = 24usize * usize::from(bit_rate);
        let viterbi_block_len = frame_bits * 4 + 24;
        let viterbi_mapping = if short_form {
            build_uep_mapping(bit_rate, prot_level)?
        } else {
            build_eep_mapping(bit_rate, prot_level)?
        };
        let prbs = build_prbs_bits(frame_bits);

        Ok(Self {
            bit_rate,
            viterbi_block_len,
            viterbi_mapping,
            prbs,
        })
    }

    pub fn deconvolve(&self, raw_bits: &[i16]) -> Result<Vec<i8>> {
        if raw_bits.len() < self.viterbi_mapping.len() {
            bail!(
                "need {} MSC soft bits for bitrate {} but got {}",
                self.viterbi_mapping.len(),
                self.bit_rate,
                raw_bits.len()
            );
        }

        let mut viterbi_block = vec![0i16; self.viterbi_block_len];
        for (soft, dst_idx) in raw_bits.iter().zip(self.viterbi_mapping.iter().copied()) {
            viterbi_block[dst_idx] = *soft;
        }

        // Temporary diagnostic: dump input stats and viterbi block
        {
            static DIAG: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = DIAG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 3 {
                let input_abs_mean =
                    raw_bits.iter().map(|x| (*x as f32).abs()).sum::<f32>() / raw_bits.len() as f32;
                let nonzero_vit = viterbi_block.iter().filter(|&&x| x != 0).count();
                tracing::debug!(
                    n,
                    input_len = raw_bits.len(),
                    input_abs_mean,
                    vit_block_len = self.viterbi_block_len,
                    mapping_len = self.viterbi_mapping.len(),
                    nonzero_vit,
                    first_input_16 = ?&raw_bits[..16.min(raw_bits.len())],
                    first_vit_32 = ?&viterbi_block[..32.min(viterbi_block.len())],
                    "MSC deconvolve input"
                );
            }
        }

        let decoded = viterbi_decode_rate_1_4(&viterbi_block, 24usize * usize::from(self.bit_rate));

        // Temporary diagnostic: check Viterbi output + metrics + BER
        {
            static DIAG2: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = DIAG2.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 100 {
                let ones = decoded.iter().filter(|&&b| b != 0).count();
                let ratio = ones as f32 / decoded.len() as f32;
                // Also run the diagnostic version to get metrics
                let (_dec2, diag) = viterbi_decode_rate_1_4_diag(
                    &viterbi_block,
                    24usize * usize::from(self.bit_rate),
                );

                // BER estimate: re-encode decoded bits, compare with non-erased input
                let polys: [u8; 4] = [109, 79, 83, 109];
                let frame_bits = 24usize * usize::from(self.bit_rate);
                let mut sr = 0u8;
                let mut ber_bits = 0u32;
                let mut ber_errors = 0u32;
                for (i, input_bit) in decoded
                    .iter()
                    .copied()
                    .chain(std::iter::repeat(0))
                    .take(frame_bits + 6)
                    .enumerate()
                {
                    sr = ((sr << 1) | (input_bit & 1)) & 0x7F;
                    for (j, poly) in polys.iter().enumerate() {
                        let mut x = sr & poly;
                        x ^= x >> 4;
                        x ^= x >> 2;
                        x ^= x >> 1;
                        let encoded_bit = x & 1;
                        let vit_pos = i * 4 + j;
                        if vit_pos < viterbi_block.len() && viterbi_block[vit_pos] != 0 {
                            // Non-erased position: compare
                            ber_bits += 1;
                            let received_bit = if viterbi_block[vit_pos] > 0 { 1u8 } else { 0u8 };
                            if received_bit != encoded_bit {
                                ber_errors += 1;
                            }
                        }
                    }
                }
                let ber = if ber_bits > 0 {
                    ber_errors as f32 / ber_bits as f32
                } else {
                    -1.0
                };

                tracing::debug!(
                    n,
                    ones_ratio = ratio,
                    decoded_len = decoded.len(),
                    metric_state0 = diag.metric_state0,
                    metric_min = diag.metric_min,
                    best_state = diag.best_state,
                    ber,
                    ber_bits,
                    ber_errors,
                    first_32 = ?&decoded[..32.min(decoded.len())],
                    "MSC Viterbi output"
                );
            }
        }

        let mut out = Vec::with_capacity(decoded.len());
        for (bit, prbs) in decoded.into_iter().zip(self.prbs.iter().copied()) {
            let descrambled = bit ^ prbs;
            out.push(if descrambled != 0 { 127 } else { -127 });
        }
        Ok(out)
    }
}

fn build_eep_mapping(bit_rate: u16, prot_level: i16) -> Result<Vec<usize>> {
    let bit_rate_i16 = bit_rate as i16;
    let prot = prot_level & 0x3;
    let option = (prot_level & (1 << 2)) >> 2;

    let (l1, l2, pi1, pi2) = if option == 0 {
        let n = bit_rate_i16 / 8;
        match prot {
            0 => (6 * n - 3, 3, 24, 23),
            1 => {
                if n == 1 {
                    (5, 1, 13, 12)
                } else {
                    (2 * n - 3, 4 * n + 3, 14, 13)
                }
            }
            2 => (6 * n - 3, 3, 8, 7),
            3 => (4 * n - 3, 2 * n + 3, 3, 2),
            _ => unreachable!(),
        }
    } else if option == 1 {
        let n = bit_rate_i16 / 32;
        let pi1 = match prot {
            0 => 10,
            1 => 6,
            2 => 4,
            3 => 2,
            _ => unreachable!(),
        };
        let pi2 = match prot {
            0 => 9,
            1 => 5,
            2 => 3,
            3 => 1,
            _ => unreachable!(),
        };
        (24 * n - 3, 3, pi1, pi2)
    } else {
        bail!("unsupported EEP option {}", option);
    };

    let mut counter = 0usize;
    let mut mapping = Vec::new();
    extract_viterbi_mapping(&mut counter, l1, get_pi_codes(pi1), &mut mapping);
    extract_viterbi_mapping(&mut counter, l2, get_pi_codes(pi2), &mut mapping);

    let pi_x = get_pi_codes(8);
    for present in pi_x.iter().take(24) {
        if *present != 0 {
            mapping.push(counter);
        }
        counter += 1;
    }

    Ok(mapping)
}

fn build_uep_mapping(bit_rate: u16, prot_level: i16) -> Result<Vec<usize>> {
    let profile = UEP_TABLE
        .iter()
        .find(|profile| profile.bit_rate == bit_rate as i16 && profile.prot_level == prot_level)
        .ok_or_else(|| {
            anyhow::anyhow!("unsupported UEP bitrate/protection {bit_rate}/{prot_level}")
        })?;

    let mut counter = 0usize;
    let mut mapping = Vec::new();
    extract_viterbi_mapping(
        &mut counter,
        profile.l1,
        get_pi_codes(profile.pi1),
        &mut mapping,
    );
    extract_viterbi_mapping(
        &mut counter,
        profile.l2,
        get_pi_codes(profile.pi2),
        &mut mapping,
    );
    extract_viterbi_mapping(
        &mut counter,
        profile.l3,
        get_pi_codes(profile.pi3),
        &mut mapping,
    );
    if profile.l4 > 0 && profile.pi4 > 0 {
        extract_viterbi_mapping(
            &mut counter,
            profile.l4,
            get_pi_codes(profile.pi4),
            &mut mapping,
        );
    }
    let pi_x = get_pi_codes(8);
    for present in pi_x.iter().take(24) {
        if *present != 0 {
            mapping.push(counter);
        }
        counter += 1;
    }
    Ok(mapping)
}

fn extract_viterbi_mapping(counter: &mut usize, lx: i16, pi: &[u8; 32], out: &mut Vec<usize>) {
    for _ in 0..lx {
        for j in 0..128usize {
            if pi[j % 32] != 0 {
                out.push(*counter);
            }
            *counter += 1;
        }
    }
}

fn get_pi_codes(pi_code: i16) -> &'static [u8; 32] {
    &PI_CODES[(pi_code - 1) as usize]
}

#[derive(Clone, Copy)]
struct UepProfile {
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

const PI_CODES: [[u8; 32]; 24] = [
    [
        1, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0,
        0, 0,
    ],
    [
        1, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0,
        0, 0,
    ],
    [
        1, 1, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0,
        0, 0,
    ],
    [
        1, 1, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0,
        0, 0,
    ],
    [
        1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0,
        0, 0,
    ],
    [
        1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0,
        0, 0,
    ],
    [
        1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0,
        0, 0,
    ],
    [
        1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1,
        0, 0,
    ],
    [
        1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1,
        0, 0,
    ],
    [
        1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1,
        0, 0,
    ],
    [
        1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1,
        0, 0,
    ],
    [
        1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1,
        0, 0,
    ],
    [
        1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1,
        0, 0,
    ],
    [
        1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1,
        0, 0,
    ],
    [
        1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1,
        0, 0,
    ],
    [
        1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1,
        1, 0,
    ],
    [
        1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1,
        1, 0,
    ],
    [
        1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1,
        1, 0,
    ],
    [
        1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1,
        1, 0,
    ],
    [
        1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1,
        1, 0,
    ],
    [
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1,
        1, 0,
    ],
    [
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 0,
    ],
    [
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 0,
    ],
    [
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1,
    ],
];

const UEP_TABLE: &[UepProfile] = &[
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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
    UepProfile {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode `bits` with the rate 1/4 convolutional code (K=7, same polys as Viterbi decoder).
    fn encode_rate_1_4(bits: &[u8]) -> Vec<i16> {
        const POLYS: [u8; 4] = [109, 79, 83, 109];
        let mut sr = 0u8;
        let mut out = Vec::with_capacity((bits.len() + 6) * 4);
        for bit in bits.iter().copied().chain(std::iter::repeat_n(0, 6)) {
            sr = ((sr << 1) | (bit & 1)) & 0x7F;
            for poly in POLYS {
                let mut x = sr & poly;
                x ^= x >> 4;
                x ^= x >> 2;
                x ^= x >> 1;
                let parity = x & 1;
                out.push(if parity != 0 { 127 } else { -127 });
            }
        }
        out
    }

    /// Apply puncturing (keep only positions in the mapping) to encoded symbols.
    fn puncture(encoded: &[i16], mapping: &[usize]) -> Vec<i16> {
        let mut punctured = vec![0i16; mapping.len()];
        for (out_idx, &src_pos) in mapping.iter().enumerate() {
            punctured[out_idx] = encoded.get(src_pos).copied().unwrap_or(0);
        }
        punctured
    }

    /// Apply PRBS energy dispersal to raw bits before encoding.
    fn scramble(bits: &mut [u8], prbs: &[u8]) {
        for (bit, p) in bits.iter_mut().zip(prbs.iter()) {
            *bit ^= p;
        }
    }

    #[test]
    fn eep_a_level2_bitrate88_roundtrips() {
        // Exact parameters from the live DAB+ service: bit_rate=88, prot_level=2, short_form=false
        let deconv = BackendDeconvolver::new(88, false, 2).unwrap();
        let frame_bits = 24 * 88; // 2112

        // Create a known bit sequence (simulate a DAB+ frame)
        let mut original_bits: Vec<u8> = (0..frame_bits).map(|i| ((i * 7 + 3) % 2) as u8).collect();

        // Apply PRBS scrambling (energy dispersal) as the transmitter would
        let prbs = build_prbs_bits(frame_bits);
        scramble(&mut original_bits, &prbs);

        // Encode with rate 1/4 convolutional code
        let encoded = encode_rate_1_4(&original_bits);
        assert_eq!(encoded.len(), (frame_bits + 6) * 4); // 2118 * 4 = 8472

        // Puncture according to EEP-A level 2 mapping
        let punctured = puncture(&encoded, &deconv.viterbi_mapping);
        assert_eq!(punctured.len(), deconv.viterbi_mapping.len());

        // Decode using the deconvolver (depuncture + Viterbi + descramble)
        let decoded = deconv.deconvolve(&punctured).unwrap();

        // Convert decoded soft bits back to hard bits
        let decoded_bits: Vec<u8> = decoded.iter().map(|&x| if x > 0 { 1 } else { 0 }).collect();

        // The descrambled output should match the ORIGINAL unscrambled bits
        let expected_bits: Vec<u8> = (0..frame_bits).map(|i| ((i * 7 + 3) % 2) as u8).collect();
        // Deconvolver internally does XOR with PRBS, so output = viterbi_output ^ prbs
        // viterbi_output should be the scrambled bits, so output = scrambled ^ prbs = original

        let mismatches: usize = decoded_bits
            .iter()
            .zip(expected_bits.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            mismatches, 0,
            "EEP-A deconvolver roundtrip failed with {} bit errors out of {}",
            mismatches, frame_bits
        );
    }
}
