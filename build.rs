fn main() {
    // Link audio decoding libraries (for eti2pcm subcommand)
    println!("cargo:rustc-link-lib=faad");
    println!("cargo:rustc-link-lib=mpg123");
}
