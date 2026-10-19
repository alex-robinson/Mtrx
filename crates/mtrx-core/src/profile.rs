//! On-disk state of one account: the encrypted SDK store plus its secrets.

use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use matrix_sdk::authentication::{matrix::MatrixSession, oauth::UserSession};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const KEYRING_SERVICE: &str = "mtrx";
const SECRETS_FILE: &str = "secrets.json";

/// Where a profile's secrets (access token, store passphrase) are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretStore {
    /// OS credential store: macOS Keychain, Windows Credential Manager, or
    /// Secret Service on Linux.
    Keyring,
    /// Plain-text file in the profile directory. For tests and development only.
    File,
}

/// A named account slot. Each profile has its own directory and secrets, so
/// several accounts can be logged in side by side.
#[derive(Clone, Debug)]
pub struct Profile {
    name: String,
    dir: PathBuf,
    secrets: SecretStore,
}

/// Everything needed to restore a session, stored as one secret.
#[derive(Serialize, Deserialize)]
pub(crate) struct StoredSession {
    pub homeserver: String,
    pub auth: StoredAuth,
    pub store_passphrase: String,
}

/// Tokens of the login method that was used.
#[derive(Serialize, Deserialize)]
pub(crate) enum StoredAuth {
    Password(MatrixSession),
    /// Browser login via the server's OAuth 2.0 provider.
    OAuth {
        client_id: String,
        session: UserSession,
    },
}

impl Profile {
    /// Profile `name` under `data_root` (default: [`Profile::default_data_root`]).
    pub fn new(name: &str, data_root: Option<&Path>, secrets: SecretStore) -> Result<Self> {
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(Error::InvalidProfileName(name.to_owned()));
        }
        let root = match data_root {
            Some(root) => root.to_owned(),
            None => Self::default_data_root()?,
        };
        Ok(Self {
            name: name.to_owned(),
            dir: root.join("profiles").join(name),
            secrets,
        })
    }

    /// Platform data directory, e.g. `~/Library/Application Support/org.mtrx.Mtrx`.
    pub fn default_data_root() -> Result<PathBuf> {
        ProjectDirs::from("org", "mtrx", "Mtrx")
            .map(|dirs| dirs.data_dir().to_owned())
            .ok_or(Error::NoDataDir)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn store_dir(&self) -> PathBuf {
        self.dir.join("store")
    }

    pub(crate) fn load_session(&self) -> Result<Option<StoredSession>> {
        let json = match self.secrets {
            SecretStore::Keyring => match self.keyring_entry()?.get_password() {
                Ok(json) => json,
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(e) => return Err(e.into()),
            },
            SecretStore::File => match fs::read_to_string(self.dir.join(SECRETS_FILE)) {
                Ok(json) => json,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.into()),
            },
        };
        Ok(Some(serde_json::from_str(&json)?))
    }

    pub(crate) fn save_session(&self, stored: &StoredSession) -> Result<()> {
        let json = serde_json::to_string(stored)?;
        match self.secrets {
            SecretStore::Keyring => self.keyring_entry()?.set_password(&json)?,
            SecretStore::File => write_private(&self.dir.join(SECRETS_FILE), &json)?,
        }
        Ok(())
    }

    /// Delete the profile's secrets and all local data.
    pub(crate) fn wipe(&self) -> Result<()> {
        if self.secrets == SecretStore::Keyring {
            match self.keyring_entry()?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e.into()),
            }
        }
        match fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Keyed by the profile directory, so profiles under different data roots
    /// never share a secret.
    fn keyring_entry(&self) -> Result<keyring::Entry> {
        Ok(keyring::Entry::new(
            KEYRING_SERVICE,
            &self.dir.to_string_lossy(),
        )?)
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
