`iced_graphics` 0.14.0 (MIT, https://github.com/iced-rs/iced) with one fix in
`src/text/editor.rs`: `Editor::min_bounds` measured only the lines scrolled
into view, and cosmic-text counts empty lines at the edges differently from
others, so a text editor sized by its content (the composer) changed height
while scrolling, moving the timeline above it (#32). It now measures all
lines, laid out in `update`. Drop this copy once upstream releases a fix.
