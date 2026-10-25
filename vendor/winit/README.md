`winit` 0.30.13 (Apache-2.0, https://github.com/rust-windowing/winit), vendored
to carry our patches to its macOS text input (see `docs/native-behaviour.md`,
section B1). Each is marked "Mtrx patch" in the code:

- `src/platform_impl/macos/view.rs`: winit #4748 as backported to 0.30 in
  https://github.com/rust-windowing/winit/pull/4749 (commit 73c6976, not
  yet merged), without its changelog entry. Emoji & Symbols and Dictation
  insert text outside a key press and without marked text, which was
  dropped; it is now an `Ime::Commit`. `selectedRange` reported no caret,
  so Dictation would not start; it now reports one at the end of the
  marked text. A commit outside a key press no longer swallows the next
  key. Drop this copy once a 0.30 release has it.
- `src/platform_impl/macos/view.rs`: keys the input system consumes.
  While the press-and-hold accent menu is up, AppKit keeps the repeats of
  the held letter, and a digit choosing an accent, for itself:
  `interpretKeyEvents` answers them with none of `insertText`,
  `setMarkedText`, `unmarkText` or `doCommandBySelector`. winit sent them
  on with their text, so the letter was typed again and again, and the
  digit too. Such a key now goes out without text. Upstream: iced #3266
  (https://github.com/iced-rs/iced/issues/3266); winit master is the same.
- Surrounding text (marked "Mtrx patch (surrounding text)"): AppKit
  replaces the letter held for the accent menu with the accent picked, by
  a range in the text it is told about; winit told it none (#30). From
  winit 0.31 (`winit-core`): `ImeSurroundingText` and
  `Ime::DeleteSurrounding`, unchanged; 0.31 sets the text through
  `ImeRequest`, here a setter like 0.30's other IME ones,
  `Window::set_ime_surrounding_text`, forgotten with `set_ime_allowed(false)`
  as 0.31 forgets capabilities. For AppKit (0.31 has it for Wayland only),
  `src/platform_impl/macos/view.rs` and `surrounding.rs`: `selectedRange`,
  `markedRange` and `attributedSubstringForProposedRange` answer from that
  text, with the marked text in place of the selection; `insertText` and
  `setMarkedText` with a replacement range send `DeleteSurrounding` for
  what it covers around the selection before the commit or preedit. Other
  platforms ignore it.
