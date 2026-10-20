# Calls inside the app: design idea

Status: idea, not planned. Today call links (room links, call cards) open
in the default browser.

## Goal

Join a call (Jitsi, Google Meet, later Element Call) inside Mtrx instead
of the browser. While in the call you can move to other rooms and spaces,
and come back to it in one click.

## Approach: the system webview

Use [`wry`](https://github.com/tauri-apps/wry), the webview crate behind
Tauri. It embeds the platform's own browser engine, so nothing is shipped
with the app (about 1 MB more):

| Platform | Engine                     | Notes                                  |
|----------|----------------------------|----------------------------------------|
| macOS    | WKWebView (Safari's WebKit) | Jitsi and Meet support Safari          |
| Windows  | WebView2 (Edge, Chromium)  | Preinstalled on Windows 10/11          |
| Linux    | WebKitGTK                  | Extra system package; weak WebRTC; X11 only as a child view |

The webview is a native view placed over a rectangle of the iced window
(a child of the window, from iced's window handle). iced lays out an empty
area; the app keeps the webview's bounds in sync with it.

Not considered: bundling Chromium (CEF), or a native video stack. Both
are the heavy machinery we want to avoid.

## How it would look

- **Call entry in the sidebar.** Below the spaces, like an extra space,
  shown only while a call is open. It shows the service and the room the
  call belongs to.
- **Call view.** Selecting the entry shows the call in the room area,
  with a thin bar: room name, "Open in browser", "Leave".
- **Moving away.** Opening another room hides the webview but keeps it
  alive, so audio continues. Clicking the entry brings it back.
- **Joining.** "Join" buttons (header, call cards) open the call view.
  A setting chooses "In Mtrx" or "In the browser". One call at a time;
  joining another asks to leave the first.

## Parts

- `mtrx-app/src/call_view.rs`: owns the webview (create, navigate,
  show/hide, set bounds, close). Reacts to window resizes and to the
  room-list width changing.
- Sidebar: the call entry; `Main` keeps `Option<OpenCall>` (room, url,
  webview).
- Overlays: iced cannot draw over a native view. Popovers, menus and
  hover help would be hidden under it. The call view therefore fills its
  own area with nothing iced draws on top, and the webview hides while a
  dialog (or Settings) covers the area.
- Bundle: `NSCameraUsageDescription` and `NSMicrophoneUsageDescription`
  in `Info.plist` (`cargo xtask bundle`). macOS then asks for camera and
  microphone access once.
- Links leaving the call (e.g. "open docs") go to the default browser.

## Risks to check first

1. **Google sign-in.** Google blocks signing in inside embedded browsers.
   Joining Meet as a guest likely works; joining with your Google account
   may not. Workaround if blocked: Meet always opens in the browser.
2. **Screen sharing** (`getDisplayMedia`) in WKWebView depends on the macOS
   version.
3. **Hidden webview.** Check that a hidden WKWebView keeps sending audio
   and video, and is not throttled.
4. **Camera and microphone prompts.** wry may need a WebKit delegate
   (`requestMediaCapturePermissionFor`) so the page is not refused.
5. **Linux.** WebKitGTK's WebRTC support is limited; on Wayland a child
   webview is not supported. Linux may stay "open in the browser".

## Spike (about a day)

A throwaway branch: one hard-coded call URL in the room area, no sidebar
entry. Check on macOS:

- [ ] Jitsi: camera, microphone, screen share.
- [ ] Meet as a guest, and signed in with a Google account.
- [ ] Switching rooms (webview hidden): the call keeps going.
- [ ] Resizing the window and the room list.

If Jitsi works and Meet works at least as a guest, build the full
version (a few more days). Otherwise keep the browser.

## Later

- Element Call (MatrixRTC): Matrix's own calls are a web app that Element
  embeds as a widget. The same webview could host it, with the widget API
  bridged to `matrix-sdk`'s widget support.
- A small floating picture of the call while in other rooms.
