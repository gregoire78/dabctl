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
    mean_value: f32,
    mean_power_vector: Vec<f32>,
    mean_sigma_sq_vector: Vec<f32>,
    mean_null_power_without_tii: Vec<f32>,
    /// Per-carrier integrated phase error (DABstar's mIntegAbsPhaseVector).
    integ_abs_phase_vector: Vec<f32>,
}

impl Default for OfdmDecoder {
    fn default() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            fft: planner.plan_fft_forward(TU),
            phase_reference: vec![Complex32::new(1.0, 0.0); TU],
            initialized: false,
            mean_value: 1.0,
            mean_power_vector: vec![0.0; K],
            mean_sigma_sq_vector: vec![0.0; K],
            mean_null_power_without_tii: vec![0.0; TU],
            integ_abs_phase_vector: vec![0.0; K],
        }
    }
}

impl OfdmDecoder {
    pub fn symbol_0_bins(&self, samples: &[Complex32]) -> Vec<Complex32> {
        self.fft_symbol(samples, 0.0)
    }

    pub fn store_reference_symbol_0_bins(&mut self, bins: &[Complex32]) {
        if bins.len() == TU {
            self.phase_reference = bins.to_vec();
            self.initialized = true;
        }
    }

    pub fn store_null_symbol_without_tii(&mut self, samples: &[Complex32]) {
        let bins = self.fft_symbol(samples, 0.0);
        if bins.len() != TU {
            return;
        }

        const ALPHA: f32 = 0.1;
        for (idx, bin) in bins.iter().enumerate() {
            mean_filter(
                &mut self.mean_null_power_without_tii[idx],
                bin.norm_sqr(),
                ALPHA,
            );
        }
    }

    pub fn process_symbol(
        &mut self,
        samples: &[Complex32],
        phase_corr: f32,
        clock_err_hz: f32,
    ) -> Vec<i16> {
        let bins = self.fft_symbol(samples, phase_corr);
        if bins.len() != TU {
            return Vec::new();
        }

        const ALPHA: f32 = 0.005;
        const RAD_PER_DEG_20: f32 = 20.0 * std::f32::consts::PI / 180.0;

        let cmap = carrier_map();
        let mut out = vec![0i16; 2 * K];
        let mut sum = 0.0f32;

        for (nom_carrier_idx, fft_idx) in cmap.iter().enumerate() {
            let cur = bins[*fft_idx];
            let prev = if self.initialized {
                self.phase_reference[*fft_idx]
            } else {
                Complex32::new(1.0, 0.0)
            };

            let fft_bin_raw = if prev.norm_sqr() > 0.0 {
                cur * norm_to_length_one(prev.conj())
            } else {
                cur
            };

            let signed_fft_idx = if *fft_idx < TU / 2 {
                *fft_idx as i16
            } else {
                *fft_idx as i16 - TU as i16
            };
            let real_carr_rel_idx = if signed_fft_idx < 0 {
                signed_fft_idx + (K / 2) as i16
            } else {
                signed_fft_idx + (K / 2 - 1) as i16
            };

            let integ_ref = &mut self.integ_abs_phase_vector[nom_carrier_idx];
            let phase_err = clock_err_hz / 1024.0
                * std::f32::consts::PI
                * (((K / 2) as f32 - real_carr_rel_idx as f32) / (K / 2) as f32)
                + *integ_ref;
            let fft_bin = fft_bin_raw * cmplx_from_phase(-phase_err);

            let fft_bin_phase = fft_bin.arg();
            let fft_bin_abs_phase = turn_phase_to_first_quadrant(fft_bin_phase);
            *integ_ref += 0.2 * ALPHA * (fft_bin_abs_phase - std::f32::consts::FRAC_PI_4);
            *integ_ref = integ_ref.clamp(-RAD_PER_DEG_20, RAD_PER_DEG_20);

            let fft_bin_power = fft_bin.norm_sqr().max(1.0e-6);
            let mean_power = &mut self.mean_power_vector[nom_carrier_idx];
            mean_filter(mean_power, fft_bin_power, ALPHA);

            let mean_level_per_bin = (*mean_power).sqrt().max(1.0e-6);
            let mean_level_at_axis = mean_level_per_bin * std::f32::consts::FRAC_1_SQRT_2;
            let real_level_dist = fft_bin.re.abs() - mean_level_at_axis;
            let imag_level_dist = fft_bin.im.abs() - mean_level_at_axis;
            let sigma_sq_per_bin =
                (real_level_dist * real_level_dist + imag_level_dist * imag_level_dist).max(1.0e-6);
            let mean_sigma_sq = &mut self.mean_sigma_sq_vector[nom_carrier_idx];
            mean_filter(mean_sigma_sq, sigma_sq_per_bin, ALPHA);

            let mut signal_power = *mean_power - self.mean_null_power_without_tii[*fft_idx];
            if signal_power <= 0.0 {
                signal_power = 0.1;
            }

            let mut w1 = (fft_bin.norm() * prev.norm()).sqrt().max(1.0e-6);
            w1 *= mean_level_per_bin;
            w1 /= (*mean_sigma_sq).max(1.0e-6);
            w1 /= (self.mean_null_power_without_tii[*fft_idx] / signal_power) + 1.0;

            let r1 = norm_to_length_one(fft_bin) * w1;
            let w2 = -100.0 / self.mean_value.max(1.0e-3);
            out[nom_carrier_idx] = soft_value(r1.re * w2);
            out[K + nom_carrier_idx] = soft_value(r1.im * w2);
            sum += r1.norm();
        }

        self.mean_value = (sum / K as f32).max(1.0e-3);

        self.phase_reference = bins;
        self.initialized = true;
        out
    }

