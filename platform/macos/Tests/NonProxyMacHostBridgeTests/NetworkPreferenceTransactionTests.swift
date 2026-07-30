import Testing
@testable import NonProxyMacHostBridge

@MainActor
struct NetworkPreferenceTransactionTests {
    @Test
    func successfulEnableDoesNotRollback() async throws {
        var events: [String] = []

        try await NetworkPreferenceTransaction.enable(
            applyTransparent: { events.append("apply-transparent") },
            applyDNS: { events.append("apply-dns") },
            restoreDNS: { events.append("restore-dns") },
            restoreTransparent: { events.append("restore-transparent") }
        )

        #expect(events == ["apply-transparent", "apply-dns"])
    }

    @Test
    func dnsFailureRestoresBothPreferencesInReverseOrder() async {
        var events: [String] = []

        await #expect(throws: TransactionTestError.applyFailed) {
            try await NetworkPreferenceTransaction.enable(
                applyTransparent: {
                    events.append("apply-transparent")
                },
                applyDNS: {
                    events.append("apply-dns")
                    throw TransactionTestError.applyFailed
                },
                restoreDNS: { events.append("restore-dns") },
                restoreTransparent: {
                    events.append("restore-transparent")
                }
            )
        }

        #expect(events == [
            "apply-transparent",
            "apply-dns",
            "restore-dns",
            "restore-transparent",
        ])
    }

    @Test
    func transparentFailureDoesNotTouchDnsBackup() async {
        var events: [String] = []

        await #expect(throws: TransactionTestError.applyFailed) {
            try await NetworkPreferenceTransaction.enable(
                applyTransparent: {
                    events.append("apply-transparent")
                    throw TransactionTestError.applyFailed
                },
                applyDNS: { events.append("apply-dns") },
                restoreDNS: { events.append("restore-dns") },
                restoreTransparent: {
                    events.append("restore-transparent")
                }
            )
        }

        #expect(events == [
            "apply-transparent",
            "restore-transparent",
        ])
    }

    @Test
    func rollbackFailureReturnsStableProductError() async {
        await #expect {
            try await NetworkPreferenceTransaction.enable(
                applyTransparent: {},
                applyDNS: { throw TransactionTestError.applyFailed },
                restoreDNS: {
                    throw TransactionTestError.restoreFailed
                },
                restoreTransparent: {}
            )
        } throws: { error in
            guard let bridgeError = error as? BridgeError else {
                return false
            }
            return bridgeError.code == "NP_MAC_PREFERENCE_ROLLBACK_FAILED"
                && bridgeError.message.contains("DNS 配置恢复失败")
        }
    }
}

private enum TransactionTestError: Error {
    case applyFailed
    case restoreFailed
}
