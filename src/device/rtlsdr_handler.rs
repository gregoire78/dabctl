// RTL-SDR handler using the rtl-sdr-rs pure-Rust crate.
// Replaces the previous bindgen/FFI bindings against the vendored librtlsdr C library.

use num_complex::Complex32;
use rtl_sdr_rs::{DeviceId, RtlSdr, TunerGain};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::{info, warn};

const READLEN_DEFAULT: usize = 8192;
const INPUT_RATE: u32 = 2_048_000;

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

/// Configuration passed to the worker thread when spawning.
/// All fields are `Clone + Send`, avoiding the `!Send` constraint on `RtlSdr`.
#[derive(Clone)]
struct DeviceConfig {
    device_index: usize,
    frequency: u32,
    ppm_offset: i32,
    autogain: bool,
    /// Selected gain in tenths of dB; only used when `autogain` is false.
    gain_value: i32,
    /// Full ordered gain table (tenths of dB) used by the software AGC.
    gains: Vec<i32>,
    /// Index of the initially selected gain in `gains`.
    gain_index: usize,
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
        gain: i16,
        autogain: bool,
        device_index: u32,
    ) -> Result<Self, String> {
        // Open briefly to validate the device and read the available gain list.
        let mut sdr = RtlSdr::open(DeviceId::Index(device_index as usize))
            .map_err(|e| format!("Opening RTL-SDR device {} failed: {}", device_index, e))?;

        let gains = sdr
            .get_tuner_gains()
            .map_err(|e| format!("Reading tuner gains failed: {}", e))?;

        let gains_str: Vec<String> = gains
            .iter()
            .map(|g| format!("{}.{}", g / 10, g % 10))
            .collect();
        info!(
            "Supported gain values ({}): {}",
            gains.len(),
            gains_str.join(" ")
        );

        let (gain_value, gain_index) = if autogain {
            info!("Auto gain enabled (hardware AGC)");
            (0, 0)
        } else {
            let index = (gain.clamp(0, 100) as usize * (gains.len() - 1)) / 100;
            let selected = gains[index];
            info!(
                "Manual gain set to {}.{} dB",
                selected / 10,
                selected.abs() % 10
            );
            (selected, index)
        };

        // Release USB before the worker thread re-opens the device.
        let _ = sdr.close();

        Ok(RtlsdrHandler {
            config: DeviceConfig {
                device_index: device_index as usize,
                frequency,
                ppm_offset,
                autogain,
                gain_value,
                gains,
                gain_index,
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

        self.running.store(true, Ordering::SeqCst);

        self.worker_handle = Some(thread::spawn(move || {
            let mut sdr = match RtlSdr::open(DeviceId::Index(config.device_index)) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("Failed to open RTL-SDR in worker thread: {}", e);
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            if let Err(e) = sdr.set_sample_rate(INPUT_RATE) {
                tracing::error!("set_sample_rate failed: {}", e);
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

            let tuner_gain = if config.autogain {
                TunerGain::Auto
            } else {
                TunerGain::Manual(config.gain_value)
            };
            if let Err(e) = sdr.set_tuner_gain(tuner_gain) {
                warn!("set_tuner_gain failed: {}", e);
            }

            if let Err(e) = sdr.reset_buffer() {
                tracing::error!("reset_buffer failed: {}", e);
                running.store(false, Ordering::SeqCst);
                return;
            }

            info!(
                "Tuned to {} Hz, sample rate {} S/s",
                sdr.get_center_freq(),
                sdr.get_sample_rate()
            );

            // Software AGC: step down gain when ADC saturation is detected.
            // Saturation = raw byte value == 0 or == 255.
            // ETSI EN 300 401 does not mandate any AGC, but ADC saturation causes
            // hard clipping that destroys the OFDM constellation.
            let mut sat_bytes: u32 = 0;
            let mut total_bytes: u32 = 0;
            let mut current_gain_index = config.gain_index;
            let mut gain_cooldown: u32 = 0;
            // Evaluate saturation over ~10 reads (~40 ms) and cool down for ~25 reads (~1 s).
            const SAT_WINDOW: u32 = (READLEN_DEFAULT * 10) as u32;
            const SAT_THRESHOLD: f32 = 0.005; // 0.5 % saturation triggers a gain step-down
            const GAIN_COOLDOWN_READS: u32 = 25;

            let mut raw = [0u8; READLEN_DEFAULT];
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match sdr.read_sync(&mut raw) {
                    Ok(n) if n == READLEN_DEFAULT => {
                        // Software AGC saturation check (manual gain mode only).
                        if !config.autogain && config.gains.len() > 1 {
                            for &b in &raw[..n] {
                                if b == 0 || b == 255 {
                                    sat_bytes += 1;
                                }
                            }
                            total_bytes += n as u32;
                            gain_cooldown = gain_cooldown.saturating_sub(1);

                            if total_bytes >= SAT_WINDOW {
                                if gain_cooldown == 0 {
                                    let ratio = sat_bytes as f32 / total_bytes as f32;
                                    if ratio > SAT_THRESHOLD && current_gain_index > 0 {
                                        current_gain_index -= 1;
                                        let new_gain = config.gains[current_gain_index];
                                        match sdr.set_tuner_gain(TunerGain::Manual(new_gain)) {
                                            Ok(()) => info!(
                                                "SAGC: saturation {:.1}% → gain reduced to {}.{} dB",
                                                ratio * 100.0,
                                                new_gain / 10,
                                                new_gain.abs() % 10
                                            ),
                                            Err(e) => warn!("SAGC: set_tuner_gain failed: {}", e),
                                        }
                                        gain_cooldown = GAIN_COOLDOWN_READS;
                                    }
                                }
                                sat_bytes = 0;
                                total_bytes = 0;
                            }
                        }

                        if let Ok(mut guard) = buffer.lock() {
                            for b in &raw[..n] {
                                guard.push_back(*b);
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

        true
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
}
