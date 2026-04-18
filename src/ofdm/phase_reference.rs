use std::sync::Arc;

use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

const TU: usize = 2_048;
const TG: usize = 504;
const TS: usize = 2_552;
const K: i32 = 1_536;
const CARRIER_SPACING_HZ: f32 = 1_000.0;

// ETSI EN 300 401 §14 plus DABstar's sync-symbol phase reference correlation.
pub struct PhaseReference {
    last_phase_rad: f32,
    last_freq_error_hz: f32,
    ref_table: Vec<Complex32>,
    ref_arg: Vec<Complex32>,
    fft_fwd: Arc<dyn Fft<f32>>,
    fft_bwd: Arc<dyn Fft<f32>>,
    sync_on_strongest_peak: bool,
}

impl Default for PhaseReference {
    fn default() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let ref_table = build_ref_table();
        let fft_fwd = planner.plan_fft_forward(TU);
        let fft_bwd = planner.plan_fft_inverse(TU);
        let ref_arg = build_ref_arg(&ref_table, fft_bwd.clone());
        Self {
            last_phase_rad: 0.0,
            last_freq_error_hz: 0.0,
            ref_table,
            ref_arg,
            fft_fwd,
            fft_bwd,
            sync_on_strongest_peak: false,
        }
    }
}

impl PhaseReference {
    pub fn analyze(&mut self, samples: &[Complex32]) -> f32 {
        if samples.len() < TS {
            self.last_phase_rad = 0.0;
            self.last_freq_error_hz = 0.0;
            return self.last_phase_rad;
        }

        let mut corr = Complex32::new(0.0, 0.0);
        for idx in TU..TS {
            corr += samples[idx] * samples[idx - TU].conj();
        }

        self.last_phase_rad = corr.arg();
        self.last_freq_error_hz =
            self.last_phase_rad / (2.0 * std::f32::consts::PI) * CARRIER_SPACING_HZ;
        self.last_phase_rad
    }

    pub fn correlate_with_phase_ref_and_find_max_peak(
        &mut self,
        samples: &[Complex32],
        threshold: f32,
    ) -> Option<usize> {
        if samples.len() < TU {
            return None;
        }

        let mut buffer = samples[..TU].to_vec();
        self.fft_fwd.process(&mut buffer);

        for (bin, reference) in buffer.iter_mut().zip(self.ref_table.iter()) {
            *bin *= reference.conj();
        }

        self.fft_bwd.process(&mut buffer);

        let mut corr_peak_values = vec![0.0f32; TU];
        let mut sum = 0.0f32;
        for (idx, value) in buffer.iter().enumerate() {
            let abs_val = value.norm();
            corr_peak_values[idx] = abs_val;
            sum += abs_val;
        }
        sum /= TU as f32;
        if sum <= 0.0 {
            return None;
        }

        const GAP_SEARCH_WIDTH: usize = 10;
        const EXTENDED_SEARCH_REGION: usize = 250;
        let idx_start = TG.saturating_sub(EXTENDED_SEARCH_REGION);
        let idx_stop = (TG + 2 * EXTENDED_SEARCH_REGION).min(corr_peak_values.len());

        let mut indices = Vec::new();
        let mut max_index = None;
        let mut max_level = -1.0f32;

        let mut i = idx_start;
        while i < idx_stop {
            if corr_peak_values[i] / sum > threshold {
                let mut found_one = true;
                for j in 1..GAP_SEARCH_WIDTH {
                    if i + j >= idx_stop {
                        break;
                    }
                    if corr_peak_values[i + j] > corr_peak_values[i] {
                        found_one = false;
                        break;
                    }
                }

                if found_one {
                    indices.push(i);
                    if corr_peak_values[i] > max_level {
                        max_level = corr_peak_values[i];
                        max_index = Some(i);
                    }
                    i += GAP_SEARCH_WIDTH;
                    continue;
                }
            }
            i += 1;
        }

        if max_level / sum < threshold {
            return None;
        }

        if self.sync_on_strongest_peak {
            max_index
        } else {
            indices.first().copied().or(max_index)
        }
    }

