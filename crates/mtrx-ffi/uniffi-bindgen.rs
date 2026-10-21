//! `cargo run -p mtrx-ffi --bin uniffi-bindgen generate --library <lib> --language swift|kotlin --out-dir <dir>`

fn main() {
    uniffi::uniffi_bindgen_main()
}
