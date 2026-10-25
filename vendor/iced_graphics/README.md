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
