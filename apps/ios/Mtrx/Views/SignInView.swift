import SwiftUI

/// The homeserver, then a password form or the browser button, whichever
/// the server offers.
struct SignInView: View {
    @ObservedObject var model: SignInModel
    @FocusState private var focus: Field?

    private enum Field {
        case server, user, password
    }

    var body: some View {
        ScrollView {
            VStack(spacing: 28) {
                header
                VStack(spacing: 14) {
                    switch model.step {
                    case .server, .checking:
                        serverForm
                    case .password:
                        serverSummary
                        passwordForm
                    case .browser:
                        serverSummary
                        browserForm
                    }
                    if let error = model.error {
                        Text(error)
                            .font(.footnote)
                            .foregroundColor(.red)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 48)
            .frame(maxWidth: 420)
            .frame(maxWidth: .infinity)
        }
        .scrollDismissesKeyboard(.interactively)
        .onAppear { focus = model.step == .server ? .server : nil }
    }

    private var header: some View {
        VStack(spacing: 10) {
            Image("Logo")
                .resizable()
                .frame(width: 96, height: 96)
                .accessibilityHidden(true)
            Text("Mtrx")
                .font(.largeTitle.weight(.semibold))
            Text("Sign in to your Matrix account")
                .foregroundStyle(.secondary)
        }
    }

    private var serverForm: some View {
        VStack(spacing: 14) {
            field {
                TextField("Homeserver, e.g. matrix.org", text: $model.homeserver)
                    .textContentType(.URL)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .submitLabel(.continue)
                    .focused($focus, equals: .server)
                    .onSubmit { model.checkServer() }
                    .disabled(model.step == .checking)
            }
            if model.step == .checking {
                HStack(spacing: 12) {
                    ProgressView()
                    Button("Cancel", role: .cancel) { model.cancel() }
                }
                .frame(maxWidth: .infinity, minHeight: 44)
            } else {
                Button {
                    model.checkServer()
                } label: {
                    Text("Continue").frame(maxWidth: .infinity, minHeight: 32)
                }
                .buttonStyle(.borderedProminent)
                .disabled(model.homeserver.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    /// The chosen server, with a way back.
    private var serverSummary: some View {
        HStack {
            Image(systemName: "server.rack")
                .foregroundStyle(.secondary)
            Text(model.homeserver.trimmingCharacters(in: .whitespacesAndNewlines))
                .lineLimit(1)
            Spacer()
            Button("Change") { model.changeServer() }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 10))
    }

    private var passwordForm: some View {
        VStack(spacing: 14) {
            field {
                TextField("Username", text: $model.user)
                    .textContentType(.username)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .submitLabel(.next)
                    .focused($focus, equals: .user)
                    .onSubmit { focus = .password }
            }
            field {
                SecureField("Password", text: $model.password)
                    .textContentType(.password)
                    .submitLabel(.go)
                    .focused($focus, equals: .password)
                    .onSubmit { model.signInWithPassword() }
            }
            .disabled(model.signingIn)
            if model.signingIn {
                HStack(spacing: 12) {
                    ProgressView()
                    Text("Signing in…").foregroundStyle(.secondary)
                    Button("Cancel", role: .cancel) { model.cancel() }
                }
                .frame(maxWidth: .infinity, minHeight: 44)
            } else {
                Button {
                    model.signInWithPassword()
                } label: {
                    Text("Sign in with password").frame(maxWidth: .infinity, minHeight: 32)
                }
                .buttonStyle(.borderedProminent)
                .disabled(model.user.isEmpty || model.password.isEmpty)
            }
        }
        .onAppear { focus = .user }
    }

    private var browserForm: some View {
        VStack(spacing: 10) {
            if model.signingIn {
                HStack(spacing: 12) {
                    ProgressView()
                    Text("Continue in your browser…").foregroundStyle(.secondary)
                    Button("Cancel", role: .cancel) { model.cancel() }
                }
                .frame(maxWidth: .infinity, minHeight: 44)
            } else {
                Button {
                    model.signInWithBrowser()
                } label: {
                    Text("Sign in with browser").frame(maxWidth: .infinity, minHeight: 32)
                }
                .buttonStyle(.borderedProminent)
            }
            Text("Your server's sign-in page opens here, and returns to Mtrx.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
    }

    private func field<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        content()
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 10))
    }
}
