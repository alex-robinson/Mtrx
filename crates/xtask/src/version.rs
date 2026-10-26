//! `cargo xtask version X.Y.Z[-beta.N]`: the version in the workspace's
//! `Cargo.toml` and, made from it and committed, in the iOS and Android
//! projects, as the Xcode and Gradle builds run no Rust. A test checks that
//! the apps' are current. iOS shows a beta as its release, `X.Y.Z`, and tells
//! the betas apart by the build number.

use std::{path::Path, process::Command};

use anyhow::{Context, Result, bail};

use super::run;

const CARGO: &str = "Cargo.toml";
const ANDROID: &str = "apps/android/app/build.gradle.kts";
/// Its app and test targets, each built as Debug and Release.
const IOS: &str = "apps/ios/Mtrx.xcodeproj/project.pbxproj";
const IOS_CONFIGURATIONS: usize = 4;

/// Set the version everywhere, and in `Cargo.lock`.
pub fn version(root: &Path, version: &str) -> Result<()> {
    let version = Version::parse(version)?;
    for (path, text) in outputs(root, &version)? {
        std::fs::write(root.join(path), text).with_context(|| format!("writing {path}"))?;
        println!("{path}");
    }
    run(Command::new(env!("CARGO"))
        .current_dir(root)
        .args(["update", "--workspace"]))?;
    println!("Cargo.lock");
    Ok(())
}

fn outputs(root: &Path, version: &Version) -> Result<Vec<(&'static str, String)>> {
    let code = version.code();
    Ok(vec![
        (
            CARGO,
            set(
                &read(root, CARGO)?,
                &[("version = ", format!("\"{version}\""), 1)],
            )?,
        ),
        (
            ANDROID,
            set(
                &read(root, ANDROID)?,
                &[
                    ("versionCode = ", code.to_string(), 1),
                    ("versionName = ", format!("\"{version}\""), 1),
                ],
            )?,
        ),
        (
            IOS,
            set(
                &read(root, IOS)?,
                &[
                    (
                        "CURRENT_PROJECT_VERSION = ",
                        format!("{code};"),
                        IOS_CONFIGURATIONS,
                    ),
                    (
                        "MARKETING_VERSION = ",
                        format!("{};", version.release()),
                        IOS_CONFIGURATIONS,
                    ),
                ],
            )?,
        ),
    ])
}

/// `MAJOR.MINOR.PATCH`, or a beta of it, `MAJOR.MINOR.PATCH-beta.N`.
pub struct Version {
    release: [u32; 3],
    beta: Option<u32>,
}

impl Version {
    /// This build's, from the workspace's `Cargo.toml`.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("the workspace's version")
    }

    fn parse(text: &str) -> Result<Self> {
        let (release, beta) = match text.split_once('-') {
            Some((release, pre)) => match pre.strip_prefix("beta.").map(str::parse) {
                Some(Ok(n @ 1..=98)) => (release, Some(n)),
                _ => bail!("{text:?}: a pre-release is `-beta.N`, N from 1 to 98"),
            },
            None => (text, None),
        };
        let parts = release
            .split('.')
            .map(str::parse)
            .collect::<Result<Vec<u32>, _>>()
            .ok()
            .and_then(|parts| <[u32; 3]>::try_from(parts).ok());
        match parts {
            Some(release @ [_, minor, patch]) if minor < 100 && patch < 100 => {
                Ok(Self { release, beta })
            }
            _ => bail!("{text:?} is not a version MAJOR.MINOR.PATCH, MINOR and PATCH below 100"),
        }
    }

    /// `MAJOR.MINOR.PATCH` without the beta, as Apple takes only numbers:
    /// iOS's and macOS's short version.
    pub fn release(&self) -> String {
        let [major, minor, patch] = self.release;
        format!("{major}.{minor}.{patch}")
    }

    /// The build number, Android's version code and iOS's and macOS's bundle
    /// version: rising with the version, as Android takes a lower one for a
    /// downgrade, and through its betas, the release itself counting as 99.
    pub fn code(&self) -> u32 {
        let [major, minor, patch] = self.release;
        major * 1_000_000 + minor * 10_000 + patch * 100 + self.beta.unwrap_or(99)
    }

    /// The `.deb`'s, the beta after a `~` to sort before the release.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn debian(&self) -> String {
        match self.beta {
            Some(n) => format!("{}~beta.{n}", self.release()),
            None => self.release(),
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.release())?;
        match self.beta {
            Some(n) => write!(f, "-beta.{n}"),
            None => Ok(()),
        }
    }
}

