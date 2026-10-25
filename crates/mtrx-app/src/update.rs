//! Updates from the app's GitHub releases: a newer version for the chosen
//! channel, looked for at start and once a day (or from Settings), and the
//! file of it that fits how this copy was installed, installed in its place
//! when signed (see [`install`]), else downloaded in the browser.

mod install;

use std::{
    fmt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use iced::Task;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::app;

/// The releases, newest first; drafts only for the repository's owners.
const RELEASES: &str = "https://api.github.com/repos/alex-robinson/Mtrx/releases";
/// How often the automatic check looks for a newer version.
const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// How often the automatic check is due to be considered.
pub const TICK: Duration = Duration::from_secs(60 * 60);
/// The public key the release files are signed with, from `cargo xtask
/// keygen`; until there is one, updates are downloaded in the browser.
const PUBLIC_KEY: Option<&str> = None;

/// What the settings keep about updates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Look for a newer version at start and once a day.
    pub automatic: bool,
    pub channel: Channel,
    /// The newest version shown in Settings: the gear has no dot for it.
    pub seen: Option<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            automatic: true,
            channel: Channel::Stable,
            seen: None,
        }
    }
}

/// Which releases count as updates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Releases only.
    #[default]
    Stable,
    /// Pre-releases too (versions such as 0.4.0-beta.1).
    Beta,
}

impl Channel {
    pub const ALL: [Self; 2] = [Self::Stable, Self::Beta];
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stable => "Stable",
            Self::Beta => "Beta",
        })
    }
}

/// A version newer than this one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub version: Version,
    /// The release's page, with what changed.
    pub page: String,
    /// The file for how this copy was installed; none if the release has
    /// no such file or this copy is not updated (built from source).
    pub file: Option<File>,
    /// How this copy was installed, and so how it is updated.
    pub install: Install,
}

impl Update {
    /// Whether the app installs it itself: its file is signed and this
    /// copy updates itself.
    pub fn installable(&self) -> bool {
        PUBLIC_KEY.is_some()
            && self.install.updates_itself()
            && self.file.as_ref().is_some_and(|f| f.signature.is_some())
    }
}

/// A file of a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub url: String,
    /// Where its signature is (`cargo xtask sign`), if the release has it.
    pub signature: Option<String>,
}

/// A release as GitHub lists it.
#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// The newest of `releases` newer than `current` on `channel`, with the
/// file for `install`.
fn newest(
    releases: &[Release],
    current: &Version,
    channel: Channel,
    install: &Install,
) -> Option<Update> {
    releases
        .iter()
        .filter(|r| !r.draft && (channel == Channel::Beta || !r.prerelease))
        .filter_map(|r| Some((Version::parse(r.tag_name.strip_prefix('v')?).ok()?, r)))
        .filter(|(version, _)| channel == Channel::Beta || version.pre.is_empty())
        .filter(|(version, _)| version > current)
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(version, release)| Update {
            version,
            page: release.html_url.clone(),
            file: install.file_name().and_then(|name| {
                let url = |name: &str| {
                    let asset = release.assets.iter().find(|a| a.name == name)?;
                    Some(asset.browser_download_url.clone())
                };
                Some(File {
                    name: name.to_owned(),
                    url: url(name)?,
                    signature: url(&format!("{name}.sig")),
                })
            }),
            install: install.clone(),
        })
}

/// This version.
pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the package version is semver")
}

/// Ask GitHub for a version newer than this one on `channel`.
async fn check(channel: Channel) -> Result<Option<Update>, String> {
    let releases: Vec<Release> = reqwest::Client::new()
        .get(RELEASES)
        // GitHub's API refuses requests without one.
        .header(
            reqwest::header::USER_AGENT,
            concat!("Mtrx/", env!("CARGO_PKG_VERSION")),
        )
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|e| format!("Could not reach GitHub: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Could not read GitHub's answer: {e}"))?;
    Ok(newest(&releases, &current(), channel, &Install::detect()))
}

/// How this copy was installed, which decides the file it is updated with
/// and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// `Mtrx.app` on macOS, at this path.
    MacApp(PathBuf),
    /// By the Windows installer, which also updates it.
    WindowsSetup,
    /// `Mtrx.exe` from the Windows zip, at this path.
    WindowsPortable(PathBuf),
    /// The AppImage at this path.
    AppImage(PathBuf),
    /// The Debian package, updated with `apt`.
    Deb,
    /// Built from source, e.g. `cargo run`: not updated.
    Source,
}

