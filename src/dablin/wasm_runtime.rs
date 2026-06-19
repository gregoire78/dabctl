//! WebAssembly-oriented runtime scaffolding.
//!
//! This module is intentionally minimal and keeps the native CLI behavior unchanged.
//! It defines a memory-based interface that future wasm bindings can expose.

#![cfg_attr(feature = "wasm-runtime", allow(dead_code))]

use anyhow::{bail, Result};

/// Output container for a memory-based LATM decode call.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LatmDecodeOutput {
    /// Concatenated LATM/LOAS bytes.
    pub latm_bytes: Vec<u8>,
    /// Metadata events as JSONL lines.
    pub metadata_jsonl: Vec<String>,
}

/// Placeholder for wasm-oriented ETI-to-LATM decoding.
///
/// The default native CLI path remains authoritative. This function exists as
/// a stable API surface to incrementally move toward wasm embedding.
pub fn decode_eti_to_latm_memory(_eti_bytes: &[u8]) -> Result<LatmDecodeOutput> {
    bail!("wasm runtime decode path is not implemented yet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_placeholder_returns_error() {
        let err = decode_eti_to_latm_memory(&[0u8; 16]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not implemented"));
    }
}
