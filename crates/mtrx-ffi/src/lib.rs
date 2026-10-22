//! Bindings of mtrx-core for the iOS (Swift) and Android (Kotlin) apps,
//! generated with UniFFI; see `docs/mobile.md`.
//!
//! The library runs on its own tokio runtime: async functions run there and
//! are awaited from Swift or Kotlin, and listeners are called on its
//! threads, never on the app's main thread.

// matrix-sdk futures are deeply nested.
#![recursion_limit = "256"]

uniffi::setup_scaffolding!("mtrx");

#[cfg(target_os = "android")]
mod android;
mod client;
mod encryption;
mod error;
mod room_list;
mod runtime;
mod secrets;
mod session;
mod timeline;
mod types;

pub use client::{Mtrx, OAuthLogin};
pub use encryption::{
    DeviceInfo, EncryptionState, EncryptionStateListener, RecoveryListener, RecoveryStatus,
    RecoveryStep, Verification, VerificationEmoji, VerificationListener,
    VerificationRequestListener, VerificationState,
};
pub use error::MtrxError;
pub use runtime::TaskHandle;
pub use secrets::{SecretBackend, SecretError};
pub use session::{ConnectionListener, RoomListListener, Session};
pub use timeline::{Timeline, TimelineListener, TypingListener};
pub use types::*;

/// Where the server sends the browser back after a browser sign-in, e.g.
/// `io.github.alex-robinson:/oauth`. The app registers its scheme (the part
/// before `:`) and passes the full redirect URL to [`OAuthLogin::finish`].
#[uniffi::export]
pub fn oauth_redirect_uri() -> String {
    mtrx_core::APP_OAUTH_REDIRECT_URI.to_owned()
}

/// Log to standard error (the Xcode console; Android discards it), e.g.
/// with `filter` `"info,matrix_sdk=warn"` (`RUST_LOG` syntax). Only the
/// first call has an effect.
#[uniffi::export]
pub fn init_logging(filter: String) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_writer(std::io::stderr)
        .try_init();
}
