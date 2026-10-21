import MtrxFFI
import SwiftUI

/// The app: the library, and whether an account is signed in.
@MainActor
final class AppModel: ObservableObject {
    enum Phase {
        /// Restoring the saved session.
        case starting
        case signedOut(SignInModel)
        case signedIn(SessionModel)
        /// The saved session could not be opened.
        case failed(String)
    }

    @Published private(set) var phase = Phase.starting
    @Published private(set) var signingOut = false

    private let core: Mtrx
    private let browser: BrowserSignIn
    /// The app is in the foreground (sync runs).
    private var foreground = false

    init(core: Mtrx, browser: BrowserSignIn) {
        self.core = core
        self.browser = browser
    }

    /// The library with the app's directories and the Keychain.
    static func live() -> AppModel {
        initLogging(filter: "info,matrix_sdk=warn")
        let files = FileManager.default
        var data = files.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let cache = files.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        try? files.createDirectory(at: data, withIntermediateDirectories: true)
        // The store is encrypted with a passphrase that stays on this device
        // (in the Keychain): a backup restored elsewhere could not open it.
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? data.setResourceValues(values)

        let secrets = KeychainSecrets()
        let defaults = UserDefaults.standard
        if !defaults.bool(forKey: "installed") {
            // A new install: secrets left by an earlier one belong to data
            // that was deleted with it.
            secrets.deleteAll()
            defaults.set(true, forKey: "installed")
        }
        let core = Mtrx(dataDir: data.path, cacheDir: cache.path, secrets: secrets)
        return AppModel(core: core, browser: WebAuthenticator())
    }

    /// Open the saved session, or ask to sign in.
    func start() async {
        switch phase {
        case .starting, .failed: break
        case .signedIn, .signedOut: return
        }
        phase = .starting
        do {
            if let session = try await core.restore() {
                signedIn(session)
            } else {
                showSignIn()
            }
        } catch {
            phase = .failed(errorMessage(error) ?? "Could not open the account.")
        }
    }

    func scenePhaseChanged(_ scenePhase: ScenePhase) {
        switch scenePhase {
        case .active: foreground = true
        case .background: foreground = false
        default: return
        }
        if case .signedIn(let model) = phase {
            model.setSyncing(foreground)
        }
    }

    func signOut() async {
        guard case .signedIn(let model) = phase, !signingOut else { return }
        signingOut = true
        defer { signingOut = false }
        do {
            try await model.logout()
            showSignIn()
        } catch {
            model.error = errorMessage(error)
        }
    }

    /// The server no longer accepts the session: delete it, to sign in again.
    func forgetSession() async {
        guard case .signedIn(let model) = phase else { return }
        do {
            try await model.forget()
            showSignIn()
        } catch {
            showSignIn(error: errorMessage(error))
        }
    }

    private func signedIn(_ session: Session) {
        let model = SessionModel(session: session)
        model.start()
        model.setSyncing(foreground)
        phase = .signedIn(model)
    }

    private func showSignIn(error: String? = nil) {
        phase = .signedOut(SignInModel(core: core, browser: browser, error: error) { [weak self] session in
            self?.signedIn(session)
        })
    }
}
