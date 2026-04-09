// RTL-SDR handler using the rtl-sdr-rs pure-Rust crate.
// Replaces the previous bindgen/FFI bindings against the vendored librtlsdr C library.

use num_complex::Complex32;
use rtl_sdr_rs::{DeviceId, RtlSdr, TunerGain};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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

/// Pre-computed conversion table: `table[i] = (i as f32 − 128.0) / 128.0`.
fn build_conv_table() -> [f32; 256] {
    let mut table = [0.0f32; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        *entry = (i as f32 - 128.0) / 128.0;
    }
    table
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

pub struct RtlsdrHandler {
    config: DeviceConfig,
    running: Arc<AtomicBool>,
    i_buffer: Arc<Mutex<VecDeque<u8>>>,
    worker_handle: Option<thread::JoinHandle<()>>,
    conv_table: [f32; 256],
}

impl RtlsdrHandler {
    pub fn new(
        frequency: u32,
        ppm_offset: i32,
        gain_mode: GainMode,
        device_index: u32,
    ) -> Result<Self, String> {
        Ok(RtlsdrHandler {
            config: DeviceConfig {
                device_index: device_index as usize,
                frequency,
                ppm_offset,
                gain_mode,
            },
            running: Arc::new(AtomicBool::new(false)),
            i_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(4 * 1024 * 1024))),
            worker_handle: None,
            conv_table: build_conv_table(),
        })
    }

    pub fn restart_reader(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }

        {
            let mut buf = self.i_buffer.lock().unwrap();
            buf.clear();
        }

        let config = self.config.clone();
        let running = self.running.clone();
        let buffer = self.i_buffer.clone();

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
            let mut agc_level = 0.0f32;
            let mut agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
            // Counter: gain is re-evaluated every SAGC_CHECK_INTERVAL read buffers.
            let mut agc_read_cntr: u32 = 0;

            let mut raw = [0u8; READLEN_DEFAULT];
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match sdr.read_sync(&mut raw) {
                    Ok(n) if n == READLEN_DEFAULT => {
                        // Update level estimate: fast attack, slow release.
                        // Each raw byte is a uint8 IQ sample; `|byte − 128|` maps to
                        // the absolute amplitude on a 0–127 scale.
                        if sagc_enabled {
                            for b in &raw[..n] {
                                let abs_sample = ((*b as i32 - 128).abs()) as f32;
                                let c = if abs_sample > agc_level {
                                    SAGC_CATT
                                } else {
                                    SAGC_CREL
                                };
                                agc_level += c * (abs_sample - agc_level);
                            }
                        }

                        if let Ok(mut guard) = buffer.lock() {
                            for b in &raw[..n] {
                                guard.push_back(*b);
                            }
                        }

                        // Evaluate thresholds every SAGC_CHECK_INTERVAL buffers.
                        agc_read_cntr = agc_read_cntr.wrapping_add(1);
                        if sagc_enabled && agc_read_cntr.is_multiple_of(SAGC_CHECK_INTERVAL) {
                            if agc_level < agc_level_min && gain_idx + 1 < gains.len() {
                                // Signal too weak — increase gain by one step.
                                gain_idx += 1;
                                agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
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
                    Ok(n) => {
                        tracing::debug!("Short read: {} bytes (discarded)", n);
                    }
                    Err(e) => {
                        tracing::error!("read_sync error: {}", e);
                        break;
                    }
                }
            }

            running.store(false, Ordering::SeqCst);
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

    pub fn get_samples(&self, v: &mut [Complex32]) -> usize {
        let size = v.len();
        let mut temp = vec![0u8; 2 * size];
        let amount = {
            let mut buf = self.i_buffer.lock().unwrap();
            let available = buf.len().min(2 * size);
            for slot in temp.iter_mut().take(available) {
                *slot = buf.pop_front().unwrap_or(0);
            }
            available
        };
        let sample_count = amount / 2;
        for i in 0..sample_count {
            v[i] = Complex32::new(
                self.conv_table[temp[2 * i] as usize],
                self.conv_table[temp[2 * i + 1] as usize],
            );
        }
        sample_count
    }

    pub fn samples(&self) -> usize {
        let buf = self.i_buffer.lock().unwrap();
        buf.len() / 2
    }

    pub fn reset_buffer(&self) {
        let mut buf = self.i_buffer.lock().unwrap();
        buf.clear();
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
        let table = build_conv_table();
        assert_eq!(table[128], 0.0);
    }

    #[test]
    fn conv_table_maps_0_to_negative_one() {
        let table = build_conv_table();
        assert!((table[0] - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn conv_table_maps_255_to_near_positive_one() {
        let table = build_conv_table();
        assert!((table[255] - (127.0 / 128.0)).abs() < 1e-6);
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
}