fn read(root: &Path, path: &str) -> Result<String> {
    std::fs::read_to_string(root.join(path)).with_context(|| format!("reading {path}"))
}

/// The lines starting with `key` after their indentation, without it.
fn lines_starting<'a>(text: &'a str, key: &str) -> Vec<&'a str> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with(key))
        .collect()
}

/// `text` with each line starting `key` (after its indentation) ending in
/// `value` instead, for each `(key, value, count)`, expecting `count` such
/// lines. Indentation and line endings stay.
fn set(text: &str, settings: &[(&str, String, usize)]) -> Result<String> {
    for (key, _, count) in settings {
        let found = lines_starting(text, key).len();
        if found != *count {
            bail!("expected {count} lines starting `{key}`, found {found}");
        }
    }
    Ok(text
        .split_inclusive('\n')
        .map(|line| {
            let body = line.trim_end_matches(['\r', '\n']);
            let ending = &line[body.len()..];
            let indent = &body[..body.len() - body.trim_start().len()];
            match settings
                .iter()
                .find(|(key, _, _)| body.trim_start().starts_with(key))
            {
                Some((key, value, _)) => format!("{indent}{key}{value}{ending}"),
                None => line.to_string(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The workspace's version, from `Cargo.toml`.
    fn current(root: &Path) -> Result<Version> {
        let toml = read(root, CARGO)?;
        let [line] = lines_starting(&toml, "version = ")[..] else {
            bail!("{CARGO} needs one line starting `version = `");
        };
        Version::parse(line.trim_start_matches("version = ").trim_matches('"'))
    }

    /// As in the icons' test, line endings may differ by checkout.
    #[test]
    fn apps_have_the_workspace_version() {
        let root = crate::workspace_root();
        let lines = |text: &str| text.replace("\r\n", "\n");
        let version = current(&root).unwrap();
        for (path, made) in outputs(&root, &version).unwrap() {
            let committed = read(&root, path).unwrap();
            assert!(
                lines(&committed) == lines(&made),
                "{path} does not have version {version}; run `cargo xtask version {version}`"
            );
        }
    }

    #[test]
    fn versions_parse_and_number() {
        let version = Version::parse("0.1.4").unwrap();
        assert_eq!(version.to_string(), "0.1.4");
        assert_eq!(version.code(), 10_499);
        assert_eq!(Version::parse("1.12.3").unwrap().code(), 1_120_399);
        let beta = Version::parse("0.4.0-beta.1").unwrap();
        assert_eq!(beta.to_string(), "0.4.0-beta.1");
        assert_eq!(beta.release(), "0.4.0");
        assert_eq!(beta.debian(), "0.4.0~beta.1");
        assert_eq!(beta.code(), 40_001);
        // Rising through the betas to the release and on to the next.
        let codes = [
            "0.3.0",
            "0.4.0-beta.1",
            "0.4.0-beta.2",
            "0.4.0",
            "0.4.1-beta.1",
        ]
        .map(|v| Version::parse(v).unwrap().code());
        assert!(codes.is_sorted_by(|a, b| a < b), "{codes:?}");
        for bad in [
            "0.1",
            "0.1.4.5",
            "0.1.x",
            "0.100.0",
            "v0.1.4",
            "0.4.0-beta",
            "0.4.0-beta.0",
            "0.4.0-beta.99",
            "0.4.0-rc.1",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad}");
        }
    }
}
