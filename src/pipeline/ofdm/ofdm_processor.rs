// OFDM processor - converted from ofdm-processor.cpp (eti-cmdline)

use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tracing::trace;

use crate::device::rtlsdr_handler::RtlsdrHandler;
use crate::pipeline::dab_constants::{jan_abs, DIFF_LENGTH, INPUT_RATE};
use crate::pipeline::dab_params::DabParams;
use crate::pipeline::dab_pipeline::DabPipeline;
use crate::pipeline::ofdm::freq_interleaver::FreqInterleaver;
use crate::pipeline::ofdm::phase_reference::PhaseReference;

/// Maximum consecutive `check_endOfNull` failures tolerated in the synced loop
/// before returning to notSynced. (ETSI EN 300 401 §8.4)
const MAX_CONSECUTIVE_SYNC_FAIL: u32 = 3;
/// Cost added to the leaky-bucket failure budget on each `check_endOfNull` miss.
const FAIL_BUDGET_COST: u32 = 2;
/// Leaky-bucket limit: when reached, forces return to notSynced even if false
/// positives have kept `consecutive_sync_fail` below `MAX_CONSECUTIVE_SYNC_FAIL`.
/// At 42 frames/s all-bad: budget rises 84/s — hits limit in ~2.4 s.
const FAIL_BUDGET_LIMIT: u32 = 200;

pub struct OfdmProcessor {
    t_null: usize,
    t_s: usize,
    t_u: usize,
    t_g: usize,
    t_f: usize,
    nr_blocks: usize,
    carriers: usize,
    carrier_diff: i32,
    threshold_1: i16,
    threshold_2: i16,
    phase_synchronizer: PhaseReference,
    freq_interleaver: FreqInterleaver,
    fft: Arc<dyn Fft<f32>>,
    fft_buffer: Vec<Complex32>,
    reference_phase: Vec<Complex32>,
    ofdm_buffer: Vec<Complex32>,
    /// Scratch buffer: differential QPSK samples before amplitude normalisation.
    r1_buf: Vec<Complex32>,
    nco_phasor: Complex32,
    fine_corrector: f32,
    coarse_corrector: i32,
    f2_correction: bool,
    s_level: f32,
    /// Last `coarse_corrector` value confirmed by a successful first-frame lock.
    /// Restored at the start of every re-acquisition so that `estimate_offset()`
    /// running on noise cannot drive the corrector far from the known-good value.
    last_stable_coarse: Option<i32>,
    running: Arc<AtomicBool>,
    // Callbacks
    sync_signal: Option<Box<dyn Fn(bool) + Send>>,
    show_snr: Option<Box<dyn Fn(i16) + Send>>,
    /// Called with the current total frequency offset in Hz (coarse + fine).
    /// Emitted every 10 decoded frames alongside the SNR report.
    show_freq_offset: Option<Box<dyn Fn(i32) + Send>>,
}

/// Errors that cause the processor to exit
pub enum ProcessorError {
    Stopped,
}

impl OfdmProcessor {
    pub fn new(dab_mode: u8, threshold_1: i16, threshold_2: i16, running: Arc<AtomicBool>) -> Self {
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

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(t_u);

        // NCO: no table needed, phase tracked incrementally

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
            threshold_2,
            phase_synchronizer,
            freq_interleaver,
            fft,
            fft_buffer: vec![Complex32::new(0.0, 0.0); t_u],
            reference_phase: vec![Complex32::new(0.0, 0.0); t_u],
            ofdm_buffer: vec![Complex32::new(0.0, 0.0); 2 * t_s],
            r1_buf: vec![Complex32::new(0.0, 0.0); carriers],
            nco_phasor: Complex32::new(1.0, 0.0),
            fine_corrector: 0.0,
            coarse_corrector: 0,
            f2_correction: true,
            s_level: 0.0,
            last_stable_coarse: None,
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

    /// Register a callback invoked with the total frequency offset in Hz
    /// (coarse + fine correctors) every 10 decoded frames.
    /// Divide by the tuned frequency in Hz and multiply by 1_000_000 to get PPM.
    pub fn set_show_freq_offset<F: Fn(i32) + Send + 'static>(&mut self, f: F) {
        self.show_freq_offset = Some(Box::new(f));
    }

