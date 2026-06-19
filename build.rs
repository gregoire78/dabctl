fn main() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").ok();
    let is_wasm = target_arch.as_deref() == Some("wasm32");
    println!("cargo:rerun-if-env-changed=LIBFEC_WASM_LIB_DIR");

    if is_wasm {
        // For wasm32 we expect a prebuilt static libfec archive.
        // Set LIBFEC_WASM_LIB_DIR to a directory containing libfec.a.
        let wasm_lib_dir = std::env::var("LIBFEC_WASM_LIB_DIR").ok().or_else(|| {
            let default = std::path::Path::new("third_party/libfec/wasm");
            if default.exists() {
                Some(default.display().to_string())
            } else {
                None
            }
        });

        let Some(wasm_lib_dir) = wasm_lib_dir else {
            panic!(
                "wasm32 build requires libfec.a. Build it and set LIBFEC_WASM_LIB_DIR=<dir containing libfec.a>"
            );
        };

        println!("cargo:rustc-link-search=native={}", wasm_lib_dir);
        println!("cargo:rustc-link-lib=static=fec");
    } else {
        // Native path: use system libfec.
        println!("cargo:rustc-link-lib=fec");
    }

    // AAC decode is not required in latm-only flavor.
    // Keep faad out of wasm-runtime builds for now.
    if std::env::var_os("CARGO_FEATURE_LATM_ONLY").is_none() && !is_wasm {
        println!("cargo:rustc-link-lib=faad");
    }

    if std::env::var_os("CARGO_FEATURE_FDK_AAC").is_some() {
        println!("cargo:rustc-link-lib=fdk-aac");
    }
}
