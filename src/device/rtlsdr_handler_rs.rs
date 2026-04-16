// RTL-SDR handler using the rtl-sdr-rs pure-Rust crate.
// Replaces the previous bindgen/FFI bindings against the vendored librtlsdr C library.

use num_complex::Complex32;
use rtl_sdr_rs::{DeviceId, RtlSdr, TunerGain};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tracing::{debug, info, warn};

const READLEN_DEFAULT: usize = 8192;
const INPUT_RATE: u32 = 2_048_000;

/// Offset-tuning: hardware is tuned `INPUT_RATE / 4` Hz above the target
/// DAB channel, and a compensating digital frequency rotation brings the
/// signal back to DC.  This moves the RTL-SDR LO leakthrough spike to
/// ±512 kHz from the DAB centre, away from the low-index OFDM subcarriers.
pub const OFFSET_TUNING_HZ: i32 = (INPUT_RATE / 4) as i32; // 512 000 Hz

// Compile-time guard: the saturating_add cast below requires a positive value.
const _: () = assert!(OFFSET_TUNING_HZ > 0, "OFFSET_TUNING_HZ must be positive");

/// SAGC: upper signal level threshold (int8 absolute-value scale, 0–127).
/// Gain is reduced when the running estimate exceeds this value.
/// Lowered from the AbracaDABra default (105 ≈ 82 %) to 95 (≈ 75 %) to shift
/// the operating window away from gain-step boundaries and reduce hunting.
const SAGC_LEVEL_MAX: f32 = 95.0;
/// SAGC: fast attack time constant — reacts quickly to overload.
/// Mirrors AbracaDABra's `m_agcLevel_catt`.
const SAGC_CATT: f32 = 0.1;
/// SAGC: slow release time constant — decays gently when the signal drops.
/// Mirrors AbracaDABra's `m_agcLevel_crel`.
const SAGC_CREL: f32 = 0.00005;
/// Number of read buffers between SAGC gain-adjustment checks.
/// Mirrors AbracaDABra's `0x03` counter mask (every 4 callbacks).
const SAGC_CHECK_INTERVAL: u32 = 4;

/// DOC: DC Offset Correction time constant.
/// Mirrors AbracaDABra's `m_doc_c = 0.05`.
/// A first-order IIR low-pass filter tracks the average I/Q DC bias introduced
/// by the RTL-SDR LO leakthrough and ADC offset; the estimated bias is then
/// subtracted from every sample before normalisation.
const DOC_C: f32 = 0.05;

/// IQ imbalance correction: per-sample IIR time constant.
///
/// α = 1 / Fs gives a time constant of 1 second at 2.048 MS/s, matching
/// DABstar's choice (see `SampleReader::getSamples()`, GPLv2).
/// Tracks E[I²], E[I×Q] and E[Q²] to estimate and cancel phase and amplitude
/// imbalance between the two ADC branches of the RTL-SDR.
const IQC_ALPHA: f32 = 1.0 / INPUT_RATE as f32; // ≈ 4.88 × 10⁻⁷

/// Division guard for IQ correction denominators.
/// Prevents division by zero / NaN when the mean power estimators have not
/// yet converged or when the input channel is silent.
const IQC_EPSILON: f32 = 1e-10;

/// Number of USB read buffers discarded on startup before IQ data enters the FIFO.
/// Prevents stale IQ data from before `reset_buffer()` contaminating the OFDM sync.
/// Mirrors AbracaDABra's `RTLSDR_RESTART_COUNTER`.
const SAGC_STARTUP_DISCARD: u32 = 2;
/// Minimum number of SAGC evaluation ticks that must elapse after a gain change
/// before the next change is allowed.  Each tick is `SAGC_CHECK_INTERVAL` read
/// buffers, so the freeze lasts `SAGC_HOLD_BUFFERS × SAGC_CHECK_INTERVAL × 8192`
/// IQ samples ≈ 32 × 4 × 8192 / 2_048_000 s ≈ 512 ms.
/// This gives the R820T tuner and the IIR level estimator enough time to settle
/// before the next gain decision, preventing rapid toggling at borderline levels.
const SAGC_HOLD_BUFFERS: u32 = 32;
/// Number of consecutive SAGC evaluation ticks where the level must stay above
/// or below a threshold before a gain change is applied.  Each tick is
/// `SAGC_CHECK_INTERVAL` buffers (≈ 16 ms), so 3 ticks ≈ 48 ms of confirmation.
/// Prevents a single noisy measurement from triggering an unnecessary gain step.
const SAGC_CONFIRM_COUNT: u32 = 3;
/// Number of consecutive direction reversals (↑↓ or ↓↑ pairs) that trigger the
/// hunting suppressor.  A value of 3 means the pattern ↑↓↑ or ↓↑↓ is enough.
const SAGC_HUNT_THRESHOLD: u32 = 3;
/// Base number of SAGC evaluation ticks to freeze after the first hunting episode.
/// Each tick is `SAGC_CHECK_INTERVAL` buffers:
/// 80 × 4 × 8192 / 2_048_000 s ≈ 1.3 s base freeze; doubles each episode.
const SAGC_HUNT_FREEZE_BASE: u32 = 80;
/// Number of SAGC evaluation ticks after startup during which the hunting detector
/// is inactive.  The first seconds of reception involve rapid gain adjustments as
/// the OFDM sync is acquired; without a grace period these trigger the hunting
/// suppressor and lock the gain for the rest of the session.
/// 128 × 4 × 8192 / 2_048_000 s ≈ 2 s grace window.
const SAGC_HUNT_GRACE_TICKS: u32 = 128;
/// Maximum number of ticks the hunting freeze can reach after repeated doubling.
/// 300 × 4 × 8192 / 2_048_000 s ≈ 4.8 s — prevents permanent SAGC lockout after
/// repeated gain-direction reversals caused by slowly-fading DAB channels.
/// Even after several doubling episodes the SAGC recovers within ≤ 5 s and can
/// respond to the next legitimate fade.
const SAGC_HUNT_FREEZE_MAX: u32 = 300;
/// Number of SAGC evaluation ticks without a gain-direction reversal before the
/// `hunt_count` accumulator is reset to zero.
///
/// True hunting is rapid oscillation (UP/DOWN at < 1 s/cycle); legitimate AGC
/// responses to DAB fades are slow (fade–recovery cycles are 10–30 s apart).
/// After `SAGC_HUNT_TIMEOUT_TICKS` ticks with no reversal (≈ 6 s), `hunt_count`
/// is zeroed so the next fade is treated as a fresh event and not blocked by
/// accumulated history from previous fade cycles.
/// 750 × 4 × 8192 / 2_048_000 s ≈ 6 s.
const SAGC_HUNT_TIMEOUT_TICKS: u32 = 750;
/// SAGC: ADC clipping threshold on the ±128 raw-byte scale.
/// Samples whose absolute value exceeds this are considered near-saturation.
/// 120 / 127 ≈ 94.5 % of ADC full scale.
const SAGC_CLIP_THRESHOLD: f32 = 120.0;
/// SAGC: maximum fraction of I/Q scalar values allowed near ADC saturation per
/// evaluation interval before the clipping path forces an immediate gain step
/// down, bypassing the normal hold / confirm protection.
///
/// DAB OFDM signals have a peak-to-average ratio of ~10 dB. The slow IIR mean
/// estimator (SAGC_CREL) can converge below SAGC_LEVEL_MAX while instantaneous
/// peaks still clip the ADC, distorting the OFDM and causing sync failures.
/// When more than 5 % of raw samples approach ADC saturation, the SAGC forces
/// the gain down regardless of the mean-level state.
const SAGC_CLIP_RATE_MAX: f32 = 0.05;

/// Absolute signal-level floor on the ±128 scale below which the SAGC considers
/// the signal absent (no aerial, genuine silence, or device disconnected).
/// 3.0 / 128 ≈ 2.3 % ADC full scale.
///
/// The slow IIR release (`SAGC_CREL`) can keep `agc_level` high for > 30 s after
/// signal loss, preventing gain recovery. When `agc_level` stays below this floor
/// for `SAGC_SILENCE_TICKS` consecutive evaluation ticks, the level estimator is
/// reset to 0 and the hunt history is cleared so the SAGC is ready to find the
/// correct gain step when the signal returns.
const SAGC_SILENCE_FLOOR: f32 = 3.0;
/// Number of consecutive evaluation ticks where `agc_level < SAGC_SILENCE_FLOOR`
/// before the silence-recovery path fires.
/// 10 × 4 × 8192 / 2_048_000 s ≈ 160 ms of confirmed silence.
const SAGC_SILENCE_TICKS: u32 = 10;
/// Number of consecutive "stable zone" evaluation ticks (signal between
/// `agc_level_min` and `SAGC_LEVEL_MAX`, no hold-off, no hunt freeze) required
/// to reset the hunt-freeze backoff multiplier to its base value.
///
/// Without this reset, `hunt_freeze_ticks` doubles on every hunting episode and
/// never decreases, leaving the SAGC permanently locked after an early episode
/// even when the signal later stabilises.
/// 500 × 4 × 8192 / 2_048_000 s ≈ 8 s of stability required.
const SAGC_HUNT_RESET_TICKS: u32 = 500;

