# Copying from messages and selecting text

Status: done. This describes how it works and what is left.

## Why it needed building

- iced only selects text in input fields (`text_input`, `text_editor`).
  The text it displays (`text`, `rich_text`) cannot be selected, and iced
  has no right-click menus.
- A message with math is drawn as one Typst image of the whole message.

## Message menu

Right-click on a message, or "⋯" in its hover toolbar, opens a menu
(`message_menu.rs`). Right-click opens it at the cursor (`context_menu.rs`,
which records where the click was and floats the menu there through the
popover's panel); "⋯" opens it below the toolbar.

| Item         | When                        | Does                                              |
|--------------|-----------------------------|---------------------------------------------------|
| Copy         | text messages               | the selection if the message is in it, else the whole message as a selection copies it (math as `$…$`) |
| Copy image   | math messages that rendered | the picture, via `arboard` (iced's clipboard holds text only) |
| Save image…  | math messages that rendered | PNG, dark text on white, 3× scale                  |

Separate Copy text, Copy LaTeX and Copy link items were dropped: copying
gives the same text, formulas included as `$…$`.
The "Select text…" stopgap (the text in a read-only editor) was removed
once the timeline itself became selectable.

## Selection in the timeline

⌘A selects the messages on screen (partly shown ones too), so it never
loads or copies the whole history. Drag across words, lines and messages. Double-click selects a word (or a
formula), triple-click a paragraph, and Shift-click extends. ⌘C copies,
and Esc or a click beside the text clears. Dragging past the top or bottom
of the timeline scrolls it.

### Units

Each message is split into units: paragraphs, headings, code blocks and
displayed math (`message.rs`). One walk over the document builds both the
view and the units (`text_map`), so the two cannot disagree. A unit has:

- its text as shown;
- a prefix used when copying from its start along with other units ("> "
  in quotes, "- " or "1. " in lists);
- the gap before it ("\n\n" between blocks, "\n" between list items).

A selection copies the text as shown, like a browser. It is not the
Markdown-like text of "Copy text": inline code has no backticks and links
have no URL, but math is `$…$` and lists and quotes keep their marks.

### Selection state

The selection lives in `Room` (`selection.rs`), not in the widgets:

```text
Selection { anchor: Point, focus: Point, dragging }
Point     { event: event id, pos: Pos }
Pos       { unit, offset }   // byte offset in the unit's text
```

The room orders the two points by the messages' places in the timeline,
and gives each message the part it covers (`Bounds::in_message`). With
several messages, each sender's run of messages is preceded by
"Name, HH:MM". The mouse release that ends a drag is caught by a
subscription, wherever it happens.

### Widgets

- `selectable.rs`: rich text as iced's `rich_text` draws it, plus
  hit-testing and highlighting through the paragraph's cosmic-text layout
  (`iced_graphics`, same version as iced). It reports `Press` (with the
  word or paragraph for double and triple clicks) and, while dragging,
  `Drag` when the cursor is beside its lines. A press and release on a
  link without moving still opens it. It uses the cursor's position, not
  the event's, because a scrollable only moves the cursor into its
  content's coordinates.
- `selectable_image.rs`: the same for a typeset math message, over the
  image.
- `drag_scroll.rs`: wraps the timeline. While dragging past its edge it
  asks for scrolling each frame, and passes a cursor at the edge to the
  text, since the content moves under a cursor that does not.

### Math messages

The renderer (`mtrx-format`) puts invisible `metadata` markers around
each text leaf in the Typst source. A leaf is each text, inline code and
inline math, and each code block and displayed math, in reading order.
In the laid-out page, glyphs between a leaf's markers belong to that
leaf. Text glyphs carry their byte offset within the leaf. Math and code
are laid out from generated Typst, so they become one piece per line,
selected whole. `Image::pieces` lists each piece's leaf, byte range and
rectangle. `message::text_map` says where each leaf is in the units, so
the image is selected with the same units as native text, and formulas
copy as `$…$`.

## Left to do

- Links in typeset (math) messages cannot be clicked; the menu's Copy link
  covers them.
- Code inside math messages is selected whole.
- Other text in the timeline is not selectable: sender names, reply
  previews, notices and state lines. Info cards are: names, IDs,
  addresses, links, read-only name and topic, and the facts line use
  `selectable_text`, which keeps its own selection and copies it with ⌘C.

## Tests

- Unit: units and their copy text (`message.rs`, `selection.rs`), hit
  testing and highlighting over an image (`selectable_image.rs`), edge
  scrolling speed (`drag_scroll.rs`).
- Simulated mouse: dragging, double and triple clicks, and dragging in a
  scrolled timeline (`message.rs`).
- Renderer: pieces in reading order and in the right places
  (`mtrx-format/tests/format.rs`).
- Snapshot: a selection from inside a math message across three senders,
  with its copied text (`app/tests.rs`).
