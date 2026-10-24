# Behaving like a Mac app: to do

Status: checklist, not started. What the text system and a native app give
for free (a web view showed it: `docs/web-variant.md` on the
`web-variant-dd34eb` branch), written down once so that it is done once,
not found one issue at a time.

## How

- Go through it in tiers, an item at a time.
- Each item gets a test with simulated input (`iced_test`, as in
  `app/tests.rs`) where it can, so it stays done.
- Fix a gap where it starts: the composer's gaps in one editor layer over
  iced's `text_editor` (or upstream in iced), not a key binding here and
  there.
- `[x]` done, `[ ]` to do; "unchecked" means nobody has tried it yet.

## Tier 1: writing and selecting

### 1. Editing in the composer

iced's `text_editor` (0.14) has no undo, and of the macOS key bindings only
the arrows (with ⌥, ⌘ and ⇧), Home/End, ⌘A/C/X/V and ⌃A/E/B/F/H/D.

- [ ] ⌘Z undo, ⇧⌘Z redo; typing grouped into words, as in a text view.
- [ ] ⌥⌫ and ⌥⌦ delete a word; ⌘⌫ deletes to the start of the line.
- [ ] ⌘↑ and ⌘↓ go to the start and end of the draft (with ⇧, select).
- [ ] ⌥↑ and ⌥↓ go to the start and end of the paragraph.
- [ ] ⌃K deletes to the end of the line, ⌃Y yanks it back, ⌃T transposes,
      ⌃N and ⌃P move down and up, ⌃O opens a line.
- [x] Double- and triple-click select a word and a line (iced).
- [ ] Dragging selected text moves it; dropping text inserts it.

### 2. Input methods

iced 0.14 shows text being composed (preedit); the rest is unchecked.

- [ ] Press-and-hold accent menu (#30).
- [ ] Dead keys: ⌥E then E gives é. Unchecked.
- [ ] Japanese and Chinese input: candidates next to the caret, Enter
      confirms rather than sends. Unchecked.
- [ ] Dictation (🎤 or Fn Fn). Unchecked.
- [ ] ⌃⌘Space opens Emoji & Symbols and inserts the pick. Unchecked.

### 3. Menu bar

The app has no menu bar of its own (winit's default app menu only), so no
Edit menu: none of its items, and no place to find shortcuts. `muda` adds
native menus to a winit window.

- [ ] App menu: About, Settings… (⌘,), Hide, Quit.
- [ ] Edit: Undo, Redo, Cut, Copy, Paste, Select All, Find (⌘F); macOS
      adds Emoji & Symbols, Start Dictation and AutoFill.
- [ ] View: Bigger, Smaller, Actual Size (⌘+ ⌘− ⌘0, already keys).
- [ ] Window: Minimize (⌘M), Zoom, Close (⌘W), Enter Full Screen (⌃⌘F).
- [ ] Help: the website.

### 4. Selection in the timeline

`docs/text-selection.md` has what is done.

- [ ] Shift with an arrow does not extend a selection (reported with
      0.2.0), though `shortcuts` in `app.rs` handles it: reproduce, test.
- [ ] ⇧⌘← and ⇧⌘→ extend to the start and end of the line, ⇧⌘↑ and ⇧⌘↓
      to the first and last message on screen.
- [ ] Sender names, reply previews, notices and state lines selectable.
- [ ] Links in typeset (math) messages clickable.
- [ ] Code in math messages selectable by character.

### 5. Composer: menu and paste

- [ ] Right-click: Cut, Copy, Paste, Select All (and Emoji & Symbols).
- [ ] ⌘V of a picture (a screenshot, say) attaches it, as dropping a file
      does.
- [ ] ⌘V of rich text (from a web page or Mail) as Markdown: links, lists,
      emphasis.
- [ ] ⌥⇧⌘V pastes as plain text.

## Tier 2: conventions

### 6. Keyboard

- [x] ⌘K: room search.
- [x] ⌘+ ⌘− ⌘0: text size.
- [ ] Page Up/Down and ⌥↑/⌥↓ scroll the timeline; ⌘↑/⌘↓ go to the oldest
      loaded and the newest message (when the composer does not take them).
- [ ] ⌥⌘↑ and ⌥⌘↓ (or ⌃Tab and ⌃⇧Tab) go to the room above and below;
      ⌘1–⌘9 to the first nine rooms.
- [ ] ⌘F: find in the room's loaded messages.
- [ ] Typing with nothing focused goes to the composer.
- [ ] Tab and ⇧Tab move between controls, with a visible focus ring;
      Space presses the focused button.
- [ ] ↑ in an empty composer edits our last message.

### 7. Mouse

- [x] Pointing hand over links and buttons, I-beam over selectable text.
- [ ] The link's address shows on hovering it (in a corner, as in a
      browser).
- [ ] Right-click a link: Open, Copy Link.
- [ ] Right-click a picture: Copy Image, Save Image….
- [ ] Drag a picture or file out of a message into Finder or another app.
- [x] Drop files onto the window to attach them.

### 8. Clipboard

- [x] Copy as text, math as `$…$` (`docs/text-selection.md`).
- [ ] Copy as HTML as well, so pasting into Mail or a document keeps
      formatting, links and lists.

### 9. Window

- [ ] Size, position and display remembered.
- [ ] ⌘W closes the window and keeps the app running and synced (the dock
      badge and notifications go on); the Dock icon opens it again.
      Closing the window quits the app now.
- [ ] Full screen, and tiling with the system's window layouts.

## Tier 3: text rendering

Screenshot tests (as in `app/tests.rs`) at each text size, in messages and
in the composer. All unchecked.

- [ ] Colour emoji, sequences (families, skin tones) and flags.
- [ ] Chinese, Japanese and Korean, with their fonts.
- [ ] Right to left (Arabic, Hebrew), mixed with left to right.
- [ ] Combining marks (é as e + ´), Devanagari, Thai.
- [ ] Long words and URLs wrap within the message.
- [ ] Math next to text in each of the above.

## Limits of iced

Not fixable in our code alone; listed so they are known, not rediscovered.

- **Accessibility.** iced has no accessibility tree, so VoiceOver,
  Voice Control and Switch Control see nothing. Upstream work in iced.
- **The system's text services.** Spell checking, autocorrect, text
  replacements, smart quotes, Writing Tools, Services and Look Up come with
  a native text view. Spell checking could be done with `NSSpellChecker`,
  but iced's editor cannot underline a word yet.
