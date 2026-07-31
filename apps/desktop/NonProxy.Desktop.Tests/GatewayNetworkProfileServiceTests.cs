using NonProxy.Common.V1;
using NonProxy.Control.V1;
using NonProxy.Desktop.Core.Platform;
using NonProxy.Desktop.Core.Services.Control;
using NonProxy.Desktop.Core.Services.Control.Gateway;
using NonProxy.Policy.V1;
using PlatformFingerprintKind = NonProxy.Desktop.Core.Platform.NetworkFingerprintKind;

namespace NonProxy.Desktop.Tests;

public sealed class GatewayNetworkProfileServiceTests
{
    private const string Fingerprint =
        "95e986531d4972a782f3a2a868cbecb194a0e0fc14f95280706077e9fbf63fc5";

    [Fact]
    public async Task SaveNewProfileUsesRevisionOneAndReturnsSafeSummary()
    {
        var client = new StubControlRpcClient
        {
            UpsertNetworkProfileResponse = new UpsertNetworkProfileResponse
            {
                Result = new NetworkProfileMutationResult
                {
                    Profile = Profile("office", "办公室", 1),
                },
            },
        };
        var service = new GatewayNetworkProfileService(client);

        var result = await service.SaveAsync(
            new NetworkProfileDraft(
                null,
                "办公室",
                PlatformFingerprintKind.WiFiSsidSha256,
                Fingerprint),
            TestContext.Current.CancellationToken);

        Assert.True(result.Accepted);
        Assert.Equal(0UL, client.LastExpectedRevision);
        Assert.Equal(1UL, client.LastUpsertedNetworkProfile?.Revision);
        Assert.Equal(NonProxy.Policy.V1.NetworkFingerprintKind.WifiSsidSha256,
            client.LastUpsertedNetworkProfile?.FingerprintKind);
        Assert.Equal("office", result.Profile?.Id);
        Assert.DoesNotContain("Office WiFi", result.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task EditProfileRequiresAndIncrementsRevision()
    {
        var client = new StubControlRpcClient
        {
            UpsertNetworkProfileResponse = new UpsertNetworkProfileResponse
            {
                Result = new NetworkProfileMutationResult
                {
                    Profile = Profile("office", "新办公室", 8),
                },
            },
        };
        var service = new GatewayNetworkProfileService(client);

        await service.SaveAsync(
            new NetworkProfileDraft(
                "office",
                "新办公室",
                PlatformFingerprintKind.WiFiSsidSha256,
                Fingerprint,
                7),
            TestContext.Current.CancellationToken);

        Assert.Equal(7UL, client.LastExpectedRevision);
        Assert.Equal(8UL, client.LastUpsertedNetworkProfile?.Revision);
        Assert.Equal("office", client.LastUpsertedNetworkProfile?.Id);
    }

    [Fact]
    public async Task CatalogRetriesWhenGenerationChangesBetweenPages()
    {
        var client = new StubControlRpcClient();
        client.NetworkProfilesResponses.Enqueue(new ListNetworkProfilesResponse
        {
            CatalogGeneration = 1,
            Page = new PageResponse { NextPageToken = "next" },
            Profiles = { Profile("old", "旧网络", 1) },
        });
        client.NetworkProfilesResponses.Enqueue(new ListNetworkProfilesResponse
        {
            CatalogGeneration = 2,
            Page = new PageResponse(),
        });
        client.NetworkProfilesResponses.Enqueue(new ListNetworkProfilesResponse
        {
            CatalogGeneration = 2,
            Page = new PageResponse(),
            Profiles = { Profile("current", "当前网络", 1) },
        });
        var service = new GatewayNetworkProfileService(client);

        var catalog = await service.GetCatalogAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal("current", Assert.Single(catalog.Items).Id);
        Assert.Equal(2UL, catalog.CatalogGeneration);
        Assert.Equal(3, client.ListNetworkProfilesCallCount);
    }

    [Fact]
    public async Task CatalogRejectsDuplicateProfileAcrossPages()
    {
        var client = new StubControlRpcClient();
        client.NetworkProfilesResponses.Enqueue(new ListNetworkProfilesResponse
        {
            CatalogGeneration = 1,
            Page = new PageResponse { NextPageToken = "next" },
            Profiles = { Profile("office", "办公室", 1) },
        });
        client.NetworkProfilesResponses.Enqueue(new ListNetworkProfilesResponse
        {
            CatalogGeneration = 1,
            Page = new PageResponse(),
            Profiles = { Profile("office", "办公室", 1) },
        });
        var service = new GatewayNetworkProfileService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.GetCatalogAsync(TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }

    [Fact]
    public async Task CatalogRejectsOneFingerprintBoundToTwoProfiles()
    {
        var client = new StubControlRpcClient
        {
            NetworkProfilesResponse = new ListNetworkProfilesResponse
            {
                CatalogGeneration = 1,
                Page = new PageResponse(),
                Profiles =
                {
                    Profile("office-a", "办公室 A", 1),
                    Profile("office-b", "办公室 B", 1),
                },
            },
        };
        var service = new GatewayNetworkProfileService(client);

        var error = await Assert.ThrowsAsync<ControlServiceException>(() =>
            service.GetCatalogAsync(TestContext.Current.CancellationToken));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }

    [Fact]
    public async Task ReferencedProfileDeletionReturnsActionableMessage()
    {
        var client = new StubControlRpcClient
        {
            DeleteNetworkProfileResponse = new DeleteNetworkProfileResponse
            {
                Result = new NetworkProfileMutationResult
                {
                    Error = new ErrorDetail
                    {
                        Code = "NP_STORAGE_NETWORK_PROFILE_IN_USE",
                    },
                },
            },
        };
        var service = new GatewayNetworkProfileService(client);

        var result = await service.DeleteAsync(
            "office",
            3,
            TestContext.Current.CancellationToken);

        Assert.False(result.Accepted);
        Assert.Contains("先删除对应规则", result.Message, StringComparison.Ordinal);
        Assert.Equal("office", client.LastDeletedNetworkProfileId);
        Assert.Equal(3UL, client.LastExpectedRevision);
    }

    [Fact]
    public void RawWifiNameIsRejectedBeforeRpcMutation()
    {
        var error = Assert.Throws<ControlServiceException>(() =>
            NetworkProfileContractMapper.ToContract(new NetworkProfileDraft(
                null,
                "办公室",
                PlatformFingerprintKind.WiFiSsidSha256,
                "Office WiFi")));

        Assert.Equal("NP_CONTROL_CONTRACT_INVALID", error.Code);
    }

    private static NetworkProfileSpec Profile(
        string id,
        string name,
        ulong revision)
    {
        return new NetworkProfileSpec
        {
            Id = id,
            DisplayName = name,
            FingerprintKind = NonProxy.Policy.V1.NetworkFingerprintKind.WifiSsidSha256,
            FingerprintValue = Fingerprint,
            Revision = revision,
        };
    }
}
