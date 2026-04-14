// ETSI EN 300 401 §14.5 — differential QPSK demodulation

use crate::pipeline::dab_constants::jan_abs;
use num_complex::Complex32;

/// Differential QPSK demodulator for one OFDM data symbol.
///
/// Maintains a per-bin phase reference (indexed by FFT bin, 0..t_u) updated
/// on each call. The post-differential symbols are accumulated in `r1_buf`
/// (carrier-indexed) for use by downstream MER estimation.
pub struct BlockDemod {
    carriers: usize,
    /// Per-bin differential phase reference; indexed by FFT bin (0..t_u).
    reference_phase: Vec<Complex32>,
    /// Scratch buffer for post-differential symbols (carrier-indexed).
    r1_buf: Vec<Complex32>,
}

impl BlockDemod {
    /// Create a new demodulator.
    ///
    /// `carriers` – number of OFDM data carriers (K).
    /// `t_u`      – FFT size (used to size the reference phase table).
    pub fn new(carriers: usize, t_u: usize) -> Self {
        BlockDemod {
            carriers,
            reference_phase: vec![Complex32::new(0.0, 0.0); t_u],
            r1_buf: vec![Complex32::new(0.0, 0.0); carriers],
        }
    }

    /// Demodulate one OFDM symbol into soft bits (differential QPSK).
    ///
    /// - `fft_out`  – full FFT output (t_u bins)
    /// - `freq_map` – signed carrier-to-bin mapping from FreqInterleaver (length = carriers)
    /// - `t_u`      – FFT size (used for negative-index wrap)
    /// - `ibits`    – output soft bits (length = 2×carriers): `[I bits | Q bits]`
    ///
    /// ETSI EN 300 401 §14.5
    pub fn process(
        &mut self,
        fft_out: &[Complex32],
        freq_map: &[i16],
        t_u: usize,
        ibits: &mut [i16],
    ) {
        for i in 0..self.carriers {
            let raw_idx = freq_map[i] as i32;
            let index = if raw_idx < 0 {
                (raw_idx + t_u as i32) as usize
            } else {
                raw_idx as usize
            };

            let r1 = fft_out[index] * self.reference_phase[index].conj();
            // Update reference for the next symbol's differential decode.
            self.reference_phase[index] = fft_out[index];
            self.r1_buf[i] = r1;

            let ab1 = jan_abs(r1);
            if ab1 > 0.0 {
                ibits[i] = (-r1.re / ab1 * 127.0).clamp(-127.0, 127.0) as i16;
                ibits[self.carriers + i] = (-r1.im / ab1 * 127.0).clamp(-127.0, 127.0) as i16;
            } else {
                ibits[i] = 0;
                ibits[self.carriers + i] = 0;
            }
        }
    }

    /// Zero all reference phase entries (call before first data symbol).
    pub fn reset_reference(&mut self) {
        self.reference_phase.fill(Complex32::new(0.0, 0.0));
    }

    /// Set reference phase from a full FFT output slice (call after block 0 FFT).
    ///
    /// Copies all `t_u` bins; only the bins indexed by `freq_map` are actually
    /// used during `process()`, so the others are harmless overhead.
    pub fn set_reference_from_fft(&mut self, fft_out: &[Complex32]) {
        self.reference_phase.copy_from_slice(fft_out);
    }

    /// Post-differential symbols for MER estimation (carrier-indexed).
    pub fn r1_buf(&self) -> &[Complex32] {
        &self.r1_buf
    }

    /// Mutable access to post-differential symbols (for in-place equalisation).
    pub fn r1_buf_mut(&mut self) -> &mut [Complex32] {
        &mut self.r1_buf
    }

