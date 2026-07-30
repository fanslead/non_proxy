import Foundation

@_cdecl("np_mac_bridge_abi_version")
public func macBridgeABIVersion() -> UInt32 {
    BridgeConstants.abiVersion
}

@_cdecl("np_mac_bridge_probe")
public func macBridgeProbe(
    operationID: UInt64,
    callback: MacBridgeCallback?,
    context: UnsafeMutableRawPointer?
) -> Int32 {
    guard let callback else {
        return -1
    }
    let sink = BridgeCallbackSink(
        operationID: operationID,
        callback: callback,
        context: context
    )
    DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(10)) {
        sink.completeProbe(ProbePayload(
            abiVersion: BridgeConstants.abiVersion,
            message: "NonProxy 原生桥接已连接"
        ))
    }
    return 0
}

@_cdecl("np_mac_bridge_query")
public func macBridgeQuery(
    operationID: UInt64,
    callback: MacBridgeCallback?,
    context: UnsafeMutableRawPointer?
) -> Int32 {
    guard let callback else {
        return -1
    }
    guard BridgeOperationGate.shared.begin() else {
        return -2
    }
    let sink = BridgeCallbackSink(
        operationID: operationID,
        callback: callback,
        context: context
    )
    Task { @MainActor in
        defer { BridgeOperationGate.shared.end() }
        await MacHostBridgeService.query(sink: sink)
    }
    return 0
}

@_cdecl("np_mac_bridge_install_and_enable")
public func macBridgeInstallAndEnable(
    operationID: UInt64,
    callback: MacBridgeCallback?,
    context: UnsafeMutableRawPointer?
) -> Int32 {
    guard let callback else {
        return -1
    }
    guard BridgeOperationGate.shared.begin() else {
        return -2
    }
    let sink = BridgeCallbackSink(
        operationID: operationID,
        callback: callback,
        context: context
    )
    Task { @MainActor in
        defer { BridgeOperationGate.shared.end() }
        await MacHostBridgeService.installAndEnable(sink: sink)
    }
    return 0
}

@_cdecl("np_mac_bridge_disable_and_uninstall")
public func macBridgeDisableAndUninstall(
    operationID: UInt64,
    callback: MacBridgeCallback?,
    context: UnsafeMutableRawPointer?
) -> Int32 {
    guard let callback else {
        return -1
    }
    guard BridgeOperationGate.shared.begin() else {
        return -2
    }
    let sink = BridgeCallbackSink(
        operationID: operationID,
        callback: callback,
        context: context
    )
    Task { @MainActor in
        defer { BridgeOperationGate.shared.end() }
        await MacHostBridgeService.disableAndUninstall(sink: sink)
    }
    return 0
}
