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
- [ ] Ubuntu job: start Synapse (`testserver/`) and run the ignored tests
      (`--ignored`, skipping `big_room`). Snapshots as artifacts.

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

- [ ] Secret Service (GNOME Keyring, KWallet) may be missing: show a clear
      error instead of failing to sign in.
- [ ] `.desktop` file with the icon; set the window's application id to
      match it (Wayland), so docks show the logo.
- [ ] File dialogs (`rfd`) use the XDG portal: check on Ubuntu.

## 4. Packages

- [ ] `cargo xtask bundle` per platform: Windows installer (Start menu
      shortcut with the AppUserModelID, uninstall), Linux AppImage and
      `.deb`.
- [ ] Release workflow: a version tag builds all three and attaches them to
      a GitHub release.

## Later

- Signing: Apple Developer ID and notarization; a Windows code-signing
  certificate (e.g. Azure Trusted Signing). Only for a wider audience.
- Flatpak.
