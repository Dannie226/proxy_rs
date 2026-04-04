fn main() {
    println!("cargo::rustc-link-lib=ssl");
    println!("cargo::rustc-link-lib=crypto");
}
