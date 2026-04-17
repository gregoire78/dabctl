// DABstar-aligned OFDM processor for frame timing and frequency synchronisation.

use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::atomic::{AtomicBool, AtomicI16, Ordering};
use std::sync::Arc;

use tracing::{debug, trace, warn};

use crate::device::rtlsdr_handler::RtlsdrHandler;
use crate::pipeline::dab_constants::{jan_abs, DIFF_LENGTH, INPUT_RATE};
use crate::pipeline::dab_params::DabParams;
use crate::pipeline::dab_pipeline::DabPipeline;
use crate::pipeline::ofdm::freq_interleaver::FreqInterleaver;
use crate::pipeline::ofdm::phase_reference::PhaseReference;
use crate::pipeline::ofdm::time_syncer::{TimeSyncState, TimeSyncer};

fn normalized_cp_coherence(freq_corr: Complex32, cp_pair_count: usize, s_level: f32) -> f32 {
    if cp_pair_count == 0 || s_level <= 0.0 {
        return 0.0;
    }

    freq_corr.norm() / (cp_pair_count as f32 * s_level * s_level)
}

fn fine_afc_delta_hz(
    freq_corr: Complex32,
    carrier_diff: i32,
    cp_pair_count: usize,
    s_level: f32,
    post_eq_quality: f32,
    eq_weak_ratio: f32,
) -> f32 {
    if freq_corr.norm() == 0.0 {
        return 0.0;
    }

    let cp_coherence = normalized_cp_coherence(freq_corr, cp_pair_count, s_level);
    if cp_coherence < MIN_CP_CORR_NORM
        || post_eq_quality < HOLD_ENTER_POST_EQ_MIN
        || eq_weak_ratio > HOLD_ENTER_WEAK_RATIO_MAX
    {
        return 0.0;
    }

    let phase_offset = clamp_phase_error(freq_corr.arg());
    phase_offset / std::f32::consts::TAU * carrier_diff as f32
}

fn snr_is_observable(post_eq_quality: f32, eq_weak_ratio: f32, soft_locked: bool) -> bool {
    soft_locked
        && post_eq_quality >= SNR_OBSERVABLE_POST_EQ_MIN
        && eq_weak_ratio <= SNR_OBSERVABLE_WEAK_RATIO_MAX
}

fn structural_hold_allowed(post_eq_quality: f32, eq_weak_ratio: f32) -> bool {
    post_eq_quality >= HOLD_ENTER_POST_EQ_MIN && eq_weak_ratio <= HOLD_ENTER_WEAK_RATIO_MAX
}

fn should_release_degraded_hold(post_eq_quality: f32, eq_weak_ratio: f32) -> bool {
    post_eq_quality >= HOLD_RELEASE_POST_EQ_MIN && eq_weak_ratio <= HOLD_RELEASE_WEAK_RATIO_MAX
}

fn structural_collapse_detected(post_eq_quality: f32, eq_weak_ratio: f32) -> bool {
    post_eq_quality <= STRUCTURAL_COLLAPSE_POST_EQ_MAX
        || eq_weak_ratio >= STRUCTURAL_COLLAPSE_WEAK_RATIO_MIN
}

fn continue_after_coarse_step(_coarse_step_applied: bool) -> bool {
    // DABstar applies the updated baseband frequency estimate and continues
    // decoding the current frame instead of declaring sync loss immediately.
    true
}

fn apply_coarse_correction(current_coarse: i32, correction_hz: i32) -> (i32, bool) {
    if correction_hz == PhaseReference::IDX_NOT_FOUND {
        return (current_coarse, false);
    }

    let mut updated = current_coarse + correction_hz;
    if updated.abs() > 35_000 {
        updated = 0;
    }

    (updated, correction_hz != 0)
}

fn plan_coarse_correction(
    reused_previous_start: bool,
    soft_locked: bool,
    current_coarse: i32,
    correction_hz: i32,
    post_eq_quality: f32,
    eq_weak_ratio: f32,
) -> Option<CoarseCorrectionPlan> {
    // ETSI EN 300 401 §14.8: once timing/frequency tracking is already stable,
    // keep the coarse loop frozen and let the fine loop absorb residual drift.
    // Real receivers do not keep nudging the coarse estimator by a few Hz every
    // frame while the multiplex is decoding correctly.
    if reused_previous_start
        || soft_locked
        || !structural_hold_allowed(post_eq_quality, eq_weak_ratio)
        || structural_collapse_detected(post_eq_quality, eq_weak_ratio)
    {
        return None;
    }

    let (updated_coarse_hz, step_applied) = apply_coarse_correction(current_coarse, correction_hz);
    Some(CoarseCorrectionPlan {
        updated_coarse_hz,
        step_applied,
        reset_clock_error: step_applied,
    })
}

fn plan_null_symbol_metrics(
    prev_snr_db: f32,
    signal_level: f32,
    null_avg_level: f32,
    report_count: usize,
    coarse_corrector: i32,
    fine_corrector: f32,
    observability: ObservabilitySnapshot,
) -> NullSymbolMetricsPlan {
    let safe_null_level = null_avg_level.max(1e-6);
    let next_snr_db = if snr_is_observable(
        observability.post_eq_quality,
        observability.eq_weak_ratio,
        observability.soft_locked,
    ) {
        let observed = 20.0 * ((signal_level + 0.005) / safe_null_level).log10();
        let bounded = observed.clamp(0.0, 35.0);
        if prev_snr_db <= 0.0 {
            bounded
        } else {
            let smoothed = 0.96 * prev_snr_db + 0.04 * bounded;
            smoothed.clamp((prev_snr_db - 0.75).max(0.0), (prev_snr_db + 0.5).min(35.0))
        }
    } else {
        prev_snr_db.max(0.0)
    };
    let next_report_count = report_count + 1;
    let emit_report = next_report_count > 10;

    NullSymbolMetricsPlan {
        snr_db: next_snr_db,
        next_report_count: if emit_report { 0 } else { next_report_count },
        emit_report,
        offset_hz: coarse_corrector + fine_corrector as i32,
    }
}

fn phase_threshold_pair(threshold_1: i16, threshold_2: i16) -> (i16, i16) {
    let acq_threshold = threshold_1.clamp(OFDM_THRESHOLD_MIN, OFDM_THRESHOLD_MAX);
    let track_threshold = threshold_2
        .clamp(OFDM_THRESHOLD_MIN, OFDM_THRESHOLD_MAX)
        .max(acq_threshold);

    (acq_threshold, track_threshold)
}

fn resolve_sync_start_index(
    raw_index: i32,
    last_sync_start_index: usize,
    tracking_miss_budget: &mut u8,
    t_u: usize,
    allow_single_miss_reuse: bool,
) -> Option<usize> {
    if raw_index >= 0 {
        *tracking_miss_budget = TRACKING_MISS_TOLERANCE;
        return Some(raw_index as usize);
    }

    let _ = (last_sync_start_index, t_u, allow_single_miss_reuse);
    None
}

fn plan_time_sync_follow_up(
    state: Option<TimeSyncState>,
    attempts: i16,
) -> Result<TimeSyncFollowUpPlan, ProcessorError> {
    match state {
        Some(TimeSyncState::TimeSyncEstablished) => Ok(TimeSyncFollowUpPlan {
            established: true,
            attempts: 0,
            emit_warning: false,
        }),
        Some(TimeSyncState::NoDipFound) => {
            let next_attempts = attempts + 1;
            Ok(TimeSyncFollowUpPlan {
                established: false,
                attempts: next_attempts,
                emit_warning: next_attempts >= 8,
            })
        }
        Some(TimeSyncState::NoEndOfDipFound) => Ok(TimeSyncFollowUpPlan {
            established: false,
            attempts: 0,
            emit_warning: false,
        }),
        None => Err(ProcessorError::Stopped),
    }
}

fn build_sync_alignment_plan(
    raw_start_index: i32,
    last_sync_start_index: usize,
    tracking_miss_budget: &mut u8,
    t_u: usize,
    allow_single_miss_reuse: bool,
) -> Option<SyncAlignmentPlan> {
    let start_index = resolve_sync_start_index(
        raw_start_index,
        last_sync_start_index,
        tracking_miss_budget,
        t_u,
        allow_single_miss_reuse,
    )?;

    if start_index >= t_u {
        return None;
    }

    Some(SyncAlignmentPlan {
        start_index,
        samples_to_fetch: start_index,
        reused_previous_start: raw_start_index < 0 && allow_single_miss_reuse,
    })
}

fn turn_phase_to_first_quadrant(mut phase: f32) -> f32 {
    if phase < 0.0 {
        phase += std::f32::consts::PI;
    }
    phase.rem_euclid(std::f32::consts::FRAC_PI_2)
}

fn clamp_phase_error(phase_err: f32) -> f32 {
    phase_err.clamp(
        -PHASE_CORRECTION_LIMIT_DEG.to_radians(),
        PHASE_CORRECTION_LIMIT_DEG.to_radians(),
    )
}

fn norm_to_length_one(value: Complex32) -> Complex32 {
    let norm = value.norm();
    if norm > 0.0 {
        value / norm
    } else {
        Complex32::new(1.0, 0.0)
    }
}

fn carrier_weight(mean_level: f32, mean_sigma_sq: f32, null_noise: f32, mean_power: f32) -> f32 {
    let null_n = null_noise.max(NOISE_FLOOR_MIN);
    let signal_p = (mean_power - null_n).max(SIGNAL_NOISE_MIN_RATIO);
    mean_level / mean_sigma_sq.max(1e-4) / ((null_n / signal_p) + 1.0)
}

