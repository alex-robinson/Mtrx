`iced_core` 0.14.0 (MIT, https://github.com/iced-rs/iced), vendored to
carry our patches to its input method types (see `docs/native-behaviour.md`,
section B1). Each is marked "Mtrx patch" in the code:

- `src/input_method.rs`, `src/text/editor.rs` (surrounding text): macOS
  replaces a letter held for its accents with the accent picked, by a
  range in the text around the caret, which iced could neither tell nor
  apply. `InputMethod::Enabled` now carries a `SurroundingText` (an excerpt
  around the selection, with the cursor and anchor in bytes),
  `input_method::Event` has `DeleteSurrounding`, and `Edit` has
  `DeleteSurrounding` to apply it. In the shape of winit 0.31's
  `ImeSurroundingText` and `Ime::DeleteSurrounding`
  (https://github.com/rust-windowing/winit, `winit-core`), so this goes
  once iced moves to winit 0.31.
- `src/text/editor.rs` (editing, `docs/native-behaviour.md` B2): `Edit`
  has `Undo` and `Redo`; the history is kept by the renderer's editor
  (`iced_graphics`).
