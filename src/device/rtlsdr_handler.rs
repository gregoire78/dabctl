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
    /// Gain as percentage (0..=100); only used when `autogain` is false.
    gain_pct: i16,
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
        Ok(RtlsdrHandler {
            config: DeviceConfig {
                device_index: device_index as usize,
                frequency,
                ppm_offset,
                autogain,
                gain_pct: gain,
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
                    let msg = format!("Failed to open RTL-SDR device {}: {}", config.device_index, e);
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
            info!(
                "Supported gain values ({}): {}",
                gains.len(),
                gains_str.join(" ")
            );

            let gain_value = if config.autogain {
                info!("Auto gain enabled (hardware AGC)");
                0i32
            } else {
                let index = (config.gain_pct.clamp(0, 100) as usize * (gains.len() - 1)) / 100;
                let selected = gains[index];
                info!(
                    "Manual gain set to {}.{} dB",
                    selected / 10,
                    selected.abs() % 10
                );
                selected
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

            let tuner_gain = if config.autogain {
                TunerGain::Auto
            } else {
                TunerGain::Manual(gain_value)
            };
            if let Err(e) = sdr.set_tuner_gain(tuner_gain) {
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

            let mut raw = [0u8; READLEN_DEFAULT];
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match sdr.read_sync(&mut raw) {
                    Ok(n) if n == READLEN_DEFAULT => {
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
}
