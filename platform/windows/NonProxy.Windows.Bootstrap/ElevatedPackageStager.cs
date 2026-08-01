using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace NonProxy.Windows.Bootstrap;

[SupportedOSPlatform("windows")]
internal sealed partial class ElevatedPackageStager : IDisposable
{
    private const int MaximumEntries = 20_000;
    private const long MaximumBytes = 4L * 1024 * 1024 * 1024;
    private const string StagingSddl =
        "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
    private readonly string _path;

    private ElevatedPackageStager(string path)
    {
        _path = path;
    }

    public string Path => _path;

    public static ElevatedPackageStager Create(string sourceRoot)
    {
        var programFiles = Environment.GetFolderPath(
            Environment.SpecialFolder.ProgramFiles);
        if (string.IsNullOrWhiteSpace(programFiles))
        {
            throw new InvalidOperationException("无法定位 Program Files。 ");
        }
        var productRoot = System.IO.Path.Combine(programFiles, "NonProxy");
        var stagingRoot = System.IO.Path.Combine(productRoot, "installer-staging");
        AssertRegularDirectory(programFiles);
        EnsureRegularDirectory(productRoot);
        EnsureRegularDirectory(stagingRoot);
        var destination = System.IO.Path.Combine(
            stagingRoot,
            Guid.NewGuid().ToString("N"));
        CreateProtectedDirectory(destination);
        var stager = new ElevatedPackageStager(destination);
        try
        {
            CopyTree(sourceRoot, destination);
            return stager;
        }
        catch
        {
            stager.Dispose();
            throw;
        }
    }

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_path)
                && IsExactStagingChild(_path))
            {
                Directory.Delete(_path, recursive: true);
            }
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException)
        {
            // 安装结果优先返回；遗留的管理员保护 staging 可由后续维护清理。
        }
    }

    private static void CopyTree(string sourceRoot, string destinationRoot)
    {
        var source = System.IO.Path.GetFullPath(sourceRoot)
            .TrimEnd(System.IO.Path.DirectorySeparatorChar);
        if (string.Equals(
            source,
            System.IO.Path.GetPathRoot(source),
            StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("发布包根目录不能是磁盘根目录。");
        }
        AssertRegularDirectory(source);
        var pending = new Stack<(string Source, string Destination)>();
        pending.Push((source, destinationRoot));
        var entries = 0;
        long bytes = 0;
        while (pending.Count > 0)
        {
            var directory = pending.Pop();
            foreach (var entry in Directory.EnumerateFileSystemEntries(
                directory.Source))
            {
                if (++entries > MaximumEntries)
                {
                    throw new InvalidDataException("发布包文件数量超出上限。");
                }
                var attributes = File.GetAttributes(entry);
                if ((attributes & FileAttributes.ReparsePoint) != 0)
                {
                    throw new InvalidDataException("发布包不允许重解析点。");
                }
                var target = System.IO.Path.Combine(
                    directory.Destination,
                    System.IO.Path.GetFileName(entry));
                if ((attributes & FileAttributes.Directory) != 0)
                {
                    Directory.CreateDirectory(target);
                    pending.Push((entry, target));
                    continue;
                }
                var length = new FileInfo(entry).Length;
                bytes = checked(bytes + length);
                if (bytes > MaximumBytes)
                {
                    throw new InvalidDataException("发布包总大小超出上限。");
                }
                File.Copy(entry, target, overwrite: false);
            }
        }
    }

    private static void CreateProtectedDirectory(string path)
    {
        if (!ConvertStringSecurityDescriptorToSecurityDescriptor(
                StagingSddl,
                1,
                out var descriptor,
                out _))
        {
            throw new Win32Exception(
                Marshal.GetLastPInvokeError(),
                "无法创建安装 staging 安全描述符。");
        }
        try
        {
            var attributes = new SecurityAttributes
            {
                Length = checked((uint)Marshal.SizeOf<SecurityAttributes>()),
                SecurityDescriptor = descriptor,
            };
            if (!CreateDirectory(path, ref attributes))
            {
                throw new Win32Exception(
                    Marshal.GetLastPInvokeError(),
                    "无法创建管理员保护的安装 staging。 ");
            }
        }
        finally
        {
            _ = LocalFree(descriptor);
        }
    }

    private static bool IsExactStagingChild(string path)
    {
        var parent = System.IO.Path.GetDirectoryName(path);
        return parent is not null
            && string.Equals(
                System.IO.Path.GetFileName(parent),
                "installer-staging",
                StringComparison.OrdinalIgnoreCase)
            && System.IO.Path.GetFileName(path).Length == 32;
    }

    private static void AssertRegularDirectory(string path)
    {
        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
        {
            throw new InvalidDataException("安装 staging 路径不允许重解析点。");
        }
    }

    private static void EnsureRegularDirectory(string path)
    {
        if (Directory.Exists(path))
        {
            AssertRegularDirectory(path);
            return;
        }
        Directory.CreateDirectory(path);
        AssertRegularDirectory(path);
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct SecurityAttributes
    {
        public uint Length;
        public nint SecurityDescriptor;
        public int InheritHandle;
    }

    [LibraryImport(
        "advapi32.dll",
        EntryPoint = "ConvertStringSecurityDescriptorToSecurityDescriptorW",
        SetLastError = true,
        StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool ConvertStringSecurityDescriptorToSecurityDescriptor(
        string stringSecurityDescriptor,
        uint stringSecurityDescriptorRevision,
        out nint securityDescriptor,
        out uint securityDescriptorSize);

    [LibraryImport(
        "kernel32.dll",
        EntryPoint = "CreateDirectoryW",
        SetLastError = true,
        StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool CreateDirectory(
        string path,
        ref SecurityAttributes securityAttributes);

    [LibraryImport("kernel32.dll")]
    private static partial nint LocalFree(nint memory);
}
