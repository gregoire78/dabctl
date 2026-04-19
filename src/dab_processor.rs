use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Result;
use num_complex::Complex32;
use tracing::{debug, info, warn};

use crate::backend::{audio::DEFAULT_DAB_PLUS_BITRATE, msc_handler::MscHandler};
use crate::cli::{AacDecoderKind, Cli};
use crate::decoder::{fib_decoder::FibDecoder, fic_decoder::FicDecoder};
use crate::device::{DeviceOptions, RtlSdrDevice};
use crate::metadata::MetadataWriter;
use crate::ofdm::{
    ofdm_decoder::{OfdmDecoder, TG, TS, TU},
    phase_reference::PhaseReference,
    sample_reader::SampleReader,
    time_syncer::TimeSyncer,
};
use crate::pcm::PcmOutput;

fn prs_threshold_for_lock_state(locked: bool) -> f32 {
    if locked {
        6.0
    } else {
        3.0
    }
}

fn frame_uses_non_tii_null(cif_count: Option<u16>, frame_index: u64) -> bool {
    let cycle = cif_count.map(u64::from).unwrap_or(frame_index);
    (cycle & 0x7) < 4
}

fn coarse_afc_should_apply(correction_hz: i32) -> bool {
    correction_hz != 0
}

fn coarse_afc_requires_reacquisition(correction_hz: i32) -> bool {
    // In the buffered dabctl live path, any non-zero coarse PRS correction must
    // restart acquisition with the updated BB offset. Continuing the same frame
    // leaves the symbol/phase history inconsistent and quickly collapses lock.
    correction_hz != 0
}

fn should_transfer_correction_to_rf(
    fic_ratio_percent: usize,
    frames_processed: u64,
    bb_freq_offs_applied_hz: i32,
    rf_freq_shift_used: bool,
) -> bool {
    // In the buffered CLI path, tiny residual BB corrections are safer to keep
    // in software. Only consider a one-time RF takeover once lock is already
    // strong and the residual offset is materially large.
    const MIN_RF_HANDOFF_HZ: i32 = 250;
    fic_ratio_percent >= 90
        && frames_processed >= 6
        && !rf_freq_shift_used
        && bb_freq_offs_applied_hz.abs() >= MIN_RF_HANDOFF_HZ
}

fn normalized_cp_coherence(freq_corr: Complex32, energy_sum: f32) -> f32 {
    if energy_sum > 1.0e-12 {
        freq_corr.norm() / energy_sum
    } else {
        0.0
    }
}

// DABstar evaluates the next sync symbol from a TU-sized preview that starts
// one guard interval before the nominal boundary so the useful-part peak lands
// near TG inside the correlator.
const TRACKING_PRS_SEARCH_BACKOFF: usize = TG;
const MAX_TRACKING_PRS_MISSES: u32 = 3;

fn fine_afc_gain(_locked_frames: u32, cp_coherence: f32) -> f32 {
    if cp_coherence < 0.02 {
        0.0
    } else if cp_coherence < 0.08 {
        0.35
    } else {
        // DABstar applies the cyclic-prefix phase correction at full scale.
        // Keep that behavior when coherence is healthy, and only damp it in
        // marginal conditions where the estimate is noisy.
        1.0
    }
}

fn tracked_frame_sample_count(prs_peak: Option<usize>, search_base: usize) -> Option<usize> {
    prs_peak.map(|peak| search_base + peak)
}

fn should_reacquire_after_prs_miss(prs_miss_count: u32) -> bool {
    prs_miss_count >= MAX_TRACKING_PRS_MISSES
}

fn preview_rot_phase(rot_phase: f32, freq_hz: f32, sample_offset: usize) -> f32 {
    const SAMPLE_RATE: f32 = 2_048_000.0;
    let phase_step = -2.0 * std::f32::consts::PI * freq_hz / SAMPLE_RATE;
    let two_pi = 2.0 * std::f32::consts::PI;
    let phase = rot_phase + phase_step * sample_offset as f32;
    (phase + std::f32::consts::PI).rem_euclid(two_pi) - std::f32::consts::PI
}

