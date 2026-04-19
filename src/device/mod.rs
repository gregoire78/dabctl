pub mod ffi;

use anyhow::{anyhow, bail, Result};
use tracing::{debug, info};

const DAB_SAMPLE_RATE_HZ: u32 = 2_048_000;

#[derive(Debug, Clone, Copy)]
pub struct DeviceOptions {
    pub index: u32,
    pub center_freq_hz: u32,
    pub gain: Option<u8>,
    pub hardware_agc: bool,
    pub driver_agc: bool,
    pub software_agc: bool,
    pub silent: bool,
}

pub struct RtlSdrDevice {
    raw: *mut ffi::rtlsdr_dev_t,
    agc_mode: &'static str,
    center_freq_hz: u32,
    read_calls: u64,
    last_tuner_gain_tenths_db: Option<i32>,
    agc_logging_enabled: bool,
}

impl RtlSdrDevice {
    pub fn open(options: &DeviceOptions) -> Result<Self> {
        let agc_mode = agc_mode_label(options);
        let requested_gain_db = options
            .gain
            .map(|gain| user_gain_to_tenths_db(gain) as f32 / 10.0);

        info!(
            device_index = options.index,
            center_freq_hz = options.center_freq_hz,
            agc_mode,
            requested_gain_db = ?requested_gain_db,
            hardware_agc = options.hardware_agc,
            driver_agc = options.driver_agc,
            software_agc = options.software_agc,
            "configuring RTL-SDR gain control"
        );

        let raw = with_suppressed_stderr(options.silent, || -> Result<*mut ffi::rtlsdr_dev_t> {
            let count = unsafe { ffi::rtlsdr_get_device_count() };
            if count == 0 {
                bail!("no RTL-SDR devices reported by the vendored old-dab backend");
            }
            if options.index >= count {
                bail!(
                    "requested RTL-SDR device index {} but only {} device(s) are available",
                    options.index,
                    count
                );
            }

            let mut raw = std::ptr::null_mut();
            checked(
                unsafe { ffi::rtlsdr_open(&mut raw, options.index) },
                "rtlsdr_open",
            )?;
            if raw.is_null() {
                bail!("rtlsdr_open returned a null device handle");
            }

            checked(
                unsafe { ffi::rtlsdr_set_sample_rate(raw, DAB_SAMPLE_RATE_HZ) },
                "rtlsdr_set_sample_rate",
            )?;
            checked(
                unsafe { ffi::rtlsdr_set_center_freq(raw, options.center_freq_hz) },
                "rtlsdr_set_center_freq",
            )?;

            if let Some(gain) = options.gain {
                checked(
                    unsafe { ffi::rtlsdr_set_tuner_gain_mode(raw, 1) },
                    "rtlsdr_set_tuner_gain_mode(manual)",
                )?;
                checked(
                    unsafe { ffi::rtlsdr_set_tuner_gain(raw, user_gain_to_tenths_db(gain)) },
                    "rtlsdr_set_tuner_gain",
                )?;
            } else {
                checked(
                    unsafe { ffi::rtlsdr_set_tuner_gain_mode(raw, 0) },
                    "rtlsdr_set_tuner_gain_mode(auto)",
                )?;
            }

            if options.hardware_agc || options.driver_agc {
                checked(
                    unsafe { ffi::rtlsdr_set_agc_mode(raw, 1) },
                    "rtlsdr_set_agc_mode",
                )?;
            }

            if options.software_agc {
                info!(
                    agc_mode,
                    "software AGC requested; enabling live AGC telemetry for the receive path"
                );
            }

            checked(
                unsafe { ffi::rtlsdr_reset_buffer(raw) },
                "rtlsdr_reset_buffer",
            )?;

            Ok(raw)
        })?;

        let current_gain = current_tuner_gain_tenths_db(raw);
        if let Some(gain_tenths_db) = current_gain {
            info!(
                agc_mode,
                tuner_gain_db = tenths_db_to_db(gain_tenths_db),
                "RTL-SDR gain control ready"
            );
        } else {
            info!(agc_mode, "RTL-SDR gain control ready");
        }

        Ok(Self {
            raw,
            agc_mode,
            center_freq_hz: options.center_freq_hz,
            read_calls: 0,
            last_tuner_gain_tenths_db: current_gain,
            agc_logging_enabled: options.hardware_agc || options.driver_agc || options.software_agc,
        })
    }

