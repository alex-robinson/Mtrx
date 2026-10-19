//! One secret value, kept in the OS keyring or a plain file.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::Result;

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
        Ok(keyring::Entry::new(KEYRING_SERVICE, &self.name)?)
    }
}

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
