#[cfg(all(feature = "wasm-runtime", target_arch = "wasm32"))]
pub mod bindings;

#[cfg(feature = "wasm-runtime")]
pub mod runtime;
