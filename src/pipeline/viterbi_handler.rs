// DABstar-aligned Viterbi spiral decoder.
//
// Rate 1/4, constraint length K=7 convolutional decoder used for both FIC and MSC.
// Polynomials: {0155, 0117, 0123, 0155} (octal).
// ETSI EN 300 401 §11.1 — convolutional coding (FIC); §13.2 (MSC EEP/UEP).

const K: usize = 7;
const RATE: usize = 4;
const NUM_STATES: usize = 1 << (K - 1); // 64
const POLYS: [i32; RATE] = [0o155, 0o117, 0o123, 0o155];
/// DABstar seeds all non-zero paths with a large initial metric and only
/// renormalises when survivors have drifted far enough upward.
const INITIAL_PATH_METRIC: u32 = 1_000;
const RENORMALIZE_THRESHOLD: u32 = 30_000;
/// Bias applied when mapping signed soft bits (−127..127) to unsigned symbols (0..254).
const SOFT_DECISION_BIAS: i32 = 127;
/// Neutral soft symbol used when input is truncated (0-confidence midpoint).
const NEUTRAL_SOFT_SYMBOL: u8 = SOFT_DECISION_BIAS as u8;
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

#[inline]
fn map_soft_decision_to_symbol(inp: i16) -> u8 {
    (inp as i32 + SOFT_DECISION_BIAS).clamp(0, 255) as u8
}

#[inline]
fn required_soft_input_len(frame_bits: usize) -> usize {
    // Rate 1/4 trellis with K-1 tail bits: RATE * (N + K - 1).
    RATE * (frame_bits + K - 1)
}

#[inline]
fn decision_words_per_step() -> usize {
    NUM_STATES.div_ceil(32)
}

#[inline]
fn branch_table_index(rate_idx: usize, state: usize) -> usize {
    rate_idx * (NUM_STATES / 2) + state
}

#[inline]
fn compute_branch_metric(branchtab: &[u8], symbols: &[u8], sym_base: usize, state: usize) -> u32 {
    let mut metric = 0u32;
    for rate_idx in 0..RATE {
        metric +=
            (branchtab[branch_table_index(rate_idx, state)] ^ symbols[sym_base + rate_idx]) as u32;
    }
    metric
}

#[inline]
fn store_survivor_pair(decisions: &mut [u32], dec_offset: usize, state: usize, d0: u32, d1: u32) {
    let word_idx = state / 16;
    let bit_pos = (2 * state) % 32;
    decisions[dec_offset + word_idx] |= (d0 | (d1 << 1)) << bit_pos;
}

#[inline]
fn traceback_decision_bit(decisions: &[u32], dec_offset: usize, endstate: u32) -> u32 {
    let bit_idx = (endstate >> ADD_SHIFT) as usize;
    let word_idx = bit_idx / 32;
    let bit_pos = bit_idx % 32;
    decisions
        .get(dec_offset + word_idx)
        .map_or(0, |word| (word >> bit_pos) & 1)
}

