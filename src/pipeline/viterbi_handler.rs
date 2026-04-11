// Viterbi decoder — converted from viterbi-spiral.cpp (eti-cmdline)
//
// Rate 1/4, constraint length K=7 convolutional decoder used for both FIC and MSC.
// Polynomials: {0155, 0117, 0123, 0155} (octal).
// ETSI EN 300 401 §11.1 — convolutional coding (FIC); §13.2 (MSC EEP/UEP).

const K: usize = 7;
const RATE: usize = 4;
const NUM_STATES: usize = 1 << (K - 1); // 64
const POLYS: [i32; RATE] = [0o155, 0o117, 0o123, 0o155];
const RENORMALIZE_THRESHOLD: u32 = 137;
/// Bias applied when mapping signed soft bits (−127..127) to unsigned symbols (0..254).
const SOFT_DECISION_BIAS: i32 = 127;
/// Chainback shift constants derived from K=7 and 8-bit byte width.
const ADD_SHIFT: usize = 8_usize.saturating_sub(K - 1); // 2
const SUB_SHIFT: usize = (K - 1).saturating_sub(8); // 0

static PARTAB: [u8; 256] = [
    0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1,
    1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0,
    1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0,
    0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1,
    1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0,
    0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1,
    0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1,
    1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0,
];

fn parity(mut x: i32) -> i32 {
    x ^= x >> 16;
    x ^= x >> 8;
    PARTAB[(x & 0xff) as usize] as i32
}

pub struct ViterbiSpiral {
    frame_bits: usize,
    branchtab: Vec<u8>,
    symbols: Vec<u8>,
    data: Vec<u8>,
    // Metric storage
    metrics1: Vec<u32>,
    metrics2: Vec<u32>,
    // Decision storage
    decisions: Vec<u32>,
}

impl ViterbiSpiral {
    pub fn new(word_length: usize) -> Self {
        let mut branchtab = vec![0u8; RATE * NUM_STATES / 2];
        // Layout: [state * RATE + rate_idx] — sequential per-state access for better cache usage.
        for state in 0..NUM_STATES / 2 {
            for i in 0..RATE {
                branchtab[state * RATE + i] = if (POLYS[i] < 0) as u8
                    ^ parity((2 * state as i32) & POLYS[i].abs()) as u8
                    != 0
                {
                    255
                } else {
                    0
                };
            }
        }

        let total_symbols = RATE * (word_length + K - 1);
        let decision_words = (word_length + K - 1) * NUM_STATES.div_ceil(32);

        ViterbiSpiral {
            frame_bits: word_length,
            branchtab,
            symbols: vec![0u8; total_symbols],
            data: vec![0u8; (word_length + K - 1) / 8 + 1],
            metrics1: vec![0u32; NUM_STATES],
            metrics2: vec![0u32; NUM_STATES],
            decisions: vec![0u32; decision_words],
        }
    }

    fn init_viterbi(&mut self) {
        self.metrics1.fill(63);
        self.metrics1[0] = 0;
        // Also reset metrics2: on the first trellis step it is the write buffer,
        // but all 64 entries are overwritten before being read, so this is defensive.
        self.metrics2.fill(0);
        self.decisions.fill(0);
    }

    fn renormalize(metrics: &mut [u32]) {
        // ETSI EN 300 401 §11.1: prevent metric overflow by subtracting the minimum
        // survivor metric when it exceeds the normalisation threshold.
        // Using the true minimum (not metrics[0]) avoids false negatives when state 0
        // is not on the best path.
        let min = *metrics.iter().min().unwrap(); // slice is always NUM_STATES long
        if min > RENORMALIZE_THRESHOLD {
            for m in metrics.iter_mut() {
                *m -= min;
            }
        }
    }

