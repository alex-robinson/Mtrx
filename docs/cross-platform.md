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

- [ ] Workflow: `cargo fmt --check`, `cargo clippy --workspace --all-targets`,
      `cargo test --workspace` on macOS, Windows and Ubuntu.
- [x] Ubuntu job: start Synapse (`testserver/`) and run the ignored tests
      (`--ignored`, skipping `big_room`). Snapshots as artifacts.
      `.github/workflows/linux.yml` (push to `linux`, or run by hand): fmt,
      clippy, tests, the Synapse tests; UI snapshots render on the CPU (Mesa
      lavapipe). Keyring tests run against GNOME Keyring in a D-Bus session,
      and once without a Secret Service. A second job builds the packages,
      installs the `.deb` and starts the AppImage under Xvfb (window class,
      sign-in screen without a keyring; screenshot as an artifact).

## 2. Windows

- [ ] `#![windows_subsystem = "windows"]` in `mtrx-app` (no console window).
- [ ] Logo in the `.exe` (an `.ico` from `logo.svg` via `cargo xtask`,
      embedded in `build.rs`).
- [ ] Notifications: need an AppUserModelID registered by a Start menu
      shortcut. Set the same ID in the app (`notify-rust`), or toasts show
      as PowerShell and clicks do not reach the app.
- [ ] Credential Manager holds at most 2560 bytes per secret. Measure the
      stored session (password and OAuth logins). If it can exceed that,
      keep the session in an encrypted file and only its key in the
      credential store.

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

- [ ] `cargo xtask bundle` per platform: Windows installer (Start menu
      shortcut with the AppUserModelID, uninstall), Linux AppImage and
      `.deb`. Linux done (`crates/xtask/src/linux.rs`):
      `Mtrx-linux-x86_64.AppImage` (appimagetool) and
      `mtrx_<version>_amd64.deb` (app, `mtrx` CLI, desktop file, icons,
      licence), built by the Linux workflow as artifacts. Built on Ubuntu
      22.04, the oldest we support (glibc 2.35). The AppImage runtime is
      pinned (type2-runtime 20251108, checksum-verified in CI).
- [ ] Release workflow: a version tag builds all three and attaches them to
      a GitHub release.

## Later

- Signing: Apple Developer ID and notarization; a Windows code-signing
  certificate (e.g. Azure Trusted Signing). Only for a wider audience.
- Flatpak.