#[inline]
fn should_renormalize(min_metric: u32) -> bool {
    min_metric > RENORMALIZE_THRESHOLD
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
        // DABstar/Karn spiral layout: the branch table is grouped by coder rate,
        // so the scalar butterfly reads Branchtab[i], Branchtab[32+i], ...
        for state in 0..NUM_STATES / 2 {
            for rate_idx in 0..RATE {
                branchtab[branch_table_index(rate_idx, state)] = if (POLYS[rate_idx] < 0) as u8
                    ^ parity((2 * state as i32) & POLYS[rate_idx].abs()) as u8
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
        self.metrics1.fill(INITIAL_PATH_METRIC);
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
        if should_renormalize(min) {
            for m in metrics.iter_mut() {
                *m -= min;
            }
        }
    }

    fn update_viterbi(&mut self) {
        let nbits = self.frame_bits + K - 1;
        let words_per_decision = decision_words_per_step();

        // Decision bits are already zeroed by init_viterbi; re-zero here to make
        // update_viterbi safe even if called without a preceding init_viterbi.
        self.decisions.fill(0);

        let max_branch_metric = RATE as u32 * 255;
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
                // DABstar scalar butterfly: metric = Branchtab[i] ^ sym0 +
                // Branchtab[32+i] ^ sym1 + Branchtab[64+i] ^ sym2 + Branchtab[96+i] ^ sym3.
                let metric = compute_branch_metric(branchtab, symbols, sym_base, i);
                let complement_metric = max_branch_metric - metric;

                let m0 = old[i].wrapping_add(metric);
                let m1 = old[i + NUM_STATES / 2].wrapping_add(complement_metric);
                let m2 = old[i].wrapping_add(complement_metric);
                let m3 = old[i + NUM_STATES / 2].wrapping_add(metric);

                // Branchless select (helps auto-vectorization)
                let d0 = (m0 > m1) as u32;
                let d1 = (m2 > m3) as u32;

                // Equivalent to: new[2*i] = if d0 { m1 } else { m0 }
                // Using branchless: min(m0, m1) when d0 means m0 > m1
                new[2 * i] = m0 ^ ((m0 ^ m1) & (0u32.wrapping_sub(d0)));
                new[2 * i + 1] = m2 ^ ((m2 ^ m3) & (0u32.wrapping_sub(d1)));

                // Pack decisions in the same two-word-per-step shape used by
                // DABstar's `decision_t` storage.
                store_survivor_pair(decisions, dec_offset, i, d0, d1);
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
        let words_per_decision = decision_words_per_step();

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
            let k = traceback_decision_bit(&self.decisions, dec_offset, endstate);
            endstate = (endstate >> 1) | (k << (K as u32 - 2 + ADD_SHIFT as u32));
            self.data[nbits_remaining as usize >> 3] = (endstate >> SUB_SHIFT) as u8;
        }
    }

    /// Deconvolve soft bits (input: signed i16 with -127..127 mapping)
    /// into hard bits (output: 0/1 bytes)
    pub fn deconvolve(&mut self, input: &[i16], output: &mut [u8]) {
        self.init_viterbi();
        let total = required_soft_input_len(self.frame_bits);
        // Reset the active symbol window so truncated inputs do not reuse stale
        // soft bits from previous calls.
        self.symbols[..total].fill(NEUTRAL_SOFT_SYMBOL);
        for (i, &inp) in input.iter().enumerate().take(total.min(input.len())) {
            self.symbols[i] = map_soft_decision_to_symbol(inp);
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

    /// Compare a decoded hard-bit sequence against the original soft symbols and
    /// puncturing pattern, returning `(checked_bits, bit_errors)`.
    ///
    /// This mirrors DABstar's BER helper and is useful when instrumenting live
    /// FIC/MSC quality without changing the decoder path itself.
    pub fn calculate_ber(
        &self,
        input: &[i16],
        puncture_table: &[bool],
        output: &[u8],
    ) -> (usize, usize) {
        let mut bits = 0usize;
        let mut errors = 0usize;
        let mut shift_reg = 0usize;
        let nbits = self.frame_bits.min(output.len());

        for (i, bit) in output.iter().copied().enumerate().take(nbits) {
            shift_reg = ((shift_reg << 1) | (bit as usize & 1)) & 0xff;
            for (j, poly) in POLYS.iter().enumerate() {
                let idx = i * RATE + j;
                if idx >= input.len() || idx >= puncture_table.len() || !puncture_table[idx] {
                    continue;
                }
                bits += 1;
                let expected = parity(shift_reg as i32 & *poly) != 0;
                let observed = input[idx] > 0;
                if observed != expected {
                    errors += 1;
                }
            }
        }

        for i in nbits..(nbits + K - 1) {
            shift_reg = (shift_reg << 1) & 0xff;
            for (j, poly) in POLYS.iter().enumerate() {
                let idx = i * RATE + j;
                if idx >= input.len() || idx >= puncture_table.len() || !puncture_table[idx] {
                    continue;
                }
                bits += 1;
                let expected = parity(shift_reg as i32 & *poly) != 0;
                let observed = input[idx] > 0;
                if observed != expected {
                    errors += 1;
                }
            }
        }

        (bits, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_soft_decision_bounds_are_clamped() {
        assert_eq!(map_soft_decision_to_symbol(i16::MIN), 0);
        assert_eq!(map_soft_decision_to_symbol(i16::MAX), 255);
    }

    #[test]
    fn map_soft_decision_nominal_range_matches_expected_bias() {
        // ETSI EN 300 401 soft bits are nominally in [-127, +127].
        assert_eq!(map_soft_decision_to_symbol(-127), 0);
        assert_eq!(map_soft_decision_to_symbol(0), 127);
        assert_eq!(map_soft_decision_to_symbol(127), 254);
    }

    #[test]
    fn required_soft_input_len_matches_formula() {
        assert_eq!(required_soft_input_len(0), RATE * (K - 1));
        assert_eq!(required_soft_input_len(32), 152);
        assert_eq!(required_soft_input_len(64), 280);
    }

    #[test]
    fn required_soft_input_len_is_monotonic() {
        assert!(required_soft_input_len(33) > required_soft_input_len(32));
        assert!(required_soft_input_len(128) > required_soft_input_len(64));
    }

    #[test]
    fn decision_words_per_step_matches_state_geometry() {
        assert_eq!(NUM_STATES, 64);
        assert_eq!(decision_words_per_step(), 2);
    }

    #[test]
    fn decision_storage_size_matches_formula() {
        let nbits = 32usize;
        let words = (nbits + K - 1) * decision_words_per_step();
        let v = ViterbiSpiral::new(nbits);
        assert_eq!(v.decisions.len(), words);
    }

    #[test]
    fn branch_table_index_is_rate_major_like_dabstar() {
        assert_eq!(branch_table_index(0, 5), 5);
        assert_eq!(branch_table_index(1, 5), 32 + 5);
        assert_eq!(branch_table_index(2, 5), 64 + 5);
        assert_eq!(branch_table_index(3, 5), 96 + 5);
    }

    #[test]
    fn traceback_decision_bit_reads_packed_survivor_state() {
        let mut decisions = vec![0u32; decision_words_per_step()];
        store_survivor_pair(&mut decisions, 0, 17, 1, 0);

        let endstate_for_even_branch = ((17usize * 2) as u32) << ADD_SHIFT;
        let endstate_for_odd_branch = ((17usize * 2 + 1) as u32) << ADD_SHIFT;

        assert_eq!(
            traceback_decision_bit(&decisions, 0, endstate_for_even_branch),
            1
        );
        assert_eq!(
            traceback_decision_bit(&decisions, 0, endstate_for_odd_branch),
            0
        );
    }

    #[test]
    fn renormalize_guard_matches_threshold_rule() {
        assert!(!should_renormalize(RENORMALIZE_THRESHOLD));
        assert!(should_renormalize(RENORMALIZE_THRESHOLD + 1));
    }

    #[test]
    fn renormalize_noop_when_min_is_below_or_equal_threshold() {
        let mut metrics = vec![RENORMALIZE_THRESHOLD, RENORMALIZE_THRESHOLD + 5, 9];
        let before = metrics.clone();
        ViterbiSpiral::renormalize(&mut metrics);
        assert_eq!(metrics, before);
    }

    #[test]
    fn renormalize_subtracts_min_when_above_threshold() {
        let mut metrics = vec![
            RENORMALIZE_THRESHOLD + 2,
            RENORMALIZE_THRESHOLD + 12,
            RENORMALIZE_THRESHOLD + 65,
        ];
        ViterbiSpiral::renormalize(&mut metrics);
        assert_eq!(metrics[0], 0);
        assert_eq!(metrics[1], 10);
        assert_eq!(metrics[2], 63);
    }

    #[test]
    fn new_does_not_panic() {
        let _v = ViterbiSpiral::new(768);
    }

    #[test]
    fn zero_input_produces_zero_output() {
        let mut v = ViterbiSpiral::new(32);
        let input = vec![0i16; required_soft_input_len(32)];
        let mut output = vec![0u8; 32];
        v.deconvolve(&input, &mut output);
        assert!(output.iter().all(|&b| b == 0));
    }

    #[test]
    fn output_is_binary() {
        let mut v = ViterbiSpiral::new(64);
        let input: Vec<i16> = (0..required_soft_input_len(64))
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
            let input = vec![0i16; required_soft_input_len(wl)];
            let mut output = vec![0u8; wl];
            v.deconvolve(&input, &mut output);
        }
    }

    #[test]
    fn strong_encoded_ones() {
        let mut v = ViterbiSpiral::new(32);
        let input = vec![127i16; required_soft_input_len(32)];
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

    #[test]
    fn truncated_input_does_not_reuse_previous_symbols() {
        let nbits = 32usize;
        let total_symbols = required_soft_input_len(nbits);

        let mut reused = ViterbiSpiral::new(nbits);
        let mut fresh = ViterbiSpiral::new(nbits);

        // First run with a non-neutral pattern to contaminate internal symbol state
        // in implementations that forget to clear it.
        let strong = vec![127i16; total_symbols];
        let mut sink = vec![0u8; nbits];
        reused.deconvolve(&strong, &mut sink);

        // Then run with a truncated input and compare with a fresh decoder.
        let truncated = vec![0i16; total_symbols / 3];
        let mut out_reused = vec![0u8; nbits];
        let mut out_fresh = vec![0u8; nbits];
        reused.deconvolve(&truncated, &mut out_reused);
        fresh.deconvolve(&truncated, &mut out_fresh);

        assert_eq!(
            out_reused, out_fresh,
            "short input output must not depend on previous decode calls"
        );
    }

    /// Test-only rate-1/4 K=7 convolutional encoder.
    ///
    /// State convention: `new_state = (old_state << 1 | input_bit) & (NUM_STATES − 1)`.
    /// This is consistent with the butterfly formula `(2 * state + input) & (NUM_STATES − 1)`
    /// used to build `branchtab`.
    #[test]
    fn calculate_ber_reports_zero_errors_on_clean_codeword() {
        let original = vec![0u8; 32];
        let soft = test_encode(&original);
        let puncture = vec![true; soft.len()];

        let mut v = ViterbiSpiral::new(32);
        let mut decoded = vec![0u8; 32];
        v.deconvolve(&soft, &mut decoded);

        let (bits, errors) = v.calculate_ber(&soft, &puncture, &decoded);
        assert!(bits > 0);
        assert_eq!(errors, 0);
    }

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
