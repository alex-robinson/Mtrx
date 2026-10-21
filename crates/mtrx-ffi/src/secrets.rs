//! Secret storage supplied by the app.

use std::sync::Arc;

/// Where the library keeps its secrets (access tokens, the store
/// passphrase), implemented by the app: on iOS with the Keychain, on Android
/// with values encrypted by an Android Keystore key. Keys are short names
/// such as `profiles/default`, relative to the data directory. Called on the
/// library's threads, never the main thread; may block briefly.
#[uniffi::export(with_foreign)]
pub trait SecretBackend: Send + Sync {
    /// The secret `key`, or `None` if there is none.
    fn load(&self, key: String) -> Result<Option<String>, SecretError>;
    /// Store `value` as the secret `key`, replacing any earlier one.
    fn save(&self, key: String, value: String) -> Result<(), SecretError>;
    /// Remove the secret `key`; succeeds if there is none.
    fn delete(&self, key: String) -> Result<(), SecretError>;
}

/// A [`SecretBackend`] failure, e.g. the Keychain is locked.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SecretError {
    #[error("{reason}")]
    Failed { reason: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SecretError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed {
            reason: error.reason,
        }
    }
}

/// The app's backend, as mtrx-core's.
pub(crate) struct AppSecrets(pub(crate) Arc<dyn SecretBackend>);

impl mtrx_core::SecretBackend for AppSecrets {
    fn load(&self, key: &str) -> Result<Option<String>, String> {
        self.0.load(key.to_owned()).map_err(|e| e.to_string())
    }

    fn save(&self, key: &str, value: &str) -> Result<(), String> {
        self.0
            .save(key.to_owned(), value.to_owned())
            .map_err(|e| e.to_string())
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        self.0.delete(key.to_owned()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use mtrx_core::SecretBackend as _;

    use super::*;

    /// An app's backend, in memory.
    #[derive(Default)]
    pub(crate) struct Memory(pub(crate) Mutex<HashMap<String, String>>);

    impl SecretBackend for Memory {
        fn load(&self, key: String) -> Result<Option<String>, SecretError> {
            Ok(self.0.lock().unwrap().get(&key).cloned())
        }

        fn save(&self, key: String, value: String) -> Result<(), SecretError> {
            self.0.lock().unwrap().insert(key, value);
            Ok(())
        }

        fn delete(&self, key: String) -> Result<(), SecretError> {
            if key.is_empty() {
                return Err(SecretError::Failed {
                    reason: "no key".into(),
                });
            }
            self.0.lock().unwrap().remove(&key);
            Ok(())
        }
    }

    #[test]
    fn app_backend_serves_the_core() {
        let memory = Arc::new(Memory::default());
        let secrets = AppSecrets(memory.clone());
        secrets.save("profiles/default", "value").unwrap();
        assert_eq!(
            secrets.load("profiles/default").unwrap().as_deref(),
            Some("value")
        );
        secrets.delete("profiles/default").unwrap();
        assert_eq!(secrets.load("profiles/default").unwrap(), None);
        assert_eq!(secrets.delete("").unwrap_err(), "no key");
    }
}
