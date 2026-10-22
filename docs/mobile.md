# iOS and Android: the shared Rust layer

Status: the Rust layer (`crates/mtrx-ffi`) and the first iOS and Android
apps (sections 3 and 4) are written. The library builds for both in CI
(`mobile-ffi.yml`); each app builds, passes its unit tests and starts to the
sign-in screen in CI, iOS on the simulator (`ios.yml`) and Android on an
emulator (`android.yml`). Unsigned. The iOS app was tried on a phone: it
signs in and lists rooms, but a new device cannot read encrypted rooms
without recovery or verification, which stage 1 (section 6) adds; the
library side of it is done. Stage 2 (section 7) organises the room list by
space, and stage 3 (section 8) adds reactions, replies, edits, deleting,
read receipts and typing; their library sides are done too. This is the
plan, and the contract the apps are built on.

## Decisions

- Native UIs on one Rust core: SwiftUI on iOS, Jetpack Compose on Android,
  both on mtrx-core through UniFFI bindings (as Element X does).
- First version, the same on both: sign in with a password or in the
  browser (OAuth), room list, room timeline with text and images, send a
  text message, sign out. No push, calls, scheduled messages or Google.
- One account per app (profile `default`).
- Mobile builds run on GitHub Actions only (macOS runner for iOS, Ubuntu for
  Android). Locally we build and test for the Mac. Unsigned; for our team.
- Minimums: iOS 16; Android API 26.

## 1. Core changes

- [x] Secrets from the app: `SecretStore::Custom(Arc<dyn SecretBackend>)`.
      The OS keyring (`SecretStore::Keyring`) is desktop only, because
      keyring 4's `Entry` API does not build for iOS (its Apple store needs
      the `protected` feature there, `v1` enables only `keychain`) and has no
      store on iOS or Android (`Entry::new` fails with `NoDefaultStore`).
      keyring-core with `apple-native-keyring-store::protected` (iOS) or
      `android-native-keyring-store` would work, but one callback serves both
      apps. A custom store gets keys relative to the data directory
      (`profiles/default`), since iOS moves an app's container on updates.
- [x] `Profile::new` takes the data root; `with_cache_root` moves the SDK's
      event and media caches to a directory the OS may clear. The desktop
      default (`directories`) is not built for iOS and Android.
- [x] Browser login in two steps: `Session::start_oauth_login(profile,
      homeserver, redirect_uri)` returns the login page URL;
      `OAuthLogin::finish(redirect_url)` completes it. The desktop's
      `login_oauth` (local web server) is built on them. The mobile redirect
      is `APP_OAUTH_REDIRECT_URI` = `io.github.alex-robinson:/oauth`: a
      private-use scheme, which servers (MAS) require to be the client URI's
      host (`alex-robinson.github.io`) reversed.
- [x] `MediaRef` serializes (handed to the apps as an opaque string).
- [x] Other dependencies build for mobile as they are: reqwest with rustls
      (platform verifier, see Android below), bundled SQLite, `image`.
      `open` and notify-rust are in the CLI and app only.

## 2. The FFI (`crates/mtrx-ffi`)

UniFFI 0.32 with proc-macros; namespace `mtrx` (Swift module `MtrxFFI`
wrapping the C module `mtrxFFI`; Kotlin package `uniffi.mtrx`). Library
types `staticlib` (iOS), `cdylib` (Android), `lib`; binary `uniffi-bindgen`.
IDs are strings, times `Date`/`Instant`; no Ruma types.

- `Mtrx(dataDir, cacheDir, secrets)`: one per app. `restore() -> Session?`,
  `login(homeserver, user, password)`, `supportsOauth(homeserver)`,
  `startOauthLogin(homeserver) -> OAuthLogin`.
- `OAuthLogin`: `url()`, `finish(redirectUrl) -> Session`.
- `Session`: `userId`, `deviceId`, `homeserver`; `startSync`, `stopSync`,
  `reconnect`; `connection()`, `subscribeConnection(listener)`;
  `subscribeRoomList(listener)`; `timeline(roomId) -> Timeline`;
  `avatar(mxcUri, size) -> Data`, `mediaPreview(media) -> Data`,
  `mediaFile(media) -> path`; `logout`, `forget`. Encryption (section 6):
  `encryptionState()`, `recoveryStatus()`, `recover(recoveryKey,
  listener)`, `enableRecovery() -> key`, `isDeviceVerified()`, `devices()`,
  `requestSelfVerification() -> Verification`,
  `subscribeVerificationRequests(listener)`.
- `Verification`: `otherUser()`, `accept()`, `state()`, `subscribe(listener)`,
  `confirm()`, `mismatch()`, `cancel()`.
