fn main() {
    // Always needed for DAB+ RS decode.
    println!("cargo:rustc-link-lib=fec");

    // AAC decode is not required in latm-only flavor.
    if std::env::var_os("CARGO_FEATURE_LATM_ONLY").is_none() {
        println!("cargo:rustc-link-lib=faad");
    }

    if std::env::var_os("CARGO_FEATURE_FDK_AAC").is_some() {
        println!("cargo:rustc-link-lib=fdk-aac");
    }
}
