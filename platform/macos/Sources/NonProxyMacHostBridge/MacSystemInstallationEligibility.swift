import Foundation
import Security

struct MacSystemInstallationEligibility {
    private static let appGroupIdentifier = "group.com.nonproxy.shared"
    private static let requiredNetworkExtensions: Set<String> = [
        "app-proxy-provider-systemextension",
        "dns-proxy-systemextension",
    ]

    static func validateLive(
        bundleURL: URL = Bundle.main.bundleURL,
        fileManager: FileManager = .default
    ) throws {
        let profileURL = bundleURL
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("embedded.provisionprofile")
        let profileType = (try? fileManager.attributesOfItem(
            atPath: profileURL.path
        ))?[.type] as? FileAttributeType
        let profileExists = profileType == .typeRegular

        guard profileExists else {
            try validate(profileExists: false, entitlements: [:])
            return
        }

        try validate(
            profileExists: true,
            entitlements: try signedEntitlements(for: bundleURL)
        )
    }

    static func validate(
        profileExists: Bool,
        entitlements: [String: Any]
    ) throws {
        guard profileExists else {
            throw BridgeError(
                code: "NP_MAC_MISSING_ENTITLEMENT",
                message:
                    "当前应用没有完整系统组件所需的 Provisioning Profile；"
                    + "开发预览包只能验证界面，不能启用系统网络组件。"
            )
        }

        let appGroups = stringValues(
            entitlements["com.apple.security.application-groups"]
        )
        let networkExtensions = stringValues(
            entitlements[
                "com.apple.developer.networking.networkextension"
            ]
        )
        let canInstallSystemExtensions =
            entitlements["com.apple.developer.system-extension.install"]
            as? Bool == true

        guard canInstallSystemExtensions,
            appGroups.contains(appGroupIdentifier),
            requiredNetworkExtensions.isSubset(of: networkExtensions)
        else {
            throw BridgeError(
                code: "NP_MAC_MISSING_ENTITLEMENT",
                message:
                    "当前应用签名或 Provisioning Profile 缺少完整网关所需的"
                    + "系统扩展、网络扩展或 App Group 权限。"
            )
        }
    }

    private static func signedEntitlements(
        for bundleURL: URL
    ) throws -> [String: Any] {
        var staticCode: SecStaticCode?
        guard SecStaticCodeCreateWithPath(
            bundleURL as CFURL,
            SecCSFlags(rawValue: 0),
            &staticCode
        ) == errSecSuccess, let staticCode else {
            throw invalidSignatureError()
        }
        guard SecStaticCodeCheckValidity(
            staticCode,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            nil
        ) == errSecSuccess else {
            throw invalidSignatureError()
        }

        var information: CFDictionary?
        guard SecCodeCopySigningInformation(
            staticCode,
            SecCSFlags(rawValue: kSecCSSigningInformation),
            &information
        ) == errSecSuccess,
            let values = information as NSDictionary?,
            let entitlements = values[kSecCodeInfoEntitlementsDict]
                as? [String: Any]
        else {
            return [:]
        }
        return entitlements
    }

    private static func stringValues(_ value: Any?) -> Set<String> {
        if let values = value as? [String] {
            return Set(values)
        }
        guard let values = value as? [Any] else {
            return []
        }
        return Set(values.compactMap { $0 as? String })
    }

    private static func invalidSignatureError() -> BridgeError {
        BridgeError(
            code: "NP_MAC_MISSING_ENTITLEMENT",
            message:
                "无法验证当前应用的最终代码签名；"
                + "请使用包含完整 Provisioning Profile 的正式签名包。"
        )
    }
}
