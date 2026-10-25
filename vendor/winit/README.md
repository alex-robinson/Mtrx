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
