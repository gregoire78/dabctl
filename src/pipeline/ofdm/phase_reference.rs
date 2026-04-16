// DABstar-aligned phase-reference correlator for OFDM symbol timing.

use crate::pipeline::ofdm::phase_table::PhaseTable;
use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

/// Half-width of the correlation search window around T_g for `find_index()`.
///
/// The cyclic-prefix correlation peak is expected near sample offset T_g within
/// the T_u-sample input buffer (the guard interval sits at the front).  Limiting
/// the search to `[T_g − WINDOW_MARGIN, T_g + 2·WINDOW_MARGIN)` avoids false
/// peaks in the distant portion of the buffer while still allowing ±250-sample
/// acquisition jitter.
///
/// For Mode I: T_g = 504, window = [254, 1004].
/// For Mode II: T_g = 126, window = [0, 626] (lower bound clamped to 0).
/// For Mode III: T_g = 63, window = [0, 313] (lower bound clamped to 0).
/// For Mode IV: T_g = 252, window = [2, 752].
///
/// Adapted from DABstar's `PhaseReference::correlate_with_phase_ref_and_find_max_peak()`
/// (`idxStart = cTg − 250`, `idxStop = cTg + 500`) (GPLv2).
const WINDOW_MARGIN: usize = 250;

/// Forward look-ahead used to confirm that a threshold-crossing point is a
/// local peak rather than the rising edge of the same peak.
///
/// DABstar checks the next few correlation bins and only accepts the current
/// position if none of them is larger; this avoids locking too early on a
/// multipath peak cluster.
const GAP_SEARCH_WIDTH: usize = 10;

/// Confidence factor for the coarse-frequency-offset estimator.
///
/// DABstar accepts the estimate only if the strongest peak in the search window
/// is at least 5× the average level of that window.
const OFFSET_CONF_FACTOR: f32 = 5.0;

/// Number of carrier positions searched in `estimate_offset()`.
/// DABstar uses `SEARCHRANGE = 2 * 70`, i.e. a ±70-bin coarse search.
const SEARCH_RANGE: usize = 2 * 70;

pub struct PhaseReference {
    t_u: usize,
    t_g: usize,
    carrier_diff: i32,
    ref_table: Vec<Complex32>,
    coarse_ref_arg: Vec<Complex32>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
}

fn calculate_relative_phase(fft_in: &[Complex32]) -> Vec<Complex32> {
    let mut arg_out = vec![Complex32::new(0.0, 0.0); fft_in.len()];
    if fft_in.is_empty() {
        return arg_out;
    }

    for i in 0..fft_in.len().saturating_sub(1) {
        arg_out[i] = fft_in[i].conj() * fft_in[i + 1];
    }
    arg_out[fft_in.len() - 1] = Complex32::new(0.0, 0.0);
    arg_out
}

fn peak_index_in_window(
    fft_buffer: &[Complex32],
    win_start: usize,
    win_end: usize,
    threshold: i16,
) -> i32 {
    if win_start >= win_end || win_end > fft_buffer.len() {
        return -1;
    }

    let total_sum: f32 = fft_buffer.iter().map(|c| c.norm()).sum();
    if total_sum == 0.0 {
        return -1;
    }

    let window = &fft_buffer[win_start..win_end];
    let threshold_value = threshold as f32 * total_sum / fft_buffer.len() as f32;
    let mut max_val = f32::NEG_INFINITY;

    for (offset, sample) in window.iter().enumerate() {
        let value = sample.norm();
        if value > max_val {
            max_val = value;
        }

        if value < threshold_value {
            continue;
        }

        let mut is_local_peak = true;
        for look_ahead in 1..GAP_SEARCH_WIDTH {
            let Some(next_sample) = window.get(offset + look_ahead) else {
                break;
            };
            if next_sample.norm() > value {
                is_local_peak = false;
                break;
            }
        }

        if is_local_peak {
            return (win_start + offset) as i32;
        }
    }

    -(max_val * fft_buffer.len() as f32 / total_sum).abs() as i32 - 1
}

impl PhaseReference {
    pub const IDX_NOT_FOUND: i32 = 100_000;

    pub fn new(
        t_u: usize,
        t_g: usize,
        carriers: usize,
        carrier_diff: i32,
        mode: i16,
        _diff_length: usize,
    ) -> Self {
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

        let mut coarse_ref_arg = calculate_relative_phase(&ref_table);
        ifft.process(&mut coarse_ref_arg);
        let norm = 1.0 / t_u as f32;
        for sample in &mut coarse_ref_arg {
            *sample = sample.conj() * norm;
        }

        PhaseReference {
            t_u,
            t_g,
            carrier_diff,
            ref_table,
            coarse_ref_arg,
            fft,
            ifft,
        }
    }

    fn correlation_search_window(&self) -> (usize, usize) {
        let win_start = self.t_g.saturating_sub(WINDOW_MARGIN);
        let win_end = (self.t_g + 2 * WINDOW_MARGIN).min(self.t_u);
        (win_start, win_end)
    }

