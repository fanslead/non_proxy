using NonProxy.Desktop.Core.Platform;

namespace NonProxy.Desktop.Mac;

internal sealed class SystemExtensionController(
    MacNativeBridgeClient nativeBridge) : ISystemComponentInstaller
{
    private const string MissingBridgeCode = "NP_MAC_BRIDGE_LIBRARY_NOT_FOUND";
    private const string MissingEntitlementCode = "NP_MAC_MISSING_ENTITLEMENT";
    private const string MissingGatewayCode = "NP_MAC_GATEWAY_NOT_PACKAGED";
    private const string InvalidGatewaySignatureCode =
        "NP_MAC_GATEWAY_INVALID_SIGNATURE";
    private const string MissingAdapterHostCode =
        "NP_MAC_ADAPTER_HOST_NOT_PACKAGED";
    private const string InvalidAdapterHostSignatureCode =
        "NP_MAC_ADAPTER_HOST_INVALID_SIGNATURE";
    private const string MissingAppGroupCode =
        "NP_MAC_APP_GROUP_UNAVAILABLE";
    private const string GatewayApprovalCode = "NP_MAC_GATEWAY_APPROVAL_REQUIRED";
    private const string AdapterHostApprovalCode =
        "NP_MAC_ADAPTER_HOST_APPROVAL_REQUIRED";
    private const string UserApprovalCode = "NP_MAC_USER_APPROVAL_REQUIRED";
    private const string BridgeBusyCode = "NP_MAC_BRIDGE_BUSY";
    private int _awaitingApproval;

    public async Task<SystemComponentState> GetStateAsync(
        CancellationToken cancellationToken)
    {
        if (Volatile.Read(ref _awaitingApproval) == 1)
        {
            return new SystemComponentState(
                SystemComponentStatus.AwaitingApproval,
                "请在系统设置中允许 NonProxy 后台项目或网络扩展。",
                UserApprovalCode,
                canOpenSystemSettings: true);
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
                    "请在系统设置中允许 NonProxy 后台项目或网络扩展。",
                    UserApprovalCode,
                    BuildSteps(state),
                    canOpenSystemSettings: true);
            }

            if (!state.GatewayAgent.Found)
            {
                return new SystemComponentState(
                    SystemComponentStatus.Unavailable,
                    "当前安装包缺少 gatewayd 后台项目。",
                    MissingGatewayCode,
                    BuildSteps(state));
            }

            if (!state.AdapterHostAgent.Found)
            {
                return new SystemComponentState(
                    SystemComponentStatus.Unavailable,
                    "当前安装包缺少 adapter-host 后台项目。",
                    MissingAdapterHostCode,
                    BuildSteps(state));
            }

            if (IsReady(state))
            {
                return new SystemComponentState(
                    SystemComponentStatus.Installed,
                    "后台服务、系统扩展和网络配置均已就绪。",
                    steps: BuildSteps(state));
            }

            if (IsAbsent(state))
            {
                return new SystemComponentState(
                    SystemComponentStatus.NotInstalled,
                    "NonProxy 系统组件尚未安装。",
                    steps: BuildSteps(state));
            }

            return new SystemComponentState(
                SystemComponentStatus.Failed,
                "NonProxy 系统组件仅部分就绪，请执行修复。",
                "NP_MAC_COMPONENT_PARTIAL",
                BuildSteps(state));
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
                result.ErrorCode,
                result.RequiresReboot);
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
                result.ErrorCode,
                result.RequiresReboot);
        }
        catch (MacNativeBridgeException exception)
        {
            return new InstallResult(false, exception.Message, exception.ErrorCode);
        }
    }

    public Task<InstallResult> OpenSystemSettingsAsync(
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        try
        {
            nativeBridge.OpenLoginItemsSystemSettings();
            return Task.FromResult(new InstallResult(
                true,
                "已打开“登录项与扩展”，允许 NonProxy 后回到应用重新检查。"));
        }
        catch (MacNativeBridgeException exception)
        {
            return Task.FromResult(new InstallResult(
                false,
                exception.Message,
                exception.ErrorCode));
        }
    }

    private static bool IsAwaitingApproval(MacHostState state)
    {
        return state.TransparentExtension.AwaitingUserApproval
            || state.DnsExtension.AwaitingUserApproval
            || state.GatewayAgent.RequiresApproval
            || state.AdapterHostAgent.RequiresApproval;
    }

    private static bool IsReady(MacHostState state)
    {
        return state.GatewayAgent.Enabled
            && state.GatewayAgent.Ready
            && state.AdapterHostAgent.Enabled
            && state.AdapterHostAgent.Ready
            && state.TransparentExtension.Enabled
            && state.DnsExtension.Enabled
            && state.TransparentPreference.Enabled
            && state.DnsPreference.Enabled;
    }

    private static bool IsAbsent(MacHostState state)
    {
        return !state.GatewayAgent.Registered
            && state.GatewayAgent.Found
            && !state.AdapterHostAgent.Registered
            && state.AdapterHostAgent.Found
            && !state.TransparentExtension.Installed
            && !state.DnsExtension.Installed
            && !state.TransparentPreference.Configured
            && !state.DnsPreference.Configured;
    }

    private static SystemComponentState FailureState(
        MacBridgeEventPayload result)
    {
        var status = StatusForErrorCode(result.ErrorCode);
        return new SystemComponentState(
            status,
            result.Message,
            result.ErrorCode,
            canOpenSystemSettings:
                status == SystemComponentStatus.AwaitingApproval);
    }

    private static SystemComponentState ExceptionState(
        MacNativeBridgeException exception)
    {
        var status = StatusForErrorCode(exception.ErrorCode);
        return new SystemComponentState(
            status,
            exception.Message,
            exception.ErrorCode,
            canOpenSystemSettings:
                status == SystemComponentStatus.AwaitingApproval);
    }

    private static IReadOnlyList<SystemComponentStep> BuildSteps(
        MacHostState state)
    {
        return
        [
            BackgroundAgentStep(
                "gateway",
                "路由后台服务",
                "gatewayd",
                "两个本地私有通道已就绪。",
                state.GatewayAgent),
            BackgroundAgentStep(
                "adapter-host",
                "客户端适配服务",
                "adapter-host",
                "适配器私有通道已就绪。",
                state.AdapterHostAgent),
            ExtensionStep(
                "transparent-proxy",
                "透明代理",
                state.TransparentExtension),
            ExtensionStep(
                "dns-proxy",
                "DNS 分流",
                state.DnsExtension),
            NetworkPreferenceStep(state),
        ];
    }

    private static SystemComponentStep BackgroundAgentStep(
        string id,
        string name,
        string productName,
        string readyDescription,
        MacBackgroundAgentSnapshot agent)
    {
        if (!agent.Found)
        {
            return new SystemComponentStep(
                id,
                name,
                SystemComponentStepStatus.Unavailable,
                $"安装包中缺少 {productName}。");
        }
        if (agent.RequiresApproval)
        {
            return new SystemComponentStep(
                id,
                name,
                SystemComponentStepStatus.AwaitingApproval,
                "需要在系统设置中允许后台项目。");
        }
        if (agent.RequiresUpgrade)
        {
            return new SystemComponentStep(
                id,
                name,
                SystemComponentStepStatus.NeedsRepair,
                $"运行中的 {productName} 版本较旧，需要安全升级。");
        }
        if (agent.Enabled && agent.Ready)
        {
            return new SystemComponentStep(
                id,
                name,
                SystemComponentStepStatus.Ready,
                readyDescription);
        }
        if (agent.Registered)
        {
            return new SystemComponentStep(
                id,
                name,
                SystemComponentStepStatus.NeedsRepair,
                "已登记，但本地通道未就绪。");
        }
        return new SystemComponentStep(
            id,
            name,
            SystemComponentStepStatus.NotInstalled,
            "尚未登记用户级后台项目。");
    }

    private static SystemComponentStep ExtensionStep(
        string id,
        string name,
        MacSystemExtensionSnapshot extension)
    {
        if (extension.AwaitingUserApproval)
        {
            return new SystemComponentStep(
                id,
                name,
                SystemComponentStepStatus.AwaitingApproval,
                "需要在系统设置中允许网络扩展。");
        }
        if (extension.Enabled)
        {
            return new SystemComponentStep(
                id,
                name,
                SystemComponentStepStatus.Ready,
                "系统扩展已安装并启用。");
        }
        if (extension.Installed)
        {
            return new SystemComponentStep(
                id,
                name,
                SystemComponentStepStatus.NeedsRepair,
                "系统扩展已安装，但尚未启用。");
        }
        return new SystemComponentStep(
            id,
            name,
            SystemComponentStepStatus.NotInstalled,
            "系统扩展尚未安装。");
    }

    private static SystemComponentStep NetworkPreferenceStep(
        MacHostState state)
    {
        if (state.TransparentPreference.Enabled
            && state.DnsPreference.Enabled)
        {
            return new SystemComponentStep(
                "network-routing",
                "网络接管",
                SystemComponentStepStatus.Ready,
                "透明代理与 DNS 配置均已启用。");
        }
        if (state.TransparentPreference.Configured
            || state.DnsPreference.Configured)
        {
            return new SystemComponentStep(
                "network-routing",
                "网络接管",
                SystemComponentStepStatus.NeedsRepair,
                "网络配置仅部分存在或尚未启用。");
        }
        return new SystemComponentStep(
            "network-routing",
            "网络接管",
            SystemComponentStepStatus.NotInstalled,
            "NonProxy 尚未接管系统网络。");
    }

    private static SystemComponentStatus StatusForErrorCode(string? errorCode)
    {
        if (errorCode is GatewayApprovalCode
            or AdapterHostApprovalCode
            or UserApprovalCode)
        {
            return SystemComponentStatus.AwaitingApproval;
        }

        if (IsUnavailableCode(errorCode))
        {
            return SystemComponentStatus.Unavailable;
        }

        return errorCode == BridgeBusyCode
            ? SystemComponentStatus.Unknown
            : SystemComponentStatus.Failed;
    }

    private static bool IsUnavailableCode(string? errorCode)
    {
        return errorCode is MissingBridgeCode
            or MissingEntitlementCode
            or MissingGatewayCode
            or InvalidGatewaySignatureCode
            or MissingAdapterHostCode
            or InvalidAdapterHostSignatureCode
            or MissingAppGroupCode;
    }
}
