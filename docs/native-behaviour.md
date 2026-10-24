# Behaving like a Mac app: to do

Status: A in progress. What the text system and a native app give
for free, written down once so that it is done once, not found one issue
at a time.

## Why iced, and why this order

A web view (`docs/web-variant.md`, branch `web-variant-dd34eb`) gave the
composer, selection, accessibility and spell checking for free, at the
price of three rendering engines (WebKit, WebKitGTK, WebView2), with Linux
the weakest and most of our users on Mac and Linux. iced renders the same
everywhere, so the gaps below are paid for once, in our code or in a copy
of one iced widget.

The list is grouped by where the fix lives, and ordered by what it buys
per day:

- **A. App code.** Cheap, no fork, each item on its own. Do these first.
- **B. The composer.** iced's `text_editor` exposes no cursor by offset
  and no history, so these need a vendored `iced_widget` (as
  `vendor/iced_graphics` already is). One fork, one project, carrying
  every item in B.
- **C. Menu bar.** Needs `muda`; on macOS the menu's key equivalents are
  matched before winit sees the key, so every Edit shortcut must be routed
  back into the focused field. After B, not before.
- **D. Text rendering.** Screenshot tests; fixes where they fail.
- **E. Limits.** Not fixable in our code; listed so they are not
  rediscovered.

## How

- An item at a time, A before B before C.
- Each item gets a test with simulated input (`iced_test`, as in
  `app/tests.rs`) where it can, so it stays done.
- Fix a gap where it starts: B in the editor layer, not a key binding
  here and there.
- `[x]` done, `[ ]` to do; "unchecked" means nobody has tried it yet.

## A. App code

### A1. Window

- [x] ⌘W closes the window and so quits, as the close button does: there
      is only ever one window (decided 2026-10-24). `main.rs` already sets
      `exit_on_close_request: false`; `Message::CloseWindow` goes offline
      and closes.
- [x] Size, position and display remembered (`placement.rs`), and
      maximised; full screen is not, the window opens as it was before. A
      window left on a display since unplugged opens on the main one.
- [ ] Full screen, and tiling with the system's window layouts. Unchecked.

### A2. Keyboard

- [x] ⌘K: room search.
- [x] ⌘+ ⌘− ⌘0: text size.
- [x] Shift with an arrow does not extend a selection (reported with
      0.2.0). A press in the timeline never reached the composer, so it
      kept the focus and took Shift+arrows: iced's `Stack` stopped at an
      event captured beside it. Patched in `vendor/iced_widget`.
- [x] ⇧⌘← and ⇧⌘→ extend to the start and end of the line, ⇧⌘↑ and ⇧⌘↓
      to the first and last message on screen.
- [x] Page Up/Down and ⌥↑/⌥↓ scroll the timeline; ⌘↑/⌘↓ go to the oldest
      loaded and the newest message (when the composer does not take them).
- [x] ⌥⌘↑ and ⌥⌘↓ (or ⌃Tab and ⌃⇧Tab) go to the room above and below;
      ⌘1–⌘9 to the first nine rooms.
- [x] ⌘F: find in the room's loaded messages (`find.rs`). Counted from
      the newest; Enter and ⌘G go to older matches.
- [x] Typing with nothing focused goes to the composer.
- [x] ↑ in an empty composer edits our last message.

### A3. Mouse

- [x] Pointing hand over links and buttons, I-beam over selectable text.
- [x] Drop files onto the window to attach them.
- [x] The link's address shows on hovering it (in a corner, as in a
      browser).
- [x] Right-click a link: Open, Copy Link.
- [x] Right-click a picture: Copy Image, Save Image…, in the timeline
      and in the viewer, sharing math's clipboard code (`message_menu.rs`).

### A4. Selection in the timeline

`docs/text-selection.md` has what is done.

- [x] Drag, double- and triple-click, Shift-click, ⌘A, ⌘C, across
      messages; math as `$…$`.
- [x] Sender names, reply previews, notices and state lines selectable.
- [x] Links in typeset (math) messages clickable.
- [x] Code in math messages selectable by character.

### A5. Clipboard and paste

Paste is a binding we can replace (`Binding::Custom`, then
`Action::Edit(Edit::Paste)`), and `arboard` reads pictures and HTML, so
these stay outside the widget.

- [x] Copy as text, math as `$…$`.
- [x] Copy as HTML as well, so pasting into Mail or a document keeps
      formatting, links and lists.
- [x] ⌘V of a picture (a screenshot, say) attaches it, as dropping a file
      does.
