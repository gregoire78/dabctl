// ETSI EN 300 401 §8.4.3 — frequency correction loop

use num_complex::Complex32;

/// Numerically Controlled Oscillator.
///
/// Tracks a unit phasor rotated by a configurable frequency. One `cos+sin`
/// call per batch amortises trig cost: O(1) trig + O(N) multiplies per call.
pub struct Nco {
    phasor: Complex32,
}

impl Nco {
    /// Create a new NCO with phasor initialised to 1+0j.
    pub fn new() -> Self {
        Nco {
            phasor: Complex32::new(1.0, 0.0),
        }
    }

    /// Rotate every sample in `samples` by `freq_hz` in-place.
    ///
    /// Computes one step phasor from one `cos+sin` call, then multiplies each
    /// sample. Renormalises the phasor at the end of the batch to prevent
    /// amplitude drift from accumulated floating-point rounding.
    pub fn apply_batch(&mut self, samples: &mut [Complex32], freq_hz: i32, sample_rate: u32) {
        if samples.is_empty() {
            return;
        }
        let delta = -2.0 * std::f32::consts::PI * freq_hz as f32 / sample_rate as f32;
        let step = Complex32::from_polar(1.0, delta);
        for sample in samples.iter_mut() {
            self.phasor *= step;
            *sample *= self.phasor;
        }
        // Renormalise phasor to prevent amplitude drift over long sequences.
        let norm = self.phasor.norm();
        if norm > 0.0 {
            self.phasor /= norm;
        }
    }

    /// Reset phasor to 1+0j.
    pub fn reset(&mut self) {
        self.phasor = Complex32::new(1.0, 0.0);
    }
}

impl Default for Nco {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn zero_frequency_leaves_samples_unchanged() {
        let mut nco = Nco::new();
        let original = vec![
            Complex32::new(1.0, 0.0),
            Complex32::new(0.0, 1.0),
            Complex32::new(-1.0, 0.5),
        ];
        let mut samples = original.clone();
        nco.apply_batch(&mut samples, 0, 2_048_000);
        for (s, o) in samples.iter().zip(original.iter()) {
            assert!(
                (s.re - o.re).abs() < 1e-5,
                "re changed: {} vs {}",
                s.re,
                o.re
            );
            assert!(
                (s.im - o.im).abs() < 1e-5,
                "im changed: {} vs {}",
                s.im,
                o.im
            );
        }
    }

    #[test]
    fn known_phase_rotates_correctly() {
        // At freq_hz = sample_rate/4, the step per sample is -π/2.
        // After one sample, a unit real input (1+0j) should rotate to (0-1j).
        let sample_rate: u32 = 2_048_000;
        let freq_hz = (sample_rate / 4) as i32; // quarter-wave per sample
        let mut nco = Nco::new();
        let mut samples = [Complex32::new(1.0, 0.0)];
        nco.apply_batch(&mut samples, freq_hz, sample_rate);
        // Expected phasor after one step: cos(-π/2) + i*sin(-π/2) = (0, -1)
        // Result: (1+0j)*(0-1j) = (0-1j)
        assert!(
            (samples[0].re).abs() < 1e-5,
            "re should be ≈0, got {}",
            samples[0].re
        );
        assert!(
            (samples[0].im - (-1.0)).abs() < 1e-5,
            "im should be ≈-1, got {}",
            samples[0].im
        );
    }

    #[test]
    fn renorm_keeps_phasor_near_unit() {
        // After many single-sample batches, the phasor norm should remain ≈1.0.
        let mut nco = Nco::new();
        let freq_hz = 12_345i32;
        let sample_rate = 2_048_000u32;
        for _ in 0..10_000 {
            let mut s = [Complex32::new(1.0, 0.0)];
            nco.apply_batch(&mut s, freq_hz, sample_rate);
        }
        // Access the phasor norm via a zero-sample batch (no-op, norm checked externally).
        // We apply one more batch and compute the expected norm.
        let mut probe = [Complex32::new(1.0, 0.0)];
        let phasor_before = nco.phasor;
        nco.apply_batch(&mut probe, 0, sample_rate); // freq=0 → step=1, phasor unchanged
        let norm = phasor_before.norm();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "phasor norm drifted to {:.6} (expected ≈1.0)",
            norm
        );
    }

    // TEST 1.1 (DoD) — CFO injection: phasor magnitude stays ≈1.0 for ±1/5/10 kHz
    // over a long burst, confirming renormalisation handles all common CFO values.
    #[test]
    fn cfo_injection_phasor_stays_unit_at_1khz() {
        let mut nco = Nco::new();
        let freq_hz = 1_000i32;
        let sample_rate = 2_048_000u32;
        // Process 200 000 samples (≈ 0.1 s worth at 2.048 Msps) in 1-sample batches.
        for _ in 0..200_000 {
            let mut s = [Complex32::new(1.0, 0.0)];
            nco.apply_batch(&mut s, freq_hz, sample_rate);
        }
        let norm = nco.phasor.norm();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "+1 kHz CFO: phasor norm drifted to {:.6} (expected ≈1.0)",
            norm
        );
    }

    #[test]
    fn cfo_injection_phasor_stays_unit_at_minus_5khz() {
        let mut nco = Nco::new();
        let freq_hz = -5_000i32;
        let sample_rate = 2_048_000u32;
        for _ in 0..200_000 {
            let mut s = [Complex32::new(1.0, 0.0)];
            nco.apply_batch(&mut s, freq_hz, sample_rate);
        }
        let norm = nco.phasor.norm();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "-5 kHz CFO: phasor norm drifted to {:.6} (expected ≈1.0)",
            norm
        );
    }

    #[test]
    fn cfo_injection_phasor_stays_unit_at_10khz() {
        let mut nco = Nco::new();
        let freq_hz = 10_000i32;
        let sample_rate = 2_048_000u32;
        for _ in 0..200_000 {
            let mut s = [Complex32::new(1.0, 0.0)];
            nco.apply_batch(&mut s, freq_hz, sample_rate);
        }
        let norm = nco.phasor.norm();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "+10 kHz CFO: phasor norm drifted to {:.6} (expected ≈1.0)",
            norm
        );
    }

    /// Helper to expose the internal phasor for testing; uses the zero-step
    /// property: apply_batch with freq=0 leaves the phasor unmoved, letting
    /// the calling test read it via `nco.phasor` if we temporarily make it pub.
    /// We work around this by checking the effect on a known input instead.
    #[test]
    fn reset_restores_unit_phasor() {
        let mut nco = Nco::new();
        let mut s = [Complex32::new(1.0, 0.0)];
        nco.apply_batch(&mut s, 100_000, 2_048_000);
        nco.reset();
        // After reset, freq=0 should leave a unit-real input unchanged.
        let mut probe = [Complex32::new(1.0, 0.0)];
        nco.apply_batch(&mut probe, 0, 2_048_000);
        // step = 1+0j, phasor = 1+0j → probe[0] = 1+0j
        let delta = -2.0 * PI * 0.0_f32 / 2_048_000.0_f32;
        let step = Complex32::from_polar(1.0, delta);
        let expected = Complex32::new(1.0, 0.0) * step; // = 1+0j
        assert!((probe[0].re - expected.re).abs() < 1e-5);
        assert!((probe[0].im - expected.im).abs() < 1e-5);
    }
}
