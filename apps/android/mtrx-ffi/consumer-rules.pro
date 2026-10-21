# Called from native code (JNA, UniFFI callbacks, the TLS verifier), which
# R8 cannot see.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.mtrx.** { *; }
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
-keep class org.mtrx.ffi.AndroidPlatform { *; }
# JNA's desktop parts.
-dontwarn java.awt.**
