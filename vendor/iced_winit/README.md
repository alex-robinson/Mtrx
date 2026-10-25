`iced_winit` 0.14.1 (MIT, https://github.com/iced-rs/iced), vendored to
carry our patches to its input method handling (see
`docs/native-behaviour.md`, section B1). Each is marked "Mtrx patch" in the
code:

- `src/window.rs`, `src/conversion.rs` (surrounding text): passes the
  surrounding text of an `InputMethod` (our `iced_core`) to our winit's
  `Window::set_ime_surrounding_text`, and its `Ime::DeleteSurrounding` on
  as `input_method::Event::DeleteSurrounding`. In the shape of winit 0.31;
  see `vendor/iced_core/README.md`.
