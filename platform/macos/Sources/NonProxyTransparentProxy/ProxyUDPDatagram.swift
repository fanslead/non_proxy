import Foundation
import Network

struct ProxyUDPOutgoingDatagram {
    let payload: Data
}

struct ProxyUDPIncomingDatagram {
    let data: Data
    let endpoint: NWEndpoint
    let acknowledgedBytes: Int
}