- [x] ⌘V of rich text (from a web page or Mail) as Markdown: links, lists,
      emphasis.
- [x] ⌥⇧⌘V pastes as plain text.
- [ ] Right-click in the composer: Cut, Copy, Paste, Select All (and
      Emoji & Symbols). `context_menu.rs` has the menu.

## B. The composer: inside `text_editor`

iced 0.14's `text_editor` has no undo, and of the macOS key bindings only
the arrows (with ⌥, ⌘ and ⇧), Home/End, ⌘A/C/X/V and ⌃A/E/B/F/H/D. Its
`Content` gives the text, the selection and a line/column cursor: nothing
to build an undo stack or a word deletion on from outside. So: vendor
`iced_widget`, and do all of B in that copy.

Decided (2026-10-24): the patch first, ours to keep; whether to offer it
upstream is judged once it works, so no feature waits on iced.

### B1. Input methods

The group writes European languages, so accents come first. Chinese,
Japanese and Korean go through an input method (type, pick a candidate,
Enter confirms), which nobody here uses: correct, not urgent. The widget
forwards every key press to the binding without looking at the preedit,
so that Enter probably sends the half-composed text today.

- [ ] Dead keys: ⌥E then E gives é. Unchecked.
- [ ] Press-and-hold accent menu (#30). winit and the NSView, not the
      widget; may need upstream winit.
- [ ] Chinese, Japanese and Korean input: candidates next to the caret,
      and Enter confirms the composition rather than sending.
- [ ] Dictation (🎤 or Fn Fn). Unchecked.
- [ ] ⌃⌘Space opens Emoji & Symbols and inserts the pick. Unchecked.

### B2. Editing

- [x] Double- and triple-click select a word and a line (iced).
- [ ] ⌘Z undo, ⇧⌘Z redo; typing grouped into words, as in a text view.
- [ ] ⌥⌫ and ⌥⌦ delete a word; ⌘⌫ deletes to the start of the line.
- [ ] ⌘↑ and ⌘↓ go to the start and end of the draft (with ⇧, select).
- [ ] ⌥↑ and ⌥↓ go to the start and end of the paragraph.
- [ ] ⌃K deletes to the end of the line, ⌃Y yanks it back, ⌃T transposes,
      ⌃N and ⌃P move down and up, ⌃O opens a line.
- [ ] Dragging selected text moves it; dropping text inserts it.

## C. Menu bar

The app has no menu bar of its own (winit's default app menu only), so no
Edit menu: none of its items, and no place to find shortcuts. `muda` adds
native menus to a winit window. Its accelerators take the key before the
window does, so each one becomes a menu event routed to whatever is
focused; do it once B's bindings exist to route to.

- [ ] App menu: About, Settings… (⌘,), Hide, Quit. Quit (⌘Q) goes
      through `Message::CloseWindow`, as closing does: winit's own Quit
      exits without going offline or ending the session.
- [ ] Edit: Undo, Redo, Cut, Copy, Paste, Select All, Find (⌘F); macOS
      adds Emoji & Symbols, Start Dictation and AutoFill.
- [ ] View: Bigger, Smaller, Actual Size (⌘+ ⌘− ⌘0, already keys).
- [ ] Window: Minimize (⌘M), Zoom, Close (⌘W), Enter Full Screen (⌃⌘F).
- [ ] Help: the website.

## D. Text rendering

Screenshot tests (as in `app/tests.rs`) at each text size, in messages and
in the composer. All unchecked.

- [ ] Colour emoji, sequences (families, skin tones) and flags.
- [ ] Chinese, Japanese and Korean, with their fonts.
- [ ] Right to left (Arabic, Hebrew), mixed with left to right.
- [ ] Combining marks (é as e + ´), Devanagari, Thai.
- [ ] Long words and URLs wrap within the message.
- [ ] Math next to text in each of the above.

## E. Limits of iced

Not fixable in our code alone.

- **Accessibility.** iced has no accessibility tree, so VoiceOver,
  Voice Control and Switch Control see nothing. Upstream work in iced.
- **The system's text services.** Spell checking, autocorrect, text
  replacements, smart quotes, Writing Tools, Services and Look Up come with
  a native text view. Spell checking could be done with `NSSpellChecker`,
  but iced's editor cannot underline a word yet.
- **Dragging out of the window.** winit cannot start a drag, so a picture
  or file cannot be dragged into Finder or another app; it would take a
  native drag session per platform (decided 2026-10-24: not now).
- **Keyboard focus.** Tab and ⇧Tab between controls with a focus ring, and
  Space to press a button: iced's buttons are not focusable.