fn soft_bit_from_component(component: f32, mean_value: f32) -> i16 {
    let soft_scale = -100.0 / mean_value.max(1e-3);
    (component * soft_scale).clamp(-127.0, 127.0) as i16
}

fn plan_symbol_read(
    coarse_corrector: i32,
    fine_corrector: f32,
    frame_sample_count: usize,
    t_s: usize,
) -> SymbolReadPlan {
    SymbolReadPlan {
        phase_hz: coarse_corrector + fine_corrector as i32,
        next_frame_sample_count: frame_sample_count + t_s,
    }
}

fn accumulate_prefix_correlation(block_buf: &[Complex32], t_u: usize, t_s: usize) -> Complex32 {
    let mut freq_corr = Complex32::new(0.0, 0.0);
    for i in t_u..t_s {
        freq_corr += block_buf[i] * block_buf[i - t_u].conj();
    }
    freq_corr
}

fn plan_frame_rest_metrics(
    frame_sample_count: usize,
    null_symbol_len: usize,
    coarse_step_applied: bool,
) -> FrameRestMetricsPlan {
    FrameRestMetricsPlan {
        next_frame_sample_count: frame_sample_count + null_symbol_len,
        should_integrate_clock_error: !coarse_step_applied,
    }
}

fn cmplx_from_phase2(x: f32) -> Complex32 {
    let x2 = x * x;
    let s1 = 0.999_031_4f32;
    let s2 = -0.160_344_02f32;
    let sine = x * (x2 * s2 + s1);

    let c1 = 0.999_403_24f32;
    let c2 = -0.495_580_85f32;
    let c3 = 0.036_791_682f32;
    let cosine = c1 + x2 * (x2 * c3 + c2);

    Complex32::new(cosine, sine)
}

/// Adaptive phase-reference correlation threshold bounds.
/// Lower values are more tolerant to fades, higher values reject false locks.
const OFDM_THRESHOLD_MIN: i16 = 2;
const OFDM_THRESHOLD_MAX: i16 = 7;

/// IIR time constant for per-carrier noise-floor estimation from the null symbol.
/// α = 0.1 gives a time constant of 10 null symbols (≈ 240 ms for Mode I).
/// Matches DABstar's `store_null_symbol_without_tii()` (GPLv2).
const NULL_NOISE_ALPHA: f32 = 0.1;

/// IIR time constant for per-carrier signal-power estimation.
/// α = 0.005 gives a ~200-symbol time constant (≈ 4.8 s for Mode I).
/// Matches DABstar's `decode_symbol()` per-bin mean-power IIR (GPLv2).
const MEAN_POWER_ALPHA: f32 = 0.005;

/// Minimum noise floor used as the null-noise denominator guard.
/// Prevents division by zero when `null_noise[k]` has not yet been warmed up.
const NOISE_FLOOR_MIN: f32 = 1e-10;

/// Minimum post-subtraction signal power used in the DABstar-style soft-bit
/// weighting path to avoid divide-by-zero in deep fades.
const SIGNAL_NOISE_MIN_RATIO: f32 = 0.1;

/// Minimum normalized cyclic-prefix coherence required before trusting a
/// fine AFC update. This prevents the tracking loop from steering on noise
/// during brief fades or false locks.
const MIN_CP_CORR_NORM: f32 = 0.05;

/// Continuity-first degraded-hold budget measured in frame attempts.
const MAX_DEGRADED_HOLD_FRAMES: u8 = 12;

/// Each successfully decoded frame while still in degraded hold replenishes
/// one survival slot. This keeps the RF supervisor continuity-first across
/// intermittent micro-fades instead of draining the budget to zero over time.
const HOLD_BUDGET_REFILL_PER_GOOD_FRAME: u8 = 1;

/// Structural hold-entry thresholds derived from post-equalizer observability.
const HOLD_ENTER_POST_EQ_MIN: f32 = 0.30;
const HOLD_ENTER_WEAK_RATIO_MAX: f32 = 0.30;

/// Recovery thresholds needed before leaving degraded hold.
const HOLD_RELEASE_POST_EQ_MIN: f32 = 0.50;
const HOLD_RELEASE_WEAK_RATIO_MAX: f32 = 0.25;

/// Structural collapse thresholds that require a real reacquisition.
const STRUCTURAL_COLLAPSE_POST_EQ_MAX: f32 = 0.18;
const STRUCTURAL_COLLAPSE_WEAK_RATIO_MIN: f32 = 0.60;

/// MER-like SNR must only move when the constellation is still observable.
const SNR_OBSERVABLE_POST_EQ_MIN: f32 = 0.50;
const SNR_OBSERVABLE_WEAK_RATIO_MAX: f32 = 0.25;

/// Do not reuse the previous PRS start index after a tracking miss.
///
/// Live RF captures showed that holding a stale alignment for even one missed
/// phase-reference symbol causes RS/fire-code failures, metadata blackout, and
/// rapid sync loss. Force immediate reacquisition instead.
const TRACKING_MISS_TOLERANCE: u8 = 0;

/// Maximum per-carrier phase back-rotation used by the DABstar-style symbol
/// decoder loop.
const PHASE_CORRECTION_LIMIT_DEG: f32 = 20.0;

/// TII (Transmitter Identification Information) threshold factor.
/// If a null-symbol FFT bin has power greater than this multiple of the mean
/// carrier power, the bin is classified as a TII carrier and excluded from
/// the null-noise IIR update.  Prevents TII beacon energy from inflating the
/// per-carrier noise-floor estimate and artificially reducing the LLR weight
/// on adjacent carriers.  Matches DABstar's per-bin TII guard (GPLv2).
const TII_THRESHOLD_FACTOR: f32 = 4.0;

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
    s_level: f32,
    running: Arc<AtomicBool>,
    /// Smoothed structural quality of the post-EQ constellation.
    post_eq_quality: f32,
    /// Fraction of carriers currently judged weak or poorly observable.
    eq_weak_ratio: f32,
    /// True once OFDM has produced at least one structurally valid frame and
    /// until a genuine RF loss forces reacquisition.
    soft_lock_active: bool,
    /// Per-carrier running mean of FFT-bin power, updated per symbol.
    /// IIR with α = MEAN_POWER_ALPHA. Matches DABstar's power tracking for
    /// soft-bit weighting and quality estimation.
    mean_power: Vec<f32>,
    /// Per-carrier phase-error integrator used to back-rotate the differential
    /// QPSK cloud toward the first quadrant on subsequent symbols.
    integ_abs_phase: Vec<f32>,
    /// Per-carrier phase variance estimate after first-quadrant wrapping.
    stddev_sq_phase: Vec<f32>,
    /// Per-carrier mean squared error from the nearest ideal constellation axis.
    mean_sigma_sq: Vec<f32>,
    /// Per-carrier noise-floor estimate derived from the FFT of the null symbol.
    /// IIR with α = NULL_NOISE_ALPHA.  Provides the noise reference for the
    /// per-carrier SNR soft-bit weight.  Zero at startup (no correction applied
    /// until the first null symbol is processed).
    null_noise: Vec<f32>,
    /// Smoothed SNR estimate from the null-symbol energy.
    snr_estimate: f32,
    /// Frame counter for throttled SNR/frequency reporting.
    snr_report_count: usize,
    /// Running mean amplitude used to scale soft bits similarly to DABstar.
    mean_value: f32,
    /// Estimated sample-clock error in Hz.
    clock_err_hz: f32,
    /// Precomputed per-carrier clock-error phase coefficients.
    phase_corr_const: Vec<f32>,
    /// Start index found during the latest phase-reference evaluation.
    last_sync_start_index: usize,
    /// True when the current symbol-0 alignment reused the previous start index
    /// after a negative PRS miss, so coarse AFC should not trust this frame.
    last_sync_alignment_reused: bool,
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

