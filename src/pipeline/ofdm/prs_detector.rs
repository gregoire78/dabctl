// Phase Reference Symbol (PRS) detector.
// Derived from phase_reference.rs (eti-cmdline port).
// ETSI EN 300 401 §14.4 — Phase Reference Symbol

use crate::pipeline::ofdm::phase_table::PhaseTable;
use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

/// PRS detector: correlates the received PRS with the known reference to
/// find the first sample of the first data symbol and estimate any coarse
/// carrier-frequency offset.
pub struct PrsDetector {
    t_u: usize,
    diff_length: usize,
    ref_table: Vec<Complex32>,
    phase_differences: Vec<Complex32>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
}

impl PrsDetector {
    /// Construct a PRS detector for the given DAB mode.
    ///
    /// `t_u`          – useful symbol length (FFT size)
    /// `carriers`     – number of active sub-carriers (K)
    /// `mode`         – DAB transmission mode (1–4)
    /// `diff_length`  – number of bins used for coarse offset estimation
    pub fn new(t_u: usize, carriers: usize, mode: i16, diff_length: usize) -> Self {
        let phase_table = PhaseTable::new(mode);
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(t_u);
        let ifft = planner.plan_fft_inverse(t_u);

        let mut ref_table = vec![Complex32::new(0.0, 0.0); t_u];

        for i in 1..=(carriers as i32 / 2) {
            let phi_k = phase_table.get_phi(i);
            ref_table[i as usize] = Complex32::new(phi_k.cos(), phi_k.sin());
            let phi_k = phase_table.get_phi(-i);
            ref_table[t_u - i as usize] = Complex32::new(phi_k.cos(), phi_k.sin());
        }

        // Prepare phase differences table for coarse frequency offset estimation.
        let mut phase_differences = vec![Complex32::new(0.0, 0.0); diff_length];
        for i in 1..=diff_length {
            phase_differences[i - 1] =
                ref_table[(t_u + i) % t_u] * ref_table[(t_u + i + 1) % t_u].conj();
        }

        PrsDetector {
            t_u,
            diff_length,
            ref_table,
            phase_differences,
            fft,
            ifft,
        }
    }

    /// Find the first sample of the first non-null symbol by correlation.
    ///
    /// Returns the sample offset (≥0) on success, or a negative value whose
    /// magnitude encodes the peak-to-mean ratio when the threshold is not met.
    pub fn find_index(&self, v: &[Complex32], threshold: i16) -> i32 {
        let mut fft_buffer = v[..self.t_u].to_vec();
        self.fft.process(&mut fft_buffer);

        // Correlate in frequency domain.
        for (fb, rt) in fft_buffer
            .iter_mut()
            .zip(self.ref_table.iter())
            .take(self.t_u)
        {
            *fb *= rt.conj();
        }

        // Back to time domain.
        self.ifft.process(&mut fft_buffer);

        // Normalise after IFFT (rustfft does not normalise).
        let norm = 1.0 / self.t_u as f32;
        for s in fft_buffer.iter_mut() {
            *s *= norm;
        }

        let sum: f32 = fft_buffer.iter().map(|c| c.norm()).sum();
        let mut max_val: f32 = -10000.0;
        let mut max_index: i32 = -1;

        for (i, f) in fft_buffer.iter().enumerate().take(self.t_u) {
            let v = f.norm();
            if v > max_val {
                max_val = v;
                max_index = i as i32;
            }
        }

        if max_val < threshold as f32 * sum / self.t_u as f32 {
            -(max_val * self.t_u as f32 / sum).abs() as i32 - 1
        } else {
            max_index
        }
    }

    /// Estimate coarse carrier-frequency offset from the PRS.
    ///
    /// Returns the offset in carrier-index units. A return value of 100
    /// indicates that no reliable estimate could be made.
    pub fn estimate_offset(&self, v: &[Complex32]) -> i16 {
        let mut fft_buffer = v[..self.t_u].to_vec();
        self.fft.process(&mut fft_buffer);

        let search_range = 2 * 35;
        let mut m_min: f32 = 1000.0;
        let mut index: i16 = 100;

        for i in (self.t_u - search_range / 2)..(self.t_u + search_range / 2) {
            let mut diff: f32 = 0.0;
            for j in 0..self.diff_length {
                let ind1 = (i + j + 1) % self.t_u;
                let ind2 = (i + j + 2) % self.t_u;
                let pd = fft_buffer[ind1] * fft_buffer[ind2].conj();
                diff += (pd * self.phase_differences[j].conj()).arg().abs();
            }
            if diff < m_min {
                m_min = diff;
                index = i as i16;
            }
        }

        index - self.t_u as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfft::FftPlanner;

    /// Build the IFFT of the mode-1 ref_table to produce a synthetic PRS in
    /// the time domain; this is the ideal input for `find_index`.
    fn synthetic_prs_mode1() -> (PrsDetector, Vec<Complex32>) {
        let t_u = 2048usize;
        let carriers = 1536usize;
        let diff_length = 50usize;
        let detector = PrsDetector::new(t_u, carriers, 1, diff_length);

        // IFFT of ref_table → time-domain PRS
        let mut planner = FftPlanner::new();
        let ifft = planner.plan_fft_inverse(t_u);
        let mut buf = detector.ref_table.clone();
        ifft.process(&mut buf);
        let norm = 1.0 / t_u as f32;
        for s in buf.iter_mut() {
            *s *= norm;
        }
        (detector, buf)
    }

    #[test]
    fn find_index_on_synthetic_prs() {
        // Correlating the PRS detector against its own ideal PRS should yield
        // a sharp peak at index 0.
        let (detector, prs) = synthetic_prs_mode1();
        let idx = detector.find_index(&prs, 2);
        assert!(
            idx >= 0,
            "find_index should succeed on synthetic PRS, got {}",
            idx
        );
        assert_eq!(idx, 0, "peak should be at index 0 for unshifted PRS");
    }

    #[test]
    fn estimate_offset_returns_near_zero_for_unshifted() {
        // The ideal (unshifted) PRS should give an offset estimate near 0.
        let (detector, prs) = synthetic_prs_mode1();
        let offset = detector.estimate_offset(&prs);
        assert!(
            offset.abs() <= 5,
            "offset for unshifted PRS should be ≤5 carriers, got {}",
            offset
        );
    }
}
