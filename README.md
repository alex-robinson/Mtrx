# Mtrx
Matrix-protocol communication app

Native Rust Matrix client for macOS, Windows and Linux, built on
[matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk).

## Layout

- `crates/mtrx-core`: Matrix logic and state (sessions, room list, timelines,
  encryption). No UI; exposes plain types and live update streams.
- `crates/mtrx-format`: Markdown/LaTeX to Matrix HTML, HTML to a document
  model, math typesetting with Typst.
- `crates/mtrx-cli`: the `mtrx` command-line client on top of core.
- `crates/mtrx-app`: the desktop app (iced).
- `testserver/`: local Synapse for development and tests.

## Desktop app

```sh
cargo run --release -p mtrx-app
```

## CLI

```sh
cargo run -p mtrx-cli -- --help
cargo run -p mtrx-cli -- login --homeserver matrix.org --user alice
cargo run -p mtrx-cli -- rooms
cargo run -p mtrx-cli -- read "Room name" -n 20
cargo run -p mtrx-cli -- send "Room name" "Hello **world**"
cargo run -p mtrx-cli -- watch "Room name"
cargo run -p mtrx-cli -- verify          # emoji verification with another device
cargo run -p mtrx-cli -- recovery recover
```

`--profile NAME` selects an account. Secrets go to the OS keychain;
`--secret-store file` keeps them in a plain file instead (development only).

## Local test server

Needs Docker (on macOS e.g. `brew install colima docker && colima start`).

```sh
testserver/run.sh                  # Synapse on http://localhost:8008
testserver/add_user.sh alice alicepw
cargo test -p mtrx-core --test integration -- --ignored
cargo test -p mtrx-app -- --ignored   # UI snapshots in target/ui-snapshots/
```
