@testable import NonProxyProviderCore
import XCTest

final class DomainNameNormalizerTests: XCTestCase {
    func testNormalizesCaseRootDotAndInternationalDomain() {
        XCTAssertEqual(
            DomainNameNormalizer.normalize("API.Example.COM."),
            "api.example.com"
        )
        XCTAssertEqual(
            DomainNameNormalizer.normalize("例子.测试"),
            "xn--fsqu00a.xn--0zwm56d"
        )
    }

    func testRejectsAddressWhitespaceAndInvalidLabels() {
        XCTAssertNil(DomainNameNormalizer.normalize("  example.com"))
        XCTAssertNil(DomainNameNormalizer.normalize("203.0.113.10"))
        XCTAssertNil(DomainNameNormalizer.normalize("-bad.example"))
        XCTAssertNil(DomainNameNormalizer.normalize("localhost"))
    }
}