    fn update_viterbi(&mut self) {
        let nbits = self.frame_bits + K - 1;
        let words_per_decision = NUM_STATES.div_ceil(32);

        // Decision bits are already zeroed by init_viterbi; re-zero here to make
        // update_viterbi safe even if called without a preceding init_viterbi.
        self.decisions.fill(0);

        let max = RATE as u32 * 255;
        let mut use_metrics1_as_old = true;
        for s in 0..nbits {
            let sym_base = s * RATE;

            // Destructure to split borrows on disjoint fields
            let Self {
                metrics1,
                metrics2,
                branchtab,
                symbols,
                decisions,
                ..
            } = self;
            let (old, new): (&[u32], &mut [u32]) = if use_metrics1_as_old {
                (metrics1.as_slice(), metrics2.as_mut_slice())
            } else {
                (metrics2.as_slice(), metrics1.as_mut_slice())
            };

            let dec_offset = s * words_per_decision;

            // Batch butterfly: process all NUM_STATES/2 states
            // Written as a tight loop for auto-vectorization
            for i in 0..NUM_STATES / 2 {
                // Compute branch metric — sequential access: branchtab[i*RATE .. i*RATE+RATE]
                let mut metric: u32 = 0;
                for j in 0..RATE {
                    metric += (branchtab[i * RATE + j] ^ symbols[sym_base + j]) as u32;
                }

                let m0 = old[i].wrapping_add(metric);
                let m1 = old[i + NUM_STATES / 2].wrapping_add(max - metric);
                let m2 = old[i].wrapping_add(max - metric);
                let m3 = old[i + NUM_STATES / 2].wrapping_add(metric);

                // Branchless select (helps auto-vectorization)
                let d0 = (m0 > m1) as u32;
                let d1 = (m2 > m3) as u32;

                // Equivalent to: new[2*i] = if d0 { m1 } else { m0 }
                // Using branchless: min(m0, m1) when d0 means m0 > m1
                new[2 * i] = m0 ^ ((m0 ^ m1) & (0u32.wrapping_sub(d0)));
                new[2 * i + 1] = m2 ^ ((m2 ^ m3) & (0u32.wrapping_sub(d1)));

                // Pack decisions
                let word_idx = i / 16;
                let bit_pos = (2 * i) % 32;
                decisions[dec_offset + word_idx] |= (d0 | (d1 << 1)) << bit_pos;
            }

            // Renormalize
            if use_metrics1_as_old {
                Self::renormalize(&mut self.metrics2);
            } else {
                Self::renormalize(&mut self.metrics1);
            }
            use_metrics1_as_old = !use_metrics1_as_old;
        }
    }

    fn chainback(&mut self) {
        let nbits = self.frame_bits;
        let words_per_decision = NUM_STATES.div_ceil(32);

        self.data.fill(0);

        // The tail bits force the encoder to end in state 0, so we start traceback
        // at endstate = 0 and skip the K-1 tail trellis steps.
        let mut endstate: u32 = 0;
        let d_offset = K - 1;
        let mut nbits_remaining = nbits as i32;
        while nbits_remaining > 0 {
            nbits_remaining -= 1;
            let s = d_offset + nbits_remaining as usize;
            let dec_offset = s * words_per_decision;
            let bit_idx = (endstate >> ADD_SHIFT) as usize;
            let word_idx = bit_idx / 32;
            let bit_pos = bit_idx % 32;
            let k = if dec_offset + word_idx < self.decisions.len() {
                (self.decisions[dec_offset + word_idx] >> bit_pos) & 1
            } else {
                0
            };
            endstate = (endstate >> 1) | (k << (K as u32 - 2 + ADD_SHIFT as u32));
            self.data[nbits_remaining as usize >> 3] = (endstate >> SUB_SHIFT) as u8;
        }
    }

