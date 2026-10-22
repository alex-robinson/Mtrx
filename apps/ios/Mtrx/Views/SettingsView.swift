import MtrxFFI
import SwiftUI

/// The account, its encryption on this device, and signing out.
struct SettingsView: View {
    @EnvironmentObject private var app: AppModel
    @EnvironmentObject private var session: SessionModel
    @Environment(\.dismiss) private var dismiss
    @State private var confirmSignOut = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Account") {
                    LabeledContent("User", value: session.userId)
                    LabeledContent("Server", value: session.homeserver)
                    LabeledContent("Device", value: session.deviceId)
                }
                .textSelection(.enabled)
                EncryptionSection()
                Section {
                    Button("Sign out…", role: .destructive) {
                        confirmSignOut = true
                    }
                    .disabled(app.signingOut)
                }
                Section {
                    LabeledContent("Version", value: version)
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .overlay {
                if app.signingOut {
                    ProgressView("Signing out…")
                        .padding(24)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                }
            }
            .confirmationDialog("Sign out of \(session.userId)?", isPresented: $confirmSignOut, titleVisibility: .visible) {
                Button("Sign out", role: .destructive) {
                    Task { await app.signOut() }
                }
            } message: {
                Text("The account's messages and keys on this device are deleted.")
            }
        }
    }

    private var version: String {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String ?? "?"
        let build = info?["CFBundleVersion"] as? String ?? "?"
        return "\(short) (\(build))"
    }
}

/// Whether this device is verified and recovery unlocked; while not, the
/// ways to fix it: the recovery key, or another device.
struct EncryptionSection: View {
    @EnvironmentObject private var session: SessionModel

    var body: some View {
        Section {
            LabeledContent("This device", value: deviceStatus)
            LabeledContent("Recovery", value: recoveryStatus)
            if session.needsEncryptionSetup, let encryption = session.encryption {
                if encryption.recovery != .disabled {
                    NavigationLink("Enter recovery key") {
                        RecoveryView(model: session.recovery)
                    }
                }
                if !encryption.deviceVerified {
                    if session.hasOtherDevice == false {
                        Text("Verifying needs another device signed in to this account, such as the desktop app.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    } else {
                        Button {
                            session.verifyWithAnotherDevice()
                        } label: {
                            HStack {
                                Text("Verify with another device")
                                Spacer()
                                if session.requestingVerification {
                                    ProgressView()
                                }
                            }
                        }
                        .disabled(session.requestingVerification)
                    }
                }
            }
        } header: {
            Text("Encryption")
        } footer: {
            if let error = session.verificationError {
                Text(error).foregroundColor(.red)
            }
        }
        .task(id: session.encryption?.deviceVerified) {
            if session.encryption?.deviceVerified == false {
                await session.loadDevices()
            }
        }
    }

    private var deviceStatus: String {
        guard let encryption = session.encryption else { return "not known yet" }
        return encryption.deviceVerified ? "verified" : "not verified"
    }

    private var recoveryStatus: String {
        switch session.encryption?.recovery {
        case nil, .unknown: return "not known yet"
        case .enabled: return "set up"
        case .disabled: return "not set up"
        case .incomplete: return "not unlocked on this device"
        }
    }
}
