// Viterbi decoder - converted from viterbi-spiral.cpp (eti-cmdline)
// Copyright (C) 2020 Jan van Katwijk - Lazy Chair Computing
//
// This implements the "spiral" Viterbi decoder used for both FIC and MSC.
// Polynomials: {0155, 0117, 0123, 0155} (octal, same as C++)

const K: usize = 7;
const RATE: usize = 4;
const NUM_STATES: usize = 1 << (K - 1); // 64
const POLYS: [i32; RATE] = [0o155, 0o117, 0o123, 0o155];
const RENORMALIZE_THRESHOLD: u32 = 137;

static PARTAB: [u8; 256] = [
    0,1,1,0,1,0,0,1,1,0,0,1,0,1,1,0, 1,0,0,1,0,1,1,0,0,1,1,0,1,0,0,1,
    1,0,0,1,0,1,1,0,0,1,1,0,1,0,0,1, 0,1,1,0,1,0,0,1,1,0,0,1,0,1,1,0,
    1,0,0,1,0,1,1,0,0,1,1,0,1,0,0,1, 0,1,1,0,1,0,0,1,1,0,0,1,0,1,1,0,
    0,1,1,0,1,0,0,1,1,0,0,1,0,1,1,0, 1,0,0,1,0,1,1,0,0,1,1,0,1,0,0,1,
    1,0,0,1,0,1,1,0,0,1,1,0,1,0,0,1, 0,1,1,0,1,0,0,1,1,0,0,1,0,1,1,0,
    0,1,1,0,1,0,0,1,1,0,0,1,0,1,1,0, 1,0,0,1,0,1,1,0,0,1,1,0,1,0,0,1,
    0,1,1,0,1,0,0,1,1,0,0,1,0,1,1,0, 1,0,0,1,0,1,1,0,0,1,1,0,1,0,0,1,
    1,0,0,1,0,1,1,0,0,1,1,0,1,0,0,1, 0,1,1,0,1,0,0,1,1,0,0,1,0,1,1,0,
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
        for state in 0..NUM_STATES / 2 {
            for i in 0..RATE {
                branchtab[i * NUM_STATES / 2 + state] =
                    if (POLYS[i] < 0) as u8 ^ parity((2 * state as i32) & POLYS[i].abs()) as u8 != 0 {
                        255
                    } else {
                        0
                    };
            }
        }

        let total_symbols = RATE * (word_length + K - 1);
        let decision_words = (word_length + K - 1) * ((NUM_STATES + 31) / 32);

        ViterbiSpiral {
            frame_bits: word_length,
            branchtab,
            symbols: vec![0u8; total_symbols],
            data: vec![0u8; (word_length + K - 1) / 8 + 1],
            metrics1: vec![0u32; NUM_STATES],
            metrics2: vec![0u32; NUM_STATES],
            decisions: vec![0u32; decision_words * 2],
        }
    }

    fn init_viterbi(&mut self) {
        for i in 0..NUM_STATES {
            self.metrics1[i] = 63;
        }
        self.metrics1[0] = 0;
        for d in self.decisions.iter_mut() {
            *d = 0;
        }
    }

    fn butterfly(branchtab: &[u8], i: usize, s: usize, syms: &[u8],
                 old_metrics: &[u32], new_metrics: &mut [u32],
                 dec: &mut [u32]) {
        let mut metric: u32 = 0;
        for j in 0..RATE {
            metric += ((branchtab[j * NUM_STATES / 2 + i] ^ syms[s * RATE + j]) as u32) >> 0;
        }
        let max = RATE as u32 * 255;

        let m0 = old_metrics[i] + metric;
        let m1 = old_metrics[i + NUM_STATES / 2] + (max - metric);
        let m2 = old_metrics[i] + (max - metric);
        let m3 = old_metrics[i + NUM_STATES / 2] + metric;

        let decision0 = if m0 > m1 { 1u32 } else { 0 };
        let decision1 = if m2 > m3 { 1u32 } else { 0 };

        new_metrics[2 * i] = if decision0 != 0 { m1 } else { m0 };
        new_metrics[2 * i + 1] = if decision1 != 0 { m3 } else { m2 };

        let words_per_decision = (NUM_STATES + 31) / 32;
        let dec_offset = s * words_per_decision;
        let word_idx = i / 16;
        let bit_pos = (2 * i) % 32;
        if dec_offset + word_idx < dec.len() {
            dec[dec_offset + word_idx] |= (decision0 | (decision1 << 1)) << bit_pos;
        }
    }

    fn renormalize(metrics: &mut [u32]) {
        if metrics[0] > RENORMALIZE_THRESHOLD {
            let min = *metrics.iter().min().unwrap();
            for m in metrics.iter_mut() {
                *m -= min;
            }
        }
    }

    fn update_viterbi(&mut self) {
        let nbits = self.frame_bits + K - 1;
        let words_per_decision = (NUM_STATES + 31) / 32;

        // Clear decisions
        for i in 0..nbits * words_per_decision {
            if i < self.decisions.len() {
                self.decisions[i] = 0;
            }
        }

        let mut use_metrics1_as_old = true;
        for s in 0..nbits {
            if use_metrics1_as_old {
                let (old, new) = (&self.metrics1.clone(), &mut self.metrics2);
                for i in 0..NUM_STATES / 2 {
                    Self::butterfly(&self.branchtab, i, s, &self.symbols, old, new, &mut self.decisions);
                }
                Self::renormalize(&mut self.metrics2);
            } else {
                let (old, new) = (&self.metrics2.clone(), &mut self.metrics1);
                for i in 0..NUM_STATES / 2 {
                    Self::butterfly(&self.branchtab, i, s, &self.symbols, old, new, &mut self.decisions);
                }
                Self::renormalize(&mut self.metrics1);
            }
            use_metrics1_as_old = !use_metrics1_as_old;
        }
    }

    fn chainback(&mut self) {
        let nbits = self.frame_bits;
        let words_per_decision = (NUM_STATES + 31) / 32;
        let add_shift = if K - 1 < 8 { 8 - (K - 1) } else { 0 };
        let sub_shift = if K - 1 > 8 { (K - 1) - 8 } else { 0 };

        let mut endstate: u32 = 0;
        endstate = (endstate % NUM_STATES as u32) << add_shift;

        for i in 0..self.data.len() {
            self.data[i] = 0;
        }

        // Look past tail
        let d_offset = K - 1;
        let mut nbits_remaining = nbits as i32;
        while nbits_remaining > 0 {
            nbits_remaining -= 1;
            let s = d_offset + nbits_remaining as usize;
            let dec_offset = s * words_per_decision;
            let bit_idx = (endstate >> add_shift) as usize;
            let word_idx = bit_idx / 32;
            let bit_pos = bit_idx % 32;
            let k = if dec_offset + word_idx < self.decisions.len() {
                (self.decisions[dec_offset + word_idx] >> bit_pos) & 1
            } else {
                0
            };
            endstate = (endstate >> 1) | (k << (K as u32 - 2 + add_shift as u32));
            self.data[nbits_remaining as usize >> 3] = (endstate >> sub_shift) as u8;
        }
    }

    /// Deconvolve soft bits (input: signed i16 with -127..127 mapping)
    /// into hard bits (output: 0/1 bytes)
    pub fn deconvolve(&mut self, input: &[i16], output: &mut [u8]) {
        self.init_viterbi();
        let total = (self.frame_bits + K - 1) * RATE;
        for i in 0..total.min(input.len()) {
            let temp = (input[i] as i32 + 127).clamp(0, 255);
            self.symbols[i] = temp as u8;
        }

        self.update_viterbi();
        self.chainback();

        // Extract bits from packed bytes
        for i in 0..self.frame_bits {
            let byte_idx = i >> 3;
            let bit_pos = 7 - (i & 7);
            output[i] = (self.data[byte_idx] >> bit_pos) & 1;
        }
    }
}
