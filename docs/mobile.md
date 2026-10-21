# iOS and Android: the shared Rust layer

Status: the Rust layer (`crates/mtrx-ffi`) and its builds are done; the apps
are to do. This is the plan, and the contract the apps are built on.

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

## 4. Android app

- Build: `scripts/build-android-ffi.sh` fills the Gradle module
  `apps/android/mtrx-ffi`: `src/main/jniLibs/{arm64-v8a,x86_64}/libmtrx_ffi.so`
  and `src/main/kotlin/uniffi/mtrx/mtrx.kt`, next to the committed
  `org/mtrx/ffi/AndroidPlatform.kt`. CI's artifact `mtrx-ffi-android` holds
  the same. The module (an Android library) needs JNA
  (`net.java.dev.jna:jna:<5.x>@aar`), `kotlinx-coroutines-android`,
  `androidx.annotation`, and the platform verifier's Kotlin part (group
  `rustls`, artifact `rustls-platform-verifier`, version of
  `rustls-platform-verifier-android` in Cargo.lock, now 0.2.0; Maven
  repository and Gradle snippet in that crate's README).
- Start: `AndroidPlatform.initialize(context)` once (in
  `Application.onCreate`), before any other call: the TLS verifier calls
  Android's through the JVM. Then `Mtrx(filesDir, cacheDir, secrets)` with
  a `SecretBackend` that encrypts values with an AES-GCM key in the Android
  Keystore and stores them in app-private storage.
- Sync: start and stop with the process lifecycle (`ProcessLifecycleOwner`).
- Browser login: open `url()` in a Custom Tab; an activity with an intent
  filter for scheme `io.github.alex-robinson` (path `/oauth`) receives the
  redirect and passes its URL to `finish`.
- R8: keep `com.sun.jna.**`, `uniffi.mtrx.**` and
  `org.rustls.platformverifier.**`.

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
