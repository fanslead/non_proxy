import Testing
@testable import NonProxyMacHostBridge

struct MacSystemInstallationEligibilityTests {
    @Test
    func rejectsMissingProvisioningProfile() {
        do {
            try MacSystemInstallationEligibility.validate(
                profileExists: false,
                entitlements: [:]
            )
            Issue.record("缺少 Provisioning Profile 时不应允许系统安装")
        } catch let error as BridgeError {
            #expect(error.code == "NP_MAC_MISSING_ENTITLEMENT")
            #expect(error.message.contains("Provisioning Profile"))
        } catch {
            Issue.record("返回了非产品错误：\(error)")
        }
    }

    @Test
    func rejectsIncompleteRestrictedEntitlements() {
        do {
            try MacSystemInstallationEligibility.validate(
                profileExists: true,
                entitlements: [
                    "com.apple.developer.system-extension.install": true,
                    "com.apple.security.application-groups": [
                        "group.com.nonproxy.shared"
                    ],
                    "com.apple.developer.networking.networkextension": [
                        "app-proxy-provider-systemextension"
                    ],
                ]
            )
            Issue.record("缺少 DNS Proxy 权限时不应允许系统安装")
        } catch let error as BridgeError {
            #expect(error.code == "NP_MAC_MISSING_ENTITLEMENT")
            #expect(error.message.contains("权限"))
        } catch {
            Issue.record("返回了非产品错误：\(error)")
        }
    }

    @Test
    func acceptsCompleteRestrictedEntitlements() throws {
        try MacSystemInstallationEligibility.validate(
            profileExists: true,
            entitlements: [
                "com.apple.developer.system-extension.install": true,
                "com.apple.security.application-groups": [
                    "group.com.nonproxy.shared"
                ],
                "com.apple.developer.networking.networkextension": [
                    "app-proxy-provider-systemextension",
                    "dns-proxy-systemextension",
                ],
            ]
        )
    }
}
