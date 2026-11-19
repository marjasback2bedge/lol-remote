// Re-embed the frontend whenever `web/dist` changes (rust-embed reads it at
// compile time in release builds).
fn main() {
    println!("cargo:rerun-if-changed=../../web/dist");
}
