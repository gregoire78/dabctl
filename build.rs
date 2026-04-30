fn main() {
    // Link faad2 (default AAC decoder)
    println!("cargo:rustc-link-lib=faad");

    // Link libfec (Phil Karn's RS decoder, same as dablin)
    println!("cargo:rustc-link-lib=fec");

    // Link fdk-aac if feature enabled
    #[cfg(feature = "fdk-aac")]
    println!("cargo:rustc-link-lib=fdk-aac");
}
