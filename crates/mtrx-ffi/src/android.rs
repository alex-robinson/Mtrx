//! Android: reqwest verifies certificates with rustls-platform-verifier,
//! which calls Android's verifier through the JVM and so needs the app's
//! Context. Kotlin calls `org.mtrx.ffi.AndroidPlatform.initialize(context)`
//! (`apps/android/mtrx-ffi/src/main/kotlin/org/mtrx/ffi/AndroidPlatform.kt`)
//! once, before any network use.

use jni::{
    EnvUnowned,
    errors::ThrowRuntimeExAndDefault,
    jni_mangle,
    objects::{JClass, JObject},
};

#[jni_mangle("org.mtrx.ffi.AndroidPlatform")]
pub fn initialize<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    context: JObject<'caller>,
) {
    env.with_env(|env| rustls_platform_verifier::android::init_with_env(env, context))
        .resolve::<ThrowRuntimeExAndDefault>();
}
