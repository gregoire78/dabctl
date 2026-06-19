fn main() {
    // Link faad2 only when PCM decode is needed (i.e. not latm-only mode)
    #[cfg(not(feature = "latm-only"))]
    println!("cargo:rustc-link-lib=faad");

    // Link libfec (Phil Karn's RS decoder, same as dablin)
    println!("cargo:rustc-link-lib=fec");

    // Link fdk-aac if feature enabled
    #[cfg(feature = "fdk-aac")]
    println!("cargo:rustc-link-lib=fdk-aac");
}
