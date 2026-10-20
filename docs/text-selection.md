# Copying from messages and selecting text

Status: steps 1 and 2 are being built; step 3 is planned.

## Why it is missing

- iced only selects text in input fields (`text_input`, `text_editor`).
  The text it displays (`text`, `rich_text`) cannot be selected, and iced
  has no right-click menus.
- A message with math is drawn as one Typst image of the whole message,
  not as text.

So copying needs a message menu (step 1), and selecting part of a message
needs either a read-only editor (step 2) or our own selectable text widget
(step 3).

## Step 1: message menu

Right-click on a message, or "⋯" in its hover toolbar, opens a menu.
Right-click opens it at the cursor; "⋯" opens it below the toolbar.

| Item                    | When                         | Does                                          |
|-------------------------|------------------------------|-----------------------------------------------|
| Copy text               | text messages                | `Document::to_plain_text`: Markdown-like, math as `$…$` |
| Copy LaTeX (per formula)| messages with math           | the formula's LaTeX, without `$`              |
| Copy link (per link)    | messages with links          | the URL                                       |
| Save image…             | math messages that rendered  | PNG, dark text on white, 3× scale (save dialog) |
| Select text…            | text messages                | step 2                                        |

Parts:

- `context_menu.rs`: a widget wrapping its content. Right-click inside
  records the cursor position (in widget state) and sends `on_open`;
  while given a menu, it floats there over everything (popover's panel,
  anchored at a point). A click outside or Esc closes it.
- `actions.rs`: `MenuAt { Cursor, Toolbar }` like the reaction window's
  `PickerAt`; `Message::{OpenMenu, CloseMenu, Copy, SaveImage,
  SelectText, …}`; `message_menu(event)` builds the items.
- `mtrx-format`: `Document::formulas()` lists the LaTeX of each formula.
- Save image renders again with fixed colours (not the theme's), so the
  file looks the same in light and dark mode.
- Not included: "Copy image" (iced's clipboard holds text only; it needs
  e.g. `arboard`). Open question.

## Step 2: Select text…

The message's text (as Copy text gives it) replaces the message in place,
in a read-only `text_editor`: drag, double-click a word, ⌘A, ⌘C. Esc or
"Done" returns to the message. It works for math messages too, as their
LaTeX source. Editing actions are dropped; the editor's own copy binding
does the copying.

## Step 3: real selection in the timeline

Goal: select text in messages as in a browser. Drag across words, lines
and messages; double-click selects a word, triple-click a paragraph;
shift-click extends; ⌘C copies; Esc or a click elsewhere clears.

### Widget

`selectable.rs`: a selectable version of `rich_text`.

- Lays out its spans as iced's `rich_text` does. Hit-testing uses
  `Paragraph::hit_test` (cursor to character offset) and
  `grapheme_position`.
- Highlights from the paragraph's cosmic-text buffer
  (`iced_graphics::text::Paragraph::buffer()`, layout runs and
  `highlight(start, end)`), one rectangle per line. This ties us to
  iced_graphics, which both of iced's renderers (wgpu, tiny-skia) use.
- A press and release without movement is still a click, so links keep
  working. Movement beyond a few pixels starts a selection.
- Each message is a `Document` of blocks. The widget maps a selection to
  (block, character offset) positions. Copying turns the selected range
  back into plain text with the same rules as `to_plain_text`, so Copy
  text and a full selection give the same result.

### Selection state

The selection lives in `Room`, not in the widgets, so it can span
messages and survive redraws:

```text
Selection { anchor: Position, focus: Position }
Position  { event: event id, block: path into the document, offset: usize }
```

- A drag starting in one message and moving over another extends the
  focus into it. Messages between anchor and focus are fully selected.
- Order comes from the timeline. Copy joins messages with a blank line,
  each preceded by "Sender, time:" when the selection spans senders, as
  Element does.
- New messages arriving or history loading do not change the selection.
  A message that is edited or deleted drops out of it.

### Math messages

Two options; to decide when we get there:

1. **Selection over the image.** Typst's layout (`Frame` items) knows
   each glyph's position and its source span. Building a map from image
   rectangles to source text makes the image selectable like text. A
   formula selects as a whole and copies as `$…$`. This is more work, but
   keeps the typeset look.
2. **Native text with formula images.** Text paragraphs are drawn by iced
   and only formulas are images. This needs inline images in a paragraph,
   which iced's `rich_text` cannot do, so it would need our own inline
   layout.

Option 1 is preferred.

### How steps 1 and 2 change

- The menu stays. It gains "Copy selection" at the top when you
  right-click inside a selection.
- "Select text…" is removed once text and math messages are selectable
  (step 3, including option 1).
- `context_menu.rs` is reused for links (Copy link, Open) and the
  selection.

### Phases and effort

| Phase | Content                                              | Effort   |
|-------|------------------------------------------------------|----------|
| A     | Selectable widget; selection inside one message; ⌘C  | 3–4 days |
| B     | Selection across messages; copying with senders      | 2 days   |
| C     | Math messages: selection over the image (option 1)   | 2–3 days |

### Tests

- Unit: selection range to text (inside blocks, across lists and quotes,
  across messages), word and paragraph boundaries.
- UI (iced_test): drag across a message, then ⌘C puts the expected text
  on the clipboard. Snapshot of the highlight in light and dark mode.