    pub fn sync_reached(&mut self) {
        self.f2_correction = false;
        // Save the current coarse corrector as the last known-good value.
        // Restored on every subsequent re-acquisition to prevent noise from
        // driving coarse_corrector far from the correct frequency during fades.
        self.last_stable_coarse = Some(self.coarse_corrector);
        trace!(
            coarse_hz = self.coarse_corrector,
            fine_hz = self.fine_corrector as i32,
            "OFDM: first frame locked — stable_coarse saved"
        );
    }

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

    /// Read and discard `n` IQ samples in chunks, using `scratch` as a temporary
    /// buffer.  Propagates stop signals from the device via `ProcessorError`.
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

    /// Get a single IQ sample from the device, applying frequency correction
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

        // Apply frequency correction via NCO.
        // Precompute the per-sample rotation phasor once (one cos+sin call)
        // and multiply the phasor state directly — avoids trig per sample.
        let delta = -2.0 * std::f32::consts::PI * phase as f32 / INPUT_RATE as f32;
        let step = Complex32::from_polar(1.0, delta);
        self.nco_phasor *= step;
        // No per-sample renormalisation here: float32 rounding drift is ~6e-8
        // per multiply; thousands of get_sample() calls produce negligible drift
        // (~6e-4 over 10 000 samples). The batch renorm in get_samples() keeps
        // the phasor unit over the full frame.
        let corrected = temp[0] * self.nco_phasor;
        self.s_level = 0.00001 * jan_abs(corrected) + (1.0 - 0.00001) * self.s_level;
        Ok(corrected)
    }

    /// Get N IQ samples with frequency correction
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

        // Precompute the per-sample rotation phasor once (one cos+sin call
        // for the whole batch) then use complex multiply per sample.
        // This replaces O(N) trig calls with O(1) trig + O(N) multiplies.
        let delta = -2.0 * std::f32::consts::PI * phase as f32 / INPUT_RATE as f32;
        let step = Complex32::from_polar(1.0, delta);
        for sample in v.iter_mut() {
            self.nco_phasor *= step;
            *sample *= self.nco_phasor;
            self.s_level = 0.00001 * jan_abs(*sample) + (1.0 - 0.00001) * self.s_level;
        }
        // Renormalise once per batch to prevent magnitude drift.
        let norm = self.nco_phasor.norm();
        if norm > 0.0 {
            self.nco_phasor /= norm;
        }
        Ok(())
    }

    /// Demodulate an OFDM data symbol into soft bits (differential QPSK).
    ///
    /// Soft bits are scaled by the **mean carrier amplitude** of the symbol rather
    /// than by each carrier's own amplitude.  In a multipath channel, attenuated
    /// carriers have amplitudes near the noise floor; dividing by their own (tiny)
    /// amplitude would produce ±127 — false certainty that poisons the Viterbi
    /// decoder.  Dividing by the symbol mean instead preserves confidence: strong
    /// carriers produce |ibits| ≈ 127, weak (faded) carriers produce |ibits| ≈ 0.
    fn process_block(&mut self, inv: &[Complex32], ibits: &mut [i16]) {
        self.fft_buffer[..self.t_u].copy_from_slice(&inv[self.t_g..self.t_g + self.t_u]);
        self.fft.process(&mut self.fft_buffer);

        // Pass 1: compute differential samples and accumulate mean amplitude.
        let mut sum_ab = 0.0f32;
        for i in 0..self.carriers {
            let mut index = self.freq_interleaver.map_in(i) as i32;
            if index < 0 {
                index += self.t_u as i32;
            }
            let index = index as usize;
            let r1 = self.fft_buffer[index] * self.reference_phase[index].conj();
            self.reference_phase[index] = self.fft_buffer[index];
            self.r1_buf[i] = r1;
            sum_ab += jan_abs(r1);
        }

        let mean_ab = sum_ab / self.carriers as f32;
        if mean_ab <= 0.0 {
            return;
        }

        // Pass 2: produce soft bits scaled by symbol mean amplitude.
        for i in 0..self.carriers {
            let r1 = self.r1_buf[i];
            ibits[i] = (-r1.re / mean_ab * 127.0).clamp(-127.0, 127.0) as i16;
            ibits[self.carriers + i] = (-r1.im / mean_ab * 127.0).clamp(-127.0, 127.0) as i16;
        }
    }

    /// Main processing loop - runs in its own thread
    /// This is the faithful translation of ofdmProcessor::run() from C++
    #[allow(unused_assignments)]
    pub fn run(&mut self, device: &RtlsdrHandler, eti_generator: &mut DabPipeline) {
        let sync_buffer_size: usize = 32768;
        let sync_buffer_mask = sync_buffer_size - 1;
        let mut env_buffer = vec![0.0f32; sync_buffer_size];
        let mut sync_buffer_index: usize;
        let mut current_strength: f32;
        let mut attempts: i16 = 0;
        let mut ibits = vec![0i16; 2 * self.carriers];
        let mut snr: f32 = 0.0;
        let mut snr_count = 0;
        let mut null_buf = vec![Complex32::new(0.0, 0.0); self.t_null];
        let mut check_buf = vec![Complex32::new(0.0, 0.0); self.t_u];
        let mut block_buf = vec![Complex32::new(0.0, 0.0); self.t_s];
        let _phase = self.coarse_corrector + self.fine_corrector as i32;

        // Warm up `s_level` by consuming half a DAB frame before the sync loop.
        // Gives the NCO level estimator a stable baseline for the first null detection.
        self.s_level = 0.0;
        if self
            .discard_samples(device, self.t_f / 2, &mut block_buf)
            .is_err()
        {
            return;
        }

        loop {
            // notSynced loop
            sync_buffer_index = 0;
            current_strength = 0.0;
            self.s_level = 0.0;
            // Reset the SNR IIR on every re-entry to notSynced so that stale
            // pre-dropout values are never carried forward into the next sync.
            snr = 0.0;
            snr_count = 0;

            if self
                .discard_samples(device, self.t_f, &mut block_buf)
                .is_err()
            {
                return;
            }

            for _ in 0..50 {
                let sample = match self.get_sample(device, 0) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                env_buffer[sync_buffer_index] = jan_abs(sample);
                current_strength += env_buffer[sync_buffer_index];
                sync_buffer_index += 1;
            }

            // SyncOnNull: look for the null level (a dip)
            let mut counter = 0i32;
            let phase = self.coarse_corrector + self.fine_corrector as i32;
            loop {
                if current_strength / 50.0 <= 0.50 * self.s_level {
                    break;
                }
                let sample = match self.get_sample(device, phase) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                env_buffer[sync_buffer_index] = jan_abs(sample);
                let old_idx = (sync_buffer_index + sync_buffer_size - 50) & sync_buffer_mask;
                current_strength += env_buffer[sync_buffer_index] - env_buffer[old_idx];
                sync_buffer_index = (sync_buffer_index + 1) & sync_buffer_mask;
                counter += 1;
                if counter > self.t_f as i32 {
                    attempts += 1;
                    if attempts >= 5 {
                        self.emit_sync_signal(false);
                        attempts = 0;
                        break;
                    }
                }
            }
            if counter > self.t_f as i32 && attempts == 0 {
                continue; // notSynced
            }
            if counter > self.t_f as i32 {
                continue;
            }

            // SyncOnEndNull: look for end of null period
            counter = 0;
            loop {
                if current_strength / 50.0 >= 0.75 * self.s_level {
                    break;
                }
                let sample = match self.get_sample(device, phase) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                env_buffer[sync_buffer_index] = jan_abs(sample);
                let old_idx = (sync_buffer_index + sync_buffer_size - 50) & sync_buffer_mask;
                current_strength += env_buffer[sync_buffer_index] - env_buffer[old_idx];
                sync_buffer_index = (sync_buffer_index + 1) & sync_buffer_mask;
                counter += 1;
                if counter > self.t_null as i32 + 50 {
                    break;
                }
            }
            if counter > self.t_null as i32 + 50 {
                continue; // notSynced
            }

            // Read T_u samples for phase synchronization (batch via temp buffer)
            if self
                .get_samples(device, &mut check_buf[..self.t_u], phase)
                .is_err()
            {
                return;
            }
            self.ofdm_buffer[..self.t_u].copy_from_slice(&check_buf[..self.t_u]);

            let start_index = self
                .phase_synchronizer
                .find_index(&self.ofdm_buffer[..self.t_u], self.threshold_1);
            if start_index < 0 {
                trace!("OFDM: phase ref not found (correlation below threshold), retry");
                continue; // notSynced
            }

            // Synchronized - enter the main frame processing loop
            let mut start_index = start_index as usize;

            // Re-enable coarse AFC for the initial frames of each new sync
            // acquisition. sync_reached() will disable it once the first
            // frame has been decoded correctly.
            self.f2_correction = true;
            // Restore the last known-good coarse corrector so that the
            // re-acquisition scan starts close to the correct frequency.
            // Without this, estimate_offset() running on noise during a deep
            // fade accumulates a random walk that can push coarse_corrector
            // hundreds of Hz from its stable value, delaying recovery.
            if let Some(stable) = self.last_stable_coarse {
                let prev = self.coarse_corrector;
                self.coarse_corrector = stable;
                self.fine_corrector = 0.0;
                trace!(
                    prev_hz = prev,
                    restored_hz = stable,
                    "OFDM: re-acq — coarse_corrector restored to stable value"
                );
            } else {
                trace!(
                    coarse_hz = self.coarse_corrector,
                    "OFDM: re-acq — no stable coarse yet, keeping current value"
                );
            }

            let mut consecutive_sync_fail: u32 = 0;
            let mut fail_budget: u32 = 0;
            let mut first_frame = true;
            loop {
                let tolerating = consecutive_sync_fail > 0;
                // SyncOnPhase: copy remaining data from sync
                let remaining = self.t_u - start_index;
                self.ofdm_buffer.copy_within(start_index..self.t_u, 0);
                let ofdm_buffer_index = remaining;

                eti_generator.new_frame();

                // Block 0: read remaining samples and process
                let phase = self.coarse_corrector + self.fine_corrector as i32;
                let needed = self.t_u - ofdm_buffer_index;
                // Use check_buf as temp scratch for remaining samples (avoids alloc)
                if self
                    .get_samples(device, &mut check_buf[..needed], phase)
                    .is_err()
                {
                    return;
                }
                self.ofdm_buffer[ofdm_buffer_index..self.t_u].copy_from_slice(&check_buf[..needed]);

                // Process block 0 inline (avoid borrow conflict with to_vec)
                self.fft_buffer[..self.t_u].copy_from_slice(&self.ofdm_buffer[..self.t_u]);
                self.fft.process(&mut self.fft_buffer);
                self.reference_phase[..self.t_u].copy_from_slice(&self.fft_buffer[..self.t_u]);

                if self.f2_correction {
                    let correction = self
                        .phase_synchronizer
                        .estimate_offset(&self.ofdm_buffer[..self.t_u]);
                    if correction != 100 {
                        let prev = self.coarse_corrector;
                        self.coarse_corrector += correction as i32 * self.carrier_diff;
                        if self.coarse_corrector.abs() > 35000 {
                            trace!(
                                coarse_hz = self.coarse_corrector,
                                "OFDM: coarse overflow (>35 kHz) — reset to 0"
                            );
                            self.coarse_corrector = 0;
                        } else if correction != 0 {
                            trace!(
                                delta_carriers = correction,
                                prev_hz = prev,
                                new_hz = self.coarse_corrector,
                                "OFDM: coarse AFC step"
                            );
                        }
                    }
                }

                // Data blocks (symbols 2..L)
                // During a tolerance frame (previous check failed), consume samples
                // but skip AFC updates: noise-derived freq_corr would corrupt
                // fine_corrector and prevent recovery when the signal returns.
                let mut freq_corr = Complex32::new(0.0, 0.0);
                for symbol_count in 2..=(self.nr_blocks as u16) {
                    let phase = self.coarse_corrector + self.fine_corrector as i32;
                    if self.get_samples(device, &mut block_buf, phase).is_err() {
                        return;
                    }

                    if !tolerating {
                        // Accumulate frequency correction from cyclic prefix
                        for i in self.t_u..self.t_s {
                            freq_corr += block_buf[i] * block_buf[i - self.t_u].conj();
                        }
                        self.process_block(&block_buf, &mut ibits);
                        eti_generator.process_block(&ibits, symbol_count as i16);
                    }
                }

                // Integrate frequency error — skipped when tolerating a sync miss
                // to prevent noise from driving fine_corrector off the true value.
                if !tolerating {
                    self.fine_corrector += 0.1 * freq_corr.arg() / std::f32::consts::PI
                        * (self.carrier_diff as f32 / 2.0);
                }

                // Skip null symbol and compute SNR
                let phase = self.coarse_corrector + self.fine_corrector as i32;
                if self.get_samples(device, &mut null_buf, phase).is_err() {
                    return;
                }

                let sum: f32 = null_buf.iter().map(|s| s.norm()).sum::<f32>() / self.t_null as f32;
                snr = 0.9 * snr + 0.1 * 20.0 * ((self.s_level + 0.005) / sum).log10();
                snr_count += 1;
                if snr_count > 10 {
                    snr_count = 0;
                    self.emit_snr(snr as i16);
                    // Emit the total frequency offset so callers can derive PPM.
                    // coarse_corrector is in Hz; fine_corrector is in Hz (float).
                    let offset_hz = self.coarse_corrector + self.fine_corrector as i32;
                    self.emit_freq_offset(offset_hz);
                }

                // Adjust fine/coarse frequency correction
                if self.fine_corrector > self.carrier_diff as f32 / 2.0 {
                    self.coarse_corrector += self.carrier_diff;
                    self.fine_corrector -= self.carrier_diff as f32;
                    trace!(
                        coarse_hz = self.coarse_corrector,
                        fine_hz = self.fine_corrector as i32,
                        "OFDM: fine→coarse wrap (+)"
                    );
                } else if self.fine_corrector < -(self.carrier_diff as f32 / 2.0) {
                    self.coarse_corrector -= self.carrier_diff;
                    self.fine_corrector += self.carrier_diff as f32;
                    trace!(
                        coarse_hz = self.coarse_corrector,
                        fine_hz = self.fine_corrector as i32,
                        "OFDM: fine→coarse wrap (-)"
                    );
                }

                // Check_endOfNull: verify sync on next frame
                let phase = self.coarse_corrector + self.fine_corrector as i32;
                if self.get_samples(device, &mut check_buf, phase).is_err() {
                    return;
                }

                start_index = {
                    let idx = self
                        .phase_synchronizer
                        .find_index(&check_buf, self.threshold_2);
                    if idx < 0 {
                        consecutive_sync_fail += 1;
                        fail_budget = fail_budget.saturating_add(FAIL_BUDGET_COST);
                        let hard_fail = consecutive_sync_fail >= MAX_CONSECUTIVE_SYNC_FAIL;
                        let budget_fail = fail_budget >= FAIL_BUDGET_LIMIT;
                        trace!(
                            consecutive = consecutive_sync_fail,
                            max = MAX_CONSECUTIVE_SYNC_FAIL,
                            fail_budget,
                            coarse_hz = self.coarse_corrector,
                            "OFDM: sync check weak — tolerating"
                        );
                        if hard_fail || budget_fail {
                            trace!(
                                coarse_hz = self.coarse_corrector,
                                fine_hz = self.fine_corrector as i32,
                                snr = snr as i16,
                                budget_fail,
                                "OFDM: sync lost after consecutive failures — returning to notSynced"
                            );
                            // Reset SNR immediately so the status display shows
                            // signal loss rather than the stale IIR value.
                            snr = 0.0;
                            snr_count = 0;
                            self.emit_snr(0);
                            break; // Lost sync, go back to notSynced
                        }
                        // Tolerate this frame: assume CP starts at beginning of buffer
                        0usize
                    } else {
                        consecutive_sync_fail = 0;
                        fail_budget = fail_budget.saturating_sub(1);
                        idx as usize
                    }
                };
                // Copy for next frame
                self.ofdm_buffer[..self.t_u].copy_from_slice(&check_buf);
                self.emit_sync_signal(true);
                // Disable coarse AFC after the first successfully decoded
                // frame — fine_corrector handles residual drift from here on.
                // (ETSI EN 300 401 §8.4.3)
                if first_frame {
                    self.sync_reached();
                    first_frame = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Simulate the leaky-bucket failure budget used in the inner synced loop.
    ///
    /// Returns the budget value after applying a sequence of frame outcomes.
    /// `true` = find_index success, `false` = find_index failure.
    fn run_budget(outcomes: &[bool], cost: u32) -> u32 {
        let mut budget: u32 = 0;
        for &ok in outcomes {
            if ok {
                budget = budget.saturating_sub(1);
            } else {
                budget = budget.saturating_add(cost);
            }
        }
        budget
    }

    /// All-bad frames must hit the limit well within 2.4 s at 42 fps.
    #[test]
    fn fail_budget_sustained_absence_hits_limit() {
        // With 42 bad frames/s and cost=2, budget rises by 84/s.
        // Limit reached in ceil(LIMIT/COST) = 100 bad frames ≈ 2.4 s.
        let all_bad: Vec<bool> = vec![false; 100];
        let budget = run_budget(&all_bad, super::FAIL_BUDGET_COST);
        assert!(
            budget >= super::FAIL_BUDGET_LIMIT,
            "Expected budget >= {} after 100 bad frames, got {budget}",
            super::FAIL_BUDGET_LIMIT
        );
    }

    /// Occasional false positives (1 per second) must NOT prevent the limit
    /// from being reached during sustained signal absence.
    #[test]
    fn fail_budget_false_positives_cannot_prevent_reacquisition() {
        // Pattern: 41 bad + 1 false positive per second.
        // Net per second: 41*2 - 1 = 81.  Limit hit after ceil(200/81) ≈ 3 s.
        let mut outcomes: Vec<bool> = Vec::new();
        for _ in 0..5 {
            // 5 seconds
            outcomes.extend(vec![false; 41]);
            outcomes.push(true); // false positive
        }
        let budget = run_budget(&outcomes, super::FAIL_BUDGET_COST);
        assert!(
            budget >= super::FAIL_BUDGET_LIMIT,
            "False positives should not prevent limit; budget={budget}, limit={}",
            super::FAIL_BUDGET_LIMIT
        );
    }

    /// During normal reception (all frames succeed), the budget must stay at 0.
    #[test]
    fn fail_budget_good_signal_stays_zero() {
        let all_good: Vec<bool> = vec![true; 500];
        let budget = run_budget(&all_good, super::FAIL_BUDGET_COST);
        assert_eq!(
            budget, 0,
            "Budget should saturate at 0 during good reception"
        );
    }

    /// Budget must recover to 0 after a brief outage followed by signal return.
    #[test]
    fn fail_budget_recovers_after_brief_outage() {
        // 50 bad frames then 200 good: should drain back to 0.
        let mut outcomes: Vec<bool> = vec![false; 50];
        outcomes.extend(vec![true; 200]);
        let budget = run_budget(&outcomes, super::FAIL_BUDGET_COST);
        assert_eq!(budget, 0, "Budget should drain to 0 after signal recovery");
    }
}
