fn main() {
    // Select AAC decoder backend(s) at compile time.
    // Inspired by AbracaDABra (KejPi, MIT licence) USE_FDKAAC cmake option.
    //
    // Without `fdk-aac` feature: link faad2 only.
    // With `fdk-aac` feature: link BOTH libraries — both backends are compiled
    // and the user selects between them at runtime via --aac-decoder.
    if std::env::var("CARGO_FEATURE_FDK_AAC").is_ok() {
        println!("cargo:rustc-link-lib=fdk-aac");
    }
    println!("cargo:rustc-link-lib=faad");
}
