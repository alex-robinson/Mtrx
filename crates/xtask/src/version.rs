//! `cargo xtask version X.Y.Z`: the version in the workspace's `Cargo.toml`
//! and, made from it and committed, in the iOS and Android projects, as the
//! Xcode and Gradle builds run no Rust. A test checks that the apps' are
//! current.

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
                        format!("{version};"),
                        IOS_CONFIGURATIONS,
                    ),
                ],
            )?,
        ),
    ])
}

/// `MAJOR.MINOR.PATCH`.
struct Version([u32; 3]);

impl Version {
    fn parse(text: &str) -> Result<Self> {
        let parts = text
            .split('.')
            .map(str::parse)
            .collect::<Result<Vec<u32>, _>>()
            .ok()
            .and_then(|parts| <[u32; 3]>::try_from(parts).ok());
        match parts {
            Some(parts @ [_, minor, patch]) if minor < 100 && patch < 100 => Ok(Self(parts)),
            _ => bail!("{text:?} is not a version MAJOR.MINOR.PATCH, MINOR and PATCH below 100"),
        }
    }

    /// The build number, Android's version code and iOS's bundle version:
    /// rising with the version, as Android takes a lower one for a downgrade.
    fn code(&self) -> u32 {
        let [major, minor, patch] = self.0;
        major * 10_000 + minor * 100 + patch
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let [major, minor, patch] = self.0;
        write!(f, "{major}.{minor}.{patch}")
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
        assert_eq!(version.code(), 104);
        assert_eq!(Version::parse("1.12.3").unwrap().code(), 11_203);
        for bad in ["0.1", "0.1.4.5", "0.1.x", "0.100.0", "v0.1.4"] {
            assert!(Version::parse(bad).is_err(), "{bad}");
        }
    }
}
