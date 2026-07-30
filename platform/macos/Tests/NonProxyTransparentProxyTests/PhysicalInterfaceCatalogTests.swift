import Network
@testable import NonProxyTransparentProxy
import XCTest

final class PhysicalInterfaceCatalogTests: XCTestCase {
    func testPrioritizesOnlyPhysicalOutboundInterfaces() {
        XCTAssertEqual(
            PhysicalInterfaceCatalog.priority(for: .wiredEthernet),
            0
        )
        XCTAssertEqual(PhysicalInterfaceCatalog.priority(for: .wifi), 1)
        XCTAssertEqual(PhysicalInterfaceCatalog.priority(for: .cellular), 2)
        XCTAssertNil(PhysicalInterfaceCatalog.priority(for: .other))
        XCTAssertNil(PhysicalInterfaceCatalog.priority(for: .loopback))
    }
}