#[inline]
fn compute_clip_rate(clip_count: u32, sagc_check_interval: u32, n_pairs: usize) -> f32 {
    let total_samples = sagc_check_interval * n_pairs as u32 * 2;
    if total_samples == 0 {
        0.0
    } else {
        clip_count as f32 / total_samples as f32
    }
}

#[inline]
fn should_force_clip_gain_down(
    clip_rate: f32,
    gain_idx: usize,
    hunt_freeze: u32,
    agc_hold_cntr: u32,
) -> bool {
    clip_rate > SAGC_CLIP_RATE_MAX && gain_idx > 0 && hunt_freeze == 0 && agc_hold_cntr == 0
}

#[inline]
fn update_sagc_silence_counter(agc_level: f32, silence_counter: u32) -> u32 {
    if agc_level >= SAGC_SILENCE_FLOOR {
        0
    } else {
        silence_counter.saturating_add(1)
    }
}

#[inline]
fn should_reset_on_silence(silence_counter: u32) -> bool {
    silence_counter >= SAGC_SILENCE_TICKS
}

#[inline]
fn apply_hunt_backoff_reset_if_stable(hunt_stable_cntr: u32, hunt_freeze_ticks: u32) -> (u32, u32) {
    let stable_next = hunt_stable_cntr.saturating_add(1);
    if stable_next >= SAGC_HUNT_RESET_TICKS && hunt_freeze_ticks > SAGC_HUNT_FREEZE_BASE {
        (0, SAGC_HUNT_FREEZE_BASE)
    } else {
        (stable_next, hunt_freeze_ticks)
    }
}

/// Select the closest gain value from `gains` (sorted list in tenths of dB)
/// given a percentage `gain_pct` in `0..=100`.
#[allow(dead_code)]
fn select_gain_from_percent(gain_pct: i16, gains: &[i32]) -> i32 {
    assert!(!gains.is_empty(), "gains list must not be empty");
    let index = (gain_pct.clamp(0, 100) as usize * (gains.len() - 1)) / 100;
    gains[index]
}

/// Compute the lower signal-level threshold factor for each gain index.
///
/// When the running SAGC level estimate falls below `factors[i] × SAGC_LEVEL_MAX`, the
/// controller bumps the gain up from index `i` to `i + 1`. The factor accounts for the size
/// of that gain step plus an adaptive hysteresis that scales with the actual step size
/// reported by the device:
///
/// ```text
/// hysteresis = max(step, 20)          // at least 2 dB, more for larger steps
/// factors[i] = 10 ^ ((gains[i] − gains[i+1] − hysteresis) / 200)
/// ```
///
/// where `gains` is sorted ascending in tenths of dB (as returned by the RTL-SDR driver).
/// Using the step itself as a floor ensures the dead-band between the up-threshold and
/// `SAGC_LEVEL_MAX` is always wide enough to prevent hunting, regardless of how closely
/// the tuner's gain steps are spaced (e.g. 36.4/37.2 dB = only 8 tenths apart).
/// For the highest gain index a fixed −5 dB factor is used (it is never triggered in
/// practice because the gain cannot be raised further).
/// Adapted from AbracaDABra by KejPi (MIT licence).
fn compute_level_min_factors(gains: &[i32]) -> Vec<f32> {
    assert!(!gains.is_empty(), "gains list must not be empty");
    let n = gains.len();
    (0..n)
        .map(|i| {
            if i + 1 < n {
                let step = gains[i + 1] - gains[i]; // tenths of dB
                                                    // Adaptive hysteresis: at least 2 dB, or the full gain step for
                                                    // larger steps. Scales automatically with the device's gain table.
                let hysteresis = step.max(20);
                f32::powf(10.0, (gains[i] - gains[i + 1] - hysteresis) as f32 / 200.0)
            } else {
                // at maximum gain: fixed −5 dB (does not trigger in practice)
                f32::powf(10.0, -5.0_f32 / 20.0)
            }
        })
        .collect()
}

/// Gain control mode for the RTL-SDR tuner.
#[derive(Clone, Debug, PartialEq)]
pub enum GainMode {
    /// Software AGC (SAGC): gain is stepped by the application based on a
    /// fast-attack / slow-release signal-level estimator.
    /// Adapted from AbracaDABra (KejPi, MIT licence).
    Software,
    /// Driver AGC: request a backend-native gain controller when available.
    /// The pure Rust backend falls back to application SAGC.
    Driver,
    /// Hardware AGC: gain is delegated to the RTL-SDR chip (`TunerGain::Auto`).
    Hardware,
    /// Fixed gain given as a percentage of the tuner's gain range (0–100).
    Manual(i16),
}

/// Configuration passed to the worker thread when spawning.
/// All fields are `Clone + Send`, avoiding the `!Send` constraint on `RtlSdr`.
#[derive(Clone)]
struct DeviceConfig {
    device_index: usize,
    frequency: u32,
    ppm_offset: i32,
    gain_mode: GainMode,
    /// When `true`, the hardware is tuned `OFFSET_TUNING_HZ` above the target
    /// frequency and a digital frequency rotation brings the signal back to DC.
    /// This moves the LO leakthrough spike away from the centre OFDM subcarriers.
    offset_tuning: bool,
    /// When `true`, a second-order IIR estimator tracks and corrects phase and
    /// amplitude imbalance between the I and Q ADC branches (IQ correction).
    /// Enabled by default; disable with `--no-iq-correction`.
    iq_correction: bool,
}

/// Shared IQ FIFO between the USB worker thread and the OFDM consumer.
///
/// Stores DC-corrected, normalised f32 IQ samples (I and Q interleaved).
/// The `Condvar` lets the OFDM thread block without spin-polling, mirroring
/// AbracaDABra's `pthread_cond_wait` approach.
struct IqFifo {
    buf: Mutex<VecDeque<f32>>,
    data_ready: Condvar,
}

impl IqFifo {
    fn new(capacity: usize) -> Self {
        IqFifo {
            buf: Mutex::new(VecDeque::with_capacity(capacity)),
            data_ready: Condvar::new(),
        }
    }
}

