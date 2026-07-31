using NonProxy.Control.V1;

namespace NonProxy.Desktop.Core.Services.Control.Rpc;

public sealed partial class GrpcControlRpcClient
{
    private static readonly TimeSpan DiagnosticsTimeout = TimeSpan.FromSeconds(30);

    public async Task<ExportDiagnosticsResponse> ExportDiagnosticsAsync(
        CancellationToken cancellationToken)
    {
        var context = await _contextProvider.CreateAsync(
            "export-diagnostics",
            cancellationToken);
        return await ExecuteAsync(
            () => Client.ExportDiagnosticsAsync(
                new ExportDiagnosticsRequest
                {
                    Context = context,
                    RedactionLevel = DiagnosticRedactionLevel.Strict,
                },
                Options(DiagnosticsTimeout, cancellationToken)).ResponseAsync);
    }
}
