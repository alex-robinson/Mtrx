//! `cargo xtask bundle` on Linux: an AppImage and a Debian package in
//! `target/bundle/`.
//!
//! Both install the same tree: the binaries, a desktop file named after the
//! app id (the window's application id, so docks show the logo) and the logo
//! as icons. The AppImage needs `appimagetool` on the `PATH`; the package
//! needs `dpkg-deb` and `dpkg-shlibdeps` (package `dpkg-dev`).

use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use super::{BUNDLE_ID as APP_ID, LOGO, render, run};

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Icon sizes for the hicolor theme.
const ICON_SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];
const MAINTAINER: &str = "Alexander Robinson <alexander.robinson@awi.de>";
const HOMEPAGE: &str = "https://github.com/alex-robinson/Mtrx";

/// Build the release binaries, then the AppImage and the `.deb`.
pub fn bundle(root: &Path) -> Result<()> {
    run(Command::new(env!("CARGO")).current_dir(root).args([
        "build",
        "--release",
        "-p",
        "mtrx-app",
        "-p",
        "mtrx-cli",
    ]))?;
    let out = root.join("target/bundle");
    fs::create_dir_all(&out)?;
    let appimage = appimage(root, &out)?;
    let deb = deb(root, &out)?;
    println!("{}", appimage.canonicalize()?.display());
    println!("{}", deb.canonicalize()?.display());
    Ok(())
}

/// `Mtrx-linux-<arch>.AppImage`: the app (not the CLI) in an AppDir.
fn appimage(root: &Path, out: &Path) -> Result<PathBuf> {
    let appdir = fresh_dir(&out.join("Mtrx.AppDir"))?;
    install(root, &appdir.join("usr"), &["mtrx-app"])?;
    // The AppDir's top level: entry point, desktop file and icon.
    symlink("usr/bin/mtrx-app", appdir.join("AppRun"))?;
    fs::write(appdir.join(format!("{APP_ID}.desktop")), desktop_file())?;
    symlink(
        format!("usr/share/icons/hicolor/256x256/apps/{APP_ID}.png"),
        appdir.join(format!("{APP_ID}.png")),
    )?;
    symlink(format!("{APP_ID}.png"), appdir.join(".DirIcon"))?;

    let arch = std::env::consts::ARCH;
    let appimage = out.join(format!("Mtrx-linux-{arch}.AppImage"));
    run(Command::new("appimagetool")
        .env("ARCH", arch)
        .arg("--no-appstream")
        .arg(&appdir)
        .arg(&appimage))?;
    fs::remove_dir_all(&appdir)?;
    Ok(appimage)
}

/// `mtrx_<version>_<arch>.deb` with the app and the `mtrx` CLI.
fn deb(root: &Path, out: &Path) -> Result<PathBuf> {
    let arch = deb_arch()?;
    let name = format!("mtrx_{VERSION}_{arch}");
    let staging = fresh_dir(&out.join(&name))?;
    let usr = staging.join("usr");
    install(root, &usr, &["mtrx-app", "mtrx"])?;
    let doc = usr.join("share/doc/mtrx");
    fs::create_dir_all(&doc)?;
    write_file(
        &doc.join("copyright"),
        &fs::read(root.join("LICENSE"))?,
        0o644,
    )?;

    let binaries = ["mtrx-app", "mtrx"].map(|b| usr.join("bin").join(b));
    let control = format!(
        "Package: mtrx\n\
         Version: {VERSION}\n\
         Architecture: {arch}\n\
         Maintainer: {MAINTAINER}\n\
         Installed-Size: {size}\n\
         Depends: {depends}\n\
         Recommends: gnome-keyring | kwalletmanager, xdg-desktop-portal, fonts-noto-color-emoji\n\
         Section: net\n\
         Priority: optional\n\
         Homepage: {HOMEPAGE}\n\
         Description: Matrix client\n \
         Mtrx is a native desktop app for the Matrix chat network, with a\n \
         command-line client (mtrx).\n",
        size = disk_usage(&usr)?.div_ceil(1024),
        depends = depends(out, &binaries)?,
    );
    let debian = staging.join("DEBIAN");
    fs::create_dir_all(&debian)?;
    write_file(&debian.join("control"), control.as_bytes(), 0o644)?;

    let deb = out.join(format!("{name}.deb"));
    run(Command::new("dpkg-deb")
        .args(["--root-owner-group", "--build"])
        .arg(&staging)
        .arg(&deb))?;
    fs::remove_dir_all(&staging)?;
    Ok(deb)
}

