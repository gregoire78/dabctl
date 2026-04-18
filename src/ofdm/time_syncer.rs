use num_complex::Complex32;

const NULL_SYMBOL_SAMPLES: usize = 2_656;
const TU: usize = 2_048;
const TG: usize = 504;
const TS: usize = TU + TG;

// DABstar uses a short envelope average for null detection.
const WINDOW: usize = 50;

// DABstar-style frame synchronisation: find the null symbol by locating the
// contiguous low-power region, then refine the PRS start with cyclic-prefix
// correlation.
#[derive(Default)]
pub struct TimeSyncer {
    /// Last known PRS start index (used for sanity-checking).
    last_prs_start: Option<usize>,
}

impl TimeSyncer {
    pub fn push(&mut self, samples: &[Complex32]) -> Option<usize> {
        self.detect(samples, None, 128)
    }

    pub fn track_near(
        &mut self,
        samples: &[Complex32],
        expected_prs_start: usize,
        search_radius: usize,
    ) -> Option<usize> {
        self.detect(samples, Some(expected_prs_start), search_radius)
    }

    fn detect(
        &mut self,
        samples: &[Complex32],
        expected_prs_start: Option<usize>,
        search_radius: usize,
    ) -> Option<usize> {
        // Need at least one null symbol + one OFDM symbol worth of samples.
        if samples.len() < NULL_SYMBOL_SAMPLES + TS + WINDOW {
            return None;
        }

        // 1. Compute a DABstar-style envelope level rather than squared power.
        let level: Vec<f32> = samples.iter().map(|s| s.norm()).collect();

        // 2. Sliding-window average envelope.
        let n = level.len();
        let mut window_sum: f64 = level[..WINDOW].iter().map(|&p| p as f64).sum();
        let mut avg = vec![0.0f32; n];
        avg[WINDOW / 2] = (window_sum / WINDOW as f64) as f32;

        for i in 1..(n - WINDOW) {
            window_sum += level[i + WINDOW - 1] as f64 - level[i - 1] as f64;
            let center = i + WINDOW / 2;
            if center < n {
                avg[center] = (window_sum / WINDOW as f64) as f32;
            }
        }

        // 3. DABstar-style threshold edge detection: scan forward until the
        // short-term mean drops below 0.55 × s_level, then continue until it
        // rises above 0.75 × s_level again.
        const FRAME_SAMPLES: usize = 196_608;
        let global_search_end = n
            .min(FRAME_SAMPLES)
            .saturating_sub(NULL_SYMBOL_SAMPLES + TS);
        if global_search_end == 0 {
            return None;
        }

        let (search_start, search_end) = if let Some(expected_prs) = expected_prs_start {
            let expected_null = expected_prs.saturating_sub(NULL_SYMBOL_SAMPLES);
            let start = expected_null.saturating_sub(search_radius);
            let end = (expected_null + search_radius).min(global_search_end.saturating_sub(1));
            (start.min(end), end)
        } else {
            (0usize, global_search_end.saturating_sub(1))
        };

        let scan_start = search_start.max(WINDOW / 2);
        let scan_end = search_end.max(scan_start);

        let search_peak = avg[scan_start..=scan_end]
            .iter()
            .copied()
            .fold(0.0f32, f32::max);
        if search_peak <= 1.0e-12 {
            return expected_prs_start;
        }

        let start_threshold = 0.55 * search_peak;
        let end_threshold = 0.75 * search_peak;

        let mut dip_start = None;
        let mut i = scan_start;
        while i <= scan_end {
            if avg[i] <= start_threshold {
                dip_start = Some(i);
                break;
            }
            i += 1;
        }

        let Some(best_start) = dip_start else {
            return expected_prs_start;
        };

        let max_dip_end =
            (best_start + NULL_SYMBOL_SAMPLES + WINDOW).min(n.saturating_sub(TS).saturating_sub(1));
        let mut rise_index = None;
        let mut i = best_start;
        while i <= max_dip_end {
            if avg[i] >= end_threshold {
                rise_index = Some(i);
                break;
            }
            i += 1;
        }

        let Some(rise_index) = rise_index else {
            return expected_prs_start;
        };
        let approx_prs_start = rise_index;

        let null_end = (best_start + NULL_SYMBOL_SAMPLES).min(n);
        let null_avg =
            avg[best_start..null_end].iter().copied().sum::<f32>() / (null_end - best_start) as f32;
        let signal_start = approx_prs_start.min(n.saturating_sub(1));
        let signal_end = (signal_start + NULL_SYMBOL_SAMPLES).min(n);
        let signal_avg = if signal_end > signal_start {
            avg[signal_start..signal_end].iter().copied().sum::<f32>()
                / (signal_end - signal_start) as f32
        } else {
            search_peak
        };
        let dip_span = rise_index.saturating_sub(best_start);

        let locked_tracking = expected_prs_start.is_some();
        let ratio = if signal_avg > 1.0e-12 {
            null_avg / signal_avg
        } else {
            1.0
        };

        let dip_too_short = dip_span < (WINDOW * 4);
        let dip_too_weak = ratio > 0.90;

        // Preserve DABstar-style timing continuity: a transient or nearly flat
        // dip must not override the expected frame boundary, and during initial
        // acquisition it must not create a false first lock either.
        if dip_too_short || dip_too_weak {
            tracing::debug!(
                best_start,
                approx_prs_start,
                prs_start = expected_prs_start.unwrap_or(approx_prs_start),
                dip_span,
                null_avg = null_avg as f32,
                signal_avg = signal_avg as f32,
                ratio = ratio as f32,
                expected_prs = ?expected_prs_start,
                prev = ?self.last_prs_start,
                "time sync null detection"
            );
            if let Some(expected_prs) = expected_prs_start {
                self.last_prs_start = Some(expected_prs);
                return Some(expected_prs);
            }
            return None;
        }

        let refine_radius = if locked_tracking {
            search_radius.min(512)
        } else {
            128
        };
        let mut prs_start =
            refine_with_prefix_correlation(samples, approx_prs_start, refine_radius);

        if let Some(expected_prs) = expected_prs_start {
            let max_prs_shift = search_radius.min(1536);
            if (prs_start as isize - expected_prs as isize).unsigned_abs() > max_prs_shift {
                prs_start = expected_prs;
            }
        }

        tracing::debug!(
            best_start,
            approx_prs_start,
            prs_start,
            dip_span,
            null_avg = null_avg as f32,
            signal_avg = signal_avg as f32,
            ratio = ratio as f32,
            expected_prs = ?expected_prs_start,
            prev = ?self.last_prs_start,
            "time sync null detection"
        );

        self.last_prs_start = Some(prs_start);
        Some(prs_start)
    }
}

