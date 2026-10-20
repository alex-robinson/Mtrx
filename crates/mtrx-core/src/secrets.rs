//! One secret value, kept in the OS keyring or a plain file.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{Error, Result};

const KEYRING_SERVICE: &str = "mtrx";

/// Where secrets (access tokens, store passphrases) are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretStore {
    /// OS credential store: macOS Keychain, Windows Credential Manager, or
    /// Secret Service on Linux.
    Keyring,
    /// Plain-text file in the data directory. For tests and development only.
    File,
}

/// A secret slot: keyring entry `name` or `file`, depending on the store.
#[derive(Clone, Debug)]
pub(crate) struct Secret {
    store: SecretStore,
    name: String,
    file: PathBuf,
}

impl Secret {
    pub(crate) fn new(store: SecretStore, name: String, file: PathBuf) -> Self {
        Self { store, name, file }
    }

    pub(crate) fn load(&self) -> Result<Option<String>> {
        match self.store {
            SecretStore::Keyring => match self.keyring_entry()?.get_password() {
                Ok(value) => Ok(Some(value)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(e.into()),
            },
            SecretStore::File => match fs::read_to_string(&self.file) {
                Ok(value) => Ok(Some(value)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.into()),
            },
        }
    }

    pub(crate) fn save(&self, value: &str) -> Result<()> {
        match self.store {
            SecretStore::Keyring => self.keyring_entry()?.set_password(value)?,
            SecretStore::File => write_private(&self.file, value)?,
        }
        Ok(())
    }

    /// Remove the secret; succeeds if there is none.
    pub(crate) fn delete(&self) -> Result<()> {
        match self.store {
            SecretStore::Keyring => match self.keyring_entry()?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e.into()),
            },
            SecretStore::File => match fs::remove_file(&self.file) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            },
        }
    }

    fn keyring_entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, &self.name).map_err(|e| match e {
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
}

/// What [`Error::NoSecretStore`] tells the user.
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) const NO_SECRET_STORE: &str = "Mtrx keeps your sign-in in the system keyring, \
    but no Secret Service is running. Install GNOME Keyring or KWallet and restart Mtrx";
#[cfg(not(all(unix, not(target_os = "macos"))))]
pub(crate) const NO_SECRET_STORE: &str = "the system keyring is not available";

fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyring_secret() -> Secret {
        let name = format!("mtrx-test-{}", std::process::id());
        Secret::new(SecretStore::Keyring, name, PathBuf::new())
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
