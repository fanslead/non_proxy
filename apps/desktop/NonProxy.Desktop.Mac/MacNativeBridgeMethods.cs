using System.Runtime.InteropServices;

namespace NonProxy.Desktop.Mac;

internal static unsafe partial class MacNativeBridgeMethods
{
    internal const string LibraryName = "NonProxyMacHostBridge";

    [LibraryImport(LibraryName, EntryPoint = "np_mac_bridge_abi_version")]
    internal static partial uint GetAbiVersion();

    [LibraryImport(LibraryName, EntryPoint = "np_mac_bridge_probe")]
    internal static partial int Probe(
        ulong operationId,
        delegate* unmanaged[Cdecl]<
            ulong,
            int,
            int,
            byte*,
            nuint,
            nint,
            void> callback,
        nint context);

    [LibraryImport(LibraryName, EntryPoint = "np_mac_bridge_query")]
    internal static partial int Query(
        ulong operationId,
        delegate* unmanaged[Cdecl]<
            ulong,
            int,
            int,
            byte*,
            nuint,
            nint,
            void> callback,
        nint context);

    [LibraryImport(
        LibraryName,
        EntryPoint = "np_mac_bridge_install_and_enable")]
    internal static partial int InstallAndEnable(
        ulong operationId,
        delegate* unmanaged[Cdecl]<
            ulong,
            int,
            int,
            byte*,
            nuint,
            nint,
            void> callback,
        nint context);

    [LibraryImport(
        LibraryName,
        EntryPoint = "np_mac_bridge_disable_and_uninstall")]
    internal static partial int DisableAndUninstall(
        ulong operationId,
        delegate* unmanaged[Cdecl]<
            ulong,
            int,
            int,
            byte*,
            nuint,
            nint,
            void> callback,
        nint context);
}
