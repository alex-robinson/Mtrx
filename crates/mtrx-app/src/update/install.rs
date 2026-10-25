//! Installing an update in place of this copy: its file downloaded and its
//! signature checked, then as this copy was installed. The new version
//! starts once this one has quit.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};

use super::{File, Install, Update};

/// Download `update`'s file, check it against `public_key` and put it in
/// place of this copy; the new version starts once this process has ended.
pub async fn install(update: Update, public_key: &'static str) -> Result<(), String> {
    let file = update
        .file
        .ok_or("The release has no file for this installation.")?;
    let data = download(&file.url).await?;
    let signature = file
        .signature
        .as_deref()
        .ok_or("The release is not signed.")?;
    let signature = String::from_utf8(download(signature).await?)
        .map_err(|_| "The release's signature is not text.".to_owned())?;
    verify(&data, &signature, public_key)?;
    tokio::task::spawn_blocking(move || put_in_place(&update.install, &file, &data))
        .await
        .map_err(|e| e.to_string())?
}

async fn download(url: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::get(url)
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|e| format!("Could not download the update: {e}"))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Could not download the update: {e}"))?;
    Ok(bytes.to_vec())
}

/// Whether `signature` (base64, from `cargo xtask sign`) is `data`'s, made
/// with the private key of `public_key` (base64).
fn verify(data: &[u8], signature: &str, public_key: &str) -> Result<(), String> {
    let decode = |text: &str| STANDARD.decode(text.trim()).ok();
    let key = decode(public_key)
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .and_then(|bytes| VerifyingKey::from_bytes(&bytes).ok())
        .ok_or("The app's update key is not valid.")?;
    let signature = decode(signature)
        .and_then(|bytes| <[u8; 64]>::try_from(bytes).ok())
        .ok_or("The release's signature is not valid.")?;
    key.verify_strict(data, &Signature::from_bytes(&signature))
        .map_err(|_| "The update's signature does not match: not installed.".to_owned())
}

fn put_in_place(install: &Install, file: &File, data: &[u8]) -> Result<(), String> {
    match install {
        Install::MacApp(app) => replace_app(app, file, data),
        _ => Err("This installation does not update itself.".to_owned()),
    }
}

/// Unpack the zipped `Mtrx.app` next to `app`, swap it in, and start it
/// once this process has ended. Without permission to write there, the zip
/// is saved in Downloads and shown in Finder instead.
fn replace_app(app: &Path, file: &File, data: &[u8]) -> Result<(), String> {
    let dir = app.parent().ok_or("The app is not in a folder.")?;
    // On the app's volume, so that the swap is two renames.
    let staging = dir.join(".mtrx-update");
    let staged = std::fs::remove_dir_all(&staging)
        .or_else(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(e),
        })
        .and_then(|()| std::fs::create_dir(&staging));
    if staged.is_err() {
        return reveal_in_downloads(file, data);
    }
    let result = unpack_and_swap(app, &staging, file, data);
    let _ = std::fs::remove_dir_all(&staging);
    result?;
    relaunch_after_exit(app)
}

fn unpack_and_swap(app: &Path, staging: &Path, file: &File, data: &[u8]) -> Result<(), String> {
    let zip = staging.join(&file.name);
    std::fs::write(&zip, data).map_err(|e| e.to_string())?;
    let unpacked = staging.join("new");
    // ditto keeps the bundle's links, permissions and signature.
    let status = Command::new("ditto")
        .arg("-x")
        .arg("-k")
        .arg(&zip)
        .arg(&unpacked)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Could not unpack the update.".to_owned());
    }
    swap(app, &unpacked.join("Mtrx.app"), &staging.join("old.app"))
}

