`iced_widget` 0.14.2 (MIT, https://github.com/iced-rs/iced), vendored to
carry our patches to iced's widgets (see `docs/native-behaviour.md`,
section B). Each is marked "Mtrx patch" in the code:

- `src/stack.rs`: `Stack::update` stopped at any captured event, also one
  captured before the stack by a widget beside it, so a press in the
  timeline never reached the composer under its paperclip and smiley, which
  kept the focus and took Shift with the arrows. The layers now get a shell
  of their own.
