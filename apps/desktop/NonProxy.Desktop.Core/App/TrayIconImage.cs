using System.Buffers.Binary;
using System.IO.Compression;

namespace NonProxy.Desktop.Core.Bootstrap;

internal static class TrayIconImage
{
    private const int Size = 32;

    public static Avalonia.Controls.WindowIcon Create()
    {
        using var output = new MemoryStream();
        output.Write([137, 80, 78, 71, 13, 10, 26, 10]);

        Span<byte> header = stackalloc byte[13];
        BinaryPrimitives.WriteUInt32BigEndian(header, Size);
        BinaryPrimitives.WriteUInt32BigEndian(header[4..], Size);
        header[8] = 8;
        header[9] = 6;
        WriteChunk(output, "IHDR"u8, header);

        using var pixels = new MemoryStream();
        using (var compressor = new ZLibStream(
            pixels,
            CompressionLevel.SmallestSize,
            leaveOpen: true))
        {
            for (var y = 0; y < Size; y++)
            {
                compressor.WriteByte(0);
                for (var x = 0; x < Size; x++)
                {
                    WritePixel(compressor, x, y);
                }
            }
        }

        WriteChunk(output, "IDAT"u8, pixels.ToArray());
        WriteChunk(output, "IEND"u8, ReadOnlySpan<byte>.Empty);
        output.Position = 0;
        return new Avalonia.Controls.WindowIcon(output);
    }

    private static void WritePixel(Stream output, int x, int y)
    {
        var distance = Math.Sqrt(
            Math.Pow(x - 15.5, 2) + Math.Pow(y - 15.5, 2));
        if (distance > 14.5)
        {
            output.Write([0, 0, 0, 0]);
            return;
        }

        var left = x is >= 8 and <= 11 && y is >= 7 and <= 24;
        var right = x is >= 20 and <= 23 && y is >= 7 and <= 24;
        var diagonalCenter = 9 + ((y - 7) * 13d / 17d);
        var diagonal = y is >= 7 and <= 24
            && Math.Abs(x - diagonalCenter) <= 1.7;
        if (left || right || diagonal)
        {
            output.Write([255, 255, 255, 255]);
            return;
        }

        output.Write([20, 98, 87, 255]);
    }

    private static void WriteChunk(
        Stream output,
        ReadOnlySpan<byte> type,
        ReadOnlySpan<byte> payload)
    {
        Span<byte> number = stackalloc byte[4];
        BinaryPrimitives.WriteUInt32BigEndian(number, (uint)payload.Length);
        output.Write(number);
        output.Write(type);
        output.Write(payload);

        var crc = uint.MaxValue;
        crc = UpdateCrc32(crc, type);
        crc = UpdateCrc32(crc, payload);
        BinaryPrimitives.WriteUInt32BigEndian(number, ~crc);
        output.Write(number);
    }

    private static uint UpdateCrc32(uint crc, ReadOnlySpan<byte> bytes)
    {
        foreach (var value in bytes)
        {
            crc ^= value;
            for (var bit = 0; bit < 8; bit++)
            {
                crc = (crc >> 1) ^ (0xEDB88320u & (uint)-(int)(crc & 1));
            }
        }

        return crc;
    }
}
