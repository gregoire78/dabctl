use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

/// Buffer-reuse FFT wrapper.
///
/// Allocates a single internal buffer at construction time and reuses it on
/// every call, eliminating per-call heap allocation in the hot path.
pub struct FftEngine {
    fft: Arc<dyn Fft<f32>>,
    buffer: Vec<Complex32>,
    inverse: bool,
}

impl FftEngine {
    /// Create a forward FFT engine for the given transform size.
    pub fn new_forward(size: usize) -> Self {
        let mut planner = FftPlanner::new();
        FftEngine {
            fft: planner.plan_fft_forward(size),
            buffer: vec![Complex32::new(0.0, 0.0); size],
            inverse: false,
        }
    }

    /// Create an inverse FFT engine for the given transform size.
    pub fn new_inverse(size: usize) -> Self {
        let mut planner = FftPlanner::new();
        FftEngine {
            fft: planner.plan_fft_inverse(size),
            buffer: vec![Complex32::new(0.0, 0.0); size],
            inverse: true,
        }
    }

    /// Copy `data` into internal buffer, run FFT, return result slice.
    ///
    /// Inverse FFT normalises by 1/N (rustfft does not normalise automatically).
    pub fn process(&mut self, data: &[Complex32]) -> &[Complex32] {
        self.buffer.copy_from_slice(data);
        self.fft.process(&mut self.buffer);
        if self.inverse {
            let n = self.buffer.len() as f32;
            for s in self.buffer.iter_mut() {
                *s /= n;
            }
        }
        &self.buffer
    }

    /// Process `data` and write the result into `out` (avoids borrow conflicts
    /// in callers that hold other references into the same struct).
    pub fn process_into(&mut self, data: &[Complex32], out: &mut [Complex32]) {
        let result = self.process(data);
        out.copy_from_slice(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: Complex32, b: Complex32, eps: f32) -> bool {
        (a.re - b.re).abs() < eps && (a.im - b.im).abs() < eps
    }

    #[test]
    fn round_trip_forward_inverse_identity() {
        // Forward then inverse of an 8-point signal should recover the original.
        let original: Vec<Complex32> = (0..8)
            .map(|i| Complex32::new(i as f32 * 0.5 - 1.0, (i as f32 * 0.3).sin()))
            .collect();

        let mut fwd = FftEngine::new_forward(8);
        let mut inv = FftEngine::new_inverse(8);

        let freq_domain = fwd.process(&original).to_vec();
        let recovered = inv.process(&freq_domain).to_vec();

        for (r, o) in recovered.iter().zip(original.iter()) {
            assert!(
                approx_eq(*r, *o, 1e-5),
                "round-trip mismatch: got {:?}, expected {:?}",
                r,
                o
            );
        }
    }

    #[test]
    fn known_4point_fft() {
        // FFT of [1, 0, 0, 0] = [1, 1, 1, 1] (DC impulse spreads uniformly).
        let input = vec![
            Complex32::new(1.0, 0.0),
            Complex32::new(0.0, 0.0),
            Complex32::new(0.0, 0.0),
            Complex32::new(0.0, 0.0),
        ];
        let mut engine = FftEngine::new_forward(4);
        let out = engine.process(&input).to_vec();
        for (i, s) in out.iter().enumerate() {
            assert!(
                approx_eq(*s, Complex32::new(1.0, 0.0), 1e-5),
                "bin {}: expected (1,0), got {:?}",
                i,
                s
            );
        }
    }

    #[test]
    fn process_into_matches_process() {
        let input: Vec<Complex32> = (0..8).map(|i| Complex32::new(i as f32, 0.0)).collect();
        let mut engine_a = FftEngine::new_forward(8);
        let mut engine_b = FftEngine::new_forward(8);

        let expected = engine_a.process(&input).to_vec();
        let mut got = vec![Complex32::new(0.0, 0.0); 8];
        engine_b.process_into(&input, &mut got);

        for (e, g) in expected.iter().zip(got.iter()) {
            assert!(approx_eq(*e, *g, 1e-6));
        }
    }
}
