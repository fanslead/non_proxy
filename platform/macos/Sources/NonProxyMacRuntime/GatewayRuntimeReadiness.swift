import Darwin
import Foundation

public enum GatewayRuntimeReadiness {
    private static let capabilityByteCount: UInt64 = 32

    public static func inspect(
        paths: MacSharedRuntimePaths,
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
}

public enum GatewayRuntimeReadinessError: Error, Equatable, Sendable {
    case invalidStateDirectory
    case invalidSocket
    case invalidCapability
}