fn refine_with_prefix_correlation(
    samples: &[Complex32],
    approx_start: usize,
    search_radius: usize,
) -> usize {
    if samples.len() < approx_start + TU + TG {
        return approx_start.min(samples.len().saturating_sub(1));
    }

    let start_min = approx_start.saturating_sub(search_radius);
    let start_max = (approx_start + search_radius).min(samples.len().saturating_sub(TU + TG));

    let mut best_start = approx_start.clamp(start_min, start_max);
    let mut best_metric = f32::MIN;

    for start in start_min..=start_max {
        let mut corr = Complex32::new(0.0, 0.0);
        let mut energy = 0.0f32;

        for idx in 0..TG {
            let a = samples[start + idx];
            let b = samples[start + TU + idx];
            corr += a * b.conj();
            energy += a.norm_sqr() + b.norm_sqr();
        }

        let metric = if energy > 1.0e-9 {
            corr.norm_sqr() / energy
        } else {
            0.0
        };

        let is_better = metric > best_metric + 1.0e-9;
        let is_tie_but_closer = (metric - best_metric).abs() <= 1.0e-9
            && (start as isize - approx_start as isize).unsigned_abs()
                < (best_start as isize - approx_start as isize).unsigned_abs();

        if is_better || is_tie_but_closer {
            best_metric = metric;
            best_start = start;
        }
    }

    best_start
}

#[cfg(test)]
mod tests {
    use num_complex::Complex32;

    use super::TimeSyncer;

