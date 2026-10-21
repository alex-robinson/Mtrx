//! The OS credential store, through the keyring crate (desktop only).

use super::Secret;
use crate::{Error, Result};

const KEYRING_SERVICE: &str = "mtrx";

/// What [`Error::NoSecretStore`] tells the user.
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) const NO_SECRET_STORE: &str = "Mtrx keeps your sign-in in the system keyring, \
    but no Secret Service is running. Install GNOME Keyring or KWallet and restart Mtrx";
#[cfg(not(all(unix, not(target_os = "macos"))))]
pub(crate) const NO_SECRET_STORE: &str = "the system keyring is not available";

/// The secret itself is the credential.
#[cfg(not(windows))]
impl Secret {
    pub(super) fn keyring_load(&self) -> Result<Option<String>> {
        get_credential(&self.name)
    }

    pub(super) fn keyring_save(&self, value: &str) -> Result<()> {
        set_credential(&self.name, value)
    }

    pub(super) fn keyring_delete(&self) -> Result<()> {
        delete_credential(&self.name)
    }
}

pub(super) fn get_credential(name: &str) -> Result<Option<String>> {
    match credential(name)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(super) fn set_credential(name: &str, value: &str) -> Result<()> {
    Ok(credential(name)?.set_password(value)?)
}

/// Succeeds if there is none.
pub(super) fn delete_credential(name: &str) -> Result<()> {
    match credential(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn credential(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, name).map_err(|e| match e {
        // The store failed to open; the reason is kept separately.
        keyring::Error::NoDefaultStore => {
            Error::NoSecretStore(match keyring::Entry::store_status() {
                Err(cause) => cause.to_string(),
                Ok(()) => e.to_string(),
            })
        }
        e => e.into(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::SecretStore;

    fn keyring_secret() -> Secret {
        let name = format!("mtrx-test-{}", std::process::id());
        Secret::new(SecretStore::Keyring, name, String::new(), PathBuf::new())
    }

    #[test]
    #[ignore = "uses the OS keyring"]
    fn keyring_round_trip() {
        let secret = keyring_secret();
        secret.save("value").unwrap();
        assert_eq!(secret.load().unwrap().as_deref(), Some("value"));
        secret.delete().unwrap();
        assert_eq!(secret.load().unwrap(), None);
    }

    /// Run with no Secret Service, e.g. `DBUS_SESSION_BUS_ADDRESS=unix:path=/none`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs a session without a Secret Service"]
    fn keyring_missing() {
        let error = keyring_secret().load().unwrap_err();
        assert!(matches!(error, Error::NoSecretStore(_)), "{error}");
        assert!(error.to_string().starts_with(NO_SECRET_STORE), "{error}");
    }
}
