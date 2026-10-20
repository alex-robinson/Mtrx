# Mtrx
Matrix-protocol communication app

Website and download: <https://alex-robinson.github.io/Mtrx/>

Native Rust Matrix client for macOS, Windows and Linux, built on
[matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk).

## Layout

- `crates/mtrx-core`: Matrix logic and state (sessions, room list, timelines,
  encryption). No UI; exposes plain types and live update streams.
- `crates/mtrx-format`: Markdown/LaTeX to Matrix HTML, HTML to a document
  model, math typesetting with Typst.
- `crates/mtrx-cli`: the `mtrx` command-line client on top of core.
- `crates/mtrx-app`: the desktop app (iced).
- `crates/xtask`: build tasks (`cargo xtask bundle`).
- `testserver/`: local Synapse for development and tests.

## Desktop app

```sh
cargo run --release -p mtrx-app
```

On macOS, `cargo xtask bundle` builds `target/bundle/Mtrx.app` with its icon
(`crates/mtrx-app/assets/logo.svg`); drag it to Applications.

## CLI

```sh
cargo run -p mtrx-cli -- --help
cargo run -p mtrx-cli -- login --homeserver matrix.org --user alice
cargo run -p mtrx-cli -- rooms
cargo run -p mtrx-cli -- read "Room name" -n 20
cargo run -p mtrx-cli -- send "Room name" "Hello **world**"
cargo run -p mtrx-cli -- watch "Room name"
cargo run -p mtrx-cli -- schedule "Room name" "tomorrow 9:00" "Good morning"
cargo run -p mtrx-cli -- scheduled       # list; then unschedule ID / send-now ID
cargo run -p mtrx-cli -- note "Idea: compare basal melt"   # a note to self
cargo run -p mtrx-cli -- notes --since 2026-10-01 -o notes.md  # notes as Markdown
cargo run -p mtrx-cli -- verify          # emoji verification with another device
cargo run -p mtrx-cli -- recovery recover
```

Scheduled messages are kept (encrypted) on this device and sent while the app
or `mtrx watch` runs. One whose time passed while neither ran is held as
overdue until you send or cancel it.

Rooms and spaces created with Mtrx trust their members: anyone may invite,
rename, change the topic and picture, pin messages and set the call link.
Removing people, deleting others' messages and changing permissions stay with
admins. `--moderated` gives the usual Matrix rules instead, and
`let-everyone-edit ROOM_OR_SPACE` opens existing rooms (needs admin rights).

`--profile NAME` selects an account. Secrets go to the OS keychain;
`--secret-store file` keeps them in a plain file instead (development only).

## Google Meet links

Mtrx can create Google Meet links with your own Google OAuth client; it ships
no Google credentials. One-time setup in the
[Google Cloud console](https://console.cloud.google.com/):

1. Create a project.
2. APIs & Services > Library: enable "Google Meet REST API".
3. APIs & Services > OAuth consent screen: user type External; add your
   Google account under Test users.
4. APIs & Services > Credentials > Create credentials > OAuth client ID,
   application type "Desktop app". Note the client ID and secret.

Then sign in once (or later in the app's Settings):

```sh
cargo run -p mtrx-cli -- google login --client-id ID   # prompts for the secret
cargo run -p mtrx-cli -- google meet                   # prints a new Meet link
cargo run -p mtrx-cli -- google status                 # or: google logout
```

As long as the app is unverified ("Testing" status), only the listed test
users can sign in. The Google sign-in is app-wide, not per profile.

## Local test server

Needs Docker (on macOS e.g. `brew install colima docker && colima start`).

```sh
testserver/run.sh                  # Synapse on http://localhost:8008
testserver/add_user.sh alice alicepw
cargo test -p mtrx-core --test integration -- --ignored
cargo test -p mtrx-app -- --ignored   # UI snapshots in target/ui-snapshots/
```
