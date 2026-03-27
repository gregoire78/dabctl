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

    let cmake_status = Command::new("cmake")
        .arg("-S")
        .arg(rtlsdr_dir)
        .arg("-B")
        .arg(&build_dir)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", out_path.display()))
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
    println!("cargo:rustc-link-lib=usb-1.0");

    // Générer les bindings avec bindgen
    let header_path = format!("{}/include/rtl-sdr.h", rtlsdr_dir);
    let include_path = format!("{}/include", rtlsdr_dir);
    
    let bindings = Builder::default()
        .header(&header_path)
        .clang_arg(format!("-I{}", include_path))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_function("rtlsdr.*")
        .allowlist_type("rtlsdr.*")
        .allowlist_var("RTLSDR.*")
        .generate()
        .expect("Unable to generate bindings");

    let out_file = out_path.join("rtlsdr.rs");
    bindings
        .write_to_file(&out_file)
        .expect("Couldn't write bindings!");

    println!("cargo:rustc-env=RTLSDR_BINDINGS={}", out_file.display());
}
