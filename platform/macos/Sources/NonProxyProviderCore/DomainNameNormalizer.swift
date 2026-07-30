import Foundation
import Network

public enum DomainNameNormalizer {
    public static func normalize(_ value: String?) -> String? {
        guard var candidate = value, !candidate.isEmpty else {
            return nil
        }
        if candidate.hasSuffix(".") {
            candidate.removeLast()
        }
        guard !candidate.isEmpty,
              candidate == candidate.trimmingCharacters(in: .whitespacesAndNewlines),
              !candidate.contains(".."),
              IPv4Address(candidate) == nil,
              IPv6Address(candidate) == nil
        else {
            return nil
        }

        var components = URLComponents()
        components.scheme = "https"
        components.host = candidate
        guard let ascii = components.url?.host()?.lowercased(),
              ascii.utf8.count <= 253,
              ascii.utf8.allSatisfy({ $0 < 128 })
        else {
            return nil
        }
        let labels = ascii.split(separator: ".", omittingEmptySubsequences: false)
        guard labels.count >= 2,
              labels.allSatisfy({ isValidLabel($0) })
        else {
            return nil
        }
        return ascii
    }

    private static func isValidLabel(_ label: Substring) -> Bool {
        guard !label.isEmpty,
              label.utf8.count <= 63,
              label.first != "-",
              label.last != "-"
        else {
            return false
        }
        return label.utf8.allSatisfy {
            ($0 >= 97 && $0 <= 122)
                || ($0 >= 48 && $0 <= 57)
                || $0 == 45
        }
    }
}
