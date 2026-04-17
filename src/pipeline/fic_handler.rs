// FIC handler - converted from fic-handler.cpp (eti-cmdline)

use crate::pipeline::dab_constants::{check_crc_bits, ChannelData};
use crate::pipeline::dab_params::DabParams;
use crate::pipeline::fib_processor::FibProcessor;
use crate::pipeline::prot_tables::get_pcodes;
use crate::pipeline::viterbi_handler::ViterbiSpiral;

/// Size of the depunctured Viterbi input buffer for FIC decoding.
/// = 24 puncture groups × 128 bits/group + 24 tail bits (ETSI EN 300 401 §11.1)
const FIC_VITERBI_BUF_SIZE: usize = 3072 + 24;
const FIC_BITS_OUT_SIZE: usize = 768;
const FICS_PER_FRAME: usize = 4;
const FIBS_PER_FIC: usize = 3;
/// DABstar keeps a bounded 0..10 success ratio rather than trusting the last
/// frame's raw 0/33/66/100 percent directly.
const DABSTAR_FIC_RATIO_MAX: i16 = 10;

#[inline]
fn update_decode_ratio_step(current_steps: i16, crc_ok: bool) -> i16 {
    let delta = if crc_ok { 1 } else { -1 };
    (current_steps + delta).clamp(0, DABSTAR_FIC_RATIO_MAX)
}

fn update_decode_ratio_steps(mut current_steps: i16, success: i16, total: i16) -> i16 {
    for _ in 0..success.max(0) as usize {
        current_steps = update_decode_ratio_step(current_steps, true);
    }
    for _ in 0..total.saturating_sub(success) as usize {
        current_steps = update_decode_ratio_step(current_steps, false);
    }
    current_steps
}

pub struct FicHandler {
    bits_per_block: usize,
    ofdm_input: Vec<i16>,
    ofdm_input_index: usize,
    fic_decode_index: usize,
    puncture_table: Vec<bool>,
    prbs: [u8; FIC_BITS_OUT_SIZE],
    viterbi: ViterbiSpiral,
    /// Pre-allocated depuncturing buffer reused across FIC blocks.
    viterbi_block: Vec<i16>,
    bit_buffer_out: Vec<u8>,
    fib_bits_frame: Vec<u8>,
    fic_valid: [bool; FICS_PER_FRAME],
    is_running: bool,
    pub fib_processor: FibProcessor,
    fic_errors: i32,
    fic_success: i32,
    fic_decode_ratio_steps: i16,
}

impl FicHandler {
    pub fn new(params: &DabParams) -> Self {
        let bits_per_block = 2 * params.k as usize;

        // Generate PRBS sequence
        let mut prbs = [0u8; 768];
        let mut shift_reg = [1u8; 9];
        for item in &mut prbs {
            *item = shift_reg[8] ^ shift_reg[4];
            let b = *item;
            for j in (1..9).rev() {
                shift_reg[j] = shift_reg[j - 1];
            }
            shift_reg[0] = b;
        }

        // Build puncture table for FIC
        let mut puncture_table = vec![false; 3072 + 24];
        let mut local = 0;

        // First 21 blocks with PI_16
        for _ in 0..21 {
            for k in 0..128 {
                if get_pcodes(15)[k % 32] != 0 {
                    puncture_table[local] = true;
                }
                local += 1;
            }
        }

        // Next 3 blocks with PI_15
        for _ in 0..3 {
            for k in 0..128 {
                if get_pcodes(14)[k % 32] != 0 {
                    puncture_table[local] = true;
                }
                local += 1;
            }
        }

        // Final 24 bits with PI_X (using PI_8)
        for k in 0..24 {
            if get_pcodes(7)[k] != 0 {
                puncture_table[local] = true;
            }
            local += 1;
        }

        FicHandler {
            bits_per_block,
            ofdm_input: vec![0i16; 2304],
            ofdm_input_index: 0,
            fic_decode_index: 0,
            puncture_table,
            prbs,
            viterbi: ViterbiSpiral::new(FIC_BITS_OUT_SIZE),
            viterbi_block: vec![0i16; FIC_VITERBI_BUF_SIZE],
            bit_buffer_out: vec![0u8; FIC_BITS_OUT_SIZE],
            fib_bits_frame: vec![0u8; FIC_BITS_OUT_SIZE * FICS_PER_FRAME],
            fic_valid: [false; FICS_PER_FRAME],
            is_running: true,
            fib_processor: FibProcessor::new(),
            fic_errors: 0,
            fic_success: 0,
            fic_decode_ratio_steps: 0,
        }
    }

    fn reset_frame_collection(&mut self) {
        self.ofdm_input_index = 0;
        self.fic_decode_index = 0;
        self.fic_valid.fill(false);
        self.fib_bits_frame.fill(0);
    }

