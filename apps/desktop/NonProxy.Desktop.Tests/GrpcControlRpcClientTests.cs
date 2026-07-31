using Google.Protobuf;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Services.Control.Events;
using NonProxy.Desktop.Core.Services.Control.Rpc;
using NonProxy.Events.V1;

namespace NonProxy.Desktop.Tests;

public sealed class GrpcControlRpcClientTests
{
    [Fact]
    public void EventMapperExposesOnlySequenceAndRefreshKind()
    {
        var events = new (EventEnvelope Envelope, ControlEventKind Expected)[]
        {
            (new EventEnvelope
            {
                Sequence = 1,
                SystemStateChanged = new SystemStateChanged(),
            }, ControlEventKind.SystemState),
            (new EventEnvelope
            {
                Sequence = 2,
                SnapshotStateChanged = new SnapshotStateChanged(),
            }, ControlEventKind.Snapshot),
            (new EventEnvelope
            {
                Sequence = 3,
                DecisionObserved = new DecisionObserved(),
            }, ControlEventKind.Decision),
            (new EventEnvelope
            {
                Sequence = 4,
                ComponentHealthChanged = new ComponentHealthChanged(),
            }, ControlEventKind.ComponentHealth),
            (new EventEnvelope
            {
                Sequence = 5,
                LearningCandidateUpdated = new LearningCandidateUpdated(),
            }, ControlEventKind.LearningCandidate),
            (new EventEnvelope { Sequence = 6 }, ControlEventKind.Unknown),
        };

        foreach (var (envelope, expected) in events)
        {
            Assert.Equal(
                new ControlEventNotification(envelope.Sequence, expected),
                GrpcControlRpcClient.MapEvent(envelope));
        }
    }

    [Fact]
    public void TestOutboundBuildsAuthenticatedBoundedRequest()
    {
        var context = new OperationContext
        {
            OperationId = "desktop:test-outbound:operation",
            SessionCapabilityToken = ByteString.CopyFrom(Enumerable.Range(0, 32)
                .Select(value => (byte)value)
                .ToArray()),
        };

        var request = GrpcControlRpcClient.CreateTestOutboundRequest(
            "office",
            context);

        Assert.Same(context, request.Context);
        Assert.Equal("office", request.OutboundId);
        Assert.NotNull(request.Timeout);
        Assert.Equal(5, request.Timeout.Seconds);
        Assert.Equal(0, request.Timeout.Nanos);
        Assert.Equal(32, request.Context.SessionCapabilityToken.Length);
    }

    [Theory]
    [InlineData(null, ExitProbeRouteKind.Direct, "")]
    [InlineData("office", ExitProbeRouteKind.Proxy, "office")]
    public void VerifyExitBuildsAuthenticatedFixedTargetRequest(
        string? outboundId,
        ExitProbeRouteKind expectedRoute,
        string expectedOutboundId)
    {
        var context = new OperationContext
        {
            OperationId = "desktop:verify-exit:operation",
            SessionCapabilityToken = ByteString.CopyFrom(new byte[32]),
        };

        var request = GrpcControlRpcClient.CreateVerifyExitRequest(
            outboundId,
            context);

        Assert.Same(context, request.Context);
        Assert.Equal(expectedRoute, request.Route);
        Assert.Equal(expectedOutboundId, request.OutboundId);
        Assert.Equal(10, request.Timeout.Seconds);
        Assert.Equal(0, request.Timeout.Nanos);
    }

    [Fact]
    public void VerifyExitDoesNotInterpretWhitespaceAsDirectRoute()
    {
        var context = new OperationContext
        {
            OperationId = "desktop:verify-exit:operation",
            SessionCapabilityToken = ByteString.CopyFrom(new byte[32]),
        };

        Assert.Throws<ArgumentException>(() =>
            GrpcControlRpcClient.CreateVerifyExitRequest(" ", context));
    }

    [Fact]
    public void SetDefaultRouteBuildsAuthenticatedOptimisticRequest()
    {
        var context = new OperationContext
        {
            OperationId = "desktop:set-default-route:operation",
            SessionCapabilityToken = ByteString.CopyFrom(new byte[32]),
        };

        var request = GrpcControlRpcClient.CreateSetDefaultRouteRequest(
            "office",
            7,
            context);

        Assert.Same(context, request.Context);
        Assert.Equal("office", request.OutboundId);
        Assert.Equal<ulong>(7, request.ExpectedRoutingRevision);
        Assert.Equal(
            SetDefaultRouteRequest.RouteOneofCase.OutboundId,
            request.RouteCase);
    }

    [Fact]
    public void SetDirectRouteUsesExplicitTrueOneOf()
    {
        var context = new OperationContext
        {
            OperationId = "desktop:set-direct-route:operation",
            SessionCapabilityToken = ByteString.CopyFrom(new byte[32]),
        };

        var request = GrpcControlRpcClient.CreateSetDirectRouteRequest(
            8,
            context);

        Assert.Same(context, request.Context);
        Assert.True(request.Direct);
        Assert.Equal<ulong>(8, request.ExpectedRoutingRevision);
        Assert.Equal(
            SetDefaultRouteRequest.RouteOneofCase.Direct,
            request.RouteCase);
    }
}
