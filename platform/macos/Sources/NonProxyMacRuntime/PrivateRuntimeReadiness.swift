import Darwin
import Foundation

enum PrivateRuntimeReadiness {
  private static let capabilityByteCount: UInt64 = 32
  private static let maximumIdentityByteCount: UInt64 = 1_024

  static func requireDirectory(
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
        throw PrivateRuntimeReadinessError.invalidStateDirectory
      }
      return owner
    } catch {
      throw PrivateRuntimeReadinessError.invalidStateDirectory
    }
  }

  static func requireSocket(
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
        throw PrivateRuntimeReadinessError.invalidSocket
      }
    } catch {
      throw PrivateRuntimeReadinessError.invalidSocket
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
        throw PrivateRuntimeReadinessError.invalidCapability
      }
      let values = try url.resourceValues(
        forKeys: [.isSymbolicLinkKey]
      )
      guard values.isSymbolicLink != true else {
        throw PrivateRuntimeReadinessError.invalidCapability
      }
    } catch {
      throw PrivateRuntimeReadinessError.invalidCapability
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
        throw PrivateRuntimeReadinessError.invalidRuntimeIdentity
      }
    } catch let error as PrivateRuntimeReadinessError {
      throw error
    } catch {
      throw PrivateRuntimeReadinessError.invalidRuntimeIdentity
    }

    let identity: PrivateRuntimeIdentity
    do {
      let data = try Data(
        contentsOf: url,
        options: [.mappedIfSafe]
      )
      identity = try JSONDecoder().decode(
        PrivateRuntimeIdentity.self,
        from: data
      )
    } catch {
      throw PrivateRuntimeReadinessError.invalidRuntimeIdentity
    }
    guard identity.schemaVersion == 1,
      !identity.semanticVersion.isEmpty,
      !identity.buildId.isEmpty,
      identity.processId > 0,
      identity.processId <= UInt32(Int32.max)
    else {
      throw PrivateRuntimeReadinessError.invalidRuntimeIdentity
    }
    guard identity.bundleFingerprint == expectedFingerprint else {
      throw PrivateRuntimeReadinessError.fingerprintMismatch
    }
    guard processIsAlive(Int32(identity.processId)) else {
      throw PrivateRuntimeReadinessError.invalidRuntimeIdentity
    }
  }

  private static func defaultProcessIsAlive(_ processID: Int32) -> Bool {
    kill(processID, 0) == 0 || errno == EPERM
  }
}

enum PrivateRuntimeReadinessError: Error, Equatable, Sendable {
  case invalidStateDirectory
  case invalidSocket
  case invalidCapability
  case invalidRuntimeIdentity
  case fingerprintMismatch
}

private struct PrivateRuntimeIdentity: Decodable {
  let schemaVersion: UInt32
  let bundleFingerprint: String
  let processId: UInt32
  let semanticVersion: String
  let buildId: String
}
