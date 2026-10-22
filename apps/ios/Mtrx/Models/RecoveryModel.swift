import Foundation
import MtrxFFI

/// Unlocking encryption with the recovery key: this device becomes verified
/// and fetches the message keys from the backup. Runs in a task of its own,
/// so leaving the screen does not stop it.
@MainActor
final class RecoveryModel: ObservableObject {
    @Published var key = ""
    @Published private(set) var recovering = false
    /// The step in progress, while recovering.
    @Published private(set) var step: RecoveryStep?
    /// Why the last attempt's key was refused: not one, or not the
    /// account's. Shown under the field, which keeps the key for correcting.
    @Published private(set) var keyError: String?
    /// Why the last attempt failed otherwise ("Could not recover." is shown
    /// above it).
    @Published private(set) var error: String?
    /// The last attempt succeeded (the screen closes).
    @Published private(set) var recovered = false

    private let session: Session
    private let onRecovered: @MainActor () -> Void
    private var task: Task<Void, Never>?

    init(session: Session, onRecovered: @escaping @MainActor () -> Void) {
        self.session = session
        self.onRecovered = onRecovered
    }

    /// Clear what the last attempt left, unless one is running.
    func reset() {
        guard !recovering else { return }
        keyError = nil
        error = nil
        recovered = false
    }

    func recover() {
        let key = self.key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !recovering, !key.isEmpty else { return }
        recovering = true
        keyError = nil
        error = nil
        recovered = false
        step = nil
        let session = self.session
        task = Task {
            // The steps arrive during the call; all of them are applied
            // before the result.
            let steps = Feed<RecoveryStep>(keepingAll: true)
            let applied = Task { @MainActor [weak self] in
                for await step in steps.stream {
                    self?.step = step
                }
            }
            let result: Result<Void, Error>
            do {
                try await session.recover(recoveryKey: key, listener: steps)
                result = .success(())
            } catch {
                result = .failure(error)
            }
            steps.finish()
            await applied.value
            finish(result)
        }
    }

    private func finish(_ result: Result<Void, Error>) {
        task = nil
        recovering = false
        step = nil
        switch result {
        case .success:
            key = ""
            recovered = true
            onRecovered()
        case .failure(MtrxError.InvalidInput):
            keyError = "The recovery key is not valid."
        case .failure(let error):
            self.error = errorMessage(error)
        }
    }
}

extension RecoveryStep {
    /// The steps in their order, as shown.
    enum Stage: Int, CaseIterable, Comparable {
        case checkingKey, importingIdentity, downloadingKeys

        static func < (a: Stage, b: Stage) -> Bool { a.rawValue < b.rawValue }

        var title: String {
            switch self {
            case .checkingKey: return "Checking the key"
            case .importingIdentity: return "Verifying this device"
            case .downloadingKeys: return "Fetching keys"
            }
        }
    }

    var stage: Stage {
        switch self {
        case .checkingKey: return .checkingKey
        case .importingIdentity: return .importingIdentity
        case .downloadingKeys: return .downloadingKeys
        }
    }
}
