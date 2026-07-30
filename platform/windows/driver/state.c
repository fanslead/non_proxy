#include "nonproxy_wfp_driver.h"

VOID
NonProxyInitializeState(
    _Out_ NP_WFP_DEVICE_EXTENSION* Extension)
{
    RtlZeroMemory(Extension, sizeof(*Extension));
    KeInitializeSpinLock(&Extension->StateLock);
    Extension->Config.Magic = NP_WFP_CONFIG_MAGIC;
    Extension->Config.Version = NP_WFP_CONFIG_VERSION;
    Extension->Config.Size = sizeof(NP_WFP_CONFIG_V2);
}

NTSTATUS
NonProxyApplyConfig(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _In_ const NP_WFP_CONFIG_V2* Config)
{
    KIRQL oldIrql;

    if (Config->Magic != NP_WFP_CONFIG_MAGIC ||
        Config->Version != NP_WFP_CONFIG_VERSION ||
        Config->Size != sizeof(NP_WFP_CONFIG_V2) ||
        (Config->Flags &
            ~(NP_WFP_CONFIG_FLAG_DNS_REDIRECT |
              NP_WFP_CONFIG_FLAG_TCP_REDIRECT)) != 0 ||
        Config->Reserved != 0) {
        return STATUS_REVISION_MISMATCH;
    }
    if ((Config->Flags == 0 && Config->ProxyProcessId != 0) ||
        (Config->Flags != 0 &&
         (Config->ProxyProcessId == 0 || Config->ProxyProcessId > MAXULONG))) {
        return STATUS_INVALID_PARAMETER;
    }
    if (((Config->Flags & NP_WFP_CONFIG_FLAG_DNS_REDIRECT) != 0 &&
         (Config->Ipv4DnsPortNetworkOrder == 0 ||
          Config->Ipv6DnsPortNetworkOrder == 0)) ||
        ((Config->Flags & NP_WFP_CONFIG_FLAG_DNS_REDIRECT) == 0 &&
         (Config->Ipv4DnsPortNetworkOrder != 0 ||
          Config->Ipv6DnsPortNetworkOrder != 0)) ||
        ((Config->Flags & NP_WFP_CONFIG_FLAG_TCP_REDIRECT) != 0 &&
         (Config->Ipv4ProxyPortNetworkOrder == 0 ||
          Config->Ipv6ProxyPortNetworkOrder == 0)) ||
        ((Config->Flags & NP_WFP_CONFIG_FLAG_TCP_REDIRECT) == 0 &&
         (Config->Ipv4ProxyPortNetworkOrder != 0 ||
          Config->Ipv6ProxyPortNetworkOrder != 0))) {
        return STATUS_INVALID_PARAMETER;
    }

    KeAcquireSpinLock(&Extension->StateLock, &oldIrql);
    if (Config->Generation < Extension->Config.Generation) {
        KeReleaseSpinLock(&Extension->StateLock, oldIrql);
        return STATUS_REVISION_MISMATCH;
    }
    Extension->Config = *Config;
    KeReleaseSpinLock(&Extension->StateLock, oldIrql);
    return STATUS_SUCCESS;
}

VOID
NonProxyDisableRedirect(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension)
{
    KIRQL oldIrql;

    KeAcquireSpinLock(&Extension->StateLock, &oldIrql);
    Extension->Config.Generation = 0;
    Extension->Config.Flags = 0;
    Extension->Config.ProxyProcessId = 0;
    Extension->Config.Ipv4ProxyPortNetworkOrder = 0;
    Extension->Config.Ipv6ProxyPortNetworkOrder = 0;
    Extension->Config.Ipv4DnsPortNetworkOrder = 0;
    Extension->Config.Ipv6DnsPortNetworkOrder = 0;
    Extension->Config.Reserved = 0;
    KeReleaseSpinLock(&Extension->StateLock, oldIrql);
}

NP_WFP_CONFIG_V2
NonProxyReadConfig(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension)
{
    KIRQL oldIrql;
    NP_WFP_CONFIG_V2 config;

    KeAcquireSpinLock(&Extension->StateLock, &oldIrql);
    config = Extension->Config;
    KeReleaseSpinLock(&Extension->StateLock, oldIrql);
    return config;
}

VOID
NonProxyReadStatus(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _Out_ NP_WFP_STATUS_V1* Status)
{
    NP_WFP_CONFIG_V2 config = NonProxyReadConfig(Extension);

    RtlZeroMemory(Status, sizeof(*Status));
    Status->Magic = NP_WFP_STATUS_MAGIC;
    Status->Version = NP_WFP_STATUS_VERSION;
    Status->Size = sizeof(*Status);
    Status->Generation = config.Generation;
    Status->ProxyProcessId = config.ProxyProcessId;
    Status->Flags = config.Flags;
    Status->ActiveClassifications =
        (UINT32)InterlockedCompareExchange(&Extension->ActiveClassifications, 0, 0);
    Status->RedirectedConnections =
        (UINT64)InterlockedCompareExchange64(&Extension->RedirectedConnections, 0, 0);
    Status->FailOpenConnections =
        (UINT64)InterlockedCompareExchange64(&Extension->FailOpenConnections, 0, 0);
}
