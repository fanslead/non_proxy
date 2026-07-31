import Darwin
import Foundation

public enum GatewayRuntimeReadiness {
    public static func inspect(
        paths: MacSharedRuntimePaths,
        expectedFingerprint: String,
        fileManager: FileManager = .default
    ) throws {
        do {
            let owner = try PrivateRuntimeReadiness.requireDirectory(
                paths.stateDirectory,
                fileManager: fileManager
            )
            try PrivateRuntimeReadiness.requireSocket(
                paths.controlSocket,
                owner: owner,
                fileManager: fileManager
            )
            try PrivateRuntimeReadiness.requireSocket(
                paths.flowSocket,
                owner: owner,
                fileManager: fileManager
            )
            try PrivateRuntimeReadiness.inspectCapability(
                paths.controlCapability,
                owner: owner,
                fileManager: fileManager
            )
            try PrivateRuntimeReadiness.inspectCapability(
                paths.providerCapability,
                owner: owner,
                fileManager: fileManager
            )
            try PrivateRuntimeReadiness.inspectRuntimeIdentity(
                paths.runtimeIdentity,
                expectedFingerprint: expectedFingerprint,
                owner: owner,
                fileManager: fileManager
            )
        } catch {
            throw map(error)
        }
    }

    static func inspectCapability(
        _ url: URL,
        owner: NSNumber? = nil,
        fileManager: FileManager
    ) throws {
        do {
            try PrivateRuntimeReadiness.inspectCapability(
                url,
                owner: owner,
                fileManager: fileManager
            )
        } catch {
            throw map(error)
        }
    }

    static func inspectRuntimeIdentity(
        _ url: URL,
        expectedFingerprint: String,
        owner: NSNumber? = nil,
        fileManager: FileManager,
        processIsAlive: (Int32) -> Bool = { processID in
            Darwin.kill(processID, 0) == 0 || errno == EPERM
        }
    ) throws {
        do {
            try PrivateRuntimeReadiness.inspectRuntimeIdentity(
                url,
                expectedFingerprint: expectedFingerprint,
                owner: owner,
                fileManager: fileManager,
                processIsAlive: processIsAlive
            )
        } catch {
            throw map(error)
        }
    }

    private static func map(_ error: Error) -> GatewayRuntimeReadinessError {
        switch error {
        case PrivateRuntimeReadinessError.invalidStateDirectory:
            .invalidStateDirectory
        case PrivateRuntimeReadinessError.invalidSocket:
            .invalidSocket
        case PrivateRuntimeReadinessError.invalidCapability:
            .invalidCapability
        case PrivateRuntimeReadinessError.fingerprintMismatch:
            .fingerprintMismatch
        default:
            .invalidRuntimeIdentity
        }
    }
}

public enum GatewayRuntimeReadinessError: Error, Equatable, Sendable {
    case invalidStateDirectory
    case invalidSocket
    case invalidCapability
    case invalidRuntimeIdentity
    case fingerprintMismatch
}