    fn correlate_with_phase_ref(&self, v: &[Complex32]) -> Vec<Complex32> {
        let mut fft_buffer = v[..self.t_u].to_vec();
        self.fft.process(&mut fft_buffer);

        for (fb, rt) in fft_buffer
            .iter_mut()
            .zip(self.ref_table.iter())
            .take(self.t_u)
        {
            *fb *= rt.conj();
        }

        self.ifft.process(&mut fft_buffer);

        let norm = 1.0 / self.t_u as f32;
        for sample in &mut fft_buffer {
            *sample *= norm;
        }

        fft_buffer
    }

    /// Find the first sample of the first non-null symbol by correlation.
    ///
    /// The search is restricted to the window `[T_g − WINDOW_MARGIN, T_g + 2·WINDOW_MARGIN)`
    /// (indices clamped to `[0, T_u)`) because the cyclic-prefix correlation peak
    /// is expected near T_g within the input buffer. The threshold is normalized
    /// against the mean over the full correlation buffer, which matches DABstar's
    /// `sum /= cTu` logic and avoids making tracking artificially harsher just
    /// because the search window is narrower than T_u.
    ///
    /// Adapted from DABstar's `correlate_with_phase_ref_and_find_max_peak()` (GPLv2).
    pub fn find_index(&self, v: &[Complex32], threshold: i16) -> i32 {
        let fft_buffer = self.correlate_with_phase_ref(v);
        let (win_start, win_end) = self.correlation_search_window();
        peak_index_in_window(&fft_buffer, win_start, win_end, threshold)
    }

