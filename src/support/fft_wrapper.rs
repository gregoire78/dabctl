// ==============================================================================
// support/fft_wrapper.rs - FFT processor abstraction
// ==============================================================================

use num_complex::Complex;
use rustfft::FftPlanner;
use std::sync::Mutex;

/// Tailles FFT supportées
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FftSize {
    Size256 = 256,
    Size512 = 512,
    Size1024 = 1024,
    Size2048 = 2048,
}

impl FftSize {
    pub fn as_usize(&self) -> usize {
        *self as usize
    }
}

/// Processeur FFT avec mise en cache des plans FFT
pub struct FftProcessor {
    fft_planner: Mutex<FftPlanner<f32>>,
}

impl FftProcessor {
    /// Créer un nouveau processeur FFT
    pub fn new() -> Self {
        Self {
            fft_planner: Mutex::new(FftPlanner::new()),
        }
    }

    /// Effectuer une FFT avant
    pub fn fft_forward(&self, input: &[Complex<f32>]) -> anyhow::Result<Vec<Complex<f32>>> {
        if input.is_empty() {
            return Ok(Vec::new());
        }

        let mut planner = self.fft_planner.lock().unwrap();
        let fft = planner.plan_fft_forward(input.len());

        let mut output = input.to_vec();
        fft.process(&mut output);

        Ok(output)
    }

    /// Effectuer une FFT inverse
    pub fn fft_inverse(&self, input: &[Complex<f32>]) -> anyhow::Result<Vec<Complex<f32>>> {
        if input.is_empty() {
            return Ok(Vec::new());
        }

        let mut planner = self.fft_planner.lock().unwrap();
        let fft = planner.plan_fft_inverse(input.len());

        let mut output = input.to_vec();
        fft.process(&mut output);

        // Normaliser par la taille
        let norm = input.len() as f32;
        for sample in &mut output {
            *sample /= norm;
        }

        Ok(output)
    }
}

impl Default for FftProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_fft_forward_single_tone() {
        let fft = FftProcessor::new();

        // Créer une sinusoïde simple
        let len = 256;
        let freq = 1.0; // 1 bin
        let mut input = Vec::new();
        for i in 0..len {
            let phase = 2.0 * PI * freq * i as f32 / len as f32;
            input.push(Complex::new(phase.cos(), phase.sin()));
        }

        let output = fft.fft_forward(&input).unwrap();
        assert_eq!(output.len(), len);
    }

    #[test]
    fn test_fft_ifft_round_trip() {
        let fft = FftProcessor::new();

        let input = vec![
            Complex::new(1.0, 0.0),
            Complex::new(0.5, 0.0),
            Complex::new(0.25, 0.0),
            Complex::new(0.125, 0.0),
        ];

        let fft_result = fft.fft_forward(&input).unwrap();
        let ifft_result = fft.fft_inverse(&fft_result).unwrap();

        // Vérifier approximativement l'identité
        for (orig, recovered) in input.iter().zip(ifft_result.iter()) {
            let diff = (orig - recovered).norm();
            assert!(diff < 1e-5, "IFT round-trip failed: {} vs {}", orig, recovered);
        }
    }
}
