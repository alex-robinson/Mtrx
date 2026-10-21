import Foundation
import MtrxFFI

/// Signing in: the homeserver first, then a password or the browser,
/// whichever the server offers.
@MainActor
final class SignInModel: ObservableObject {
    enum Step: Equatable {
        /// Entering the homeserver.
        case server
        /// Asking the server how to sign in.
        case checking
        case password
        case browser
    }

    @Published var homeserver = "matrix.org"
    @Published var user = ""
    @Published var password = ""
    @Published private(set) var step = Step.server
    /// A sign-in is in progress (with a password or in the browser).
    @Published private(set) var signingIn = false
    @Published private(set) var error: String?

    /// The scheme of `oauthRedirectUri()`, to which the browser returns.
    static let callbackScheme = String(oauthRedirectUri().prefix { $0 != ":" })

    private let core: Mtrx
    private let browser: BrowserSignIn
    private let signedIn: (Session) -> Void
    private var task: Task<Void, Never>?
    /// Counts attempts; one cancelled or replaced does not report back.
    private var attempt = 0

    init(core: Mtrx, browser: BrowserSignIn, error: String? = nil, signedIn: @escaping (Session) -> Void) {
        self.core = core
        self.browser = browser
        self.error = error
        self.signedIn = signedIn
    }

    private var server: String {
        homeserver.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Ask the server whether it offers browser sign-in.
    func checkServer() {
        let server = self.server
        guard step == .server, !server.isEmpty else { return }
        let attempt = begin()
        step = .checking
        task = Task {
            do {
                let oauth = try await core.supportsOauth(homeserver: server)
                guard attempt == self.attempt else { return }
                step = oauth ? .browser : .password
            } catch {
                guard attempt == self.attempt else { return }
                step = .server
                self.error = errorMessage(error)
            }
            finish(attempt)
        }
    }

    func signInWithPassword() {
        let (server, user, password) = (self.server, self.user.trimmingCharacters(in: .whitespaces), self.password)
        guard step == .password, !signingIn, !user.isEmpty, !password.isEmpty else { return }
        let attempt = begin()
        signingIn = true
        task = Task {
            do {
                let session = try await core.login(homeserver: server, user: user, password: password)
                // Signed in, even if cancelled meanwhile.
                self.password = ""
                signedIn(session)
            } catch {
                guard attempt == self.attempt else { return }
                self.error = errorMessage(error)
            }
            finish(attempt)
        }
    }

    /// Sign in on the server's page, in a browser sheet.
    func signInWithBrowser() {
        let server = self.server
        guard step == .browser, !signingIn else { return }
        let attempt = begin()
        signingIn = true
        task = Task {
            do {
                // Dropping `login` (on failure) cancels the sign-in.
                let login = try await core.startOauthLogin(homeserver: server)
                guard let url = URL(string: login.url()) else {
                    throw MtrxError.Other(message: "The server's sign-in page has an invalid address.")
                }
                let callback = try await browser.signIn(at: url, callbackScheme: Self.callbackScheme)
                let session = try await login.finish(redirectUrl: callback.absoluteString)
                signedIn(session)
            } catch {
                guard attempt == self.attempt else { return }
                self.error = errorMessage(error)
            }
            finish(attempt)
        }
    }

    /// Stop what is in progress: checking the server, or signing in.
    func cancel() {
        task?.cancel()
        task = nil
        attempt += 1
        signingIn = false
        if step == .checking {
            step = .server
        }
    }

    /// Back to entering the homeserver.
    func changeServer() {
        cancel()
        error = nil
        step = .server
    }

    private func begin() -> Int {
        task?.cancel()
        attempt += 1
        error = nil
        return attempt
    }

    private func finish(_ attempt: Int) {
        guard attempt == self.attempt else { return }
        task = nil
        signingIn = false
    }
}
