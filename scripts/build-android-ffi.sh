#!/usr/bin/env bash
# Build mtrx-ffi for Android (arm64-v8a devices, x86_64 emulators) into the
# Gradle module apps/android/mtrx-ffi: src/main/jniLibs/<abi>/libmtrx_ffi.so
# and the Kotlin bindings src/main/kotlin/uniffi/mtrx/mtrx.kt.
#   scripts/build-android-ffi.sh          optimized (profile `mobile`)
#   scripts/build-android-ffi.sh --debug  faster to build
# Needs the Android NDK (ANDROID_NDK_HOME), cargo-ndk (cargo install
# cargo-ndk) and `rustup target add aarch64-linux-android x86_64-linux-android`.
set -euo pipefail
cd "$(dirname "$0")/.."

profile=mobile
dir=mobile
if [[ "${1:-}" == "--debug" ]]; then
  profile=dev
  dir=debug
fi
# Lowest Android API level the library supports.
api=26

module=apps/android/mtrx-ffi/src/main
rm -rf "$module/jniLibs"
# Android ABI and Rust target.
for pair in arm64-v8a:aarch64-linux-android x86_64:x86_64-linux-android; do
  abi="${pair%%:*}"
  target="${pair#*:}"
  cargo ndk --target "$abi" --platform "$api" build -p mtrx-ffi --lib --profile "$profile"
  mkdir -p "$module/jniLibs/$abi"
  cp "target/$target/$dir/libmtrx_ffi.so" "$module/jniLibs/$abi/"
done

rm -rf "$module/kotlin/uniffi"
cargo run -q -p mtrx-ffi --bin uniffi-bindgen -- generate \
  --library "target/aarch64-linux-android/$dir/libmtrx_ffi.so" \
  --language kotlin --out-dir "$module/kotlin" --no-format
find "$module/jniLibs" "$module/kotlin" -type f
