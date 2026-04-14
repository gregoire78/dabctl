// OFDM processor — refactored orchestrator.
// ETSI EN 300 401 §8 — DAB transmission frame structure

use num_complex::Complex32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tracing::{debug, trace};

use crate::device::rtlsdr_handler::RtlsdrHandler;
use crate::pipeline::dab_constants::{jan_abs, DIFF_LENGTH, INPUT_RATE};
use crate::pipeline::dab_params::DabParams;
use crate::pipeline::dab_pipeline::DabPipeline;
use crate::pipeline::ofdm::block_demod::BlockDemod;
use crate::pipeline::ofdm::equalizer::Equalizer;
use crate::pipeline::ofdm::fft_engine::FftEngine;
use crate::pipeline::ofdm::freq_interleaver::FreqInterleaver;
use crate::pipeline::ofdm::mer::estimate_mer;
use crate::pipeline::ofdm::nco::Nco;
use crate::pipeline::ofdm::phase_reference::PhaseReference;
use crate::pipeline::ofdm::synchronizer::{NullState, SyncState};

/// LMS step size for the decision-directed channel equalizer.
///
/// 0.005 gives a good trade-off between convergence speed and oscillation
/// resistance on typical indoor multipath DAB channels.  A larger value (e.g.
/// 0.02) converges faster but risks oscillation under heavy multipath.
const EQUALIZER_MU: f32 = 0.005;

/// Maximum coarse AFC correction applied in a single PRS frame (carrier units).
///
/// RTL-SDR crystals are typically accurate to ±20–25 ppm.  At a 250 MHz centre
/// frequency that is ±6 kHz, or ±6 carriers in DAB Mode 1 (1 kHz spacing).
/// Clamping each step to ±3 carriers bounds the maximum single-frame correction
/// to half the expected worst-case offset; the outer acquisition loop repeats
/// until the coarse corrector has converged, after which the fine corrector
/// (cyclic-prefix correlation) handles any sub-carrier residual.
///
/// Without this clamp the `estimate_offset` function — which is unreliable at
/// SNR below ~6 dB — can return values up to ±35, causing the coarse corrector
/// to oscillate by tens of kHz each frame and preventing lock entirely.
const AFC_MAX_STEP_CARRIERS: i16 = 3;

/// Normalise every complex symbol in `symbols` to unit magnitude in-place.
///
/// Symbols with magnitude below [`f32::EPSILON`] are left unchanged to avoid
/// division by zero; their phase is undefined and any QPSK decision on them
/// will be random regardless.
///
/// # Why this is needed
///
/// The raw differential QPSK output `r1 = fft_data × conj(fft_prs)` is a
/// product of two FFT outputs, so its magnitude scales with `|fft|²` rather
/// than `|fft|`.  For a DAB Mode 1 signal at typical SDR amplitude levels
/// `|r1|` is on the order of 200–300, far larger than the unit-amplitude
/// symbols the LMS equalizer expects.  The LMS stability condition
/// `µ·|sym|² < 2` then becomes `0.005 × 256² ≈ 328 < 2` — completely
/// violated — and equalizer weights diverge exponentially, producing NaN in
/// the MER metric after approximately 16 symbols.
///
/// For DQPSK only the *phase* of `r1` carries information; normalising the
/// magnitude to 1 is therefore lossless for both equalization and soft-bit
/// generation.
fn normalise_unit_circle(symbols: &mut [Complex32]) {
    for s in symbols.iter_mut() {
        let mag = s.norm();
        if mag > f32::EPSILON {
            *s /= mag;
        }
    }
}

/// Acquisition / tracking detection thresholds (inclusive bounds).
const OFDM_THRESHOLD_MIN: i16 = 2;
const OFDM_THRESHOLD_MAX: i16 = 7;

