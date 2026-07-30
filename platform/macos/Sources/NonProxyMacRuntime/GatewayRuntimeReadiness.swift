import Darwin
import Foundation

public enum GatewayRuntimeReadiness {
    private static let capabilityByteCount: UInt64 = 32
    private static let maximumIdentityByteCount: UInt64 = 1_024

    public static func inspect(
        paths: MacSharedRuntimePaths,
        expectedFingerprint: String,
        fileManager: FileManager = .default
    ) throws {
        let owner = try requireDirectory(
            paths.stateDirectory,
            fileManager: fileManager
        )
        try requireSocket(
            paths.controlSocket,
            owner: owner,
            fileManager: fileManager
        )
        try requireSocket(
            paths.flowSocket,
            owner: owner,
            fileManager: fileManager
        )
        try inspectCapability(
            paths.controlCapability,
            owner: owner,
            fileManager: fileManager
        )
        try inspectCapability(
            paths.providerCapability,
            owner: owner,
            fileManager: fileManager
        )
        try inspectRuntimeIdentity(
            paths.runtimeIdentity,
            expectedFingerprint: expectedFingerprint,
            owner: owner,
            fileManager: fileManager
        )
    }

    private static func requireDirectory(
        _ url: URL,
        fileManager: FileManager
    ) throws -> NSNumber {
        do {
            let values = try url.resourceValues(
                forKeys: [.isSymbolicLinkKey]
            )
            let attributes = try fileManager.attributesOfItem(
                atPath: url.path
            )
            guard values.isSymbolicLink != true,
                  attributes[.type] as? FileAttributeType == .typeDirectory,
                  let permissions =
                    attributes[.posixPermissions] as? NSNumber,
                  permissions.uint16Value & 0o077 == 0,
                  let owner = attributes[.ownerAccountID] as? NSNumber,
                  owner.uint32Value == geteuid()
            else {
                throw GatewayRuntimeReadinessError.invalidStateDirectory
            }
            return owner
        } catch {
            throw GatewayRuntimeReadinessError.invalidStateDirectory
        }
    }

    private static func requireSocket(
        _ url: URL,
        owner: NSNumber,
        fileManager: FileManager
    ) throws {
        do {
            let values = try url.resourceValues(
                forKeys: [.isSymbolicLinkKey]
            )
            let attributes = try fileManager.attributesOfItem(
                atPath: url.path
            )
            guard values.isSymbolicLink != true,
                  attributes[.type] as? FileAttributeType == .typeSocket,
                  let permissions =
                    attributes[.posixPermissions] as? NSNumber,
                  permissions.uint16Value & 0o077 == 0,
                  attributes[.ownerAccountID] as? NSNumber == owner
            else {
                throw GatewayRuntimeReadinessError.invalidSocket
            }
        } catch {
            throw GatewayRuntimeReadinessError.invalidSocket
        }
    }

    static func inspectCapability(
        _ url: URL,
        owner: NSNumber? = nil,
        fileManager: FileManager
    ) throws {
        do {
            let attributes = try fileManager.attributesOfItem(
                atPath: url.path
            )
            guard attributes[.type] as? FileAttributeType == .typeRegular,
                  let size = attributes[.size] as? NSNumber,
                  size.uint64Value == capabilityByteCount,
                  let permissions =
                    attributes[.posixPermissions] as? NSNumber,
                  permissions.uint16Value & 0o077 == 0,
                  owner == nil
                    || attributes[.ownerAccountID] as? NSNumber == owner
            else {
                throw GatewayRuntimeReadinessError.invalidCapability
            }
            let values = try url.resourceValues(
                forKeys: [.isSymbolicLinkKey]
            )
            guard values.isSymbolicLink != true else {
                throw GatewayRuntimeReadinessError.invalidCapability
            }
        } catch {
            throw GatewayRuntimeReadinessError.invalidCapability
        }
    }

    static func inspectRuntimeIdentity(
        _ url: URL,
        expectedFingerprint: String,
        owner: NSNumber? = nil,
        fileManager: FileManager,
        processIsAlive: (Int32) -> Bool = defaultProcessIsAlive
    ) throws {
        let attributes: [FileAttributeKey: Any]
        do {
            let values = try url.resourceValues(
                forKeys: [.isSymbolicLinkKey]
            )
            attributes = try fileManager.attributesOfItem(
                atPath: url.path
            )
            guard values.isSymbolicLink != true,
                  attributes[.type] as? FileAttributeType == .typeRegular,
                  let size = attributes[.size] as? NSNumber,
                  size.uint64Value > 0,
                  size.uint64Value <= maximumIdentityByteCount,
                  let permissions =
                    attributes[.posixPermissions] as? NSNumber,
                  permissions.uint16Value & 0o777 == 0o600,
                  owner == nil
                    || attributes[.ownerAccountID] as? NSNumber == owner
            else {
                throw GatewayRuntimeReadinessError.invalidRuntimeIdentity
            }
        } catch let error as GatewayRuntimeReadinessError {
            throw error
        } catch {
            throw GatewayRuntimeReadinessError.invalidRuntimeIdentity
        }

        let identity: GatewayRuntimeIdentity
        do {
            let data = try Data(
                contentsOf: url,
                options: [.mappedIfSafe]
            )
            identity = try JSONDecoder().decode(
                GatewayRuntimeIdentity.self,
                from: data
            )
        } catch {
            throw GatewayRuntimeReadinessError.invalidRuntimeIdentity
        }
        guard identity.schemaVersion == 1,
              !identity.semanticVersion.isEmpty,
              !identity.buildId.isEmpty,
              identity.processId > 0,
              identity.processId <= UInt32(Int32.max)
        else {
            throw GatewayRuntimeReadinessError.invalidRuntimeIdentity
        }
        guard identity.bundleFingerprint == expectedFingerprint else {
            throw GatewayRuntimeReadinessError.fingerprintMismatch
        }
        guard processIsAlive(Int32(identity.processId)) else {
            throw GatewayRuntimeReadinessError.invalidRuntimeIdentity
        }
    }

    private static func defaultProcessIsAlive(_ processID: Int32) -> Bool {
        kill(processID, 0) == 0 || errno == EPERM
    }
}

public enum GatewayRuntimeReadinessError: Error, Equatable, Sendable {
    case invalidStateDirectory
    case invalidSocket
    case invalidCapability
    case invalidRuntimeIdentity
    case fingerprintMismatch
}

private struct GatewayRuntimeIdentity: Decodable {
    let schemaVersion: UInt32
    let bundleFingerprint: String
    let processId: UInt32
    let semanticVersion: String
    let buildId: String
}
