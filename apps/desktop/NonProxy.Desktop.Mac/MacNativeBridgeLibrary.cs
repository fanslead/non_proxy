using System.Reflection;
using System.Runtime.InteropServices;

namespace NonProxy.Desktop.Mac;

internal static class MacNativeBridgeLibrary
{
    private const string FileName = "libNonProxyMacHostBridge.dylib";
    private static int _initialized;

    internal static void EnsureResolverRegistered()
    {
        if (Interlocked.Exchange(ref _initialized, 1) == 1)
        {
            return;
        }

        NativeLibrary.SetDllImportResolver(
            typeof(MacNativeBridgeLibrary).Assembly,
            Resolve);
    }

    private static nint Resolve(
        string libraryName,
        Assembly assembly,
        DllImportSearchPath? searchPath)
    {
        if (!string.Equals(
                libraryName,
                MacNativeBridgeMethods.LibraryName,
                StringComparison.Ordinal))
        {
            return nint.Zero;
        }

        foreach (var candidate in CandidatePaths())
        {
            if (File.Exists(candidate)
                && NativeLibrary.TryLoad(candidate, out var handle))
            {
                return handle;
            }
        }

        return nint.Zero;
    }

    private static IEnumerable<string> CandidatePaths()
    {
        yield return Path.GetFullPath(Path.Combine(
            AppContext.BaseDirectory,
            "..",
            "Frameworks",
            FileName));

        var processPath = Environment.ProcessPath;
        if (!string.IsNullOrWhiteSpace(processPath))
        {
            var executableDirectory = Path.GetDirectoryName(processPath);
            if (!string.IsNullOrWhiteSpace(executableDirectory))
            {
                yield return Path.GetFullPath(Path.Combine(
                    executableDirectory,
                    "..",
                    "Frameworks",
                    FileName));
            }
        }
    }
}
