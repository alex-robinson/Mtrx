#!/usr/bin/env bash
# Build mtrx-ffi for this machine and generate its Swift and Kotlin bindings
# into target/bindings/{swift,kotlin}, to check the API goes through UniFFI.
# The apps get theirs from scripts/build-{ios,android}-ffi.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

case "$(uname -s)" in
  Darwin) lib=target/debug/libmtrx_ffi.dylib ;;
  *) lib=target/debug/libmtrx_ffi.so ;;
esac
cargo build -p mtrx-ffi --lib
rm -rf target/bindings
for language in swift kotlin; do
  cargo run -q -p mtrx-ffi --bin uniffi-bindgen -- generate \
    --library "$lib" --language "$language" --out-dir "target/bindings/$language" --no-format
done
find target/bindings -type f
