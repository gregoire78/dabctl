// ==============================================================================
// eti_handling/mod.rs - ETI Handling module
// ==============================================================================

pub mod viterbi_handler;
pub mod fic_handler;
pub mod fib_processor;
pub mod eti_generator;
pub mod cif_interleaver;
pub mod eti_handler;
pub mod protection;

pub use eti_handler::{EtiGenerator, EtiFrame};
pub use protection::{ProtectionScheme, UepProtection, EepProtection};
