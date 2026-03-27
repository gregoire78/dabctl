// ==============================================================================
// support/mod.rs - Orchestration du module support
// ==============================================================================

pub mod band_handler;
pub mod dab_params;
pub mod fft_wrapper;
pub mod percentile;

pub use band_handler::BandHandler;
pub use dab_params::DabParams;
pub use fft_wrapper::{FftProcessor, FftSize};
pub use percentile::percentile95_from_histogram;
