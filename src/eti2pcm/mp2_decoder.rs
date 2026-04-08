/// MPEG-1/2 Layer II (MP2) decoder using libmpg123 for DAB (non-DAB+).
use std::ptr;

#[allow(non_camel_case_types)]
mod ffi {
    use std::os::raw::{c_char, c_int, c_long, c_uchar, c_void};

    pub type mpg123_handle = *mut c_void;

    pub const MPG123_OK: c_int = 0;
    pub const MPG123_NEW_FORMAT: c_int = -11;
    pub const MPG123_NEED_MORE: c_int = -10;
    pub const MPG123_DONE: c_int = -12;
    pub const MPG123_ENC_SIGNED_16: c_int = 0x00D0;

    unsafe extern "C" {
        pub fn mpg123_init() -> c_int;
        pub fn mpg123_new(decoder: *const c_char, error: *mut c_int) -> mpg123_handle;
        pub fn mpg123_delete(handle: mpg123_handle);
        pub fn mpg123_open_feed(handle: mpg123_handle) -> c_int;
        pub fn mpg123_feed(handle: mpg123_handle, data: *const c_uchar, size: usize) -> c_int;
        pub fn mpg123_read(
            handle: mpg123_handle,
            outmemory: *mut c_uchar,
            outmemsize: usize,
            done: *mut usize,
        ) -> c_int;
        pub fn mpg123_getformat(
            handle: mpg123_handle,
            rate: *mut c_long,
            channels: *mut c_int,
            encoding: *mut c_int,
        ) -> c_int;
        pub fn mpg123_format_none(handle: mpg123_handle) -> c_int;
        pub fn mpg123_format(
            handle: mpg123_handle,
            rate: c_long,
            channels: c_int,
            encodings: c_int,
        ) -> c_int;
        #[allow(dead_code)]
        pub fn mpg123_exit();
    }
}

static MPG123_INIT: std::sync::Once = std::sync::Once::new();

/// MP2 decoder wrapper
pub struct Mp2Decoder {
    handle: ffi::mpg123_handle,
    pub sample_rate: u32,
    pub channels: u8,
    format_known: bool,
    output_buf: Vec<u8>,
}

impl Mp2Decoder {
    pub fn new() -> Result<Self, String> {
        MPG123_INIT.call_once(|| unsafe {
            ffi::mpg123_init();
        });

        unsafe {
            let mut err: std::os::raw::c_int = 0;
            let handle = ffi::mpg123_new(ptr::null(), &mut err);
            if handle.is_null() {
                return Err(format!("mpg123_new failed: {}", err));
            }

            if ffi::mpg123_open_feed(handle) != ffi::MPG123_OK {
                ffi::mpg123_delete(handle);
                return Err("mpg123_open_feed failed".into());
            }

            // Accept common DAB sample rates
            ffi::mpg123_format_none(handle);
            for rate in [48000i64, 24000, 32000, 16000] {
                ffi::mpg123_format(
                    handle,
                    rate as std::os::raw::c_long,
                    1 | 2,
                    ffi::MPG123_ENC_SIGNED_16,
                );
            }

            Ok(Mp2Decoder {
                handle,
                sample_rate: 0,
                channels: 0,
                format_known: false,
                output_buf: vec![0u8; 8192],
            })
        }
    }

    /// Feed raw subchannel data and decode. Returns PCM samples (interleaved i16).
    pub fn feed(&mut self, data: &[u8]) -> Vec<Vec<i16>> {
        let mut results = Vec::new();

        unsafe {
            let ret = ffi::mpg123_feed(self.handle, data.as_ptr(), data.len());
            if ret != ffi::MPG123_OK {
                return results;
            }

            loop {
                let mut done: usize = 0;
                let ret = ffi::mpg123_read(
                    self.handle,
                    self.output_buf.as_mut_ptr(),
                    self.output_buf.len(),
                    &mut done,
                );

                if ret == ffi::MPG123_NEW_FORMAT {
                    let mut rate: std::os::raw::c_long = 0;
                    let mut channels: std::os::raw::c_int = 0;
                    let mut encoding: std::os::raw::c_int = 0;
                    ffi::mpg123_getformat(self.handle, &mut rate, &mut channels, &mut encoding);
                    self.sample_rate = rate as u32;
                    self.channels = channels as u8;
                    self.format_known = true;
                    continue;
                }

                if done > 0 {
                    let samples: &[i16] = std::slice::from_raw_parts(
                        self.output_buf.as_ptr() as *const i16,
                        done / 2,
                    );
                    results.push(samples.to_vec());
                }

                if ret == ffi::MPG123_NEED_MORE || ret == ffi::MPG123_DONE || done == 0 {
                    break;
                }
            }
        }

        results
    }
}

impl Drop for Mp2Decoder {
    fn drop(&mut self) {
        unsafe {
            ffi::mpg123_delete(self.handle);
        }
    }
}

unsafe impl Send for Mp2Decoder {}
