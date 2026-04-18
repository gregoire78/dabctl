const K: usize = 7;
const RATE: usize = 4;
const NUMSTATES: usize = 64;
const POLYS: [u8; RATE] = [109, 79, 83, 109];
const INIT_METRIC: i32 = 1_000;

fn parity(mut x: u8) -> u8 {
    x ^= x >> 4;
    x ^= x >> 2;
    x ^= x >> 1;
    x & 1
}

pub fn build_prbs_bits(len: usize) -> Vec<u8> {
    let mut shift_register = [1u8; 9];
    let mut prbs = vec![0u8; len];
    for bit in &mut prbs {
        *bit = shift_register[8] ^ shift_register[4];
        for idx in (1..9).rev() {
            shift_register[idx] = shift_register[idx - 1];
        }
        shift_register[0] = *bit;
    }
    prbs
}

/// Diagnostic info from Viterbi decode.
#[derive(Debug)]
pub struct ViterbiDiag {
    /// Metric at state 0 (expected termination state).
    pub metric_state0: i32,
    /// Minimum metric across all 64 states.
    pub metric_min: i32,
    /// State with the minimum metric.
    pub best_state: usize,
}

pub fn viterbi_decode_rate_1_4(input: &[i16], frame_bits: usize) -> Vec<u8> {
    let (output, _diag) = viterbi_decode_rate_1_4_diag(input, frame_bits);
    output
}

pub fn viterbi_decode_rate_1_4_diag(input: &[i16], frame_bits: usize) -> (Vec<u8>, ViterbiDiag) {
    let nbits = frame_bits + (K - 1);
    let branchtab = build_branchtable();
    let mut decisions = vec![0u64; nbits];
    let mut old_metrics = [INIT_METRIC; NUMSTATES];
    let mut new_metrics = [0i32; NUMSTATES];

    old_metrics[0] = 0;

    for (step, decision_word) in decisions.iter_mut().enumerate() {
        let sym_base = step * RATE;
        let sym0 = i32::from(quantize(*input.get(sym_base).unwrap_or(&0)));
        let sym1 = i32::from(quantize(*input.get(sym_base + 1).unwrap_or(&0)));
        let sym2 = i32::from(quantize(*input.get(sym_base + 2).unwrap_or(&0)));
        let sym3 = i32::from(quantize(*input.get(sym_base + 3).unwrap_or(&0)));

        let mut word = 0u64;
        for i in 0..(NUMSTATES / 2) {
            let metric = ((branchtab[i] as i32) ^ sym0)
                + ((branchtab[32 + i] as i32) ^ sym1)
                + ((branchtab[64 + i] as i32) ^ sym2)
                + ((branchtab[96 + i] as i32) ^ sym3);
            let m_metric = 1020 - metric;

            let m0 = old_metrics[i] + metric;
            let m1 = old_metrics[i + 32] + m_metric;
            let m2 = old_metrics[i] + m_metric;
            let m3 = old_metrics[i + 32] + metric;

            let decision0 = (m0 - m1) > 0;
            let decision1 = (m2 - m3) > 0;

            new_metrics[2 * i] = if decision0 { m1 } else { m0 };
            new_metrics[2 * i + 1] = if decision1 { m3 } else { m2 };
            word |= ((decision0 as u64) | ((decision1 as u64) << 1)) << (i * 2);
        }

        *decision_word = word;
        std::mem::swap(&mut old_metrics, &mut new_metrics);
    }

    // Diagnostic: collect final metrics
    let metric_state0 = old_metrics[0];
    let (best_state, metric_min) = old_metrics
        .iter()
        .enumerate()
        .min_by_key(|(_, m)| **m)
        .map(|(s, m)| (s, *m))
        .unwrap_or((0, 0));

    let mut output = vec![0u8; frame_bits];
    let mut endstate = 0usize;
    for framebit in (0..frame_bits).rev() {
        let decision_word = decisions[framebit + (K - 1)];
        let bit_index = endstate >> 2;
        let k = ((decision_word >> bit_index) & 1) as u8;
        endstate = (endstate >> 1) | (usize::from(k) << K);
        output[framebit] = k;
    }

    let diag = ViterbiDiag {
        metric_state0,
        metric_min,
        best_state,
    };

    (output, diag)
}

fn build_branchtable() -> [u8; RATE * NUMSTATES / 2] {
    let mut branchtab = [0u8; RATE * NUMSTATES / 2];
    for state in 0..(NUMSTATES / 2) {
        let shifted = ((state as u8) << 1) & 0x7F;
        for (poly_idx, poly) in POLYS.iter().copied().enumerate() {
            branchtab[poly_idx * (NUMSTATES / 2) + state] =
                if parity(shifted & poly) != 0 { 255 } else { 0 };
        }
    }
    branchtab
}

fn quantize(value: i16) -> u8 {
    let shifted = i32::from(value) + 127;
    shifted.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::viterbi_decode_rate_1_4;

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

    #[test]
    fn roundtrips_rate_one_over_four_bits() {
        let bits = vec![1, 0, 1, 1, 0, 0, 1, 0, 1, 0, 1, 1, 1, 0, 0, 1];
        let encoded = encode_rate_1_4(&bits);
        let decoded = viterbi_decode_rate_1_4(&encoded, bits.len());
        assert_eq!(decoded, bits);
    }

    #[test]
    fn withstands_moderate_symbol_noise() {
        let bits = vec![
            1, 0, 1, 1, 0, 0, 1, 0, 1, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1,
            0, 1, 0,
        ];
        let mut encoded = encode_rate_1_4(&bits);
        for idx in [3usize, 9, 14, 33, 47, 62, 71, 88, 97, 115] {
            if let Some(sym) = encoded.get_mut(idx) {
                *sym = -*sym / 2;
            }
        }
        let decoded = viterbi_decode_rate_1_4(&encoded, bits.len());
        assert_eq!(decoded, bits);
    }

    /// Simulate the MSC EEP-A depuncturing path: encode at rate 1/4,
    /// puncture (keep only positions marked 1 in PI pattern), then
    /// depuncture (insert 0 at erased positions) and decode.
    #[test]
    fn roundtrips_with_rate_half_depuncturing() {
        // PI_8 pattern: rate 1/2 (keep 2 out of 4)
        let pi: [u8; 32] = [
            1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1,
            1, 0, 0,
        ];

        let bits: Vec<u8> = (0..128).map(|i| ((i * 7 + 3) % 2) as u8).collect();
        let encoded = encode_rate_1_4(&bits);

        // Puncture: keep only positions where pi pattern = 1
        let mut punctured = Vec::new();
        for (i, sym) in encoded.iter().copied().enumerate() {
            if pi[i % 32] != 0 {
                punctured.push(sym);
            }
        }

        // Depuncture: rebuild viterbi block, erased positions = 0
        let viterbi_len = encoded.len();
        let mut viterbi_block = vec![0i16; viterbi_len];
        let mut src = 0;
        for (i, slot) in viterbi_block.iter_mut().enumerate() {
            if pi[i % 32] != 0 {
                *slot = punctured[src];
                src += 1;
            }
        }

        let decoded = viterbi_decode_rate_1_4(&viterbi_block, bits.len());
        assert_eq!(decoded, bits, "rate-1/2 depunctured roundtrip failed");
    }
}
