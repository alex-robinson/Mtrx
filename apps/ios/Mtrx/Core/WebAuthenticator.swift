import AuthenticationServices
import UIKit

/// Shows a server's sign-in page and returns the URL the server redirects
/// to when done.
@MainActor
protocol BrowserSignIn: AnyObject {
    func signIn(at url: URL, callbackScheme: String) async throws -> URL
}

/// Browser sign-in with `ASWebAuthenticationSession`, which shares Safari's
/// cookies (an account already signed in on the server's page is offered)
/// and receives the redirect to the app's scheme without a URL type.
@MainActor
final class WebAuthenticator: NSObject, BrowserSignIn {
    private var session: ASWebAuthenticationSession?

    func signIn(at url: URL, callbackScheme: String) async throws -> URL {
        let result = Completion()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                result.continuation = continuation
                let session = ASWebAuthenticationSession(url: url, callbackURLScheme: callbackScheme) { url, error in
                    if let url {
                        result.resume(.success(url))
                    } else {
                        result.resume(.failure(error ?? CancellationError()))
                    }
                }
                session.presentationContextProvider = self
                self.session = session
                if !session.start() {
                    result.resume(.failure(SignInError.browserUnavailable))
                }
            }
        } onCancel: {
            Task { @MainActor in self.session?.cancel() }
        }
    }

    enum SignInError: LocalizedError {
        case browserUnavailable

        var errorDescription: String? {
            "Could not open the sign-in page."
        }
    }

    /// Resumes the continuation once, whichever of the session's completion
    /// and a failed start comes first.
    private final class Completion: @unchecked Sendable {
        var continuation: CheckedContinuation<URL, Error>?
        private let lock = NSLock()

        func resume(_ result: Result<URL, Error>) {
            lock.lock()
            let continuation = self.continuation
            self.continuation = nil
            lock.unlock()
            continuation?.resume(with: result)
        }
    }
}

extension WebAuthenticator: ASWebAuthenticationPresentationContextProviding {
    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            let windows = scenes.flatMap(\.windows)
            return windows.first(where: \.isKeyWindow) ?? windows.first ?? ASPresentationAnchor()
        }
    }
}
