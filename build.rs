use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let vendor_dir = manifest_dir.join("vendor").join("old-dab-rtlsdr");
    let header = vendor_dir.join("include").join("rtl-sdr.h");

    println!("cargo:rerun-if-changed={}", header.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap_or_else(|_| "target".to_string()));
    let bindings_path = out_dir.join("rtlsdr_bindings.rs");

    let builder = bindgen::Builder::default()
        .header(header.to_string_lossy())
        .allowlist_function("rtlsdr_.*")
        .allowlist_type("rtlsdr_.*")
        .allowlist_var("RTLSDR_.*")
        .generate_comments(true)
        .derive_default(true);

    match builder.generate() {
        Ok(bindings) => {
            let _ = bindings.write_to_file(&bindings_path);
        }
        Err(err) => {
            println!("cargo:warning=bindgen failed for rtl-sdr.h: {err}");
        }
    }

    let build_dir = out_dir.join("rtlsdr-build");
    let configure = Command::new("cmake")
        .args([
            "-S",
            &vendor_dir.to_string_lossy(),
            "-B",
            &build_dir.to_string_lossy(),
            "-DCMAKE_BUILD_TYPE=Release",
            "-DDETACH_KERNEL_DRIVER=ON",
            "-DINSTALL_UDEV_RULES=OFF",
        ])
        .status();

    if matches!(configure, Ok(status) if status.success()) {
        let build = Command::new("cmake")
            .args([
                "--build",
                &build_dir.to_string_lossy(),
                "--config",
                "Release",
            ])
            .status();

        if matches!(build, Ok(status) if status.success()) {
            let lib_dir = build_dir.join("src");
            println!("cargo:rustc-link-search=native={}", build_dir.display());
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
            println!("cargo:rustc-link-lib=static=rtlsdr");
            println!("cargo:rustc-link-lib=usb-1.0");
            println!("cargo:rustc-link-lib=pthread");
        } else {
            println!("cargo:warning=cmake build for vendored rtl-sdr did not succeed; runtime device access may be unavailable");
        }
    } else {
        println!("cargo:warning=cmake configure for vendored rtl-sdr did not succeed; runtime device access may be unavailable");
    }
}
