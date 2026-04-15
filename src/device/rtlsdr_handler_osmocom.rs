// RTL-SDR handler using the osmocom librtlsdr C library (FFI via bindgen).
// Provides the same public API as rtlsdr_handler_rs so the two backends are
// interchangeable via Cargo features.

#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    clippy::all
)]
mod rtlsdr_sys {
    mod bindings {
        include!(concat!(env!("OUT_DIR"), "/rtlsdr.rs"));
    }
    pub use bindings::*;
}

use num_complex::Complex32;
use std::collections::VecDeque;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tracing::{debug, info, warn};

const READLEN_DEFAULT: usize = 8192;
const INPUT_RATE: u32 = 2_048_000;
/// DAB Band III channel bandwidth (ETSI EN 300 401 §2.2): 1.536 MHz.
/// Setting the tuner IF filter to this value reduces adjacent-channel noise
/// and improves SNR on the 8-bit RTL2832U ADC.
const DAB_CHANNEL_BW: u32 = 1_536_000;

const SAGC_LEVEL_MAX: f32 = 95.0;
const SAGC_CATT: f32 = 0.1;
const SAGC_CREL: f32 = 0.00005;
const SAGC_CHECK_INTERVAL: u32 = 4;
const DOC_C: f32 = 0.05;
const SAGC_STARTUP_DISCARD: u32 = 2;
const SAGC_HOLD_BUFFERS: u32 = 32;
const SAGC_CONFIRM_COUNT: u32 = 3;
const SAGC_HUNT_THRESHOLD: u32 = 3;
const SAGC_HUNT_FREEZE_BASE: u32 = 80;
const SAGC_HUNT_GRACE_TICKS: u32 = 128;
const SAGC_HUNT_FREEZE_MAX: u32 = 300;
const SAGC_HUNT_TIMEOUT_TICKS: u32 = 750;
const SAGC_CLIP_THRESHOLD: f32 = 120.0;
const SAGC_CLIP_RATE_MAX: f32 = 0.05;
const SAGC_SILENCE_FLOOR: f32 = 3.0;
const SAGC_SILENCE_TICKS: u32 = 10;
const SAGC_HUNT_RESET_TICKS: u32 = 500;
const SAGC_TELEMETRY_TICKS: u32 = 125;

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

#[allow(dead_code)]
fn select_gain_from_percent(gain_pct: i16, gains: &[i32]) -> i32 {
    assert!(!gains.is_empty(), "gains list must not be empty");
    let index = (gain_pct.clamp(0, 100) as usize * (gains.len() - 1)) / 100;
    gains[index]
}

fn compute_level_min_factors(gains: &[i32]) -> Vec<f32> {
    assert!(!gains.is_empty(), "gains list must not be empty");
    let n = gains.len();
    (0..n)
        .map(|i| {
            if i + 1 < n {
                let step = gains[i + 1] - gains[i];
                let hysteresis = step.max(20);
                f32::powf(10.0, (gains[i] - gains[i + 1] - hysteresis) as f32 / 200.0)
            } else {
                f32::powf(10.0, -5.0_f32 / 20.0)
            }
        })
        .collect()
}

/// Gain control mode for the RTL-SDR tuner.
#[derive(Clone, Debug, PartialEq)]
pub enum GainMode {
    /// Software AGC (SAGC): gain is stepped by the application.
    Software,
    /// Hardware AGC: gain delegated to the RTL-SDR chip.
    Hardware,
    /// Fixed gain as a percentage of the tuner's range (0–100).
    Manual(i16),
}

#[derive(Clone)]
struct DeviceConfig {
    device_index: u32,
    frequency: u32,
    ppm_offset: i32,
    gain_mode: GainMode,
}

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
    current_gain_tenths: Arc<AtomicI32>,
    fifo: Arc<IqFifo>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

/// Helper: set tuner to manual gain mode and apply `gain_tenths`.
/// Returns true on success.
unsafe fn ffi_set_manual_gain(dev: *mut rtlsdr_sys::rtlsdr_dev_t, gain_tenths: i32) -> bool {
    rtlsdr_sys::rtlsdr_set_agc_mode(dev, 0);
    rtlsdr_sys::rtlsdr_set_tuner_gain_mode(dev, 1);
    rtlsdr_sys::rtlsdr_set_tuner_gain(dev, gain_tenths) == 0
}

