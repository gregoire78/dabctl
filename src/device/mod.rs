#[cfg(any(feature = "rtl-sdr-old-dab", feature = "rtl-sdr-osmocom"))]
mod rtlsdr_handler_osmocom;
#[cfg(not(any(feature = "rtl-sdr-old-dab", feature = "rtl-sdr-osmocom")))]
mod rtlsdr_handler_rs;

pub mod rtlsdr_handler {
    // old-dab and osmocom share the same C API → same handler implementation.
    #[cfg(any(feature = "rtl-sdr-old-dab", feature = "rtl-sdr-osmocom"))]
    pub use super::rtlsdr_handler_osmocom::{GainMode, RtlsdrHandler};

    #[cfg(not(any(feature = "rtl-sdr-old-dab", feature = "rtl-sdr-osmocom")))]
    pub use super::rtlsdr_handler_rs::{GainMode, RtlsdrHandler};
}
