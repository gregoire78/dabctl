fn main() {
    // Link audio decoding libraries (for eti2pcm subcommand)
    println!("cargo:rustc-link-lib=faad");
}