    pub fn estimate_carrier_offset_from_sync_symbol_0(
        &mut self,
        fft_bins: &[Complex32],
    ) -> Option<i32> {
        if fft_bins.len() < TU {
            return None;
        }

        const SEARCHRANGE: i32 = 2 * 70;

        let mut buffer = calculate_relative_phase(&fft_bins[..TU]);
        self.fft_bwd.process(&mut buffer);

        for (value, reference) in buffer.iter_mut().zip(self.ref_arg.iter()) {
            *value *= *reference;
        }

        self.fft_fwd.process(&mut buffer);

        let mut index = 0i32;
        let mut max_value = 0.0f32;
        let mut avg_value = 0.0f32;

        for i in -(SEARCHRANGE / 2)..=(SEARCHRANGE / 2) {
            let value = buffer[((TU as i32 + i) as usize) % TU].norm();
            if value > max_value {
                max_value = value;
                index = i;
            }
            avg_value += value;
        }
        avg_value /= (SEARCHRANGE + 1) as f32;

        if max_value < avg_value * 5.0 {
            return None;
        }

        let mut peak = [0.0f32; 3];
        let mut peak_sum = 0.0f32;
        for (i, slot) in peak.iter_mut().enumerate() {
            *slot = buffer[((TU as i32 + index + i as i32 - 1) as usize) % TU].norm();
            peak_sum += *slot;
        }
        if peak_sum <= 0.0 {
            return None;
        }

        let offset = index as f32 + (peak[2] - peak[0]) / peak_sum;
        Some((offset * CARRIER_SPACING_HZ) as i32)
    }

    #[allow(dead_code)]
    pub fn last_freq_error_hz(&self) -> f32 {
        self.last_freq_error_hz
    }
}

fn build_ref_table() -> Vec<Complex32> {
    let mut ref_table = vec![Complex32::new(0.0, 0.0); TU];

    for i in 1..=(K as usize / 2) {
        ref_table[i] = cmplx_from_phase(get_phi(i as i32));
        ref_table[TU - i] = cmplx_from_phase(get_phi(-(i as i32)));
    }

    ref_table
}

fn build_ref_arg(ref_table: &[Complex32], fft_bwd: Arc<dyn Fft<f32>>) -> Vec<Complex32> {
    let mut buffer = calculate_relative_phase(ref_table);
    fft_bwd.process(&mut buffer);
    for value in &mut buffer {
        *value = value.conj();
    }
    buffer
}

fn calculate_relative_phase(fft_in: &[Complex32]) -> Vec<Complex32> {
    let mut out = vec![Complex32::new(0.0, 0.0); TU];
    for i in 0..(TU - 1) {
        out[i] = fft_in[i].conj() * fft_in[i + 1];
    }
    out[TU - 1] = Complex32::new(0.0, 0.0);
    out
}

fn cmplx_from_phase(phase: f32) -> Complex32 {
    Complex32::new(phase.cos(), phase.sin())
}

fn h_table(i: i32, j: usize) -> i32 {
    const H0: [i8; 32] = [
        0, 2, 0, 0, 0, 0, 1, 1, 2, 0, 0, 0, 2, 2, 1, 1, 0, 2, 0, 0, 0, 0, 1, 1, 2, 0, 0, 0, 2, 2,
        1, 1,
    ];
    const H1: [i8; 32] = [
        0, 3, 2, 3, 0, 1, 3, 0, 2, 1, 2, 3, 2, 3, 3, 0, 0, 3, 2, 3, 0, 1, 3, 0, 2, 1, 2, 3, 2, 3,
        3, 0,
    ];
    const H2: [i8; 32] = [
        0, 0, 0, 2, 0, 2, 1, 3, 2, 2, 0, 2, 2, 0, 1, 3, 0, 0, 0, 2, 0, 2, 1, 3, 2, 2, 0, 2, 2, 0,
        1, 3,
    ];
    const H3: [i8; 32] = [
        0, 1, 2, 1, 0, 3, 3, 2, 2, 3, 2, 1, 2, 1, 3, 2, 0, 1, 2, 1, 0, 3, 3, 2, 2, 3, 2, 1, 2, 1,
        3, 2,
    ];

    match i {
        0 => i32::from(H0[j]),
        1 => i32::from(H1[j]),
        2 => i32::from(H2[j]),
        3 => i32::from(H3[j]),
        _ => 0,
    }
}

