using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using NonProxy.Windows.Security;

namespace NonProxy.Windows.Bootstrap;

public sealed class ReleasePackageValidator(
    IReleaseTrustVerifier trustVerifier,
    string expectedPublisherCertificateSha256)
{
    private const long MaximumManifestBytes = 16L * 1024 * 1024;
    private const long MaximumFileBytes = 1024L * 1024 * 1024;
    private const long MaximumPackageBytes = 4L * 1024 * 1024 * 1024;
    private const int MaximumPackageEntries = 20_000;
    private const string SignatureMarker = "# SIG # Begin signature block";
    private static readonly HashSet<string> RequiredPublisherFiles =
        new(StringComparer.OrdinalIgnoreCase)
        {
            "adapter/nonproxy-adapter-host.exe",
            "bootstrap/NonProxy.Windows.Bootstrap.exe",
            "desktop/NonProxy.Desktop.Windows.exe",
            "service/nonproxy-gatewayd.exe",
        };
    private static readonly HashSet<string> RequiredFiles =
        new(RequiredPublisherFiles, StringComparer.OrdinalIgnoreCase)
        {
            "driver/NonProxyWfp.cat",
            "driver/NonProxyWfp.inf",
            "driver/NonProxyWfp.sys",
            "release-metadata.json",
            "tools/install-system-components.ps1",
            "tools/NonProxy.Windows.AdapterHost.psm1",
            "tools/NonProxy.Windows.Common.psm1",
            "tools/NonProxy.Windows.DriverPackage.psm1",
            "tools/NonProxy.Windows.Service.psm1",
            "tools/verify-release-package.ps1",
        };
    private static readonly HashSet<string> ScriptExtensions =
        new(StringComparer.OrdinalIgnoreCase)
        {
            ".ps1",
            ".psd1",
            ".psm1",
        };
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        MaxDepth = 8,
        PropertyNameCaseInsensitive = false,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
    };

    public ValidatedReleasePackage Validate(string packageRoot)
    {
        if (!CompiledWindowsPublisherIdentity.IsCanonicalCertificateSha256(
                expectedPublisherCertificateSha256))
        {
            throw new InvalidOperationException("未编译固定的 Windows 发布者身份。");
        }
        var root = Path.GetFullPath(packageRoot);
        if (string.Equals(
            root,
            Path.GetPathRoot(root),
            StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("发布包根目录不能是磁盘根目录。");
        }
        root = root.TrimEnd(Path.DirectorySeparatorChar);
        AssertRegularPath(root);
        var trustPath = ResolveRequiredFile(root, "release-trust.ps1");
        var manifestPath = ResolveRequiredFile(root, "release-manifest.json");
        var trustSigner = RequireExpectedPublisher(trustPath);
        var expectedManifestHash = ReadTrustedManifestHash(trustPath);
        var actualManifestHash = HashBoundedFile(
            manifestPath,
            MaximumManifestBytes,
            out _);
        if (actualManifestHash != expectedManifestHash)
        {
            throw new CryptographicException("发布清单与签名信任文件不匹配。");
        }
        var manifest = ReadManifest(manifestPath);
        ValidateManifestHeader(manifest, trustSigner);
        var expectedFiles = ValidateFiles(root, manifest);
        ValidateNoExtraFiles(root, expectedFiles, manifestPath, trustPath);
        VerifyDriverCatalog(root);
        return new ValidatedReleasePackage(
            root,
            manifest.Version!,
            manifest.Architecture!,
            actualManifestHash,
            trustSigner.ThumbprintSha1,
            trustSigner.CertificateSha256);
    }

    private WindowsSignerCertificate RequireExpectedPublisher(string path)
    {
        var signer = trustVerifier.VerifyAuthenticode(path);
        if (signer is null
            || signer.CertificateSha256 != expectedPublisherCertificateSha256)
        {
            throw new CryptographicException("文件发布者不是编译固定的受信证书。");
        }
        return signer;
    }

    private static string ReadTrustedManifestHash(string trustPath)
    {
        var text = ReadBoundedText(trustPath, 2 * 1024 * 1024);
        var markerIndex = text.IndexOf(SignatureMarker, StringComparison.Ordinal);
        if (markerIndex < 0)
        {
            throw new InvalidDataException("发布信任文件缺少签名块。");
        }
        var unsigned = text[..markerIndex].Trim();
        const string prefix = "$NonProxyReleaseManifestSha256 = '";
        if (!unsigned.StartsWith(prefix, StringComparison.Ordinal)
            || !unsigned.EndsWith('\'')
            || unsigned.Length != prefix.Length + 64 + 1)
        {
            throw new InvalidDataException("发布信任文件格式无效。");
        }
        var hash = unsigned.Substring(prefix.Length, 64);
        if (!CompiledWindowsPublisherIdentity.IsCanonicalCertificateSha256(hash))
        {
            throw new InvalidDataException("发布清单哈希格式无效。");
        }
        return hash;
    }

    private static ReleaseManifest ReadManifest(string path)
    {
        var text = ReadBoundedText(path, MaximumManifestBytes);
        return JsonSerializer.Deserialize<ReleaseManifest>(text, JsonOptions)
            ?? throw new InvalidDataException("发布清单为空。");
    }

    private void ValidateManifestHeader(
        ReleaseManifest manifest,
        WindowsSignerCertificate trustSigner)
    {
        if (manifest.SchemaVersion != 1
            || manifest.Product != "NonProxy"
            || string.IsNullOrWhiteSpace(manifest.Version)
            || manifest.MinimumWindowsBuild < 18362
            || manifest.PublisherCertificateSha256
                != expectedPublisherCertificateSha256
            || !string.Equals(
                manifest.PublisherThumbprintHint,
                trustSigner.ThumbprintSha1,
                StringComparison.OrdinalIgnoreCase)
            || !DateTimeOffset.TryParse(
                manifest.SignedUtc,
                System.Globalization.CultureInfo.InvariantCulture,
                System.Globalization.DateTimeStyles.RoundtripKind,
                out _)
            || manifest.Files is not
            { Length: > 0 and <= MaximumPackageEntries })
        {
            throw new InvalidDataException("发布清单头不受支持。");
        }
        var architecture = RuntimeInformation.OSArchitecture switch
        {
            Architecture.X64 => "x64",
            Architecture.Arm64 => "arm64",
            _ => throw new PlatformNotSupportedException("Windows 原生架构不受支持。"),
        };
        if (manifest.Architecture != architecture)
        {
            throw new InvalidDataException("发布包架构与当前系统不匹配。");
        }
        if (OperatingSystem.IsWindows()
            && Environment.OSVersion.Version.Build < manifest.MinimumWindowsBuild)
        {
            throw new PlatformNotSupportedException("Windows Build 低于发布包要求。");
        }
    }

    private HashSet<string> ValidateFiles(
        string root,
        ReleaseManifest manifest)
    {
        var expected = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        long totalBytes = 0;
        foreach (var entry in manifest.Files!)
        {
            var relative = NormalizeRelativePath(entry.Path);
            if (!expected.Add(relative)
                || entry.Size is <= 0 or > MaximumFileBytes
                || !CompiledWindowsPublisherIdentity.IsCanonicalCertificateSha256(
                    entry.Sha256))
            {
                throw new InvalidDataException($"发布文件条目无效：{relative}");
            }
            totalBytes = checked(totalBytes + entry.Size);
            if (totalBytes > MaximumPackageBytes)
            {
                throw new InvalidDataException("发布包总大小超出限制。");
            }
            var path = ResolveRequiredFile(root, relative);
            var actualHash = HashBoundedFile(path, MaximumFileBytes, out var size);
            if (size != entry.Size || actualHash != entry.Sha256)
            {
                throw new CryptographicException($"发布文件内容不匹配：{relative}");
            }
            if (RequiredPublisherFiles.Contains(relative)
                || ScriptExtensions.Contains(Path.GetExtension(relative)))
            {
                _ = RequireExpectedPublisher(path);
            }
        }
        if (!RequiredFiles.IsSubsetOf(expected))
        {
            throw new InvalidDataException("发布包缺少消费安装所需文件。");
        }
        return expected;
    }

    private static void ValidateNoExtraFiles(
        string root,
        HashSet<string> expected,
        string manifestPath,
        string trustPath)
    {
        foreach (var file in EnumeratePackageFiles(root))
        {
            AssertRegularPathChain(root, file);
            if (string.Equals(file, manifestPath, StringComparison.OrdinalIgnoreCase)
                || string.Equals(file, trustPath, StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }
            var relative = Path.GetRelativePath(root, file).Replace('\\', '/');
            if (!expected.Contains(relative))
            {
                throw new InvalidDataException($"发布包包含清单外文件：{relative}");
            }
        }
    }

    private static IEnumerable<string> EnumeratePackageFiles(string root)
    {
        var pending = new Stack<string>();
        pending.Push(root);
        var entries = 0;
        while (pending.Count > 0)
        {
            var directory = pending.Pop();
            foreach (var entry in Directory.EnumerateFileSystemEntries(directory))
            {
                if (++entries > MaximumPackageEntries + 2)
                {
                    throw new InvalidDataException("发布包文件数量超出限制。");
                }
                var attributes = File.GetAttributes(entry);
                if ((attributes & FileAttributes.ReparsePoint) != 0)
                {
                    throw new InvalidDataException("发布包不允许重解析点。");
                }
                if ((attributes & FileAttributes.Directory) != 0)
                {
                    pending.Push(entry);
                }
                else
                {
                    yield return entry;
                }
            }
        }
    }

    private void VerifyDriverCatalog(string root)
    {
        var catalog = ResolveRequiredFile(root, "driver/NonProxyWfp.cat");
        trustVerifier.VerifyCatalogMember(
            catalog,
            ResolveRequiredFile(root, "driver/NonProxyWfp.inf"));
        trustVerifier.VerifyCatalogMember(
            catalog,
            ResolveRequiredFile(root, "driver/NonProxyWfp.sys"));
    }

    private static string NormalizeRelativePath(string? path)
    {
        if (string.IsNullOrWhiteSpace(path)
            || path.Length > 512
            || path.Contains('\\')
            || path.StartsWith('/')
            || path.Contains(':'))
        {
            throw new InvalidDataException("发布清单包含无效相对路径。");
        }
        var segments = path.Split('/');
        if (segments.Any(segment => segment is "" or "." or ".."))
        {
            throw new InvalidDataException("发布清单路径包含无效分段。");
        }
        return path;
    }

    private static string ResolveRequiredFile(string root, string relative)
    {
        var normalized = NormalizeRelativePath(relative);
        var path = Path.GetFullPath(Path.Combine(
            root,
            normalized.Replace('/', Path.DirectorySeparatorChar)));
        var prefix = root + Path.DirectorySeparatorChar;
        if (!path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)
            || !File.Exists(path))
        {
            throw new FileNotFoundException("发布包缺少文件。", normalized);
        }
        AssertRegularPathChain(root, path);
        return path;
    }

    private static void AssertRegularPathChain(string root, string path)
    {
        var current = Path.GetFullPath(path);
        while (true)
        {
            AssertRegularPath(current);
            if (string.Equals(current, root, StringComparison.OrdinalIgnoreCase))
            {
                return;
            }
            current = Path.GetDirectoryName(current)
                ?? throw new InvalidDataException("发布文件逃逸包根目录。");
            if (!current.StartsWith(root, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidDataException("发布文件逃逸包根目录。");
            }
        }
    }

    private static void AssertRegularPath(string path)
    {
        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
        {
            throw new InvalidDataException("发布包不允许重解析点。");
        }
    }

    private static string HashBoundedFile(
        string path,
        long maximumBytes,
        out long size)
    {
        using var stream = new FileStream(
            path,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read);
        size = stream.Length;
        if (size is <= 0 || size > maximumBytes)
        {
            throw new InvalidDataException("发布文件大小超出限制。");
        }
        return Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant();
    }

    private static string ReadBoundedText(string path, long maximumBytes)
    {
        using var stream = new FileStream(
            path,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read);
        if (stream.Length is <= 0 || stream.Length > maximumBytes)
        {
            throw new InvalidDataException("发布文本大小超出限制。");
        }
        using var reader = new StreamReader(
            stream,
            detectEncodingFromByteOrderMarks: true);
        return reader.ReadToEnd();
    }
}
