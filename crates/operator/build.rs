//! Build script for operator

fn main() {
    // Track proto files for changes
    println!("cargo:rerun-if-changed=../proto/");
}
