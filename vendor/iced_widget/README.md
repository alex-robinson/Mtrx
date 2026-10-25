`iced_widget` 0.14.2 (MIT, https://github.com/iced-rs/iced), vendored to
carry our patches to iced's widgets (see `docs/native-behaviour.md`,
section B). Each is marked "Mtrx patch" in the code:

- `src/stack.rs`: `Stack::update` stopped at any captured event, also one
  captured before the stack by a widget beside it, so a press in the
  timeline never reached the composer under its paperclip and smiley, which
  kept the focus and took Shift with the arrows. The layers now get a shell
  of their own.
- `src/text_editor.rs`: the key bindings are asked also when the editor
  has no focus, and a custom binding was published then: Enter sent the
  composer's draft with nothing focused. Custom bindings now act only
  while focused, as the default ones.
- `src/text_editor.rs`, `src/text_input.rs`: macOS gives a key's text also
  with ⌘, so ⌘K typed "k" in the composer and ⌘1 "1" in the room search.
  The text of a shortcut (`is_shortcut`) is no longer typed.
- `src/text_editor.rs`, `src/text_input.rs`: the input method was asked
  only while the window was key. Emoji & Symbols takes the key window
  while open and types the pick into the field focused before, which
  macOS then dropped. A focused field now keeps its input method, as a
  text view stays the first responder; only the caret stops blinking.
- `src/text_editor.rs`, `src/text_input.rs` (surrounding text): the
  fields tell the input method their text around the caret (the editor
  its lines with the caret and the selection, a field all of it unless
  secure), and apply its `DeleteSurrounding` before the commit. See
  `vendor/iced_core/README.md`.
- `src/text_editor.rs` (editing, `docs/native-behaviour.md` B2): key
  bindings for the editing iced lacks. ⌘Z and ⇧⌘Z (Ctrl+Z, Ctrl+Shift+Z
  and Ctrl+Y elsewhere) undo and redo (`Binding::Undo`, `Binding::Redo`).
  ⌥⌫ and ⌥⌦ delete a word (Ctrl+Backspace, Ctrl+Delete elsewhere), ⌘⌫
  and ⌘⌦ to the start and end of the line as shown (`Binding::DeleteTo`).
  ⌘↑ and ⌘↓ go to the start and end of the text (with ⇧, select), ⌥↑
  and ⌥↓ (Ctrl+↑ and Ctrl+↓ elsewhere) by paragraph.
- `src/text_editor.rs` (editing): on macOS, the Emacs keys of its text
  system (`emacs`) instead of `convert_macos_shortcut`, whose keys came
  with a control character as their text and so bound nothing (only ⌃H
  worked): ⌃A and ⌃E to the paragraph's edges, ⌃B ⌃F ⌃P ⌃N, ⌃H ⌃D, and
  ⌃K, ⌃Y, ⌃T, ⌃O. The text input still uses `convert_macos_shortcut`.
- `src/text_editor.rs` (editing): a press in the selection (while
  focused) grabs it: a caret shows where it would drop, following the
  pointer, and releasing moves it there (`Edit::MoveSelection`), or copies
  it with ⌥ (Ctrl elsewhere); released where pressed, it is a click. The
  pointer is an arrow over the selection, as in a text view.
