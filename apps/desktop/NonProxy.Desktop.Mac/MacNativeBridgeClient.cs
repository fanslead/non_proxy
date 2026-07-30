using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Serialization.Metadata;

namespace NonProxy.Desktop.Mac;

internal sealed class MacNativeBridgeClient
{
    internal const uint SupportedAbiVersion = 3;
    private const int StartAccepted = 0;
    private long _nextOperationId;
    private int _abiValidated;

    internal async Task<MacBridgeProbePayload> ProbeAsync(
        CancellationToken cancellationToken)
    {
        var json = await InvokeAsync(
            NativeOperation.Probe,
            approvalRequired: null,
            operationCompleted: null,
            cancellationToken);
        return Deserialize(
            json,
            MacNativeJsonContext.Default.MacBridgeProbePayload);
    }

    internal async Task<MacBridgeEventPayload> QueryAsync(
        CancellationToken cancellationToken)
    {
        var json = await InvokeAsync(
            NativeOperation.Query,
            approvalRequired: null,
            operationCompleted: null,
            cancellationToken);
        return Deserialize(
            json,
            MacNativeJsonContext.Default.MacBridgeEventPayload);
    }

    internal async Task<MacBridgeEventPayload> InstallAndEnableAsync(
        Action approvalRequired,
        Action operationCompleted,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(approvalRequired);
        ArgumentNullException.ThrowIfNull(operationCompleted);
        var json = await InvokeAsync(
            NativeOperation.InstallAndEnable,
            approvalRequired,
            operationCompleted,
            cancellationToken);
        return Deserialize(
            json,
            MacNativeJsonContext.Default.MacBridgeEventPayload);
    }

    internal async Task<MacBridgeEventPayload> DisableAndUninstallAsync(
        CancellationToken cancellationToken)
    {
        var json = await InvokeAsync(
            NativeOperation.DisableAndUninstall,
            approvalRequired: null,
            operationCompleted: null,
            cancellationToken);
        return Deserialize(
            json,
            MacNativeJsonContext.Default.MacBridgeEventPayload);
    }

    internal void OpenLoginItemsSystemSettings()
    {
        EnsureAbiVersion();
        try
        {
            var result = MacNativeBridgeMethods.OpenLoginItemsSystemSettings();
            if (result != StartAccepted)
            {
                throw new MacNativeBridgeException(
                    "NP_MAC_SYSTEM_SETTINGS_OPEN_FAILED",
                    "无法打开 macOS 后台项目设置。");
            }
        }
        catch (Exception exception)
            when (exception is DllNotFoundException
                or EntryPointNotFoundException
                or BadImageFormatException)
        {
            throw new MacNativeBridgeException(
                "NP_MAC_BRIDGE_LIBRARY_NOT_FOUND",
                "当前安装包无法加载 macOS 原生宿主桥接。",
                exception);
        }
    }

    private unsafe Task<string> InvokeAsync(
        NativeOperation operation,
        Action? approvalRequired,
        Action? operationCompleted,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        EnsureAbiVersion();

        var operationId = unchecked((ulong)Interlocked.Increment(
            ref _nextOperationId));
        var context = new MacNativeBridgeOperation(
            operationId,
            approvalRequired,
            operationCompleted,
            cancellationToken);
        var handle = context.AllocateHandle();

        int startResult;
        try
        {
            delegate* unmanaged[Cdecl]<
                ulong,
                int,
                int,
                byte*,
                nuint,
                nint,
                void> callback = &HandleNativeCallback;
            startResult = operation switch
            {
                NativeOperation.Probe => MacNativeBridgeMethods.Probe(
                    operationId,
                    callback,
                    handle),
                NativeOperation.Query => MacNativeBridgeMethods.Query(
                    operationId,
                    callback,
                    handle),
                NativeOperation.InstallAndEnable =>
                    MacNativeBridgeMethods.InstallAndEnable(
                        operationId,
                        callback,
                        handle),
                NativeOperation.DisableAndUninstall =>
                    MacNativeBridgeMethods.DisableAndUninstall(
                        operationId,
                        callback,
                        handle),
                _ => -1,
            };
        }
        catch (Exception exception)
            when (exception is DllNotFoundException
                or EntryPointNotFoundException
                or BadImageFormatException)
        {
            context.CompleteStartFailure(
                "NP_MAC_BRIDGE_LIBRARY_NOT_FOUND",
                "当前安装包无法加载 macOS 原生宿主桥接。",
                exception);
            return context.Completion;
        }

        if (startResult != StartAccepted)
        {
            context.CompleteStartFailure(
                StartErrorCode(startResult),
                StartErrorMessage(startResult));
        }
        return context.Completion;
    }

    private void EnsureAbiVersion()
    {
        if (Volatile.Read(ref _abiValidated) == 1)
        {
            return;
        }

        MacNativeBridgeLibrary.EnsureResolverRegistered();
        uint actualVersion;
        try
        {
            actualVersion = MacNativeBridgeMethods.GetAbiVersion();
        }
        catch (Exception exception)
            when (exception is DllNotFoundException
                or EntryPointNotFoundException
                or BadImageFormatException)
        {
            throw new MacNativeBridgeException(
                "NP_MAC_BRIDGE_LIBRARY_NOT_FOUND",
                "当前安装包无法加载 macOS 原生宿主桥接。",
                exception);
        }
        if (actualVersion != SupportedAbiVersion)
        {
            throw new MacNativeBridgeException(
                "NP_MAC_BRIDGE_ABI_MISMATCH",
                $"原生桥接 ABI 版本为 {actualVersion}，应用要求 {SupportedAbiVersion}。");
        }
        Volatile.Write(ref _abiValidated, 1);
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe void HandleNativeCallback(
        ulong operationId,
        int eventKind,
        int statusCode,
        byte* payload,
        nuint payloadLength,
        nint contextPointer)
    {
        try
        {
            var handle = GCHandle.FromIntPtr(contextPointer);
            if (handle.Target is MacNativeBridgeOperation context)
            {
                context.HandleCallback(
                    operationId,
                    eventKind,
                    statusCode,
                    payload,
                    payloadLength);
            }
        }
        catch
        {
            // 任何异常都必须止于托管回调，不能穿过 Swift/C ABI 边界。
        }
    }

    private static T Deserialize<T>(
        string json,
        JsonTypeInfo<T> typeInfo)
    {
        try
        {
            return JsonSerializer.Deserialize(json, typeInfo)
                ?? throw new JsonException("响应为空。");
        }
        catch (JsonException exception)
        {
            throw new MacNativeBridgeException(
                "NP_MAC_BRIDGE_INVALID_JSON",
                "原生桥接返回了无法识别的 JSON。",
                exception);
        }
    }

    private static string StartErrorCode(int startResult)
    {
        return startResult switch
        {
            -2 => "NP_MAC_BRIDGE_BUSY",
            _ => "NP_MAC_BRIDGE_INVALID_ARGUMENT",
        };
    }

    private static string StartErrorMessage(int startResult)
    {
        return startResult switch
        {
            -2 => "已有一个 macOS 系统操作正在进行，请稍后重试。",
            _ => "无法启动 macOS 原生桥接操作。",
        };
    }

    private enum NativeOperation
    {
        Probe,
        Query,
        InstallAndEnable,
        DisableAndUninstall,
    }
}