    fn fft_symbol(&self, samples: &[Complex32], _phase_corr: f32) -> Vec<Complex32> {
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

fn soft_value(value: f32) -> i16 {
    value as i16
}

fn mean_filter(target: &mut f32, value: f32, alpha: f32) {
    *target = (1.0 - alpha) * *target + alpha * value;
}

fn norm_to_length_one(value: Complex32) -> Complex32 {
    let norm = value.norm();
    if norm > 1.0e-12 {
        value / norm
    } else {
        Complex32::new(0.0, 0.0)
    }
}

/// Construct a unit complex number from a phase angle.
fn cmplx_from_phase(phase: f32) -> Complex32 {
    Complex32::new(phase.cos(), phase.sin())
}

/// Map any phase to the first quadrant [0, PI/2).
/// For QPSK with constellation at PI/4 + n*PI/2, the ideal absolute
/// phase is always PI/4.
fn turn_phase_to_first_quadrant(phase: f32) -> f32 {
    phase.rem_euclid(std::f32::consts::FRAC_PI_2)
}

fn carrier_map() -> Vec<usize> {
    // Generate the LCG permutation table (ETSI EN 300 401 §14.6).
    // DABstar's FreqInterleaver::createMapper stores entries in LCG iteration
    // order: mPermTable[index++] = tmp[i] - T_u/2.  We replicate that exact
    // ordering here (push in iteration order).
    let mut tmp = vec![0i16; TU];
    for idx in 1..TU {
        tmp[idx] = (13 * tmp[idx - 1] + 511) % TU as i16;
    }

    let mut map = Vec::with_capacity(K);
    for permuted in tmp {
        if permuted == (TU / 2) as i16 {
            continue;
        }
        if !(256..=(256 + K as i16)).contains(&permuted) {
            continue;
        }

        let rel = permuted - (TU / 2) as i16;
        let fft_idx = if rel < 0 {
            (TU as i16 + rel) as usize
        } else {
            rel as usize
        };
        map.push(fft_idx);
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
        let bins = decoder.symbol_0_bins(&symbol);
        decoder.store_reference_symbol_0_bins(&bins);
        let bits = decoder.process_symbol(&symbol, 0.0, 0.0);
        assert_eq!(bits.len(), 3072);
    }

    #[test]
    fn updates_null_symbol_noise_floor() {
        let symbol = vec![Complex32::new(0.05, -0.02); TS];
        let mut decoder = OfdmDecoder::default();
        decoder.store_null_symbol_without_tii(&symbol);
        assert!(decoder.mean_null_power_without_tii.iter().any(|v| *v > 0.0));
    }
}
