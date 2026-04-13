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
/// Each symbol is first normalised to the unit circle (`s / |s|`) before the
/// nearest QPSK point is looked up.  This makes the metric independent of the
/// per-carrier amplitude variation inherent to differential QPSK output (where
/// magnitude ≈ |H(f)|², not 1).
///
/// ```text
/// s_norm       = s / |s|
/// error_power  = mean(|s_norm − nearest_qpsk(s_norm)|²)
/// MER_dB       = −10 × log10(error_power)     (signal_power = 1 after norm)
/// ```
///
/// Returns `0.0` if `symbols` is empty, all zero, or `error_power` is near zero
/// (perfect constellation alignment).
pub fn estimate_mer(symbols: &[Complex32]) -> f32 {
    if symbols.is_empty() {
        return 0.0;
    }

    let mut error_sum = 0.0f32;
    let mut count = 0u32;

    for &s in symbols {
        let mag = s.norm();
        if mag < f32::EPSILON {
            continue;
        }
        let s_norm = s / mag;
        let ideal = nearest_qpsk(s_norm);
        error_sum += (s_norm - ideal).norm_sqr();
        count += 1;
    }

    if count == 0 {
        return 0.0;
    }

    let mean_error = error_sum / count as f32;
    if mean_error < f32::EPSILON {
        return 0.0;
    }

    // signal_power = 1 (normalised), so MER = 10 log10(1 / mean_error)
    -10.0 * mean_error.log10()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_1_SQRT_2;

    #[test]
    fn perfect_qpsk_returns_zero() {
        // Exact QPSK points on the unit circle → error_power ≈ 0, guard returns 0.0.
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
    fn scaled_qpsk_gives_same_mer_as_unit_qpsk() {
        // After normalisation, amplitude scaling must not affect the MER estimate.
        let unit_symbols = vec![
            Complex32::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            Complex32::new(-FRAC_1_SQRT_2, FRAC_1_SQRT_2),
        ];
        let noise = 0.05_f32;
        let noisy: Vec<Complex32> = unit_symbols
            .iter()
            .map(|&s| Complex32::new(s.re + noise, s.im + noise))
            .collect();
        let scaled_noisy: Vec<Complex32> = noisy.iter().map(|&s| s * 1000.0).collect();

        let mer_unit = estimate_mer(&noisy);
        let mer_scaled = estimate_mer(&scaled_noisy);
        assert!(
            (mer_unit - mer_scaled).abs() < 0.1,
            "MER should be amplitude-invariant; unit={:.2} scaled={:.2}",
            mer_unit,
            mer_scaled,
        );
    }

    #[test]
    fn near_perfect_qpsk_gives_high_mer() {
        // Symbols close to QPSK points should yield high MER (> 20 dB).
        let noise_scale = 0.02_f32;
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
            mer > 20.0,
            "near-perfect QPSK should give MER > 20 dB, got {:.1} dB",
            mer
        );
    }

    #[test]
    fn noisy_gives_low_mer() {
        // Symbols at 45° between QPSK neighbours have maximum distance from any ideal point.
        let symbols = vec![
            Complex32::new(0.0, 1.0),  // 90°  — equidistant from (+,+) and (-,+)
            Complex32::new(1.0, 0.0),  // 0°   — equidistant from (+,+) and (+,-)
            Complex32::new(0.0, -1.0), // 270° — equidistant from (+,-) and (-,-)
            Complex32::new(-1.0, 0.0), // 180° — equidistant from (-,+) and (-,-)
        ];
        let mer = estimate_mer(&symbols);
        assert!(
            mer < 10.0,
            "maximally noisy symbols should give MER < 10 dB, got {:.1} dB",
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