/// Rotate `src` into `dst` applying baseband frequency `freq_hz` continuously.
/// `rot_phase` is advanced for each sample so phase is coherent across calls.
/// Mirrors DABstar's per-symbol mixer (getSamples with iDoMixer=true).
fn rotate_into(src: &[Complex32], dst: &mut [Complex32], rot_phase: &mut f32, freq_hz: f32) {
    const SAMPLE_RATE: f32 = 2_048_000.0;
    let phase_step = -2.0 * std::f32::consts::PI * freq_hz / SAMPLE_RATE;
    let two_pi = 2.0 * std::f32::consts::PI;
    for (out, s) in dst.iter_mut().zip(src.iter()) {
        *out = *s * Complex32::new(rot_phase.cos(), rot_phase.sin());
        *rot_phase += phase_step;
    }
    *rot_phase = (*rot_phase + std::f32::consts::PI).rem_euclid(two_pi) - std::f32::consts::PI;
}

#[derive(Debug, Clone)]
pub struct ReceiverConfig {
    pub channel: String,
    pub sid: u32,
    pub label: Option<String>,
    pub center_freq_hz: u32,
    pub gain: Option<u8>,
    pub hardware_agc: bool,
    pub driver_agc: bool,
    pub software_agc: bool,
    pub silent: bool,
    pub slide_dir: Option<std::path::PathBuf>,
    pub slide_base64: bool,
    pub device_index: u32,
    pub aac_decoder: AacDecoderKind,
}

impl ReceiverConfig {
    pub fn from_cli(cli: &Cli, center_freq_hz: u32) -> Self {
        Self {
            channel: cli.channel.clone(),
            sid: cli.sid,
            label: cli.label.clone(),
            center_freq_hz,
            gain: cli.gain,
            hardware_agc: cli.hardware_agc,
            driver_agc: cli.driver_agc,
            software_agc: cli.software_agc,
            silent: cli.silent,
            slide_dir: cli.slide_dir.clone(),
            slide_base64: cli.slide_base64,
            device_index: cli.device_index,
            aac_decoder: cli.aac_decoder,
        }
    }

    pub fn device_options(&self) -> DeviceOptions {
        DeviceOptions {
            index: self.device_index,
            center_freq_hz: self.center_freq_hz,
            gain: self.gain,
            hardware_agc: self.hardware_agc,
            driver_agc: self.driver_agc,
            software_agc: self.software_agc,
            silent: self.silent,
        }
    }
}

// Mirrors DABstar's DabProcessor orchestration order:
// SampleReader -> TimeSyncer -> PhaseReference -> OfdmDecoder -> FIC / MSC split.
pub struct DabProcessor {
    config: ReceiverConfig,
    fic_decoder: FicDecoder,
    fib_decoder: FibDecoder,
    msc_handler: MscHandler,
    phase_reference: PhaseReference,
    time_syncer: TimeSyncer,
    ofdm_decoder: OfdmDecoder,
    /// Smoothed sample clock error in Hz (DABstar's mClockErrHz).
    clock_err_hz: f32,
    /// Floating accumulator of the residual BB frequency correction, matching
    /// DABstar's mFreqOffsSyncSymb.
    bb_freq_offs_hz: f32,
    /// Integer Hz value actually applied by the software mixer, matching
    /// DABstar's mFreqOffsBBHz.
    bb_freq_offs_applied_hz: i32,
    /// DABstar hands a stable coarse correction over to RF once the FIC is
    /// healthy; do that once in the CLI path too.
    rf_freq_shift_used: bool,
    /// Persistent BB mixer phase accumulator, matching DABstar's SampleReader
    /// mixer continuity across getSamples() calls.
    bb_rot_phase: f32,
    /// Number of contiguous frames processed since the last acquisition/reset.
    locked_frame_count: u32,
    /// Number of consecutive PRS tracking misses while nominal framing is kept.
    prs_miss_count: u32,
    /// Pre-allocated work buffer for BB rotation of OFDM and null symbols.
    work_buf: Vec<num_complex::Complex32>,
}

impl DabProcessor {
    pub fn new(config: ReceiverConfig) -> Self {
        Self {
            fic_decoder: FicDecoder::default(),
            fib_decoder: FibDecoder::default(),
            msc_handler: MscHandler::new(DEFAULT_DAB_PLUS_BITRATE, config.aac_decoder),
            phase_reference: PhaseReference::default(),
            time_syncer: TimeSyncer::default(),
            ofdm_decoder: OfdmDecoder::default(),
            clock_err_hz: 0.0,
            bb_freq_offs_hz: 0.0,
            bb_freq_offs_applied_hz: 0,
            rf_freq_shift_used: false,
            bb_rot_phase: 0.0,
            locked_frame_count: 0,
            prs_miss_count: 0,
            work_buf: vec![num_complex::Complex32::new(0.0, 0.0); 2_656],
            config,
        }
    }

