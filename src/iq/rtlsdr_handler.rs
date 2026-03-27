use crate::iq::IqSource;
use crate::iq::rtlsdr_port::eti_cmdline_gain_selection;
use crate::rtlsdr_sys;
use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::os::raw::{c_int, c_void};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

const DAB_SAMPLE_RATE: u32 = 2_048_000;
const ASYNC_READ_LEN: u32 = 8192;
const ASYNC_RING_BYTES: usize = 4 * 1024 * 1024;

struct AsyncBufferState {
    queue: VecDeque<u8>,
    closed: bool,
    worker_error: Option<i32>,
}

struct CallbackContext {
    shared: Arc<(Mutex<AsyncBufferState>, Condvar)>,
    max_bytes: usize,
}

pub struct RtlSdrSource {
    dev: *mut rtlsdr_sys::rtlsdr_dev,
    shared: Arc<(Mutex<AsyncBufferState>, Condvar)>,
    callback_ctx: *mut CallbackContext,
    worker: Option<thread::JoinHandle<()>>,
}

impl RtlSdrSource {
    pub fn new(
        device_index: u32,
        frequency: u32,
        gain_percent: u32,
        ppm_offset: i32,
        autogain: bool,
    ) -> Result<Self> {
        unsafe {
            let device_count = rtlsdr_sys::rtlsdr_get_device_count();
            if device_count == 0 {
                return Err(anyhow!("No RTL-SDR devices found"));
            }

            if device_index >= device_count {
                return Err(anyhow!(
                    "Device index {} out of range (available: 0-{})",
                    device_index,
                    device_count - 1
                ));
            }

            let mut dev: *mut rtlsdr_sys::rtlsdr_dev = std::ptr::null_mut();
            let result = rtlsdr_sys::rtlsdr_open(&mut dev, device_index);
            if result != 0 {
                return Err(anyhow!("Failed to open RTL-SDR device: {}", result));
            }
            if dev.is_null() {
                return Err(anyhow!("RTL-SDR device is null"));
            }

            configure_device(dev, frequency, gain_percent, ppm_offset, autogain).inspect_err(|_e| {
                rtlsdr_sys::rtlsdr_close(dev);
            })?;

            let shared = Arc::new((
                Mutex::new(AsyncBufferState {
                    queue: VecDeque::with_capacity(ASYNC_RING_BYTES),
                    closed: false,
                    worker_error: None,
                }),
                Condvar::new(),
            ));

            let callback_ctx = Box::into_raw(Box::new(CallbackContext {
                shared: Arc::clone(&shared),
                max_bytes: ASYNC_RING_BYTES,
            }));

            let dev_addr = dev as usize;
            let ctx_addr = callback_ctx as usize;
            let worker_shared = Arc::clone(&shared);
            let worker = thread::spawn(move || {
                let dev_ptr = dev_addr as *mut rtlsdr_sys::rtlsdr_dev;
                let ctx_ptr = ctx_addr as *mut c_void;
                let status = rtlsdr_sys::rtlsdr_read_async(
                    dev_ptr,
                    Some(rtlsdr_async_callback),
                    ctx_ptr,
                    0,
                    ASYNC_READ_LEN,
                );

                let (lock, cv) = &*worker_shared;
                let mut state = lock.lock().expect("rtl async state poisoned");
                state.closed = true;
                if status != 0 {
                    state.worker_error = Some(status);
                }
                cv.notify_all();
            });

            Ok(Self {
                dev,
                shared,
                callback_ctx,
                worker: Some(worker),
            })
        }
    }
}

unsafe extern "C" fn rtlsdr_async_callback(buf: *mut u8, len: u32, ctx: *mut c_void) {
    if buf.is_null() || ctx.is_null() || len == 0 {
        return;
    }

    let context = &*(ctx as *mut CallbackContext);
    let incoming = std::slice::from_raw_parts(buf, len as usize);
    let (lock, cv) = &*context.shared;
    let mut state = match lock.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    if state.closed {
        return;
    }

    // Keep the freshest samples when the consumer is slower than producer.
    let needed = state.queue.len().saturating_add(incoming.len());
    if needed > context.max_bytes {
        let drop_count = needed - context.max_bytes;
        state.queue.drain(0..drop_count);
    }
    state.queue.extend(incoming.iter().copied());
    cv.notify_all();
}

