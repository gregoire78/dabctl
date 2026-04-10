// FIC handler - converted from fic-handler.cpp (eti-cmdline)

use crate::pipeline::dab_constants::{check_crc_bits, ChannelData};
use crate::pipeline::dab_params::DabParams;
use crate::pipeline::fib_processor::FibProcessor;
use crate::pipeline::prot_tables::get_pcodes;
use crate::pipeline::viterbi_handler::ViterbiSpiral;

pub struct FicHandler {
    bits_per_block: usize,
    ofdm_input: Vec<i16>,
    puncture_table: Vec<bool>,
    prbs: [u8; 768],
    viterbi: ViterbiSpiral,
    bit_buffer_out: Vec<u8>,
    pub fib_processor: FibProcessor,
    fic_errors: i32,
    fic_success: i32,
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
            puncture_table,
            prbs,
            viterbi: ViterbiSpiral::new(768),
            bit_buffer_out: vec![0u8; 768],
            fib_processor: FibProcessor::new(),
            fic_errors: 0,
            fic_success: 0,
        }
    }

    /// Process 3 FIC blocks (blocks 2,3,4) of BitsperBlock soft bits each.
    /// Returns (fib_bytes: [4*768 bits], valid: [4 bools])
    pub fn process_fic_block(&mut self, data: &[i16], out: &mut [u8], valid: &mut [bool]) {
        let mut index = 0usize;
        let mut ficno = 0usize;

        for i in 0..3 {
            for j in 0..self.bits_per_block {
                self.ofdm_input[index] = data[i * self.bits_per_block + j];
                index += 1;
                if index >= 2304 {
                    self.process_fic_input(ficno, out, &mut valid[ficno]);
                    index = 0;
                    ficno += 1;
                }
            }
        }
    }

    fn process_fic_input(&mut self, ficno: usize, fib_bytes: &mut [u8], valid: &mut bool) {
        // Depuncture
        let mut viterbi_block = vec![0i16; 3072 + 24];
        let mut input_count = 0;
        for (i, vb) in viterbi_block.iter_mut().enumerate().take(3072 + 24) {
            if self.puncture_table[i] {
                *vb = self.ofdm_input[input_count];
                input_count += 1;
            }
        }

        // Viterbi decode
        self.viterbi
            .deconvolve(&viterbi_block, &mut self.bit_buffer_out);

        // Energy dispersal (PRBS descramble)
        for i in 0..768 {
            self.bit_buffer_out[i] ^= self.prbs[i];
        }

        // CRC check on each of the 3 FIBs in this ficno
        *valid = true;
        for i in (ficno * 3)..(ficno * 3 + 3) {
            let fib_idx = i % 3;
            let p = &self.bit_buffer_out[fib_idx * 256..(fib_idx + 1) * 256];
            if !check_crc_bits(p, 256) {
                *valid = false;
                self.fic_errors += 1;
                continue;
            }
            self.fic_success += 1;
            self.fib_processor.process_fib(p, ficno as u16);
        }

        // Copy bits to output
        let offset = ficno * 768;
        if offset + 768 <= fib_bytes.len() {
            fib_bytes[offset..offset + 768].copy_from_slice(&self.bit_buffer_out);
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

    pub fn get_fic_quality(&self) -> i16 {
        if self.fic_errors + self.fic_success > 0 {
            (self.fic_success * 100 / (self.fic_errors + self.fic_success)) as i16
        } else {
            0
        }
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
}
