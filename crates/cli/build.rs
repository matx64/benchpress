fn main() {
    println!(
        "cargo::rustc-env=BENCHPRESS_TARGET={}",
        std::env::var("TARGET").expect("TARGET")
    );
}
