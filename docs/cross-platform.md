# Windows and Linux: to do

Status: macOS first. This is the plan for Windows and Linux builds.

## Decisions

- Builds and tests run on GitHub Actions (macOS, Windows, Ubuntu runners).
- Linux target: Ubuntu (GNOME) first.
- Audience: our team. Unsigned builds are fine; users click through the
  first-run warnings (SmartScreen, Gatekeeper).
- No local VMs. Windows and Linux are checked by CI only until someone on
  the team can try a build on a real machine.

## 1. CI

- [x] Workflow: `cargo fmt --check`, `cargo clippy --workspace --all-targets`,
      `cargo test --workspace` on macOS, Windows and Ubuntu:
      `.github/workflows/{macos,windows,linux}.yml`, on pushes to `main` and
      `core-cli`, each also building its packages.
- [x] Ubuntu job: start Synapse (`testserver/`) and run the ignored tests
      (`--ignored`, skipping `big_room`). Snapshots as artifacts.
      `.github/workflows/linux.yml`: fmt,
      clippy, tests, the Synapse tests; UI snapshots render on the CPU (Mesa
      lavapipe). Keyring tests run against GNOME Keyring in a D-Bus session,
      and once without a Secret Service. A second job builds the packages,
      installs the `.deb` and starts the AppImage under Xvfb (window class,
      sign-in screen without a keyring; screenshot as an artifact).

## 2. Windows

CI: `.github/workflows/windows.yml` (clippy, tests, `cargo xtask bundle`;
the packages are the run's artifact).

- [x] `#![windows_subsystem = "windows"]` in `mtrx-app` (no console window).
- [x] Logo in the `.exe`: `cargo xtask bundle` renders `logo.svg` to an
      `.ico` (16 to 256 px) and passes it to `mtrx-app`'s `build.rs`
      (`MTRX_ICON`), which embeds it with version information
      (`winresource`). Plain `cargo build` has no exe icon, only the window
      icon set at run time.
- [x] Notifications: the installer's Start menu shortcut carries the
      AppUserModelID `org.mtrx.Mtrx`; the app sends toasts with that ID
      when the shortcut exists (`mtrx-app/src/windows.rs`). The portable
      zip has no shortcut, so its toasts keep notify-rust's default sender
      (PowerShell). Not yet tried on a real machine.
- [x] Credential Manager holds at most 2560 bytes (1280 UTF-16 characters)
      per secret. A stored session serializes to about 400 characters
      (password login with Synapse's tokens, OAuth with those of MAS, which
      matrix.org uses), but tokens are opaque and have no length limit. So on Windows each secret is a file encrypted
      with XChaCha20-Poly1305 (`secrets.enc` in the profile, `google.enc`)
      and only its random key is in Credential Manager
      (`mtrx-core/src/secrets/windows.rs`). macOS and Linux unchanged.

## 3. Linux

- [x] Secret Service (GNOME Keyring, KWallet) may be missing: show a clear
      error instead of failing to sign in. `Error::NoSecretStore`, shown on
      the sign-in screen at start.
- [x] `.desktop` file with the icon; set the window's application id to
      match it (Wayland), so docks show the logo. Both `org.mtrx.Mtrx`;
      icons from `logo.svg` in the hicolor theme. Not yet seen in a real
      GNOME dock.
- [x] File dialogs (`rfd`) use the XDG portal (its default features; CI
      checks). Not yet tried on a desktop: CI has no portal. Without one,
      rfd falls back to zenity.

## 4. Packages

- [x] `cargo xtask bundle` per platform:
  - [x] Linux (`crates/xtask/src/linux.rs`): `Mtrx-linux-x86_64.AppImage`
        (appimagetool) and `mtrx_<version>_amd64.deb` (app, `mtrx` CLI,
        desktop file, icons, licence), built by the Linux workflow as
        artifacts. Built on Ubuntu 22.04, the oldest we support (glibc
        2.35). The AppImage runtime is pinned (type2-runtime 20251108,
        checksum-verified in CI).
  - [x] Windows: `target/bundle/Mtrx-windows-x64.zip` (portable: `Mtrx.exe`,
        `LICENSE.txt`) and `Mtrx-windows-x64-setup.exe` (Inno Setup; per
        user, no administrator rights, into `%LOCALAPPDATA%\Programs\Mtrx`;
        Start menu shortcut with the AppUserModelID; uninstaller in
        Settings > Apps). Unsigned. The installer is built by CI but not yet
        run on a real machine.
- [x] Release workflow: a version tag builds all three and attaches them to
      a GitHub release. `.github/workflows/release.yml` runs the three
      platform workflows on the tag and makes a draft release with
      `Mtrx-macos-arm64.zip`, `Mtrx-windows-x64-setup.exe`,
      `Mtrx-windows-x64.zip`, `Mtrx-linux-x86_64.AppImage` and
      `Mtrx-linux-amd64.deb`; it is described and published by hand.

## Later

- Signing: Apple Developer ID and notarization; a Windows code-signing
  certificate (e.g. Azure Trusted Signing). Only for a wider audience.
- Flatpak.
