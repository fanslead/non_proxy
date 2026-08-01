using System.Runtime.Versioning;
using System.Security.Principal;
using System.Text.Json;
using NonProxy.Windows.Security;

namespace NonProxy.Windows.Bootstrap;

internal static class Program
{
    private const int PackageRejectedExitCode = 20;
    private const int OperationFailedExitCode = 21;
    private const int RebootRequiredExitCode = 3010;

    public static async Task<int> Main(string[] args)
    {
        if (!OperatingSystem.IsWindowsVersionAtLeast(10, 0, 18362))
        {
            return WriteFailure(
                BootstrapAction.Query,
                "当前系统不是受支持的 Windows 版本。",
                "NP_WINDOWS_VERSION_UNSUPPORTED",
                PackageRejectedExitCode);
        }
        BootstrapArguments arguments;
        try
        {
            arguments = BootstrapArguments.Parse(args);
        }
        catch (ArgumentException exception)
        {
            return WriteFailure(
                BootstrapAction.Query,
                exception.Message,
                "NP_WINDOWS_BOOTSTRAP_ARGUMENT_INVALID",
                PackageRejectedExitCode);
        }
        var publisher = CompiledWindowsPublisherIdentity.Read(
            typeof(Program).Assembly);
        if (publisher is null)
        {
            return WriteFailure(
                arguments.Action,
                "当前构建没有编译固定的 Windows 发布者身份。",
                "NP_WINDOWS_PUBLISHER_NOT_CONFIGURED",
                PackageRejectedExitCode);
        }
        if (arguments.Action != BootstrapAction.Query && !IsAdministrator())
        {
            return WriteFailure(
                arguments.Action,
                "Windows 系统组件变更需要管理员批准。",
                "NP_WINDOWS_ELEVATION_REQUIRED",
                OperationFailedExitCode);
        }
        var validator = new ReleasePackageValidator(
            new WindowsReleaseTrustVerifier(),
            publisher);
        ElevatedPackageStager? staging = null;
        ValidatedReleasePackage package;
        try
        {
            staging = arguments.Action == BootstrapAction.Query
                ? null
                : ElevatedPackageStager.Create(arguments.PackageRoot);
            package = staging is null
                ? validator.Validate(arguments.PackageRoot)
                : validator.Validate(staging.Path);
        }
        catch (Exception exception) when (IsExpectedFailure(exception))
        {
            staging?.Dispose();
            return WriteFailure(
                arguments.Action,
                exception.Message,
                "NP_WINDOWS_PACKAGE_UNTRUSTED",
                PackageRejectedExitCode);
        }
        try
        {
            var result = await ConsumerPowerShellInstaller.RunAsync(
                arguments.Action,
                package);
            if (arguments.Action == BootstrapAction.Query)
            {
                Console.Out.WriteLine(result.Json);
            }
            return result.RequiresReboot ? RebootRequiredExitCode : 0;
        }
        catch (Exception exception) when (IsExpectedFailure(exception))
        {
            return WriteFailure(
                arguments.Action,
                exception.Message,
                "NP_WINDOWS_COMPONENT_TRANSACTION_FAILED",
                OperationFailedExitCode);
        }
        finally
        {
            staging?.Dispose();
        }
    }

    private static int WriteFailure(
        BootstrapAction action,
        string message,
        string errorCode,
        int exitCode)
    {
        if (action == BootstrapAction.Query)
        {
            Console.Out.WriteLine(JsonSerializer.Serialize(new
            {
                success = false,
                status = exitCode == PackageRejectedExitCode
                    ? "Unavailable"
                    : "Partial",
                message,
                errorCode,
                requiresReboot = false,
                steps = Array.Empty<object>(),
            }));
        }
        return exitCode;
    }

    [SupportedOSPlatform("windows")]
    private static bool IsAdministrator()
    {
        using var identity = WindowsIdentity.GetCurrent();
        return new WindowsPrincipal(identity).IsInRole(
            WindowsBuiltInRole.Administrator);
    }

    private static bool IsExpectedFailure(Exception exception) =>
        exception is ArgumentException
            or IOException
            or UnauthorizedAccessException
            or InvalidOperationException
            or System.Security.Cryptography.CryptographicException
            or JsonException
            or PlatformNotSupportedException
            or System.ComponentModel.Win32Exception
            or OverflowException
            or System.Security.SecurityException;
}