- `Timeline`: `roomId`, `subscribe(listener)`, `paginateBack(events)`,
  `sendText(markdown)`, `markRead`. Messages (section 8), each taking the
  row's `TimelineItem.id`: `reply(itemId, markdown)`, `edit(itemId,
  markdown)`, `react(itemId, key)` (adds ours, or removes it),
  `delete(itemId)` (redacts), `retry(itemId)`, `discard(itemId)`; and
  `setTyping(typing)`, `subscribeTyping(listener)`.
- Listeners (implemented by the apps): `RoomListListener.onUpdate(RoomListState)`,
  `TimelineListener.onUpdate([TimelineItem])`,
  `TypingListener.onUpdate([String])` (names; on every change only),
  `ConnectionListener.onUpdate(ConnectionState)`,
  `VerificationListener.onUpdate(VerificationState)`; each gets the whole
  state, now and after every change. `RecoveryListener.onUpdate(RecoveryStep)`
  gets each step of a recovery; `VerificationRequestListener.onRequest(Verification)`
  each incoming request. `SecretBackend`: `load`, `save`,
  `delete` (throws `SecretError.Failed`).
- `TaskHandle` (from each subscribe): `cancel()`, `isFinished()`; dropping
  it cancels.
- Records: `RoomListState { rooms, loaded, spaces, roomsOutsideSpaces }`
  (section 7), `SpaceRooms { spaceId, roomIds }`, `RoomSummary`, `TimelineItem
  { id, content }` (`start`, `event`, `dayDivider`, `readMarker`),
  `TimelineEvent` (with `editable`, `readBy: [Reader]`), `EventContent`,
  `Message { kind, body, edited, media, reactions, replyTo, rawBody }`,
  `Reaction { key, count, byUs, senders: [Reader] }`, `Reader { userId,
  name, avatarUrl }`, `ReplyQuote { eventId, state, senderId, senderName,
  kind, body }`, `QuoteState` (`loading`, `loaded`, `failed`), `Media`,
  `SendState` (`failed(error)`: retry or discard). `TimelineItem.id` is unique and stable (also when a
  local echo becomes the server's event): use it as list identity.
  `EventContent.unableToDecrypt`: an encrypted event without its key (yet).
  Encryption: `EncryptionState { deviceVerified, recovery }`,
  `RecoveryStatus` (`unknown`, `enabled`, `disabled`, `incomplete`),
  `RecoveryStep` (`checkingKey`, `importingIdentity`,
  `downloadingKeys(done, total)`), `VerificationState` (`waiting`,
  `emojis([VerificationEmoji])`, `confirmed`, `sharingKeys`, `done`,
  `cancelled(reason)`), `VerificationEmoji { symbol, description }`,
  `DeviceInfo { id, name, verified, current }`.
- Errors: `MtrxError` (`NotLoggedIn`, `AlreadyLoggedIn`, `LoginCancelled`,
  `RoomNotFound`, `InvalidInput`, `SecretStorage`, `Other(message)`).
  `InvalidInput(reason)`: a malformed ID or URL, or a recovery key that is
  not one or not the account's ("The recovery key is not valid").
- `oauthRedirectUri()`, `initLogging(filter)` (stderr: Xcode's console).

Full snapshots instead of diffs: simpler for the apps, and cheap at our
sizes. Diffs (the SDK's `VectorDiff`s) can come later for big timelines.

### Threading

- The library owns a multi-threaded tokio runtime. Each async call runs as a
  task there and is awaited from Swift's or Kotlin's executor; cancelling
  the Swift `Task` or Kotlin coroutine cancels it.
- Listeners and `SecretBackend` are called on the runtime's threads. The
  apps hop to the main thread before touching UI state (`@MainActor` /
  `DispatchQueue.main`; `Dispatchers.Main` / a `StateFlow`), and return
  quickly.
- Objects may be used from any thread. Swift releases them deterministically;
  in Kotlin call `close()` (or `cancel()` on a `TaskHandle`) instead of
  waiting for the garbage collector.

### Media

`avatar` and `mediaPreview` return encoded image bytes (cached in the SDK's
store); `mediaFile` saves the whole decrypted file under
`<cacheDir>/profiles/default/media/` and returns its path. Signing out
deletes it.

## 3. iOS app

- Build: `scripts/build-ios-ffi.sh` fills the Swift package
  `apps/ios/MtrxFFI` (`mtrxFFI.xcframework` for device and simulator, arm64;
  `Sources/MtrxFFI/mtrx.swift`). Add it as a local package; `import MtrxFFI`.
  CI's artifact `MtrxFFI-ios` is the same package, zipped.
- Start: `Mtrx(dataDir: Application Support, cacheDir: Caches, secrets:)`
  with a Keychain `SecretBackend` (generic passwords, service
  `org.mtrx.Mtrx`, account = key, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`).
  Then `restore()`; if nil, show sign-in.
- Sync: `startSync()` when the scene becomes active, `stopSync()` when it
  goes to the background (iOS suspends the process).
- Browser login: `ASWebAuthenticationSession(url: login.url(),
  callbackURLScheme: "io.github.alex-robinson")`; pass the callback URL to
  `finish`. No URL type in Info.plist is needed for this session.

### What exists

SwiftUI, iOS 16, iPhone and iPad, bundle `org.mtrx.Mtrx`; no team (signed
ad hoc for the simulator). Sign in
(the homeserver, then a password or the browser, as `supportsOauth` says),
room list, room timeline with text and images, sending text, sign out.
Stage 1 (section 6): the banner, Settings' "Encryption" section, the
recovery key screen with its steps, emoji verification in both directions
(one sheet over everything, Settings returns after it; a request that
comes while another is unanswered replaces it, one during a verification
is cancelled), "The recovery key is not valid." under the field, and
"Could not decrypt this message." rows. Stage 2 (section 7): the Chats
and Rooms tabs. Stage 3 (section 8): reactions, replies, edits,
deleting, retrying and discarding a refused send, read receipts and
typing.

```
apps/ios/
  Mtrx.xcodeproj      app target Mtrx, tests MtrxTests, local package MtrxFFI;
                      folders are synchronized groups (no per-file entries)
  Mtrx/
    MtrxApp.swift     entry; scene phase -> sync
    Core/             Keychain backend, Feed (listeners -> AsyncStream),
                      browser sign-in, error messages
    Models/           AppModel (phase: starting, signed out, signed in),
                      SignInModel, SessionModel (room lists, selected space,
                      connection, sync, encryption state, verification
                      requests), RecoveryModel, VerificationModel,
                      TimelineModel (also composing, actions, typing,
                      going to a quote), ImageCache, TimelineRows (rows,
                      menu entries), RoomLists, Emoji
    Views/            sign-in, home (Chats and Rooms tabs, encryption
                      banner), room (timeline, typing line, composer),
                      message sheets (who reacted or read, all emoji,
                      select text), settings, recovery key, verification
    Assets.xcassets   icon (logo.svg, full-bleed), logo, accent colour
  MtrxTests/          models with fakes of the FFI classes
```

