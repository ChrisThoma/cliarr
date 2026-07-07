fn main() {
    // The compile-time target triple picks the matching release asset in
    // `cliarr update`; cfg! reconstruction can't recover the full triple.
    println!("cargo:rustc-env=CLIARR_TARGET={}", std::env::var("TARGET").unwrap());
}