/// Refactored OFDM processor.
///
/// Each logical concern is delegated to a focused sub-module:
/// - [`Nco`]        — frequency correction
/// - [`FftEngine`]  — FFT with buffer reuse
/// - [`BlockDemod`] — differential QPSK per symbol
/// - [`SyncState`]  — null-symbol detection state machine
/// - [`PhaseReference`] — PRS correlation and coarse AFC
pub struct OfdmProcessor {
    // ── DAB frame geometry ────────────────────────────────────────────────────
    t_null: usize,
    t_s: usize,
    t_u: usize,
    t_g: usize,
    t_f: usize,
    nr_blocks: usize,
    carriers: usize,
    carrier_diff: i32,

    // ── Acquisition threshold ─────────────────────────────────────────────────
    threshold_1: i16,

    // ── Sub-modules ───────────────────────────────────────────────────────────
    phase_synchronizer: PhaseReference,
    nco: Nco,
    fft_engine: FftEngine,
    block_demod: BlockDemod,
    /// Decision-directed LMS channel equalizer applied per-carrier after DQPSK.
    ///
    /// Applied on the post-differential symbols held in `block_demod.r1_buf` so
    /// that residual per-carrier amplitude/phase distortion from multipath is
    /// suppressed before the soft bits are forwarded to the Viterbi decoder.
    /// This is the primary lever for improving `fib_quality` on real channels.
    equalizer: Equalizer,
    sync_state: SyncState,

    // ── Pre-computed lookup tables (allocated once) ───────────────────────────
    /// Carrier-to-FFT-bin map (signed, as returned by FreqInterleaver).
    freq_map: Vec<i16>,

    // ── Per-call reuse buffers (zero-allocation hot path) ─────────────────────
    /// Full FFT output buffer (t_u complex values).
    fft_buf: Vec<Complex32>,
    /// PRS / guard-stripping buffer (t_u complex values).
    ofdm_buffer: Vec<Complex32>,

    // ── Frequency correction state ────────────────────────────────────────────
    fine_corrector: f32,
    coarse_corrector: i32,
    f2_correction: bool,

    // ── Signal level tracker ──────────────────────────────────────────────────
    s_level: f32,

    // ── Lifecycle ─────────────────────────────────────────────────────────────
    running: Arc<AtomicBool>,

    // ── Optional callbacks ────────────────────────────────────────────────────
    sync_signal: Option<Box<dyn Fn(bool) + Send>>,
    show_snr: Option<Box<dyn Fn(i16) + Send>>,
    show_freq_offset: Option<Box<dyn Fn(i32) + Send>>,
}

/// Returned by internal sample-fetch helpers to signal a clean shutdown.
pub enum ProcessorError {
    Stopped,
}

impl OfdmProcessor {
    pub fn new(dab_mode: u8, threshold_1: i16, running: Arc<AtomicBool>) -> Self {
        let params = DabParams::new(dab_mode);
        let t_u = params.t_u as usize;
        let t_s = params.t_s as usize;
        let t_g = params.t_g as usize;
        let t_null = params.t_null as usize;
        let t_f = params.t_f as usize;
        let nr_blocks = params.l as usize;
        let carriers = params.k as usize;
        let carrier_diff = params.carrier_diff;

        let freq_interleaver = FreqInterleaver::new(&params);
        let phase_synchronizer =
            PhaseReference::new(t_u, carriers, params.dab_mode, DIFF_LENGTH as usize);

        // Pre-compute carrier→bin lookup table once.
        let freq_map: Vec<i16> = (0..carriers).map(|i| freq_interleaver.map_in(i)).collect();

        OfdmProcessor {
            t_null,
            t_s,
            t_u,
            t_g,
            t_f,
            nr_blocks,
            carriers,
            carrier_diff,
            threshold_1,
            phase_synchronizer,
            nco: Nco::new(),
            fft_engine: FftEngine::new_forward(t_u),
            block_demod: BlockDemod::new(carriers, t_u),
            equalizer: Equalizer::new(carriers, EQUALIZER_MU),
            sync_state: SyncState::new(t_f, t_null),
            freq_map,
            fft_buf: vec![Complex32::new(0.0, 0.0); t_u],
            ofdm_buffer: vec![Complex32::new(0.0, 0.0); t_u],
            fine_corrector: 0.0,
            coarse_corrector: 0,
            f2_correction: true,
            s_level: 0.0,
            running,
            sync_signal: None,
            show_snr: None,
            show_freq_offset: None,
        }
    }

