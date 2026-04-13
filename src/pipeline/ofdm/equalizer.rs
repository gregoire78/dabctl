// ETSI EN 300 401 — channel equalisation
// Implements decision-directed LMS (Least Mean Squares) per OFDM carrier.

use num_complex::Complex32;

/// 1/√2 — the magnitude of each component of a unit-power QPSK symbol.
const INV_SQRT2: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Round `s` to the nearest QPSK constellation point (±1/√2, ±1/√2).
#[inline]
fn nearest_qpsk(s: Complex32) -> Complex32 {
    Complex32::new(
        if s.re >= 0.0 { INV_SQRT2 } else { -INV_SQRT2 },
        if s.im >= 0.0 { INV_SQRT2 } else { -INV_SQRT2 },
    )
}

/// Adaptive channel equalizer — one LMS weight per OFDM carrier.
///
/// Decision-directed: after equalising a symbol, the "desired" signal is the
/// nearest ideal QPSK point. The error drives the weight update, allowing the
/// equalizer to track slow channel variations.
pub struct Equalizer {
    weights: Vec<Complex32>,
    /// Pre-computed `Complex32::new(mu, 0.0)` to avoid per-carrier construction.
    mu_c: Complex32,
}

impl Equalizer {
    /// Create a new equalizer with all weights initialised to 1+0j.
    ///
    /// `carriers` – number of OFDM data carriers.
    /// `mu`       – LMS step size (typical value 0.01; larger = faster but noisier).
    pub fn new(carriers: usize, mu: f32) -> Self {
        Equalizer {
            weights: vec![Complex32::new(1.0, 0.0); carriers],
            mu_c: Complex32::new(mu, 0.0),
        }
    }

    /// Apply LMS equalisation in-place.
    ///
    /// For each carrier i:
    /// 1. Equalise: `out = symbols[i] * weights[i]`
    /// 2. Decide:   `desired = nearest_qpsk(out)`
    /// 3. Error:    `error = desired - out`
    /// 4. Update:   `weights[i] += mu × error × symbols[i].conj()`
    /// 5. Replace:  `symbols[i] = out`
    pub fn equalize(&mut self, symbols: &mut [Complex32]) {
        for (sym, w) in symbols.iter_mut().zip(self.weights.iter_mut()) {
            let out = *sym * *w;
            let desired = nearest_qpsk(out);
            let error = desired - out;
            *w += self.mu_c * error * sym.conj();
            *sym = out;
        }
    }

    /// Reset all weights to 1+0j.
    pub fn reset(&mut self) {
        self.weights.fill(Complex32::new(1.0, 0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_1_SQRT_2;

    #[test]
    fn unity_weights_leave_signal_unchanged() {
        // With mu=0.0 (no weight update) and weights = 1+0j, equalize is a no-op.
        let carriers = 4;
        let original: Vec<Complex32> = vec![
            Complex32::new(0.7, 0.3),
            Complex32::new(-0.5, 0.5),
            Complex32::new(0.1, -0.9),
            Complex32::new(-0.6, -0.4),
        ];
        let mut symbols = original.clone();
        let mut eq = Equalizer::new(carriers, 0.0);
        eq.equalize(&mut symbols);

        for (s, o) in symbols.iter().zip(original.iter()) {
            assert!(
                (s.re - o.re).abs() < 1e-6,
                "re changed: {} vs {}",
                s.re,
                o.re
            );
            assert!(
                (s.im - o.im).abs() < 1e-6,
                "im changed: {} vs {}",
                s.im,
                o.im
            );
        }
    }

    #[test]
    fn weights_converge_toward_qpsk_constellation() {
        // Feed 200 identical QPSK symbols — weights should stay bounded
        // and the equalised output should remain in the QPSK ballpark.
        let carriers = 4;
        let qpsk = Complex32::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2);
        let mut eq = Equalizer::new(carriers, 0.01);

        for _ in 0..200 {
            let mut symbols = vec![qpsk; carriers];
            eq.equalize(&mut symbols);
            for s in &symbols {
                // Output magnitude should stay reasonable (no runaway).
                assert!(
                    s.norm() < 10.0,
                    "equalised symbol magnitude out of range: {}",
                    s.norm()
                );
            }
        }
        // Weights must remain finite and bounded.
        for (i, w) in eq.weights.iter().enumerate() {
            assert!(
                w.norm() < 100.0,
                "weight[{}] magnitude diverged: {}",
                i,
                w.norm()
            );
        }
    }

    #[test]
    fn reset_restores_unity_weights() {
        let carriers = 4;
        let mut eq = Equalizer::new(carriers, 0.05);
        let qpsk = Complex32::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2);
        for _ in 0..50 {
            let mut s = vec![qpsk; carriers];
            eq.equalize(&mut s);
        }
        eq.reset();
        assert_eq!(
            eq.weights[0],
            Complex32::new(1.0, 0.0),
            "weight[0] should be 1+0j after reset"
        );
        for w in &eq.weights {
            assert_eq!(*w, Complex32::new(1.0, 0.0));
        }
    }
}