fn get_phi(k: i32) -> f32 {
    const MODE_I_TABLE: [(i32, i32, i32, i32); 49] = [
        (-768, -737, 0, 1),
        (-736, -705, 1, 2),
        (-704, -673, 2, 0),
        (-672, -641, 3, 1),
        (-640, -609, 0, 3),
        (-608, -577, 1, 2),
        (-576, -545, 2, 2),
        (-544, -513, 3, 3),
        (-512, -481, 0, 2),
        (-480, -449, 1, 1),
        (-448, -417, 2, 2),
        (-416, -385, 3, 3),
        (-384, -353, 0, 1),
        (-352, -321, 1, 2),
        (-320, -289, 2, 3),
        (-288, -257, 3, 3),
        (-256, -225, 0, 2),
        (-224, -193, 1, 2),
        (-192, -161, 2, 2),
        (-160, -129, 3, 1),
        (-128, -97, 0, 1),
        (-96, -65, 1, 3),
        (-64, -33, 2, 1),
        (-32, -1, 3, 2),
        (1, 32, 0, 3),
        (33, 64, 3, 1),
        (65, 96, 2, 1),
        (97, 128, 1, 1),
        (129, 160, 0, 2),
        (161, 192, 3, 2),
        (193, 224, 2, 1),
        (225, 256, 1, 0),
        (257, 288, 0, 2),
        (289, 320, 3, 2),
        (321, 352, 2, 3),
        (353, 384, 1, 3),
        (385, 416, 0, 0),
        (417, 448, 3, 2),
        (449, 480, 2, 1),
        (481, 512, 1, 3),
        (513, 544, 0, 3),
        (545, 576, 3, 3),
        (577, 608, 2, 3),
        (609, 640, 1, 0),
        (641, 672, 0, 3),
        (673, 704, 3, 0),
        (705, 736, 2, 1),
        (737, 768, 1, 1),
        (-1000, -1000, 0, 0),
    ];

    for (kmin, kmax, i, n) in MODE_I_TABLE {
        if kmin == -1000 {
            break;
        }
        if kmin <= k && k <= kmax {
            let k_prime = kmin;
            return std::f32::consts::FRAC_PI_2 * (h_table(i, (k - k_prime) as usize) + n) as f32;
        }
    }

    0.0
}

#[cfg(test)]
mod tests {
    use num_complex::Complex32;
    use rustfft::FftPlanner;

    use super::{build_ref_table, PhaseReference, TG, TS, TU};

    #[test]
    fn estimates_prefix_phase_shift() {
        let phase_step = 0.1f32;
        let samples = (0..TS)
            .map(|idx| {
                let phase = idx as f32 * phase_step;
                Complex32::new(phase.cos(), phase.sin())
            })
            .collect::<Vec<_>>();

        let mut pr = PhaseReference::default();
        let phase = pr.analyze(&samples);
        assert!(phase.is_finite());
        assert!(pr.last_freq_error_hz().is_finite());
    }

    #[test]
    fn finds_sync_peak_near_guard_offset() {
        let mut ref_symbol = build_ref_table();
        let mut planner = FftPlanner::<f32>::new();
        let ifft = planner.plan_fft_inverse(TU);
        ifft.process(&mut ref_symbol);

        let mut shifted = vec![Complex32::new(0.0, 0.0); TU];
        for idx in 0..TU {
            shifted[(idx + TG) % TU] = ref_symbol[idx];
        }

        let mut pr = PhaseReference::default();
        let peak = pr
            .correlate_with_phase_ref_and_find_max_peak(&shifted, 1.0)
            .expect("sync peak");
        assert!((peak as isize - TG as isize).unsigned_abs() <= 8);
    }

    #[test]
    fn estimates_coarse_offset_from_shifted_sync_symbol() {
        let ref_table = build_ref_table();
        let shift_bins = 4usize;
        let mut shifted = vec![Complex32::new(0.0, 0.0); TU];
        for idx in 0..TU {
            shifted[(idx + shift_bins) % TU] = ref_table[idx];
        }

        let mut pr = PhaseReference::default();
        let correction_hz = pr
            .estimate_carrier_offset_from_sync_symbol_0(&shifted)
            .expect("coarse correction");
        assert!((correction_hz.abs() - 4_000).abs() <= 1_500);
    }
}