    fn collect_block_soft_bits(&mut self, block: &[i16], out: &mut [u8], valid: &mut [bool]) {
        for &soft_bit in block {
            self.ofdm_input[self.ofdm_input_index] = soft_bit;
            self.ofdm_input_index += 1;

            if self.ofdm_input_index >= self.ofdm_input.len() {
                let ficno = self.fic_decode_index;
                let block_valid = self.process_fic_input(ficno, out);
                if ficno < self.fic_valid.len() {
                    self.fic_valid[ficno] = block_valid;
                }
                if ficno < valid.len() {
                    valid[ficno] = block_valid;
                }
                self.ofdm_input_index = 0;
                self.fic_decode_index += 1;
            }
        }
    }

    fn depuncture_fic_bits(&mut self) {
        self.viterbi_block.fill(0);
        let mut input_count = 0usize;
        for (i, vb) in self.viterbi_block.iter_mut().enumerate() {
            if self.puncture_table[i] {
                *vb = self.ofdm_input[input_count];
                input_count += 1;
            }
        }
    }

    fn descramble_fic_bits(&mut self) {
        for i in 0..FIC_BITS_OUT_SIZE {
            self.bit_buffer_out[i] ^= self.prbs[i];
        }
    }

    /// Process the three FIC OFDM blocks for one DAB frame in the same staged
    /// collect → depuncture → Viterbi → descramble flow as DABstar.
    pub fn process_block(&mut self, data: &[i16], out: &mut [u8], valid: &mut [bool]) {
        if !self.is_running {
            return;
        }

        self.reset_frame_collection();
        for i in 0..FIBS_PER_FIC {
            let start = i * self.bits_per_block;
            let end = start + self.bits_per_block;
            self.collect_block_soft_bits(&data[start..end], out, valid);
        }
    }

    pub fn process_fic_block(&mut self, data: &[i16], out: &mut [u8], valid: &mut [bool]) {
        self.process_block(data, out, valid);
    }

    fn process_fic_input(&mut self, ficno: usize, fib_bytes: &mut [u8]) -> bool {
        if !self.is_running {
            return false;
        }

        self.depuncture_fic_bits();
        self.viterbi
            .deconvolve(&self.viterbi_block, &mut self.bit_buffer_out);
        self.descramble_fic_bits();

        let mut valid = true;
        let mut frame_success = 0i16;
        for fib_idx in 0..FIBS_PER_FIC {
            let p = &self.bit_buffer_out[fib_idx * 256..(fib_idx + 1) * 256];
            let crc_ok = check_crc_bits(p, 256);
            if !crc_ok {
                valid = false;
                self.fic_errors += 1;
                continue;
            }
            self.fic_success += 1;
            frame_success += 1;
            self.fib_processor.process_fib(p, ficno as u16);
        }
        self.fic_decode_ratio_steps = update_decode_ratio_steps(
            self.fic_decode_ratio_steps,
            frame_success,
            FIBS_PER_FIC as i16,
        );

        let offset = ficno * FIC_BITS_OUT_SIZE;
        if offset + FIC_BITS_OUT_SIZE <= fib_bytes.len() {
            fib_bytes[offset..offset + FIC_BITS_OUT_SIZE].copy_from_slice(&self.bit_buffer_out);
        }
        if offset + FIC_BITS_OUT_SIZE <= self.fib_bits_frame.len() {
            self.fib_bits_frame[offset..offset + FIC_BITS_OUT_SIZE]
                .copy_from_slice(&self.bit_buffer_out);
        }

        valid
    }

    pub fn stop(&mut self) {
        self.is_running = false;
        self.fib_processor.clear_ensemble();
    }

    pub fn restart(&mut self) {
        self.fic_decode_ratio_steps = 0;
        self.reset_quality_counters();
        self.reset_frame_collection();
        self.is_running = true;
    }

    pub fn get_fib_bits(&self, v: &mut [u8], b: &mut [bool]) {
        for (dst, src) in v.iter_mut().zip(self.fib_bits_frame.iter()) {
            *dst = *src;
        }
        for (dst, src) in b.iter_mut().zip(self.fic_valid.iter()) {
            *dst = *src;
        }
    }

    pub fn get_channel_info(&self, n: usize) -> ChannelData {
        self.fib_processor.get_channel_info(n)
    }

    pub fn get_cif_count(&self) -> (i16, i16) {
        self.fib_processor.get_cif_count()
    }

    /// Reset per-frame FIC quality counters.
    ///
    /// Call this at the start of each FIC frame (OFDM block 2) so that
    /// `get_fic_quality()` reflects only the most recent frame rather than
    /// an ever-growing historical average.
    pub fn reset_quality_counters(&mut self) {
        self.fic_errors = 0;
        self.fic_success = 0;
    }

