fn is_wasm_target(target_arch: Option<&str>) -> bool {
    target_arch == Some("wasm32")
}

fn resolve_wasm_lib_dir(env_value: Option<String>, default_exists: bool) -> Option<String> {
    env_value.or_else(|| {
        if default_exists {
            Some("third_party/libfec/wasm".to_string())
        } else {
            None
        }
    })
}

fn should_link_faad(latm_only_enabled: bool, is_wasm: bool) -> bool {
    !latm_only_enabled && !is_wasm
}

fn should_link_fdk(fdk_enabled: bool) -> bool {
    fdk_enabled
}

fn main() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").ok();
    let is_wasm = is_wasm_target(target_arch.as_deref());
    println!("cargo:rerun-if-env-changed=LIBFEC_WASM_LIB_DIR");

    if is_wasm {
        // For wasm32 we expect a prebuilt static libfec archive.
        // Set LIBFEC_WASM_LIB_DIR to a directory containing libfec.a.
        let default_exists = std::path::Path::new("third_party/libfec/wasm").exists();
        let wasm_lib_dir =
            resolve_wasm_lib_dir(std::env::var("LIBFEC_WASM_LIB_DIR").ok(), default_exists);

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
    let latm_only_enabled = std::env::var_os("CARGO_FEATURE_LATM_ONLY").is_some();
    if should_link_faad(latm_only_enabled, is_wasm) {
        println!("cargo:rustc-link-lib=faad");
    }

    let fdk_enabled = std::env::var_os("CARGO_FEATURE_FDK_AAC").is_some();
    if should_link_fdk(fdk_enabled) {
        println!("cargo:rustc-link-lib=fdk-aac");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_wasm_target() {
        assert!(is_wasm_target(Some("wasm32")));
        assert!(!is_wasm_target(Some("x86_64")));
        assert!(!is_wasm_target(None));
    }

    #[test]
    fn resolve_wasm_lib_dir_prefers_env_value() {
        let dir = resolve_wasm_lib_dir(Some("/tmp/custom".to_string()), false);
        assert_eq!(dir.as_deref(), Some("/tmp/custom"));
    }

    #[test]
    fn resolve_wasm_lib_dir_uses_default_when_available() {
        let dir = resolve_wasm_lib_dir(None, true);
        assert_eq!(dir.as_deref(), Some("third_party/libfec/wasm"));
    }

    #[test]
    fn resolve_wasm_lib_dir_none_when_unavailable() {
        let dir = resolve_wasm_lib_dir(None, false);
        assert!(dir.is_none());
    }

    #[test]
    fn faad_link_rule() {
        assert!(should_link_faad(false, false));
        assert!(!should_link_faad(true, false));
        assert!(!should_link_faad(false, true));
    }

    #[test]
    fn fdk_link_rule() {
        assert!(should_link_fdk(true));
        assert!(!should_link_fdk(false));
    }
}
