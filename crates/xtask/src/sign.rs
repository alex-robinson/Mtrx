//! Signatures of the release files, which the app checks before it updates
//! itself (`crates/mtrx-app/src/update.rs`).
//!
//! - `keygen`: print a new key pair: the private key for the repository's
//!   `MTRX_UPDATE_KEY` secret, the public key for the app.
//! - `sign FILE...`: write `FILE.sig` next to each file (the signature in
//!   base64), with the private key in `MTRX_UPDATE_KEY`.

use std::fs;

use anyhow::{Context, Result, anyhow};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};

pub fn keygen() -> Result<()> {
    // The thread's generator is cryptographically secure, seeded by the OS.
    let secret: [u8; 32] = rand::random();
    let key = SigningKey::from_bytes(&secret);
    println!("Private key, for the MTRX_UPDATE_KEY secret (keep a copy safe; show nobody):");
    println!("{}", STANDARD.encode(secret));
    println!("Public key, for PUBLIC_KEY in crates/mtrx-app/src/update.rs:");
    println!("{}", STANDARD.encode(key.verifying_key().to_bytes()));
    Ok(())
}

pub fn sign(files: &[&str]) -> Result<()> {
    let key = std::env::var("MTRX_UPDATE_KEY")
        .context("MTRX_UPDATE_KEY: the private key from `cargo xtask keygen`")?;
    let key = signing_key(&key)?;
    for file in files {
        let data = fs::read(file).with_context(|| format!("reading {file}"))?;
        let signature = format!("{file}.sig");
        fs::write(&signature, signature_of(&key, &data))?;
        println!("{signature}");
    }
    Ok(())
}

/// The key from its base64 text.
fn signing_key(text: &str) -> Result<SigningKey> {
    let secret: [u8; 32] = STANDARD
        .decode(text.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| anyhow!("MTRX_UPDATE_KEY is not a key from `cargo xtask keygen`"))?;
    Ok(SigningKey::from_bytes(&secret))
}

/// `data`'s signature in base64, as the app reads it.
fn signature_of(key: &SigningKey, data: &[u8]) -> String {
    STANDARD.encode(key.sign(data).to_bytes())
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, VerifyingKey};

    use super::*;

    #[test]
    fn signatures_verify_with_the_public_key() {
        let key = signing_key(&STANDARD.encode([7u8; 32])).unwrap();
        let signature = signature_of(&key, b"Mtrx");
        let bytes: [u8; 64] = STANDARD.decode(signature).unwrap().try_into().unwrap();
        let public = VerifyingKey::from_bytes(&key.verifying_key().to_bytes()).unwrap();
        let signature = Signature::from_bytes(&bytes);
        assert!(public.verify_strict(b"Mtrx", &signature).is_ok());
        assert!(public.verify_strict(b"Mtrx!", &signature).is_err());
        assert!(signing_key("not a key").is_err());
    }
}
