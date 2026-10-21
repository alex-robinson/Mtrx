# iOS and Android: the shared Rust layer

Status: the Rust layer (`crates/mtrx-ffi`) and its builds are done, and the
first iOS app (section 3); the Android app is to do. This is the plan, and
the contract the apps are built on.

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
  `mediaFile(media) -> path`; `logout`, `forget`.
- `Timeline`: `roomId`, `subscribe(listener)`, `paginateBack(events)`,
  `sendText(markdown)`, `markRead`.
- Listeners (implemented by the apps): `RoomListListener.onUpdate(RoomListState)`,
  `TimelineListener.onUpdate([TimelineItem])`,
  `ConnectionListener.onUpdate(ConnectionState)`; each gets the whole
  state, now and after every change. `SecretBackend`: `load`, `save`,
  `delete` (throws `SecretError.Failed`).
- `TaskHandle` (from each subscribe): `cancel()`, `isFinished()`; dropping
  it cancels.
- Records: `RoomListState { rooms, loaded }`, `RoomSummary`, `TimelineItem
  { id, content }` (`start`, `event`, `dayDivider`, `readMarker`),
  `TimelineEvent`, `EventContent`, `Message { kind, body, edited, media }`,
  `Media`, `SendState`. `TimelineItem.id` is unique and stable (also when a
  local echo becomes the server's event): use it as list identity.
- Errors: `MtrxError` (`NotLoggedIn`, `AlreadyLoggedIn`, `LoginCancelled`,
  `RoomNotFound`, `InvalidInput`, `SecretStorage`, `Other(message)`).
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

SwiftUI, iOS 16, iPhone and iPad, bundle `org.mtrx.Mtrx`, unsigned. Sign in
(the homeserver, then a password or the browser, as `supportsOauth` says),
room list, room timeline with text and images, sending text, sign out.

```
apps/ios/
  Mtrx.xcodeproj      app target Mtrx, tests MtrxTests, local package MtrxFFI;
                      folders are synchronized groups (no per-file entries)
  Mtrx/
    MtrxApp.swift     entry; scene phase -> sync
    Core/             Keychain backend, Feed (listeners -> AsyncStream),
                      browser sign-in, error messages
    Models/           AppModel (phase: starting, signed out, signed in),
                      SignInModel, SessionModel (room list, connection, sync),
                      TimelineModel, ImageCache, TimelineRows, RoomSections
    Views/            sign-in, room list, room (timeline, composer), settings
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
  -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO
xcodebuild test -project Mtrx.xcodeproj -scheme Mtrx \
  -destination 'platform=iOS Simulator,name=<an iPhone>' CODE_SIGNING_ALLOWED=NO
```

CI (`.github/workflows/ios.yml`): builds the package with the script, the
app (Release) for the simulator, runs the unit tests on the first iPhone
simulator, and uploads `Mtrx-ios-simulator` (the zipped `.app`; install
with `xcrun simctl install booted Mtrx.app`). Not run yet.

To do: opening images full size and files (`mediaFile`); answering
invites; reconnecting when the network returns (`reconnect`); a build for
devices (signing).

## 4. Android app

Jetpack Compose (Material 3) in `apps/android`, a Gradle project with a
version catalog (`gradle/libs.versions.toml`: AGP 8.10.1, Kotlin 2.1.21 with
its Compose compiler plugin, Compose BOM 2025.05.01; Gradle 8.14.3, JDK 17).
Application id `org.mtrx.app`, minSdk 26, target and compile SDK 36. Written
without an Android SDK at hand: it has been built only by CI.

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
  then the unit tests.

Remaining:

- The generated Kotlin does not compile yet, as far as can be seen without
  a compiler: UniFFI declares error fields named `message` (`MtrxError`'s
  `InvalidInput`, `SecretStorage`, `Other`; `SecretError::Failed`) as
  properties next to its own `override val message`. Rename those fields in
  `crates/mtrx-ffi` (or make the errors flat), then adjust
  `model/Errors.kt`, `Secrets.kt` and the tests that construct them.
- Each CI run signs with a new debug key, so an update needs the previous
  build uninstalled; a shared debug keystore (in the repository or a CI
  secret) fixes that.
- Not yet run on a device or emulator. Not done: notifications, sending
  images, opening files, reactions and replies, release signing.

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
      Not run yet.
- [x] Tests: conversions and the runtime (unit); sign-in, room list,
      timeline, image, send and sign-out through the bindings against
      Synapse (`cargo test -p mtrx-ffi --test integration -- --ignored`,
      in the Linux workflow).

## Later

- Push notifications (APNs, FCM; a push gateway, notification service
  extension / decryption in the background).
- Replies, reactions, edits, read receipts, typing, sending images.
- Several accounts; Face ID / biometric lock; verification and recovery.
- Timeline diffs instead of snapshots; log to OSLog and logcat.
- Signing and store distribution (TestFlight, Play internal testing).
