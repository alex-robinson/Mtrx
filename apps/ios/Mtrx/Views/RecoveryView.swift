import MtrxFFI
import SwiftUI

/// Entering the recovery key, and the recovery's progress. Back to
/// Settings when it succeeds.
struct RecoveryView: View {
    @ObservedObject var model: RecoveryModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Form {
            Section {
                TextField("Recovery key", text: $model.key, axis: .vertical)
                    .lineLimit(3...6)
                    .font(.body.monospaced())
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .disabled(model.recovering)
            } footer: {
                if let error = model.keyError {
                    Text(error)
                        .foregroundColor(.red)
                } else {
                    Text("The key you saved when you set up recovery, e.g. on the desktop app. Spaces are fine.")
                }
            }
            Section {
                Button {
                    model.recover()
                } label: {
                    Text("Recover")
                        .frame(maxWidth: .infinity)
                }
                .disabled(model.recovering || model.key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            if model.recovering {
                Section {
                    ForEach(RecoveryStep.Stage.allCases, id: \.self) { stage in
                        StepRow(stage: stage, current: model.step)
                    }
                } footer: {
                    Text("This can take a minute with many rooms.")
                }
            }
            if let error = model.error {
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Could not recover.")
                            .font(.headline)
                        Text(error)
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    .foregroundColor(.red)
                }
            }
        }
        .navigationTitle("Recovery key")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { model.reset() }
        .onChange(of: model.recovered) { recovered in
            if recovered { dismiss() }
        }
    }
}

/// One step of the recovery: done, in progress or to come.
private struct StepRow: View {
    let stage: RecoveryStep.Stage
    let current: RecoveryStep?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                icon.frame(width: 20)
                Text(title)
                    .foregroundColor(status == .pending ? .secondary : .primary)
            }
            if case .downloadingKeys(let done, let total) = current, stage == .downloadingKeys, total > 0 {
                ProgressView(value: Double(done), total: Double(total))
                    .padding(.leading, 30)
            }
        }
    }

    private enum Status { case done, running, pending }

    private var status: Status {
        guard let current else { return stage == .checkingKey ? .running : .pending }
        if stage < current.stage { return .done }
        return stage == current.stage ? .running : .pending
    }

    private var title: String {
        if case .downloadingKeys(let done, let total) = current, stage == .downloadingKeys, total > 0 {
            return "Fetching keys: \(done) of \(total) rooms"
        }
        return stage.title
    }

    @ViewBuilder
    private var icon: some View {
        switch status {
        case .done:
            Image(systemName: "checkmark.circle.fill").foregroundColor(.green)
        case .running:
            ProgressView()
        case .pending:
            Image(systemName: "circle").foregroundStyle(.secondary)
        }
    }
}