/// Install `binaries`, the desktop file and the icons under `prefix`
/// (`/usr` once installed).
fn install(root: &Path, prefix: &Path, binaries: &[&str]) -> Result<()> {
    let bin = prefix.join("bin");
    fs::create_dir_all(&bin)?;
    for binary in binaries {
        let built = root.join("target/release").join(binary);
        let data = fs::read(&built).with_context(|| format!("reading {}", built.display()))?;
        write_file(&bin.join(binary), &data, 0o755)?;
    }

    let applications = prefix.join("share/applications");
    fs::create_dir_all(&applications)?;
    write_file(
        &applications.join(format!("{APP_ID}.desktop")),
        desktop_file().as_bytes(),
        0o644,
    )?;

    let logo = root.join(LOGO);
    let icons = prefix.join("share/icons/hicolor");
    for size in ICON_SIZES {
        let dir = icons.join(format!("{size}x{size}/apps"));
        fs::create_dir_all(&dir)?;
        let png = dir.join(format!("{APP_ID}.png"));
        render(&logo, &png, size)?;
        fs::set_permissions(&png, fs::Permissions::from_mode(0o644))?;
    }
    let scalable = icons.join("scalable/apps");
    fs::create_dir_all(&scalable)?;
    write_file(
        &scalable.join(format!("{APP_ID}.svg")),
        &fs::read(&logo)?,
        0o644,
    )?;
    Ok(())
}

/// The desktop entry. Its name, `<APP_ID>.desktop`, and `StartupWMClass`
/// match the window's application id (Wayland) and class (X11).
fn desktop_file() -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Mtrx\n\
         GenericName=Matrix Client\n\
         Comment=Chat on Matrix\n\
         Exec=mtrx-app\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         Categories=Network;InstantMessaging;Chat;\n\
         Keywords=Matrix;Chat;Messaging;\n\
         StartupNotify=true\n\
         StartupWMClass={APP_ID}\n"
    )
}

/// The package's dependencies: the shared libraries the binaries link,
/// from `dpkg-shlibdeps`, and those the app loads at run time (keyboard
/// layouts; Vulkan or OpenGL for drawing).
fn depends(out: &Path, binaries: &[PathBuf]) -> Result<String> {
    // dpkg-shlibdeps reads a debian/control from its working directory.
    let dir = fresh_dir(&out.join("shlibdeps"))?;
    fs::create_dir_all(dir.join("debian"))?;
    fs::write(
        dir.join("debian/control"),
        "Source: mtrx\n\nPackage: mtrx\nArchitecture: any\n",
    )?;
    let mut command = Command::new("dpkg-shlibdeps");
    command.current_dir(&dir).arg("-O");
    for binary in binaries {
        command.arg(format!("-e{}", binary.display()));
    }
    let output = command
        .output()
        .with_context(|| format!("running {command:?}"))?;
    if !output.status.success() {
        bail!(
            "{command:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_dir_all(&dir)?;
    let stdout = String::from_utf8(output.stdout)?;
    let linked = stdout
        .lines()
        .find_map(|line| line.strip_prefix("shlibs:Depends="))
        .context("no shlibs:Depends from dpkg-shlibdeps")?;
    Ok(format!("{linked}, libxkbcommon0, libvulkan1 | libegl1"))
}

fn deb_arch() -> Result<&'static str> {
    Ok(match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        arch => bail!("no Debian architecture for {arch}"),
    })
}

/// Bytes of the files under `dir`.
fn disk_usage(dir: &Path) -> Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        total += if meta.is_dir() {
            disk_usage(&entry.path())?
        } else {
            meta.len()
        };
    }
    Ok(total)
}

/// `dir`, empty.
fn fresh_dir(dir: &Path) -> Result<PathBuf> {
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)?;
    Ok(dir.to_owned())
}

fn write_file(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}
