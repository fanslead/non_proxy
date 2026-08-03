import Foundation
import ServiceManagement

struct BackgroundAgentDescriptor: Sendable {
  let productName: String
  let errorPrefix: String
  let packagedDescription: String
  let readinessDescription: String

  static let gateway = BackgroundAgentDescriptor(
    productName: "gatewayd",
    errorPrefix: "NP_MAC_GATEWAY",
    packagedDescription: "gatewayd 后台项目",
    readinessDescription: "本地控制通道"
  )

  static let adapterHost = BackgroundAgentDescriptor(
    productName: "adapter-host",
    errorPrefix: "NP_MAC_ADAPTER_HOST",
    packagedDescription: "adapter-host 后台项目",
    readinessDescription: "本地适配器通道"
  )
}

@MainActor
struct BackgroundAgentController {
  private let descriptor: BackgroundAgentDescriptor
  private let service: any BackgroundAgentServicing
  private let installationValidator: () throws -> Void
  private let fingerprintProvider: () throws -> String
  private let runtimeInspector: (String) -> BackgroundRuntimeState
  private let readinessAttempts: Int
  private let readinessDelay: Duration

  init(
    descriptor: BackgroundAgentDescriptor,
    service: any BackgroundAgentServicing,
    installationValidator: @escaping () throws -> Void,
    fingerprintProvider: @escaping () throws -> String,
    runtimeInspector: @escaping (String) -> BackgroundRuntimeState,
    readinessAttempts: Int = 100,
    readinessDelay: Duration = .milliseconds(100)
  ) {
    self.descriptor = descriptor
    self.service = service
    self.installationValidator = installationValidator
    self.fingerprintProvider = fingerprintProvider
    self.runtimeInspector = runtimeInspector
    self.readinessAttempts = max(1, readinessAttempts)
    self.readinessDelay = readinessDelay
  }

  func query() throws -> BackgroundAgentSnapshot {
    try requireInstallationEligibility()
    let status = service.status
    let runtimeState = status == .enabled ? inspectRuntime() : .notReady
    return Self.snapshot(
      status: status,
      runtimeReady: runtimeState == .ready,
      requiresUpgrade: runtimeState == .requiresReplacement
    )
  }

  func registerAndWait(
    approvalHandler: @escaping () -> Void,
    prepareForReplacement: @escaping () async throws -> Void
  ) async throws -> BackgroundAgentRegistrationOutcome {
    try requireInstallationEligibility()
    let expectedFingerprint = try expectedFingerprint()
    let initialStatus = service.status
    let isFreshRegistration = initialStatus == .notRegistered
    switch initialStatus {
    case .notFound:
      throw notPackagedError()
    case .requiresApproval:
      approvalHandler()
      throw approvalRequiredError()
    case .enabled:
      if isRuntimeReady(expectedFingerprint: expectedFingerprint) {
        return BackgroundAgentRegistrationOutcome(
          newlyRegistered: false
        )
      }
      try await prepareForReplacement()
      try await unregister()
    case .notRegistered:
      break
    @unknown default:
      throw unknownStatusError()
    }

    var didRegister = false
    do {
      try service.register()
      didRegister = true
    } catch {
      let currentStatus = service.status
      if currentStatus == .requiresApproval {
        approvalHandler()
        throw approvalRequiredError()
      }
      if currentStatus != .enabled {
        throw mapRegistrationError(error)
      }
    }

    switch service.status {
    case .enabled:
      do {
        try await waitUntilReady(
          expectedFingerprint: expectedFingerprint
        )
      } catch {
        guard didRegister, isFreshRegistration else {
          throw error
        }
        do {
          try await unregister()
        } catch let rollbackError {
          throw BridgeError(
            code: "NP_MAC_INSTALL_ROLLBACK_FAILED",
            message:
              "\(error.localizedDescription)；"
              + "\(descriptor.productName) 回滚失败："
              + rollbackError.localizedDescription
          )
        }
        throw error
      }
      return BackgroundAgentRegistrationOutcome(
        newlyRegistered: didRegister && isFreshRegistration
      )
    case .requiresApproval:
      approvalHandler()
      throw approvalRequiredError()
    case .notFound:
      throw notPackagedError()
    case .notRegistered:
      throw BridgeError(
        code: "\(descriptor.errorPrefix)_REGISTRATION_FAILED",
        message:
          "\(descriptor.productName) 未能登记为用户后台项目。"
      )
    @unknown default:
      throw unknownStatusError()
    }
  }

  func unregister() async throws {
    switch service.status {
    case .notRegistered:
      return
    case .notFound:
      throw notPackagedError()
    case .enabled, .requiresApproval:
      break
    @unknown default:
      throw unknownStatusError()
    }

    do {
      try await service.unregister()
    } catch {
      let nsError = error as NSError
      if nsError.domain == SMAppServiceErrorDomain,
        nsError.code == kSMErrorJobNotFound
      {
        return
      }
      throw BridgeError(
        code: "\(descriptor.errorPrefix)_UNREGISTER_FAILED",
        message:
          "无法停止并移除 \(descriptor.productName) 后台项目："
          + nsError.localizedDescription
      )
    }
  }