    pub fn run(
        &mut self,
        metadata: &mut MetadataWriter,
        pcm: &mut PcmOutput,
        running: Arc<AtomicBool>,
    ) -> Result<()> {
        info!("receive chain initialized");
        let device = match RtlSdrDevice::open(&self.config.device_options()) {
            Ok(device) => device,
            Err(err) => {
                warn!(error = %err, "RTL-SDR input unavailable; returning without PCM output");
                return Ok(());
            }
        };

        if let Some(label) = self.config.label.as_deref().filter(|l| !l.is_empty()) {
            metadata.write_service(self.config.sid, label)?;
        }

        let mut reader = SampleReader::new(device);
        reader.set_dc_and_iq_correction(true, false);
        self.bb_rot_phase = 0.0;
        self.locked_frame_count = 0;
        self.prs_miss_count = 0;
        self.rf_freq_shift_used = false;
        let mut ofdm_started = false;
        let mut last_dynamic_label = String::new();
        let mut ensemble_announced = false;
        let mut service_announced = false;
        let mut slide_dir_ready = false;

        // DAB Mode I: one frame = 196608 complex samples (null + 76 OFDM
        // symbols).  read_iq_block() takes a *byte* count; each complex
        // sample is 2 IQ bytes.
        const FRAME_SAMPLES: usize = 196_608;

        // ── Phase 1: initial sync ──────────────────────────────────────
        // Use frame-sized reads and accumulate a few frames, matching the
        // streaming behavior of DABstar more closely and avoiding a large
        // blocking startup read on the RTL backend.
        let mut buf: Vec<num_complex::Complex32> = Vec::new();
        while running.load(Ordering::SeqCst) {
            debug!(bytes = 2 * FRAME_SAMPLES, "reading initial IQ block");
            let iq = match reader.read_iq_block(2 * FRAME_SAMPLES) {
                Ok(iq) => iq,
                Err(err) => {
                    warn!(error = %err, "stopping: RTL-SDR read failure during initial sync");
                    return Ok(());
                }
            };
            debug!(samples = iq.len(), "initial IQ read completed");
            buf.extend_from_slice(&iq);

            if buf.len() > 4 * FRAME_SAMPLES {
                let drop = buf.len() - 4 * FRAME_SAMPLES;
                buf.drain(..drop);
            }

            self.time_syncer.set_signal_level(reader.signal_level());
            match self.time_syncer.push(&buf) {
                Some(prs_start) => {
                    let refined_prs_start = if buf.len() >= prs_start + TU {
                        self.phase_reference
                            .correlate_with_phase_ref_and_find_max_peak(
                                &buf[prs_start..(prs_start + TU)],
                                prs_threshold_for_lock_state(false),
                            )
                            .map(|peak| prs_start + peak.saturating_sub(TG))
                            .unwrap_or(prs_start)
                    } else {
                        prs_start
                    };
                    info!(
                        prs_offset = refined_prs_start,
                        buf_complex_samples = buf.len(),
                        "initial DAB frame sync acquired"
                    );
                    // Keep everything from PRS onwards; the first sample in
                    // buf is the start of a PRS. Then drop excess startup
                    // backlog so the blocking RTL reader resumes near live time.
                    buf.drain(..refined_prs_start);
                    if buf.len() > FRAME_SAMPLES {
                        buf.truncate(FRAME_SAMPLES);
                    }
                    break;
                }
                None => {
                    debug!(
                        buf_complex_samples = buf.len(),
                        "no DAB null-symbol found yet; continuing startup stream"
                    );
                    continue;
                }
            }
        }

        // ── Phase 2: continuous frame-by-frame processing ──────────────
        // Track the next PRS near the expected boundary so small sample-clock
        // drift is absorbed without allowing large false jumps.
        let mut frames_processed: u64 = 0;
        let mut pending_rf_handoff_hz: Option<i32> = None;
        while running.load(Ordering::SeqCst) {
            while buf.len() >= FRAME_SAMPLES + TU && running.load(Ordering::SeqCst) {
                // Once locked, DABstar does not rescan for a fresh null symbol each
                // frame. It evaluates the next PRS from a short preview window
                // just before the nominal boundary and tracks the local PRS peak
                // there without re-running a full null search.
                let prs_search_base = FRAME_SAMPLES.saturating_sub(TRACKING_PRS_SEARCH_BACKOFF);
                let mut prs_preview_phase = preview_rot_phase(
                    self.bb_rot_phase,
                    self.bb_freq_offs_applied_hz as f32,
                    prs_search_base,
                );
                rotate_into(
                    &buf[prs_search_base..(prs_search_base + TU)],
                    &mut self.work_buf[..TU],
                    &mut prs_preview_phase,
                    self.bb_freq_offs_applied_hz as f32,
                );
                let sample_count = if let Some(sample_count) = tracked_frame_sample_count(
                    self.phase_reference
                        .correlate_with_phase_ref_and_find_max_peak(
                            &self.work_buf[..TU],
                            prs_threshold_for_lock_state(true),
                        ),
                    prs_search_base,
                ) {
                    self.prs_miss_count = 0;
                    sample_count
                } else {
                    self.prs_miss_count = self.prs_miss_count.saturating_add(1);
                    if should_reacquire_after_prs_miss(self.prs_miss_count) {
                        debug!(
                            frames_processed,
                            prs_search_base,
                            prs_miss_count = self.prs_miss_count,
                            "lost PRS tracking; restarting time sync like DABstar"
                        );
                        self.ofdm_decoder.reset();
                        self.time_syncer = TimeSyncer::default();
                        self.locked_frame_count = 0;
                        self.prs_miss_count = 0;
                        ofdm_started = false;
                        buf.drain(..FRAME_SAMPLES.min(buf.len()));
                        while running.load(Ordering::SeqCst) {
                            debug!(bytes = 2 * FRAME_SAMPLES, "reading initial IQ block");
                            let iq = match reader.read_iq_block(2 * FRAME_SAMPLES) {
                                Ok(iq) => iq,
                                Err(err) => {
                                    warn!(error = %err, "stopping: RTL-SDR read failure during initial sync");
                                    return Ok(());
                                }
                            };
                            debug!(samples = iq.len(), "initial IQ read completed");
                            buf.extend_from_slice(&iq);

                            if buf.len() > 4 * FRAME_SAMPLES {
                                let drop = buf.len() - 4 * FRAME_SAMPLES;
                                buf.drain(..drop);
                            }

                            self.time_syncer.set_signal_level(reader.signal_level());
                            match self.time_syncer.push(&buf) {
                                Some(prs_start) => {
                                    let refined_prs_start = if buf.len() >= prs_start + TU {
                                        self.phase_reference
                                            .correlate_with_phase_ref_and_find_max_peak(
                                                &buf[prs_start..(prs_start + TU)],
                                                prs_threshold_for_lock_state(false),
                                            )
                                            .map(|peak| prs_start + peak.saturating_sub(TG))
                                            .unwrap_or(prs_start)
                                    } else {
                                        prs_start
                                    };
                                    info!(
                                        prs_offset = refined_prs_start,
                                        buf_complex_samples = buf.len(),
                                        "initial DAB frame sync acquired"
                                    );
                                    buf.drain(..refined_prs_start);
                                    if buf.len() > FRAME_SAMPLES {
                                        buf.truncate(FRAME_SAMPLES);
                                    }
                                    break;
                                }
                                None => {
                                    debug!(
                                        buf_complex_samples = buf.len(),
                                        "no DAB null-symbol found yet; continuing startup stream"
                                    );
                                    continue;
                                }
                            }
                        }
                        frames_processed += 1;
                        continue;
                    }
                    debug!(
                        frames_processed,
                        prs_search_base,
                        prs_miss_count = self.prs_miss_count,
                        "transient PRS miss; keeping nominal frame boundary"
                    );
                    FRAME_SAMPLES
                };
                let frame_end = sample_count.min(buf.len());
                let frame = &buf[..frame_end];
                if frame.len() < 76 * TS {
                    break;
                }

                // ── Symbol 0 (PRS): rotate then process ───────────────────────
                // DABstar keeps the BB mixer phase continuous across sample reads;
                // do the same here by preserving `self.bb_rot_phase` across frames.
                rotate_into(
                    &frame[..TS],
                    &mut self.work_buf,
                    &mut self.bb_rot_phase,
                    self.bb_freq_offs_applied_hz as f32,
                );
                let symbol_0_bins = self.ofdm_decoder.symbol_0_bins(&self.work_buf);
                self.ofdm_decoder
                    .store_reference_symbol_0_bins(&symbol_0_bins);
                let mut coarse_correction_hz = 0;
                if self.fic_decoder.decode_ratio_percent() < 30 {
                    if let Some(correction_hz) = self
                        .phase_reference
                        .estimate_carrier_offset_from_sync_symbol_0(&symbol_0_bins)
                    {
                        if coarse_afc_should_apply(correction_hz) {
                            coarse_correction_hz = correction_hz;
                            self.bb_freq_offs_hz += correction_hz as f32;
                            self.bb_freq_offs_hz = self.bb_freq_offs_hz.clamp(-35_000.0, 35_000.0);
                            self.bb_freq_offs_applied_hz = self.bb_freq_offs_hz.round() as i32;
                            self.clock_err_hz = 0.0;
                            debug!(
                                correction_hz,
                                bb_freq_offs_hz = self.bb_freq_offs_hz,
                                bb_freq_offs_applied_hz = self.bb_freq_offs_applied_hz,
                                "applied coarse PRS frequency correction"
                            );
                        }
                    }
                }
                if should_transfer_correction_to_rf(
                    self.fic_decoder.decode_ratio_percent(),
                    frames_processed,
                    self.bb_freq_offs_applied_hz,
                    self.rf_freq_shift_used,
                ) {
                    pending_rf_handoff_hz = Some(self.bb_freq_offs_applied_hz);
                }

                if coarse_afc_requires_reacquisition(coarse_correction_hz) {
                    debug!(
                        frames_processed,
                        sample_count,
                        bb_freq_offs_hz = self.bb_freq_offs_hz,
                        "coarse correction changed; restarting on next frame like DABstar"
                    );
                    self.ofdm_decoder.reset();
                    self.time_syncer = TimeSyncer::default();
                    self.locked_frame_count = 0;
                    ofdm_started = false;
                    buf.drain(..sample_count.min(buf.len()));
                    while running.load(Ordering::SeqCst) {
                        debug!(bytes = 2 * FRAME_SAMPLES, "reading initial IQ block");
                        let iq = match reader.read_iq_block(2 * FRAME_SAMPLES) {
                            Ok(iq) => iq,
                            Err(err) => {
                                warn!(error = %err, "stopping: RTL-SDR read failure during initial sync");
                                return Ok(());
                            }
                        };
                        debug!(samples = iq.len(), "initial IQ read completed");
                        buf.extend_from_slice(&iq);

                        if buf.len() > 4 * FRAME_SAMPLES {
                            let drop = buf.len() - 4 * FRAME_SAMPLES;
                            buf.drain(..drop);
                        }

                        self.time_syncer.set_signal_level(reader.signal_level());
                        match self.time_syncer.push(&buf) {
                            Some(prs_start) => {
                                let refined_prs_start = if buf.len() >= prs_start + TU {
                                    self.phase_reference
                                        .correlate_with_phase_ref_and_find_max_peak(
                                            &buf[prs_start..(prs_start + TU)],
                                            prs_threshold_for_lock_state(false),
                                        )
                                        .map(|peak| prs_start + peak.saturating_sub(TG))
                                        .unwrap_or(prs_start)
                                } else {
                                    prs_start
                                };
                                info!(
                                    prs_offset = refined_prs_start,
                                    buf_complex_samples = buf.len(),
                                    "initial DAB frame sync acquired"
                                );
                                buf.drain(..refined_prs_start);
                                if buf.len() > FRAME_SAMPLES {
                                    buf.truncate(FRAME_SAMPLES);
                                }
                                break;
                            }
                            None => {
                                debug!(
                                    buf_complex_samples = buf.len(),
                                    "no DAB null-symbol found yet; continuing startup stream"
                                );
                                continue;
                            }
                        }
                    }
                    frames_processed += 1;
                    continue;
                }

                // ── Symbols 1–75: rotate each symbol then process ─────────────
                // `self.bb_rot_phase` continues from the prior symbol and frame,
                // matching DABstar's continuous per-sample mixer.
                let mut frame_freq_corr = Complex32::new(0.0, 0.0);
                let mut frame_cp_energy = 0.0f32;

                for ofdm_symbol_idx in 1usize..76 {
                    if !running.load(Ordering::SeqCst) {
                        break;
                    }

                    let symbol_start = ofdm_symbol_idx * TS;
                    let symbol_end = symbol_start + TS;
                    if symbol_end > frame.len() {
                        break;
                    }

                    // Apply per-symbol BB rotation (mirrors DABstar's getSamples mixer).
                    rotate_into(
                        &frame[symbol_start..symbol_end],
                        &mut self.work_buf,
                        &mut self.bb_rot_phase,
                        self.bb_freq_offs_applied_hz as f32,
                    );
                    let symbol = self.work_buf.as_slice();

                    for idx in TU..TS {
                        let a = symbol[idx];
                        let b = symbol[idx - TU];
                        frame_freq_corr += a * b.conj(); // eval phase shift in cyclic prefix part
                        frame_cp_energy += a.norm_sqr() + b.norm_sqr();
                    }

                    let phase_corr = self.phase_reference.analyze(symbol);
                    let soft_bits =
                        self.ofdm_decoder
                            .process_symbol(symbol, phase_corr, self.clock_err_hz);

                    if ofdm_symbol_idx <= 3 {
                        if ofdm_symbol_idx == 1 {
                            // DABstar resets the FIC accumulator at the start of each frame.
                            self.fic_decoder.reset_frame();
                        }
                        for fib in self.fic_decoder.push_soft_bits(&soft_bits) {
                            self.fib_decoder.process_fib(&fib);
                        }
                    } else {
                        if let Some(service_info) = self
                            .fib_decoder
                            .selected_audio_service(self.config.sid, self.config.label.as_deref())
                        {
                            self.msc_handler.configure_service(service_info)?;
                        }
                        let samples = self
                            .msc_handler
                            .process_block(&soft_bits, ofdm_symbol_idx)?;
                        if !samples.is_empty() {
                            pcm.write_interleaved(&samples)?;
                        }
                    }

                    if ofdm_symbol_idx == 1 && !ofdm_started {
                        info!(phase_corr_rad = phase_corr, "OFDM decode started");
                        ofdm_started = true;
                    }
                }

                if frame.len() >= 76 * TS + 2_656
                    && frame_uses_non_tii_null(self.fib_decoder.cif_count(), frames_processed)
                {
                    let null_start = 76 * TS;
                    let null_end = null_start + 2_656;
                    rotate_into(
                        &frame[null_start..null_end],
                        &mut self.work_buf,
                        &mut self.bb_rot_phase,
                        self.bb_freq_offs_applied_hz as f32,
                    );
                    self.ofdm_decoder
                        .store_null_symbol_without_tii(&self.work_buf[..(null_end - null_start)]);
                }

                if !coarse_afc_should_apply(coarse_correction_hz) {
                    let raw_err = 2_048_000.0 * (sample_count as f32 / FRAME_SAMPLES as f32 - 1.0);
                    let clamped = raw_err.clamp(-307.2, 307.2);
                    self.clock_err_hz = 0.9 * self.clock_err_hz + 0.1 * clamped;
                }

                // Use the DABstar cyclic-prefix phase estimate, but gate and damp
                // the update in the buffered CLI path when coherence is weak.
                let frame_phase_offset_rad = if frame_freq_corr.norm_sqr() > 1.0e-12 {
                    frame_freq_corr.arg()
                } else {
                    0.0
                };
                let phase_limit_rad = 20.0_f32.to_radians();
                let limited_phase_offset_rad =
                    frame_phase_offset_rad.clamp(-phase_limit_rad, phase_limit_rad);
                let cp_coherence = normalized_cp_coherence(frame_freq_corr, frame_cp_energy);
                let fine_gain = fine_afc_gain(self.locked_frame_count, cp_coherence);
                self.bb_freq_offs_hz +=
                    fine_gain * limited_phase_offset_rad / (2.0 * std::f32::consts::PI) * 1_000.0;
                self.bb_freq_offs_hz = self.bb_freq_offs_hz.clamp(-35_000.0, 35_000.0);
                self.bb_freq_offs_applied_hz = self.bb_freq_offs_hz.round() as i32;

                debug!(
                    frames_processed,
                    sample_count,
                    clock_err_hz = self.clock_err_hz,
                    bb_freq_offs_hz = self.bb_freq_offs_hz,
                    bb_freq_offs_applied_hz = self.bb_freq_offs_applied_hz,
                    frame_phase_offset_rad,
                    limited_phase_offset_rad,
                    cp_coherence,
                    fine_gain,
                    "tracked next OFDM frame boundary"
                );

                buf.drain(..sample_count.min(buf.len()));
                frames_processed += 1;
                self.locked_frame_count = self.locked_frame_count.saturating_add(1);
            }

            if let Some(rf_shift_hz) = pending_rf_handoff_hz.take() {
                let shifted_center = (u64::from(reader.center_freq_hz()) as i64
                    + i64::from(rf_shift_hz))
                .clamp(0, u32::MAX as i64) as u32;
                if let Err(err) = reader.set_center_freq_hz(shifted_center) {
                    warn!(error = %err, shifted_center, "failed to transfer BB frequency correction to RF");
                } else if let Err(err) = reader.reset_buffer() {
                    warn!(error = %err, "failed to reset RTL-SDR buffer after RF retune");
                } else {
                    self.rf_freq_shift_used = true;
                    self.bb_freq_offs_hz = 0.0;
                    self.bb_freq_offs_applied_hz = 0;
                    self.bb_rot_phase = 0.0;
                    self.clock_err_hz = 0.0;
                    self.ofdm_decoder.reset();
                    self.time_syncer = TimeSyncer::default();
                    self.locked_frame_count = 0;
                    self.prs_miss_count = 0;
                    ofdm_started = false;
                    buf.clear();
                    info!(
                        shifted_center,
                        rf_shift_hz, "transferred stable coarse frequency correction to RTL-SDR"
                    );
                    continue;
                }
            }

            // Read one more frame of complex IQ (= 2 * FRAME_SAMPLES bytes).
            let new_iq = match reader.read_iq_block(2 * FRAME_SAMPLES) {
                Ok(iq) => iq,
                Err(err) => {
                    warn!(error = %err, "stopping receive loop after RTL-SDR read failure");
                    break;
                }
            };
            buf.extend_from_slice(&new_iq);

            // Metadata updates (once per read iteration).
            if !ensemble_announced {
                if let Some(label) = self.fib_decoder.ensemble_label() {
                    metadata.write_ensemble(
                        u32::from(self.fib_decoder.ensemble_id().unwrap_or(0)),
                        label,
                    )?;
                    ensemble_announced = true;
                }
            }
            if !service_announced {
                if let Some(label) = self.fib_decoder.service_label_for_sid(self.config.sid) {
                    metadata.write_service(self.config.sid, label)?;
                    service_announced = true;
                }
            }
            if let Some(dl) = self.msc_handler.last_dynamic_label() {
                if dl != last_dynamic_label {
                    metadata.write_dynamic_label(dl)?;
                    last_dynamic_label.clear();
                    last_dynamic_label.push_str(dl);
                }
            }
            if let Some(dir) = &self.config.slide_dir {
                if !slide_dir_ready {
                    let _ = std::fs::create_dir_all(dir);
                    slide_dir_ready = true;
                }
            }
            if let Some(slide) = self.msc_handler.take_last_slide() {
                metadata.write_slide(
                    &slide.content_name,
                    &slide.content_type,
                    &slide.data,
                    self.config.slide_base64,
                )?;
                if let Some(dir) = &self.config.slide_dir {
                    let _ = metadata.save_slide_to_dir(dir, &slide.content_name, &slide.data);
                }
            }
        }

        info!(
            channel = %self.config.channel,
            frames_processed,
            fic_decode_ratio = self.fic_decoder.decode_ratio_percent(),
            service_count = self.fib_decoder.service_count(),
            "receive pass completed"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use num_complex::Complex32;

    use super::{
        coarse_afc_requires_reacquisition, coarse_afc_should_apply, fine_afc_gain,
        frame_uses_non_tii_null, normalized_cp_coherence, preview_rot_phase,
        prs_threshold_for_lock_state, should_reacquire_after_prs_miss,
        should_transfer_correction_to_rf, tracked_frame_sample_count, TRACKING_PRS_SEARCH_BACKOFF,
    };
    use crate::ofdm::ofdm_decoder::TG;

    #[test]
    fn uses_dabstar_prs_thresholds() {
        assert_eq!(prs_threshold_for_lock_state(false), 3.0);
        assert_eq!(prs_threshold_for_lock_state(true), 6.0);
    }

    #[test]
    fn follows_dabstar_non_tii_null_cadence() {
        for frame in 0..4 {
            assert!(frame_uses_non_tii_null(None, frame));
        }
        for frame in 4..8 {
            assert!(!frame_uses_non_tii_null(None, frame));
        }
    }

    #[test]
    fn prefers_real_cif_count_for_null_tii_phase() {
        assert!(frame_uses_non_tii_null(Some(3), 7));
        assert!(!frame_uses_non_tii_null(Some(5), 0));
    }

    #[test]
    fn coarse_afc_applies_and_forces_reacquisition_on_nonzero_step() {
        assert!(coarse_afc_should_apply(37));
        assert!(!coarse_afc_should_apply(0));
        assert!(coarse_afc_requires_reacquisition(37));
        assert!(coarse_afc_requires_reacquisition(-4));
        assert!(!coarse_afc_requires_reacquisition(0));
    }

    #[test]
    fn receiver_config_carries_slide_base64_flag() {
        let cli = crate::cli::Cli::try_parse_from([
            "dabctl",
            "-C",
            "6C",
            "-s",
            "0xF2F8",
            "--slide-base64",
        ])
        .expect("CLI should parse slide-base64");

        let cfg = super::ReceiverConfig::from_cli(&cli, 220_352_000);
        assert!(cfg.slide_base64);
    }

    #[test]
    fn coarse_afc_restart_is_required_after_frequency_step() {
        assert!(coarse_afc_requires_reacquisition(1));
        assert!(coarse_afc_requires_reacquisition(-37));
        assert!(!coarse_afc_requires_reacquisition(0));
    }

    #[test]
    fn rf_handoff_is_deferred_until_stable_and_large() {
        assert!(!should_transfer_correction_to_rf(100, 6, 26, false));
        assert!(!should_transfer_correction_to_rf(70, 10, 600, false));
        assert!(!should_transfer_correction_to_rf(100, 2, 600, false));
        assert!(!should_transfer_correction_to_rf(100, 10, 600, true));
        assert!(should_transfer_correction_to_rf(100, 10, 600, false));
    }

    #[test]
    fn normalized_cp_coherence_is_zero_without_energy() {
        assert_eq!(normalized_cp_coherence(Complex32::new(1.0, 0.0), 0.0), 0.0);
    }

    #[test]
    fn fine_afc_gain_is_gated_but_dabstar_strength_when_coherent() {
        assert_eq!(fine_afc_gain(0, 0.01), 0.0);
        assert!(fine_afc_gain(0, 0.04) > 0.0);
        assert_eq!(fine_afc_gain(0, 0.10), 1.0);
        assert_eq!(fine_afc_gain(25, 0.10), 1.0);
    }

    #[test]
    fn centered_prs_tracking_allows_early_frame_adjustment() {
        const FRAME_SAMPLES: usize = 196_608;
        const SEARCH_BASE: usize = FRAME_SAMPLES - TG;
        assert_eq!(TRACKING_PRS_SEARCH_BACKOFF, TG);
        assert_eq!(
            tracked_frame_sample_count(Some(384), SEARCH_BASE),
            Some(FRAME_SAMPLES - 120)
        );
        assert_eq!(
            tracked_frame_sample_count(Some(504), SEARCH_BASE),
            Some(FRAME_SAMPLES)
        );
        assert_eq!(tracked_frame_sample_count(None, SEARCH_BASE), None);
    }

    #[test]
    fn transient_prs_misses_do_not_force_immediate_reacquisition() {
        assert!(!should_reacquire_after_prs_miss(1));
        assert!(!should_reacquire_after_prs_miss(2));
        assert!(should_reacquire_after_prs_miss(3));
    }

    #[test]
    fn preview_phase_matches_live_mixer_step() {
        let phase0 = preview_rot_phase(0.0, 1_000.0, 1_024);
        assert!(phase0.is_finite());
        assert!(phase0.abs() > 0.001);
    }
}
