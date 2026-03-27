use crate::pipeline::{
    crc16_matches,
    DabNormalizedFrame,
    FicBitstreamCandidate,
    FicBlockCandidate,
    FicDeinterleavedCandidate,
    FicDemapper,
    FicPreDecoder,
    FicSegmentCandidate,
    DAB_FIC_SYMBOL_COUNT,
    DAB_MODE_I_ACTIVE_CARRIERS,
};
use crate::viterbi::{
    fic_deinterleave_mode_i,
    fic_deinterleave_mode_i_soft,
    fic_depuncture_mode_i_soft,
    viterbi_decode_soft,
};
use rustfft::num_complex::Complex32;

// Taille fixe de la trame FIC Mode I en bits QPSK (3 symboles × 1536 porteuses × 2 bits)
const DAB_FIC_BITS_TOTAL: usize = DAB_FIC_SYMBOL_COUNT * DAB_MODE_I_ACTIVE_CARRIERS * 2;

impl Default for FicDemapper {
    fn default() -> Self {
        Self::new()
    }
}

impl FicDemapper {
    pub fn new() -> Self {
        Self
    }

    pub fn demapp_candidates(&self, frames: &[DabNormalizedFrame]) -> Vec<FicBitstreamCandidate> {
        frames
            .iter()
            .map(|frame| self.demapp_frame(frame))
            .collect()
    }

    pub fn demapp_frame(&self, frame: &DabNormalizedFrame) -> FicBitstreamCandidate {
        let mut bits = Vec::new();
        let mut soft_bits = Vec::new();
        let mut previous_symbol = &frame.phase_reference;

        for symbol in &frame.fic_symbols {
            for (carrier_idx, (carrier, previous_carrier)) in symbol
                .carriers
                .iter()
                .zip(previous_symbol.carriers.iter())
                .enumerate()
            {
                let differential = differential_qpsk_symbol(*previous_carrier, *carrier);
                // Scale soft bits by the per-carrier channel gain weight.
                // Weak carriers (deep fades) contribute small LLR values → Viterbi
                // treats them as near-erasures rather than misleading hard decisions.
                let weight = frame.channel_gains.get(carrier_idx).copied().unwrap_or(1.0);
                let soft_i = quantize_soft_axis(differential.re * weight);
                let soft_q = quantize_soft_axis(differential.im * weight);
                let bit_i = soft_axis_to_bit(soft_i);
                let bit_q = soft_axis_to_bit(soft_q);
                // ETSI EN 300 401 §14.4.1 : ik,0 est codé sur l'axe Im (Q),
                // ik,1 sur l'axe Re (I). Le bitstream FIC doit être (ik,0, ik,1)
                // donc Im en premier, Re en second.
                bits.push(bit_q);
                bits.push(bit_i);
                soft_bits.push(soft_q);
                soft_bits.push(soft_i);
            }
            previous_symbol = symbol;
        }

        FicBitstreamCandidate {
            frame_start_sample: frame.start_sample,
            bit_count: bits.len(),
            bits,
            soft_bits,
        }
    }
}

impl FicPreDecoder {
    pub fn new(fic_symbol_count: usize, segment_bits: usize) -> Self {
        Self {
            fic_symbol_count,
            segment_bits,
        }
    }

    /// Désentrelacement + Viterbi : produit des candidats désentrelacés
    /// en utilisant la vraie table de permutation ETSI EN 300 401 §14.6.1.
    pub fn deinterleave_candidates(
        &self,
        candidates: &[FicBitstreamCandidate],
    ) -> Vec<FicDeinterleavedCandidate> {
        candidates
            .iter()
            .filter_map(|candidate| self.deinterleave_candidate(candidate))
            .collect()
    }

    pub fn deinterleave_candidate(
        &self,
        candidate: &FicBitstreamCandidate,
    ) -> Option<FicDeinterleavedCandidate> {
        if candidate.bit_count == 0 {
            return None;
        }

        // Chemin réel : désentrelacement ETSI §14.6.1
        if candidate.bit_count == DAB_FIC_BITS_TOTAL {
            let deinterleaved = fic_deinterleave_mode_i(&candidate.bits);
            let source_soft_bits = if candidate.soft_bits.len() == candidate.bit_count {
                candidate.soft_bits.clone()
            } else {
                hard_bits_to_soft(&candidate.bits)
            };
            let soft_deinterleaved = fic_deinterleave_mode_i_soft(&source_soft_bits);
            return Some(FicDeinterleavedCandidate {
                frame_start_sample: candidate.frame_start_sample,
                bits: deinterleaved,
                soft_bits: soft_deinterleaved,
            });
        }

        // Chemin de test (données courtes) : transposition symbole→porteuse simplifiée
        if !candidate.bit_count.is_multiple_of(self.fic_symbol_count) {
            return None;
        }
        let bits_per_symbol = candidate.bit_count / self.fic_symbol_count;
        if !bits_per_symbol.is_multiple_of(2) {
            return None;
        }
        let carriers_per_symbol = bits_per_symbol / 2;
        let mut bits = Vec::with_capacity(candidate.bit_count);
        let source_soft_bits = if candidate.soft_bits.len() == candidate.bit_count {
            candidate.soft_bits.clone()
        } else {
            hard_bits_to_soft(&candidate.bits)
        };
        let mut soft_bits = Vec::with_capacity(candidate.bit_count);
        for carrier_index in 0..carriers_per_symbol {
            for symbol_index in 0..self.fic_symbol_count {
                let base = symbol_index * bits_per_symbol + carrier_index * 2;
                bits.push(candidate.bits[base]);
                bits.push(candidate.bits[base + 1]);
                soft_bits.push(source_soft_bits[base]);
                soft_bits.push(source_soft_bits[base + 1]);
            }
        }
        Some(FicDeinterleavedCandidate {
            frame_start_sample: candidate.frame_start_sample,
            bits,
            soft_bits,
        })
    }

