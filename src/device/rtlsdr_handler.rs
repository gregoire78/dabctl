// RTL-SDR handler - converted from rtlsdr-handler.cpp (eti-cmdline)

use crate::rtlsdr_sys;
use num_complex::Complex32;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

const READLEN_DEFAULT: u32 = 8192;
const INPUT_RATE: u32 = 2048000;

/// Pre-computed conversion table: convTable[i] = (i as f32 - 128.0) / 128.0
fn build_conv_table() -> [f32; 256] {
    let mut table = [0.0f32; 256];
    for i in 0..256 {
        table[i] = (i as f32 - 128.0) / 128.0;
    }
    table
}

pub struct RtlsdrHandler {
    device: *mut rtlsdr_sys::rtlsdr_dev_t,
    running: Arc<AtomicBool>,
    i_buffer: Arc<Mutex<VecDeque<u8>>>,
    worker_handle: Option<thread::JoinHandle<()>>,
    conv_table: [f32; 256],
    frequency: u32,
    ppm_offset: i32,
    effective_gain: i32,
    autogain: bool,
}

// The rtlsdr device pointer is thread-safe when used correctly
unsafe impl Send for RtlsdrHandler {}

impl RtlsdrHandler {
    pub fn new(
        frequency: u32,
        ppm_offset: i32,
        gain: i16,
        autogain: bool,
        device_index: u32,
    ) -> Result<Self, String> {
        let mut device: *mut rtlsdr_sys::rtlsdr_dev_t = std::ptr::null_mut();

        unsafe {
            let device_count = rtlsdr_sys::rtlsdr_get_device_count();
            if device_count == 0 {
                return Err("No RTL-SDR devices found".to_string());
            }

            let r = rtlsdr_sys::rtlsdr_open(&mut device, device_index);
            if r < 0 {
                return Err(format!("Opening RTL-SDR device {} failed", device_index));
            }

            let r = rtlsdr_sys::rtlsdr_set_sample_rate(device, INPUT_RATE);
            if r < 0 {
                rtlsdr_sys::rtlsdr_close(device);
                return Err("Setting sample rate failed".to_string());
            }

            let actual_rate = rtlsdr_sys::rtlsdr_get_sample_rate(device);
            eprintln!("samplerate set to {}", actual_rate);

            rtlsdr_sys::rtlsdr_set_tuner_gain_mode(device, 0);

            let gains_count = rtlsdr_sys::rtlsdr_get_tuner_gains(device, std::ptr::null_mut());
            let mut gains = vec![0i32; gains_count as usize];
            rtlsdr_sys::rtlsdr_get_tuner_gains(device, gains.as_mut_ptr());

            eprint!("Supported gain values ({}): ", gains_count);
            for g in &gains {
                eprint!("{}.{} ", g / 10, g % 10);
            }
            eprintln!();

            if ppm_offset != 0 {
                let r = rtlsdr_sys::rtlsdr_set_freq_correction(device, ppm_offset);
                if r == 0 {
                    let corr = rtlsdr_sys::rtlsdr_get_freq_correction(device);
                    eprintln!("Frequency correction set to {} ppm", corr);
                } else {
                    eprintln!("Setting frequency correction failed");
                }
            }

            if autogain {
                rtlsdr_sys::rtlsdr_set_agc_mode(device, 1);
            }

            let gain_index = (gain as usize * (gains_count as usize - 1)) / 100;
            let effective_gain = gains[gain_index.min(gains.len() - 1)];
            let set_index = (gain as usize * gains_count as usize) / 100;
            let set_gain = gains[set_index.min(gains.len() - 1)];
            eprintln!(
                "effective gain: {}.{}",
                set_gain / 10,
                set_gain % 10
            );
            rtlsdr_sys::rtlsdr_set_tuner_gain(device, set_gain);

            Ok(RtlsdrHandler {
                device,
                running: Arc::new(AtomicBool::new(false)),
                i_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(4 * 1024 * 1024))),
                worker_handle: None,
                conv_table: build_conv_table(),
                frequency,
                ppm_offset,
                effective_gain,
                autogain,
            })
        }
    }

    pub fn restart_reader(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }

        unsafe {
            // Flush buffer
            {
                let mut buf = self.i_buffer.lock().unwrap();
                buf.clear();
            }

            let r = rtlsdr_sys::rtlsdr_reset_buffer(self.device);
            if r < 0 {
                return false;
            }

            rtlsdr_sys::rtlsdr_set_freq_correction(self.device, self.ppm_offset);
            rtlsdr_sys::rtlsdr_set_center_freq(self.device, self.frequency);
        }

        let running = self.running.clone();
        let buffer = self.i_buffer.clone();
        let device_raw = self.device as usize; // cast to usize for Send

        running.store(true, Ordering::SeqCst);

        self.worker_handle = Some(thread::spawn(move || {
            unsafe extern "C" fn callback(buf: *mut u8, len: u32, ctx: *mut std::ffi::c_void) {
                if buf.is_null() || len != READLEN_DEFAULT {
                    return;
                }
                let buffer = &*(ctx as *const Mutex<VecDeque<u8>>);
                let slice = std::slice::from_raw_parts(buf, len as usize);
                if let Ok(mut guard) = buffer.lock() {
                    guard.extend(slice);
                }
            }

            let device = device_raw as *mut rtlsdr_sys::rtlsdr_dev_t;
            let buffer_ptr = Arc::into_raw(buffer.clone());
            unsafe {
                rtlsdr_sys::rtlsdr_read_async(
                    device,
                    Some(callback),
                    buffer_ptr as *mut std::ffi::c_void,
                    0,
                    READLEN_DEFAULT,
                );
            }
        }));

        unsafe {
            rtlsdr_sys::rtlsdr_set_tuner_gain(self.device, self.effective_gain);
            if self.autogain {
                rtlsdr_sys::rtlsdr_set_agc_mode(self.device, 1);
            }
        }

        true
    }

    pub fn stop_reader(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        unsafe {
            rtlsdr_sys::rtlsdr_cancel_async(self.device);
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn get_samples(&self, v: &mut [Complex32]) -> usize {
        let size = v.len();
        let mut temp = vec![0u8; 2 * size];
        let amount = {
            let mut buf = self.i_buffer.lock().unwrap();
            let available = buf.len().min(2 * size);
            for i in 0..available {
                temp[i] = buf.pop_front().unwrap_or(0);
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
        unsafe {
            rtlsdr_sys::rtlsdr_close(self.device);
        }
    }
}
