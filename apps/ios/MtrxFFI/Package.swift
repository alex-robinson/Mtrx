// swift-tools-version:5.9
// Mtrx's Rust library and its Swift bindings, both built by
// scripts/build-ios-ffi.sh: mtrxFFI.xcframework and Sources/MtrxFFI/mtrx.swift.
import PackageDescription

let package = Package(
    name: "MtrxFFI",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "MtrxFFI", targets: ["MtrxFFI"]),
    ],
    targets: [
        // The static library, its C header and module map (module mtrxFFI).
        .binaryTarget(name: "mtrxFFI", path: "mtrxFFI.xcframework"),
        // The generated Swift API.
        .target(
            name: "MtrxFFI",
            dependencies: ["mtrxFFI"],
            path: "Sources/MtrxFFI",
            linkerSettings: [
                // TLS certificate checks (rustls-platform-verifier).
                .linkedFramework("Security"),
                // Proxy settings (reqwest).
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreFoundation"),
            ]
        ),
    ]
)