    pub fn set_sync_signal<F: Fn(bool) + Send + 'static>(&mut self, f: F) {
        self.sync_signal = Some(Box::new(f));
    }
    pub fn set_show_snr<F: Fn(i16) + Send + 'static>(&mut self, f: F) {
        self.show_snr = Some(Box::new(f));
    }
    pub fn set_show_freq_offset<F: Fn(i32) + Send + 'static>(&mut self, f: F) {
        self.show_freq_offset = Some(Box::new(f));
    }

    /// Called once sync is confirmed; disables the coarse AFC search.
    pub fn sync_reached(&mut self) {
        self.f2_correction = false;
    }

    // ── Callback shims ────────────────────────────────────────────────────────

    fn emit_sync_signal(&self, val: bool) {
        if let Some(ref f) = self.sync_signal {
            f(val);
        }
    }
    fn emit_snr(&self, val: i16) {
        if let Some(ref f) = self.show_snr {
            f(val);
        }
    }
    fn emit_freq_offset(&self, offset_hz: i32) {
        if let Some(ref f) = self.show_freq_offset {
            f(offset_hz);
        }
    }

    // ── Sample acquisition helpers ────────────────────────────────────────────

    /// Discard `n` samples using `scratch` as a reusable staging buffer.
    fn discard_samples(
        &mut self,
        device: &RtlsdrHandler,
        n: usize,
        scratch: &mut [Complex32],
    ) -> Result<(), ProcessorError> {
        let mut remaining = n;
        while remaining > 0 {
            let chunk = remaining.min(scratch.len());
            self.get_samples(device, &mut scratch[..chunk], 0)?;
            remaining -= chunk;
        }
        Ok(())
    }

    /// Fetch and frequency-correct one sample, updating the running signal level.
    fn get_sample(
        &mut self,
        device: &RtlsdrHandler,
        phase: i32,
    ) -> Result<Complex32, ProcessorError> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(ProcessorError::Stopped);
        }
        let mut temp = [Complex32::new(0.0, 0.0)];
        if device.get_samples(&mut temp) == 0 {
            return Err(ProcessorError::Stopped);
        }
        self.nco.apply_batch(&mut temp, phase, INPUT_RATE);
        self.s_level = 0.00001 * jan_abs(temp[0]) + (1.0 - 0.00001) * self.s_level;
        Ok(temp[0])
    }

    /// Fetch and frequency-correct a batch of samples, updating signal level.
    fn get_samples(
        &mut self,
        device: &RtlsdrHandler,
        v: &mut [Complex32],
        phase: i32,
    ) -> Result<(), ProcessorError> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(ProcessorError::Stopped);
        }
        if device.get_samples(v) < v.len() {
            return Err(ProcessorError::Stopped);
        }
        self.nco.apply_batch(v, phase, INPUT_RATE);
        for &sample in v.iter() {
            self.s_level = 0.00001 * jan_abs(sample) + (1.0 - 0.00001) * self.s_level;
        }
        Ok(())
    }

    // ── Block processing ──────────────────────────────────────────────────────

    /// FFT one data block (guard-stripped), demodulate to soft bits, and
    /// apply the LMS equalizer on the post-differential symbols.
    ///
    /// `block` must be at least `t_s` samples long; the first `t_g` are the guard.
    fn process_data_block(&mut self, block: &[Complex32], ibits: &mut [i16]) {
        // Strip cyclic prefix, run FFT.
        self.fft_engine
            .process_into(&block[self.t_g..self.t_g + self.t_u], &mut self.fft_buf);

        // Differential QPSK demodulation → fills r1_buf and ibits.
        let t_u = self.t_u;
        self.block_demod
            .process(&self.fft_buf, &self.freq_map, t_u, ibits);

        // Normalise each differential symbol to unit magnitude before equalization.
        // See [`normalise_unit_circle`] for a detailed explanation.
        normalise_unit_circle(self.block_demod.r1_buf_mut());

        // Apply LMS equalizer to post-differential symbols then regenerate soft
        // bits.  The two fields are disjoint so Rust NLL can split the borrows;
        // explicit tuple destructuring makes the disjointness visible to the
        // borrow checker.
        let (equalizer, block_demod) = (&mut self.equalizer, &mut self.block_demod);
        equalizer.equalize(block_demod.r1_buf_mut());
        block_demod.recompute_ibits(ibits);
    }

    // ── Main run loop ─────────────────────────────────────────────────────────

    /// Run the OFDM processing loop until the `running` flag is cleared.
    ///
    /// Samples are read from `device`, demodulated, and forwarded to
    /// `eti_generator` frame-by-frame.
    #[allow(unused_assignments)]
    pub fn run(&mut self, device: &RtlsdrHandler, eti_generator: &mut DabPipeline) {
        let mut ibits = vec![0i16; 2 * self.carriers];
        let mut snr: f32 = 0.0;
        let mut snr_count = 0i32;
        let mut mer_acc: f32 = 0.0;
        let mut mer_count: u32 = 0;

        // Allocate per-frame scratch buffers (no hot-path allocation after this point).
        let mut null_buf = vec![Complex32::new(0.0, 0.0); self.t_null];
        let mut block_buf = vec![Complex32::new(0.0, 0.0); self.t_s];
        let mut check_buf = vec![Complex32::new(0.0, 0.0); self.t_u];

        self.s_level = 0.0;
        if self
            .discard_samples(device, self.t_f / 2, &mut block_buf)
            .is_err()
        {
            return;
        }

        // ── Outer acquisition loop ─────────────────────────────────────────────
        loop {
            self.s_level = 0.0;
            self.sync_state.reset();

            if self
                .discard_samples(device, self.t_f, &mut block_buf)
                .is_err()
            {
                return;
            }

            // Prefill the 50-sample sliding window.
            for _ in 0..50 {
                let s = match self.get_sample(device, 0) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                self.sync_state.prefill(jan_abs(s));
            }

            let phase = self.coarse_corrector + self.fine_corrector as i32;

            // ── Null + end-of-null detection ───────────────────────────────────
            let mut found_frame_start = false;
            loop {
                let s = match self.get_sample(device, phase) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                match self.sync_state.detect_null(jan_abs(s), self.s_level) {
                    NullState::Searching | NullState::NullFound => {}
                    NullState::EndOfNull => {
                        found_frame_start = true;
                        break;
                    }
                    NullState::Timeout => {
                        if self.sync_state.false_sync_pending() {
                            self.emit_sync_signal(false);
                        }
                        break;
                    }
                }
            }
            if !found_frame_start {
                continue;
            }

            // Read T_u samples for PRS correlation.
            if self.get_samples(device, &mut check_buf, phase).is_err() {
                return;
            }
            self.ofdm_buffer.copy_from_slice(&check_buf);

            let acq_threshold = self
                .threshold_1
                .clamp(OFDM_THRESHOLD_MIN, OFDM_THRESHOLD_MAX);

            let start_index = self
                .phase_synchronizer
                .find_index(&self.ofdm_buffer, acq_threshold);
            if start_index < 0 {
                trace!("OFDM: phase ref not found");
                continue;
            }
            let mut start_index = start_index as usize;
            debug!(
                start_index,
                s_level = self.s_level,
                coarse_hz = self.coarse_corrector,
                fine_hz = self.fine_corrector as i32,
                "OFDM: acquisition succeeded"
            );
            self.f2_correction = true;
            let mut first_frame = true;
            let mut inner_frame_count: u32 = 0;

            // ── Inner tracking loop ────────────────────────────────────────────
            loop {
                // Align ofdm_buffer to the PRS boundary.
                let remaining = self.t_u - start_index;
                self.ofdm_buffer.copy_within(start_index..self.t_u, 0);

                inner_frame_count += 1;
                trace!(
                    frame = inner_frame_count,
                    start_index,
                    s_level = self.s_level,
                    "OFDM: frame"
                );

                eti_generator.new_frame();

                let phase = self.coarse_corrector + self.fine_corrector as i32;
                let needed = self.t_u - remaining;
                if self
                    .get_samples(device, &mut check_buf[..needed], phase)
                    .is_err()
                {
                    return;
                }
                self.ofdm_buffer[remaining..self.t_u].copy_from_slice(&check_buf[..needed]);

                // ── Block 0 (PRS): FFT → set DQPSK reference ─────────────────
                // Inline to allow NLL to split borrows across disjoint fields
                // (ofdm_buffer, fft_engine, fft_buf, block_demod are all separate).
                let t_u = self.t_u;
                self.fft_engine
                    .process_into(&self.ofdm_buffer[..t_u], &mut self.fft_buf);
                self.block_demod.set_reference_from_fft(&self.fft_buf);

                // Coarse AFC from PRS (disabled once sync is confirmed).
                if self.f2_correction {
                    let correction = self.phase_synchronizer.estimate_offset(&self.ofdm_buffer);
                    if correction != 100 {
                        // Clamp to ±AFC_MAX_STEP_CARRIERS per step: estimate_offset is noisy at low
                        // SNR and can return values up to ±35, which would send the
                        // coarse corrector far from the true offset and prevent lock.
                        let clamped = correction.clamp(-AFC_MAX_STEP_CARRIERS, AFC_MAX_STEP_CARRIERS);
                        let prev = self.coarse_corrector;
                        self.coarse_corrector += clamped as i32 * self.carrier_diff;
                        if self.coarse_corrector.abs() > 35_000 {
                            trace!(coarse_hz = self.coarse_corrector, "OFDM: coarse overflow");
                            self.coarse_corrector = 0;
                        } else if clamped != 0 {
                            trace!(
                                delta_carriers = clamped,
                                prev_hz = prev,
                                new_hz = self.coarse_corrector,
                                "OFDM: coarse AFC step"
                            );
                        }
                    }
                }

                // ── Data blocks 2..=nr_blocks ─────────────────────────────────
                // ETSI EN 300 401 §14 — symbol numbering starts at 1 (PRS).
                let mut freq_corr = Complex32::new(0.0, 0.0);
                for symbol_count in 2..=(self.nr_blocks as u16) {
                    let phase = self.coarse_corrector + self.fine_corrector as i32;
                    if self.get_samples(device, &mut block_buf, phase).is_err() {
                        return;
                    }
                    // Fine-frequency correction accumulation (cyclic prefix correlation).
                    for i in self.t_u..self.t_s {
                        freq_corr += block_buf[i] * block_buf[i - self.t_u].conj();
                    }
                    self.process_data_block(&block_buf, &mut ibits);
                    mer_acc += estimate_mer(self.block_demod.r1_buf());
                    mer_count += 1;
                    eti_generator.process_block(&ibits, symbol_count as i16);
                }

                // Fine frequency update from cyclic-prefix correlation.
                // Gain 0.15 gives faster CFO convergence than the original 0.10 while
                // remaining stable; the guard-interval correlation is averaged over all
                // data symbols in the frame so the estimate is already low-variance.
                self.fine_corrector +=
                    0.15 * freq_corr.arg() / std::f32::consts::PI * (self.carrier_diff as f32 / 2.0);

                // ── Null symbol ────────────────────────────────────────────────
                let phase = self.coarse_corrector + self.fine_corrector as i32;
                if self.get_samples(device, &mut null_buf, phase).is_err() {
                    return;
                }

                // SNR from null-symbol noise floor vs. long-term signal level.
                let null_mean = null_buf.iter().map(|s| s.norm()).sum::<f32>() / self.t_null as f32;
                snr = 0.9 * snr + 0.1 * 20.0 * ((self.s_level + 0.005) / null_mean).log10();

                // MER from equalized post-differential symbols.
                let avg_mer = if mer_count > 0 {
                    mer_acc / mer_count as f32
                } else {
                    0.0
                };
                mer_acc = 0.0;
                mer_count = 0;

                trace!(
                    s_level = self.s_level,
                    null_mean,
                    snr_db = snr,
                    mer_db = avg_mer,
                    "OFDM: SNR/MER sample"
                );

                snr_count += 1;
                if snr_count > 10 {
                    snr_count = 0;
                    self.emit_snr(snr as i16);
                    let offset_hz = self.coarse_corrector + self.fine_corrector as i32;
                    self.emit_freq_offset(offset_hz);
                }

                // ── Fine → coarse carrier wrap ────────────────────────────────
                let half_carrier = self.carrier_diff as f32 / 2.0;
                if self.fine_corrector > half_carrier {
                    self.coarse_corrector += self.carrier_diff;
                    self.fine_corrector -= self.carrier_diff as f32;
                    trace!(
                        coarse_hz = self.coarse_corrector,
                        fine_hz = self.fine_corrector as i32,
                        "OFDM: fine→coarse wrap (+)"
                    );
                } else if self.fine_corrector < -half_carrier {
                    self.coarse_corrector -= self.carrier_diff;
                    self.fine_corrector += self.carrier_diff as f32;
                    trace!(
                        coarse_hz = self.coarse_corrector,
                        fine_hz = self.fine_corrector as i32,
                        "OFDM: fine→coarse wrap (-)"
                    );
                }

                // ── Read next T_u for PRS correlation ─────────────────────────
                let phase = self.coarse_corrector + self.fine_corrector as i32;
                if self.get_samples(device, &mut check_buf, phase).is_err() {
                    return;
                }

                start_index = {
                    let idx = self
                        .phase_synchronizer
                        .find_index(&check_buf, acq_threshold);
                    if idx < 0 {
                        trace!(frames = inner_frame_count, "OFDM: PRS correlation miss");
                        break;
                    }
                    idx as usize
                };
                self.ofdm_buffer.copy_from_slice(&check_buf);
                self.emit_sync_signal(true);
                if first_frame {
                    self.sync_reached();
                    first_frame = false;
                }
            } // inner tracking loop
        } // outer acquisition loop
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicI16, AtomicI32, Ordering};
    use std::sync::Arc;

    fn make_processor(mode: u8) -> OfdmProcessor {
        OfdmProcessor::new(mode, 2, Arc::new(AtomicBool::new(true)))
    }

    // ── Construction ─────────────────────────────────────────────────────────

    #[test]
    fn new_mode1_does_not_panic() {
        let _p = make_processor(1);
    }

    #[test]
    fn new_mode2_does_not_panic() {
        let _p = make_processor(2);
    }

    #[test]
    fn new_mode3_does_not_panic() {
        let _p = make_processor(3);
    }

    #[test]
    fn new_mode4_does_not_panic() {
        let _p = make_processor(4);
    }

    // ── Callback wiring: sync_signal ─────────────────────────────────────────

    #[test]
    fn sync_signal_fires_true_when_emitted() {
        let mut p = make_processor(1);
        let fired = Arc::new(AtomicBool::new(false));
        let fired2 = fired.clone();
        p.set_sync_signal(move |v| fired2.store(v, Ordering::SeqCst));
        p.emit_sync_signal(true);
        assert!(fired.load(Ordering::SeqCst));
    }

    #[test]
    fn sync_signal_fires_false_when_emitted() {
        let mut p = make_processor(1);
        let fired = Arc::new(AtomicBool::new(true));
        let fired2 = fired.clone();
        p.set_sync_signal(move |v| fired2.store(v, Ordering::SeqCst));
        p.emit_sync_signal(false);
        assert!(!fired.load(Ordering::SeqCst));
    }

    #[test]
    fn no_sync_callback_does_not_panic() {
        let p = make_processor(1);
        p.emit_sync_signal(true); // must not panic
    }

    // ── Callback wiring: show_snr ────────────────────────────────────────────

    #[test]
    fn snr_signal_fires_with_correct_value() {
        let mut p = make_processor(1);
        let last_snr = Arc::new(AtomicI16::new(0));
        let last2 = last_snr.clone();
        p.set_show_snr(move |v| last2.store(v, Ordering::SeqCst));
        p.emit_snr(42);
        assert_eq!(last_snr.load(Ordering::SeqCst), 42);
    }

    #[test]
    fn no_snr_callback_does_not_panic() {
        let p = make_processor(1);
        p.emit_snr(10); // must not panic
    }

    // ── Callback wiring: show_freq_offset ────────────────────────────────────

    #[test]
    fn freq_offset_signal_fires_with_correct_value() {
        let mut p = make_processor(1);
        let last = Arc::new(AtomicI32::new(0));
        let last2 = last.clone();
        p.set_show_freq_offset(move |v| last2.store(v, Ordering::SeqCst));
        p.emit_freq_offset(-1234);
        assert_eq!(last.load(Ordering::SeqCst), -1234);
    }

    #[test]
    fn no_freq_offset_callback_does_not_panic() {
        let p = make_processor(1);
        p.emit_freq_offset(500); // must not panic
    }

    // ── sync_reached ─────────────────────────────────────────────────────────

    #[test]
    fn f2_correction_starts_enabled() {
        let p = make_processor(1);
        assert!(p.f2_correction, "f2_correction must start enabled");
    }

    #[test]
    fn sync_reached_disables_f2_correction() {
        let mut p = make_processor(1);
        p.sync_reached();
        assert!(!p.f2_correction, "f2_correction must be disabled after sync_reached()");
    }

    // ── Initial frequency state ───────────────────────────────────────────────

    #[test]
    fn fine_corrector_starts_at_zero() {
        let p = make_processor(1);
        assert_eq!(p.fine_corrector, 0.0);
    }

    #[test]
    fn coarse_corrector_starts_at_zero() {
        let p = make_processor(1);
        assert_eq!(p.coarse_corrector, 0);
    }

    // ── normalise_unit_circle ─────────────────────────────────────────────────

    #[test]
    fn normalise_unit_circle_scales_large_symbol_to_unit() {
        // FFT-scale symbols have |r1| ≈ 256; after normalisation |s| must be 1.
        let mut symbols = vec![Complex32::new(180.0, 100.0)];
        normalise_unit_circle(&mut symbols);
        let mag = symbols[0].norm();
        assert!(
            (mag - 1.0).abs() < 1e-5,
            "expected unit magnitude, got {mag}"
        );
    }

    #[test]
    fn normalise_unit_circle_preserves_unit_symbol() {
        use std::f32::consts::FRAC_1_SQRT_2;
        let qpsk = Complex32::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2);
        let mut symbols = vec![qpsk];
        normalise_unit_circle(&mut symbols);
        let mag = symbols[0].norm();
        assert!(
            (mag - 1.0).abs() < 1e-5,
            "unit symbol should stay unit, got {mag}"
        );
        // Phase must be unchanged.
        let phase_before = qpsk.arg();
        let phase_after = symbols[0].arg();
        assert!(
            (phase_before - phase_after).abs() < 1e-5,
            "phase must be preserved: {phase_before} → {phase_after}"
        );
    }

    #[test]
    fn normalise_unit_circle_leaves_zero_symbol_unchanged() {
        // A zero-magnitude symbol must not produce NaN (division by zero).
        let mut symbols = vec![Complex32::new(0.0, 0.0)];
        normalise_unit_circle(&mut symbols);
        assert!(
            symbols[0].re.is_finite() && symbols[0].im.is_finite(),
            "zero symbol must not become NaN/Inf after normalisation"
        );
        assert_eq!(symbols[0], Complex32::new(0.0, 0.0));
    }

    #[test]
    fn normalise_unit_circle_all_symbols_become_unit_magnitude() {
        // Mixed large, small, and negative amplitudes; all must normalise to 1.
        let inputs = [
            Complex32::new(256.0, 256.0),
            Complex32::new(-100.0, 50.0),
            Complex32::new(0.5, -0.5),
            Complex32::new(-0.01, 0.01),
        ];
        let mut symbols = inputs.to_vec();
        normalise_unit_circle(&mut symbols);
        for (i, s) in symbols.iter().enumerate() {
            let mag = s.norm();
            assert!(
                (mag - 1.0).abs() < 1e-5,
                "symbol[{i}] magnitude should be 1.0, got {mag}"
            );
        }
    }
}