    #[test]
    fn locates_symbol_after_null_region() {
        let mut samples = vec![Complex32::new(1.0, 0.0); 196_608];
        // Insert a null symbol at position 1000.
        for sample in &mut samples[1000..(1000 + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }

        let mut syncer = TimeSyncer::default();
        let start = syncer.push(&samples).expect("time sync should be found");
        // PRS starts after the null: approximately 1000 + 2656 = 3656.
        assert!(
            (3500..3800).contains(&start),
            "expected PRS start near 3656, got {}",
            start
        );
    }

    #[test]
    fn rejects_capture_without_real_null_dip() {
        let samples = vec![Complex32::new(1.0, 0.0); 196_608];

        let mut syncer = TimeSyncer::default();
        let start = syncer.push(&samples);
        assert!(start.is_none());
    }

    #[test]
    fn rejects_short_weak_startup_dip() {
        let mut samples = vec![Complex32::new(1.0, 0.0); 196_608];
        for sample in &mut samples[500..620] {
            *sample = Complex32::new(0.18, 0.0);
        }

        let mut syncer = TimeSyncer::default();
        let start = syncer.push(&samples);
        assert!(start.is_none(), "false startup dip must not acquire sync");
    }

    #[test]
    fn consistent_across_frames() {
        // Two consecutive frames with the same null position should give consistent results.
        let mut syncer = TimeSyncer::default();

        let make_frame = |null_pos: usize| -> Vec<Complex32> {
            let mut s = vec![Complex32::new(1.0, 0.5); 196_608];
            for sample in &mut s[null_pos..(null_pos + 2656)] {
                *sample = Complex32::new(0.0, 0.0);
            }
            s
        };

        let start1 = syncer.push(&make_frame(2000)).expect("sync 1");
        let start2 = syncer.push(&make_frame(2000)).expect("sync 2");
        assert!(
            (start1 as isize - start2 as isize).unsigned_abs() < 20,
            "expected consistent sync, got {} vs {}",
            start1,
            start2
        );
    }

    #[test]
    fn accepts_realistic_shallow_null_dip() {
        let mut samples = vec![Complex32::new(1.0, 0.0); 196_608];
        for sample in &mut samples[4000..(4000 + 2656)] {
            *sample = Complex32::new(0.53, 0.0);
        }

        let mut syncer = TimeSyncer::default();
        let start = syncer
            .push(&samples)
            .expect("shallow dip should still sync");
        assert!(
            (6600..6800).contains(&start),
            "expected PRS start near 6656, got {}",
            start
        );
    }

    #[test]
    fn prefers_first_valid_null_edge_over_later_deeper_dip() {
        let mut samples = vec![Complex32::new(1.0, 0.0); 196_608];

        for sample in &mut samples[4000..(4000 + 2656)] {
            *sample = Complex32::new(0.50, 0.0);
        }
        for sample in &mut samples[12000..(12000 + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }

        let mut syncer = TimeSyncer::default();
        let start = syncer
            .push(&samples)
            .expect("the first valid null edge should be used");
        assert!(
            (6500..6900).contains(&start),
            "expected PRS start near first dip at 6656, got {}",
            start
        );
    }

    #[test]
    fn local_tracking_window_stays_near_expected_boundary() {
        let mut syncer = TimeSyncer::default();

        let mut first_frame = vec![Complex32::new(1.0, 0.5); 196_608];
        for sample in &mut first_frame[2000..(2000 + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }
        let _ = syncer.push(&first_frame).expect("initial sync");

        let expected_prs = 6 * super::TS;
        let true_null = expected_prs - 2656;
        let mut window = vec![Complex32::new(1.0, 0.5); 12 * super::TS];

        for sample in &mut window[true_null..(true_null + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }

        // A spurious early low-power dip should not steal the boundary lock.
        for sample in &mut window[512..(512 + 2656)] {
            *sample = Complex32::new(0.05, 0.0);
        }

        let start = syncer
            .track_near(&window, expected_prs, 384)
            .expect("tracking sync");
        assert!(
            (start as isize - expected_prs as isize).unsigned_abs() <= 256,
            "expected PRS near {}, got {}",
            expected_prs,
            start
        );
    }

    #[test]
    fn tracked_sync_accepts_shallow_local_dip_when_locked() {
        let mut syncer = TimeSyncer::default();

        let mut first_frame = vec![Complex32::new(1.0, 0.5); 196_608];
        for sample in &mut first_frame[2000..(2000 + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }
        let _ = syncer.push(&first_frame).expect("initial sync");

        let expected_prs = 6 * super::TS;
        let true_null = expected_prs - 2656;
        let mut window = vec![Complex32::new(1.0, 0.5); 12 * super::TS];

        for sample in &mut window[true_null..(true_null + 2656)] {
            *sample = Complex32::new(0.62, 0.0);
        }

        let start = syncer
            .track_near(&window, expected_prs, 384)
            .expect("locked tracking should survive a shallow null dip");
        assert!(
            (start as isize - expected_prs as isize).unsigned_abs() <= 256,
            "expected PRS near {}, got {}",
            expected_prs,
            start
        );
    }

    #[test]
    fn accepts_valid_tracking_shift_within_search_window() {
        let mut syncer = TimeSyncer::default();

        let mut first_frame = vec![Complex32::new(1.0, 0.5); 196_608];
        for sample in &mut first_frame[2000..(2000 + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }
        let _ = syncer.push(&first_frame).expect("initial sync");

        let expected_prs = 6 * super::TS;
        let far_null = expected_prs + 900 - 2656;
        let mut window = vec![Complex32::new(1.0, 0.5); 12 * super::TS];
        for sample in &mut window[far_null..(far_null + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }

        let start = syncer
            .track_near(&window, expected_prs, 2048)
            .expect("a valid dip inside the search window should be accepted");
        assert!(
            (start as isize - (expected_prs + 900) as isize).unsigned_abs() <= 128,
            "expected PRS near shifted boundary {}, got {}",
            expected_prs + 900,
            start
        );
    }

    #[test]
    fn locked_tracking_without_usable_null_stays_on_expected_boundary() {
        let mut syncer = TimeSyncer::default();

        let mut first_frame = vec![Complex32::new(1.0, 0.5); 196_608];
        for sample in &mut first_frame[2000..(2000 + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }
        let _ = syncer.push(&first_frame).expect("initial sync");

        let expected_prs = 6 * super::TS;
        let mut window = vec![Complex32::new(1.0, 0.0); 12 * super::TS];

        // Very shallow dip: not trustworthy enough to drive null-based timing.
        let true_null = expected_prs - 2656;
        for sample in &mut window[true_null..(true_null + 2656)] {
            *sample = Complex32::new(0.97, 0.0);
        }

        let start = syncer
            .track_near(&window, expected_prs, 384)
            .expect("locked tracking should keep boundary");
        assert!(
            (start as isize - expected_prs as isize).unsigned_abs() <= 32,
            "expected PRS near {}, got {}",
            expected_prs,
            start
        );
    }

    #[test]
    fn rejects_short_false_dip_while_locked() {
        let mut syncer = TimeSyncer::default();

        let mut first_frame = vec![Complex32::new(1.0, 0.5); 196_608];
        for sample in &mut first_frame[2000..(2000 + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }
        let _ = syncer.push(&first_frame).expect("initial sync");

        let expected_prs = 6 * super::TS;
        let mut window = vec![Complex32::new(1.0, 0.0); 12 * super::TS];

        // A very short transient drop may cross the threshold but must not be
        // treated as a full DAB null symbol.
        for sample in &mut window[11111..11171] {
            *sample = Complex32::new(0.02, 0.0);
        }

        let start = syncer
            .track_near(&window, expected_prs, 4096)
            .expect("locked tracking should reject the transient");
        assert!(
            (start as isize - expected_prs as isize).unsigned_abs() <= 32,
            "expected PRS near {}, got {}",
            expected_prs,
            start
        );
    }

    #[test]
    fn rejects_large_locked_prs_jump_from_edge_dip() {
        let mut syncer = TimeSyncer::default();

        let mut first_frame = vec![Complex32::new(1.0, 0.5); 196_608];
        for sample in &mut first_frame[2000..(2000 + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }
        let _ = syncer.push(&first_frame).expect("initial sync");

        let expected_prs = 6 * super::TS;
        let mut window = vec![Complex32::new(1.0, 0.0); 12 * super::TS];

        // A broad early dip near the edge of the tracking window must not pull
        // the PRS estimate thousands of samples away from the expected lock.
        for sample in &mut window[10608..(10608 + 1072)] {
            *sample = Complex32::new(0.10, 0.0);
        }

        let start = syncer
            .track_near(&window, expected_prs, 2048)
            .expect("locked tracking should keep the expected boundary");
        assert!(
            (start as isize - expected_prs as isize).unsigned_abs() <= 256,
            "expected PRS near {}, got {}",
            expected_prs,
            start
        );
    }
}
