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

fn resolve_wasm_faad_dir(env_value: Option<String>, default_exists: bool) -> Option<String> {
    env_value.or_else(|| {
        if default_exists {
            Some("third_party/libfaad/wasm".to_string())
        } else {
            None
        }
    })
}

fn should_link_faad(latm_only_enabled: bool, is_wasm: bool, wasm_faad2_enabled: bool) -> bool {
    if wasm_faad2_enabled {
        // User opted into wasm FAAD; they must provide a wasm32-compiled libfaad.a.
        return true;
    }
    !latm_only_enabled && !is_wasm
}

fn should_link_fdk(fdk_enabled: bool) -> bool {
    fdk_enabled
}

fn main() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").ok();
    let is_wasm = is_wasm_target(target_arch.as_deref());
    println!("cargo:rerun-if-env-changed=LIBFEC_WASM_LIB_DIR");
    println!("cargo:rerun-if-env-changed=LIBFAAD_WASM_LIB_DIR");

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
    // Keep faad out of plain wasm-runtime builds; link it when wasm-faad2 is set.
    let latm_only_enabled = std::env::var_os("CARGO_FEATURE_LATM_ONLY").is_some();
    let wasm_faad2_enabled = std::env::var_os("CARGO_FEATURE_WASM_FAAD2").is_some();
    if should_link_faad(latm_only_enabled, is_wasm, wasm_faad2_enabled) {
        if is_wasm {
            // For wasm32 we need a prebuilt static libfaad archive.
            // Build it with scripts/build-libfaad-wasm.sh and set LIBFAAD_WASM_LIB_DIR.
            let default_exists =
                std::path::Path::new("third_party/libfaad/wasm/libfaad.a").exists();
            let faad_lib_dir =
                resolve_wasm_faad_dir(std::env::var("LIBFAAD_WASM_LIB_DIR").ok(), default_exists);
            let Some(faad_lib_dir) = faad_lib_dir else {
                panic!(
                    "wasm-faad2 build requires libfaad.a for wasm32.\n\
                     Build it first: bash scripts/build-libfaad-wasm.sh\n\
                     Or set LIBFAAD_WASM_LIB_DIR=<dir containing libfaad.a>"
                );
            };
            println!("cargo:rustc-link-search=native={}", faad_lib_dir);
            println!("cargo:rustc-link-lib=static=faad");
        } else {
            // Native path: use system libfaad.
            println!("cargo:rustc-link-lib=faad");
        }
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
    fn resolve_wasm_faad_dir_prefers_env_value() {
        let dir = resolve_wasm_faad_dir(Some("/tmp/faad".to_string()), false);
        assert_eq!(dir.as_deref(), Some("/tmp/faad"));
    }

    #[test]
    fn resolve_wasm_faad_dir_uses_default_when_available() {
        let dir = resolve_wasm_faad_dir(None, true);
        assert_eq!(dir.as_deref(), Some("third_party/libfaad/wasm"));
    }

    #[test]
    fn resolve_wasm_faad_dir_none_when_unavailable() {
        let dir = resolve_wasm_faad_dir(None, false);
        assert!(dir.is_none());
    }

    #[test]
    fn faad_link_rule() {
        assert!(should_link_faad(false, false, false));
        assert!(!should_link_faad(true, false, false));
        assert!(!should_link_faad(false, true, false));
        // wasm-faad2 overrides the wasm exclusion
        assert!(should_link_faad(true, true, true));
    }

    #[test]
    fn fdk_link_rule() {
        assert!(should_link_fdk(true));
        assert!(!should_link_fdk(false));
    }
}