    /// Deconvolve soft bits (input: signed i16 with -127..127 mapping)
    /// into hard bits (output: 0/1 bytes)
    pub fn deconvolve(&mut self, input: &[i16], output: &mut [u8]) {
        self.init_viterbi();
        let total = (self.frame_bits + K - 1) * RATE;
        for (i, &inp) in input.iter().enumerate().take(total.min(input.len())) {
            let temp = (inp as i32 + SOFT_DECISION_BIAS).clamp(0, 255);
            self.symbols[i] = temp as u8;
        }

        self.update_viterbi();
        self.chainback();

        // Extract bits from packed bytes
        for (i, out) in output.iter_mut().enumerate().take(self.frame_bits) {
            let byte_idx = i >> 3;
            let bit_pos = 7 - (i & 7);
            *out = (self.data[byte_idx] >> bit_pos) & 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_does_not_panic() {
        let _v = ViterbiSpiral::new(768);
    }

    #[test]
    fn zero_input_produces_zero_output() {
        let mut v = ViterbiSpiral::new(32);
        let input = vec![0i16; 152];
        let mut output = vec![0u8; 32];
        v.deconvolve(&input, &mut output);
        assert!(output.iter().all(|&b| b == 0));
    }

    #[test]
    fn output_is_binary() {
        let mut v = ViterbiSpiral::new(64);
        let input: Vec<i16> = (0..280)
            .map(|i| if i % 3 == 0 { 127 } else { -127 })
            .collect();
        let mut output = vec![0u8; 64];
        v.deconvolve(&input, &mut output);
        assert!(output.iter().all(|&b| b == 0 || b == 1));
    }

    #[test]
    fn different_word_lengths() {
        for wl in [16, 32, 64, 128, 768] {
            let mut v = ViterbiSpiral::new(wl);
            let input = vec![0i16; (wl + 6) * 4];
            let mut output = vec![0u8; wl];
            v.deconvolve(&input, &mut output);
        }
    }

    #[test]
    fn strong_encoded_ones() {
        let mut v = ViterbiSpiral::new(32);
        let input = vec![127i16; 152];
        let mut output = vec![0u8; 32];
        v.deconvolve(&input, &mut output);
        assert!(output.iter().all(|&b| b == 0 || b == 1));
    }

    #[test]
    fn round_trip_all_zeros() {
        // All-zero input: encoder state stays at 0, all parity bits = 0 (soft = -127).
        // ETSI EN 300 401 §11.1
        let nbits = 32usize;
        let original = vec![0u8; nbits];
        let soft = test_encode(&original);
        assert!(
            soft.iter().all(|&s| s == -127),
            "all-zero encoder must produce all -127"
        );

        let mut v = ViterbiSpiral::new(nbits);
        let mut decoded = vec![0u8; nbits];
        v.deconvolve(&soft, &mut decoded);
        assert_eq!(decoded, original, "decoded bits must match original");
    }

    #[test]
    fn round_trip_single_one() {
        // A single 1-bit at position 0, rest zeros.  The path metric for the correct
        // sequence is strictly lower than all competing paths, so the decoder is
        // unambiguous.  ETSI EN 300 401 §11.1.
        let nbits = 32usize;
        let mut original = vec![0u8; nbits];
        original[0] = 1;
        let soft = test_encode(&original);

        let mut v = ViterbiSpiral::new(nbits);
        let mut decoded = vec![0u8; nbits];
        v.deconvolve(&soft, &mut decoded);

        assert_eq!(decoded, original, "decoded bits must match original");
    }

    #[test]
    fn decode_is_stable_across_repeated_calls() {
        // Verify that re-using a ViterbiSpiral instance across multiple calls
        // produces consistent results (metrics2 reset bug guard).
        let nbits = 32usize;
        let original = vec![0u8; nbits];
        let soft = test_encode(&original);

        let mut v = ViterbiSpiral::new(nbits);
        let mut decoded = vec![0u8; nbits];
        for _ in 0..3 {
            v.deconvolve(&soft, &mut decoded);
            assert_eq!(decoded, original, "result must be stable on repeated calls");
        }
    }

    /// Test-only rate-1/4 K=7 convolutional encoder.
    ///
    /// State convention: `new_state = (old_state << 1 | input_bit) & (NUM_STATES − 1)`.
    /// This is consistent with the butterfly formula `(2 * state + input) & (NUM_STATES − 1)`
    /// used to build `branchtab`.
    fn test_encode(bits: &[u8]) -> Vec<i16> {
        let total = bits.len() + K - 1; // data bits + K-1 zero tail bits to flush the register
        let mut out = Vec::with_capacity(total * RATE);
        let mut state: usize = 0;

        for &bit in bits.iter().chain(std::iter::repeat(&0u8).take(K - 1)) {
            state = ((state << 1) | bit as usize) & (NUM_STATES - 1);
            for &poly in POLYS.iter() {
                let p = parity(state as i32 & poly);
                out.push(if p == 0 { -127i16 } else { 127i16 });
            }
        }
        out
    }
}
