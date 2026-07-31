using System.Security.Cryptography;
using Google.Protobuf.WellKnownTypes;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using NonProxy.Desktop.Core.Services.Control.Transport;
using NonProxy.Events.V1;

namespace NonProxy.ControlSmoke;

internal static class Program
{
    public static async Task<int> Main()
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(20));
        try
        {
            var endpoint = LocalControlEndpoint.FromUnixEnvironment();
            var channelFactory = new UnixDomainSocketControlChannelFactory(endpoint);
            var capabilityProvider = new FileSessionCapabilityProvider(endpoint);
            var contextProvider = new OperationContextProvider(capabilityProvider);
            using var client = new GrpcControlRpcClient(
                channelFactory,
                contextProvider);
            var policies = new GatewayPolicyService(
                client,
                new PolicyContractMapper(new SmokePlatformInformation()));
            await ImportSmokeOutboundIfRequestedAsync(client, timeout.Token);
            await VerifyLearningAsync(client, timeout.Token);
            await VerifyDiagnosticsAsync(client, timeout.Token);
            var initial = await client.GetSystemStatusAsync(timeout.Token);
            if (initial.ActiveSnapshotVersion != 0
                || initial.PendingSnapshotVersion != 0)
            {
                throw new InvalidOperationException("隔离测试状态目录不是空状态。");
            }

            var saved = await policies.SaveAsync(
                new PolicyDraft(
                    null,
                    "跨语言联调规则",
                    PolicyScope.Website,
                    "smoke.nonproxy.test",
                    PolicyAction.Direct),
                timeout.Token);
            if (!saved.Accepted || saved.Applied || saved.SnapshotVersion != 1)
            {
                throw new InvalidOperationException("规则发布状态不符合待确认语义。");
            }

            var catalog = await policies.GetCatalogAsync(timeout.Token);
            var item = catalog.Items.SingleOrDefault(policy =>
                policy.MatchValue == "smoke.nonproxy.test");
            if (item?.State != PolicyApplyState.Pending
                || catalog.PendingSnapshotVersion != 1)
            {
                throw new InvalidOperationException("跨语言规则状态回读不一致。");
            }

            Console.WriteLine(
                "控制平面跨语言联调通过：UDS、会话认证、写入、发布、学习会话、严格脱敏诊断和状态回读一致。");
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine($"控制平面跨语言联调失败：{exception.Message}");
            return 1;
        }
    }

    private static async Task ImportSmokeOutboundIfRequestedAsync(
        IControlRpcClient client,
        CancellationToken cancellationToken)
    {
        var rawPort = Environment.GetEnvironmentVariable(
            "NONPROXY_SMOKE_PROXY_PORT");
        if (string.IsNullOrWhiteSpace(rawPort))
        {
            return;
        }
        if (!ushort.TryParse(rawPort, out var port) || port == 0)
        {
            throw new InvalidOperationException("代理联调端口无效。");
        }

        var service = new GatewayOutboundService(client);
        var imported = await service.ImportAsync(
            new OutboundImportDraft(
                "smoke-http",
                OutboundProxyKind.HttpConnect,
                "127.0.0.1",
                port,
                null,
                null),
            cancellationToken);
        if (imported.Outbounds.SingleOrDefault()?.Id != "smoke-http")
        {
            throw new InvalidOperationException("代理联调出口导入结果不完整。");
        }
    }

    private static async Task VerifyLearningAsync(
        GrpcControlRpcClient client,
        CancellationToken cancellationToken)
    {
        var started = await client.StartLearningSessionAsync(
            new StartLearningSessionRequest
            {
                Kind = LearningSessionKind.Site,
                Duration = Duration.FromTimeSpan(TimeSpan.FromSeconds(60)),
                BrowserContextId = "smoke-browser-context",
                NormalizedSite = "nonproxy.test",
            },
            cancellationToken);
        if (started.Error is not null || string.IsNullOrWhiteSpace(started.SessionId))
        {
            throw new InvalidOperationException(
                $"学习会话启动失败：{started.Error?.Code ?? "missing-session"}");
        }

        var recorded = await client.RecordLearningObservationAsync(
            new RecordLearningObservationRequest
            {
                SessionId = started.SessionId,
                ObservationId = "smoke-observation",
                BrowserContextId = "smoke-browser-context",
                Kind = LearningObservationKind.Subresource,
                NormalizedDomain = "api.nonproxy.test",
                InitiatorDomain = "nonproxy.test",
                ResourceType = LearningResourceType.Fetch,
            },
            cancellationToken);
        if (recorded.Error is not null
            || recorded.Duplicate
            || recorded.Candidate?.Kind != LearningCandidateKind.RequiredFirstParty)
        {
            throw new InvalidOperationException(
                $"学习观测聚合失败：{recorded.Error?.Code ?? "invalid-candidate"}");
        }

        var candidates = await client.ListLearningCandidatesAsync(
            started.SessionId,
            cancellationToken);
        if (candidates.Error is not null
            || candidates.Session?.BrowserContextId != "smoke-browser-context"
            || candidates.Candidates.Count != 1)
        {
            throw new InvalidOperationException(
                $"学习候选查询失败：{candidates.Error?.Code ?? "invalid-list"}");
        }

        var stopped = await client.StopLearningSessionAsync(
            started.SessionId,
            cancellationToken);
        if (stopped.Error is not null
            || stopped.CandidateCount != 1
            || stopped.Session?.State != LearningSessionState.Stopped)
        {
            throw new InvalidOperationException(
                $"学习会话停止失败：{stopped.Error?.Code ?? "invalid-stop"}");
        }
    }

    private static async Task VerifyDiagnosticsAsync(
        GrpcControlRpcClient client,
        CancellationToken cancellationToken)
    {
        var response = await client.ExportDiagnosticsAsync(cancellationToken);
        if (response.Error is not null
            || response.AppliedRedactionLevel != DiagnosticRedactionLevel.Strict
            || response.ConnectionSampleCount != 0
            || response.Sha256.Length != 32
            || string.IsNullOrWhiteSpace(response.LocalPath)
            || response.IncludedSections.Count == 0)
        {
            throw new InvalidOperationException(
                $"严格脱敏诊断导出失败：{response.Error?.Code ?? "invalid-result"}");
        }
        var content = await File.ReadAllBytesAsync(
            response.LocalPath,
            cancellationToken);
        if (!CryptographicOperations.FixedTimeEquals(
                SHA256.HashData(content),
                response.Sha256.Span))
        {
            throw new InvalidOperationException("诊断文件 SHA-256 与控制响应不一致。");
        }
        var text = System.Text.Encoding.UTF8.GetString(content);
        if (text.Contains(
                "smoke-browser-context",
                StringComparison.Ordinal)
            || text.Contains("api.nonproxy.test", StringComparison.Ordinal))
        {
            throw new InvalidOperationException("严格诊断文件泄漏了学习会话或目标域名。");
        }
    }

    private sealed class SmokePlatformInformation : IPlatformInformation
    {
        public PlatformKind Platform => PlatformKind.MacOS;

        public string DisplayName => "联调平台";
    }
}
