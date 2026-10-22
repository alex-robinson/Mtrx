# iOS and Android: the shared Rust layer

Status: the Rust layer (`crates/mtrx-ffi`) and the first iOS and Android
apps (sections 3 and 4) are written. The library builds for both in CI
(`mobile-ffi.yml`); each app builds, passes its unit tests and starts to the
sign-in screen in CI, iOS on the simulator (`ios.yml`) and Android on an
emulator (`android.yml`). Unsigned. The iOS app was tried on a phone: it
signs in and lists rooms, but a new device cannot read encrypted rooms
without recovery or verification, which stage 1 (section 6) adds; the
library side of it is done. Stage 2 (section 7) organises the room list by
space; its library side is done too. This is the plan, and the contract the
apps are built on.

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
  `sendText(markdown)`, `markRead`.
- Listeners (implemented by the apps): `RoomListListener.onUpdate(RoomListState)`,
  `TimelineListener.onUpdate([TimelineItem])`,
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
  `TimelineEvent`, `EventContent`, `Message { kind, body, edited, media }`,
  `Media`, `SendState`. `TimelineItem.id` is unique and stable (also when a
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
is cancelled), and "Could not decrypt this message." rows.

```
apps/ios/
  Mtrx.xcodeproj      app target Mtrx, tests MtrxTests, local package MtrxFFI;
                      folders are synchronized groups (no per-file entries)
  Mtrx/
    MtrxApp.swift     entry; scene phase -> sync
    Core/             Keychain backend, Feed (listeners -> AsyncStream),
                      browser sign-in, error messages
    Models/           AppModel (phase: starting, signed out, signed in),
                      SignInModel, SessionModel (room list, connection, sync,
                      encryption state, verification requests),
                      RecoveryModel, VerificationModel, TimelineModel,
                      ImageCache, TimelineRows, RoomSections
    Views/            sign-in, room list (encryption banner), room
                      (timeline, composer), settings, recovery key,
                      verification
    Assets.xcassets   icon (logo.svg, full-bleed), logo, accent colour
  MtrxTests/          models with fakes of the FFI classes
```

- Threading: listeners yield into an `AsyncStream` that keeps only the
  newest state; a main-actor task applies it. Models are `@MainActor`
  `ObservableObject`s; views only read them.
- Room list: invites, direct messages, rooms (joined spaces left out, as on
  the desktop), most recently active first; search by name. Invites are
  listed but answered on the desktop (no join call in the FFI yet).
- Timeline: rows keyed by `TimelineItem.id`; runs of state events folded
  into one line (as the desktop writes them); messages grouped by sender.
  Bottom-anchored (`defaultScrollAnchor`) on iOS 17+, a flipped scroll view
  on iOS 16. Older messages load while the top is on screen; read when shown.
- Images: avatars and `mediaPreview`s decoded off the main thread, kept in
  an `NSCache` (the library caches the downloads).
- Tests: fakes subclass the generated classes (`init(noHandle:)`), so the
  models need no protocol layer.

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
are checked so far; the CI start does not check which screen shows);
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
  - `model/Images.kt`: avatars and image previews, decoded (downsampled)
    and kept in an in-memory LRU cache.
  - Screens: sign-in (homeserver, then password or "Sign in with browser" as
    `supportsOauth` says), room list (avatars, unread and mention counts,
    last activity, filters All, People, Rooms, Spaces; banners for offline
    and signed out by the server; sign-out), and a room's timeline
    (newest at the bottom, keyed by `TimelineItem.id`; day dividers, read
    marker, grouped bubbles with sender names and avatars, send states,
    images, state events summarised in a line as on the desktop; older
    events near the top; marked read while the newest message shows;
    composer with `sendText`). Navigation Compose: `rooms`, `room/{roomId}`.
  - Unit tests (JVM) on fakes of the library's objects (subclasses made with
    `NoHandle`): secrets, the account's sync and sign-out, timeline rows,
    the timeline's view model.
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
- Run only to the sign-in screen (emulator, CI); signing in, the room list
  and the timeline are not yet tried on an emulator or device. Not done:
  notifications, sending images, opening files, reactions and replies,
  release signing.

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
      a direct chat with a space member, invites in and outside spaces): all
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

## Later

- The room list can show a room before its `m.room.encryption` event has
  synced (Synapse creates a room's state one event at a time, and sync can
  deliver the room in between), so a message sent in that window goes out
  unencrypted. The FFI tests wait for `RoomSummary.encrypted`. A core issue
  to look at (for the desktop too): sending should wait until the room's
  encryption state is known, rather than the apps working around it.

- Push notifications (APNs, FCM; a push gateway, notification service
  extension / decryption in the background).
- Replies, reactions, edits, read receipts, typing, sending images.
- Several accounts; Face ID / biometric lock; verifying other people;
  setting up and resetting recovery from the apps.
- Timeline diffs instead of snapshots; log to OSLog and logcat.
- Signing and store distribution (TestFlight, Play internal testing).
