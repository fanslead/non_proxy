import CoreLocation
import Foundation
import NonProxyMacNetworkIdentity

@MainActor
final class CurrentNetworkIdentityController: NSObject,
    CLLocationManagerDelegate
{
    private static let authorizationTimeout: Duration = .seconds(30)

    private let locationManager = CLLocationManager()
    private var authorizationContinuation:
        CheckedContinuation<CLAuthorizationStatus, Never>?

    override init() {
        super.init()
        locationManager.delegate = self
    }

    func capture() async -> CurrentNetworkPayload {
        var snapshot = await readNetworkSnapshot()
        var authorization = locationManager.authorizationStatus
        if Self.needsWiFiAuthorization(snapshot),
           authorization == .notDetermined
        {
            authorization = await requestAuthorization()
            if authorization == .authorizedAlways {
                snapshot = await readNetworkSnapshot()
            }
        }
        return CurrentNetworkPayload.result(
            snapshot: snapshot,
            permission: Self.permissionState(authorization)
        )
    }

    nonisolated func locationManagerDidChangeAuthorization(
        _ manager: CLLocationManager
    ) {
        let status = manager.authorizationStatus
        guard status != .notDetermined else {
            return
        }
        Task { @MainActor [weak self] in
            self?.finishAuthorization(status)
        }
    }

    private func readNetworkSnapshot() async -> MacNetworkEnvironmentSnapshot {
        let monitor = MacNetworkEnvironmentMonitor()
        await monitor.start()
        let snapshot = monitor.snapshot()
        monitor.stop()
        return snapshot
    }

    private func requestAuthorization() async -> CLAuthorizationStatus {
        await withCheckedContinuation { continuation in
            authorizationContinuation = continuation
            locationManager.requestWhenInUseAuthorization()
            Task { @MainActor [weak self] in
                try? await Task.sleep(for: Self.authorizationTimeout)
                guard let self else {
                    return
                }
                self.finishAuthorization(
                    self.locationManager.authorizationStatus
                )
            }
        }
    }

    private func finishAuthorization(_ status: CLAuthorizationStatus) {
        let continuation = authorizationContinuation
        authorizationContinuation = nil
        continuation?.resume(returning: status)
    }

    private static func needsWiFiAuthorization(
        _ snapshot: MacNetworkEnvironmentSnapshot
    ) -> Bool {
        snapshot.fingerprints.contains {
            $0.kind == .interfaceClass && $0.value == "wifi"
        } && !snapshot.fingerprints.contains {
            $0.kind == .wifiSSIDHash
        }
    }

    private static func permissionState(
        _ status: CLAuthorizationStatus
    ) -> NetworkLocationPermissionState {
        switch status {
        case .authorizedAlways:
            .authorized
        case .denied:
            .denied
        case .restricted:
            .restricted
        case .notDetermined:
            .notDetermined
        @unknown default:
            .unknown
        }
    }
}
