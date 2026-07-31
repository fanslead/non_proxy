import AppKit
import Foundation
import Security
import UniformTypeIdentifiers

private let maximumApplicationCandidates = 2_048
private let maximumApplicationResults = 512
private let maximumApplicationDisplayNameBytes = 200

@MainActor
struct ApplicationCatalogController {
    func list() async -> [ApplicationDescriptor] {
        let runningURLs = NSWorkspace.shared.runningApplications.compactMap(
            \.bundleURL
        )
        let runningPaths = Set(runningURLs.map(Self.normalizedPath))
        let candidates = runningURLs + installedApplicationURLs()
        return await Task.detached(priority: .userInitiated) {
            Self.describeApplications(
                candidates: candidates,
                runningPaths: runningPaths
            )
        }.value
    }

    nonisolated private static func describeApplications(
        candidates: [URL],
        runningPaths: Set<String>
    ) -> [ApplicationDescriptor] {
        var applicationsByIdentity: [String: ApplicationDescriptor] = [:]

        for url in candidates.prefix(maximumApplicationCandidates) {
            guard
                let application = Self.describe(
                    url: url,
                    isRunning: runningPaths.contains(Self.normalizedPath(url))
                )
            else {
                continue
            }
            if let existing = applicationsByIdentity[application.stableIdentity],
                existing.isRunning && !application.isRunning
            {
                continue
            }
            applicationsByIdentity[application.stableIdentity] = application
            if applicationsByIdentity.count >= maximumApplicationResults {
                break
            }
        }

        return applicationsByIdentity.values.sorted {
            if $0.isRunning != $1.isRunning {
                return $0.isRunning
            }
            return $0.displayName.localizedCaseInsensitiveCompare(
                $1.displayName
            ) == .orderedAscending
        }
    }

    func choose() async throws -> ApplicationDescriptor? {
        let panel = NSOpenPanel()
        panel.title = "选择要直连的应用"
        panel.message = "NonProxy 只读取应用身份和代码签名，不读取应用数据。"
        panel.prompt = "选择应用"
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.allowedContentTypes = [.applicationBundle]
        panel.directoryURL = URL(fileURLWithPath: "/Applications")

        guard await panel.begin() == .OK,
            let url = panel.url
        else {
            return nil
        }
        guard let application = Self.describe(url: url, isRunning: false) else {
            throw BridgeError(
                code: "NP_MAC_APPLICATION_IDENTITY_UNAVAILABLE",
                message: "无法读取所选应用的稳定身份，请确认它是有效的 macOS 应用。"
            )
        }
        return application
    }

    private func installedApplicationURLs() -> [URL] {
        let fileManager = FileManager.default
        let roots = [
            URL(fileURLWithPath: "/Applications", isDirectory: true),
            URL(fileURLWithPath: "/System/Applications", isDirectory: true),
            URL(
                fileURLWithPath: "/System/Applications/Utilities",
                isDirectory: true
            ),
            fileManager.homeDirectoryForCurrentUser
                .appending(path: "Applications", directoryHint: .isDirectory),
        ]
        return roots.flatMap { root in
            (try? fileManager.contentsOfDirectory(
                at: root,
                includingPropertiesForKeys: [.isDirectoryKey],
                options: [.skipsHiddenFiles]
            ))?.filter { $0.pathExtension.caseInsensitiveCompare("app") == .orderedSame }
                ?? []
        }
    }

    nonisolated private static func describe(
        url: URL,
        isRunning: Bool
    ) -> ApplicationDescriptor? {
        guard url.pathExtension.caseInsensitiveCompare("app") == .orderedSame,
            let bundle = Bundle(url: url)
        else {
            return nil
        }
        let bundleIdentifier = normalizedIdentity(bundle.bundleIdentifier)
        guard bundleIdentifier != "com.nonproxy.desktop" else {
            return nil
        }
        guard let signature = signingIdentity(url: url) else {
            return nil
        }
        guard let stableIdentity = normalizedIdentity(signature.identifier) else {
            return nil
        }
        let displayName =
            [
                bundle.object(
                    forInfoDictionaryKey: "CFBundleDisplayName"
                ) as? String,
                bundle.object(forInfoDictionaryKey: "CFBundleName") as? String,
                url.deletingPathExtension().lastPathComponent,
            ]
            .compactMap(normalizedDisplayName)
            .first ?? stableIdentity

        return ApplicationDescriptor(
            displayName: displayName,
            stableIdentity: stableIdentity,
            signerIdentity: normalizedIdentity(signature.teamIdentifier),
            bundleIdentifier: bundleIdentifier,
            isRunning: isRunning
        )
    }

    nonisolated private static func signingIdentity(
        url: URL
    ) -> (identifier: String?, teamIdentifier: String?)? {
        var staticCode: SecStaticCode?
        let createStatus = SecStaticCodeCreateWithPath(
            url as CFURL,
            SecCSFlags(rawValue: 0),
            &staticCode
        )
        guard createStatus == errSecSuccess, let staticCode else {
            return nil
        }
        let validityStatus = SecStaticCodeCheckValidity(
            staticCode,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            nil
        )
        guard validityStatus == errSecSuccess else {
            return nil
        }

        var information: CFDictionary?
        let copyStatus = SecCodeCopySigningInformation(
            staticCode,
            SecCSFlags(rawValue: kSecCSSigningInformation),
            &information
        )
        guard copyStatus == errSecSuccess,
            let values = information as NSDictionary?
        else {
            return nil
        }
        return (
            values[kSecCodeInfoIdentifier] as? String,
            values[kSecCodeInfoTeamIdentifier] as? String
        )
    }

    nonisolated private static func normalizedIdentity(
        _ value: String?
    ) -> String? {
        guard let value,
            !value.isEmpty,
            value == value.trimmingCharacters(in: .whitespacesAndNewlines),
            value.utf8.count <= 255,
            value.unicodeScalars.allSatisfy({
                !CharacterSet.controlCharacters.contains($0)
            })
        else {
            return nil
        }
        return value
    }

    nonisolated private static func normalizedDisplayName(
        _ value: String?
    ) -> String? {
        guard let value else {
            return nil
        }
        let normalized = value.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        guard !normalized.isEmpty,
            normalized.utf8.count <= maximumApplicationDisplayNameBytes,
            normalized.unicodeScalars.allSatisfy({
                !CharacterSet.controlCharacters.contains($0)
            })
        else {
            return nil
        }
        return normalized
    }

    nonisolated private static func normalizedPath(_ url: URL) -> String {
        url.standardizedFileURL.resolvingSymlinksInPath().path
    }
}