- Threading: listeners yield into an `AsyncStream` that keeps only the
  newest state; a main-actor task applies it. Models are `@MainActor`
  `ObservableObject`s; views only read them.
- Home: a `TabView` with Chats and Rooms, each a `NavigationStack` (also on
  iPad) whose rooms open with the tab bar hidden (`.toolbar(.hidden, for:
  .tabBar)`); Settings is a sheet. `RoomLists` derives the chats and the
  spaces (with their rooms and unread dots, then "Other rooms") from each
  `RoomListState`. Rooms is a paged `TabView` (spaces, the selected space's
  rooms; a "Spaces" button goes back too); the page and the selected space
  live in `SessionModel`, the space in UserDefaults (`selectedSpace`: a
  space ID or `other-rooms`), removed at sign-out. The encryption banner
  is the first row of each list; each list has a search by name, names
  starting with it first. Invites are listed but answered on the desktop
  (no join call in the FFI yet).
- Timeline: rows keyed by `TimelineItem.id`; runs of state events folded
  into one line (as the desktop writes them); messages grouped by sender.
  Bottom-anchored (`defaultScrollAnchor`) on iOS 17+, a flipped scroll view
  on iOS 16. Older messages load while the top is on screen; read when shown.
  Others' messages outside direct chats have the sender's name above the
  first of a group and their avatar (32 pt) beside its bubble, with the
  avatars' column kept for the rest of the group; direct chats show
  neither, as before.