    pub fn center_freq_hz(&self) -> u32 {
        self.center_freq_hz
    }

    pub fn set_center_freq_hz(&mut self, center_freq_hz: u32) -> Result<()> {
        checked(
            unsafe { ffi::rtlsdr_set_center_freq(self.raw, center_freq_hz) },
            "rtlsdr_set_center_freq",
        )?;
        self.center_freq_hz = center_freq_hz;
        debug!(center_freq_hz, "updated RTL-SDR center frequency");
        Ok(())
    }

    pub fn reset_buffer(&mut self) -> Result<()> {
        checked(
            unsafe { ffi::rtlsdr_reset_buffer(self.raw) },
            "rtlsdr_reset_buffer",
        )
    }

    pub fn read_sync(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut n_read = 0;
        checked(
            unsafe {
                ffi::rtlsdr_read_sync(
                    self.raw,
                    buf.as_mut_ptr().cast(),
                    buf.len() as i32,
                    &mut n_read,
                )
            },
            "rtlsdr_read_sync",
        )?;

        let n_read = usize::try_from(n_read)
            .map_err(|_| anyhow!("rtlsdr_read_sync returned a negative length"))?;

        self.read_calls = self.read_calls.saturating_add(1);
        if self.agc_logging_enabled {
            let current_gain = current_tuner_gain_tenths_db(self.raw);
            let gain_changed = current_gain != self.last_tuner_gain_tenths_db;
            let should_log =
                self.read_calls <= 4 || self.read_calls.is_multiple_of(16) || gain_changed;

            if should_log {
                if let Some(gain_tenths_db) = current_gain {
                    debug!(
                        agc_mode = self.agc_mode,
                        read_call = self.read_calls,
                        bytes = n_read,
                        tuner_gain_db = tenths_db_to_db(gain_tenths_db),
                        gain_changed,
                        "AGC tuner state"
                    );
                } else {
                    debug!(
                        agc_mode = self.agc_mode,
                        read_call = self.read_calls,
                        bytes = n_read,
                        gain_changed,
                        "AGC tuner state"
                    );
                }
                self.last_tuner_gain_tenths_db = current_gain;
            }
        }

        Ok(n_read)
    }
}

impl Drop for RtlSdrDevice {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            let _ = unsafe { ffi::rtlsdr_close(self.raw) };
        }
    }
}

fn checked(code: i32, api: &str) -> Result<()> {
    if code < 0 {
        bail!("{api} failed with status {code}");
    }
    Ok(())
}

fn agc_mode_label(options: &DeviceOptions) -> &'static str {
    if options.hardware_agc {
        "hardware-agc"
    } else if options.driver_agc {
        "driver-agc"
    } else if options.software_agc {
        "software-agc"
    } else if options.gain.is_some() {
        "manual-gain"
    } else {
        "tuner-auto"
    }
}

fn current_tuner_gain_tenths_db(raw: *mut ffi::rtlsdr_dev_t) -> Option<i32> {
    let gain = unsafe { ffi::rtlsdr_get_tuner_gain(raw) };
    (gain >= 0).then_some(gain)
}

fn tenths_db_to_db(gain_tenths_db: i32) -> f32 {
    gain_tenths_db as f32 / 10.0
}

fn user_gain_to_tenths_db(gain: u8) -> i32 {
    i32::from(gain) * 5
}

fn with_suppressed_stderr<T>(silent: bool, f: impl FnOnce() -> T) -> T {
    if !silent {
        return f();
    }

    let saved_fd = unsafe { libc::dup(libc::STDERR_FILENO) };
    if saved_fd < 0 {
        return f();
    }

    let dev_null_fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY) };
    if dev_null_fd >= 0 {
        unsafe {
            libc::dup2(dev_null_fd, libc::STDERR_FILENO);
            libc::close(dev_null_fd);
        }
    }

    let result = f();

    unsafe {
        libc::dup2(saved_fd, libc::STDERR_FILENO);
        libc::close(saved_fd);
    }

    result
}
