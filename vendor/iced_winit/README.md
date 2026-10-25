`iced_winit` 0.14.1 (MIT, https://github.com/iced-rs/iced), vendored to
carry our patches to its input method handling (see
`docs/native-behaviour.md`, section B1). Each is marked "Mtrx patch" in the
code:

- `src/window.rs` (surrounding text): takes the `InputMethod` of our
  `iced_core`, with its surrounding text.