impl IqSource for RtlSdrSource {
    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let (lock, cv) = &*self.shared;
        let mut state = lock.lock().map_err(|_| anyhow!("RTL async buffer lock poisoned"))?;

        while state.queue.is_empty() && !state.closed {
            state = cv
                .wait(state)
                .map_err(|_| anyhow!("RTL async buffer wait poisoned"))?;
        }

        if state.queue.is_empty() {
            if let Some(code) = state.worker_error {
                return Err(anyhow!("Error reading from RTL-SDR async worker: {}", code));
            }
            return Ok(0);
        }

        let n = buffer.len().min(state.queue.len());
        for out in buffer.iter_mut().take(n) {
            *out = state.queue.pop_front().unwrap_or(0);
        }
        Ok(n)
    }
}

impl Drop for RtlSdrSource {
    fn drop(&mut self) {
        unsafe {
            if !self.dev.is_null() {
                let _ = rtlsdr_sys::rtlsdr_cancel_async(self.dev);

                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }

                if !self.callback_ctx.is_null() {
                    let _ = Box::from_raw(self.callback_ctx);
                    self.callback_ctx = std::ptr::null_mut();
                }

                rtlsdr_sys::rtlsdr_close(self.dev);
            }

            self.dev = std::ptr::null_mut();
        }
    }
}

unsafe fn select_percent_gain_tenth_db(dev: *mut rtlsdr_sys::rtlsdr_dev, percent: u32) -> Option<i32> {
    let gains = list_supported_gains_tenth_db(dev)?;
    eti_cmdline_gain_selection(&gains, percent).map(|selection| selection.set_gain_tenth_db)
}

unsafe fn list_supported_gains_tenth_db(dev: *mut rtlsdr_sys::rtlsdr_dev) -> Option<Vec<i32>> {
    let count = rtlsdr_sys::rtlsdr_get_tuner_gains(dev, std::ptr::null_mut());
    if count <= 0 {
        return None;
    }

    let mut gains = vec![0i32; count as usize];
    let filled = rtlsdr_sys::rtlsdr_get_tuner_gains(dev, gains.as_mut_ptr());
    if filled <= 0 {
        return None;
    }
    gains.truncate(filled as usize);
    Some(gains)
}

unsafe fn configure_device(
    dev: *mut rtlsdr_sys::rtlsdr_dev,
    frequency: u32,
    gain_percent: u32,
    ppm_offset: i32,
    autogain: bool,
) -> Result<()> {
    let result = rtlsdr_sys::rtlsdr_set_sample_rate(dev, DAB_SAMPLE_RATE);
    if result != 0 {
        return Err(anyhow!("Failed to set sample rate: {}", result));
    }

    if ppm_offset != 0 {
        let result = rtlsdr_sys::rtlsdr_set_freq_correction(dev, ppm_offset as c_int);
        if result != 0 {
            return Err(anyhow!("Failed to set frequency correction: {}", result));
        }
    }

    let result = rtlsdr_sys::rtlsdr_set_center_freq(dev, frequency);
    if result != 0 {
        return Err(anyhow!("Failed to set frequency: {}", result));
    }

    // eti-cmdline keeps tuner gain mode on auto for RTL-SDR and still sets a tuner gain value.
    let result = rtlsdr_sys::rtlsdr_set_tuner_gain_mode(dev, 0);
    if result != 0 {
        return Err(anyhow!("Failed to set tuner gain mode: {}", result));
    }

    let rtl_gain = select_percent_gain_tenth_db(dev, gain_percent)
        .ok_or_else(|| anyhow!("Failed to read supported tuner gains for percent mapping"))?;
    let result = rtlsdr_sys::rtlsdr_set_tuner_gain(dev, rtl_gain);
    if result != 0 {
        return Err(anyhow!("Failed to set tuner gain: {}", result));
    }

    let agc_mode = if autogain { 1 } else { 0 };
    let result = rtlsdr_sys::rtlsdr_set_agc_mode(dev, agc_mode);
    if result != 0 {
        return Err(anyhow!("Failed to set AGC mode: {}", result));
    }

    let result = rtlsdr_sys::rtlsdr_reset_buffer(dev);
    if result != 0 {
        return Err(anyhow!("Failed to reset buffer: {}", result));
    }

    Ok(())
}