    /// Estimate coarse frequency offset.
    ///
    /// This follows DABstar's sync-symbol-0 carrier-offset estimator:
    /// 1. FFT the candidate phase-reference symbol
    /// 2. form relative phase products between adjacent FFT bins
    /// 3. IFFT to time domain, multiply by the precomputed reference response
    /// 4. FFT back and pick the strongest peak in the ±70-bin coarse window
    pub fn estimate_offset(&self, v: &[Complex32]) -> i32 {
        let mut fft_buffer = v[..self.t_u].to_vec();
        self.fft.process(&mut fft_buffer);

        let mut corr = calculate_relative_phase(&fft_buffer);
        self.ifft.process(&mut corr);
        let norm = 1.0 / self.t_u as f32;
        for (sample, ref_arg) in corr.iter_mut().zip(self.coarse_ref_arg.iter()) {
            *sample = (*sample * norm) * *ref_arg;
        }
        self.fft.process(&mut corr);

        let half = (SEARCH_RANGE / 2) as i32;
        let mut index: i32 = 0;
        let mut max_value = 0.0f32;
        let mut avg = 0.0f32;

        for i in -half..=half {
            let idx = (self.t_u as i32 + i).rem_euclid(self.t_u as i32) as usize;
            let value = corr[idx].norm();
            if value > max_value {
                max_value = value;
                index = i;
            }
            avg += value;
        }
        avg /= (SEARCH_RANGE + 1) as f32;

        if avg == 0.0 || max_value < avg * OFFSET_CONF_FACTOR {
            return Self::IDX_NOT_FOUND;
        }

        let mut peak = [0.0f32; 3];
        let mut peak_sum = 0.0f32;
        for (n, slot) in peak.iter_mut().enumerate() {
            let delta = n as i32 - 1;
            let idx = (self.t_u as i32 + index + delta).rem_euclid(self.t_u as i32) as usize;
            *slot = corr[idx].norm();
            peak_sum += *slot;
        }

        let offset = if peak_sum > 0.0 {
            index as f32 + (peak[2] - peak[0]) / peak_sum
        } else {
            index as f32
        };

        (offset * self.carrier_diff as f32) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::dab_params::DabParams;

    fn make_phase_ref(mode: u8) -> PhaseReference {
        let p = DabParams::new(mode);
        PhaseReference::new(
            p.t_u as usize,
            p.t_g as usize,
            p.k as usize,
            p.carrier_diff,
            p.dab_mode,
            50,
        )
    }

    // ── find_index window ─────────────────────────────────────────────────────

    /// With an all-zero input the correlation peak is zero everywhere;
    /// `find_index` must return a negative value (below-threshold sentinel).
    #[test]
    fn find_index_all_zero_returns_negative() {
        let pr = make_phase_ref(1);
        let t_u = DabParams::new(1).t_u as usize;
        let v = vec![Complex32::new(0.0, 0.0); t_u];
        let result = pr.find_index(&v, 3);
        assert!(
            result < 0,
            "all-zero input should give negative result, got {result}"
        );
    }

    /// The search window must be strictly inside [0, T_u).
    /// For Mode I (T_g=504, T_u=2048): [254, 1004].
    /// For Mode II (T_g=126, T_u=512): [0, 626) → clamped to [0, 512).
    #[test]
    fn find_index_window_bounds_mode1() {
        let p = DabParams::new(1);
        let t_g = p.t_g as usize;
        let t_u = p.t_u as usize;
        let win_start = t_g.saturating_sub(WINDOW_MARGIN);
        let win_end = (t_g + 2 * WINDOW_MARGIN).min(t_u);
        assert!(win_start < win_end);
        assert!(win_end <= t_u);
        assert_eq!(win_start, 254);
        assert_eq!(win_end, 1004);
    }

    #[test]
    fn find_index_window_bounds_mode2() {
        let p = DabParams::new(2);
        let t_g = p.t_g as usize;
        let t_u = p.t_u as usize;
        let win_start = t_g.saturating_sub(WINDOW_MARGIN);
        let win_end = (t_g + 2 * WINDOW_MARGIN).min(t_u);
        assert!(win_start < win_end);
        assert!(win_end <= t_u);
        assert_eq!(win_start, 0); // 126 < 250, saturating_sub clamps to 0
    }

    // ── estimate_offset confidence + sub-bin ──────────────────────────────────

    /// With an all-zero input there is no useful signal; the confidence check
    /// should fire and return the IDX_NOT_FOUND sentinel (100000).
    #[test]
    fn estimate_offset_all_zero_returns_not_found() {
        let pr = make_phase_ref(1);
        let t_u = DabParams::new(1).t_u as usize;
        let v = vec![Complex32::new(0.0, 0.0); t_u];
        assert_eq!(
            pr.estimate_offset(&v),
            PhaseReference::IDX_NOT_FOUND,
            "all-zero → IDX_NOT_FOUND"
        );
    }

    /// The return value must always be in [−70 kHz, 70 kHz] or equal to the sentinel 100000.
    #[test]
    fn estimate_offset_return_range() {
        use std::f32::consts::PI;
        let pr = make_phase_ref(1);
        let t_u = DabParams::new(1).t_u as usize;
        // Use a rotating phasor as a simple non-zero signal.
        let v: Vec<Complex32> = (0..t_u)
            .map(|n| Complex32::from_polar(1.0, 2.0 * PI * 5.0 * n as f32 / t_u as f32))
            .collect();
        let r = pr.estimate_offset(&v);
        assert!(
            r == PhaseReference::IDX_NOT_FOUND || (-70_000..=70_000).contains(&r),
            "result {r} out of expected range"
        );
    }

    #[test]
    fn estimate_offset_detects_large_carrier_shift_within_dabstar_window() {
        let pr = make_phase_ref(1);
        let t_u = DabParams::new(1).t_u as usize;

        let mut shifted_ref = vec![Complex32::new(0.0, 0.0); t_u];
        let shift: isize = 50;
        for (idx, sample) in pr.ref_table.iter().copied().enumerate() {
            let dst = ((idx as isize + shift).rem_euclid(t_u as isize)) as usize;
            shifted_ref[dst] = sample;
        }

        let mut planner = FftPlanner::new();
        let ifft = planner.plan_fft_inverse(t_u);
        ifft.process(&mut shifted_ref);
        let norm = 1.0 / t_u as f32;
        for sample in &mut shifted_ref {
            *sample *= norm;
        }

        let estimated = pr.estimate_offset(&shifted_ref);
        assert!(
            estimated != 100,
            "large but valid coarse shift should be detected"
        );
        let expected_hz = shift as i32 * DabParams::new(1).carrier_diff;
        assert!(
            (estimated - expected_hz).abs() <= 2_000,
            "expected coarse shift about {expected_hz} Hz, got {estimated}"
        );
    }

    #[test]
    fn peak_index_prefers_first_valid_peak() {
        let mut corr = vec![Complex32::new(0.0, 0.0); 64];
        corr[10] = Complex32::new(3.0, 0.0);
        corr[11] = Complex32::new(2.5, 0.0);
        corr[30] = Complex32::new(5.0, 0.0);
        corr[31] = Complex32::new(4.5, 0.0);

        assert_eq!(peak_index_in_window(&corr, 0, 64, 2), 10);
    }

    #[test]
    fn peak_index_uses_first_local_peak_not_first_threshold_crossing() {
        let mut corr = vec![Complex32::new(0.0, 0.0); 64];
        corr[10] = Complex32::new(1.0, 0.0);
        corr[11] = Complex32::new(3.0, 0.0);
        corr[30] = Complex32::new(5.0, 0.0);
        corr[31] = Complex32::new(4.5, 0.0);

        assert_eq!(peak_index_in_window(&corr, 0, 64, 2), 11);
    }

    #[test]
    fn peak_index_uses_full_buffer_mean_for_strict_tracking_thresholds() {
        let mut corr = vec![Complex32::new(0.0, 0.0); 2048];
        for sample in corr.iter_mut().take(1004).skip(254) {
            *sample = Complex32::new(1.0, 0.0);
        }
        corr[504] = Complex32::new(3.0, 0.0);
        corr[505] = Complex32::new(2.0, 0.0);

        assert_eq!(peak_index_in_window(&corr, 254, 1004, 6), 504);
    }
}
