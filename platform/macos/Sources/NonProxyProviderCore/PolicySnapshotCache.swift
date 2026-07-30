import Foundation
import NonProxyProviderContracts
import SwiftProtobuf

public actor PolicySnapshotCache {
    private let directory: URL
    private let fileURL: URL

    public init(directory: URL, providerName: String) throws {
        guard !providerName.isEmpty,
              providerName.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "-" })
        else {
            throw ProviderError.invalidConfiguration("Provider 缓存名称无效")
        }
        self.directory = directory
        fileURL = directory.appendingPathComponent(
            "\(providerName)-snapshot.pb",
            isDirectory: false
        )
    }

    public func save(_ snapshot: VerifiedPolicySnapshot) throws {
        try rejectSymbolicLink(directory)
        do {
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try rejectSymbolicLink(fileURL)
            let bytes = try snapshot.wireSnapshot.serializedData()
            try bytes.write(to: fileURL, options: [.atomic])
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: fileURL.path
            )
        } catch let error as ProviderError {
            throw error
        } catch {
            throw ProviderError.snapshotCache("无法原子保存已验证策略快照")
        }
    }

    public func load() throws -> VerifiedPolicySnapshot? {
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            return nil
        }
        try rejectSymbolicLink(fileURL)
        do {
            let bytes = try Data(
                contentsOf: fileURL,
                options: [.mappedIfSafe, .uncached]
            )
            guard bytes.count <= SnapshotValidator.maximumPayloadBytes + 64 * 1024 else {
                throw ProviderError.snapshotCache("本地策略快照文件超过大小上限")
            }
            let snapshot = try Nonproxy_Policy_V1_CompiledPolicySnapshot(
                serializedBytes: bytes
            )
            return try SnapshotValidator.validate(snapshot)
        } catch let error as ProviderError {
            throw error
        } catch {
            throw ProviderError.snapshotCache("本地策略快照无法读取或解码")
        }
    }

    private func rejectSymbolicLink(_ url: URL) throws {
        guard FileManager.default.fileExists(atPath: url.path) else {
            return
        }
        let values = try url.resourceValues(forKeys: [.isSymbolicLinkKey])
        guard values.isSymbolicLink != true else {
            throw ProviderError.snapshotCache("策略快照路径不能是符号链接")
        }
    }
}
