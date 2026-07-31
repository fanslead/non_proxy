import Foundation

public enum AdapterHostRuntimeReadiness {
  public static func inspect(
    paths: MacSharedRuntimePaths,
    expectedFingerprint: String,
    fileManager: FileManager = .default
  ) throws {
    do {
      let owner = try PrivateRuntimeReadiness.requireDirectory(
        paths.adapterHostStateDirectory,
        fileManager: fileManager
      )
      try PrivateRuntimeReadiness.requireSocket(
        paths.adapterHostSocket,
        owner: owner,
        fileManager: fileManager
      )
      try PrivateRuntimeReadiness.inspectCapability(
        paths.adapterHostCapability,
        owner: owner,
        fileManager: fileManager
      )
      try PrivateRuntimeReadiness.inspectRuntimeIdentity(
        paths.adapterHostRuntimeIdentity,
        expectedFingerprint: expectedFingerprint,
        owner: owner,
        fileManager: fileManager
      )
    } catch {
      throw map(error)
    }
  }

  private static func map(
    _ error: Error
  ) -> AdapterHostRuntimeReadinessError {
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

public enum AdapterHostRuntimeReadinessError: Error, Equatable, Sendable {
  case invalidStateDirectory
  case invalidSocket
  case invalidCapability
  case invalidRuntimeIdentity
  case fingerprintMismatch
}
