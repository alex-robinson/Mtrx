#!/usr/bin/env bash
# Build mtrx-ffi for iOS (device and simulator, both arm64) into the Swift
# package apps/ios/MtrxFFI: mtrxFFI.xcframework (static library, C header,
# module map) and Sources/MtrxFFI/mtrx.swift (the bindings). The app adds
# the package as a local package and imports MtrxFFI.
#   scripts/build-ios-ffi.sh          optimized (profile `mobile`)
#   scripts/build-ios-ffi.sh --debug  faster to build
# Needs Xcode and `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`.
set -euo pipefail
cd "$(dirname "$0")/.."

profile=mobile
dir=mobile
if [[ "${1:-}" == "--debug" ]]; then
  profile=dev
  dir=debug
fi
# The package's minimum (Package.swift).
export IPHONEOS_DEPLOYMENT_TARGET=16.0

targets=(aarch64-apple-ios aarch64-apple-ios-sim)
for target in "${targets[@]}"; do
  cargo build -p mtrx-ffi --lib --profile "$profile" --target "$target"
done

package=apps/ios/MtrxFFI
generated="$(mktemp -d)"
trap 'rm -rf "$generated"' EXIT
cargo run -q -p mtrx-ffi --bin uniffi-bindgen -- generate \
  --library "target/aarch64-apple-ios/$dir/libmtrx_ffi.a" \
  --language swift --out-dir "$generated"

# An xcframework's headers carry their module map as module.modulemap.
headers="$generated/headers"
mkdir -p "$headers"
cp "$generated/mtrxFFI.h" "$headers/"
cp "$generated/mtrxFFI.modulemap" "$headers/module.modulemap"

rm -rf "$package/mtrxFFI.xcframework"
args=()
for target in "${targets[@]}"; do
  args+=(-library "target/$target/$dir/libmtrx_ffi.a" -headers "$headers")
done
xcodebuild -create-xcframework "${args[@]}" -output "$package/mtrxFFI.xcframework"

mkdir -p "$package/Sources/MtrxFFI"
cp "$generated/mtrx.swift" "$package/Sources/MtrxFFI/mtrx.swift"
echo "Swift package: $package"
