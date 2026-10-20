//! Credential Manager holds at most 2560 bytes per credential, stored as
//! UTF-16: 1280 characters. A stored session is about 400 characters, but
//! its tokens are opaque and have no length limit (e.g. JWTs). So each
//! secret is kept in a file, encrypted with a random key, and only the key
//! is a credential.

use std::{
    fs,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce, aead::Aead};

use super::{Secret, delete_credential, get_credential, set_credential};
use crate::Result;

const NONCE_LEN: usize = 24;

type Key = [u8; 32];

impl Secret {
    pub(super) fn keyring_load(&self) -> Result<Option<String>> {
        let Some(key) = get_credential(&self.name)? else {
            return Ok(None);
        };
        let sealed = match fs::read(self.sealed_file()) {
            Ok(sealed) => sealed,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(Some(open(&decode_key(&key)?, &sealed)?))
    }

    /// Keeps an existing key, so other processes (the app, the CLI) can
    /// always read the file.
    pub(super) fn keyring_save(&self, value: &str) -> Result<()> {
        let key = match get_credential(&self.name)? {
            Some(key) => decode_key(&key)?,
            None => {
                let key: Key = rand::random();
                set_credential(&self.name, &STANDARD.encode(key))?;
                key
            }
        };
        Ok(replace(&self.sealed_file(), &seal(&key, value))?)
    }

    pub(super) fn keyring_delete(&self) -> Result<()> {
        delete_credential(&self.name)?;
        match fs::remove_file(self.sealed_file()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Next to the plain file of [`super::SecretStore::File`].
    fn sealed_file(&self) -> PathBuf {
        self.file.with_extension("enc")
    }
}

/// Nonce followed by the ciphertext of `value`.
fn seal(key: &Key, value: &str) -> Vec<u8> {
    let nonce: [u8; NONCE_LEN] = rand::random();
    let ciphertext = cipher(key)
        .encrypt(&XNonce::from(nonce), value.as_bytes())
        .expect("encrypting in memory");
    [&nonce[..], &ciphertext].concat()
}

fn open(key: &Key, sealed: &[u8]) -> std::io::Result<String> {
    let invalid = || Error::new(ErrorKind::InvalidData, "secret file does not match its key");
    let (nonce, ciphertext) = sealed.split_at_checked(NONCE_LEN).ok_or_else(invalid)?;
    let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| invalid())?;
    let value = cipher(key)
        .decrypt(&XNonce::from(nonce), ciphertext)
        .map_err(|_| invalid())?;
    String::from_utf8(value).map_err(|_| invalid())
}

fn cipher(key: &Key) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(&(*key).into())
}

fn decode_key(encoded: &str) -> std::io::Result<Key> {
    STANDARD
        .decode(encoded)
        .ok()
        .and_then(|key| key.try_into().ok())
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "malformed secret key"))
}

/// Write through a temporary file, so readers never see half a file.
fn replace(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SecretStore;

    /// Longer than a credential can hold.
    fn large_value() -> String {
        "token.".repeat(2000)
    }

    #[test]
    fn sealed_value_opens_with_its_key_only() {
        let (key, other): (Key, Key) = (rand::random(), rand::random());
        let sealed = seal(&key, &large_value());
        assert_eq!(open(&key, &sealed).unwrap(), large_value());
        assert!(open(&other, &sealed).is_err());
        assert!(open(&key, &sealed[..10]).is_err());
    }

    #[test]
    fn large_secret_round_trips_through_credential_manager() {
        let dir = tempfile::tempdir().unwrap();
        let name = format!("mtrx-test:{}", dir.path().display());
        let secret = Secret::new(
            SecretStore::Keyring,
            name.clone(),
            dir.path().join("s.json"),
        );

        // Too large to be a credential itself.
        assert!(set_credential(&name, &large_value()).is_err());

        secret.save(&large_value()).unwrap();
        assert_eq!(secret.load().unwrap(), Some(large_value()));
        secret.save("replaced").unwrap();
        assert_eq!(secret.load().unwrap().as_deref(), Some("replaced"));

        secret.delete().unwrap();
        assert_eq!(secret.load().unwrap(), None);
        assert!(!dir.path().join("s.enc").exists());
        assert_eq!(get_credential(&name).unwrap(), None);
    }
}
