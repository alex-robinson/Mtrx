//! `cargo xtask bundle` on Windows: in `target/bundle`, a portable
//! `Mtrx-windows-x64.zip` and a per-user installer
//! `Mtrx-windows-x64-setup.exe` (Inno Setup).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use resvg::tiny_skia;

use super::{BUNDLE_ID, LOGO, rasterize, run};

/// Icon sizes in the `.ico`: the small ones for lists and the taskbar, 256
/// for large icon views.
const ICON_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];
/// Identifies the app to the installer across versions (upgrades, uninstall).
const APP_GUID: &str = "1994920B-0736-4ED3-8E97-F50A7D1BC63D";

/// Build the release app with its icon and package it.
pub fn bundle(root: &Path) -> Result<()> {
    let out = root.join("target/bundle");
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    let stage = out.join("Mtrx");
    std::fs::create_dir_all(&stage)?;

    // The build script of mtrx-app embeds the icon given in `MTRX_ICON`.
    let icon = out.join("Mtrx.ico");
    std::fs::write(&icon, ico(&root.join(LOGO))?)?;
    run(Command::new(env!("CARGO"))
        .current_dir(root)
        .env("MTRX_ICON", &icon)
        .args(["build", "--release", "-p", "mtrx-app"]))?;
    std::fs::copy(
        root.join("target/release/mtrx-app.exe"),
        stage.join("Mtrx.exe"),
    )?;
    // The licence travels with the app.
    std::fs::copy(root.join("LICENSE"), stage.join("LICENSE.txt"))?;

    run(Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg("Compress-Archive -Path $env:STAGE -DestinationPath $env:ZIP")
        .env("STAGE", &stage)
        .env("ZIP", out.join("Mtrx-windows-x64.zip")))?;

    let script = out.join("Mtrx.iss");
    std::fs::write(&script, inno_script())?;
    run(Command::new(iscc()).arg("/Q").arg(&script))?;

    for file in ["Mtrx-windows-x64.zip", "Mtrx-windows-x64-setup.exe"] {
        println!("{}", out.join(file).display());
    }
    Ok(())
}

/// The logo as an `.ico`: 32-bit bitmaps for the small sizes, PNG for 256
/// (the format Windows expects for each).
fn ico(svg: &Path) -> Result<Vec<u8>> {
    let images = ICON_SIZES
        .iter()
        .map(|&size| {
            let pixmap = rasterize(svg, size)?;
            if size >= 256 {
                pixmap.encode_png().context("encoding PNG")
            } else {
                Ok(bitmap(&pixmap))
            }
        })
        .collect::<Result<Vec<_>>>()?;

    let count = images.len() as u16;
    let mut ico = Vec::new();
    // ICONDIR: reserved, type 1 (icon), image count.
    for field in [0, 1, count] {
        ico.extend(field.to_le_bytes());
    }
    // ICONDIRENTRY per image; the images follow the directory.
    let mut offset = 6 + 16 * images.len();
    for (&size, image) in ICON_SIZES.iter().zip(&images) {
        // 0 stands for 256.
        let side = if size >= 256 { 0 } else { size as u8 };
        ico.extend([side, side, 0, 0]);
        ico.extend(1u16.to_le_bytes()); // colour planes
        ico.extend(32u16.to_le_bytes()); // bits per pixel
        ico.extend((image.len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += image.len();
    }
    for image in images {
        ico.extend(image);
    }
    Ok(ico)
}

/// A 32-bit BGRA device-independent bitmap as stored in an `.ico`: rows
/// bottom-up, height doubled for the (unused, all-zero) AND mask.
fn bitmap(pixmap: &tiny_skia::Pixmap) -> Vec<u8> {
    let (width, height) = (pixmap.width(), pixmap.height());
    let mask_row = width.div_ceil(32) * 4;
    let mut bmp = Vec::new();
    // BITMAPINFOHEADER
    bmp.extend(40u32.to_le_bytes());
    bmp.extend((width as i32).to_le_bytes());
    bmp.extend((2 * height as i32).to_le_bytes());
    bmp.extend(1u16.to_le_bytes()); // planes
    bmp.extend(32u16.to_le_bytes()); // bits per pixel
    bmp.extend(0u32.to_le_bytes()); // uncompressed
    bmp.extend((height * (width * 4 + mask_row)).to_le_bytes());
    bmp.extend([0; 16]); // resolution, palette
    for row in pixmap.pixels().chunks(width as usize).rev() {
        for pixel in row {
            let c = pixel.demultiply();
            bmp.extend([c.blue(), c.green(), c.red(), c.alpha()]);
        }
    }
    bmp.resize(bmp.len() + (height * mask_row) as usize, 0);
    bmp
}

/// Inno Setup's compiler: on `PATH`, or where its installer puts it.
fn iscc() -> PathBuf {
    [
        ("ProgramFiles(x86)", r"Inno Setup 6\ISCC.exe"),
        ("ProgramFiles", r"Inno Setup 6\ISCC.exe"),
        ("LOCALAPPDATA", r"Programs\Inno Setup 6\ISCC.exe"),
    ]
    .iter()
    .filter_map(|(var, file)| Some(PathBuf::from(std::env::var_os(var)?).join(file)))
    .find(|path| path.exists())
    .unwrap_or_else(|| "iscc".into())
}

/// Per-user install (no administrator rights) into
/// `%LOCALAPPDATA%\Programs\Mtrx`, with a Start menu shortcut carrying the
/// app's AppUserModelID, so notifications show as Mtrx, and an uninstaller.
/// The app looks for this shortcut (`crates/mtrx-app/src/windows.rs`).
fn inno_script() -> String {
    r#"[Setup]
AppId={{@GUID@}
AppName=Mtrx
AppVersion=@VERSION@
AppPublisher=Mtrx
AppPublisherURL=https://github.com/alex-robinson/Mtrx
DefaultDirName={autopf}\Mtrx
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=.
OutputBaseFilename=Mtrx-windows-x64-setup
SetupIconFile=Mtrx.ico
UninstallDisplayIcon={app}\Mtrx.exe
UninstallDisplayName=Mtrx
WizardStyle=modern
Compression=lzma2
SolidCompression=yes

[Files]
Source: "Mtrx\Mtrx.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "Mtrx\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Mtrx"; Filename: "{app}\Mtrx.exe"; AppUserModelID: "@APP_ID@"

[Run]
Filename: "{app}\Mtrx.exe"; Description: "{cm:LaunchProgram,Mtrx}"; Flags: nowait postinstall skipifsilent
"#
    .replace("@GUID@", APP_GUID)
    .replace("@VERSION@", env!("CARGO_PKG_VERSION"))
    .replace("@APP_ID@", BUNDLE_ID)
}
