use std::sync::Arc;

use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

pub const TU: usize = 2_048;
pub const TG: usize = 504;
pub const TS: usize = 2_552;
pub const K: usize = 1_536;

// ETSI EN 300 401 §14: FFT + differential QPSK carrier demodulation.
pub struct OfdmDecoder {
    fft: Arc<dyn Fft<f32>>,
    phase_reference: Vec<Complex32>,
    initialized: bool,
}

impl Default for OfdmDecoder {
    fn default() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            fft: planner.plan_fft_forward(TU),
            phase_reference: vec![Complex32::new(1.0, 0.0); TU],
            initialized: false,
        }
    }
}

impl OfdmDecoder {
    pub fn store_reference_symbol_0(&mut self, samples: &[Complex32]) {
        let bins = self.fft_symbol(samples);
        if bins.len() == TU {
            self.phase_reference = bins;
            self.initialized = true;
        }
    }

    pub fn process_symbol(&mut self, samples: &[Complex32]) -> Vec<i8> {
        let bins = self.fft_symbol(samples);
        if bins.len() != TU {
            return Vec::new();
        }

        let mut out = vec![0i8; 2 * K];
        for (nom_carrier_idx, fft_idx) in carrier_map().iter().enumerate() {
            let cur = bins[*fft_idx];
            let prev = if self.initialized {
                self.phase_reference[*fft_idx]
            } else {
                Complex32::new(1.0, 0.0)
            };
            let diff = if prev.norm_sqr() > 0.0 {
                cur * prev.conj() / prev.norm()
            } else {
                cur
            };

            out[nom_carrier_idx] = soft_clip(diff.re * 64.0);
            out[K + nom_carrier_idx] = soft_clip(diff.im * 64.0);
        }

        self.phase_reference = bins;
        self.initialized = true;
        out
    }

    fn fft_symbol(&self, samples: &[Complex32]) -> Vec<Complex32> {
        let mut buffer = if samples.len() >= TS {
            samples[TG..(TG + TU)].to_vec()
        } else if samples.len() >= TU {
            samples[..TU].to_vec()
        } else {
            return Vec::new();
        };

        self.fft.process(&mut buffer);
        buffer
    }
}

fn soft_clip(value: f32) -> i8 {
    value.clamp(-127.0, 127.0) as i8
}

fn carrier_map() -> Vec<usize> {
    let mut map = Vec::with_capacity(K);
    for rel in -768..=768 {
        if rel == 0 {
            continue;
        }
        let idx = if rel < 0 {
            (TU as isize + rel) as usize
        } else {
            rel as usize
        };
        map.push(idx);
    }
    map
}

#[cfg(test)]
mod tests {
    use num_complex::Complex32;
    use rustfft::FftPlanner;

    use super::{OfdmDecoder, TG, TS, TU};

    #[test]
    fn produces_full_soft_bit_vector() {
        let mut bins = vec![Complex32::new(0.0, 0.0); TU];
        for bin in bins.iter_mut().take(768).skip(1) {
            *bin = Complex32::new(1.0, 1.0);
        }
        for bin in bins.iter_mut().skip(TU - 768) {
            *bin = Complex32::new(1.0, 1.0);
        }

        let mut planner = FftPlanner::<f32>::new();
        let ifft = planner.plan_fft_inverse(TU);
        ifft.process(&mut bins);

        let mut symbol = vec![Complex32::new(0.0, 0.0); TS];
        symbol[..TG].copy_from_slice(&bins[TU - TG..]);
        symbol[TG..].copy_from_slice(&bins);

        let mut decoder = OfdmDecoder::default();
        decoder.store_reference_symbol_0(&symbol);
        let bits = decoder.process_symbol(&symbol);
        assert_eq!(bits.len(), 3072);
    }
}
