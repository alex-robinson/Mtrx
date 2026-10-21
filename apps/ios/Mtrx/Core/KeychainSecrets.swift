import Foundation
import MtrxFFI
import Security

/// The library's secrets (access tokens, the store passphrase) as generic
/// passwords in the Keychain: service `org.mtrx.Mtrx`, account = the key.
/// Readable after the first unlock (so sync can resume in the background)
/// and never leaving this device.
final class KeychainSecrets: SecretBackend {
    let service: String

    init(service: String = "org.mtrx.Mtrx") {
        self.service = service
    }

    func load(key: String) throws -> String? {
        var query = item(key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess:
            guard let data = result as? Data, let value = String(data: data, encoding: .utf8) else {
                throw SecretError.Failed(reason: "The Keychain item \(key) is not text")
            }
            return value
        case errSecItemNotFound:
            return nil
        default:
            throw failure(status, "read", key)
        }
    }

    func save(key: String, value: String) throws {
        let attributes: [String: Any] = [
            kSecValueData as String: Data(value.utf8),
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        var status = SecItemUpdate(item(key) as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            status = SecItemAdd(item(key).merging(attributes) { $1 } as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw failure(status, "save", key) }
    }

    func delete(key: String) throws {
        let status = SecItemDelete(item(key) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw failure(status, "delete", key)
        }
    }

    /// Delete every secret of the app. Keychain items outlive the app on
    /// iOS, but its data does not: after a reinstall they belong to nothing.
    func deleteAll() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
        ]
        SecItemDelete(query as CFDictionary)
    }

    private func item(_ key: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
        ]
    }

    private func failure(_ status: OSStatus, _ action: String, _ key: String) -> SecretError {
        let reason = SecCopyErrorMessageString(status, nil) as String? ?? "error \(status)"
        return .Failed(reason: "Could not \(action) \(key) in the Keychain: \(reason)")
    }
}
