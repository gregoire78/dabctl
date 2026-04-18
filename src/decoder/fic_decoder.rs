use crate::decoder::viterbi::{build_prbs_bits, viterbi_decode_rate_1_4};

const FIB_BITS: usize = 256;
const FIB_BYTES: usize = 32;
const FIC_SIZE_VIT_IN: usize = 2304;
const FIC_SIZE_VIT_OUT: usize = 768;
const VITERBI_BLOCK_SIZE: usize = 3072 + 24;

// Literal FIC/FIB staging: depuncture -> Viterbi -> descramble -> CRC-checked FIBs.
pub struct FicDecoder {
    puncture_table: Vec<bool>,
    prbs: Vec<u8>,
    fic_viterbi_soft_input: [i16; FIC_SIZE_VIT_IN],
    index: usize,
    /// DABstar's recent FIC quality estimate: a saturating up/down counter
    /// in the range 0..=10, reported as percent ×10.
    fic_decode_success_ratio: usize,
    valid_fibs: usize,
    total_fibs: usize,
}

impl Default for FicDecoder {
    fn default() -> Self {
        let mut puncture_table = vec![false; VITERBI_BLOCK_SIZE];
        let mut local = 0usize;

        for _ in 0..21 {
            for k in 0..128usize {
                if PI_16[k % 32] != 0 {
                    puncture_table[local] = true;
                }
                local += 1;
            }
        }
        for _ in 0..3 {
            for k in 0..128usize {
                if PI_15[k % 32] != 0 {
                    puncture_table[local] = true;
                }
                local += 1;
            }
        }
        for present in PI_8.iter().take(24usize) {
            if *present != 0 {
                puncture_table[local] = true;
            }
            local += 1;
        }

        Self {
            puncture_table,
            prbs: build_prbs_bits(FIC_SIZE_VIT_OUT),
            fic_viterbi_soft_input: [0; FIC_SIZE_VIT_IN],
            index: 0,
            fic_decode_success_ratio: 0,
            valid_fibs: 0,
            total_fibs: 0,
        }
    }
}

impl FicDecoder {
    /// Mirror DABstar's `process_block(iOfdmSymbIdx==1)` reset: called at the
    /// start of each DAB frame when symbol 1 (first FIC symbol) arrives.
    pub fn reset_frame(&mut self) {
        self.index = 0;
    }

    pub fn push_soft_bits(&mut self, soft_bits: &[i16]) -> Vec<[u8; FIB_BYTES]> {
        let mut out = Vec::new();

        for soft_bit in soft_bits {
            if self.index < FIC_SIZE_VIT_IN {
                self.fic_viterbi_soft_input[self.index] = *soft_bit;
                self.index += 1;
            }

            if self.index >= FIC_SIZE_VIT_IN {
                out.extend(self.process_fic_input());
                self.index = 0;
            }
        }

        out
    }

    pub fn decode_ratio_percent(&self) -> usize {
        self.fic_decode_success_ratio * 10
    }

    fn update_decode_success_ratio(&mut self, fib_valid: bool) {
        if fib_valid {
            if self.fic_decode_success_ratio < 10 {
                self.fic_decode_success_ratio += 1;
            }
        } else if self.fic_decode_success_ratio > 0 {
            self.fic_decode_success_ratio -= 1;
        }
    }

