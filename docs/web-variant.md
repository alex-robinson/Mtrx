# A web UI for the desktop app: spike

Can a web UI (Svelte in a Tauri webview) over the same Rust core replace the
iced app? This spike tests the parts where iced cost us most: text
selection and copy, the composer (IME, press-and-hold accents, long
drafts), math, scrolling. Its code (`crates/mtrx-web`, the Tauri crate,
and `apps/web`, the page; 24 October 2026) was not kept, only this
evaluation.

## What was built

- Sign in with a password; the session is restored on the next start.
  Its own data directory (`org.mtrx.MtrxWeb`), so it runs beside the iced
  app.
- Room list from the core's room list stream: names, unread and mention
  counts, invites.
- One room's timeline: messages with formatting, inline and display math
  (KaTeX), day dividers, the read marker, sending state; older messages
  load on scrolling up; read receipts sent while at the bottom.
- Composer: grows with the draft up to 40% of the window, then scrolls;
  Enter sends (Markdown and `$…$` via the core), Shift+Enter adds a line.
- Sign out.

The core reaches the page as Tauri commands (calls: `restore`, `login`,
`logout`, `open_room`, `paginate_back`, `send`, `mark_read`) and events (the
core's streams: `rooms`, `timeline`), mirroring the UniFFI split.
`crates/mtrx-web/src/dto.rs` maps the core's types to JSON, as `mtrx-ffi`
does for the mobile apps. **No change to `mtrx-core` was needed.**

## Measurements

macOS 27, Apple silicon, release builds of both, same account and rooms on
the local Synapse.

| | iced (`Mtrx.app`) | web (`Mtrx Web.app`) |
|---|---|---|
| App bundle | 173 MiB | 104 MiB |
| Zipped (download) | 60 MiB | 32 MiB |
| Executable, stripped | 136 MiB | 77 MiB |
| Start-up to first frame | 210 ms | 370 ms |
| Start-up to room list shown | 290 ms | 420 ms |
| Memory, room list (footprint) | 155 MiB | 85 MiB (app 33, WebKit page 34, GPU 14, network 5) |
| Memory, room of 259 messages open | 187 MiB | 192 MiB (app 36, WebKit page 80, GPU 68, network 5) |

Start-up: from `open` to the first frame ("Starting…") and to the first
frame with rooms, medians of ten starts with the store synced; the first
start after a build took 1.6 s for both. Memory: `footprint` 10 s after
start, medians of five. The web variant starts about 130 ms later
(WebKit's processes and loading the page). It needs half the memory for the room list; with a room open the
page and GPU processes grow with the timeline and the two are level.

The web bundle is smaller because it has neither Typst (math) nor wgpu;
the spike also lacks media and PDF code, which would add little (the
webview shows images, video and PDFs itself). The page is 0.3 MiB of
JavaScript, 35 KiB of CSS and 0.6 MiB of KaTeX fonts.

Timeline updates: the core sends the whole timeline on every change, and
the spike forwards it as is: 85 KB of JSON for 265 messages (about 330
bytes each), four updates per message sent. From an update arriving to the
next frame painted took 12–52 ms (debug build, two animation frames
included), so whole snapshots are fine at this size; diffs would be worth
it for rooms with thousands of loaded messages.

## Lines of code

| | Lines |
|---|---|
| Rust: commands and events, `dto.rs`, start-up | 594 |
| Rust: debug-only test hook | 32 |
| Svelte components (markup, logic, scoped CSS) | 738 |
| TypeScript (API types and calls, copy) | 137 |
| Global CSS | 91 |
| Config (Tauri, Vite, TypeScript, npm) | 126 |

About 1,560 lines of code. For comparison the iced app is 21,700 lines;
its text selection alone (`selection.rs`, `selectable.rs`,
`selectable_image.rs`, the units in `message.rs`) is about 2,000.

## What came for free

- **Selection and copy.** The browser's selection across messages, with
  what browsers bring (double- and triple-click, Shift-click,
  Shift+arrows, drag-scrolling at the edges; checked here: a selection
  over six messages, starting mid-word and ending inside a formula).
  Only the copied text needed code (`apps/web/src/lib/copy.ts`, 48 lines):
  math as `$…$`/`$$…$$` and without senders, times and dividers, as the
  iced app copies. KaTeX's own `copy-tex` loses the line breaks between
  messages.
- **Input.** The composer is a `<textarea>`: the system's text input,
  so IME, dictation, Writing Tools, Emoji & Symbols, the macOS text
  services and the press-and-hold accent menu (issue #30; checked: holding
  "e", then "2", types "é") come with it. Enter while an input method composes confirms the composition rather
  than sending (`isComposing`).
- **Long drafts.** `field-sizing: content` with a `max-height` makes the
  composer grow and then scroll inside itself; scrolling a 40-line draft
  with blank lines did not move the timeline (no counterpart of the
  vendored `iced_graphics` patch of #32).
- **Accessibility.** The page shows in the macOS accessibility tree as is:
  rooms are buttons with their names and counts, messages are text
  (checked with the accessibility API; VoiceOver itself not tried). The
  iced app has no accessibility tree.
- **Math.** KaTeX renders `$…$` and display math from the core's document
  model; formulas are typeset once and cached. 22 lines.
- **Scrolling.** Native, with momentum and the system's scroll bars.
- **Dark mode, text zoom, fonts, emoji, links.** CSS
  (`prefers-color-scheme`), the system font stack, colour emoji.

## Gaps and problems

- **Spell checking is off in an app's web view.** WebKit checks as you
  type only with the `WebContinuousSpellCheckingEnabled` default on, which
  Safari sets for itself; the app registers it at start-up (macOS only,
  `main.rs`, needs `objc2`). The default Tauri menu has no Spelling menu.
- **Keeping the view in place when older messages load** is our code
  (`Timeline.svelte`, about 40 lines): note the first visible message
  before an update, restore its position after. The browser's own scroll
  anchoring (`overflow-anchor`) is turned off so the two do not fight.
- **Links** would open inside the window and replace the app; clicks on
  links go to the system browser (`tauri-plugin-opener`).
- **A covered window is not drawn.** WebKit stops painting a page whose
  window is fully hidden (`visibilityState` "hidden"). Harmless for
  use, but it broke the first start-up measurements.
- **Testing.** There is no WebDriver for WKWebView on macOS (Tauri's
  `tauri-driver` covers Linux and Windows), and input events posted to a
  background web view are ignored. The spike adds a debug-only hook
  (`MTRX_WEB_TEST_FIFO`: run JavaScript in the page, report back) that
  drove the checks above; component tests could run in a browser with
  Tauri's IPC mocked.
- **One lockfile with Tauri's Linux stack.** Tauri's GTK 3 crates (Linux
  only, but in the shared `Cargo.lock`) pin `toml_datetime = 0.6.3`, which
  moves Typst's `toml` from 0.8.23 to 0.8.2 for the iced app too (still
  within its `^0.8`). Linux CI would need `libwebkit2gtk-4.1-dev` for
  `cargo clippy --workspace`; the web variant's `build.rs` writes a
  placeholder page so the workspace builds without npm.
- **Two toolchains.** Node and npm next to Cargo. TypeScript 7 does not yet
  work with `svelte-check`, so TypeScript is held at 6.
- **Linux and Windows render with other engines** (WebKitGTK, WebView2):
  three engines to test instead of one renderer. WebKitGTK is the weakest
  (input methods, scrolling performance).
- **IPC is JSON.** Every timeline update is serialized in Rust and parsed in
  JavaScript (above). Media would need a custom URL scheme served from
  Rust (decrypted `mxc://` content), not JSON.

## A full port, feature by feature

Days of focused work, assuming the core's API stays as it is (none of
these needed core changes in the spike, but encryption and calls were not
tried).

| Feature (iced modules, lines) | Web | Days |
|---|---|---|
| Sign-in in the browser (OAuth) (`login.rs` 215) | `login_oauth` + opener | 0.5 |
| Room list: spaces, categories, sorting, search, colours, drag to arrange (`sidebar.rs`, `arrange.rs`, `tint.rs` 2,080) | Svelte lists, HTML drag and drop | 4 |
| Timeline: replies, reactions, edits, deletions, receipts, typing, retry (`room.rs`, `message.rs`, `actions.rs` 3,250) | components on the existing DTOs | 4 |
| Message and context menus (`message_menu.rs`, `context_menu.rs`, `popover.rs` 740) | HTML menus or Tauri native menus | 1 |
| Composer: mentions, emoji picker, editing, replying, pasting and dropping files (`mention.rs`, `emoji.rs` 740) | textarea + popovers | 3 |
| Selection and copy (`selection.rs`, `selectable*.rs` 1,630) | done (copy as image of math: canvas) | 0.5 |
| Math (`math.rs` 246, Typst) | done (KaTeX) | 0 |
| Media: images, video, audio, files (`media.rs` 738) | `mxc://` URL scheme in Rust, `<img>`/`<video>` | 2 |
| PDF preview (`pdf.rs`, `pdf_viewer.rs` 660) | WebKit shows PDFs | 0.5 |
| Send later (`send_later.rs` 597) | dialog + list | 1.5 |
| Calls by link, embedded calls (`calls.rs` 371) | iframe or second webview | 1.5 |
| Room info, members, presence (`info.rs`, `members.rs`, `presence.rs` 1,280) | panels | 2 |
| Encryption: verification, recovery key (in `dialog.rs` 894) | dialogs | 2 |
| Settings, appearance, text size (`settings.rs`, `appearance.rs`, `text_size.rs` 1,390) | CSS variables, zoom | 1.5 |
| Notifications, dock badge (`badge.rs` 191) | `tauri-plugin-notification`, badge API | 1 |
| Last view, window size, menus and shortcuts | `tauri-plugin-window-state`, menu API | 1 |
| Packaging and CI for macOS, Windows, Linux (`xtask` bundle) | Tauri bundler, WebKitGTK in CI | 2 |
| Tests (`app/tests.rs` 2,312) | component tests with mocked IPC, hook-driven end-to-end | 3 |
| **Total** | | **about 31** |

So about six weeks to parity, with the risks in Linux (WebKitGTK) and in
testing, and with selection, input, accessibility and scrolling no longer
ours to maintain.
