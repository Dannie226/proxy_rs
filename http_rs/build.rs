fn main() {
    println!("cargo::rustc-link-search=./http/target/debug/");
    println!("cargo::rustc-link-lib=http");
}
