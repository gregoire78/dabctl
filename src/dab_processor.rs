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

fn frame_uses_non_tii_null(frame_index: u64) -> bool {
    (frame_index & 0x7) < 4
}

fn coarse_afc_should_apply(correction_hz: i32) -> bool {
    correction_hz != 0
}

fn coarse_afc_requires_reacquisition(_correction_hz: i32) -> bool {
    // DABstar applies the updated BB offset and continues decoding the
    // current frame. It only skips clock-error integration for frames where a
    // non-zero coarse step was applied.
    false
}

/// Rotate `src` into `dst` applying baseband frequency `freq_hz` continuously.
/// `rot_phase` is advanced for each sample so phase is coherent across calls.
/// Mirrors DABstar's per-symbol mixer (getSamples with iDoMixer=true).
fn rotate_into(src: &[Complex32], dst: &mut [Complex32], rot_phase: &mut f32, freq_hz: f32) {
    const SAMPLE_RATE: f32 = 2_048_000.0;
    let phase_step = -2.0 * std::f32::consts::PI * freq_hz / SAMPLE_RATE;
    for (out, s) in dst.iter_mut().zip(src.iter()) {
        *out = *s * Complex32::new(rot_phase.cos(), rot_phase.sin());
        *rot_phase += phase_step;
    }
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
    /// Baseband frequency correction (applied per-symbol during demodulation).
    bb_freq_offs_hz: f32,
    /// Persistent BB mixer phase accumulator, matching DABstar's SampleReader
    /// mixer continuity across getSamples() calls.
    bb_rot_phase: f32,
    /// Pre-allocated work buffer for per-symbol BB rotation (size = TS).
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
            bb_rot_phase: 0.0,
            work_buf: vec![num_complex::Complex32::new(0.0, 0.0); TS],
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
        let mut ofdm_started = false;
        let mut last_dynamic_label = String::new();
        let mut ensemble_announced = false;
        let mut service_announced = false;
        let mut slide_dir_ready = false;

        // DAB Mode I: one frame = 196608 complex samples (null + 76 OFDM
        // symbols).  read_iq_block() takes a *byte* count; each complex
        // sample is 2 IQ bytes.
        const FRAME_SAMPLES: usize = 196_608;
        const FRAME_ALIGN_SEARCH: usize = 6 * TS;
        const FRAME_TRACK_TOLERANCE: usize = 2_048;

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
        while running.load(Ordering::SeqCst) {
            while buf.len() >= FRAME_SAMPLES + FRAME_ALIGN_SEARCH && running.load(Ordering::SeqCst)
            {
                let search_start = FRAME_SAMPLES.saturating_sub(FRAME_ALIGN_SEARCH);
                let search_end = (FRAME_SAMPLES + FRAME_ALIGN_SEARCH).min(buf.len());
                let sample_count = self
                    .time_syncer
                    .track_near(
                        &buf[search_start..search_end],
                        FRAME_ALIGN_SEARCH,
                        FRAME_TRACK_TOLERANCE,
                    )
                    .map(|rel| search_start + rel)
                    .unwrap_or(FRAME_SAMPLES);
                let sample_count = if buf.len() >= sample_count + TU {
                    self.phase_reference
                        .correlate_with_phase_ref_and_find_max_peak(
                            &buf[sample_count..(sample_count + TU)],
                            prs_threshold_for_lock_state(true),
                        )
                        .map(|peak| sample_count + peak.saturating_sub(TG))
                        .unwrap_or(sample_count)
                } else {
                    sample_count
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
                    self.bb_freq_offs_hz,
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
                            self.clock_err_hz = 0.0;
                            debug!(
                                correction_hz,
                                bb_freq_offs_hz = self.bb_freq_offs_hz,
                                "applied coarse PRS frequency correction"
                            );
                        }
                    }
                }
                if coarse_afc_requires_reacquisition(coarse_correction_hz) {
                    debug!(
                        frames_processed,
                        sample_count,
                        bb_freq_offs_hz = self.bb_freq_offs_hz,
                        "coarse correction changed; restarting on next frame like DABstar"
                    );
                    buf.drain(..sample_count.min(buf.len()));
                    frames_processed += 1;
                    continue;
                }

                // ── Symbols 1–75: rotate each symbol then process ─────────────
                // `self.bb_rot_phase` continues from the prior symbol and frame,
                // matching DABstar's continuous per-sample mixer.
                let mut frame_freq_corr = Complex32::new(0.0, 0.0);

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
                        self.bb_freq_offs_hz,
                    );
                    let symbol = self.work_buf.as_slice();

                    for idx in TU..TS {
                        let a = symbol[idx];
                        let b = symbol[idx - TU];
                        frame_freq_corr += a * b.conj(); // eval phase shift in cyclic prefix part
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

                if frame.len() >= 76 * TS + 2_656 && frame_uses_non_tii_null(frames_processed) {
                    let null_start = 76 * TS;
                    let null_end = null_start + 2_656;
                    // Null symbol: power measurement only, rotation not needed.
                    self.ofdm_decoder
                        .store_null_symbol_without_tii(&frame[null_start..null_end]);
                }

                if !coarse_afc_should_apply(coarse_correction_hz) {
                    let raw_err = 2_048_000.0 * (sample_count as f32 / FRAME_SAMPLES as f32 - 1.0);
                    let clamped = raw_err.clamp(-307.2, 307.2);
                    self.clock_err_hz = 0.9 * self.clock_err_hz + 0.1 * clamped;
                }

                // DABstar: always integrate the full cyclic-prefix phase error (gain=1.0).
                // limit_symmetrically to ±20° then convert rad → Hz at 1000 Hz/carrier.
                let frame_phase_offset_rad = if frame_freq_corr.norm_sqr() > 1.0e-12 {
                    frame_freq_corr.arg()
                } else {
                    0.0
                };
                let phase_limit_rad = 20.0_f32.to_radians();
                let limited_phase_offset_rad =
                    frame_phase_offset_rad.clamp(-phase_limit_rad, phase_limit_rad);
                self.bb_freq_offs_hz +=
                    limited_phase_offset_rad / (2.0 * std::f32::consts::PI) * 1_000.0;
                self.bb_freq_offs_hz = self.bb_freq_offs_hz.clamp(-35_000.0, 35_000.0);
                // bb_freq_offs_hz is applied per-symbol at processing time; no
                // reader.set_bb_freq_offset_hz() call needed.

                debug!(
                    frames_processed,
                    sample_count,
                    clock_err_hz = self.clock_err_hz,
                    bb_freq_offs_hz = self.bb_freq_offs_hz,
                    frame_phase_offset_rad,
                    limited_phase_offset_rad,
                    "tracked next OFDM frame boundary"
                );

                buf.drain(..sample_count.min(buf.len()));
                frames_processed += 1;
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
                    metadata.write_ensemble(0, label)?;
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

    use super::{
        coarse_afc_requires_reacquisition, coarse_afc_should_apply, frame_uses_non_tii_null,
        prs_threshold_for_lock_state,
    };

    #[test]
    fn uses_dabstar_prs_thresholds() {
        assert_eq!(prs_threshold_for_lock_state(false), 3.0);
        assert_eq!(prs_threshold_for_lock_state(true), 6.0);
    }

    #[test]
    fn follows_dabstar_non_tii_null_cadence() {
        for frame in 0..4 {
            assert!(frame_uses_non_tii_null(frame));
        }
        for frame in 4..8 {
            assert!(!frame_uses_non_tii_null(frame));
        }
    }

    #[test]
    fn coarse_afc_applies_without_forcing_reacquisition() {
        assert!(coarse_afc_should_apply(37));
        assert!(!coarse_afc_should_apply(0));
        assert!(!coarse_afc_requires_reacquisition(37));
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
    fn dabstar_coarse_afc_does_not_force_reacquisition() {
        assert!(!coarse_afc_requires_reacquisition(1));
        assert!(!coarse_afc_requires_reacquisition(-37));
        assert!(!coarse_afc_requires_reacquisition(0));
    }
}
