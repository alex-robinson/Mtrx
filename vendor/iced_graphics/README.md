`iced_graphics` 0.14.0 (MIT, https://github.com/iced-rs/iced) with fixes in
`src/text/editor.rs`:

- `Editor::min_bounds` measured only the lines scrolled into view, and
  cosmic-text counts empty lines at the edges differently from others, so
  a text editor sized by its content (the composer) changed height while
  scrolling, moving the timeline above it (#32). It now measures all
  lines, laid out in `update`. Drop this once upstream releases a fix.
- Surrounding text (marked "Mtrx patch (surrounding text)"):
  `Edit::DeleteSurrounding` (our `iced_core`) deletes the bytes an input
  method asks for around the selection, across line endings, and keeps
  the selection for the commit that follows. See
  `vendor/iced_core/README.md`.
- Editing (marked "Mtrx patch (editing)", `docs/native-behaviour.md` B2):
  `src/text/editor/history.rs` keeps the editor's undo history, from
  cosmic-text's change tracking, grouped as a text view does (typing and
  deleting by words, a paste or an input method's commit a step of its
  own); `Edit::Undo` and `Edit::Redo` replay it and bring the selection
  back. A new editor (a new `Content`) starts a fresh history.
  `Edit::DeleteTo` deletes to where a motion goes, a character if it goes
  nowhere (⌘⌫ at a line's start joins it to the one before). A motion may
  take several of cosmic-text's (`to_motions`): the paragraph motions go
  back (forward) a character first, so that they go on to the paragraph
  before (after).