pub struct RtlsdrHandler {
    config: DeviceConfig,
    running: Arc<AtomicBool>,
    /// Current tuner gain in tenths of dB, updated by the SAGC worker thread.
    /// Value is -1 when hardware AGC is active (gain is unknown).
    current_gain_tenths: Arc<AtomicI32>,
    fifo: Arc<IqFifo>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl RtlsdrHandler {
    /// Open the RTL-SDR device and build a handler ready for streaming.
    ///
    /// * `frequency` — target DAB channel centre frequency in Hz.
    /// * `ppm_offset` — crystal frequency correction in parts-per-million.
    /// * `gain_mode` — `Software` (SAGC), `Hardware` (chip AGC), or `Manual(pct)`.
    /// * `device_index` — USB device index (0 for the first/only dongle).
    /// * `offset_tuning` — when `true`, the hardware is tuned [`OFFSET_TUNING_HZ`]
    ///   above `frequency` and a compensating digital rotation shifts the signal
    ///   back to DC, moving the LO leakthrough spike out of the low-index OFDM
    ///   subcarriers.
    /// * `iq_correction` — when `true`, a second-order IIR estimator tracks
    ///   and corrects ADC IQ imbalance (phase and amplitude mismatch between
    ///   the I and Q branches).
    pub fn new(
        frequency: u32,
        ppm_offset: i32,
        gain_mode: GainMode,
        device_index: u32,
        offset_tuning: bool,
        iq_correction: bool,
    ) -> Result<Self, String> {
        // Capacity: 1 Mi f32 values = 512 Ki IQ pairs ≈ 250 ms at 2 Msps.
        let capacity = 1024 * 1024;
        Ok(RtlsdrHandler {
            config: DeviceConfig {
                device_index: device_index as usize,
                frequency,
                ppm_offset,
                gain_mode,
                offset_tuning,
                iq_correction,
            },
            running: Arc::new(AtomicBool::new(false)),
            current_gain_tenths: Arc::new(AtomicI32::new(0)),
            fifo: Arc::new(IqFifo::new(capacity)),
            worker_handle: None,
        })
    }

    pub fn restart_reader(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }

        {
            let mut buf = self.fifo.buf.lock().unwrap();
            buf.clear();
        }

        let config = self.config.clone();
        let running = self.running.clone();
        let fifo = self.fifo.clone();
        let current_gain_tenths = self.current_gain_tenths.clone();

        // Oneshot channel: worker reports init success/failure before entering
        // the read loop.  This avoids opening the device twice.
        let (init_tx, init_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

        self.running.store(true, Ordering::SeqCst);

        self.worker_handle = Some(thread::spawn(move || {
            let mut sdr = match RtlSdr::open(DeviceId::Index(config.device_index)) {
                Ok(d) => d,
                Err(e) => {
                    let msg = format!(
                        "Failed to open RTL-SDR device {}: {}",
                        config.device_index, e
                    );
                    let _ = init_tx.send(Err(msg));
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            // Read gain table and select gain — all in a single device session.
            let gains = match sdr.get_tuner_gains() {
                Ok(g) => g,
                Err(e) => {
                    let msg = format!("Reading tuner gains failed: {}", e);
                    let _ = init_tx.send(Err(msg));
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let gains_str: Vec<String> = gains
                .iter()
                .map(|g| format!("{}.{}", g / 10, g % 10))
                .collect();
            tracing::debug!(
                "Supported gain values ({}): {}",
                gains.len(),
                gains_str.join(" ")
            );

            // Resolve the initial gain index and operating mode.
            let (initial_gain_idx, sagc_enabled, hardware_agc) = match config.gain_mode {
                GainMode::Software => {
                    let idx = (gains.len() - 1) * 50 / 100;
                    info!(
                        "Software AGC (SAGC) enabled, starting at {:.1} dB (50%)",
                        gains[idx] as f32 / 10.0
                    );
                    (idx, true, false)
                }
                GainMode::Driver => {
                    let idx = (gains.len() - 1) * 50 / 100;
                    warn!(
                        "Driver AGC requested on the pure Rust RTL backend; falling back to application SAGC"
                    );
                    (idx, true, false)
                }
                GainMode::Hardware => {
                    info!("Hardware AGC enabled (RTL-SDR chip)");
                    (0, false, true)
                }
                GainMode::Manual(pct) => {
                    let idx = (pct.clamp(0, 100) as usize * (gains.len() - 1)) / 100;
                    info!("Manual gain set to {:.1} dB", gains[idx] as f32 / 10.0);
                    (idx, false, false)
                }
            };

            if let Err(e) = sdr.set_sample_rate(INPUT_RATE) {
                let msg = format!("set_sample_rate failed: {}", e);
                let _ = init_tx.send(Err(msg));
                running.store(false, Ordering::SeqCst);
                return;
            }

            if config.ppm_offset != 0 {
                if let Err(e) = sdr.set_freq_correction(config.ppm_offset) {
                    warn!("set_freq_correction failed: {}", e);
                }
            }

            // When offset tuning is enabled, the hardware is tuned OFFSET_TUNING_HZ
            // above the requested DAB frequency.  A compensating digital rotation in
            // the sample loop shifts the signal back to DC, moving the LO leakthrough
            // spike away from the low-index OFDM subcarriers.
            let hw_freq = if config.offset_tuning {
                config.frequency.saturating_add(OFFSET_TUNING_HZ as u32)
            } else {
                config.frequency
            };
            if let Err(e) = sdr.set_center_freq(hw_freq) {
                tracing::error!("set_center_freq failed: {}", e);
                running.store(false, Ordering::SeqCst);
                return;
            }

            let initial_tuner_gain = if hardware_agc {
                TunerGain::Auto
            } else {
                TunerGain::Manual(gains[initial_gain_idx])
            };
            if let Err(e) = sdr.set_tuner_gain(initial_tuner_gain) {
                warn!("set_tuner_gain failed: {}", e);
            }

            if let Err(e) = sdr.reset_buffer() {
                let msg = format!("reset_buffer failed: {}", e);
                let _ = init_tx.send(Err(msg));
                running.store(false, Ordering::SeqCst);
                return;
            }

            info!(
                "Tuned to {} Hz (DAB centre {} Hz), sample rate {} S/s{}",
                sdr.get_center_freq(),
                config.frequency,
                sdr.get_sample_rate(),
                if config.offset_tuning {
                    format!(" [offset-tuning +{} Hz]", OFFSET_TUNING_HZ)
                } else {
                    String::new()
                }
            );

            // Signal successful initialization to the main thread.
            let _ = init_tx.send(Ok(()));

            // Publish the initial gain so the status thread can read it immediately.
            if hardware_agc {
                current_gain_tenths.store(-1, Ordering::Relaxed);
            } else {
                current_gain_tenths.store(gains[initial_gain_idx], Ordering::Relaxed);
            }

            // ── SAGC state ────────────────────────────────────────────────────
            // Pre-compute the lower threshold factor for every gain step.
            // Adapted from AbracaDABra's agcLevelMinFactorList.
            let level_min_factors = compute_level_min_factors(&gains);
            let mut gain_idx = initial_gain_idx;
            // Running level estimate on the absolute int8 scale (0–127).
            // Note: agc_level is intentionally NOT reset on re-tune, mirroring
            // AbracaDABra's commented-out `// m_agcLevel = 0.0;`.
            let mut agc_level = 0.0f32;
            let mut agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
            // Counter: gain is re-evaluated every SAGC_CHECK_INTERVAL read buffers.
            let mut agc_read_cntr: u32 = 0;
            // Hold-off counter: decremented at each SAGC evaluation tick.
            // When > 0, gain adjustments are suspended to prevent oscillation.
            let mut agc_hold_cntr: u32 = 0;
            // Confirmation counters: a gain change only fires after SAGC_CONFIRM_COUNT
            // consecutive evaluation ticks all agree on the same direction.
            // Prevents a single noisy measurement from triggering a gain step.
            let mut agc_up_confirm: u32 = 0; // ticks where level < level_min
            let mut agc_down_confirm: u32 = 0; // ticks where level > SAGC_LEVEL_MAX
                                               // Count of raw I/Q scalar values that exceeded SAGC_CLIP_THRESHOLD since
                                               // the last SAGC evaluation tick. Reset after each evaluation.
            let mut clip_count: u32 = 0;
            // Hunting suppressor: tracks consecutive direction reversals and
            // freezes the control loop when the gain bounces between two adjacent
            // steps with no stable operating point.  Uses exponential backoff so
            // that repeated episodes on the same pair converge to a stable lock
            // on the lower (safer) gain step.
            let mut last_gain_dir: i32 = 0; // +1 = last change was up, -1 = down
            let mut hunt_count: u32 = 0;
            let mut hunt_freeze: u32 = 0;
            // Backoff multiplier: doubles with each hunting episode (capped at max).
            let mut hunt_freeze_ticks: u32 = SAGC_HUNT_FREEZE_BASE;
            // Grace period: hunting detection is suppressed for the first
            // SAGC_HUNT_GRACE_TICKS evaluation ticks after startup so that the
            // rapid gain steps during initial OFDM sync acquisition do not
            // immediately trigger the hunting suppressor and lock the gain.
            let mut hunt_grace_cntr: u32 = SAGC_HUNT_GRACE_TICKS;
            // Silence detector: consecutive evaluation ticks where agc_level is
            // below SAGC_SILENCE_FLOOR. When it reaches SAGC_SILENCE_TICKS, the
            // level estimator is reset so gain can recover without waiting for the
            // slow IIR release.
            let mut sagc_silence_cntr: u32 = 0;
            // Stability counter: consecutive "stable zone" evaluation ticks.
            // When it reaches SAGC_HUNT_RESET_TICKS, the hunt-freeze backoff
            // multiplier is reset to SAGC_HUNT_FREEZE_BASE.
            let mut hunt_stable_cntr: u32 = 0;
            // Inactivity timer: evaluation ticks since the last gain-direction
            // reversal.  When it reaches SAGC_HUNT_TIMEOUT_TICKS, `hunt_count`
            // is reset to 0 so slowly-recurring fade cycles are not mistaken
            // for rapid hunting.
            let mut hunt_last_reversal_ticks: u32 = 0;

            // ── DOC state ─────────────────────────────────────────────────────
            // DC Offset Correction: independent IIR estimators for I and Q.
            // Adapted from AbracaDABra's `m_dcI` / `m_dcQ` (MIT licence).
            // Operates on the ±128 integer scale (before /128 normalisation).
            // The estimate from the previous buffer is applied to the current
            // buffer, giving a 1-buffer lag that is negligible for a slow IIR.
            let mut dc_i = 0.0f32;
            let mut dc_q = 0.0f32;

            // ── IQ imbalance correction state ─────────────────────────────────
            // Second-order cross-channel IIR estimators on the normalised scale.
            // Initialised to (1, 0, 1) so that phi = 0 and gain_q = 1 at startup
            // — correction is effectively a no-op until the estimators converge.
            // Based on DABstar's SampleReader::getSamples() (GPLv2, Thomas Neder).
            let use_iq_correction = config.iq_correction;
            let mut iqc_mean_ii = 1.0f32; // E[I²]
            let mut iqc_mean_iq = 0.0f32; // E[I×Q]
            let mut iqc_mean_qq = 1.0f32; // E[Q²]

            // ── Offset-tuning phasor state ────────────────────────────────────
            // When offset tuning is enabled, the hardware is tuned OFFSET_TUNING_HZ
            // above the DAB channel centre.  The signal appears at -OFFSET_TUNING_HZ
            // in the digital baseband.  Multiplying sample n by
            //   e^(+j · 2π · OFFSET_TUNING_HZ / Fs · n)
            // shifts it back to DC.  The phasor is advanced per-sample and
            // renormalised once per buffer to prevent floating-point drift.
            let use_offset_tuning = config.offset_tuning;
            let mut ot_phasor = Complex32::new(1.0_f32, 0.0_f32);
            let ot_rotation = if use_offset_tuning {
                let angle =
                    2.0 * std::f32::consts::PI * OFFSET_TUNING_HZ as f32 / INPUT_RATE as f32;
                Complex32::new(angle.cos(), angle.sin())
            } else {
                Complex32::new(1.0_f32, 0.0_f32)
            };

            // ── Startup discard ───────────────────────────────────────────────
            // Skip the first SAGC_STARTUP_DISCARD buffers so that any residual
            // IQ data from before reset_buffer() never enters the FIFO.
            // Mirrors AbracaDABra's `m_captureStartCntr`.
            let mut startup_discard = SAGC_STARTUP_DISCARD;

            // Pre-allocated scratch buffer: holds the normalised f32 samples computed
            // outside the FIFO lock so the OFDM consumer is never blocked during the
            // SAGC + DOC arithmetic (one entry per raw byte).
            let mut sample_buf = vec![0.0f32; READLEN_DEFAULT];
            let mut raw = [0u8; READLEN_DEFAULT];
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match sdr.read_sync(&mut raw) {
                    Ok(0) => {
                        tracing::debug!("read_sync returned 0 bytes; retrying");
                    }
                    Ok(_) => {
                        if startup_discard > 0 {
                            startup_discard -= 1;
                            continue;
                        }

                        let n_pairs = READLEN_DEFAULT / 2;

                        // ── Phase 1: SAGC + DOC accumulation, WITHOUT holding the FIFO lock ──
                        //
                        // Raw bytes are interleaved as I₀Q₀I₁Q₁…; each offset by 128
                        // (unsigned → signed). SAGC operates on |byte − 128| for all
                        // samples (I and Q mixed), matching AbracaDABra's per-byte loop.
                        // DOC applies the estimate from the PREVIOUS buffer (1-buffer lag).
                        //
                        // IQ conversion: I = (raw_I − 128) / 128, Q = (raw_Q − 128) / 128.
                        // This maps the RTL-SDR 8-bit unsigned ADC output (0..255) to the
                        // normalised complex baseband range (−1.0 .. +1.0).
                        // ETSI EN 300 401 §14.2 — baseband IQ representation.
                        //
                        // Decoupling the arithmetic from the mutex eliminates the contention
                        // window during which the OFDM consumer thread was blocked on
                        // get_samples() waiting for the same lock.
                        let mut sum_i = 0.0f32;
                        let mut sum_q = 0.0f32;
                        for k in 0..n_pairs {
                            let i_raw = raw[2 * k] as f32 - 128.0;
                            let q_raw = raw[2 * k + 1] as f32 - 128.0;

                            // DOC accumulation (±128 scale)
                            sum_i += i_raw;
                            sum_q += q_raw;

                            // SAGC — absolute amplitude on the raw (pre-DOC) scale.
                            // SAGC goal is to keep the ADC within its linear range; ADC
                            // saturation occurs on the raw signal, so raw abs is correct.
                            // DC offset (≈ 2–5 on the ±128 scale) is negligible for this
                            // purpose; measuring raw abs gives accurate ADC utilisation.
                            if sagc_enabled {
                                for &abs_val in &[i_raw.abs(), q_raw.abs()] {
                                    let c = if abs_val > agc_level {
                                        SAGC_CATT
                                    } else {
                                        SAGC_CREL
                                    };
                                    agc_level += c * (abs_val - agc_level);
                                    // Track ADC near-clipping for the clip-rate detector.
                                    if abs_val >= SAGC_CLIP_THRESHOLD {
                                        clip_count += 1;
                                    }
                                }
                            }

                            // Store DC-corrected, normalised sample pair in local scratch
                            sample_buf[2 * k] = (i_raw - dc_i) / 128.0;
                            sample_buf[2 * k + 1] = (q_raw - dc_q) / 128.0;
                        }

                        // ── Phase 1b: IQ imbalance correction ────────────────────────────
                        // Estimates and compensates phase and amplitude imbalance between
                        // the I and Q branches of the RTL-SDR ADC using cross-channel
                        // second-order IIR statistics on the normalised, DC-removed samples.
                        //
                        // Model (following DABstar, adapted from Windytan / SampleReader):
                        //   phi     = E[I·Q] / E[I²]        — cross-phase factor
                        //   Q_corr  = Q − phi · I            — phase-corrected Q
                        //   gain_q  = sqrt(E[I²] / E[Q²])   — amplitude correction
                        //   Q_out   = Q_corr · gain_q
                        //
                        // α = 1/Fs ≈ 4.9×10⁻⁷ → time constant ≈ 1 s.
                        // Guard against degenerate states (silence or saturated channel)
                        // with floor checks before division.
                        if use_iq_correction {
                            for k in 0..n_pairs {
                                let i = sample_buf[2 * k];
                                let q = sample_buf[2 * k + 1];
                                // Update cross-channel second-order statistics.
                                iqc_mean_ii += IQC_ALPHA * (i * i - iqc_mean_ii);
                                iqc_mean_iq += IQC_ALPHA * (i * q - iqc_mean_iq);
                                iqc_mean_qq += IQC_ALPHA * (q * q - iqc_mean_qq);
                                // Phase correction: remove I→Q leakage.
                                let phi = if iqc_mean_ii > IQC_EPSILON {
                                    iqc_mean_iq / iqc_mean_ii
                                } else {
                                    0.0
                                };
                                let q_corr = q - phi * i;
                                // Amplitude correction: equalise I and Q channel gains.
                                let gain_q = if iqc_mean_qq > IQC_EPSILON {
                                    (iqc_mean_ii / iqc_mean_qq).sqrt()
                                } else {
                                    1.0
                                };
                                sample_buf[2 * k + 1] = q_corr * gain_q;
                            }
                        }

                        // ── Phase 1c: offset-tuning frequency rotation ───────────────────
                        // When the hardware is tuned OFFSET_TUNING_HZ above the DAB
                        // centre, the signal sits at -OFFSET_TUNING_HZ in the digital
                        // baseband. Multiplying sample n by the phasor
                        //   e^(+j · 2π · OFFSET_TUNING_HZ / Fs · n)
                        // shifts it back to DC.  The phasor is advanced once per sample
                        // pair and renormalised after each full buffer to prevent
                        // floating-point amplitude drift.
                        if use_offset_tuning {
                            for k in 0..n_pairs {
                                let s = Complex32::new(sample_buf[2 * k], sample_buf[2 * k + 1]);
                                let shifted = s * ot_phasor;
                                sample_buf[2 * k] = shifted.re;
                                sample_buf[2 * k + 1] = shifted.im;
                                ot_phasor *= ot_rotation;
                            }
                            // Renormalise once per buffer to keep |phasor| = 1.
                            let norm =
                                (ot_phasor.re * ot_phasor.re + ot_phasor.im * ot_phasor.im).sqrt();
                            if norm > 0.0 {
                                ot_phasor /= norm;
                            }
                        }

                        // ── Phase 2: single lock acquisition for a bulk push ──────────────
                        // The OFDM consumer is only blocked during this short extend(), not
                        // during the heavier arithmetic above.
                        {
                            let mut guard = fifo.buf.lock().unwrap();
                            guard.extend(sample_buf[..2 * n_pairs].iter().copied());
                        }
                        fifo.data_ready.notify_all();

                        // IIR DOC update: dc_x ← (1 − DOC_C) × dc_x + DOC_C × mean_x
                        // Mirrors AbracaDABra: `dcI = sumI * doc_c / (len>>1) + dcI − doc_c * dcI`
                        dc_i += DOC_C * (sum_i / n_pairs as f32 - dc_i);
                        dc_q += DOC_C * (sum_q / n_pairs as f32 - dc_q);

                        // ── SAGC gain evaluation ──────────────────────────────
                        agc_read_cntr = agc_read_cntr.wrapping_add(1);
                        if sagc_enabled && agc_read_cntr.is_multiple_of(SAGC_CHECK_INTERVAL) {
                            // Count down the startup grace period.
                            hunt_grace_cntr = hunt_grace_cntr.saturating_sub(1);

                            // Hunt-count inactivity timeout: reset hunt_count after
                            // SAGC_HUNT_TIMEOUT_TICKS ticks without a reversal so that
                            // slow fade cycles (>= 6 s between gain-direction changes)
                            // are not misidentified as rapid hunting.
                            hunt_last_reversal_ticks = hunt_last_reversal_ticks.saturating_add(1);
                            if hunt_last_reversal_ticks >= SAGC_HUNT_TIMEOUT_TICKS && hunt_count > 0
                            {
                                debug!(
                                    "SAGC: hunt count reset after {} ticks of inactivity",
                                    hunt_last_reversal_ticks
                                );
                                hunt_count = 0;
                                hunt_last_reversal_ticks = 0;
                            }
                            // ── Clipping-path ────────────────────────────────────
                            // A DAB OFDM signal has a peak-to-average ratio of ~10 dB.
                            // The slow IIR mean estimator (SAGC_CREL) can converge below
                            // SAGC_LEVEL_MAX while instantaneous peaks still clip the ADC,
                            // causing OFDM demodulation failures. When more than
                            // SAGC_CLIP_RATE_MAX of raw samples are near-saturating the ADC,
                            // force an immediate gain step down bypassing hold / confirm.
                            let clip_rate =
                                compute_clip_rate(clip_count, SAGC_CHECK_INTERVAL, n_pairs);
                            clip_count = 0;
                            // Silence counter: maintained before every decision.
                            // Resets automatically when the signal is above the floor.
                            sagc_silence_cntr =
                                update_sagc_silence_counter(agc_level, sagc_silence_cntr);
                            if should_force_clip_gain_down(
                                clip_rate,
                                gain_idx,
                                hunt_freeze,
                                agc_hold_cntr,
                            ) {
                                gain_idx -= 1;
                                agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                                agc_hold_cntr = SAGC_HOLD_BUFFERS;
                                agc_up_confirm = 0;
                                agc_down_confirm = 0;
                                if last_gain_dir == 1 && hunt_grace_cntr == 0 {
                                    hunt_last_reversal_ticks = 0;
                                    hunt_count += 1;
                                } else {
                                    hunt_count = 0;
                                }
                                last_gain_dir = -1;
                                if let Err(e) =
                                    sdr.set_tuner_gain(TunerGain::Manual(gains[gain_idx]))
                                {
                                    warn!("SAGC: set_tuner_gain failed: {}", e);
                                } else {
                                    debug!(
                                        "SAGC: clip ↓ {:.1} dB (clip {:.1}%)",
                                        gains[gain_idx] as f32 / 10.0,
                                        clip_rate * 100.0,
                                    );
                                }
                            } else if should_reset_on_silence(sagc_silence_cntr) {
                                // True signal absence: the slow IIR release (SAGC_CREL)
                                // would otherwise keep agc_level high for > 30 s, blocking
                                // gain recovery. Reset the estimator and hunt history so the
                                // SAGC can find the right gain step when the signal returns.
                                agc_level = 0.0;
                                hunt_freeze = 0;
                                hunt_freeze_ticks = SAGC_HUNT_FREEZE_BASE;
                                hunt_count = 0;
                                last_gain_dir = 0;
                                agc_up_confirm = 0;
                                agc_down_confirm = 0;
                                hunt_stable_cntr = 0;
                                debug!("SAGC: silence detected → estimator and hunt state reset");
                            } else if hunt_freeze > 0 {
                                // Hunting suppressor active: hold current gain, reset
                                // confirmation counters so we start fresh after the freeze.
                                hunt_freeze -= 1;
                                hunt_stable_cntr = 0;
                                agc_up_confirm = 0;
                                agc_down_confirm = 0;
                            } else if agc_hold_cntr > 0 {
                                // Still in hold-off period after the last gain change.
                                agc_hold_cntr -= 1;
                                hunt_stable_cntr = 0;
                                agc_up_confirm = 0;
                                agc_down_confirm = 0;
                            } else if agc_level < agc_level_min && gain_idx + 1 < gains.len() {
                                // Level below threshold: accumulate confirmation ticks.
                                hunt_stable_cntr = 0;
                                agc_down_confirm = 0;
                                agc_up_confirm += 1;
                                if agc_up_confirm >= SAGC_CONFIRM_COUNT {
                                    agc_up_confirm = 0;
                                    // Signal too weak — increase gain by one step.
                                    gain_idx += 1;
                                    agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                                    agc_hold_cntr = SAGC_HOLD_BUFFERS;
                                    // Hunting detection: consecutive direction reversals.
                                    if last_gain_dir == -1 && hunt_grace_cntr == 0 {
                                        hunt_last_reversal_ticks = 0;
                                        hunt_count += 1;
                                        if hunt_count >= SAGC_HUNT_THRESHOLD {
                                            // Stay on the lower (safer) gain step and freeze.
                                            // The gain was incremented then immediately reverted
                                            // (net zero hardware change), so clear agc_hold_cntr
                                            // — no hardware settling time is needed.
                                            gain_idx -= 1;
                                            agc_level_min =
                                                level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                                            agc_hold_cntr = 0;
                                            hunt_freeze = hunt_freeze_ticks;
                                            hunt_freeze_ticks = hunt_freeze_ticks
                                                .saturating_mul(2)
                                                .min(SAGC_HUNT_FREEZE_MAX);
                                            hunt_count = 0;
                                            last_gain_dir = -1;
                                            warn!(
                                                "SAGC: hunting ↑↓↑ between {:.1} and {:.1} dB, \
                                                 locking {:.1} dB for {} ticks",
                                                gains[gain_idx] as f32 / 10.0,
                                                gains[gain_idx + 1] as f32 / 10.0,
                                                gains[gain_idx] as f32 / 10.0,
                                                hunt_freeze,
                                            );
                                            if let Err(e) = sdr
                                                .set_tuner_gain(TunerGain::Manual(gains[gain_idx]))
                                            {
                                                warn!("SAGC: set_tuner_gain failed: {}", e);
                                            }
                                        } else {
                                            last_gain_dir = 1;
                                            if let Err(e) = sdr
                                                .set_tuner_gain(TunerGain::Manual(gains[gain_idx]))
                                            {
                                                warn!("SAGC: set_tuner_gain failed: {}", e);
                                            } else {
                                                debug!(
                                                    "SAGC: gain ↑ {:.1} dB (level {:.1})",
                                                    gains[gain_idx] as f32 / 10.0,
                                                    agc_level,
                                                );
                                            }
                                        }
                                    } else {
                                        hunt_count = 0;
                                        last_gain_dir = 1;
                                        if let Err(e) =
                                            sdr.set_tuner_gain(TunerGain::Manual(gains[gain_idx]))
                                        {
                                            warn!("SAGC: set_tuner_gain failed: {}", e);
                                        } else {
                                            debug!(
                                                "SAGC: gain ↑ {:.1} dB (level {:.1})",
                                                gains[gain_idx] as f32 / 10.0,
                                                agc_level,
                                            );
                                        }
                                    }
                                }
                            } else if agc_level > SAGC_LEVEL_MAX && gain_idx > 0 {
                                // Level above threshold: accumulate confirmation ticks.
                                hunt_stable_cntr = 0;
                                agc_up_confirm = 0;
                                agc_down_confirm += 1;
                                if agc_down_confirm >= SAGC_CONFIRM_COUNT {
                                    agc_down_confirm = 0;
                                    // Signal too strong — decrease gain by one step.
                                    gain_idx -= 1;
                                    agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                                    agc_hold_cntr = SAGC_HOLD_BUFFERS;
                                    // Hunting detection: consecutive direction reversals.
                                    if last_gain_dir == 1 && hunt_grace_cntr == 0 {
                                        hunt_last_reversal_ticks = 0;
                                        hunt_count += 1;
                                        if hunt_count >= SAGC_HUNT_THRESHOLD {
                                            // Already on the lower step; freeze here.
                                            // agc_hold_cntr retains SAGC_HOLD_BUFFERS from the
                                            // gain step above: the hardware gain DID change, so
                                            // the hold-off is valid.  After hunt_freeze expires,
                                            // the SAGC stays held for SAGC_HOLD_BUFFERS more
                                            // ticks to let the hardware settle.
                                            hunt_freeze = hunt_freeze_ticks;
                                            hunt_freeze_ticks = hunt_freeze_ticks
                                                .saturating_mul(2)
                                                .min(SAGC_HUNT_FREEZE_MAX);
                                            hunt_count = 0;
                                            last_gain_dir = -1;
                                            warn!(
                                                "SAGC: hunting ↓↑↓ between {:.1} and {:.1} dB, \
                                                 locking {:.1} dB for {} ticks",
                                                gains[gain_idx] as f32 / 10.0,
                                                gains[gain_idx + 1] as f32 / 10.0,
                                                gains[gain_idx] as f32 / 10.0,
                                                hunt_freeze,
                                            );
                                            if let Err(e) = sdr
                                                .set_tuner_gain(TunerGain::Manual(gains[gain_idx]))
                                            {
                                                warn!("SAGC: set_tuner_gain failed: {}", e);
                                            }
                                        } else {
                                            last_gain_dir = -1;
                                            if let Err(e) = sdr
                                                .set_tuner_gain(TunerGain::Manual(gains[gain_idx]))
                                            {
                                                warn!("SAGC: set_tuner_gain failed: {}", e);
                                            } else {
                                                debug!(
                                                    "SAGC: gain ↓ {:.1} dB (level {:.1})",
                                                    gains[gain_idx] as f32 / 10.0,
                                                    agc_level,
                                                );
                                            }
                                        }
                                    } else {
                                        hunt_count = 0;
                                        last_gain_dir = -1;
                                        if let Err(e) =
                                            sdr.set_tuner_gain(TunerGain::Manual(gains[gain_idx]))
                                        {
                                            warn!("SAGC: set_tuner_gain failed: {}", e);
                                        } else {
                                            debug!(
                                                "SAGC: gain ↓ {:.1} dB (level {:.1})",
                                                gains[gain_idx] as f32 / 10.0,
                                                agc_level,
                                            );
                                        }
                                    }
                                }
                            } else {
                                // Signal in the stable zone — reset confirmation counters.
                                agc_up_confirm = 0;
                                agc_down_confirm = 0;
                                // Count consecutive stable ticks toward hunt-backoff reset.
                                // After SAGC_HUNT_RESET_TICKS ticks the backoff multiplier
                                // returns to its base value, restoring SAGC responsiveness
                                // after a sustained period of well-behaved operation.
                                let (stable_next, freeze_next) = apply_hunt_backoff_reset_if_stable(
                                    hunt_stable_cntr,
                                    hunt_freeze_ticks,
                                );
                                if freeze_next != hunt_freeze_ticks {
                                    debug!("SAGC: sustained stability → hunt backoff reset");
                                }
                                hunt_stable_cntr = stable_next;
                                hunt_freeze_ticks = freeze_next;
                            }
                        }
                        // Publish the current gain index after every SAGC evaluation so
                        // the status thread always has an up-to-date value.
                        if sagc_enabled {
                            current_gain_tenths.store(gains[gain_idx], Ordering::Relaxed);
                        }
                    }
                    Err(e) => {
                        tracing::error!("read_sync error: {}", e);
                        break;
                    }
                }
            }

            running.store(false, Ordering::SeqCst);
            // Wake any blocked consumer so it can observe running == false.
            fifo.data_ready.notify_all();
            // sdr dropped here; USB cleanup handled by rusb::DeviceHandle::Drop
        }));

        // Wait for the worker to finish device initialization before returning.
        match init_rx.recv() {
            Ok(Ok(())) => true,
            Ok(Err(msg)) => {
                tracing::error!("RTL-SDR worker init failed: {}", msg);
                self.running.store(false, Ordering::SeqCst);
                false
            }
            Err(_) => {
                tracing::error!("RTL-SDR worker exited before completing init");
                self.running.store(false, Ordering::SeqCst);
                false
            }
        }
    }

    /// Signal the USB worker thread to stop (non-blocking).  The caller can then
    /// join the OFDM thread (which owns `RtlsdrHandler`) knowing that the worker
    /// will exit as soon as the current `read_sync` call returns — typically < 1 s.
    pub fn reader_running(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    /// Returns the current tuner gain in tenths of dB as reported by the SAGC
    /// worker thread.  Returns -1 when hardware AGC is active (gain unknown).
    pub fn current_gain_tenths_db(&self) -> i32 {
        self.current_gain_tenths.load(Ordering::Relaxed)
    }

    /// Returns a clone of the shared gain atomic so callers that outlive this
    /// handler (e.g. a status thread spawned before the OFDM move) can still
    /// read the current gain.
    pub fn gain_tenths_arc(&self) -> Arc<AtomicI32> {
        self.current_gain_tenths.clone()
    }

    pub fn stop_reader(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }

    /// Read up to `v.len()` DC-corrected, normalised IQ samples from the FIFO.
    ///
    /// Blocks via `Condvar` (zero CPU spin) until enough data is available or
    /// the worker thread stops. Returns the number of samples written into `v`.
    /// Returns 0 only when the worker has stopped and no data remains.
    pub fn get_samples(&self, v: &mut [Complex32]) -> usize {
        let needed = 2 * v.len(); // interleaved f32 pairs (I, Q)
        let mut guard = self.fifo.buf.lock().unwrap();
        loop {
            if guard.len() >= needed {
                break;
            }
            if !self.running.load(Ordering::Relaxed) {
                // Worker stopped — drain whatever is available.
                break;
            }
            guard = self.fifo.data_ready.wait(guard).unwrap();
        }
        let available = guard.len().min(needed);
        let sample_count = available / 2;
        for slot in v.iter_mut().take(sample_count) {
            let i_val = guard.pop_front().unwrap_or(0.0);
            let q_val = guard.pop_front().unwrap_or(0.0);
            *slot = Complex32::new(i_val, q_val);
        }
        sample_count
    }

    pub fn samples(&self) -> usize {
        self.fifo.buf.lock().unwrap().len() / 2
    }

    pub fn reset_buffer(&self) {
        self.fifo.buf.lock().unwrap().clear();
    }
}

impl Drop for RtlsdrHandler {
    fn drop(&mut self) {
        self.stop_reader();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_gain_at_zero_percent_returns_lowest_gain() {
        let gains = vec![0, 10, 20, 30, 40];
        assert_eq!(select_gain_from_percent(0, &gains), 0);
    }

    #[test]
    fn select_gain_at_100_percent_returns_highest_gain() {
        let gains = vec![0, 10, 20, 30, 40];
        assert_eq!(select_gain_from_percent(100, &gains), 40);
    }

    #[test]
    fn select_gain_at_50_percent_returns_midpoint() {
        let gains = vec![0, 10, 20, 30, 40];
        assert_eq!(select_gain_from_percent(50, &gains), 20);
    }

    #[test]
    fn select_gain_clamps_above_100() {
        let gains = vec![0, 10, 20, 30, 40];
        assert_eq!(select_gain_from_percent(200, &gains), 40);
    }

    #[test]
    fn select_gain_clamps_below_0() {
        let gains = vec![0, 10, 20, 30, 40];
        assert_eq!(select_gain_from_percent(-10, &gains), 0);
    }

    #[test]
    fn select_gain_single_element_list() {
        let gains = vec![42];
        assert_eq!(select_gain_from_percent(50, &gains), 42);
    }

    #[test]
    fn conv_table_maps_128_to_zero() {
        // build_conv_table removed; verify the inline formula used in the worker.
        let val = (128u8 as f32 - 128.0) / 128.0;
        assert_eq!(val, 0.0);
    }

    #[test]
    fn conv_table_maps_0_to_negative_one() {
        let val = (0u8 as f32 - 128.0) / 128.0;
        assert!((val - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn conv_table_maps_255_to_near_positive_one() {
        let val = (255u8 as f32 - 128.0) / 128.0;
        assert!((val - (127.0 / 128.0)).abs() < 1e-6);
    }

    // ── DOC: DC Offset Correction estimator ──────────────────────────────────

    #[test]
    fn doc_estimator_converges_to_constant_bias() {
        // Simulate a DC bias of +10 on the ±128 scale.
        // After enough buffers the IIR estimate should be close to 10.0.
        let bias = 10.0f32;
        let mut dc = 0.0f32;
        for _ in 0..200 {
            // mean of a buffer that is all-bias
            dc += DOC_C * (bias - dc);
        }
        assert!((dc - bias).abs() < 0.1, "dc={dc:.3} expected ≈{bias}");
    }

    #[test]
    fn doc_correction_removes_bias_after_convergence() {
        // After convergence, the corrected sample should be near zero.
        let bias = 20.0f32;
        let mut dc = 0.0f32;
        for _ in 0..300 {
            dc += DOC_C * (bias - dc);
        }
        let raw = bias; // sample equal to the bias
        let corrected = raw - dc;
        assert!(
            corrected.abs() < 0.5,
            "corrected={corrected:.3} expected ≈0"
        );
    }

    #[test]
    fn doc_estimator_tracks_zero_mean_signal() {
        // A zero-mean signal (sum_i = 0) should keep dc_i near 0.
        let mut dc = 0.0f32;
        for _ in 0..100 {
            dc += DOC_C * (0.0 - dc);
        }
        assert!(dc.abs() < 1e-6);
    }

    // ── IQ imbalance correction: IIR estimator behaviour ─────────────────────

    /// After many samples with a synthetic IQ imbalance, `phi` must trend
    /// positive (leakage from I into Q) and `gain_q` must exceed 1 (Q amplitude
    /// < I amplitude → correction boosts Q).
    ///
    /// Signal: I = cos(θ), Q = 0.8·sin(θ) + 0.1·cos(θ)
    ///   → E[I·Q] = 0.1·E[cos²] = 0.05, E[I²] = 0.5, E[Q²] ≈ 0.325
    ///   → phi_∞ = 0.1, gain_q_∞ ≈ 1.24
    /// After 1 tc (1 s at 2 MS/s, 63 % convergence):
    ///   phi ≈ 0.046, gain_q ≈ 1.09.
    #[test]
    fn iqc_estimators_converge_to_known_imbalance() {
        use std::f32::consts::PI;
        let n_samples = INPUT_RATE as usize; // 1 second worth
        let mut mean_ii = 1.0f32;
        let mut mean_iq = 0.0f32;
        let mut mean_qq = 1.0f32;
        let alpha = IQC_ALPHA;
        for n in 0..n_samples {
            let theta = 2.0 * PI * 1000.0 * n as f32 / INPUT_RATE as f32;
            let i = theta.cos();
            // Distorted Q: amplitude ×0.8, plus a +0.1 phase leak from I.
            let q_distorted = 0.8 * theta.sin() + 0.1 * i;
            mean_ii += alpha * (i * i - mean_ii);
            mean_iq += alpha * (i * q_distorted - mean_iq);
            mean_qq += alpha * (q_distorted * q_distorted - mean_qq);
        }
        let phi = if mean_ii > 1e-10 {
            mean_iq / mean_ii
        } else {
            0.0
        };
        let gain_q = if mean_qq > 1e-10 {
            (mean_ii / mean_qq).sqrt()
        } else {
            1.0
        };
        // phi is converging toward +0.1 from 0; after 1 tc ≈ 0.046 expected.
        assert!(
            phi > 0.03,
            "phi={phi:.4} should be > 0.03 (positive I→Q leakage)"
        );
        assert!(
            gain_q > 1.0,
            "gain_q={gain_q:.4} should be > 1 (Q was attenuated)"
        );
    }

    /// On a balanced IQ signal (no imbalance), phi must stay near 0 and
    /// gain_q must stay near 1 after convergence.
    #[test]
    fn iqc_balanced_signal_produces_no_correction() {
        use std::f32::consts::PI;
        let n_samples = INPUT_RATE as usize;
        let mut mean_ii = 1.0f32;
        let mut mean_iq = 0.0f32;
        let mut mean_qq = 1.0f32;
        let alpha = IQC_ALPHA;
        for n in 0..n_samples {
            let theta = 2.0 * PI * 1000.0 * n as f32 / INPUT_RATE as f32;
            let i = theta.cos();
            let q = theta.sin(); // perfect quadrature, equal amplitude
            mean_ii += alpha * (i * i - mean_ii);
            mean_iq += alpha * (i * q - mean_iq);
            mean_qq += alpha * (q * q - mean_qq);
        }
        let phi = if mean_ii > IQC_EPSILON {
            mean_iq / mean_ii
        } else {
            0.0
        };
        let gain_q = if mean_qq > IQC_EPSILON {
            (mean_ii / mean_qq).sqrt()
        } else {
            1.0
        };
        assert!(
            phi.abs() < 0.01,
            "phi={phi:.6} should be ≈0 for balanced IQ"
        );
        assert!(
            (gain_q - 1.0).abs() < 0.01,
            "gain_q={gain_q:.6} should be ≈1 for balanced IQ"
        );
    }

    /// Degenerate case: I channel is silent → mean_ii stays near 0.
    /// phi and gain_q must be safe (no NaN / infinity).
    #[test]
    fn iqc_silent_i_channel_is_safe() {
        let mut mean_ii = 1.0f32;
        let mut mean_iq = 0.0f32;
        let mut mean_qq = 1.0f32;
        let alpha = IQC_ALPHA;
        for _ in 0..INPUT_RATE as usize {
            mean_ii += alpha * (0.0f32 * 0.0 - mean_ii);
            mean_iq += alpha * (0.0f32 * 0.5 - mean_iq);
            mean_qq += alpha * (0.5f32 * 0.5 - mean_qq);
        }
        let phi = if mean_ii > IQC_EPSILON {
            mean_iq / mean_ii
        } else {
            0.0
        };
        let gain_q = if mean_qq > IQC_EPSILON {
            (mean_ii / mean_qq).sqrt()
        } else {
            1.0
        };
        assert!(phi.is_finite(), "phi must be finite");
        assert!(gain_q.is_finite(), "gain_q must be finite");
    }

    // ── SAGC: compute_level_min_factors ───────────────────────────────────────

    #[test]
    fn level_min_factors_single_gain_uses_last_index_formula() {
        let factors = compute_level_min_factors(&[100]);
        assert_eq!(factors.len(), 1);
        let expected = f32::powf(10.0, -5.0_f32 / 20.0); // −5 dB
        assert!((factors[0] - expected).abs() < 1e-6);
    }

    #[test]
    fn level_min_factors_known_values() {
        // Gains in tenths of dB: 0, 9, 14.
        // steps: 9 and 5 — both < 20, so hysteresis = 20 for both.
        let gains = vec![0i32, 9, 14];
        let factors = compute_level_min_factors(&gains);
        assert_eq!(factors.len(), 3);

        // factors[0]: step=9, hysteresis=max(9,20)=20 → 10^((0 − 9 − 20) / 200)
        let f0 = f32::powf(10.0, (0 - 9 - 20) as f32 / 200.0);
        assert!((factors[0] - f0).abs() < 1e-6, "factor[0] mismatch");

        // factors[1]: step=5, hysteresis=max(5,20)=20 → 10^((9 − 14 − 20) / 200)
        let f1 = f32::powf(10.0, (9 - 14 - 20) as f32 / 200.0);
        assert!((factors[1] - f1).abs() < 1e-6, "factor[1] mismatch");

        // factors[2] = 10^(−5 / 20)  (last index)
        let f2 = f32::powf(10.0, -5.0_f32 / 20.0);
        assert!((factors[2] - f2).abs() < 1e-6, "factor[2] mismatch");
    }

    #[test]
    fn level_min_factors_adaptive_hysteresis_for_large_step() {
        // Step of 40 tenths (4 dB) > 20 → hysteresis = 40 (step itself).
        let gains = vec![0i32, 400];
        let factors = compute_level_min_factors(&gains);
        // hysteresis = max(400, 20) = 400
        let expected = f32::powf(10.0, (0 - 400 - 400) as f32 / 200.0);
        assert!(
            (factors[0] - expected).abs() < 1e-6,
            "adaptive factor mismatch"
        );
        // Must be smaller than the fixed-20 factor (larger dead-band).
        let fixed_20 = f32::powf(10.0, (0 - 400 - 20) as f32 / 200.0);
        assert!(
            factors[0] < fixed_20,
            "adaptive factor should be lower than fixed-20"
        );
    }

    #[test]
    fn level_min_factors_are_all_between_zero_and_one() {
        let gains = vec![0i32, 9, 14, 27, 37, 77, 87];
        for f in compute_level_min_factors(&gains) {
            assert!(f > 0.0 && f < 1.0, "factor {f} out of (0, 1)");
        }
    }

    // ── SAGC: EMA level estimator behaviour ───────────────────────────────────

    #[test]
    fn sagc_ema_attack_is_fast() {
        // With c_att = 0.1, the level should reach >90 % of the target in 50 steps.
        let mut level = 0.0f32;
        let sample = 100.0f32;
        for _ in 0..50 {
            let c = if sample > level { SAGC_CATT } else { SAGC_CREL };
            level += c * (sample - level);
        }
        // Geometric formula: level = 100 × (1 − (1 − c_att)^50)
        let expected = 100.0 * (1.0 - (1.0 - SAGC_CATT).powi(50));
        assert!((level - expected).abs() < 1e-3);
        assert!(level > 90.0, "expected fast attack, got level {level:.2}");
    }

    #[test]
    fn sagc_ema_release_is_slow() {
        // Starting at 100, a zero-value sample should barely reduce the level after 1 000 steps.
        let mut level = 100.0f32;
        let sample = 0.0f32;
        for _ in 0..1_000 {
            let c = if sample > level { SAGC_CATT } else { SAGC_CREL };
            level += c * (sample - level);
        }
        // Geometric formula: level = 100 × (1 − c_rel)^1000
        let expected = 100.0 * (1.0 - SAGC_CREL).powi(1_000);
        assert!((level - expected).abs() < 1e-2);
        assert!(level > 90.0, "expected slow release, got level {level:.2}");
    }

    // ── SAGC: hold-off counter behaviour ─────────────────────────────────────

    #[test]
    fn sagc_hold_off_blocks_adjustment_immediately_after_gain_change() {
        // Simulate the hold-off logic: after a gain change, agc_hold_cntr is set
        // to SAGC_HOLD_BUFFERS and the next SAGC_HOLD_BUFFERS ticks must not
        // trigger another change.
        let gains = vec![0i32, 90, 140, 270, 370];
        let level_min_factors = compute_level_min_factors(&gains);
        let mut gain_idx: usize = 2; // start in the middle
        let mut agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
        let mut agc_hold_cntr: u32 = 0;

        // Trigger a gain-up: level below minimum
        let weak_level = agc_level_min * 0.5;
        if agc_hold_cntr > 0 {
            agc_hold_cntr -= 1;
        } else if weak_level < agc_level_min && gain_idx + 1 < gains.len() {
            gain_idx += 1;
            agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
            agc_hold_cntr = SAGC_HOLD_BUFFERS;
        }
        assert_eq!(gain_idx, 3, "gain should have stepped up");
        assert_eq!(agc_hold_cntr, SAGC_HOLD_BUFFERS);

        // For the next SAGC_HOLD_BUFFERS ticks the gain must not change, even with
        // a level that would normally trigger another step.
        let initial_gain_idx = gain_idx;
        for _ in 0..SAGC_HOLD_BUFFERS {
            if agc_hold_cntr > 0 {
                agc_hold_cntr -= 1;
            } else if weak_level < agc_level_min && gain_idx + 1 < gains.len() {
                gain_idx += 1;
                agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                agc_hold_cntr = SAGC_HOLD_BUFFERS;
            }
        }
        assert_eq!(
            gain_idx, initial_gain_idx,
            "gain must not change during hold-off"
        );
        assert_eq!(agc_hold_cntr, 0, "hold-off counter must reach zero");
    }

    #[test]
    fn sagc_hold_off_allows_adjustment_after_expiry() {
        // After the hold-off expires (counter reaches 0), a further weak-signal
        // condition must be allowed to step the gain up again.
        let gains = vec![0i32, 90, 140, 270, 370];
        let level_min_factors = compute_level_min_factors(&gains);
        let mut gain_idx: usize = 1;
        let mut agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
        let mut agc_hold_cntr: u32 = 0;

        // First gain step + hold-off arm
        let weak_level = agc_level_min * 0.5;
        if agc_hold_cntr > 0 {
            agc_hold_cntr -= 1;
        } else if weak_level < agc_level_min && gain_idx + 1 < gains.len() {
            gain_idx += 1;
            agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
            agc_hold_cntr = SAGC_HOLD_BUFFERS;
        }
        assert_eq!(gain_idx, 2);

        // Drain hold-off
        for _ in 0..SAGC_HOLD_BUFFERS {
            if agc_hold_cntr > 0 {
                agc_hold_cntr -= 1;
            }
        }
        assert_eq!(agc_hold_cntr, 0);

        // After expiry, another weak-signal tick must step the gain again
        let weak_level2 = agc_level_min * 0.5;
        if agc_hold_cntr > 0 {
            agc_hold_cntr -= 1;
        } else if weak_level2 < agc_level_min && gain_idx + 1 < gains.len() {
            gain_idx += 1;
            let _ = level_min_factors[gain_idx] * SAGC_LEVEL_MAX; // threshold updated in production code
            agc_hold_cntr = SAGC_HOLD_BUFFERS;
        }
        assert_eq!(gain_idx, 3, "gain must step up again after hold-off expiry");
        assert_eq!(agc_hold_cntr, SAGC_HOLD_BUFFERS);
    }

    // ── SAGC: silence detector ────────────────────────────────────────────────

    #[test]
    fn sagc_silence_detector_resets_level_and_hunt_state() {
        // After SAGC_SILENCE_TICKS consecutive ticks with agc_level < SAGC_SILENCE_FLOOR,
        // the level estimator must be reset to 0 and all hunt state cleared so the SAGC
        // can find the right gain step when the signal returns.
        // Start artificially high (due to slow IIR release); immediately overwritten
        // to simulate signal loss before the first silence-detector tick.
        let mut agc_level = SAGC_SILENCE_FLOOR - 0.1;
        let mut hunt_freeze = 200u32;
        let mut hunt_freeze_ticks = SAGC_HUNT_FREEZE_BASE * 4;
        let mut hunt_count = 2u32;
        let mut last_gain_dir = -1i32;
        let mut sagc_silence_cntr = 0u32;

        for _ in 0..SAGC_SILENCE_TICKS {
            sagc_silence_cntr = update_sagc_silence_counter(agc_level, sagc_silence_cntr);
            if should_reset_on_silence(sagc_silence_cntr) {
                agc_level = 0.0;
                hunt_freeze = 0;
                hunt_freeze_ticks = SAGC_HUNT_FREEZE_BASE;
                hunt_count = 0;
                last_gain_dir = 0;
                sagc_silence_cntr = 0;
            }
        }

        assert_eq!(agc_level, 0.0, "agc_level must be reset to 0 on silence");
        assert_eq!(hunt_freeze, 0, "hunt_freeze must be cleared on silence");
        assert_eq!(
            hunt_freeze_ticks, SAGC_HUNT_FREEZE_BASE,
            "hunt backoff must be reset to base on silence"
        );
        assert_eq!(hunt_count, 0, "hunt_count must be cleared on silence");
        assert_eq!(last_gain_dir, 0, "last_gain_dir must be cleared on silence");
    }

    // ── SAGC: hunt backoff reset after stability ──────────────────────────────

    #[test]
    fn sagc_hunt_backoff_resets_after_stable_zone() {
        // After SAGC_HUNT_RESET_TICKS consecutive stable-zone ticks, the
        // hunt_freeze_ticks backoff multiplier must be reset to SAGC_HUNT_FREEZE_BASE,
        // restoring full SAGC responsiveness after sustained stable operation.
        let mut hunt_freeze_ticks: u32 = SAGC_HUNT_FREEZE_BASE * 8; // elevated by past hunting
        let mut hunt_stable_cntr: u32 = 0;

        for _ in 0..SAGC_HUNT_RESET_TICKS {
            (hunt_stable_cntr, hunt_freeze_ticks) =
                apply_hunt_backoff_reset_if_stable(hunt_stable_cntr, hunt_freeze_ticks);
        }

        assert_eq!(
            hunt_freeze_ticks, SAGC_HUNT_FREEZE_BASE,
            "hunt backoff must be reset to base after sustained stability"
        );
        assert_eq!(
            hunt_stable_cntr, 0,
            "stability counter must reset after firing"
        );
    }

    #[test]
    fn silence_counter_resets_when_signal_recovers() {
        let low = SAGC_SILENCE_FLOOR - 0.5;
        let high = SAGC_SILENCE_FLOOR + 1.0;
        let mut cntr = 0u32;

        cntr = update_sagc_silence_counter(low, cntr);
        cntr = update_sagc_silence_counter(low, cntr);
        assert_eq!(cntr, 2);

        cntr = update_sagc_silence_counter(high, cntr);
        assert_eq!(cntr, 0, "silence counter must clear on recovered level");
    }

    #[test]
    fn backoff_reset_helper_preserves_state_before_threshold() {
        let (cntr, freeze) = apply_hunt_backoff_reset_if_stable(10, SAGC_HUNT_FREEZE_BASE * 2);
        assert_eq!(cntr, 11);
        assert_eq!(freeze, SAGC_HUNT_FREEZE_BASE * 2);
    }

    // ── SAGC: clip-rate decision helpers ─────────────────────────────────────

    #[test]
    fn clip_rate_returns_zero_when_no_samples() {
        let rate = compute_clip_rate(10, SAGC_CHECK_INTERVAL, 0);
        assert_eq!(rate, 0.0);
    }

    #[test]
    fn clip_rate_matches_expected_ratio() {
        let n_pairs = 4096usize;
        let total_samples = SAGC_CHECK_INTERVAL * n_pairs as u32 * 2;
        let clip_count = total_samples / 10; // 10%
        let rate = compute_clip_rate(clip_count, SAGC_CHECK_INTERVAL, n_pairs);
        let expected = clip_count as f32 / total_samples as f32;
        assert!(
            (rate - expected).abs() < 1e-9,
            "clip rate mismatch: expected {expected}, got {rate}"
        );
    }

    #[test]
    fn force_clip_gain_down_requires_all_guards_to_pass() {
        // Above clip threshold, valid gain index, no freeze, no hold => force down.
        assert!(should_force_clip_gain_down(0.10, 3, 0, 0));

        // Any guard failing must block the forced step.
        assert!(!should_force_clip_gain_down(0.01, 3, 0, 0));
        assert!(!should_force_clip_gain_down(0.10, 0, 0, 0));
        assert!(!should_force_clip_gain_down(0.10, 3, 1, 0));
        assert!(!should_force_clip_gain_down(0.10, 3, 0, 1));
    }

    // ── Offset-tuning: phasor rotation ───────────────────────────────────────

    /// OFFSET_TUNING_HZ is exactly Fs/4 = 512 000 Hz.
    /// The per-sample rotation angle is therefore π/2, making the rotation
    /// e^(+jπ/2) = (0, 1) = +j.  After 4 samples the phasor completes one
    /// full cycle, so sample n gets multiplied by j^n.
    #[test]
    fn offset_tuning_hz_equals_fs_over_4() {
        assert_eq!(OFFSET_TUNING_HZ, (INPUT_RATE / 4) as i32);
    }

    #[test]
    fn offset_tuning_rotation_is_quarter_cycle() {
        // angle = 2π × 512000 / 2048000 = π/2
        let angle = 2.0 * std::f32::consts::PI * OFFSET_TUNING_HZ as f32 / INPUT_RATE as f32;
        let expected_angle = std::f32::consts::FRAC_PI_2;
        assert!((angle - expected_angle).abs() < 1e-5, "angle={angle}");

        let rotation = Complex32::new(angle.cos(), angle.sin());
        // cos(π/2) ≈ 0, sin(π/2) ≈ 1  →  rotation ≈ j
        assert!(rotation.re.abs() < 1e-5);
        assert!((rotation.im - 1.0).abs() < 1e-5);
    }

    #[test]
    fn offset_tuning_phasor_four_step_cycle() {
        // Starting from (1, 0), multiplying by j each step completes
        // the cycle (1,0) → (0,1) → (-1,0) → (0,-1) → (1,0).
        let angle = 2.0 * std::f32::consts::PI * OFFSET_TUNING_HZ as f32 / INPUT_RATE as f32;
        let rotation = Complex32::new(angle.cos(), angle.sin());
        let mut phasor = Complex32::new(1.0, 0.0);
        let tol = 1e-5_f32;

        phasor *= rotation;
        assert!(
            phasor.re.abs() < tol && (phasor.im - 1.0).abs() < tol,
            "step 1: {phasor:?}"
        );

        phasor *= rotation;
        assert!(
            (phasor.re + 1.0).abs() < tol && phasor.im.abs() < tol,
            "step 2: {phasor:?}"
        );

        phasor *= rotation;
        assert!(
            phasor.re.abs() < tol && (phasor.im + 1.0).abs() < tol,
            "step 3: {phasor:?}"
        );

        phasor *= rotation;
        assert!(
            (phasor.re - 1.0).abs() < tol && phasor.im.abs() < tol,
            "step 4: {phasor:?}"
        );
    }

    #[test]
    fn offset_tuning_renormalisation_keeps_unit_magnitude() {
        // After many multiply steps, floating-point errors accumulate.
        // The per-buffer renormalisation step should keep |phasor| ≈ 1.
        let angle = 2.0 * std::f32::consts::PI * OFFSET_TUNING_HZ as f32 / INPUT_RATE as f32;
        let rotation = Complex32::new(angle.cos(), angle.sin());
        let mut phasor = Complex32::new(1.0, 0.0);

        // Simulate 100 full buffers of 4096 sample-pairs each without renorm.
        for _ in 0..(100 * 4096) {
            phasor *= rotation;
        }
        // Magnitude drifts slightly; renormalise and check it returns to 1.
        let norm = (phasor.re * phasor.re + phasor.im * phasor.im).sqrt();
        if norm > 0.0 {
            phasor /= norm;
        }
        let mag = (phasor.re * phasor.re + phasor.im * phasor.im).sqrt();
        assert!((mag - 1.0).abs() < 1e-6, "magnitude after renorm: {mag}");
    }
}