/// Helper: enable hardware AGC.
unsafe fn ffi_set_hardware_agc(dev: *mut rtlsdr_sys::rtlsdr_dev_t) {
    rtlsdr_sys::rtlsdr_set_tuner_gain_mode(dev, 0);
    rtlsdr_sys::rtlsdr_set_agc_mode(dev, 1);
}

impl RtlsdrHandler {
    pub fn new(
        frequency: u32,
        ppm_offset: i32,
        gain_mode: GainMode,
        device_index: u32,
    ) -> Result<Self, String> {
        let capacity = 1024 * 1024;
        Ok(RtlsdrHandler {
            config: DeviceConfig {
                device_index,
                frequency,
                ppm_offset,
                gain_mode,
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
            self.fifo.buf.lock().unwrap().clear();
        }

        let config = self.config.clone();
        let running = self.running.clone();
        let fifo = self.fifo.clone();
        let current_gain_tenths = self.current_gain_tenths.clone();

        let (init_tx, init_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

        self.running.store(true, Ordering::SeqCst);

        self.worker_handle = Some(thread::spawn(move || {
            // ── Open device ──────────────────────────────────────────────────
            let mut dev: *mut rtlsdr_sys::rtlsdr_dev_t = std::ptr::null_mut();
            let r = unsafe { rtlsdr_sys::rtlsdr_open(&mut dev, config.device_index) };
            if r < 0 || dev.is_null() {
                let _ = init_tx.send(Err(format!(
                    "Failed to open RTL-SDR device {}: error {}",
                    config.device_index, r
                )));
                running.store(false, Ordering::SeqCst);
                return;
            }

            // ── Read gain table ──────────────────────────────────────────────
            let gains_count =
                unsafe { rtlsdr_sys::rtlsdr_get_tuner_gains(dev, std::ptr::null_mut()) };
            if gains_count <= 0 {
                let _ = init_tx.send(Err("Failed to read tuner gain list".to_string()));
                unsafe { rtlsdr_sys::rtlsdr_close(dev) };
                running.store(false, Ordering::SeqCst);
                return;
            }
            let mut gains = vec![0i32; gains_count as usize];
            unsafe { rtlsdr_sys::rtlsdr_get_tuner_gains(dev, gains.as_mut_ptr()) };

            let gains_str: Vec<String> = gains
                .iter()
                .map(|g| format!("{}.{}", g / 10, g % 10))
                .collect();
            debug!(
                "Supported gain values ({}): {}",
                gains.len(),
                gains_str.join(" ")
            );

            // ── Resolve initial gain / mode ──────────────────────────────────
            let (initial_gain_idx, sagc_enabled, hardware_agc) = match config.gain_mode {
                GainMode::Software => {
                    let idx = (gains.len() - 1) * 50 / 100;
                    info!(
                        "Software AGC (SAGC) enabled, starting at {:.1} dB (50%)",
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

            // ── Configure device ─────────────────────────────────────────────
            let r = unsafe { rtlsdr_sys::rtlsdr_set_sample_rate(dev, INPUT_RATE) };
            if r < 0 {
                let _ = init_tx.send(Err(format!("set_sample_rate failed: {}", r)));
                unsafe { rtlsdr_sys::rtlsdr_close(dev) };
                running.store(false, Ordering::SeqCst);
                return;
            }

            // Narrow the tuner IF filter to the DAB Band III channel bandwidth
            // (1.536 MHz) to reject adjacent signals and maximise SNR on the
            // 8-bit RTL2832U ADC.  Not all tuner chips honour this call, so a
            // failure is non-fatal.
            let r = unsafe { rtlsdr_sys::rtlsdr_set_tuner_bandwidth(dev, DAB_CHANNEL_BW) };
            if r < 0 {
                warn!("set_tuner_bandwidth failed (non-fatal): {}", r);
            }

            if config.ppm_offset != 0 {
                let r = unsafe { rtlsdr_sys::rtlsdr_set_freq_correction(dev, config.ppm_offset) };
                if r < 0 {
                    warn!("set_freq_correction failed: {}", r);
                }
            }

            let r = unsafe { rtlsdr_sys::rtlsdr_set_center_freq(dev, config.frequency) };
            if r < 0 {
                let _ = init_tx.send(Err(format!("set_center_freq failed: {}", r)));
                unsafe { rtlsdr_sys::rtlsdr_close(dev) };
                running.store(false, Ordering::SeqCst);
                return;
            }

            if hardware_agc {
                unsafe { ffi_set_hardware_agc(dev) };
            } else {
                let r = unsafe { ffi_set_manual_gain(dev, gains[initial_gain_idx]) };
                if !r {
                    warn!("set_tuner_gain failed for initial gain");
                }
            }

            let r = unsafe { rtlsdr_sys::rtlsdr_reset_buffer(dev) };
            if r < 0 {
                let _ = init_tx.send(Err(format!("reset_buffer failed: {}", r)));
                unsafe { rtlsdr_sys::rtlsdr_close(dev) };
                running.store(false, Ordering::SeqCst);
                return;
            }

            let center = unsafe { rtlsdr_sys::rtlsdr_get_center_freq(dev) };
            let rate = unsafe { rtlsdr_sys::rtlsdr_get_sample_rate(dev) };
            info!("Tuned to {} Hz, sample rate {} S/s", center, rate);

            // Signal successful init to main thread.
            let _ = init_tx.send(Ok(()));

            if hardware_agc {
                current_gain_tenths.store(-1, Ordering::Relaxed);
            } else {
                current_gain_tenths.store(gains[initial_gain_idx], Ordering::Relaxed);
            }

            // ── SAGC state ───────────────────────────────────────────────────
            let level_min_factors = compute_level_min_factors(&gains);
            let mut gain_idx = initial_gain_idx;
            let mut agc_level = 0.0f32;
            let mut agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
            let mut agc_read_cntr: u32 = 0;
            let mut agc_hold_cntr: u32 = 0;
            let mut agc_up_confirm: u32 = 0;
            let mut agc_down_confirm: u32 = 0;
            let mut clip_count: u32 = 0;
            let mut last_gain_dir: i32 = 0;
            let mut hunt_count: u32 = 0;
            let mut hunt_freeze: u32 = 0;
            let mut hunt_freeze_ticks: u32 = SAGC_HUNT_FREEZE_BASE;
            let mut hunt_grace_cntr: u32 = SAGC_HUNT_GRACE_TICKS;
            let mut sagc_silence_cntr: u32 = 0;
            let mut hunt_stable_cntr: u32 = 0;
            let mut hunt_last_reversal_ticks: u32 = 0;
            let mut sagc_telemetry_cntr: u32 = 0;

            // ── DOC state ────────────────────────────────────────────────────
            let mut dc_i = 0.0f32;
            let mut dc_q = 0.0f32;

            // ── Startup discard ──────────────────────────────────────────────
            let mut startup_discard = SAGC_STARTUP_DISCARD;

            let mut sample_buf = vec![0.0f32; READLEN_DEFAULT];
            let mut raw = [0u8; READLEN_DEFAULT];

            // ── Read loop ────────────────────────────────────────────────────
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }

                let mut n_read: c_int = 0;
                let r = unsafe {
                    rtlsdr_sys::rtlsdr_read_sync(
                        dev,
                        raw.as_mut_ptr() as *mut _,
                        READLEN_DEFAULT as c_int,
                        &mut n_read,
                    )
                };

                if r < 0 {
                    tracing::error!("rtlsdr_read_sync error: {}", r);
                    break;
                }
                if n_read == 0 {
                    tracing::warn!("rtlsdr_read_sync returned 0 bytes; retrying");
                    continue;
                }

                if startup_discard > 0 {
                    startup_discard -= 1;
                    continue;
                }

                let n_pairs = READLEN_DEFAULT / 2;

                // ── Phase 1: SAGC + DOC accumulation ────────────────────────
                let mut sum_i = 0.0f32;
                let mut sum_q = 0.0f32;
                for k in 0..n_pairs {
                    let i_raw = raw[2 * k] as f32 - 128.0;
                    let q_raw = raw[2 * k + 1] as f32 - 128.0;

                    sum_i += i_raw;
                    sum_q += q_raw;

                    if sagc_enabled {
                        for &abs_val in &[i_raw.abs(), q_raw.abs()] {
                            let c = if abs_val > agc_level {
                                SAGC_CATT
                            } else {
                                SAGC_CREL
                            };
                            agc_level += c * (abs_val - agc_level);
                            if abs_val >= SAGC_CLIP_THRESHOLD {
                                clip_count += 1;
                            }
                        }
                    }

                    sample_buf[2 * k] = (i_raw - dc_i) / 128.0;
                    sample_buf[2 * k + 1] = (q_raw - dc_q) / 128.0;
                }

                // ── Phase 2: push to FIFO ────────────────────────────────────
                {
                    let mut guard = fifo.buf.lock().unwrap();
                    guard.extend(sample_buf[..2 * n_pairs].iter().copied());
                }
                fifo.data_ready.notify_all();

                dc_i += DOC_C * (sum_i / n_pairs as f32 - dc_i);
                dc_q += DOC_C * (sum_q / n_pairs as f32 - dc_q);

                // ── SAGC gain evaluation ─────────────────────────────────────
                agc_read_cntr = agc_read_cntr.wrapping_add(1);
                if sagc_enabled && agc_read_cntr.is_multiple_of(SAGC_CHECK_INTERVAL) {
                    hunt_grace_cntr = hunt_grace_cntr.saturating_sub(1);

                    hunt_last_reversal_ticks = hunt_last_reversal_ticks.saturating_add(1);
                    if hunt_last_reversal_ticks >= SAGC_HUNT_TIMEOUT_TICKS && hunt_count > 0 {
                        debug!(
                            "SAGC: hunt count reset after {} ticks of inactivity",
                            hunt_last_reversal_ticks
                        );
                        hunt_count = 0;
                        hunt_last_reversal_ticks = 0;
                    }

                    let clip_rate = compute_clip_rate(clip_count, SAGC_CHECK_INTERVAL, n_pairs);
                    clip_count = 0;

                    sagc_silence_cntr = update_sagc_silence_counter(agc_level, sagc_silence_cntr);

                    if should_force_clip_gain_down(clip_rate, gain_idx, hunt_freeze, agc_hold_cntr)
                    {
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
                        if unsafe { !ffi_set_manual_gain(dev, gains[gain_idx]) } {
                            warn!("SAGC: set_tuner_gain failed");
                        } else {
                            debug!(
                                "SAGC: clip ↓ {:.1} dB (clip {:.1}%)",
                                gains[gain_idx] as f32 / 10.0,
                                clip_rate * 100.0,
                            );
                        }
                    } else if should_reset_on_silence(sagc_silence_cntr) {
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
                        hunt_freeze -= 1;
                        hunt_stable_cntr = 0;
                        agc_up_confirm = 0;
                        agc_down_confirm = 0;
                    } else if agc_hold_cntr > 0 {
                        agc_hold_cntr -= 1;
                        hunt_stable_cntr = 0;
                        agc_up_confirm = 0;
                        agc_down_confirm = 0;
                    } else if agc_level < agc_level_min && gain_idx + 1 < gains.len() {
                        hunt_stable_cntr = 0;
                        agc_down_confirm = 0;
                        agc_up_confirm += 1;
                        if agc_up_confirm >= SAGC_CONFIRM_COUNT {
                            agc_up_confirm = 0;
                            gain_idx += 1;
                            agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                            agc_hold_cntr = SAGC_HOLD_BUFFERS;
                            if last_gain_dir == -1 && hunt_grace_cntr == 0 {
                                hunt_last_reversal_ticks = 0;
                                hunt_count += 1;
                                if hunt_count >= SAGC_HUNT_THRESHOLD {
                                    gain_idx -= 1;
                                    agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
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
                                    if unsafe { !ffi_set_manual_gain(dev, gains[gain_idx]) } {
                                        warn!("SAGC: set_tuner_gain failed");
                                    }
                                } else {
                                    last_gain_dir = 1;
                                    if unsafe { !ffi_set_manual_gain(dev, gains[gain_idx]) } {
                                        warn!("SAGC: set_tuner_gain failed");
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
                                if unsafe { !ffi_set_manual_gain(dev, gains[gain_idx]) } {
                                    warn!("SAGC: set_tuner_gain failed");
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
                        hunt_stable_cntr = 0;
                        agc_up_confirm = 0;
                        agc_down_confirm += 1;
                        if agc_down_confirm >= SAGC_CONFIRM_COUNT {
                            agc_down_confirm = 0;
                            gain_idx -= 1;
                            agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                            agc_hold_cntr = SAGC_HOLD_BUFFERS;
                            if last_gain_dir == 1 && hunt_grace_cntr == 0 {
                                hunt_last_reversal_ticks = 0;
                                hunt_count += 1;
                                if hunt_count >= SAGC_HUNT_THRESHOLD {
                                    gain_idx += 1;
                                    agc_level_min = level_min_factors[gain_idx] * SAGC_LEVEL_MAX;
                                    agc_hold_cntr = 0;
                                    hunt_freeze = hunt_freeze_ticks;
                                    hunt_freeze_ticks = hunt_freeze_ticks
                                        .saturating_mul(2)
                                        .min(SAGC_HUNT_FREEZE_MAX);
                                    hunt_count = 0;
                                    last_gain_dir = 1;
                                    warn!(
                                        "SAGC: hunting ↓↑↓ between {:.1} and {:.1} dB, \
                                         locking {:.1} dB for {} ticks",
                                        gains[gain_idx - 1] as f32 / 10.0,
                                        gains[gain_idx] as f32 / 10.0,
                                        gains[gain_idx] as f32 / 10.0,
                                        hunt_freeze,
                                    );
                                    if unsafe { !ffi_set_manual_gain(dev, gains[gain_idx]) } {
                                        warn!("SAGC: set_tuner_gain failed");
                                    }
                                } else {
                                    last_gain_dir = -1;
                                    if unsafe { !ffi_set_manual_gain(dev, gains[gain_idx]) } {
                                        warn!("SAGC: set_tuner_gain failed");
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
                                if unsafe { !ffi_set_manual_gain(dev, gains[gain_idx]) } {
                                    warn!("SAGC: set_tuner_gain failed");
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
                        agc_up_confirm = 0;
                        agc_down_confirm = 0;
                        let (stable_next, freeze_next) =
                            apply_hunt_backoff_reset_if_stable(hunt_stable_cntr, hunt_freeze_ticks);
                        if freeze_next != hunt_freeze_ticks {
                            debug!("SAGC: sustained stability → hunt backoff reset");
                        }
                        hunt_stable_cntr = stable_next;
                        hunt_freeze_ticks = freeze_next;
                    }

                    if sagc_enabled {
                        current_gain_tenths.store(gains[gain_idx], Ordering::Relaxed);
                        sagc_telemetry_cntr = sagc_telemetry_cntr.wrapping_add(1);
                        if sagc_telemetry_cntr.is_multiple_of(SAGC_TELEMETRY_TICKS) {
                            info!(
                                "SAGC: gain={:.1}dB level={:.1}/{:.1}-{:.1} clip={:.2}% hold={} freeze={} hunt={} silence={}",
                                gains[gain_idx] as f32 / 10.0,
                                agc_level,
                                agc_level_min,
                                SAGC_LEVEL_MAX,
                                clip_rate * 100.0,
                                agc_hold_cntr,
                                hunt_freeze,
                                hunt_count,
                                sagc_silence_cntr,
                            );
                        }
                    }
                }
            }

            running.store(false, Ordering::SeqCst);
            fifo.data_ready.notify_all();
            unsafe { rtlsdr_sys::rtlsdr_close(dev) };
        }));

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

    pub fn reader_running(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    pub fn current_gain_tenths_db(&self) -> i32 {
        self.current_gain_tenths.load(Ordering::Relaxed)
    }

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

    pub fn get_samples(&self, v: &mut [Complex32]) -> usize {
        let needed = 2 * v.len();
        let mut guard = self.fifo.buf.lock().unwrap();
        loop {
            if guard.len() >= needed {
                break;
            }
            if !self.running.load(Ordering::Relaxed) {
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