    fn process_fic_input(&mut self) -> Vec<[u8; FIB_BYTES]> {
        let mut viterbi_block = vec![0i16; VITERBI_BLOCK_SIZE];
        let mut fic_read_idx = 0usize;

        for (idx, punctured) in self.puncture_table.iter().copied().enumerate() {
            if punctured {
                viterbi_block[idx] = self.fic_viterbi_soft_input[fic_read_idx];
                fic_read_idx += 1;
            }
        }

        // DIAG_FIC: compute abs_mean of input soft bits
        let abs_sum: i64 = self.fic_viterbi_soft_input[..FIC_SIZE_VIT_IN]
            .iter()
            .map(|v| (*v as i64).abs())
            .sum();
        let fic_abs_mean = abs_sum / FIC_SIZE_VIT_IN as i64;

        let mut decoded_bits = viterbi_decode_rate_1_4(&viterbi_block, FIC_SIZE_VIT_OUT);

        // DIAG_FIC: compute BER by re-encoding and comparing with non-erased positions
        {
            const FIC_POLYS: [u8; 4] = [109, 79, 83, 109];
            // Re-encode the decoded bits (before PRBS descramble) — but decoded_bits
            // is pre-PRBS at this point
            let mut ber_errors = 0usize;
            let mut ber_total = 0usize;
            // We need decoded_bits BEFORE PRBS. At this point decoded_bits is already
            // the Viterbi output (before PRBS undo). So we re-encode these bits.
            let num_data_bits = FIC_SIZE_VIT_OUT;
            let mut shift_reg: u8 = 0;
            for (bit_idx, bit) in decoded_bits.iter().take(num_data_bits).enumerate() {
                shift_reg = ((shift_reg << 1) | (*bit & 1)) & 0x7F;
                for (poly_idx, poly) in FIC_POLYS.iter().enumerate() {
                    let vit_pos = bit_idx * 4 + poly_idx;
                    if vit_pos < VITERBI_BLOCK_SIZE {
                        let expected_parity = (shift_reg & poly).count_ones() as u8 & 1;
                        let soft_val = viterbi_block[vit_pos];
                        if soft_val != 0 {
                            ber_total += 1;
                            let received_bit = if soft_val > 0 { 1u8 } else { 0u8 };
                            if received_bit != expected_parity {
                                ber_errors += 1;
                            }
                        }
                    }
                }
            }
            if self.total_fibs < 30 {
                tracing::info!(
                    fic_abs_mean,
                    ber_errors,
                    ber_total,
                    ber_pct = if ber_total > 0 {
                        100 * ber_errors / ber_total
                    } else {
                        0
                    },
                    "DIAG_FIC: FIC BER estimate"
                );
            }
        }

        for (bit, prbs) in decoded_bits.iter_mut().zip(self.prbs.iter().copied()) {
            *bit ^= prbs;
        }

        let mut fibs = Vec::new();
        for fib_idx in 0..3usize {
            let start = fib_idx * FIB_BITS;
            let end = start + FIB_BITS;
            self.total_fibs += 1;
            let fib_valid = check_crc_bits(&decoded_bits[start..end]);
            self.update_decode_success_ratio(fib_valid);
            if fib_valid {
                self.valid_fibs += 1;
                fibs.push(bits_to_fib(&decoded_bits[start..end]));
            }
        }

        if self.total_fibs <= 30 {
            tracing::info!(
                valid = self.valid_fibs,
                total = self.total_fibs,
                ratio_pct = self.decode_ratio_percent(),
                "DIAG_FIC: FIB CRC stats"
            );
        }

        fibs
    }
}

fn bits_to_fib(bits: &[u8]) -> [u8; FIB_BYTES] {
    let mut fib = [0u8; FIB_BYTES];
    for (byte_idx, byte) in fib.iter_mut().enumerate() {
        let mut value = 0u8;
        for bit_idx in 0..8 {
            value = (value << 1) | bits[byte_idx * 8 + bit_idx];
        }
        *byte = value;
    }
    fib
}

fn check_crc_bits(bits: &[u8]) -> bool {
    const CRC_POLYNOME: [u8; 15] = [0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];

    let mut reg = [1u8; 16];
    for (idx, bit) in bits.iter().copied().enumerate() {
        let inv_bit = if idx >= bits.len().saturating_sub(16) {
            1u8
        } else {
            0u8
        };

        if (reg[0] ^ (bit ^ inv_bit)) == 1 {
            for tap in 0..15 {
                reg[tap] = CRC_POLYNOME[tap] ^ reg[tap + 1];
            }
            reg[15] = 1;
        } else {
            reg.copy_within(1..16, 0);
            reg[15] = 0;
        }
    }

    reg.iter().all(|&bit| bit == 0)
}

const PI_8: [u8; 32] = [
    1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0,
];
const PI_15: [u8; 32] = [
    1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0,
];
const PI_16: [u8; 32] = [
    1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0,
];

#[cfg(test)]
mod tests {
    use super::FicDecoder;

    #[test]
    fn dabstar_fic_quality_counter_is_recent_and_saturating() {
        let mut decoder = FicDecoder::default();

        for _ in 0..12 {
            decoder.update_decode_success_ratio(true);
        }
        assert_eq!(decoder.decode_ratio_percent(), 100);

        decoder.update_decode_success_ratio(false);
        assert_eq!(decoder.decode_ratio_percent(), 90);

        for _ in 0..20 {
            decoder.update_decode_success_ratio(false);
        }
        assert_eq!(decoder.decode_ratio_percent(), 0);
    }
}
