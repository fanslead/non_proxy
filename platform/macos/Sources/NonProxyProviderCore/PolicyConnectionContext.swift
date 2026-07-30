import Foundation
import NonProxyProviderContracts

public struct PolicyAppIdentity: Sendable {
    public let stableID: String
    public let signerID: String?
    public let parentStableID: String?
    public let helperGroupID: String?

    public init(
        stableID: String,
        signerID: String? = nil,
        parentStableID: String? = nil,
        helperGroupID: String? = nil
    ) {
        self.stableID = stableID
        self.signerID = signerID
        self.parentStableID = parentStableID
        self.helperGroupID = helperGroupID
    }

    public static let unknown = Self(stableID: "unknown-app")
}

public struct PolicyDestination: Sendable {
    public let normalizedDomain: String?
    public let registrableDomain: String?
    public let ipAddress: String?
    public let transport: Nonproxy_Common_V1_TransportProtocol
    public let port: UInt16

    public init(
        normalizedDomain: String?,
        registrableDomain: String?,
        ipAddress: String?,
        transport: Nonproxy_Common_V1_TransportProtocol,
        port: UInt16
    ) {
        self.normalizedDomain = normalizedDomain
        self.registrableDomain = registrableDomain
        self.ipAddress = ipAddress
        self.transport = transport
        self.port = port
    }
}

public struct PolicyConnectionContext: Sendable {
    public let app: PolicyAppIdentity
    public let destination: PolicyDestination
    public let networkProfileID: String?

    public init(
        app: PolicyAppIdentity,
        destination: PolicyDestination,
        networkProfileID: String? = nil
    ) {
        self.app = app
        self.destination = destination
        self.networkProfileID = networkProfileID
    }
}

public struct PolicyDecision: Sendable {
    public let result: Nonproxy_Policy_V1_DecisionSpec
    public let matchedPolicyID: String?
    public let snapshotVersion: UInt64
    public let reasonCode: String

    public init(
        result: Nonproxy_Policy_V1_DecisionSpec,
        matchedPolicyID: String?,
        snapshotVersion: UInt64,
        reasonCode: String
    ) {
        self.result = result
        self.matchedPolicyID = matchedPolicyID
        self.snapshotVersion = snapshotVersion
        self.reasonCode = reasonCode
    }
}