impl Install {
    pub fn detect() -> Self {
        let Ok(exe) = std::env::current_exe() else {
            return Self::Source;
        };
        let uninstaller = exe.with_file_name("unins000.exe").exists();
        let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        Self::of(std::env::consts::OS, &exe, appimage, uninstaller)
    }

    /// On `os`, running `exe`; `appimage`: the AppImage it runs from (its
    /// `$APPIMAGE`); `uninstaller`: the Windows installer's uninstaller is
    /// next to it.
    fn of(os: &str, exe: &Path, appimage: Option<PathBuf>, uninstaller: bool) -> Self {
        match os {
            "macos" => {
                let app = exe
                    .parent()
                    .filter(|dir| dir.ends_with("Contents/MacOS"))
                    .and_then(|dir| dir.parent()?.parent())
                    .filter(|app| app.extension().is_some_and(|e| e == "app"));
                app.map_or(Self::Source, |app| Self::MacApp(app.to_owned()))
            }
            "windows" if exe.file_name().is_some_and(|n| n == "Mtrx.exe") => {
                if uninstaller {
                    Self::WindowsSetup
                } else {
                    Self::WindowsPortable(exe.to_owned())
                }
            }
            "linux" => match appimage {
                Some(appimage) => Self::AppImage(appimage),
                None if exe.starts_with("/usr") => Self::Deb,
                None => Self::Source,
            },
            _ => Self::Source,
        }
    }

    /// Whether the app puts a new version in place of this copy.
    fn updates_itself(&self) -> bool {
        matches!(self, Self::MacApp(_))
    }

