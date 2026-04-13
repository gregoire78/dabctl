// MER estimation per ETSI TR 101 290

use num_complex::Complex32;

/// 1/√2 — the magnitude of each component of a unit-power QPSK symbol.
const INV_SQRT2: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Round `s` to the nearest QPSK constellation point (±1/√2, ±1/√2).
pub fn nearest_qpsk(s: Complex32) -> Complex32 {
    Complex32::new(
        if s.re >= 0.0 { INV_SQRT2 } else { -INV_SQRT2 },
        if s.im >= 0.0 { INV_SQRT2 } else { -INV_SQRT2 },
    )
}

/// Estimate MER (Modulation Error Ratio) in dB from post-differential-QPSK symbols.
///
/// ```text
/// signal_power = mean(|s|²)
/// error_power  = mean(|s − nearest_qpsk(s)|²)
/// MER_dB       = 10 × log10(signal_power / error_power)
/// ```
///
/// Returns `0.0` if `symbols` is empty or `error_power` is near zero
/// (perfect constellation alignment).
pub fn estimate_mer(symbols: &[Complex32]) -> f32 {
    if symbols.is_empty() {
        return 0.0;
    }

    let n = symbols.len() as f32;
    let signal_power = symbols.iter().map(|&s| s.norm_sqr()).sum::<f32>() / n;
    let error_power = symbols
        .iter()
        .map(|&s| {
            let ideal = nearest_qpsk(s);
            (s - ideal).norm_sqr()
        })
        .sum::<f32>()
        / n;

    if error_power < f32::EPSILON {
        return 0.0;
    }

    10.0 * (signal_power / error_power).log10()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_1_SQRT_2;

    #[test]
    fn perfect_qpsk_returns_zero_or_very_high() {
        // Exact QPSK points have error_power ≈ 0, triggering the near-zero guard.
        let symbols = vec![
            Complex32::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            Complex32::new(-FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            Complex32::new(FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
            Complex32::new(-FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
        ];
        let mer = estimate_mer(&symbols);
        assert_eq!(mer, 0.0, "perfect QPSK should return 0.0 (guard active)");
    }

    #[test]
    fn near_perfect_qpsk_gives_high_mer() {
        // QPSK symbols with 1% Gaussian-like noise should give MER > 30 dB.
        let noise_scale = 0.01_f32;
        let symbols: Vec<Complex32> = [
            (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            (-FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            (FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
            (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
        ]
        .iter()
        .map(|&(re, im)| Complex32::new(re + noise_scale, im + noise_scale))
        .collect();

        let mer = estimate_mer(&symbols);
        assert!(
            mer > 30.0,
            "near-perfect QPSK should give MER > 30 dB, got {:.1} dB",
            mer
        );
    }

    #[test]
    fn noisy_gives_low_mer() {
        // Purely random noise concentrated away from the QPSK ideal points
        // should give low MER (well below 10 dB).
        // We construct symbols at the midpoints between QPSK neighbours —
        // these have maximum distance from any ideal point.
        let symbols = vec![
            Complex32::new(0.0, FRAC_1_SQRT_2), // equidistant from (±1/√2, 1/√2)
            Complex32::new(FRAC_1_SQRT_2, 0.0), // equidistant from (1/√2, ±1/√2)
            Complex32::new(0.0, -FRAC_1_SQRT_2), // equidistant from (±1/√2, -1/√2)
            Complex32::new(-FRAC_1_SQRT_2, 0.0), // equidistant from (-1/√2, ±1/√2)
        ];
        let mer = estimate_mer(&symbols);
        assert!(
            mer < 10.0,
            "noisy symbols should give MER < 10 dB, got {:.1} dB",
            mer
        );
    }

    #[test]
    fn single_known_point_test() {
        // (INV_SQRT2, 0.001) → im > 0, so nearest_qpsk returns (INV_SQRT2, INV_SQRT2).
        let s = Complex32::new(FRAC_1_SQRT_2, 0.001);
        let nearest = nearest_qpsk(s);
        assert_eq!(
            nearest,
            Complex32::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            "nearest_qpsk({:?}) should be (INV_SQRT2, INV_SQRT2)",
            s
        );
    }

    #[test]
    fn empty_slice_returns_zero() {
        assert_eq!(estimate_mer(&[]), 0.0);
    }
}
