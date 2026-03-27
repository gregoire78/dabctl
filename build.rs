use bindgen::Builder;
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = PathBuf::from(&out_dir);

    // Déterminer si on compile en tant que libusb statique ou dynamique
    // Pour simplifier, on utilise cmake pour compiler librtlsdr
    let rtlsdr_dir = "rtl-sdr";
    
    // Compiler librtlsdr avec CMake
    let build_dir = out_path.join("rtlsdr-build");
    std::fs::create_dir_all(&build_dir).ok();

    let target = env::var("TARGET").unwrap_or_default();
    let is_cross_aarch64 = target.contains("aarch64");

    let mut cmake_cmd = Command::new("cmake");
    cmake_cmd
        .arg("-S")
        .arg(rtlsdr_dir)
        .arg("-B")
        .arg(&build_dir)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", out_path.display()));

    if is_cross_aarch64 {
        cmake_cmd
            .arg("-DCMAKE_SYSTEM_NAME=Linux")
            .arg("-DCMAKE_SYSTEM_PROCESSOR=aarch64")
            .arg("-DCMAKE_C_COMPILER=aarch64-linux-gnu-gcc")
            .arg("-DCMAKE_FIND_ROOT_PATH=/usr/aarch64-linux-gnu;/usr")
            .arg("-DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY")
            .arg("-DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=BOTH")
            .arg("-DLIBUSB_LIBRARIES=/usr/lib/aarch64-linux-gnu/libusb-1.0.so")
            .arg("-DLIBUSB_INCLUDE_DIRS=/usr/include/libusb-1.0")
            .env("PKG_CONFIG_PATH", "/usr/lib/aarch64-linux-gnu/pkgconfig")
            .env("PKG_CONFIG_LIBDIR", "/usr/lib/aarch64-linux-gnu/pkgconfig");
    }

    let cmake_status = cmake_cmd
        .status()
        .expect("Failed to run cmake configure");

    if !cmake_status.success() {
        panic!("CMake configuration failed");
    }

    let build_status = Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .arg("--config")
        .arg("Release")
        .status()
        .expect("Failed to run cmake build");

    if !build_status.success() {
        panic!("CMake build failed");
    }

    // Installer les fichiers générés
    let install_status = Command::new("cmake")
        .arg("--install")
        .arg(&build_dir)
        .arg("--config")
        .arg("Release")
        .status()
        .expect("Failed to run cmake install");

    if !install_status.success() {
        panic!("CMake install failed");
    }

    // Chercher les fichiers de bibliothèque
    let lib_path = out_path.join("lib");
    println!("cargo:rustc-link-search=native={}", lib_path.display());
    println!("cargo:rustc-link-lib=rtlsdr");

    // Chercher libusb
    if is_cross_aarch64 {
        println!("cargo:rustc-link-search=native=/usr/lib/aarch64-linux-gnu");
    }
    println!("cargo:rustc-link-lib=usb-1.0");

    // Générer les bindings avec bindgen
    let header_path = format!("{}/include/rtl-sdr.h", rtlsdr_dir);
    let include_path = format!("{}/include", rtlsdr_dir);
    
    let mut builder = Builder::default()
        .header(&header_path)
        .clang_arg(format!("-I{}", include_path))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_function("rtlsdr.*")
        .allowlist_type("rtlsdr.*")
        .allowlist_var("RTLSDR.*");

    if is_cross_aarch64 {
        builder = builder
            .clang_arg("--sysroot=/usr/aarch64-linux-gnu")
            .clang_arg("--target=aarch64-linux-gnu");
    }

    let bindings = builder
        .generate()
        .expect("Unable to generate bindings");

    let out_file = out_path.join("rtlsdr.rs");
    bindings
        .write_to_file(&out_file)
        .expect("Couldn't write bindings!");

    println!("cargo:rustc-env=RTLSDR_BINDINGS={}", out_file.display());
}