    /// Returns `(success, total)` FIB CRC counts for the current frame window.
    ///
    /// Prefer this over `get_fic_quality()` when accumulating counts over a
    /// longer reporting window (e.g. the 1-second status log), so the quality
    /// ratio can be computed from the summed counts rather than averaged
    /// per-frame percentages.
    pub fn get_fic_counts(&self) -> (i16, i16) {
        (
            self.fic_success as i16,
            (self.fic_success + self.fic_errors) as i16,
        )
    }

    pub fn get_fic_quality(&self) -> i16 {
        if self.fic_errors + self.fic_success > 0 {
            (self.fic_success * 100 / (self.fic_errors + self.fic_success)) as i16
        } else {
            0
        }
    }

    /// DABstar-style smoothed decode ratio in percent (0, 10, …, 100).
    pub fn get_fic_decode_ratio_percent(&self) -> i16 {
        self.fic_decode_ratio_steps * 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::dab_params::DabParams;

    #[test]
    fn creation_mode1() {
        let params = DabParams::new(1);
        let fh = FicHandler::new(&params);
        assert_eq!(fh.get_fic_quality(), 0);
    }

    #[test]
    fn creation_mode2() {
        let params = DabParams::new(2);
        let _fh = FicHandler::new(&params);
    }

    #[test]
    fn channel_data_initially_unused() {
        let params = DabParams::new(1);
        let fh = FicHandler::new(&params);
        for i in 0..64 {
            assert!(
                !fh.get_channel_info(i).in_use,
                "Channel {} should not be in use initially",
                i
            );
        }
    }

    #[test]
    fn cif_count_initial() {
        let params = DabParams::new(1);
        let fh = FicHandler::new(&params);
        let (hi, lo) = fh.get_cif_count();
        assert_eq!(hi, -1);
        assert_eq!(lo, -1);
    }

    #[test]
    fn process_zero_block() {
        let params = DabParams::new(1);
        let mut fh = FicHandler::new(&params);
        let bits_per_block = 2 * params.k as usize;
        let data = vec![0i16; bits_per_block * 3];
        let mut out = vec![0u8; 768];
        let mut valid = vec![false; 4];
        fh.process_fic_block(&data, &mut out, &mut valid);
    }

    /// Pre-allocating `viterbi_block` as a struct field must not change the
    /// output: two consecutive calls with identical input must produce
    /// identical `out` and `valid` results.
    #[test]
    fn viterbi_block_prealloc_output_is_deterministic() {
        let params = DabParams::new(1);
        let bits_per_block = 2 * params.k as usize;
        // Populate with a non-trivial pattern so PRBS and Viterbi actually run.
        let data: Vec<i16> = (0..bits_per_block * 3)
            .map(|i| if i % 2 == 0 { 127 } else { -127 })
            .collect();

        let mut fh = FicHandler::new(&params);

        let mut out1 = vec![0u8; 768 * 4];
        let mut valid1 = vec![false; 4];
        fh.process_fic_block(&data, &mut out1, &mut valid1);

        // Second call on the same handler reuses the pre-allocated buffer.
        let mut out2 = vec![0u8; 768 * 4];
        let mut valid2 = vec![false; 4];
        fh.process_fic_block(&data, &mut out2, &mut valid2);

        assert_eq!(
            out1, out2,
            "Pre-allocated buffer must be zeroed before each use"
        );
        assert_eq!(valid1, valid2);
    }

    #[test]
    fn decode_ratio_steps_match_dabstar_leaky_behavior() {
        assert_eq!(update_decode_ratio_steps(0, 3, 3), 3);
        assert_eq!(update_decode_ratio_steps(3, 0, 3), 0);
        assert_eq!(update_decode_ratio_steps(9, 3, 3), 10);
        assert_eq!(update_decode_ratio_steps(5, 2, 3), 6);
    }

    #[test]
    fn get_fib_bits_returns_last_frame_snapshot() {
        let params = DabParams::new(1);
        let mut fh = FicHandler::new(&params);
        let bits_per_block = 2 * params.k as usize;
        let data = vec![0i16; bits_per_block * 3];
        let mut out = vec![0u8; 768 * 4];
        let mut valid = [false; 4];

        fh.process_fic_block(&data, &mut out, &mut valid);

        let mut snapshot = vec![0u8; 768 * 4];
        let mut snapshot_valid = [false; 4];
        fh.get_fib_bits(&mut snapshot, &mut snapshot_valid);

        assert_eq!(snapshot, out);
        assert_eq!(snapshot_valid, valid);
    }

    #[test]
    fn process_fic_block_accepts_four_slot_valid_buffer() {
        let params = DabParams::new(1);
        let mut fh = FicHandler::new(&params);
        let bits_per_block = 2 * params.k as usize;
        let data = vec![0i16; bits_per_block * 3];
        let mut out = vec![0u8; 768];
        let mut valid = [false; 4];

        fh.process_fic_block(&data, &mut out, &mut valid);

        assert_eq!(valid.len(), 4);
    }
}