    /// The name of the release file this copy is updated with.
    fn file_name(&self) -> Option<&'static str> {
        Some(match self {
            Self::MacApp(_) => "Mtrx-macos-arm64.zip",
            Self::WindowsSetup => "Mtrx-windows-x64-setup.exe",
            Self::WindowsPortable(_) => "Mtrx-windows-x64.zip",
            Self::AppImage(_) => "Mtrx-linux-x86_64.AppImage",
            Self::Deb => "Mtrx-linux-amd64.deb",
            Self::Source => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Time to look, if the last automatic check is a day old.
    Tick,
    /// Look now, from Settings or the app menu.
    Check,
    Checked(Result<Option<Update>, String>),
    /// Get the update: install it if signed, else its file in the browser,
    /// or its page.
    Download,
    /// It is in place (the new version starts once this one has quit), or
    /// not.
    Installed(Result<(), String>),
    /// The update's release page, with what changed.
    Notes,
}

/// The updates' state while the app runs.
#[derive(Default)]
pub struct Updater {
    pub checking: bool,
    /// Downloading and installing an update.
    pub installing: bool,
    /// When the last check ended.
    pub checked: Option<Instant>,
    pub available: Option<Update>,
    pub error: Option<String>,
}

impl Updater {
    pub fn update(&mut self, message: Message, preferences: &Preferences) -> Task<app::Message> {
        match message {
            Message::Tick => {
                let due = self.checked.is_none_or(|at| at.elapsed() >= INTERVAL);
                if preferences.automatic && due && !self.checking {
                    return self.check(preferences.channel);
                }
            }
            Message::Check => {
                if !self.checking {
                    return self.check(preferences.channel);
                }
            }
            Message::Checked(result) => {
                self.checking = false;
                self.checked = Some(Instant::now());
                match result {
                    Ok(available) => {
                        self.available = available;
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Message::Download => match (&self.available, PUBLIC_KEY) {
                (Some(update), Some(key)) if update.installable() && !self.installing => {
                    self.installing = true;
                    self.error = None;
                    return Task::perform(install::install(update.clone(), key), |result| {
                        app::Message::Update(Message::Installed(result))
                    });
                }
                (Some(update), _) if !self.installing => {
                    let url = update.file.as_ref().map_or(&update.page, |f| &f.url);
                    let _ = open::that_detached(url);
                }
                _ => {}
            },
            Message::Installed(Ok(())) => {
                // Quit as the window's close button does: the session ends.
                return iced::window::oldest()
                    .and_then(|id| Task::done(app::Message::CloseWindow(id)));
            }
            Message::Installed(Err(e)) => {
                self.installing = false;
                self.error = Some(e);
            }
            Message::Notes => {
                if let Some(update) = &self.available {
                    let _ = open::that_detached(&update.page);
                }
            }
        }
        Task::none()
    }

    fn check(&mut self, channel: Channel) -> Task<app::Message> {
        self.checking = true;
        Task::perform(check(channel), |result| {
            app::Message::Update(Message::Checked(result))
        })
    }

    /// An update not yet shown in Settings: the gear's dot.
    pub fn unseen(&self, preferences: &Preferences) -> bool {
        self.available
            .as_ref()
            .is_some_and(|u| preferences.seen != Some(u.version.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, assets: &[&str]) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease,
            html_url: format!("https://github.com/alex-robinson/Mtrx/releases/tag/{tag}"),
            assets: assets
                .iter()
                .map(|name| Asset {
                    name: (*name).into(),
                    browser_download_url: format!("https://example.org/{tag}/{name}"),
                })
                .collect(),
        }
    }

    #[test]
    fn the_newest_version_of_the_channel() {
        let mac = Install::MacApp("/Applications/Mtrx.app".into());
        let zip = "Mtrx-macos-arm64.zip";
        let mut draft = release("v0.6.0", false, &[zip]);
        draft.draft = true;
        let releases = [
            draft,
            release("v0.5.0-beta.1", true, &[zip]),
            release("v0.4.1", false, &[zip, "Mtrx-macos-arm64.zip.sig"]),
            release("v0.4.0", false, &[]),
            release("v0.3.0", false, &[zip]),
            release("not-a-version", false, &[zip]),
        ];
        let current = Version::new(0, 3, 0);
        let stable = newest(&releases, &current, Channel::Stable, &mac).unwrap();
        assert_eq!(stable.version, Version::new(0, 4, 1));
        assert_eq!(
            stable.file,
            Some(File {
                name: zip.into(),
                url: format!("https://example.org/v0.4.1/{zip}"),
                signature: Some(format!("https://example.org/v0.4.1/{zip}.sig")),
            })
        );
        assert_eq!(stable.install, mac);
        let beta = newest(&releases, &current, Channel::Beta, &mac).unwrap();
        assert_eq!(beta.version.to_string(), "0.5.0-beta.1");
        // A release without this installation's file still shows.
        let windows = newest(&releases, &current, Channel::Stable, &Install::WindowsSetup);
        assert_eq!(windows.map(|u| u.file), Some(None));
        assert_eq!(
            newest(&releases, &Version::new(0, 4, 1), Channel::Stable, &mac),
            None
        );
        // A pre-release version not marked as one is still not stable.
        let unmarked = [release("v0.5.0-rc.1", false, &[zip])];
        assert_eq!(newest(&unmarked, &current, Channel::Stable, &mac), None);
    }

    #[test]
    fn how_this_copy_was_installed() {
        let of = |os, exe: &str, appimage: Option<&str>, uninstaller| {
            Install::of(os, Path::new(exe), appimage.map(PathBuf::from), uninstaller)
        };
        assert_eq!(
            of(
                "macos",
                "/Applications/Mtrx.app/Contents/MacOS/mtrx-app",
                None,
                false
            ),
            Install::MacApp("/Applications/Mtrx.app".into())
        );
        assert_eq!(
            of("macos", "/Users/a/Mtrx/target/debug/mtrx-app", None, false),
            Install::Source
        );
        // Forward slashes, which Windows takes too, split on every host.
        let exe = "C:/Users/a/AppData/Local/Programs/Mtrx/Mtrx.exe";
        assert_eq!(of("windows", exe, None, true), Install::WindowsSetup);
        assert_eq!(
            of("windows", exe, None, false),
            Install::WindowsPortable(exe.into())
        );
        assert_eq!(
            of("windows", "C:/src/target/debug/mtrx-app.exe", None, false),
            Install::Source
        );
        assert_eq!(
            of(
                "linux",
                "/tmp/.mount_x/usr/bin/mtrx-app",
                Some("/home/a/Mtrx.AppImage"),
                false
            ),
            Install::AppImage("/home/a/Mtrx.AppImage".into())
        );
        assert_eq!(of("linux", "/usr/bin/mtrx-app", None, false), Install::Deb);
        assert_eq!(
            of("linux", "/home/a/src/target/debug/mtrx-app", None, false),
            Install::Source
        );
    }
}