  static func snapshot(
    status: SMAppService.Status,
    runtimeReady: Bool,
    requiresUpgrade: Bool = false
  ) -> BackgroundAgentSnapshot {
    switch status {
    case .notRegistered:
      BackgroundAgentSnapshot(
        registered: false,
        enabled: false,
        requiresApproval: false,
        found: true,
        ready: false,
        requiresUpgrade: false
      )
    case .enabled:
      BackgroundAgentSnapshot(
        registered: true,
        enabled: true,
        requiresApproval: false,
        found: true,
        ready: runtimeReady,
        requiresUpgrade: requiresUpgrade
      )
    case .requiresApproval:
      BackgroundAgentSnapshot(
        registered: true,
        enabled: false,
        requiresApproval: true,
        found: true,
        ready: false,
        requiresUpgrade: false
      )
    case .notFound:
      BackgroundAgentSnapshot(
        registered: false,
        enabled: false,
        requiresApproval: false,
        found: false,
        ready: false,
        requiresUpgrade: false
      )
    @unknown default:
      BackgroundAgentSnapshot(
        registered: false,
        enabled: false,
        requiresApproval: false,
        found: false,
        ready: false,
        requiresUpgrade: false
      )
    }
  }

  private func waitUntilReady(
    expectedFingerprint: String
  ) async throws {
    for attempt in 0..<readinessAttempts {
      if isRuntimeReady(expectedFingerprint: expectedFingerprint) {
        return
      }
      if attempt + 1 < readinessAttempts {
        try await Task.sleep(for: readinessDelay)
      }
    }
    throw BridgeError(
      code: "\(descriptor.errorPrefix)_NOT_READY",
      message:
        "\(descriptor.productName) 已获准运行，但"
        + "\(descriptor.readinessDescription)未在限定时间内就绪。"
    )
  }

  private func inspectRuntime() -> BackgroundRuntimeState {
    guard let fingerprint = try? expectedFingerprint() else {
      return .notReady
    }
    return runtimeInspector(fingerprint)
  }

  private func isRuntimeReady(expectedFingerprint: String) -> Bool {
    runtimeInspector(expectedFingerprint) == .ready
  }

  private func expectedFingerprint() throws -> String {
    do {
      return try fingerprintProvider()
    } catch {
      throw BridgeError(
        code: "\(descriptor.errorPrefix)_FINGERPRINT_INVALID",
        message:
          "当前 NonProxy 安装包缺少有效的 "
          + "\(descriptor.productName) 版本指纹。"
      )
    }
  }

  private func requireInstallationEligibility() throws {
    do {
      try installationValidator()
    } catch let error as BridgeError {
      throw error
    } catch {
      throw BridgeError(
        code: "NP_MAC_APP_GROUP_UNAVAILABLE",
        message: "当前应用无法访问 NonProxy 共享 App Group，请检查签名与权限。"
      )
    }
  }

  private func mapRegistrationError(_ error: Error) -> BridgeError {
    let nsError = error as NSError
    guard nsError.domain == SMAppServiceErrorDomain else {
      return registrationError(nsError.localizedDescription)
    }
    switch nsError.code {
    case kSMErrorInvalidSignature:
      return BridgeError(
        code: "\(descriptor.errorPrefix)_INVALID_SIGNATURE",
        message:
          "\(descriptor.productName) 或宿主应用的代码签名无效。"
      )
    case kSMErrorJobPlistNotFound, kSMErrorToolNotValid:
      return notPackagedError()
    case kSMErrorLaunchDeniedByUser:
      return approvalRequiredError()
    default:
      return registrationError(nsError.localizedDescription)
    }
  }

  private func registrationError(_ detail: String) -> BridgeError {
    BridgeError(
      code: "\(descriptor.errorPrefix)_REGISTRATION_FAILED",
      message:
        "无法登记 \(descriptor.productName) 后台项目：" + detail
    )
  }

  private func approvalRequiredError() -> BridgeError {
    BridgeError(
      code: "\(descriptor.errorPrefix)_APPROVAL_REQUIRED",
      message:
        "请在“系统设置 → 通用 → 登录项与扩展”中允许 "
        + "NonProxy 后台项目，然后重试。"
    )
  }

  private func notPackagedError() -> BridgeError {
    BridgeError(
      code: "\(descriptor.errorPrefix)_NOT_PACKAGED",
      message:
        "当前 NonProxy 安装包缺少 \(descriptor.packagedDescription)。"
    )
  }

  private func unknownStatusError() -> BridgeError {
    BridgeError(
      code: "\(descriptor.errorPrefix)_STATUS_UNKNOWN",
      message:
        "macOS 返回了无法识别的 \(descriptor.productName) 后台项目状态。"
    )
  }
}

@MainActor
protocol BackgroundAgentServicing: AnyObject {
  var status: SMAppService.Status { get }

  func register() throws
  func unregister() async throws
}

extension SMAppService: BackgroundAgentServicing {}

enum BackgroundRuntimeState {
  case ready
  case requiresReplacement
  case notReady
}
