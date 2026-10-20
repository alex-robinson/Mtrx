//! On-disk state of one account: the encrypted SDK store plus its secrets.

use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use matrix_sdk::authentication::{matrix::MatrixSession, oauth::UserSession};
use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    secrets::{Secret, SecretStore},
};

const SECRETS_FILE: &str = "secrets.json";

/// A named account slot. Each profile has its own directory and secrets, so
/// several accounts can be logged in side by side.
#[derive(Clone, Debug)]
pub struct Profile {
    name: String,
    dir: PathBuf,
    secret: Secret,
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

impl StoredAuth {
    pub(crate) fn tokens(self) -> matrix_sdk::SessionTokens {
        match self {
            Self::Password(session) => session.tokens,
            Self::OAuth { session, .. } => session.tokens,
        }
    }
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
        let dir = root.join("profiles").join(name);
        // Keyed by the profile directory, so profiles under different data
        // roots never share a secret.
        let secret = Secret::new(
            secrets,
            dir.to_string_lossy().into_owned(),
            dir.join(SECRETS_FILE),
        );
        Ok(Self {
            name: name.to_owned(),
            dir,
            secret,
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
        match self.secret.load()? {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    pub(crate) fn save_session(&self, stored: &StoredSession) -> Result<()> {
        self.secret.save(&serde_json::to_string(stored)?)
    }

    /// Delete the profile's secrets and all local data.
    pub(crate) fn wipe(&self) -> Result<()> {
        self.secret.delete()?;
        self.remove_data()
    }

    /// Delete the local data (the encrypted store), keeping the secrets.
    pub(crate) fn remove_data(&self) -> Result<()> {
        match fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Forget the session: without its secrets the store cannot be opened.
    pub(crate) fn forget_session(&self) -> Result<()> {
        self.secret.delete()
    }
}
