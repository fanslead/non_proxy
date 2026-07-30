import Network

enum DirectConnectionFactory {
    static func makeTCP(
        endpoint: NWEndpoint,
        interface: NWInterface
    ) -> NWConnection {
        NWConnection(
            to: endpoint,
            using: parameters(base: .tcp, interface: interface)
        )
    }

    static func makeUDP(
        endpoint: NWEndpoint,
        interface: NWInterface
    ) -> NWConnection {
        NWConnection(
            to: endpoint,
            using: parameters(base: .udp, interface: interface)
        )
    }

    private static func parameters(
        base: NWParameters,
        interface: NWInterface
    ) -> NWParameters {
        base.requiredInterface = interface
        base.prohibitedInterfaceTypes = [.other, .loopback]
        return base
    }
}
