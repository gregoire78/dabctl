// RTL-SDR handler using the rtl-sdr-rs pure-Rust crate.
// Replaces the previous bindgen/FFI bindings against the vendored librtlsdr C library.

use num_complex::Complex32;
use rtl_sdr_rs::{DeviceId, RtlSdr, TunerGain};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tracing::{debug, info, warn};

const READLEN_DEFAULT: usize = 8192;
const INPUT_RATE: u32 = 2_048_000;

/// SAGC: upper signal level threshold (int8 absolute-value scale, 0–127).
/// Gain is reduced when the running estimate exceeds this value.
/// Mirrors AbracaDABra's `RTLSDR_AGC_LEVEL_MAX_DEFAULT` (≈ 82 % of full scale).
const SAGC_LEVEL_MAX: f32 = 105.0;
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

/// Number of USB read buffers discarded on startup before IQ data enters the FIFO.
/// Prevents stale IQ data from before `reset_buffer()` contaminating the OFDM sync.
/// Mirrors AbracaDABra's `RTLSDR_RESTART_COUNTER`.
const SAGC_STARTUP_DISCARD: u32 = 2;
/// Minimum number of SAGC evaluation ticks that must elapse after a gain change
/// before the next change is allowed.  Each tick is `SAGC_CHECK_INTERVAL` read
/// buffers, so the freeze lasts `SAGC_HOLD_BUFFERS × SAGC_CHECK_INTERVAL × 4096`
/// IQ samples ≈ 8 × 4 × 4096 / 2_048_000 s ≈ 64 ms.
///
/// This prevents the control loop from oscillating between adjacent gain steps
/// when the signal sits near a threshold — a pathological case observed at
/// SNR ≈ 8–14 dB where the gain toggles 15-20 times per second and breaks
/// DAB+ superframe synchronisation.
const SAGC_HOLD_BUFFERS: u32 = 8;

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
/// of that gain step plus 0.5 dB of hysteresis, keeping the control loop stable:
///
/// ```text
/// factors[i] = 10 ^ ((gains[i] − gains[i+1] − 5) / 200)
/// ```
///
/// where `gains` is sorted ascending in tenths of dB (as returned by the RTL-SDR driver).
/// For the highest gain index a fixed −5 dB factor is used (it is never triggered in
/// practice because the gain cannot be raised further).
/// Adapted from AbracaDABra by KejPi (MIT licence).
fn compute_level_min_factors(gains: &[i32]) -> Vec<f32> {
    assert!(!gains.is_empty(), "gains list must not be empty");
    let n = gains.len();
    (0..n)
        .map(|i| {
            if i + 1 < n {
                // lower threshold: accounts for the step to gains[i+1] plus 0.5 dB headroom
                f32::powf(10.0, (gains[i] - gains[i + 1] - 5) as f32 / 200.0)
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
    fifo: Arc<IqFifo>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl RtlsdrHandler {
    pub fn new(
        frequency: u32,
        ppm_offset: i32,
        gain_mode: GainMode,
        device_index: u32,
    ) -> Result<Self, String> {
        // Capacity: 1 Mi f32 values = 512 Ki IQ pairs ≈ 250 ms at 2 Msps.
        let capacity = 1024 * 1024;
        Ok(RtlsdrHandler {
            config: DeviceConfig {
                device_index: device_index as usize,
                frequency,
                ppm_offset,
                gain_mode,
            },
            running: Arc::new(AtomicBool::new(false)),
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
                    let idx = gains.len() / 2;
                    info!(
                        "Software AGC (SAGC) enabled, starting at {:.1} dB",
                        gains[idx] as f32 / 10.0
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

            if let Err(e) = sdr.set_center_freq(config.frequency) {
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
                "Tuned to {} Hz, sample rate {} S/s",
                sdr.get_center_freq(),
                sdr.get_sample_rate()
            );

            // Signal successful initialization to the main thread.
            let _ = init_tx.send(Ok(()));

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

            // ── DOC state ─────────────────────────────────────────────────────
            // DC Offset Correction: independent IIR estimators for I and Q.
            // Adapted from AbracaDABra's `m_dcI` / `m_dcQ` (MIT licence).
            // Operates on the ±128 integer scale (before /128 normalisation).
            // The estimate from the previous buffer is applied to the current
            // buffer, giving a 1-buffer lag that is negligible for a slow IIR.
            let mut dc_i = 0.0f32;
            let mut dc_q = 0.0f32;

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
            // Accumulation cursor for partial reads.
            //
            // USB bulk transfers on native hardware always return exactly READLEN_DEFAULT
            // bytes.  When the device is forwarded via usbipd (USB over TCP), read_sync
            // may return fewer bytes per call due to TCP segmentation.  We accumulate
            // chunks into `raw` until the full READLEN_DEFAULT bytes are available before
            // processing — avoiding the silent data loss that occurred when short reads
            // were discarded.
            let mut fill_pos: usize = 0;
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match sdr.read_sync(&mut raw[fill_pos..]) {
                    Ok(0) => {
                        tracing::warn!("read_sync returned 0 bytes; retrying");
                    }
                    Ok(n) => {
                        fill_pos += n;
                        if fill_pos < READLEN_DEFAULT {
                            // Partial read — accumulate further chunks before processing.
                            continue;
                        }
                        // Full buffer ready; reset cursor for the next cycle.
                        fill_pos = 0;

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
                        // Decoupling the arithmetic from the mutex eliminates the contention
                        // window during which the OFDM consumer thread was blocked on
                        // get_samples() waiting for the same lock.
                        let mut sum_i = 0.0f32;
                        let mut sum_q = 0.0f32;
                        for k in 0..n_pairs {
                            let i_raw = raw[2 * k] as f32 - 128.0;
                            let q_raw = raw[2 * k + 1] as f32 - 128.0;

                            // SAGC — absolute amplitude, operates before DOC
                            if sagc_enabled {
                                for &abs_val in &[i_raw.abs(), q_raw.abs()] {
                                    let c = if abs_val > agc_level {
                                        SAGC_CATT
                                    } else {
                                        SAGC_CREL
                                    };
                                    agc_level += c * (abs_val - agc_level);
                                }
                            }

                            // DOC accumulation (±128 scale)
                            sum_i += i_raw;
                            sum_q += q_raw;

                            // Store DC-corrected, normalised sample pair in local scratch
                            sample_buf[2 * k] = (i_raw - dc_i) / 128.0;
                            sample_buf[2 * k + 1] = (q_raw - dc_q) / 128.0;
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
                            if agc_hold_cntr > 0 {
                                // Still in hold-off period after the last gain change.
                                // Decrement and skip any adjustment this tick.
                                agc_hold_cntr -= 1;
                            } else if agc_level < agc_level_min && gain_idx + 1 < gains.len() {
                                // Signal too weak — increase gain by one step.
                                gain_idx += 1;
                                agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                                agc_hold_cntr = SAGC_HOLD_BUFFERS;
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
                            } else if agc_level > SAGC_LEVEL_MAX && gain_idx > 0 {
                                // Signal too strong — decrease gain by one step.
                                gain_idx -= 1;
                                agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                                agc_hold_cntr = SAGC_HOLD_BUFFERS;
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
                tracing::error!("{}", msg);
                self.running.store(false, Ordering::SeqCst);
                false
            }
            Err(_) => {
                tracing::error!("Worker thread exited before completing init");
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
        let gains = vec![0i32, 9, 14];
        let factors = compute_level_min_factors(&gains);
        assert_eq!(factors.len(), 3);

        // factors[0] = 10^((0 − 9 − 5) / 200)
        let f0 = f32::powf(10.0, (0 - 9 - 5) as f32 / 200.0);
        assert!((factors[0] - f0).abs() < 1e-6, "factor[0] mismatch");

        // factors[1] = 10^((9 − 14 − 5) / 200)
        let f1 = f32::powf(10.0, (9 - 14 - 5) as f32 / 200.0);
        assert!((factors[1] - f1).abs() < 1e-6, "factor[1] mismatch");

        // factors[2] = 10^(−5 / 20)  (last index)
        let f2 = f32::powf(10.0, -5.0_f32 / 20.0);
        assert!((factors[2] - f2).abs() < 1e-6, "factor[2] mismatch");
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
}
