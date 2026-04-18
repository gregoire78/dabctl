pub mod ffi;

use anyhow::{anyhow, bail, Result};

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
}

impl RtlSdrDevice {
    pub fn open(options: &DeviceOptions) -> Result<Self> {
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

            let _software_agc_requested = options.software_agc;

            checked(
                unsafe { ffi::rtlsdr_reset_buffer(raw) },
                "rtlsdr_reset_buffer",
            )?;

            Ok(raw)
        })?;

        Ok(Self { raw })
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

        usize::try_from(n_read).map_err(|_| anyhow!("rtlsdr_read_sync returned a negative length"))
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
