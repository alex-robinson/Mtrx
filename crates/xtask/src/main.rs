//! Build tasks, run with `cargo xtask <task>`:
//!
//! - `render SVG PNG SIZE`: render an SVG to a square PNG.
//! - `bundle`: build the app and wrap it as `target/bundle/Mtrx.app` (macOS),
//!   an AppImage and a `.deb` (Linux, see [`linux`]), or a portable zip and
//!   an installer (Windows, see `windows.rs`).

#[cfg(target_os = "linux")]
mod linux;

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use resvg::{tiny_skia, usvg};

#[cfg(windows)]
mod windows;

/// The app's logo, also its icon.
const LOGO: &str = "crates/mtrx-app/assets/logo.svg";
/// Also the Linux desktop file's name and the window's application id.
const BUNDLE_ID: &str = "org.mtrx.Mtrx";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = workspace_root();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["render", svg, png, size] => render(Path::new(svg), Path::new(png), size.parse()?),
        #[cfg(target_os = "linux")]
        ["bundle"] => linux::bundle(&root),
        #[cfg(windows)]
        ["bundle"] => windows::bundle(&root),
        #[cfg(not(any(target_os = "linux", windows)))]
        ["bundle"] => bundle(&root),
        _ => bail!("usage: cargo xtask render SVG PNG SIZE | cargo xtask bundle"),
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Render `svg` to a `size`×`size` PNG.
fn render(svg: &Path, png: &Path, size: u32) -> Result<()> {
    rasterize(svg, size)?.save_png(png)?;
    Ok(())
}

/// Render `svg` to `size`×`size` pixels.
fn rasterize(svg: &Path, size: u32) -> Result<tiny_skia::Pixmap> {
    let data = std::fs::read(svg).with_context(|| format!("reading {}", svg.display()))?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default())?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size).context("empty size")?;
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// Build the release app and wrap it with its icon and Info.plist.
#[cfg(not(any(target_os = "linux", windows)))]
fn bundle(root: &Path) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("the app bundle is for macOS");
    }
    run(Command::new(env!("CARGO")).current_dir(root).args([
        "build",
        "--release",
        "-p",
        "mtrx-app",
    ]))?;

    let app = root.join("target/bundle/Mtrx.app");
    if app.exists() {
        std::fs::remove_dir_all(&app)?;
    }
    let contents = app.join("Contents");
    std::fs::create_dir_all(contents.join("MacOS"))?;
    std::fs::create_dir_all(contents.join("Resources"))?;
    std::fs::copy(
        root.join("target/release/mtrx-app"),
        contents.join("MacOS/mtrx-app"),
    )?;
    std::fs::write(contents.join("Info.plist"), info_plist())?;
    // The licence travels with the app.
    std::fs::copy(root.join("LICENSE"), contents.join("Resources/LICENSE"))?;

    // The icon set macOS wants: 16–512 pt at 1× and 2×.
    let iconset = root.join("target/bundle/Mtrx.iconset");
    if iconset.exists() {
        std::fs::remove_dir_all(&iconset)?;
    }
    std::fs::create_dir_all(&iconset)?;
    for points in [16, 32, 128, 256, 512] {
        let logo = root.join(LOGO);
        render(
            &logo,
            &iconset.join(format!("icon_{points}x{points}.png")),
            points,
        )?;
        render(
            &logo,
            &iconset.join(format!("icon_{points}x{points}@2x.png")),
            points * 2,
        )?;
    }
    run(Command::new("iconutil")
        .arg("--convert")
        .arg("icns")
        .arg(&iconset)
        .arg("--output")
        .arg(contents.join("Resources/Mtrx.icns")))?;
    std::fs::remove_dir_all(&iconset)?;

    // Ad-hoc signature: a stable identity for notifications and the keychain.
    run(Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(&app))?;
    println!("{}", app.canonicalize()?.display());
    Ok(())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn info_plist() -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Mtrx</string>
    <key>CFBundleDisplayName</key><string>Mtrx</string>
    <key>CFBundleIdentifier</key><string>{BUNDLE_ID}</string>
    <key>CFBundleExecutable</key><string>mtrx-app</string>
    <key>CFBundleIconFile</key><string>Mtrx</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>{version}</string>
    <key>CFBundleVersion</key><string>{version}</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.social-networking</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
"#
    )
}

fn run(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("running {command:?}"))?;
    if !status.success() {
        bail!("{command:?} failed: {status}");
    }
    Ok(())
}