    /// Segmentation : si la taille est celle du FIC complet, on applique le
    /// dépuncturing + Viterbi pour obtenir directement les bits FIB.
    /// Sinon (tests unitaires avec données courtes) : segmentation par blocs.
    pub fn segment_candidates(
        &self,
        candidates: &[FicDeinterleavedCandidate],
    ) -> Vec<FicSegmentCandidate> {
        let mut segments = Vec::new();

        for candidate in candidates {
            if candidate.bits.len() == DAB_FIC_BITS_TOTAL {
                // Chemin réel (eti-cmdline):
                // 3 symboles FIC (9216 soft bits) -> 4 blocs de 2304,
                // dépuncturing 2304->3096, Viterbi, suppression tail bits,
                // descrambling PRBS puis découpe en FIB de 256 bits.
                let source_soft_bits = if candidate.soft_bits.len() == candidate.bits.len() {
                    candidate.soft_bits.clone()
                } else {
                    hard_bits_to_soft(&candidate.bits)
                };
                let prbs = fic_prbs_768();
                let mut segment_index = 0usize;

                for chunk2304 in source_soft_bits.chunks(2304) {
                    if chunk2304.len() != 2304 {
                        continue;
                    }

                    let depunctured = fic_depuncture_mode_i_soft(chunk2304);
                    let decoded_bits = viterbi_decode_soft(&depunctured);
                    if decoded_bits.len() < 768 {
                        continue;
                    }

                    let mut descrambled = vec![0u8; 768];
                    for idx in 0..768 {
                        descrambled[idx] = (decoded_bits[idx] & 1) ^ prbs[idx];
                    }

                    for fib_bits in descrambled.chunks(self.segment_bits) {
                        segments.push(FicSegmentCandidate {
                            frame_start_sample: candidate.frame_start_sample,
                            segment_index,
                            bit_count: fib_bits.len(),
                            bits: fib_bits.to_vec(),
                        });
                        segment_index += 1;
                    }
                }
            } else {
                // Chemin de test : segmentation directe
                for (segment_index, chunk) in candidate.bits.chunks(self.segment_bits).enumerate() {
                    segments.push(FicSegmentCandidate {
                        frame_start_sample: candidate.frame_start_sample,
                        segment_index,
                        bit_count: chunk.len(),
                        bits: chunk.to_vec(),
                    });
                }
            }
        }

        segments
    }

    pub fn build_blocks(&self, segments: &[FicSegmentCandidate]) -> Vec<FicBlockCandidate> {
        segments
            .iter()
            .filter(|segment| segment.bit_count == self.segment_bits)
            .map(|segment| FicBlockCandidate {
                frame_start_sample: segment.frame_start_sample,
                block_index: segment.segment_index,
                bit_count: segment.bit_count,
                crc_ok: crc16_matches(segment.bits.as_slice()),
                bits: segment.bits.clone(),
            })
            .collect()
    }
}

fn differential_qpsk_symbol(previous: Complex32, current: Complex32) -> Complex32 {
    let previous_energy = previous.norm_sqr();
    if previous_energy <= 1e-9 {
        return current;
    }

    current * previous.conj() / previous_energy
}

fn quantize_soft_axis(value: f32) -> i16 {
    let scaled = (value * 16.0).round();
    scaled.clamp(-127.0, 127.0) as i16
}

fn soft_axis_to_bit(value: i16) -> u8 {
    if value >= 0 {
        0
    } else {
        1
    }
}

fn hard_bits_to_soft(bits: &[u8]) -> Vec<i16> {
    bits.iter()
        .map(|bit| if *bit == 0 { 127 } else { -127 })
        .collect()
}

fn fic_prbs_768() -> [u8; 768] {
    let mut prbs = [0u8; 768];
    let mut shift = [1u8; 9];
    for bit in &mut prbs {
        let pn = shift[8] ^ shift[4];
        *bit = pn;
        for idx in (1..9).rev() {
            shift[idx] = shift[idx - 1];
        }
        shift[0] = pn;
    }
    prbs
}
