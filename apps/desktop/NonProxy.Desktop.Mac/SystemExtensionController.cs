using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Mac;

internal sealed class SystemExtensionController(
    MacNativeBridgeClient nativeBridge) : ISystemComponentInstaller
{
    private const string MissingBridgeCode = "NP_MAC_BRIDGE_LIBRARY_NOT_FOUND";
    private const string MissingEntitlementCode = "NP_MAC_MISSING_ENTITLEMENT";
    private const string BridgeBusyCode = "NP_MAC_BRIDGE_BUSY";
    private int _awaitingApproval;

    public async Task<SystemComponentState> GetStateAsync(
        CancellationToken cancellationToken)
    {
        if (Volatile.Read(ref _awaitingApproval) == 1)
        {
            return new SystemComponentState(
                SystemComponentStatus.AwaitingApproval,
                "请在系统设置中允许 NonProxy 网络扩展。");
        }

        try
        {
            var result = await nativeBridge.QueryAsync(cancellationToken);
            if (!result.Success || result.State is null)
            {
                return FailureState(result);
            }

            var state = result.State;
            if (IsAwaitingApproval(state)
                || Volatile.Read(ref _awaitingApproval) == 1)
            {
                return new SystemComponentState(
                    SystemComponentStatus.AwaitingApproval,
                    "请在系统设置中允许 NonProxy 网络扩展。");
            }

            if (IsReady(state))
            {
                return new SystemComponentState(
                    SystemComponentStatus.Installed,
                    "系统扩展和网络配置均已就绪。");
            }

            if (IsAbsent(state))
            {
                return new SystemComponentState(
                    SystemComponentStatus.NotInstalled,
                    "NonProxy 系统扩展尚未安装。");
            }

            return new SystemComponentState(
                SystemComponentStatus.Failed,
                "NonProxy 系统组件仅部分就绪，请执行修复。",
                "NP_MAC_COMPONENT_PARTIAL");
        }
        catch (MacNativeBridgeException exception)
        {
            return ExceptionState(exception);
        }
    }

    public async Task<InstallResult> InstallAsync(
        CancellationToken cancellationToken)
    {
        Volatile.Write(ref _awaitingApproval, 0);
        try
        {
            var result = await nativeBridge.InstallAndEnableAsync(
                () => Volatile.Write(ref _awaitingApproval, 1),
                () => Volatile.Write(ref _awaitingApproval, 0),
                cancellationToken);
            Volatile.Write(ref _awaitingApproval, 0);
            return new InstallResult(
                result.Success,
                result.Message,
                result.ErrorCode);
        }
        catch (MacNativeBridgeException exception)
        {
            Volatile.Write(ref _awaitingApproval, 0);
            return new InstallResult(false, exception.Message, exception.ErrorCode);
        }
    }

    public async Task<InstallResult> UninstallAsync(
        CancellationToken cancellationToken)
    {
        Volatile.Write(ref _awaitingApproval, 0);
        try
        {
            var result = await nativeBridge.DisableAndUninstallAsync(
                cancellationToken);
            return new InstallResult(
                result.Success,
                result.Message,
                result.ErrorCode);
        }
        catch (MacNativeBridgeException exception)
        {
            return new InstallResult(false, exception.Message, exception.ErrorCode);
        }
    }

    private static bool IsAwaitingApproval(MacHostState state)
    {
        return state.TransparentExtension.AwaitingUserApproval
            || state.DnsExtension.AwaitingUserApproval;
    }

    private static bool IsReady(MacHostState state)
    {
        return state.TransparentExtension.Enabled
            && state.DnsExtension.Enabled
            && state.TransparentPreference.Enabled
            && state.DnsPreference.Enabled;
    }

    private static bool IsAbsent(MacHostState state)
    {
        return !state.TransparentExtension.Installed
            && !state.DnsExtension.Installed
            && !state.TransparentPreference.Configured
            && !state.DnsPreference.Configured;
    }

    private static SystemComponentState FailureState(
        MacBridgeEventPayload result)
    {
        var status = IsUnavailableCode(result.ErrorCode)
            ? SystemComponentStatus.Unavailable
            : result.ErrorCode == BridgeBusyCode
                ? SystemComponentStatus.Unknown
            : SystemComponentStatus.Failed;
        return new SystemComponentState(
            status,
            result.Message,
            result.ErrorCode);
    }

    private static SystemComponentState ExceptionState(
        MacNativeBridgeException exception)
    {
        var status = IsUnavailableCode(exception.ErrorCode)
            ? SystemComponentStatus.Unavailable
            : exception.ErrorCode == BridgeBusyCode
                ? SystemComponentStatus.Unknown
            : SystemComponentStatus.Failed;
        return new SystemComponentState(
            status,
            exception.Message,
            exception.ErrorCode);
    }

    private static bool IsUnavailableCode(string? errorCode)
    {
        return errorCode is MissingBridgeCode or MissingEntitlementCode;
    }
}