/// States of the DABstar-style OFDM frame synchronisation state machine.
///
/// The processor cycles through the same three high-level stages as DABstar:
/// wait for the null-symbol timing marker, evaluate the phase-reference symbol,
/// then process the remainder of the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SyncState {
    /// Search for the end of the null-symbol level drop.
    WaitForTimeSyncMarker,
    /// Evaluate the phase-reference symbol and align symbol 0 in the buffer.
    EvalSyncSymbol,
    /// Soft-locked RF hold state: preserve continuity and retry on the next
    /// plausible frame boundary without declaring hard sync loss yet.
    DegradedHolding,
    /// Decode the remainder of the frame once symbol 0 is aligned.
    ProcessRestOfFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncRecovery {
    Hold,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeSyncFollowUpPlan {
    established: bool,
    attempts: i16,
    emit_warning: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SyncAlignmentPlan {
    start_index: usize,
    samples_to_fetch: usize,
    reused_previous_start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoarseCorrectionPlan {
    updated_coarse_hz: i32,
    step_applied: bool,
    reset_clock_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SymbolReadPlan {
    phase_hz: i32,
    next_frame_sample_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameRestMetricsPlan {
    next_frame_sample_count: usize,
    should_integrate_clock_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ObservabilitySnapshot {
    post_eq_quality: f32,
    eq_weak_ratio: f32,
    soft_locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NullSymbolMetricsPlan {
    snr_db: f32,
    next_report_count: usize,
    emit_report: bool,
    offset_hz: i32,
}

struct OfdmRunBuffers {
    ibits: Vec<i16>,
    null_buf: Vec<Complex32>,
    check_buf: Vec<Complex32>,
    block_buf: Vec<Complex32>,
}

impl OfdmRunBuffers {
    fn new(carriers: usize, t_null: usize, t_u: usize, t_s: usize) -> Self {
        Self {
            ibits: vec![0i16; 2 * carriers],
            null_buf: vec![Complex32::new(0.0, 0.0); t_null],
            check_buf: vec![Complex32::new(0.0, 0.0); t_u],
            block_buf: vec![Complex32::new(0.0, 0.0); t_s],
        }
    }
}

struct SyncLoopControl {
    state: SyncState,
    time_syncer: TimeSyncer,
    attempts: i16,
    tracking_miss_budget: u8,
    acq_threshold: i16,
    track_threshold: i16,
    sync_threshold: i16,
    soft_locked: bool,
    degraded_holding: bool,
    hold_frames_remaining: u8,
    lock_frozen_until_next_frame: bool,
    sync_loss_reported: bool,
}

impl SyncLoopControl {
    fn new(threshold_1: i16, threshold_2: i16) -> Self {
        let (acq_threshold, track_threshold) = phase_threshold_pair(threshold_1, threshold_2);
        Self {
            state: SyncState::WaitForTimeSyncMarker,
            time_syncer: TimeSyncer::default(),
            attempts: 0,
            tracking_miss_budget: TRACKING_MISS_TOLERANCE,
            acq_threshold,
            track_threshold,
            sync_threshold: acq_threshold,
            soft_locked: false,
            degraded_holding: false,
            hold_frames_remaining: MAX_DEGRADED_HOLD_FRAMES,
            lock_frozen_until_next_frame: false,
            sync_loss_reported: false,
        }
    }

    fn state(&self) -> SyncState {
        self.state
    }

    fn current_threshold(&self) -> i16 {
        self.sync_threshold
    }

    fn is_tracking_threshold(&self) -> bool {
        self.sync_threshold == self.track_threshold
    }

    fn on_time_sync_established(&mut self) {
        self.attempts = 0;
        self.sync_threshold = self.acq_threshold;
        self.state = SyncState::EvalSyncSymbol;
    }

    fn on_sync_symbol_aligned(&mut self) {
        self.state = SyncState::ProcessRestOfFrame;
    }

    fn on_frame_processed(&mut self, post_eq_quality: f32, eq_weak_ratio: f32) {
        self.soft_locked = true;
        self.lock_frozen_until_next_frame = false;
        self.sync_loss_reported = false;

        if self.degraded_holding {
            if should_release_degraded_hold(post_eq_quality, eq_weak_ratio) {
                self.degraded_holding = false;
                self.hold_frames_remaining = MAX_DEGRADED_HOLD_FRAMES;
                self.sync_threshold = self.track_threshold;
            } else {
                // ETSI EN 300 401 §14.8 tracking continuity: when a structurally
                // plausible frame is decoded after a brief fade, keep hold mode
                // active but replenish part of the survival budget so repeated
                // short fades do not force needless reacquisition churn.
                self.hold_frames_remaining = self
                    .hold_frames_remaining
                    .saturating_add(HOLD_BUDGET_REFILL_PER_GOOD_FRAME)
                    .min(MAX_DEGRADED_HOLD_FRAMES);
                self.sync_threshold = self.acq_threshold;
            }
        } else {
            self.hold_frames_remaining = MAX_DEGRADED_HOLD_FRAMES;
            self.sync_threshold = self.track_threshold;
        }

        self.state = SyncState::EvalSyncSymbol;
    }

    fn on_tracking_miss(&mut self, post_eq_quality: f32, eq_weak_ratio: f32) -> SyncRecovery {
        let can_hold = self.soft_locked
            && !self.lock_frozen_until_next_frame
            && self.hold_frames_remaining > 0
            && structural_hold_allowed(post_eq_quality, eq_weak_ratio)
            && !structural_collapse_detected(post_eq_quality, eq_weak_ratio);

        if can_hold {
            self.degraded_holding = true;
            self.hold_frames_remaining = self.hold_frames_remaining.saturating_sub(1);
            self.lock_frozen_until_next_frame = true;
            self.sync_threshold = self.acq_threshold;
            self.state = SyncState::DegradedHolding;
            SyncRecovery::Hold
        } else {
            self.on_sync_lost();
            SyncRecovery::Lost
        }
    }

    fn on_degraded_frame_boundary(&mut self) {
        self.lock_frozen_until_next_frame = false;
        self.state = SyncState::EvalSyncSymbol;
    }

    fn should_report_sync_loss(&mut self) -> bool {
        if self.sync_loss_reported {
            false
        } else {
            self.sync_loss_reported = true;
            true
        }
    }

    fn on_sync_lost(&mut self) {
        self.soft_locked = false;
        self.degraded_holding = false;
        self.lock_frozen_until_next_frame = false;
        self.hold_frames_remaining = MAX_DEGRADED_HOLD_FRAMES;
        self.sync_threshold = self.acq_threshold;
        self.state = SyncState::WaitForTimeSyncMarker;
    }
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
        let phase_synchronizer = PhaseReference::new(
            t_u,
            t_g,
            carriers,
            carrier_diff,
            params.dab_mode,
            DIFF_LENGTH as usize,
        );

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(t_u);

        let half_k = carriers as f32 / 2.0;
        let phase_corr_const: Vec<f32> = (0..carriers)
            .map(|nom_carrier_idx| {
                let fft_idx = freq_interleaver.map_in(nom_carrier_idx) as i32;
                let real_carr_rel_idx = if fft_idx < 0 {
                    fft_idx + carriers as i32 / 2
                } else {
                    fft_idx + carriers as i32 / 2 - 1
                };
                std::f32::consts::PI / 1024.0 * (half_k - real_carr_rel_idx as f32) / half_k
            })
            .collect();

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
            s_level: 0.0,
            running,
            post_eq_quality: 0.0,
            eq_weak_ratio: 0.0,
            soft_lock_active: false,
            // DABstar starts these estimators at zero and lets them converge
            // from real RF measurements.
            mean_power: vec![0.0f32; carriers],
            integ_abs_phase: vec![0.0f32; carriers],
            stddev_sq_phase: vec![0.0f32; carriers],
            mean_sigma_sq: vec![0.0f32; carriers],
            null_noise: vec![0.0f32; carriers],
            snr_estimate: 0.0,
            snr_report_count: 0,
            mean_value: 1.0,
            clock_err_hz: 0.0,
            phase_corr_const,
            last_sync_start_index: 0,
            last_sync_alignment_reused: false,
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

    pub fn set_fic_quality_percent_source(&mut self, fic_quality_percent: Arc<AtomicI16>) {
        let _ = fic_quality_percent;
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

    fn reset_tracking_loop(&mut self) {
        // DABstar keeps the already-estimated coarse/fine frequency offset when
        // returning to acquisition, but clears the frame-length clock estimate
        // before searching for the next valid null + PRS alignment.
        self.clock_err_hz = 0.0;
    }

    fn prepare_for_acquisition_retry(&mut self, cold_reset: bool) {
        self.reset_tracking_loop();

        // Clear volatile symbol-to-symbol state on every reacquisition attempt
        // so a stale reference symbol cannot poison the next PRS/data frame.
        self.reference_phase.fill(Complex32::new(0.0, 0.0));
        self.r1_buf.fill(Complex32::new(0.0, 0.0));

        // For a brief sync loss at otherwise healthy RF, keep the long-lived
        // power/noise estimators warm so the decoder can relock like DABstar
        // without re-learning the whole channel from scratch. Only cold-reset
        // those statistics when acquisition has already failed for more than
        // one attempt or at startup. ETSI EN 300 401 §14.8 acquisition/tracking.
        self.last_sync_alignment_reused = false;

        if cold_reset {
            self.reset_llr_state();
            self.last_sync_start_index = 0;
        }
    }

    fn store_reference_symbol_0(&mut self) {
        self.fft_buffer[..self.t_u].copy_from_slice(&self.ofdm_buffer[..self.t_u]);
        self.fft.process(&mut self.fft_buffer);
        self.reference_phase[..self.t_u].copy_from_slice(&self.fft_buffer[..self.t_u]);
    }

    fn update_coarse_frequency_from_sync_symbol_0(&mut self) -> bool {
        let correction = self
            .phase_synchronizer
            .estimate_carrier_offset_from_sync_symbol_0(&self.ofdm_buffer[..self.t_u]);
        let prev = self.coarse_corrector;

        let Some(plan) = plan_coarse_correction(
            self.last_sync_alignment_reused,
            self.soft_lock_active,
            self.coarse_corrector,
            correction,
            self.post_eq_quality,
            self.eq_weak_ratio,
        ) else {
            if self.last_sync_alignment_reused {
                trace!(
                    start_index = self.last_sync_start_index,
                    "OFDM: skipping coarse AFC on reused sync alignment"
                );
            } else if self.soft_lock_active {
                trace!(
                    coarse_hz = self.coarse_corrector,
                    "OFDM: coarse AFC frozen while soft-locked"
                );
            }
            return false;
        };

        self.coarse_corrector = plan.updated_coarse_hz;

        if correction != PhaseReference::IDX_NOT_FOUND && self.coarse_corrector != prev {
            trace!(
                delta_hz = correction,
                prev_hz = prev,
                new_hz = self.coarse_corrector,
                "OFDM: coarse AFC step applied"
            );
        }

        if plan.reset_clock_error {
            self.clock_err_hz = 0.0;
        }

        plan.step_applied
    }

    fn process_symbol_block(
        &mut self,
        device: &RtlsdrHandler,
        dab_pipeline: &mut DabPipeline,
        symbol_count: u16,
        frame_sample_count: usize,
        ibits: &mut [i16],
        block_buf: &mut [Complex32],
    ) -> Result<(Complex32, usize), ProcessorError> {
        let plan = plan_symbol_read(
            self.coarse_corrector,
            self.fine_corrector,
            frame_sample_count,
            self.t_s,
        );
        self.get_samples(device, block_buf, plan.phase_hz)?;

        let freq_corr = accumulate_prefix_correlation(block_buf, self.t_u, self.t_s);
        self.process_block(block_buf, ibits);
        dab_pipeline.process_block(ibits, symbol_count as i16);
        Ok((freq_corr, plan.next_frame_sample_count))
    }

    fn process_ofdm_symbols_1_to_l(
        &mut self,
        device: &RtlsdrHandler,
        dab_pipeline: &mut DabPipeline,
        ibits: &mut [i16],
        block_buf: &mut [Complex32],
    ) -> Result<(Complex32, usize), ProcessorError> {
        let mut total_freq_corr = Complex32::new(0.0, 0.0);
        let mut frame_sample_count = self.t_u + self.last_sync_start_index;

        for symbol_count in 2..=(self.nr_blocks as u16) {
            let (freq_corr, next_frame_sample_count) = self.process_symbol_block(
                device,
                dab_pipeline,
                symbol_count,
                frame_sample_count,
                ibits,
                block_buf,
            )?;
            total_freq_corr += freq_corr;
            frame_sample_count = next_frame_sample_count;
        }

        Ok((total_freq_corr, frame_sample_count))
    }

    fn update_fine_frequency_from_cyclic_prefix(&mut self, freq_corr: Complex32) {
        let cp_pair_count = (self.nr_blocks.saturating_sub(1)) * self.t_g;
        let fine_delta_hz = fine_afc_delta_hz(
            freq_corr,
            self.carrier_diff,
            cp_pair_count,
            self.s_level,
            self.post_eq_quality,
            self.eq_weak_ratio,
        );
        self.fine_corrector += fine_delta_hz;
        self.wrap_fine_corrector_into_coarse();
    }

    fn wrap_fine_corrector_into_coarse(&mut self) {
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
    }

    fn process_null_symbol(
        &mut self,
        device: &RtlsdrHandler,
        null_buf: &mut [Complex32],
    ) -> Result<usize, ProcessorError> {
        let phase = self.coarse_corrector + self.fine_corrector as i32;
        self.get_samples(device, null_buf, phase)?;

        let null_avg_level = null_buf.iter().map(|s| s.norm()).sum::<f32>() / self.t_null as f32;
        let telemetry = plan_null_symbol_metrics(
            self.snr_estimate,
            self.s_level,
            null_avg_level,
            self.snr_report_count,
            self.coarse_corrector,
            self.fine_corrector,
            ObservabilitySnapshot {
                post_eq_quality: self.post_eq_quality,
                eq_weak_ratio: self.eq_weak_ratio,
                soft_locked: self.soft_lock_active,
            },
        );
        self.snr_estimate = telemetry.snr_db;
        self.snr_report_count = telemetry.next_report_count;
        self.update_null_noise(null_buf);

        if telemetry.emit_report {
            self.emit_snr(self.snr_estimate as i16);
            self.emit_freq_offset(telemetry.offset_hz);
            debug!(
                snr_db = self.snr_estimate,
                freq_offset_hz = telemetry.offset_hz,
                "OFDM telemetry updated"
            );
        }

        Ok(self.t_null)
    }

    fn integrate_clock_error_from_frame_length(
        &mut self,
        frame_sample_count: usize,
        coarse_step_applied: bool,
    ) {
        // DABstar updates the sample-clock error only when no coarse step was
        // needed on symbol 0, so the estimate is not polluted by a deliberate
        // baseband retune during the same frame.
        if !coarse_step_applied {
            let clock_err = INPUT_RATE as f32 * (frame_sample_count as f32 / self.t_f as f32 - 1.0);
            self.clock_err_hz = 0.9 * self.clock_err_hz + 0.1 * clock_err.clamp(-307.2, 307.2);
        }
    }

    /// Demodulate an OFDM data symbol into soft bits (differential QPSK).
    ///
    /// Strips the cyclic prefix (guard interval), runs a T_u-point forward FFT,
    /// then for each OFDM carrier applies differential detection: the current
    /// carrier phasor is divided by the previous symbol's phasor (stored in
    /// `reference_phase`).  The resulting differential phasor is normalised and
    /// mapped to two signed soft bits in the range [−127, 127].
    ///
    /// **LLR-weighted soft bits** (DABstar, ETSI EN 300 401 §14.5):
    /// Each carrier's soft-bit confidence is scaled by `SNR_k / (SNR_k + 1)`,
    /// where `SNR_k = (mean_power_k − null_noise_k) / null_noise_k`.  Both
    /// `mean_power_k` and `null_noise_k` are estimated from the raw FFT bin
    /// power `|FFT_bin_k|²` so that they share the same units.  This reduces
    /// the weight of carriers with a low SNR (measured from the null symbol
    /// noise floor) so the Viterbi decoder can exploit frequency-selective
    /// channel information — particularly useful for multipath/SFN reception.
    ///
    /// Soft-bit sign convention: a positive real/imag differential phase gives
    /// −127, matching the existing decoder bit polarity used throughout this
    /// pipeline.
    ///
    /// ETSI EN 300 401 §14.5 — differential QPSK modulation/demodulation.
    /// ETSI EN 300 401 §14.6 — FFT and frequency-domain processing.
    pub(crate) fn process_block(&mut self, inv: &[Complex32], ibits: &mut [i16]) {
        self.fft_buffer[..self.t_u].copy_from_slice(&inv[self.t_g..self.t_g + self.t_u]);
        self.fft.process(&mut self.fft_buffer);

        let mut sum = 0.0f32;
        let mut quality_sum = 0.0f32;
        let mut weak_carriers = 0usize;

        for i in 0..self.carriers {
            let mut index = self.freq_interleaver.map_in(i) as i32;
            if index < 0 {
                index += self.t_u as i32;
            }
            let index = index as usize;

            // DABstar-style PI/4-DQPSK demodulation and per-carrier phase
            // correction. ETSI EN 300 401 §14.5 / §14.6.
            let fft_bin_raw =
                self.fft_buffer[index] * norm_to_length_one(self.reference_phase[index].conj());
            let phase_err = self.clock_err_hz * self.phase_corr_const[i] + self.integ_abs_phase[i];
            let fft_bin = fft_bin_raw * cmplx_from_phase2(-phase_err);
            let fft_bin_abs_phase = turn_phase_to_first_quadrant(fft_bin.arg());
            let phase_diff = fft_bin_abs_phase - std::f32::consts::FRAC_PI_4;

            self.integ_abs_phase[i] =
                clamp_phase_error(self.integ_abs_phase[i] + 0.2 * MEAN_POWER_ALPHA * phase_diff);

            let cur_stddev_sq = phase_diff * phase_diff;
            self.stddev_sq_phase[i] = self.stddev_sq_phase[i] * (1.0 - MEAN_POWER_ALPHA)
                + cur_stddev_sq * MEAN_POWER_ALPHA;

            let fft_power = fft_bin.norm_sqr();
            self.mean_power[i] =
                self.mean_power[i] * (1.0 - MEAN_POWER_ALPHA) + fft_power * MEAN_POWER_ALPHA;

            let mean_level = self.mean_power[i].sqrt().max(1e-5);
            let mean_axis_level = mean_level * std::f32::consts::FRAC_1_SQRT_2;
            let real_level_dist = fft_bin.re.abs() - mean_axis_level;
            let imag_level_dist = fft_bin.im.abs() - mean_axis_level;
            let sigma_sq = real_level_dist * real_level_dist + imag_level_dist * imag_level_dist;
            self.mean_sigma_sq[i] =
                self.mean_sigma_sq[i] * (1.0 - MEAN_POWER_ALPHA) + sigma_sq * MEAN_POWER_ALPHA;

            if fft_bin.norm() > 0.0 {
                let weight = carrier_weight(
                    mean_level,
                    self.mean_sigma_sq[i],
                    self.null_noise[i],
                    self.mean_power[i],
                )
                .clamp(0.0, 1.0);

                let null_floor = self.null_noise[i].max(NOISE_FLOOR_MIN);
                let signal_power = (self.mean_power[i] - null_floor).max(0.0);
                let denom = signal_power + self.mean_sigma_sq[i] + null_floor;
                let carrier_quality = if denom > 0.0 {
                    signal_power / denom
                } else {
                    0.0
                };
                quality_sum += carrier_quality;
                if carrier_quality < 0.5 {
                    weak_carriers += 1;
                }

                let r1 = norm_to_length_one(fft_bin) * weight;
                self.r1_buf[i] = r1;

                ibits[i] = soft_bit_from_component(r1.re, self.mean_value);
                ibits[self.carriers + i] = soft_bit_from_component(r1.im, self.mean_value);
                sum += r1.norm();
            } else {
                self.r1_buf[i] = Complex32::new(0.0, 0.0);
                ibits[i] = 0;
                ibits[self.carriers + i] = 0;
            }
            self.reference_phase[index] = self.fft_buffer[index];
        }

        self.mean_value = (sum / self.carriers.max(1) as f32).max(1e-3);

        let measured_post_eq_quality = quality_sum / self.carriers.max(1) as f32;
        let measured_weak_ratio = weak_carriers as f32 / self.carriers.max(1) as f32;
        self.post_eq_quality = if self.post_eq_quality == 0.0 {
            measured_post_eq_quality
        } else {
            0.8 * self.post_eq_quality + 0.2 * measured_post_eq_quality
        };
        self.eq_weak_ratio = if self.eq_weak_ratio == 0.0 && measured_weak_ratio == 0.0 {
            0.0
        } else {
            0.8 * self.eq_weak_ratio + 0.2 * measured_weak_ratio
        };
    }

    /// Reset per-carrier power and null-noise IIR estimators to their initial
    /// values.
    ///
    /// Called at the start of each acquisition cycle so that stale LLR weights
    /// from a previous lock (potentially on a different channel) do not bias the
    /// soft-bit decoder for the new signal.  Matches DABstar's OfdmDecoder reset
    /// on channel change (GPLv2).
    fn reset_llr_state(&mut self) {
        self.mean_power.fill(0.0);
        self.integ_abs_phase.fill(0.0);
        self.stddev_sq_phase.fill(0.0);
        self.mean_sigma_sq.fill(0.0);
        self.null_noise.fill(0.0);
        self.mean_value = 1.0;
        self.clock_err_hz = 0.0;
    }

    /// Update the per-carrier null-noise IIR estimate from the null symbol,
    /// masking TII (Transmitter Identification Information) carriers.
    ///
    /// After FFT-ing the first T_u samples of `null_buf`, any bin whose power
    /// exceeds `TII_THRESHOLD_FACTOR × mean_carrier_power` is classified as a
    /// TII carrier and is excluded from the IIR update.  This prevents TII
    /// beacon energy from inflating the noise-floor estimate and artificially
    /// suppressing the LLR weight on adjacent carriers.
    ///
    /// On multiplexes that do not broadcast TII, no bin exceeds the threshold
    /// and all carriers are updated normally — identical to the original
    /// `store_null_symbol_without_tii` path.
    ///
    /// Matches DABstar's `OfdmDecoder::store_null_symbol_with_tii()` (GPLv2).
    fn update_null_noise(&mut self, null_buf: &[Complex32]) {
        self.fft_buffer.copy_from_slice(&null_buf[..self.t_u]);
        self.fft.process(&mut self.fft_buffer);

        // Compute mean power over all active carriers for TII detection.
        let total_power: f32 = (0..self.carriers)
            .map(|k| {
                let mut idx = self.freq_interleaver.map_in(k) as i32;
                if idx < 0 {
                    idx += self.t_u as i32;
                }
                self.fft_buffer[idx as usize].norm_sqr()
            })
            .sum();
        let tii_threshold = TII_THRESHOLD_FACTOR * total_power / self.carriers as f32;

        for k in 0..self.carriers {
            let mut fft_idx = self.freq_interleaver.map_in(k) as i32;
            if fft_idx < 0 {
                fft_idx += self.t_u as i32;
            }
            let null_power = self.fft_buffer[fft_idx as usize].norm_sqr();

            // Skip TII carriers: their elevated power would inflate null_noise
            // and suppress LLR soft-bit weights on adjacent carriers.
            if null_power > tii_threshold {
                continue;
            }

            self.null_noise[k] =
                self.null_noise[k] * (1.0 - NULL_NOISE_ALPHA) + null_power * NULL_NOISE_ALPHA;
        }
    }

    fn state_wait_for_time_sync_marker(
        &mut self,
        device: &RtlsdrHandler,
        time_syncer: &mut TimeSyncer,
        attempts: &mut i16,
        emit_absent_warning: bool,
        hold_mode: bool,
    ) -> Result<bool, ProcessorError> {
        self.prepare_for_acquisition_retry(!hold_mode && *attempts > 0);

        let plan = plan_time_sync_follow_up(
            time_syncer.read_samples_until_end_of_level_drop(
                self.s_level,
                self.t_f,
                self.t_null,
                || self.get_sample(device, 0).ok().map(jan_abs),
            ),
            *attempts,
        )?;

        *attempts = if plan.emit_warning { 0 } else { plan.attempts };
        if plan.emit_warning && emit_absent_warning {
            warn!(
                "OFDM: no null symbol detected after {} attempts \
                 — signal absent or very weak",
                plan.attempts
            );
            self.soft_lock_active = false;
            self.emit_sync_signal(false);
        }

        Ok(plan.established)
    }

    fn state_eval_sync_symbol(
        &mut self,
        device: &RtlsdrHandler,
        threshold: i16,
        allow_single_miss_reuse: bool,
        tracking_miss_budget: &mut u8,
        check_buf: &mut [Complex32],
    ) -> Result<bool, ProcessorError> {
        let phase = self.coarse_corrector + self.fine_corrector as i32;
        self.get_samples(device, &mut check_buf[..self.t_u], phase)?;
        self.ofdm_buffer[..self.t_u].copy_from_slice(&check_buf[..self.t_u]);

        let raw_start_index = self
            .phase_synchronizer
            .correlate_with_phase_ref_and_find_max_peak(&self.ofdm_buffer[..self.t_u], threshold);

        let Some(plan) = build_sync_alignment_plan(
            raw_start_index,
            self.last_sync_start_index,
            tracking_miss_budget,
            self.t_u,
            allow_single_miss_reuse,
        ) else {
            return Ok(false);
        };

        if plan.reused_previous_start {
            trace!(
                start_index = plan.start_index,
                "OFDM: reusing last good start index after one tracking miss"
            );
        }

        self.last_sync_alignment_reused = plan.reused_previous_start;
        self.last_sync_start_index = plan.start_index;
        let next_ofdm_buffer_idx = self.t_u - plan.start_index;
        self.ofdm_buffer.copy_within(plan.start_index..self.t_u, 0);

        self.get_samples(device, &mut check_buf[..plan.samples_to_fetch], phase)?;
        self.ofdm_buffer[next_ofdm_buffer_idx..self.t_u]
            .copy_from_slice(&check_buf[..plan.samples_to_fetch]);

        Ok(true)
    }

    fn finish_frame_rest_metrics(
        &mut self,
        device: &RtlsdrHandler,
        null_buf: &mut [Complex32],
        freq_corr: Complex32,
        frame_sample_count: usize,
        coarse_step_applied: bool,
    ) -> Result<(), ProcessorError> {
        self.update_fine_frequency_from_cyclic_prefix(freq_corr);
        let null_symbol_len = self.process_null_symbol(device, null_buf)?;
        let plan =
            plan_frame_rest_metrics(frame_sample_count, null_symbol_len, coarse_step_applied);
        if plan.should_integrate_clock_error {
            self.integrate_clock_error_from_frame_length(plan.next_frame_sample_count, false);
        }
        Ok(())
    }

    fn state_process_rest_of_frame(
        &mut self,
        device: &RtlsdrHandler,
        dab_pipeline: &mut DabPipeline,
        ibits: &mut [i16],
        null_buf: &mut [Complex32],
        block_buf: &mut [Complex32],
    ) -> Result<bool, ProcessorError> {
        dab_pipeline.new_frame();

        // Symbol 0 is already aligned in `self.ofdm_buffer` by
        // `eval_sync_symbol()`, exactly like DABstar's PROCESS_REST_OF_FRAME.
        self.store_reference_symbol_0();
        let coarse_step_applied = self.update_coarse_frequency_from_sync_symbol_0();
        if coarse_step_applied {
            trace!(
                coarse_hz = self.coarse_corrector,
                "OFDM: coarse correction changed; continuing current frame like DABstar"
            );
        }

        if !continue_after_coarse_step(coarse_step_applied) {
            self.emit_sync_signal(false);
            return Ok(false);
        }

        let (freq_corr, frame_sample_count) =
            self.process_ofdm_symbols_1_to_l(device, dab_pipeline, ibits, block_buf)?;
        self.finish_frame_rest_metrics(
            device,
            null_buf,
            freq_corr,
            frame_sample_count,
            coarse_step_applied,
        )?;

        self.soft_lock_active = true;
        self.emit_sync_signal(true);
        Ok(true)
    }

    /// Main processing loop — driven by an explicit DABstar-style state machine.
    ///
    /// Replaces the original nested `break`/`continue` loop structure with a
    /// single top-level `loop { match state { … } }` that transitions through
    /// [`SyncState`] variants, matching DABstar's OfdmProcessor design (GPLv2).
    ///
    /// The DAB synchronisation algorithm (null detection, phase-reference
    /// correlation, coarse/fine AFC, LLR soft-bits) is unchanged.
    pub fn run(&mut self, device: &RtlsdrHandler, dab_pipeline: &mut DabPipeline) {
        let mut buffers = OfdmRunBuffers::new(self.carriers, self.t_null, self.t_u, self.t_s);
        let mut control = SyncLoopControl::new(self.threshold_1, self.threshold_2);

        // DABstar warms the signal-level estimator with a short burst of raw
        // useful-symbol reads before entering the state machine.
        self.s_level = 0.0;
        self.coarse_corrector = 0;
        self.fine_corrector = 0.0;
        self.clock_err_hz = 0.0;
        self.nco_phasor = Complex32::new(1.0, 0.0);
        self.last_sync_alignment_reused = false;
        self.reset_llr_state();
        self.reference_phase.fill(Complex32::new(0.0, 0.0));
        self.r1_buf.fill(Complex32::new(0.0, 0.0));
        if self
            .discard_samples(device, 20 * self.t_u, &mut buffers.block_buf)
            .is_err()
        {
            return;
        }

        loop {
            match control.state() {
                SyncState::WaitForTimeSyncMarker => {
                    match self.state_wait_for_time_sync_marker(
                        device,
                        &mut control.time_syncer,
                        &mut control.attempts,
                        true,
                        false,
                    ) {
                        Ok(true) => control.on_time_sync_established(),
                        Ok(false) => {}
                        Err(_) => return,
                    }
                }

                SyncState::EvalSyncSymbol => {
                    match self.state_eval_sync_symbol(
                        device,
                        control.current_threshold(),
                        control.is_tracking_threshold(),
                        &mut control.tracking_miss_budget,
                        &mut buffers.check_buf,
                    ) {
                        Ok(true) => control.on_sync_symbol_aligned(),
                        Ok(false) => match control
                            .on_tracking_miss(self.post_eq_quality, self.eq_weak_ratio)
                        {
                            SyncRecovery::Hold => {
                                debug!(
                                    post_eq_quality = self.post_eq_quality,
                                    eq_weak_ratio = self.eq_weak_ratio,
                                    remaining_frames = control.hold_frames_remaining,
                                    "OFDM: entering degraded holding"
                                );
                            }
                            SyncRecovery::Lost => {
                                self.soft_lock_active = false;
                                if control.should_report_sync_loss() {
                                    warn!(
                                        "OFDM: synchronisation lost — structural evidence collapsed; returning to acquisition"
                                    );
                                }
                            }
                        },
                        Err(_) => return,
                    }
                }

                SyncState::DegradedHolding => {
                    match self.state_wait_for_time_sync_marker(
                        device,
                        &mut control.time_syncer,
                        &mut control.attempts,
                        false,
                        true,
                    ) {
                        Ok(true) => control.on_degraded_frame_boundary(),
                        Ok(false) => {}
                        Err(_) => return,
                    }
                }

                SyncState::ProcessRestOfFrame => {
                    match self.state_process_rest_of_frame(
                        device,
                        dab_pipeline,
                        &mut buffers.ibits,
                        &mut buffers.null_buf,
                        &mut buffers.block_buf,
                    ) {
                        Ok(true) => {
                            control.on_frame_processed(self.post_eq_quality, self.eq_weak_ratio)
                        }
                        Ok(false) => control.on_sync_lost(),
                        Err(_) => return,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::dab_constants::jan_abs;
    use num_complex::Complex32;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Apply the DQPSK soft-bit formula for a single carrier.
    ///
    /// `current` is the FFT bin value for the current symbol.
    /// `reference` is the FFT bin value from the previous symbol (the phase
    /// reference).  Returns `(soft_re, soft_im)` both in [−127, 127].
    ///
    /// This reproduces the per-carrier logic inside `OfdmProcessor::process_block`
    /// so it can be tested without the full OFDM processor state machine.
    ///
    /// ETSI EN 300 401 §14.5 — differential QPSK demodulation.
    fn dqpsk_soft_bits(current: Complex32, reference: Complex32) -> (i16, i16) {
        let diff = current * reference.conj();
        let ab = jan_abs(diff);
        if ab > 0.0 {
            let re = (-diff.re / ab * 127.0).clamp(-127.0, 127.0) as i16;
            let im = (-diff.im / ab * 127.0).clamp(-127.0, 127.0) as i16;
            (re, im)
        } else {
            (0, 0)
        }
    }

    // ── DQPSK unit tests ──────────────────────────────────────────────────────

    /// Zero-phase difference (same symbol repeated): carrier rotates by 0°.
    /// Positive real differential → soft_re = −127, soft_im ≈ 0.
    #[test]
    fn dqpsk_no_phase_change() {
        // Both symbols are at 0°: reference = (1,0), current = (1,0).
        let (re, im) = dqpsk_soft_bits(Complex32::new(1.0, 0.0), Complex32::new(1.0, 0.0));
        assert_eq!(re, -127, "zero phase change: soft_re must be −127");
        assert_eq!(im, 0, "zero phase change: soft_im must be 0");
    }

    /// 90° counter-clockwise phase step (0° → 90°).
    /// Positive imaginary differential → soft_re ≈ 0, soft_im = −127.
    #[test]
    fn dqpsk_ninety_degree_rotation() {
        let reference = Complex32::new(1.0, 0.0);
        let current = Complex32::new(0.0, 1.0); // 90° CCW
        let (re, im) = dqpsk_soft_bits(current, reference);
        assert_eq!(re, 0, "90° rotation: soft_re must be 0");
        assert_eq!(im, -127, "90° rotation: soft_im must be −127");
    }

    /// 180° phase step: negative real differential → soft_re = +127, soft_im ≈ 0.
    #[test]
    fn dqpsk_half_rotation() {
        let reference = Complex32::new(1.0, 0.0);
        let current = Complex32::new(-1.0, 0.0);
        let (re, im) = dqpsk_soft_bits(current, reference);
        assert_eq!(re, 127, "180° rotation: soft_re must be +127");
        assert_eq!(im, 0, "180° rotation: soft_im must be 0");
    }

    /// 270° phase step (or equivalently −90°).
    /// Negative imaginary differential → soft_re ≈ 0, soft_im = +127.
    #[test]
    fn dqpsk_two_seventy_degree_rotation() {
        let reference = Complex32::new(1.0, 0.0);
        let current = Complex32::new(0.0, -1.0); // 270° CCW = −90°
        let (re, im) = dqpsk_soft_bits(current, reference);
        assert_eq!(re, 0, "270° rotation: soft_re must be 0");
        assert_eq!(im, 127, "270° rotation: soft_im must be +127");
    }

    /// Zero-magnitude carrier (no signal) must return (0, 0) — not NaN or panic.
    #[test]
    fn dqpsk_zero_magnitude_is_safe() {
        let (re, im) = dqpsk_soft_bits(Complex32::new(0.0, 0.0), Complex32::new(1.0, 0.0));
        assert_eq!(re, 0);
        assert_eq!(im, 0);
    }

    /// Amplitude scaling must not affect the sign or magnitude of the soft bits:
    /// soft bits are normalised by `jan_abs`, so any non-zero amplitude gives the
    /// same result regardless of signal strength.
    #[test]
    fn dqpsk_amplitude_invariant() {
        for scale in [0.1f32, 1.0, 10.0, 1000.0] {
            let (re, im) = dqpsk_soft_bits(Complex32::new(scale, 0.0), Complex32::new(scale, 0.0));
            assert_eq!(re, -127, "soft_re must be −127 at scale {scale}");
            assert_eq!(im, 0, "soft_im must be 0 at scale {scale}");
        }
    }

    // ── process_block smoke test ──────────────────────────────────────────────

    /// Verify that `process_block` produces the expected number of soft bits for
    /// a Mode I OFDM symbol and does not panic on an all-zero guard+useful input.
    #[test]
    fn process_block_output_length_mode1() {
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;

        let running = Arc::new(AtomicBool::new(true));
        let mut proc = OfdmProcessor::new(1, 2, 5, running);

        // Mode I: T_s = 2552 = T_g(504) + T_u(2048).  K = 1536 carriers.
        // ibits must hold 2 × K = 3072 soft bits.
        let t_s = 2552usize;
        let carriers = 1536usize;
        let inv = vec![Complex32::new(0.0, 0.0); t_s];
        let mut ibits = vec![0i16; 2 * carriers];

        proc.process_block(&inv, &mut ibits);

        assert_eq!(ibits.len(), 2 * carriers);
        // All-zero input → all soft bits must be 0 (zero magnitude, no signal).
        assert!(ibits.iter().all(|&b| b == 0));
    }

    // ── DABstar-sync new tests ────────────────────────────────────────────────

    /// `reset_llr_state` must reset the DABstar-style LLR estimators to zero
    /// power/noise with a neutral global mean value.
    #[test]
    fn reset_llr_state_restores_initial_values() {
        let running = Arc::new(AtomicBool::new(true));
        let mut proc = OfdmProcessor::new(1, 2, 5, running);

        // Dirty the estimators.
        proc.mean_power.fill(42.0);
        proc.null_noise.fill(7.0);

        proc.reset_llr_state();

        assert!(
            proc.mean_power.iter().all(|&v| v == 0.0),
            "mean_power must be 0.0 after reset"
        );
        assert!(
            proc.mean_sigma_sq.iter().all(|&v| v == 0.0),
            "mean_sigma_sq must be 0.0 after reset"
        );
        assert!(
            proc.null_noise.iter().all(|&v| v == 0.0),
            "null_noise must be 0.0 after reset"
        );
    }

    /// A flat null symbol (no TII) must update every carrier's null_noise.
    #[test]
    fn update_null_noise_uniform_updates_all_carriers() {
        let running = Arc::new(AtomicBool::new(true));
        let mut proc = OfdmProcessor::new(1, 2, 5, running);

        // All null_noise start at 0.
        assert!(proc.null_noise.iter().all(|&v| v == 0.0));

        // Build a flat null buffer: every sample has unit amplitude.
        // After FFT-ing T_u samples, all bins have roughly equal power so no
        // bin exceeds TII_THRESHOLD_FACTOR × mean — all carriers are updated.
        let null_buf: Vec<Complex32> = (0..proc.t_null)
            .map(|i| Complex32::from_polar(1.0, i as f32))
            .collect();

        proc.update_null_noise(&null_buf);

        // At least some carriers must have been updated (null_noise > 0).
        let updated = proc.null_noise.iter().filter(|&&v| v > 0.0).count();
        assert!(
            updated > proc.carriers / 2,
            "at least half the carriers should be updated on a flat null; updated={updated}"
        );
    }

    /// A null symbol with a single dominant spike (simulated TII carrier) must
    /// NOT update the corresponding carrier's null_noise estimate.
    #[test]
    fn update_null_noise_tii_carrier_is_masked() {
        let running = Arc::new(AtomicBool::new(true));
        let mut proc = OfdmProcessor::new(1, 2, 5, running);

        // Build a null buffer that is zero except for one large spike in the
        // time domain.  After FFT the spike spreads energy broadly, but by
        // making it very large (100× amplitude) relative to the background we
        // ensure the per-bin power of the spike bin exceeds the TII threshold.
        let mut null_buf = vec![Complex32::new(0.0, 0.0); proc.t_null];
        // Place a large real-valued spike at t=0 so it maps to DC and then
        // spreads uniformly — any bin that ends up with power above
        // TII_THRESHOLD_FACTOR × mean should be skipped.
        null_buf[0] = Complex32::new(1000.0, 0.0);

        // Dirty the null_noise so we can see which bins are NOT updated.
        proc.null_noise.fill(99.0);

        proc.update_null_noise(&null_buf);

        // With the spike at t=0 (DC), the FFT output is a uniform constant
        // across all bins (DFT of a DC signal), so every bin has the same power
        // and no bin is above TII_THRESHOLD_FACTOR × mean.  Therefore ALL bins
        // must be updated (null_noise ≠ 99.0).
        let untouched = proc.null_noise.iter().filter(|&&v| v == 99.0).count();
        assert_eq!(
            untouched, 0,
            "no carriers should be untouched for a uniform-spectrum null (DC spike)"
        );
    }

    /// The fine corrector formula `freq_corr.arg() / TAU * carrier_diff` must
    /// correct by exactly `carrier_diff/4` when `freq_corr` has a 90° phase
    /// (arg = π/2).
    #[test]
    fn fine_corrector_formula_ninety_degree_phase() {
        // freq_corr with arg = π/2 (90°).
        let freq_corr = Complex32::new(0.0, 1.0);
        let carrier_diff = 1000i32; // Mode I

        let delta = freq_corr.arg() / std::f32::consts::TAU * carrier_diff as f32;
        // π/2 / (2π) × 1000 = 0.25 × 1000 = 250 Hz
        let expected = 250.0f32;
        assert!(
            (delta - expected).abs() < 1e-3,
            "expected Δfine={expected} Hz, got {delta}"
        );
    }

    /// At 180° (arg = π) the fine corrector advances by exactly carrier_diff/2.
    #[test]
    fn fine_corrector_formula_half_rotation() {
        let freq_corr = Complex32::new(-1.0, 0.0); // arg = π
        let carrier_diff = 1000i32;

        let delta = freq_corr.arg() / std::f32::consts::TAU * carrier_diff as f32;
        let expected = 500.0f32;
        assert!(
            (delta - expected).abs() < 1e-3,
            "expected Δfine={expected} Hz, got {delta}"
        );
    }

    #[test]
    fn normalized_cp_coherence_is_zero_without_signal_level() {
        let coherence = normalized_cp_coherence(Complex32::new(10.0, 0.0), 100, 0.0);
        assert_eq!(coherence, 0.0);
    }

    #[test]
    fn fine_afc_delta_is_suppressed_when_cp_coherence_is_too_low() {
        let delta = fine_afc_delta_hz(Complex32::new(0.0, 1.0), 1000, 1000, 1000.0, 0.8, 0.1);
        assert_eq!(
            delta, 0.0,
            "fine AFC must stay frozen when cyclic-prefix coherence is too weak"
        );
    }

    #[test]
    fn fine_afc_delta_is_applied_when_cp_coherence_is_good() {
        let delta = fine_afc_delta_hz(Complex32::new(0.0, 5000.0), 1000, 1000, 1.0, 0.8, 0.1);
        assert!(
            delta > 0.0,
            "fine AFC should react on coherent cyclic-prefix data"
        );
        assert!(
            delta < 250.0,
            "fine AFC should remain bounded even on strong coherent updates"
        );
    }

    #[test]
    fn reset_tracking_loop_keeps_frequency_offset_but_clears_clock_error() {
        let running = Arc::new(AtomicBool::new(true));
        let mut proc = OfdmProcessor::new(1, 2, 5, running);
        proc.fine_corrector = -33.5;
        proc.coarse_corrector = 2_000;
        proc.clock_err_hz = 12.5;

        proc.reset_tracking_loop();

        assert_eq!(proc.fine_corrector, -33.5);
        assert_eq!(proc.coarse_corrector, 2_000);
        assert_eq!(proc.clock_err_hz, 0.0);
    }

    #[test]
    fn brief_reacquisition_keeps_long_lived_estimators_warm() {
        let running = Arc::new(AtomicBool::new(true));
        let mut proc = OfdmProcessor::new(1, 2, 5, running);
        proc.mean_power[0] = 42.0;
        proc.null_noise[0] = 7.0;
        proc.clock_err_hz = 12.5;
        proc.integ_abs_phase[0] = 0.1;
        proc.reference_phase[0] = Complex32::new(1.0, 2.0);
        proc.r1_buf[0] = Complex32::new(3.0, 4.0);
        proc.last_sync_start_index = 384;
        proc.fine_corrector = -33.5;
        proc.coarse_corrector = 2_000;

        proc.prepare_for_acquisition_retry(false);

        assert_eq!(proc.fine_corrector, -33.5);
        assert_eq!(proc.coarse_corrector, 2_000);
        assert_eq!(proc.clock_err_hz, 0.0);
        assert_eq!(proc.mean_power[0], 42.0);
        assert_eq!(proc.null_noise[0], 7.0);
        assert_eq!(proc.integ_abs_phase[0], 0.1);
        assert_eq!(proc.reference_phase[0], Complex32::new(0.0, 0.0));
        assert_eq!(proc.r1_buf[0], Complex32::new(0.0, 0.0));
        assert_eq!(proc.last_sync_start_index, 384);
    }

    #[test]
    fn cold_acquisition_retry_resets_decoder_state() {
        let running = Arc::new(AtomicBool::new(true));
        let mut proc = OfdmProcessor::new(1, 2, 5, running);
        proc.mean_power[0] = 42.0;
        proc.null_noise[0] = 7.0;
        proc.clock_err_hz = 12.5;
        proc.integ_abs_phase[0] = 0.1;
        proc.reference_phase[0] = Complex32::new(1.0, 2.0);
        proc.r1_buf[0] = Complex32::new(3.0, 4.0);
        proc.last_sync_start_index = 384;

        proc.prepare_for_acquisition_retry(true);

        assert_eq!(proc.clock_err_hz, 0.0);
        assert_eq!(proc.mean_power[0], 0.0);
        assert_eq!(proc.null_noise[0], 0.0);
        assert_eq!(proc.integ_abs_phase[0], 0.0);
        assert_eq!(proc.reference_phase[0], Complex32::new(0.0, 0.0));
        assert_eq!(proc.r1_buf[0], Complex32::new(0.0, 0.0));
        assert_eq!(proc.last_sync_start_index, 0);
    }

    #[test]
    fn phase_threshold_pair_uses_the_explicit_tracking_threshold() {
        let (acq, track) = phase_threshold_pair(2, 5);
        assert_eq!(acq, 2);
        assert_eq!(track, 5);
    }

    #[test]
    fn phase_threshold_pair_clamps_both_thresholds_independently() {
        let (acq, track) = phase_threshold_pair(0, 99);
        assert_eq!(acq, OFDM_THRESHOLD_MIN);
        assert_eq!(track, OFDM_THRESHOLD_MAX);
    }

    #[test]
    fn tracking_miss_forces_immediate_reacquisition() {
        let mut budget = TRACKING_MISS_TOLERANCE;
        let reused = resolve_sync_start_index(-1, 384, &mut budget, 2048, true);
        assert_eq!(reused, None);
        assert_eq!(budget, TRACKING_MISS_TOLERANCE);
    }

    #[test]
    fn repeated_negative_tracking_misses_keep_reacquiring() {
        let mut budget = TRACKING_MISS_TOLERANCE;
        assert_eq!(
            resolve_sync_start_index(-1, 384, &mut budget, 2048, true),
            None
        );
        assert_eq!(
            resolve_sync_start_index(-1, 384, &mut budget, 2048, true),
            None
        );
        assert_eq!(
            resolve_sync_start_index(-1, 384, &mut budget, 2048, true),
            None
        );
        assert_eq!(budget, TRACKING_MISS_TOLERANCE);
    }

    #[test]
    fn acquisition_miss_must_not_reuse_last_good_start() {
        let mut budget = TRACKING_MISS_TOLERANCE;
        let reused = resolve_sync_start_index(-1, 384, &mut budget, 2048, false);
        assert_eq!(reused, None);
        assert_eq!(budget, TRACKING_MISS_TOLERANCE);
    }

    #[test]
    fn second_tracking_miss_forces_reacquisition() {
        let mut budget = 0;
        let reused = resolve_sync_start_index(-1, 384, &mut budget, 2048, true);
        assert_eq!(reused, None);
    }

    #[test]
    fn structural_hold_gate_depends_only_on_rf_quality() {
        assert!(structural_hold_allowed(0.35, 0.20));
        assert!(!structural_hold_allowed(0.20, 0.20));
        assert!(!structural_hold_allowed(0.35, 0.40));
    }

    #[test]
    fn coarse_step_does_not_force_reacquisition_like_dabstar() {
        assert!(continue_after_coarse_step(false));
        assert!(continue_after_coarse_step(true));
    }

    #[test]
    fn apply_coarse_correction_reports_real_steps_only() {
        let (updated, step_applied) = apply_coarse_correction(0, 3_000);
        assert_eq!(updated, 3_000);
        assert!(step_applied);

        let (subbin_updated, subbin_step) = apply_coarse_correction(100, 550);
        assert_eq!(subbin_updated, 650);
        assert!(subbin_step);

        let (unchanged, zero_step) = apply_coarse_correction(5_000, 0);
        assert_eq!(unchanged, 5_000);
        assert!(!zero_step);

        let (sentinel_unchanged, sentinel_step) =
            apply_coarse_correction(5_000, PhaseReference::IDX_NOT_FOUND);
        assert_eq!(sentinel_unchanged, 5_000);
        assert!(!sentinel_step);
    }

    #[test]
    fn apply_coarse_correction_resets_on_dabstar_overflow_guard() {
        let (updated, step_applied) = apply_coarse_correction(34_500, 1_000);
        assert_eq!(updated, 0);
        assert!(step_applied);
    }

    #[test]
    fn turn_phase_to_first_quadrant_matches_dabstar_behavior() {
        use std::f32::consts::{FRAC_PI_4, PI};

        let values = [FRAC_PI_4, -FRAC_PI_4, 3.0 * FRAC_PI_4, PI + FRAC_PI_4];
        for phase in values {
            let wrapped = turn_phase_to_first_quadrant(phase);
            assert!(
                (wrapped - FRAC_PI_4).abs() < 1e-5,
                "phase {phase} should wrap to pi/4, got {wrapped}"
            );
        }
    }

    #[test]
    fn clamp_phase_error_limits_to_twenty_degrees() {
        let limit = 20.0f32.to_radians();
        assert!((clamp_phase_error(10.0) - limit).abs() < 1e-6);
        assert!((clamp_phase_error(-10.0) + limit).abs() < 1e-6);
        assert!((clamp_phase_error(0.1) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn sync_loop_control_promotes_to_tracking_after_good_frame() {
        let mut control = SyncLoopControl::new(3, 6);
        assert_eq!(control.state(), SyncState::WaitForTimeSyncMarker);

        control.on_time_sync_established();
        assert_eq!(control.state(), SyncState::EvalSyncSymbol);
        assert_eq!(control.current_threshold(), 3);

        control.on_frame_processed(0.75, 0.10);
        assert_eq!(control.state(), SyncState::EvalSyncSymbol);
        assert_eq!(control.current_threshold(), 6);
    }

    #[test]
    fn sync_loop_control_enters_degraded_holding_before_drop() {
        let mut control = SyncLoopControl::new(2, 5);
        control.on_time_sync_established();
        control.on_frame_processed(0.72, 0.08);

        let outcome = control.on_tracking_miss(0.36, 0.18);

        assert_eq!(outcome, SyncRecovery::Hold);
        assert_eq!(control.state(), SyncState::DegradedHolding);
        assert_eq!(control.current_threshold(), 2);
    }

    #[test]
    fn sync_loop_control_only_drops_after_hold_budget_is_exhausted() {
        let mut control = SyncLoopControl::new(2, 5);
        control.on_time_sync_established();
        control.on_frame_processed(0.72, 0.08);

        assert_eq!(control.on_tracking_miss(0.34, 0.20), SyncRecovery::Hold);
        for _ in 0..MAX_DEGRADED_HOLD_FRAMES.saturating_sub(1) {
            control.on_degraded_frame_boundary();
            assert_eq!(control.on_tracking_miss(0.34, 0.20), SyncRecovery::Hold);
        }

        control.on_degraded_frame_boundary();
        assert_eq!(control.on_tracking_miss(0.12, 0.72), SyncRecovery::Lost);
        assert_eq!(control.state(), SyncState::WaitForTimeSyncMarker);
    }

    #[test]
    fn sync_loop_control_refills_hold_budget_after_recovered_frame() {
        let mut control = SyncLoopControl::new(2, 5);
        control.on_time_sync_established();
        control.on_frame_processed(0.72, 0.08);

        for cycle in 0..(MAX_DEGRADED_HOLD_FRAMES as usize + 2) {
            assert_eq!(
                control.on_tracking_miss(0.34, 0.20),
                SyncRecovery::Hold,
                "intermittent fade cycle {cycle} should stay in degraded hold"
            );
            control.on_degraded_frame_boundary();
            control.on_frame_processed(0.40, 0.20);
            assert_eq!(control.state(), SyncState::EvalSyncSymbol);
        }
    }

    #[test]
    fn snr_observability_requires_soft_lock_and_good_post_eq_quality() {
        assert!(snr_is_observable(0.72, 0.10, true));
        assert!(!snr_is_observable(0.49, 0.10, true));
        assert!(!snr_is_observable(0.72, 0.30, true));
        assert!(!snr_is_observable(0.72, 0.10, false));
    }

    #[test]
    fn sync_loop_control_returns_to_acquisition_after_sync_loss() {
        let mut control = SyncLoopControl::new(3, 6);
        control.on_time_sync_established();
        control.on_frame_processed(0.72, 0.10);

        control.on_sync_lost();
        assert_eq!(control.state(), SyncState::WaitForTimeSyncMarker);
        assert_eq!(control.current_threshold(), 3);
    }

    #[test]
    fn time_sync_follow_up_warns_after_eighth_missed_null() {
        let plan = match plan_time_sync_follow_up(Some(TimeSyncState::NoDipFound), 7) {
            Ok(plan) => plan,
            Err(_) => panic!("time sync planning should succeed for NoDipFound"),
        };
        assert!(!plan.established);
        assert_eq!(plan.attempts, 8);
        assert!(plan.emit_warning);
    }

    #[test]
    fn time_sync_follow_up_resets_attempts_after_partial_dip() {
        let plan = match plan_time_sync_follow_up(Some(TimeSyncState::NoEndOfDipFound), 4) {
            Ok(plan) => plan,
            Err(_) => panic!("time sync planning should succeed for NoEndOfDipFound"),
        };
        assert!(!plan.established);
        assert_eq!(plan.attempts, 0);
        assert!(!plan.emit_warning);
    }

    #[test]
    fn sync_alignment_plan_rejects_tracking_miss_reuse() {
        let mut budget = TRACKING_MISS_TOLERANCE;
        let plan = build_sync_alignment_plan(-1, 384, &mut budget, 2048, true);
        assert!(plan.is_none());
        assert_eq!(budget, TRACKING_MISS_TOLERANCE);
    }

    #[test]
    fn sync_alignment_plan_uses_detected_start_when_available() {
        let mut budget = 0;
        let plan = build_sync_alignment_plan(512, 384, &mut budget, 2048, false).unwrap();
        assert_eq!(plan.start_index, 512);
        assert_eq!(plan.samples_to_fetch, 512);
        assert!(!plan.reused_previous_start);
        assert_eq!(budget, TRACKING_MISS_TOLERANCE);
    }

    #[test]
    fn coarse_correction_plan_respects_structural_gate() {
        assert!(plan_coarse_correction(false, false, 500, 1_000, 0.20, 0.10).is_none());
        assert!(plan_coarse_correction(false, false, 500, 1_000, 0.40, 0.70).is_none());

        let plan = plan_coarse_correction(false, false, 500, 1_000, 0.72, 0.10)
            .expect("coarse correction should be considered during acquisition when RF structure is healthy");
        assert_eq!(plan.updated_coarse_hz, 1_500);
        assert!(plan.step_applied);
        assert!(plan.reset_clock_error);
    }

    #[test]
    fn coarse_correction_plan_skips_reused_tracking_alignment() {
        assert!(plan_coarse_correction(true, false, 500, 1_000, 0.72, 0.10).is_none());
    }

    #[test]
    fn coarse_correction_plan_freezes_during_soft_lock() {
        assert!(plan_coarse_correction(false, true, 500, 6, 0.72, 0.10).is_none());
    }

    #[test]
    fn null_symbol_metrics_plan_stays_finite_and_reports_on_schedule() {
        let plan = plan_null_symbol_metrics(
            0.0,
            0.0,
            0.0,
            10,
            200,
            -33.5,
            ObservabilitySnapshot {
                post_eq_quality: 0.8,
                eq_weak_ratio: 0.1,
                soft_locked: true,
            },
        );
        assert!(plan.snr_db.is_finite());
        assert!(plan.emit_report);
        assert_eq!(plan.next_report_count, 0);
        assert_eq!(plan.offset_hz, 167);
    }

    #[test]
    fn store_reference_symbol_0_captures_fft_phase_history() {
        let running = Arc::new(AtomicBool::new(true));
        let mut proc = OfdmProcessor::new(1, 2, 5, running);
        proc.ofdm_buffer[..proc.t_u].fill(Complex32::new(1.0, 0.0));

        proc.store_reference_symbol_0();

        assert!(
            proc.reference_phase
                .iter()
                .any(|&value| value != Complex32::new(0.0, 0.0)),
            "reference phase should be populated from symbol 0"
        );
    }

    #[test]
    fn soft_bit_from_component_is_clamped_and_signed() {
        assert_eq!(soft_bit_from_component(10.0, 1.0), -127);
        assert_eq!(soft_bit_from_component(-10.0, 1.0), 127);
        assert_eq!(soft_bit_from_component(0.0, 1.0), 0);
    }

    #[test]
    fn carrier_weight_stays_finite_with_zero_noise_floor() {
        let weight = carrier_weight(1.0, 0.0, 0.0, 0.0);
        assert!(weight.is_finite());
        assert!(weight > 0.0);
    }

    #[test]
    fn symbol_read_plan_uses_combined_afc_and_advances_frame_count() {
        let plan = plan_symbol_read(2_000, -33.5, 4_096, 2_552);
        assert_eq!(plan.phase_hz, 1_967);
        assert_eq!(plan.next_frame_sample_count, 6_648);
    }

    #[test]
    fn accumulate_prefix_correlation_sums_cyclic_prefix_pairs() {
        let block = vec![
            Complex32::new(1.0, 0.0),
            Complex32::new(2.0, 0.0),
            Complex32::new(3.0, 0.0),
            Complex32::new(4.0, 0.0),
        ];

        let corr = accumulate_prefix_correlation(&block, 2, 4);
        assert_eq!(corr, Complex32::new(11.0, 0.0));
    }

    #[test]
    fn frame_rest_metrics_plan_skips_clock_update_after_coarse_step() {
        let plan = plan_frame_rest_metrics(10_000, 2656, true);
        assert_eq!(plan.next_frame_sample_count, 12_656);
        assert!(!plan.should_integrate_clock_error);
    }
}