/// Put `new` in place of `app`, moving `app` to `old`; if that fails, `app`
/// stays as it was.
fn swap(app: &Path, new: &Path, old: &Path) -> Result<(), String> {
    if !new.join("Contents/MacOS").is_dir() {
        return Err("The update has no Mtrx.app.".to_owned());
    }
    std::fs::rename(app, old).map_err(|e| format!("Could not replace Mtrx: {e}"))?;
    std::fs::rename(new, app).map_err(|e| {
        let _ = std::fs::rename(old, app);
        format!("Could not replace Mtrx: {e}")
    })
}

/// The zip in the user's Downloads, shown in Finder, to install by hand.
fn reveal_in_downloads(file: &File, data: &[u8]) -> Result<(), String> {
    let downloads = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Downloads"))
        .ok_or("No Downloads folder.")?;
    let zip = downloads.join(&file.name);
    std::fs::write(&zip, data).map_err(|e| e.to_string())?;
    let _ = Command::new("open").arg("-R").arg(&zip).status();
    Err(format!(
        "Mtrx cannot replace itself where it is; the update is in Downloads: {}",
        file.name
    ))
}

/// Start `app` once this process has ended (it holds the account's stores).
fn relaunch_after_exit(app: &Path) -> Result<(), String> {
    Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; open "$2""#)
        .arg("sh")
        .arg(std::process::id().to_string())
        .arg(app)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not start the new version: {e}"))
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;

    #[test]
    fn only_a_matching_signature_verifies() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let public = STANDARD.encode(key.verifying_key().to_bytes());
        let signature = STANDARD.encode(key.sign(b"Mtrx").to_bytes());
        assert_eq!(verify(b"Mtrx", &signature, &public), Ok(()));
        assert!(verify(b"Mtrx!", &signature, &public).is_err());
        assert!(verify(b"Mtrx", "not base64", &public).is_err());
        let other = STANDARD.encode(SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes());
        assert!(verify(b"Mtrx", &signature, &other).is_err());
    }

    #[test]
    fn the_new_app_takes_the_old_ones_place() {
        let dir = tempfile::TempDir::new().unwrap();
        let app = dir.path().join("Mtrx.app");
        let new = dir.path().join("new/Mtrx.app");
        let old = dir.path().join("old.app");
        for (bundle, version) in [(&app, "old"), (&new, "new")] {
            std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
            std::fs::write(bundle.join("Contents/version"), version).unwrap();
        }
        swap(&app, &new, &old).unwrap();
        assert_eq!(
            std::fs::read_to_string(app.join("Contents/version")).unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read_to_string(old.join("Contents/version")).unwrap(),
            "old"
        );

        // Not a bundle: nothing moves.
        let empty = dir.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        assert!(swap(&app, &empty, &dir.path().join("older.app")).is_err());
        assert_eq!(
            std::fs::read_to_string(app.join("Contents/version")).unwrap(),
            "new"
        );
    }

    /// The release's zip, as `ditto` makes it in CI, unpacked and swapped in.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_zipped_app_replaces_this_one() {
        let dir = tempfile::TempDir::new().unwrap();
        let build = dir.path().join("build/Mtrx.app");
        std::fs::create_dir_all(build.join("Contents/MacOS")).unwrap();
        std::fs::write(build.join("Contents/version"), "new").unwrap();
        let zip = dir.path().join("Mtrx-macos-arm64.zip");
        let zipped = Command::new("ditto")
            .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
            .arg(&build)
            .arg(&zip)
            .status()
            .unwrap();
        assert!(zipped.success());

        let app = dir.path().join("Applications/Mtrx.app");
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        let staging = dir.path().join("Applications/.mtrx-update");
        std::fs::create_dir(&staging).unwrap();
        let file = File {
            name: "Mtrx-macos-arm64.zip".into(),
            url: String::new(),
            signature: None,
        };
        unpack_and_swap(&app, &staging, &file, &std::fs::read(&zip).unwrap()).unwrap();
        let version = std::fs::read_to_string(app.join("Contents/version")).unwrap();
        assert_eq!(version, "new");
    }
}