- Messages (section 8): holding a bubble opens a context menu (the
  bubble is its preview) with what `MessageRow.actions` allows: on iOS
  17 the six common emoji as a palette (ours selected), then "More
  Reactions" (a palette fits six, so no "+" in it); on iOS 16 a "React"
  submenu with both. Then Reply, Copy, Select Text (a sheet with a
  `UITextView`), Edit (`editable`), Delete (own, sent; confirmed), Retry
  and Discard (failed). React and Reply once `eventId != nil`. The full
  picker is a searchable grid of the emoji the system's Unicode data
  marks as emoji by default, one character each, searched by Unicode
  name (no skin tones, flags or sequences). Bubbles no longer have
  `textSelection` (its long press would take the menu's).
- Reactions: chips under the bubble, wrapping (`FlowLayout`); ours in
  the accent colour; tap toggles, hold lists who reacted. Quotes: a bar,
  the sender and three lines inside the bubble (above an image, in a
  bubble of its own); a file's kind and name as "Image: cat.png"; tapping
  a loaded one scrolls to the original and marks it for 2 s, after
  `paginateBack(50)` as needed (the model checks each new item list; at
  the start it waits for the items with the start before saying it is
  not found). Read receipts: 16 pt avatars at the row's right, three
  then "+N"; tap lists them. "(edited)" follows the text.
- Composer: a bar "Replying to <name>" with the first line, or "Editing"
  (the field holds `rawBody`; the draft from before comes back after);
  ✕ cancels; a send that fails restores both. `setTyping` on every
  change of the draft (calls chained so they arrive in order), `false`
  when the room disappears. The typing line sits above the composer
  (always one line high); three people are "Anna, Ben and Cleo are
  typing…", more "Anna, Ben and N others…". `subscribeTyping` runs
  while the room is on screen.
- Actions (`react`, `delete`, `retry`, `discard`) run in tasks the model
  creates; sending, replying and editing in `send()`. Back-paginations
  are chained, so loading older messages and looking for a quote do not
  overlap.
- Images: avatars and `mediaPreview`s decoded off the main thread, kept in
  an `NSCache` (the library caches the downloads).
- Tests: fakes subclass the generated classes (`init(noHandle:)`), so the
  models need no protocol layer. Stage 3: rows carrying reactions, quote,
  edited and readers; menu entries for own, others', unsent, failed,
  deleted and undecryptable messages; "+N"; quote texts; emoji search;
  replying, editing, cancelling and a failed reply; typing calls, names
  and lines; the actions; going to a quote (shown, after paginating, at
  the room's start, after 20 calls).

Locally: run `scripts/build-ios-ffi.sh` once (or unzip CI's `MtrxFFI-ios`
into `apps/ios`), open `apps/ios/Mtrx.xcodeproj`, run on a simulator; for a
device, choose a team under Signing. From the command line:

```
cd apps/ios
xcodebuild build -project Mtrx.xcodeproj -scheme Mtrx \
  -destination 'generic/platform=iOS Simulator'
xcodebuild test -project Mtrx.xcodeproj -scheme Mtrx \
  -destination 'platform=iOS Simulator,name=<an iPhone>'
```

Simulator builds are signed ad hoc, Xcode's default there; no team is
needed. Do not turn signing off (`CODE_SIGNING_ALLOWED=NO`): an unsigned app
has no application identifier, and the Keychain refuses it ("A required
entitlement is not present"), so no account can be opened.

CI (`.github/workflows/ios.yml`): builds the package with the script, the
app (Release) for the simulator, runs the unit tests on the first iPhone
simulator, starts the app there (it must still run after 15 s, without a
crash report; the screenshot is the artifact `ios-smoke`), and uploads
`Mtrx-ios-simulator` (the zipped `.app`; install with `xcrun simctl install
booted Mtrx.app`). Passes; the screenshot shows the sign-in screen.

To do: signing in, the room list, the timeline, recovery and verification
tried against a server (only the models' unit tests and the sign-in screen
are checked so far; the CI start does not check which screen shows; the
stage 3 room was seen in the simulator with made-up items only);
opening images full size and
files (`mediaFile`); answering invites; reconnecting when the network
returns (`reconnect`); a build for devices (signing).

## 4. Android app

Jetpack Compose (Material 3) in `apps/android`, a Gradle project with a
version catalog (`gradle/libs.versions.toml`: AGP 8.10.1, Kotlin 2.1.21 with
its Compose compiler plugin, Compose BOM 2025.05.01; Gradle 8.14.3, JDK 17).
Application id `org.mtrx.app`, minSdk 26, target and compile SDK 36. Built
and tested by CI only; no Android SDK is used locally.

- `:mtrx-ffi`, an Android library: `scripts/build-android-ffi.sh` fills it
  with `src/main/jniLibs/{arm64-v8a,x86_64}/libmtrx_ffi.so` and
  `src/main/kotlin/uniffi/mtrx/mtrx.kt` (gitignored), next to the committed
  `org/mtrx/ffi/AndroidPlatform.kt`. Dependencies: JNA
  (`net.java.dev.jna:jna:5.17.0@aar`), `kotlinx-coroutines-android`,
  `androidx.annotation`, and the platform verifier's Kotlin part
  (`org.rustls:rustls-platform-verifier`, an aar from the crate's Maven
  repository on GitHub, at the version of `rustls-platform-verifier-android` the build
  reads from Cargo.lock). Consumer R8 rules keep `com.sun.jna.**`,
  `uniffi.mtrx.**` and `org.rustls.platformverifier.**`.
- `:app` (`org.mtrx.app`), only the ABIs the library is built for:
  - `MtrxApplication`: `AndroidPlatform.initialize` first, then
    `Mtrx(filesDir, cacheDir, secrets)` and the `Account`; sync follows
    `ProcessLifecycleOwner` (start in the foreground, stop in the background).
  - `model/Secrets.kt`: `FileSecretBackend`, one file per secret in
    `noBackupFilesDir/secrets` named by the SHA-256 of its key, encrypted
    with AES-256-GCM (random IV, the key as associated data) by a key that
    stays in the Android Keystore (alias `mtrx-secrets`). Backups and device
    transfer are off, as the key cannot follow the data.
  - `model/Account.kt`: the session holder (no Compose): `restore()` at
    start, sign-in, sign-out and `forget()`; state, room list and connection
    as `StateFlow`s set by the listeners (on the library's threads); session
    changes and sync under one mutex; subscriptions cancelled and closed at
    sign-out. Browser sign-in: `startOauthLogin`, the page in a Custom Tab,
    and `OAuthRedirectActivity` (intent filter for scheme
    `io.github.alex-robinson`; the path `/oauth` is checked in code, as
    Android ignores path filters without a host) calls `finish` and returns
    to `MainActivity`. Back from the browser without the redirect cancels.
    Encryption (section 6) too: `encryption` (asked when sync starts, once
    the room list is loaded, and again with its updates while recovery is
    `unknown`; after a recovery and a verification), `recover` with its
    steps as a flow (in the account's scope, so leaving the screen does not
    stop it), our `devices`, and the verification: the request subscription
    is one of the session's; a request from another user is cancelled, one
    from our own device is offered (one at a time; declined while a
    verification runs).
  - `model/VerificationModel.kt`: a `Verification` and its subscription,
    held by the `Account` (outgoing or accepted), its state, `confirm`,
    `mismatch`, `cancel` in the account's scope; `close()` cancels it unless
    it ended, then closes both. `model/Encryption.kt`: the banner rule and
    the recovery's state (`InvalidInput` from `recover` is
    `Recovery.InvalidKey`: "The recovery key is not valid." under the
    field, the key kept).
  - `model/Spaces.kt` (section 7): `SpaceEntry` (a space, or "Other
    rooms") and `SpaceSelection`, held by the `Account`: the selected entry
    while the room list has it, saved through `SelectedSpaceStore`
    (`model/ViewPreferences.kt`: Preferences DataStore `view`, key
    `selected_space`; `androidx.datastore:datastore-preferences`); a saved
    one waits for a list that has it and is forgotten when a loaded list
    does not; cleared at sign-out (and `forget`). `RoomSortChoice`, also
    the `Account`'s: the order of the spaces' rooms (`RoomSort`: Default,
    A to Z, Z to A, Most recent), saved through `RoomSortStore` (key
    `room_sort`), kept at sign-out (the device's, not the account's).
    `rooms/RoomLists.kt`: the tabs' lists from each `RoomListState`
    (`chats`, `spaceRows` with the unread dot, `roomsOf`, and `sectionsOf`:
    a space's rooms and its direct chats apart, each `sorted`: by name with
    the locale's `Collator`, case aside; most recent by `lastActivity`, never
    active last; ties in list order).
  - `model/Images.kt`: avatars and image previews, decoded (downsampled)
    and kept in an in-memory LRU cache.
  - Screens: sign-in (homeserver, then password or "Sign in with browser" as
    `supportsOauth` says), the home (`ui/HomeScreen.kt`, section 7: a
    bottom bar with Chats, Spaces and Settings, only on this screen; Chats
    the direct chats; Spaces a `HorizontalPager` of the spaces, with "Other
    rooms", and the selected entry's rooms, titled with its name, back or
    the arrow to the spaces; tapping the Spaces tab again also returns to
    them; a new selection, tapped or restored, pages right; the entry's
    page in two sections, Rooms and People (its direct chats; each left
    out while empty), in the order chosen in the top bar's sort menu, for
    every space; Settings the account, encryption and "Sign out" (after
    "Sign out?"); tab, page and scroll positions are the screen's saved
    state, so they are kept while a room, recovery or verification is
    open; rows with avatars, unread and mention counts, last activity,
    "Invited"; banners for offline and signed out by the server over all
    tabs), and a room's timeline (the room's name, and for a direct chat
    the room's avatar beside it, both tapped for the view model's
    `showPersonInfo`, which does nothing yet; newest at the bottom, keyed
    by `TimelineItem.id`; day dividers, read marker, grouped bubbles, send
    states, images, state events summarised in a line as on the desktop;
    older events near the top; marked read while the newest message
    shows; "Could not decrypt this message." for an event without its
    key; away from the bottom, a round down-arrow button over the
    messages at the bottom right goes to the newest). Messages (section 8): the sender's name in
    the body text size above a group, the avatar beside its first bubble;
    holding a bubble opens a bottom sheet with the six common reactions
    (ours marked) and "+" for the full picker (`androidx.emoji2:
    emoji2-emojipicker`'s `EmojiPickerView` in a dialog, themed with
    AppCompat's DayNight, which its resources need), then Reply, Copy,
    Select text, Edit, Delete (after "Delete this message for everyone?"),
    Retry and Discard, as `timeline/TimelineRows.kt`'s `messageActions`
    allows; swiping a bubble left (a horizontal `draggable`, so vertical
    drags scroll and edge swipes stay the system's back) shows the reply
    icon and, past 64 dp when let go, replies as Reply does, for the
    messages Reply is offered for (`EventRow.canReply`); it springs back;
    reaction chips under the bubble (tap toggles, hold lists who);
    the reply's quote in the bubble (tap goes to the original); "(edited)"
    muted after the text; read receipts as up to three 16 dp avatars and
    "+N" on a line under the message at the right margin (tap lists
    them); a typing line above the composer (`subscribeTyping`, its
    handle cancelled and closed when the room is left); the composer's
    bar for a reply or an edit with ✕, an edit starting from `rawBody`
    and giving back the earlier text when it ends; `setTyping` on every
    change, one call at a time in order, and `false` when the room is
    left. The timeline's view model takes each action's row from the
    latest rows; sends and actions run in a scope of the model's own, not
    `viewModelScope`, which leaving the room cancels (a cancelled call
    aborts the library's task); the timeline is closed once the last
    typing notice is sent. Going to a quote's original pages back 50
    events at a time, at most 20 times, looking in each new list; after
    the room's start or the last page it waits up to two seconds for
    their items before saying it was not found. Encryption: the banner over
    Chats and Spaces (opens the Settings tab); the Settings tab with the
    account and "Encryption" (status lines, "Enter recovery key", "Verify
    with another device", or the note that another device is needed); the
    recovery key screen with its steps and progress; the verification
    screen (one for both directions; back cancels it while it runs); the
    prompt for a request, over any screen. Navigation Compose: `home`,
    `room/{roomId}`, `settings/recovery`, `settings/verification`. Icons
    the core set lacks (spaces, reply, sort) are drawn from their Material
    paths (`ui/Icons.kt`).
  - Unit tests (JVM) on fakes of the library's objects (subclasses made with
    `NoHandle`): secrets, the account's sync and sign-out, timeline rows,
    the timeline's view model; messages (section 8): the rows' reactions,
    quotes, edits and readers ("+N"), the menu's entries for others',
    our, unsent and failed messages, replying and editing in the
    composer, typing both ways and on leaving the room (a send under way
    still completes), the typing line's texts, and going to a quote's
    original (loaded already, after paginating, arriving after the call,
    at the room's start, after 20 pages); swiping to reply only to sent
    messages, and no reply to one not sent; the encryption state and banner, the
    recovery's steps and errors (an invalid key), the verification model
    (states, confirm, mismatch, cancel, close), and verification requests
    (another user's cancelled, ours accepted or declined); the Chats and
    Rooms lists from a `RoomListState` like the library's integration test
    (two top-level spaces, a nested one, direct chats, invites), a space's
    Rooms and People sections and the four orders (names in either case,
    ties, rooms never active), the selected space (saved, restored at the
    next launch, forgotten when gone or at sign-out), and the chosen order
    (saved, restored, a choice before the saved one is read wins, kept at
    sign-out).
- Build locally, with the Android SDK and NDK, Rust's two Android targets
  and cargo-ndk: `scripts/build-android-ffi.sh`, then in `apps/android`
  `gradle :app:assembleDebug` (Gradle 8.11.1 or later, or a wrapper made with
  `gradle wrapper --gradle-version 8.14.3`; `local.properties` or
  `ANDROID_HOME` for the SDK). Tests: `gradle :app:testDebugUnitTest`.
- CI (`.github/workflows/android.yml`, Ubuntu): the library and bindings
  with the build script, then the debug APK, artifact `Mtrx-android-debug`,
  then the unit tests, then `scripts/android-smoke.sh` on an x86_64 emulator
  (API 34): installs and starts the APK, checks that the sign-in screen
  shows, the process lives and the log has no crash; screenshot, UI tree and
  log as artifact `android-smoke`.

Remaining:

- Each CI run signs with a new debug key, so an update needs the previous
  build uninstalled; a shared debug keystore (in the repository or a CI
  secret) fixes that.
- Run only to the sign-in screen (emulator, CI); signing in, the room list,
  the timeline, recovery and verification are not yet tried on an emulator
  or device (only their models' unit tests); nor are the messages'
  menus, sheets and the emoji picker, swiping to reply, the button to the
  newest message, and the Settings tab. Not done: notifications, sending
  images, opening files, release signing.

## 5. Builds and CI

- [x] `scripts/generate-ffi-bindings.sh`: host build, Swift and Kotlin into
      `target/bindings/` (checks the API goes through UniFFI). Swift
      bindings compiled and run against the macOS library locally.
- [x] `scripts/build-ios-ffi.sh`, `scripts/build-android-ffi.sh` (profile
      `mobile`: release with LTO; `--debug` for a quicker build). Generated
      files are gitignored. The bindgen binary links the library, so each
      script also builds mtrx-ffi once for the host.
- [x] `.github/workflows/mobile-ffi.yml`: iOS (xcframework, the package
      built for the simulator, a simulator executable linked against it) and
      Android (cargo-ndk with the runner's NDK; checks the JNI entry point).
      Both pass.
- [x] Tests: conversions and the runtime (unit); sign-in, room list,
      timeline, image, send and sign-out; recovery (a new device reads an
      encrypted message after the recovery key, in its open timeline); and
      emoji verification between two devices, after which the new one reads
      history, also in its open timeline; the rooms by space (nested spaces,
      a direct chat with a space member, invites in and outside spaces);
      messages between two people (a reply and its quote; a reaction by
      each, counted and named on both sides, and removed again; an edit,
      marked edited; a deletion; typing, by name; a read receipt moving to
      the message read, with the reader's name); a send the server refuses,
      retried and then discarded, after which the next message goes: all
      through the bindings against Synapse (`cargo test -p mtrx-ffi --test
      integration -- --ignored`, in the Linux workflow).

## 6. Stage 1: encryption

A new device has no keys for earlier messages, so encrypted rooms show
nothing readable until the user enters the recovery key or verifies the
device with another one (the desktop app, say). Both apps add the same four
things; the library part is done and tested (section 5).

**State and banner.** `Session.encryptionState()` returns
`EncryptionState { deviceVerified, recovery }`. There is no subscription
(the core has no stream for it): ask (a) when the room list first reports
`loaded == true` after `startSync()`, (b) after `recover` returns, (c) after
a verification reaches `done`, (d) when the app returns to the foreground.
Keep the result in the session model (iOS `SessionModel`, Android
`Account`). Show a banner at the top of the room list when
`!deviceVerified || recovery == incomplete`; not while `recovery ==
unknown` (no sync yet: ask again at the next moment above). Text: "Verify
this device or enter your recovery key to read encrypted messages."; a tap
opens the encryption settings.

**Settings, section "Encryption".** Status lines: "This device: verified /
not verified", "Recovery: set up / not set up / not unlocked on this
device" (`enabled` / `disabled` / `incomplete`). Two actions while the
banner would show:
- "Enter recovery key" (unless `recovery == disabled`): a screen with a
  multi-line text field (monospaced, no autocorrection or capitalisation;
  spaces are fine) and a Recover button calling
  `recover(recoveryKey:listener:)`. The listener's steps, in order:
  `checkingKey` ("Checking the key"), `importingIdentity` ("Verifying this
  device"), then `downloadingKeys(done, total)` once per room (a progress
  bar, "Fetching keys: done of total rooms"; there may be none). It can
  take a minute with many rooms. A mistyped key, or one of another
  account or an older backup, fails at `checkingKey` with
  `MtrxError.InvalidInput("The recovery key is not valid")`: show the
  reason under the field and keep the key for correcting. Other errors are
  `MtrxError.Other(reason)`: "Could not recover." with the reason below. On
  success go back to Settings and ask `encryptionState()` again.
- "Verify with another device" (while `!deviceVerified`):
  `requestSelfVerification()` returns a
  `Verification`; open the verification screen with it. (`devices()` lists
  our devices, `current` marking this one; if no other device is listed, say
  that one is needed and offer only the recovery key.)
`enableRecovery()` (set up recovery, returns the new key to show once) is
there, but not part of stage 1's UI: accounts set it up on the desktop.

**Verification screen**, one for both directions, driven by
`verification.subscribe(listener)` (the whole `VerificationState` each time;
it ends after `done` or `cancelled`):
- `waiting`: "Accept the request on your other device." (outgoing) or
  "Waiting for the other device…" (incoming, after accepting); a Cancel
  button → `cancel()`.
- `emojis(emojis)`: the seven emoji in order, in a grid (the `symbol` large,
  its `description` below), "Do they match the other device?", buttons
  "They match" → `confirm()` and "They do not match" → `mismatch()`.
- `confirmed`: "Waiting for the other device to confirm…".
- `sharingKeys`: "Finishing… keep the app open."
- `done`: "This device is verified." and a Done button; ask
  `encryptionState()` again.
- `cancelled(reason)`: "Verification was cancelled." with the reason; Close.
`state()` gives the current state without waiting. Keep the `Verification`
(and its `TaskHandle`) in a model, not a view: dropping it stops following
the protocol and the other device times out. In Kotlin `close()` both when
the screen is done.

**Incoming requests.** Right after `startSync()` (with the other
subscriptions), `subscribeVerificationRequests(listener)`; keep the handle
for the session and cancel it at sign-out. Requests arrive only while sync
runs. On `onRequest(verification)`: if `verification.otherUser() !=
session.userId()` (another person: not in stage 1) call `cancel()`.
Otherwise show "Another device wants to verify this one." with Accept →
`accept()`, then the verification screen (it starts at `waiting`; the other
device starts the comparison), and Decline → `cancel()`.

**Timelines.** Show `EventContent.unableToDecrypt` as a muted, italic row
"Could not decrypt this message." (grouped like messages; not hidden).
After a recovery or verification such rows turn into the messages by
themselves, in timelines already open: the listener gets the new items; the
apps need not reopen anything.

**Threading.** As for all calls (section 2): async calls are awaited from
Swift's or Kotlin's executor and listeners run on the library's threads,
so hop to the main thread before touching UI state. `recover`'s listener
is called during the call. Cancelling the awaiting Swift `Task` or Kotlin
coroutine cancels the call, so run `recover` and `confirm` from the model
(iOS: a `Task` the model owns; Android: `viewModelScope` or the `Account`'s
scope), not from a view's lifetime.

**Errors and tests.** No new `MtrxError` cases (iOS switches over them
exhaustively). App tests use fakes as before: subclass `Verification` with
`init(noHandle:)` / `NoHandle`, and `Session` for `encryptionState`,
`recover`, `requestSelfVerification` and `subscribeVerificationRequests`.

## 7. Stage 2: organising chats

The room list is one long list today. The desktop organises it by space: a
column of spaces, then the rooms of the selected space. The apps get the
same lists from `RoomListState`, computed by the library as the desktop
computes them (`mtrx_core::RoomsBySpace`); the library part is done and
tested (section 5).

**Layout.** A bottom bar with two tabs, Chats and Rooms; hidden while a
room is open. Tapping a room pushes the room screen; the system back
gesture returns to the list it came from (Chats, or the space's rooms),
which keeps its tab, page and scroll position.

**Chats.** Direct chats only, newest message first:
`rooms.filter { kind == direct }`, in the order of `rooms`. Nothing new is
needed: `rooms` is already ordered by recency (the SDK's: latest message or
recency stamp, rooms with unsent messages first). Keep that order; do not
re-sort by `lastActivity`, which is nil for some rooms (an invite, say).
Show `lastActivity` as the time. Invites to direct chats are there
(`invited`), and the Notes to self (`directUserId == session.userId()`).

**Rooms.** A horizontal pager of two pages, like the desktop's two columns.
- Left page, the spaces: `spaces` in order (top-level spaces, most
  recently active first, as the desktop's space column), each shown by its
  `RoomSummary` (look up `spaceId` in `rooms` for the name and avatar), with
  a dot while any of its `roomIds` has `unreadMessages > 0`. Then one entry
  "Other rooms" for `roomsOutsideSpaces`, left out while that is empty.
- Tapping an entry selects it and pages to the right; the right page shows
  the selected entry's rooms: `roomIds` (or `roomsOutsideSpaces`) mapped to
  their `RoomSummary`s, in that order, under the space's name. Swiping right,
  or back on Android, returns to the spaces.
- Remember the selected entry (a space ID, or "other rooms") across
  launches: in the app's preferences (UserDefaults / DataStore), deleted at
  sign-out. At start, while it is in `spaces`, select it and open on the
  right page; the first states may be a cache, so wait until `loaded` before
  deciding it is gone. If it is gone (the space was left, or it became
  nested in another), or when it disappears while shown, show the spaces
  page with nothing selected. Without a saved entry, open on the spaces
  page. (The desktop saves its space and room in `view.json` the same way.)

**What each list holds** (the desktop's rules, applied by the library):
- `spaces`: joined spaces in no joined space. Nested spaces are not
  entries of their own: their rooms are listed in the top-level space.
- A space's `roomIds`: every room below it, through nested spaces (by the
  spaces' `m.space.child` events); invites to rooms and spaces below it;
  direct chats with someone who is a joined member of it; rooms the user put
  into one of its categories on the desktop. Not the nested spaces
  themselves. A room can be in several spaces. The Notes to self is a direct
  chat with a member (the user), so every space lists it, as the desktop
  shows it in every space.
- Direct chats join a space's list once the space's members are loaded: an
  update shortly after the space first appears (a network call per space
  then, and again when its number of joined members changes).
- `roomsOutsideSpaces`: rooms no space lists, and invites to spaces outside
  them. No direct chats (they are all in Chats) and no joined spaces.
- Invites (`invited`) are in the lists they would belong to once joined;
  mark them as invites. They are answered on the desktop (no join call yet).
- Everything is in the order of `rooms`, most recently active first.

Differences from the desktop, by design: its first entry is Home, with all
rooms, where the apps have "Other rooms"; and it splits a space's rooms into
sections (invites, favourites, the user's categories, rooms, people, low
priority), where the apps show one list in the order of `rooms`.

**Updates and tests.** Each `RoomListState` is whole: derive the lists from
each one (cheap at our sizes); rows are keyed by room ID. The new fields
have defaults, so `RoomListState(rooms:loaded:)` in the apps' fakes still
builds; tests of the lists pass `spaces` and `roomsOutsideSpaces`. No new
`MtrxError` cases.

## 8. Stage 3: messages

The timeline shows text and images today. Stage 3 brings what the desktop
does with messages: reactions, replies, edits, deleting, read receipts,
typing, and retrying a failed send. The library part is done and tested
(section 5); everything comes in the timeline's items, whole, as before.

**Rows and actions.** Every action on a message takes its row's
`TimelineItem.id` (`itemId`), which identifies also a message not sent yet
(it has no `eventId` until then). Pass the ID of a row from the latest
items; a row gone meanwhile (or a reply to a message not yet sent) fails
with `MtrxError.InvalidInput`. Server failures, and `retry` of a message
that did not fail or `discard` of one already sent, are `MtrxError.Other`:
show the reason in a short alert or snackbar.

**Messages.**
- Sender: the name (`senderName`, else the ID's localpart) in the normal
  text size above the first bubble of a group (it is too small today), and
  the avatar (`senderAvatarUrl`) beside the bubble, at its side (others'
  messages on the left), not in a corner of the screen. Not for our own.
- "(edited)", muted, after the text when `Message.edited`.
- Deleted (`EventContent.redacted`): a muted, italic "Message deleted".

**Holding a message** opens a menu (iOS: a context menu with a preview;
Android: a bottom sheet), offering only what applies:
- React: a row of common emoji (👍 ❤️ 😂 😮 😢 🎉, ours highlighted) and
  "+" for a full picker → `react(itemId, key)`.
- Reply → the composer replies (below).
- Copy (`Message.body`) and Select text (a sheet with the text selectable).
- Edit, when `TimelineEvent.editable` → the composer edits (below).
- Delete, when `own && eventId != nil`, after a confirmation ("Delete this
  message for everyone?") → `delete(itemId)`.
- For `sendState == failed`: Retry → `retry(itemId)`, Discard →
  `discard(itemId)`. Later messages wait until it is retried or
  discarded. `waiting` (offline) needs nothing: it is sent when the server
  is back.
React and Reply need a sent message: offer them once `eventId != nil`.

**Reactions** (`Message.reactions`, in the order first given): chips under
the bubble, each the `key` and its `count`; ours (`byUs`) highlighted with
the accent colour. Tap → `react(itemId, key)` (adds ours or removes it).
Long-press → a sheet listing `senders` (avatar and name; `name` may be nil
until known: show the ID's localpart).

**Replies.** `Message.replyTo` is the quote, drawn above the text inside
the bubble: a bar in the accent colour, the sender (`senderName`, else
`senderId`'s localpart) in semibold, and up to three lines of `body`
(for `kind` image, video, audio or file: "Image", "Video", "Audio" or
"File" and the name). While `state == loading`: "Loading…" (the library
fetches the original, also when it is far back; the quote updates by
itself). `failed`: "Could not load the message", not tappable. Tapping a
loaded quote goes to the original, as the desktop does: find the row whose
`event.eventId == quote.eventId`; if it is there, scroll to it and
highlight it for two seconds; if not, call `paginateBack(50)` and look
again in each new item list (not only when the call returns: the items
arrive separately), up to 20 calls, until found. Stop with "The original
message was not found in the room's history" when `paginateBack` returned
`true` (the room's start) and the items after it lack it, or after 20
calls. Keep this in the timeline model; show a spinner while loading.

**Composer.**
- Reply: a bar above the field, "Replying to <name>" and the first line of
  the message, with ✕ to cancel; Send → `reply(itemId, text)` instead of
  `sendText`, then the bar closes.
- Edit: the field filled with `Message.rawBody` (the Markdown we wrote),
  a bar "Editing" with ✕ to cancel; Send → `edit(itemId, text)`. Only one of replying and editing at a time.
- Typing: `setTyping(true)` when the text changes and is not empty,
  `setTyping(false)` when it is emptied, sent, or the room is left (the
  library throttles: call it on every change).

**Typing line.** `subscribeTyping(listener)` when the room opens (cancel
the handle when it closes) gives the names of the others typing, on every
change (nobody at first). One muted line just above the composer, its
height kept while empty so the timeline does not jump: "Anna is typing…",
"Anna and Ben are typing…", "Anna, Ben and 2 others are typing…".

**Read receipts.** `TimelineEvent.readBy`: the others whose receipt is on
this event, i.e. who read up to here (never us; receipts on state events
are moved to the message before, so folded state lines lose none). Small
round avatars (16 pt/dp) at the right margin of that message's row, at most
three, then "+N"; tap → a sheet listing them (avatar and name). Names and
avatars may arrive in a later update. `markRead()` as today.

**Threading and errors.** As before: the actions are async, run them from
the model (iOS: a `Task` the model owns; Android: `viewModelScope`), not
from a view's lifetime, so leaving the screen does not cancel a send.
`TypingListener` is called on the library's threads: hop to the main
thread. No new `MtrxError` cases.

**Tests.** The new record fields have defaults (`reactions: []`, `replyTo:
nil`, `rawBody: ""`, `editable: false`, `readBy: []`), so the apps' fakes
build unchanged. Fakes of `Timeline` subclass it (`init(noHandle:)` /
`NoHandle`) and override the actions to record their calls, and
`subscribeTyping` to keep the listener and call it. Worth testing in the
models: which menu entries show for own, others', failed and unsent
messages; reply and edit state in the composer;
the quote jump (found at once, after paginating, at the room's start, after
20 calls); typing line texts; reader overflow ("+N").

## Later

- The room list can show a room before its `m.room.encryption` event has
  synced (Synapse creates a room's state one event at a time, and sync can
  deliver the room in between), so a message sent in that window goes out
  unencrypted. The FFI tests wait for `RoomSummary.encrypted`. A core issue
  to look at (for the desktop too): sending should wait until the room's
  encryption state is known, rather than the apps working around it.

- Push notifications (APNs, FCM; a push gateway, notification service
  extension / decryption in the background).
- Sending images; mentions with `@` completion; forwarding.
- Several accounts; Face ID / biometric lock; verifying other people;
  setting up and resetting recovery from the apps.
- Timeline diffs instead of snapshots; log to OSLog and logcat.
- Signing and store distribution (TestFlight, Play internal testing).