    /// Regenerate soft bits from the current `r1_buf`.
    ///
    /// Call this after in-place equalisation of `r1_buf_mut()` to propagate
    /// the equalised symbols to `ibits` without re-running differential demod.
    pub fn recompute_ibits(&self, ibits: &mut [i16]) {
        for i in 0..self.carriers {
            let r1 = self.r1_buf[i];
            let ab1 = jan_abs(r1);
            if ab1 > 0.0 {
                ibits[i] = (-r1.re / ab1 * 127.0).clamp(-127.0, 127.0) as i16;
                ibits[self.carriers + i] = (-r1.im / ab1 * 127.0).clamp(-127.0, 127.0) as i16;
            } else {
                ibits[i] = 0;
                ibits[self.carriers + i] = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_1_SQRT_2;

    /// Build a minimal freq_map: carrier i → bin i+1 (skip DC at 0).
    fn simple_freq_map(carriers: usize) -> Vec<i16> {
        (1..=carriers as i16).collect()
    }

    #[test]
    fn identity_reference_gives_zero_phase_change() {
        // If we set the reference to the current FFT output and immediately
        // process the same output again, the differential phase is 0° for every
        // carrier. For a non-zero amplitude, this puts ibits near max positive.
        let carriers = 4;
        let t_u = 8;
        let freq_map = simple_freq_map(carriers);

        let fft_out: Vec<Complex32> = (0..t_u)
            .map(|i| Complex32::new((i + 1) as f32, 0.0))
            .collect();

        let mut demod = BlockDemod::new(carriers, t_u);
        demod.set_reference_from_fft(&fft_out);

        let mut ibits = vec![0i16; 2 * carriers];
        demod.process(&fft_out, &freq_map, t_u, &mut ibits);

        // After zero phase rotation: r1 = fft[i] * fft[i].conj() = |fft[i]|² (real, positive).
        // ibits[i] = -r1.re / |r1| * 127 = -127 (negative means aligned in-phase).
        // (Signal is real and positive → DQPSK I-channel bit is maximum magnitude negative.)
        for i in 0..carriers {
            assert!(
                ibits[i].abs() > 100,
                "ibits[{}] = {} should be near ±127",
                i,
                ibits[i]
            );
            // Q channel should be ≈0 (no imaginary component).
            assert!(
                ibits[carriers + i].abs() < 10,
                "ibits[carriers+{}] = {} should be near 0",
                i,
                ibits[carriers + i]
            );
        }
    }

    #[test]
    fn known_qpsk_symbol_soft_bits() {
        // A QPSK point in the first quadrant (positive re, positive im):
        // after differential demodulation against a unit-real reference,
        // ibits[I] should be negative (positive real → ibits = -re/|r| * 127 < 0)
        // ibits[Q] should be negative (positive imag → ibits = -im/|r| * 127 < 0).
        let carriers = 1;
        let t_u = 4;
        let freq_map = vec![1i16];

        let reference = vec![
            Complex32::new(1.0, 0.0),
            Complex32::new(1.0, 0.0), // bin 1 = reference
            Complex32::new(0.0, 0.0),
            Complex32::new(0.0, 0.0),
        ];
        // First-quadrant QPSK symbol: (1/√2, 1/√2)
        let fft_out = vec![
            Complex32::new(0.0, 0.0),
            Complex32::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            Complex32::new(0.0, 0.0),
            Complex32::new(0.0, 0.0),
        ];

        let mut demod = BlockDemod::new(carriers, t_u);
        demod.set_reference_from_fft(&reference);
        let mut ibits = vec![0i16; 2 * carriers];
        demod.process(&fft_out, &freq_map, t_u, &mut ibits);

        // r1 = (FRAC_1_SQRT_2 + i*FRAC_1_SQRT_2) * (1+0j).conj() = FRAC_1_SQRT_2 + i*FRAC_1_SQRT_2
        // ab1 = jan_abs(r1) = L1 (Manhattan) norm = |re| + |im| = FRAC_1_SQRT_2 + FRAC_1_SQRT_2 = √2
        // ibits[I] = -(FRAC_1_SQRT_2 / √2) * 127 ≈ -90 (negative)
        // ibits[Q] = -(FRAC_1_SQRT_2 / √2) * 127 ≈ -90 (negative)
        assert!(
            ibits[0] < -50,
            "I soft bit should be < -50, got {}",
            ibits[0]
        );
        assert!(
            ibits[1] < -50,
            "Q soft bit should be < -50, got {}",
            ibits[1]
        );
    }

    #[test]
    fn zero_fft_output_yields_zero_ibits() {
        let carriers = 4;
        let t_u = 8;
        let freq_map = simple_freq_map(carriers);
        let fft_out = vec![Complex32::new(0.0, 0.0); t_u];

        let mut demod = BlockDemod::new(carriers, t_u);
        let mut ibits = vec![99i16; 2 * carriers]; // non-zero sentinel
        demod.process(&fft_out, &freq_map, t_u, &mut ibits);

        for &bit in &ibits {
            assert_eq!(bit, 0, "all ibits should be 0 for zero FFT output");
        }
    }

    // TEST 5.1 (DoD) — Softbits always bounded to [-127, 127].
    //
    // Feed various signal amplitudes (including extreme values) and verify
    // that every soft bit produced by the DQPSK demodulator stays in [-127, 127].
    #[test]
    fn softbits_bounded_at_127_for_any_amplitude() {
        let carriers = 4;
        let t_u = 8;
        let freq_map = simple_freq_map(carriers);

        // Test several signal amplitudes including very large values.
        for amplitude in &[0.001f32, 1.0, 10.0, 1000.0, 1e6] {
            let reference: Vec<Complex32> = (0..t_u)
                .map(|i| {
                    if i > 0 && i <= carriers {
                        Complex32::new(*amplitude, 0.0)
                    } else {
                        Complex32::new(0.0, 0.0)
                    }
                })
                .collect();
            let fft_out: Vec<Complex32> = (0..t_u)
                .map(|i| {
                    if i > 0 && i <= carriers {
                        Complex32::new(*amplitude * FRAC_1_SQRT_2, *amplitude * FRAC_1_SQRT_2)
                    } else {
                        Complex32::new(0.0, 0.0)
                    }
                })
                .collect();

            let mut demod = BlockDemod::new(carriers, t_u);
            demod.set_reference_from_fft(&reference);
            let mut ibits = vec![0i16; 2 * carriers];
            demod.process(&fft_out, &freq_map, t_u, &mut ibits);

            for (idx, &bit) in ibits.iter().enumerate() {
                assert!(
                    bit >= -127 && bit <= 127,
                    "softbits[{}] = {} out of bounds [-127, 127] at amplitude {}",
                    idx,
                    bit,
                    amplitude
                );
            }
        }
    }

    // TEST 5.1 (DoD) — Softbits symmetric for I and Q components.
    //
    // With a 45° QPSK signal relative to a real-valued reference, the
    // differential demodulator produces equal I and Q components, confirming
    // symmetric handling of both channels.
    #[test]
    fn softbits_symmetric_iq_for_qpsk() {
        // Use a unit-real reference and a 45° QPSK data symbol.
        // r1 = (1/√2 + i/√2) * conj(1+0j) = 1/√2 + i/√2
        // ab1 = |1/√2| + |1/√2| = √2
        // ibits[I] = -(1/√2)/√2 * 127 = -0.5 * 127 ≈ -64
        // ibits[Q] = -(1/√2)/√2 * 127 = -0.5 * 127 ≈ -64  (equal to I)
        let carriers = 4;
        let t_u = 8;
        let freq_map = simple_freq_map(carriers);

        let reference: Vec<Complex32> = (0..t_u)
            .map(|i| {
                if i > 0 && i <= carriers {
                    Complex32::new(1.0, 0.0) // unit-real reference
                } else {
                    Complex32::new(0.0, 0.0)
                }
            })
            .collect();
        // Data: 45° QPSK symbol (first quadrant).
        let fft_out: Vec<Complex32> = (0..t_u)
            .map(|i| {
                if i > 0 && i <= carriers {
                    Complex32::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2)
                } else {
                    Complex32::new(0.0, 0.0)
                }
            })
            .collect();

        let mut demod = BlockDemod::new(carriers, t_u);
        demod.set_reference_from_fft(&reference);
        let mut ibits = vec![0i16; 2 * carriers];
        demod.process(&fft_out, &freq_map, t_u, &mut ibits);

        // All soft bits must be in [-127, 127].
        for &bit in &ibits {
            assert!(
                bit >= -127 && bit <= 127,
                "softbits must be in [-127, 127], got {}",
                bit
            );
        }
        // I and Q halves should be equal: same I and Q differential phase.
        for i in 0..carriers {
            assert_eq!(
                ibits[i], ibits[carriers + i],
                "I softbit[{}]={} should equal Q softbit at 45° input",
                i, ibits[i]
            );
        }
    }
}