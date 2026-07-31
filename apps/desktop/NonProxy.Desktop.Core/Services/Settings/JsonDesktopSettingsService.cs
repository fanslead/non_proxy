using System.Text.Json;

namespace NonProxy.Desktop.Core.Services.Settings;

public sealed class JsonDesktopSettingsService : IDesktopSettingsService, IDisposable
{
    private const long MaximumSettingsBytes = 64 * 1024;
    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true,
    };

    private readonly DesktopSettingsPath _path;
    private readonly SemaphoreSlim _gate = new(1, 1);

    public JsonDesktopSettingsService(DesktopSettingsPath path)
    {
        _path = path;
    }

    public async Task<DesktopSettings> GetAsync(CancellationToken cancellationToken)
    {
        await _gate.WaitAsync(cancellationToken);
        try
        {
            if (!File.Exists(_path.FilePath))
            {
                return DesktopSettings.Defaults;
            }

            if (new FileInfo(_path.FilePath).Length > MaximumSettingsBytes)
            {
                return DesktopSettings.Defaults;
            }

            await using var stream = new FileStream(
                _path.FilePath,
                FileMode.Open,
                FileAccess.Read,
                FileShare.Read,
                bufferSize: 4096,
                FileOptions.Asynchronous | FileOptions.SequentialScan);
            var settings = await JsonSerializer.DeserializeAsync<DesktopSettings>(
                stream,
                SerializerOptions,
                cancellationToken);
            return Normalize(settings);
        }
        catch (JsonException)
        {
            return DesktopSettings.Defaults;
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task SaveAsync(
        DesktopSettings settings,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(settings);
        var normalized = Normalize(settings);
        await _gate.WaitAsync(cancellationToken);
        try
        {
            var directory = Path.GetDirectoryName(_path.FilePath)
                ?? throw new InvalidOperationException("设置文件缺少父目录。");
            Directory.CreateDirectory(directory);
            var temporaryPath = $"{_path.FilePath}.{Guid.NewGuid():N}.tmp";
            try
            {
                await using (var stream = new FileStream(
                    temporaryPath,
                    FileMode.CreateNew,
                    FileAccess.Write,
                    FileShare.None,
                    bufferSize: 4096,
                    FileOptions.Asynchronous | FileOptions.WriteThrough))
                {
                    await JsonSerializer.SerializeAsync(
                        stream,
                        normalized,
                        SerializerOptions,
                        cancellationToken);
                    await stream.FlushAsync(cancellationToken);
                }

                File.Move(temporaryPath, _path.FilePath, overwrite: true);
            }
            finally
            {
                if (File.Exists(temporaryPath))
                {
                    File.Delete(temporaryPath);
                }
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    public void Dispose()
    {
        _gate.Dispose();
    }

    internal static DesktopSettings Normalize(DesktopSettings? settings)
    {
        return settings?.Theme switch
        {
            "Light" => new DesktopSettings("Light"),
            "Dark" => new DesktopSettings("Dark"),
            _ => DesktopSettings.Defaults,
        };
    }
}
