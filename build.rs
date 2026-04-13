use std::path::PathBuf;
use std::process::Command;

fn generate_bindings(header: &str, include_dir: Option<&str>) {
    let out_path = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let mut builder = bindgen::Builder::default()
        .header(header)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_function("rtlsdr.*")
        .allowlist_type("rtlsdr.*")
        .allowlist_var("RTLSDR.*");
    if let Some(inc) = include_dir {
        builder = builder.clang_arg(format!("-I{}", inc));
    }
    builder
        .generate()
        .expect("Unable to generate rtlsdr bindings")
        .write_to_file(out_path.join("rtlsdr.rs"))
        .expect("Couldn't write rtlsdr bindings");
}

/// Build the vendored old-dab/rtlsdr static library via cmake.
///
/// The static lib target is `rtlsdr_static`; cmake produces
/// `<build_dir>/src/librtlsdr.a` on Linux.
fn build_old_dab_rtlsdr() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = PathBuf::from(&out_dir);
    let build_dir = out_path.join("old-dab-rtlsdr-build");
    let src_dir = PathBuf::from("vendor/old-dab-rtlsdr")
        .canonicalize()
        .expect("vendor/old-dab-rtlsdr not found — did you run `git submodule update --init`?");

    std::fs::create_dir_all(&build_dir).expect("failed to create cmake build dir");

    let status = Command::new("cmake")
        .arg("-S")
        .arg(&src_dir)
        .arg("-B")
        .arg(&build_dir)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        // Suppress install of udev rules / tools / pkg-config to keep OUT_DIR clean.
        .arg("-DINSTALL_UDEV_RULES=OFF")
        .status()
        .expect("cmake configure failed — is cmake installed?");
    assert!(status.success(), "cmake configure step failed");

    let status = Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .arg("--target")
        .arg("rtlsdr_static")
        .arg("--config")
        .arg("Release")
        .status()
        .expect("cmake build failed");
    assert!(status.success(), "cmake build of rtlsdr_static failed");

    // Static lib lives in the `src/` sub-directory of the cmake build tree.
    let lib_dir = build_dir.join("src");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=rtlsdr");
    // rtlsdr depends on libusb-1.0 at link time.
    println!("cargo:rustc-link-lib=usb-1.0");

    // Regenerate bindings when the vendor header changes.
    let header = src_dir.join("include/rtl-sdr.h");
    let include = src_dir.join("include");
    println!("cargo:rerun-if-changed={}", header.display());
    generate_bindings(
        header.to_str().expect("header path is not UTF-8"),
        Some(include.to_str().expect("include path is not UTF-8")),
    );
}

fn main() {
    // ── RTL-SDR backend ────────────────────────────────────────────────────────
    if std::env::var("CARGO_FEATURE_RTL_SDR_OLD_DAB").is_ok() {
        // Build vendored old-dab/rtlsdr and link statically.
        println!("cargo:rerun-if-changed=vendor/old-dab-rtlsdr");
        build_old_dab_rtlsdr();
    } else if std::env::var("CARGO_FEATURE_RTL_SDR_OSMOCOM").is_ok() {
        // Link against the system librtlsdr-dev (osmocom fork).
        println!("cargo:rustc-link-lib=rtlsdr");
        println!("cargo:rerun-if-changed=/usr/include/rtl-sdr.h");
        generate_bindings("/usr/include/rtl-sdr.h", None);
    }

    // ── AAC decoder backend ────────────────────────────────────────────────────
    if std::env::var("CARGO_FEATURE_FDK_AAC").is_ok() {
        println!("cargo:rustc-link-lib=fdk-aac");
    }
    println!("cargo:rustc-link-lib=faad");
}
