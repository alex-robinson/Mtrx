//! On-disk state of one account: the encrypted SDK store plus its secrets.

use std::{
    fs,
    path::{Path, PathBuf},
};

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
    /// Data the OS may delete; `dir` unless [`Profile::with_cache_root`].
    cache_dir: PathBuf,
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
    /// Profile `name` under `data_root`, e.g. [`Profile::default_data_root`].
    pub fn new(name: &str, data_root: &Path, secrets: SecretStore) -> Result<Self> {
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(Error::InvalidProfileName(name.to_owned()));
        }
        let dir = data_root.join("profiles").join(name);
        // Keyed by the profile directory, so profiles under different data
        // roots never share a secret.
        let secret = Secret::new(
            secrets,
            dir.to_string_lossy().into_owned(),
            format!("profiles/{name}"),
            dir.join(SECRETS_FILE),
        );
        Ok(Self {
            name: name.to_owned(),
            cache_dir: dir.clone(),
            dir,
            secret,
        })
    }

    /// Keep caches (the SDK's event and media caches) under `cache_root`
    /// instead of the data root: a directory the OS may clear, such as the
    /// Caches directory on iOS or the cache directory on Android.
    pub fn with_cache_root(mut self, cache_root: &Path) -> Self {
        self.cache_dir = cache_root.join("profiles").join(&self.name);
        self
    }

    /// Desktop data directory, e.g. `~/Library/Application Support/org.mtrx.Mtrx`.
    /// The mobile apps pass the platform's directories instead.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn default_data_root() -> Result<PathBuf> {
        directories::ProjectDirs::from("org", "mtrx", "Mtrx")
            .map(|dirs| dirs.data_dir().to_owned())
            .ok_or(Error::NoDataDir)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The profile's cache directory: its own under the cache root, else [`Profile::dir`].
    /// Deleted with the profile's other local data.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub(crate) fn store_dir(&self) -> PathBuf {
        self.dir.join("store")
    }

    /// The SDK's caches; the same as [`Profile::store_dir`] without a cache root.
    pub(crate) fn cache_store_dir(&self) -> PathBuf {
        self.cache_dir.join("store")
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

    /// Delete the local data (the encrypted store and caches), keeping the secrets.
    pub(crate) fn remove_data(&self) -> Result<()> {
        remove_dir(&self.cache_dir)?;
        remove_dir(&self.dir)
    }

    /// Forget the session: without its secrets the store cannot be opened.
    pub(crate) fn forget_session(&self) -> Result<()> {
        self.secret.delete()
    }
}

/// Succeeds if there is none.
fn remove_dir(dir: &Path) -> Result<()> {
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_stay_with_the_data_without_a_cache_root() {
        let profile = Profile::new("p", Path::new("/data"), SecretStore::File).unwrap();
        assert_eq!(profile.dir(), Path::new("/data/profiles/p"));
        assert_eq!(profile.cache_dir(), profile.dir());
        assert_eq!(profile.cache_store_dir(), profile.store_dir());
    }

    #[test]
    fn cache_root_holds_the_caches_and_is_removed_with_the_data() {
        let (data, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let profile = Profile::new("p", data.path(), SecretStore::File)
            .unwrap()
            .with_cache_root(cache.path());
        assert_eq!(profile.cache_dir(), cache.path().join("profiles/p"));
        assert_eq!(profile.store_dir(), data.path().join("profiles/p/store"));
        assert_eq!(
            profile.cache_store_dir(),
            cache.path().join("profiles/p/store")
        );

        fs::create_dir_all(profile.store_dir()).unwrap();
        fs::create_dir_all(profile.cache_store_dir()).unwrap();
        profile.remove_data().unwrap();
        assert!(!profile.dir().exists());
        assert!(!profile.cache_dir().exists());
        // Nothing left to remove.
        profile.remove_data().unwrap();
    }

    #[test]
    fn invalid_names_are_rejected() {
        for name in ["", "a/b", "..", "ä"] {
            let error = Profile::new(name, Path::new("/data"), SecretStore::File).unwrap_err();
            assert!(
                matches!(error, Error::InvalidProfileName(_)),
                "{name}: {error}"
            );
        }
    }
}
