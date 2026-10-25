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
  (`iced_graphics`). `Edit::DeleteTo(Motion)` deletes from the cursor to
  where a motion takes it (⌥⌫: a word). `Motion::ParagraphBackward` and
  `ParagraphForward` (⌥↑ and ⌥↓, which `Motion::widen` gives for ↑ and ↓)
  go to the paragraph's start and end, on to the next when already there;
  `ParagraphStart` and `ParagraphEnd` (⌃A, ⌃E) stay in it. `Edit::Kill`,
  `Yank`, `Transpose` and `OpenLine` for macOS's ⌃K, ⌃Y, ⌃T and ⌃O.
  `Edit::MoveSelection` drops the dragged selection at a point (or a copy
  of it), and `Editor::caret_at` tells where a click at a point puts the
  caret, to draw it while dragging (`src/renderer/null.rs` too).
- `src/event.rs` (menu actions, marked "Mtrx patch (menu actions)",
  `docs/native-behaviour.md` C): `Event::StandardAction`, our winit's
  `StandardAction` (Undo, Copy, Find, Zoom In, ...): on macOS the Edit and
  View menus take their shortcuts before the window, so these come instead
  of the key presses. `StandardAction::key_press` is the key press each
  stands for (⌘Z for Undo), which widgets and apps handle it as.
- `src/input_method.rs`, `src/text/editor.rs` (menu actions): macOS enables
  an Edit menu's items as the focused text view can do them.
  `InputMethod::Enabled` carries an `EditState` (can undo, can redo, has a
  selection), and `InputMethod::enables` tells which standard actions to
  enable: with no field focused, those an app may take for itself (Copy,
  Select All, Find, the text size). `Editor::can_undo` and `can_redo`
  (`src/renderer/null.rs` too).
