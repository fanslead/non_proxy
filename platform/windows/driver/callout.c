#include "nonproxy_wfp_driver.h"

static UINT32
NonProxyAppIdField(
    _In_ UINT16 LayerId)
{
    return LayerId == FWPS_LAYER_ALE_CONNECT_REDIRECT_V4
        ? FWPS_FIELD_ALE_CONNECT_REDIRECT_V4_ALE_APP_ID
        : FWPS_FIELD_ALE_CONNECT_REDIRECT_V6_ALE_APP_ID;
}

static UINT16
NonProxyProxyPort(
    _In_ const NP_WFP_CONFIG_V2* Config,
    _In_ UINT16 LayerId,
    _In_ UINT64 FilterContext)
{
    if (FilterContext == NP_WFP_FILTER_CONTEXT_DNS) {
        return LayerId == FWPS_LAYER_ALE_CONNECT_REDIRECT_V4
            ? Config->Ipv4DnsPortNetworkOrder
            : Config->Ipv6DnsPortNetworkOrder;
    }
    return LayerId == FWPS_LAYER_ALE_CONNECT_REDIRECT_V4
        ? Config->Ipv4ProxyPortNetworkOrder
        : Config->Ipv6ProxyPortNetworkOrder;
}

static VOID
NonProxySetLoopback(
    _Inout_ SOCKADDR_STORAGE* Remote,
    _In_ UINT16 LayerId,
    _In_ UINT16 Port)
{
    if (LayerId == FWPS_LAYER_ALE_CONNECT_REDIRECT_V4) {
        SOCKADDR_IN* address = (SOCKADDR_IN*)Remote;
        RtlZeroMemory(address, sizeof(*address));
        address->sin_family = AF_INET;
        address->sin_addr.S_un.S_addr = RtlUlongByteSwap(INADDR_LOOPBACK);
        address->sin_port = Port;
    } else {
        SOCKADDR_IN6* address = (SOCKADDR_IN6*)Remote;
        RtlZeroMemory(address, sizeof(*address));
        address->sin6_family = AF_INET6;
        address->sin6_addr.u.Byte[15] = 1;
        address->sin6_port = Port;
    }
}

static NP_WFP_REDIRECT_CONTEXT_V1*
NonProxyAllocateContext(
    _In_ const FWPS_INCOMING_VALUES0* IncomingValues,
    _In_ const FWPS_INCOMING_METADATA_VALUES0* Metadata,
    _In_ const FWPS_CONNECT_REQUEST0* Request,
    _Out_ SIZE_T* TotalSize)
{
    UINT32 field = NonProxyAppIdField(IncomingValues->layerId);
    const FWP_BYTE_BLOB* appId =
        IncomingValues->incomingValue[field].value.byteBlob;
    UINT32 appIdLength = appId == NULL ? 0 : appId->size;
    NP_WFP_REDIRECT_CONTEXT_V1* context;

    if (appIdLength > NP_WFP_MAX_APP_ID_BYTES) {
        return NULL;
    }
    *TotalSize = FIELD_OFFSET(NP_WFP_REDIRECT_CONTEXT_V1, AppId) + appIdLength;
    context = ExAllocatePool2(
        POOL_FLAG_NON_PAGED,
        *TotalSize,
        NP_WFP_POOL_TAG);
    if (context == NULL) {
        return NULL;
    }

    RtlZeroMemory(context, *TotalSize);
    context->Magic = NP_WFP_CONTEXT_MAGIC;
    context->Version = NP_WFP_CONTEXT_VERSION;
    context->HeaderSize = FIELD_OFFSET(NP_WFP_REDIRECT_CONTEXT_V1, AppId);
    context->TotalSize = (UINT32)*TotalSize;
    context->ProcessId =
        FWPS_IS_METADATA_FIELD_PRESENT(Metadata, FWPS_METADATA_FIELD_PROCESS_ID)
            ? Metadata->processId
            : 0;
    context->OriginalLocal = Request->localAddressAndPort;
    context->OriginalRemote = Request->remoteAddressAndPort;
    context->AppIdLength = appIdLength;
    if (appIdLength != 0 && appId->data != NULL) {
        RtlCopyMemory(context->AppId, appId->data, appIdLength);
    }
    return context;
}

static BOOLEAN
NonProxyShouldRedirect(
    _In_ const FWPS_INCOMING_METADATA_VALUES0* Metadata,
    _In_ const NP_WFP_CONFIG_V2* Config,
    _In_ UINT64 FilterContext)
{
    FWPS_CONNECTION_REDIRECT_STATE state;
    UINT32 requiredFlag;

    if (FilterContext == NP_WFP_FILTER_CONTEXT_DNS) {
        requiredFlag = NP_WFP_CONFIG_FLAG_DNS_REDIRECT;
    } else if (FilterContext == NP_WFP_FILTER_CONTEXT_TCP) {
        requiredFlag = NP_WFP_CONFIG_FLAG_TCP_REDIRECT;
    } else {
        return FALSE;
    }

    if ((Config->Flags & requiredFlag) == 0 ||
        !FWPS_IS_METADATA_FIELD_PRESENT(Metadata, FWPS_METADATA_FIELD_PROCESS_ID) ||
        Metadata->processId == Config->ProxyProcessId) {
        return FALSE;
    }
    if (!FWPS_IS_METADATA_FIELD_PRESENT(
            Metadata,
            FWPS_METADATA_FIELD_REDIRECT_RECORD_HANDLE)) {
        return TRUE;
    }
    state = FwpsQueryConnectionRedirectState(
        Metadata->redirectRecords,
        g_NonProxyWfp->RedirectHandle,
        NULL);
    return state == FWPS_CONNECTION_NOT_REDIRECTED ||
        state == FWPS_CONNECTION_REDIRECTED_BY_OTHER;
}

VOID NTAPI
NonProxyClassifyConnect(
    _In_ const FWPS_INCOMING_VALUES0* IncomingValues,
    _In_ const FWPS_INCOMING_METADATA_VALUES0* Metadata,
    _Inout_opt_ VOID* LayerData,
    _In_opt_ const VOID* ClassifyContext,
    _In_ const FWPS_FILTER1* Filter,
    _In_ UINT64 FlowContext,
    _Inout_ FWPS_CLASSIFY_OUT0* ClassifyOut)
{
    NP_WFP_CONFIG_V2 config;
    UINT64 classifyHandle = 0;
    FWPS_CONNECT_REQUEST0* request = NULL;
    NP_WFP_REDIRECT_CONTEXT_V1* context = NULL;
    SIZE_T contextSize = 0;
    NTSTATUS status;
    BOOLEAN contextTransferred = FALSE;
    BOOLEAN writableAcquired = FALSE;

    UNREFERENCED_PARAMETER(LayerData);
    UNREFERENCED_PARAMETER(FlowContext);

    if ((ClassifyOut->rights & FWPS_RIGHT_ACTION_WRITE) == 0 ||
        g_NonProxyWfp == NULL) {
        return;
    }
    ClassifyOut->actionType = FWP_ACTION_PERMIT;
    InterlockedIncrement(&g_NonProxyWfp->ActiveClassifications);
    config = NonProxyReadConfig(g_NonProxyWfp);
    if (!NonProxyShouldRedirect(Metadata, &config, Filter->context)) {
        goto Exit;
    }

    status = FwpsAcquireClassifyHandle0(
        (VOID*)ClassifyContext,
        0,
        &classifyHandle);
    if (!NT_SUCCESS(status)) {
        goto FailOpen;
    }
    status = FwpsAcquireWritableLayerDataPointer0(
        classifyHandle,
        Filter->filterId,
        0,
        (VOID**)&request,
        ClassifyOut);
    if (!NT_SUCCESS(status) || request == NULL) {
        goto FailOpen;
    }
    writableAcquired = TRUE;
    if (request->previousVersion != NULL &&
        request->previousVersion->localRedirectHandle != NULL) {
        ClassifyOut->actionType = FWP_ACTION_PERMIT;
        ClassifyOut->rights |= FWPS_RIGHT_ACTION_WRITE;
        goto Exit;
    }

    context = NonProxyAllocateContext(
        IncomingValues,
        Metadata,
        request,
        &contextSize);
    if (context == NULL) {
        goto FailOpen;
    }
    request->localRedirectHandle = g_NonProxyWfp->RedirectHandle;
    request->localRedirectTargetPID = (UINT32)config.ProxyProcessId;
    request->localRedirectContext = context;
    request->localRedirectContextSize = contextSize;
    NonProxySetLoopback(
        &request->remoteAddressAndPort,
        IncomingValues->layerId,
        NonProxyProxyPort(&config, IncomingValues->layerId, Filter->context));

    ClassifyOut->actionType = FWP_ACTION_PERMIT;
    ClassifyOut->rights |= FWPS_RIGHT_ACTION_WRITE;
    FwpsApplyModifiedLayerData0(
        classifyHandle,
        request,
        0);
    writableAcquired = FALSE;
    contextTransferred = TRUE;
    InterlockedIncrement64(&g_NonProxyWfp->RedirectedConnections);
    goto Exit;

FailOpen:
    InterlockedIncrement64(&g_NonProxyWfp->FailOpenConnections);
    ClassifyOut->actionType = FWP_ACTION_PERMIT;
    ClassifyOut->rights |= FWPS_RIGHT_ACTION_WRITE;

Exit:
    if (writableAcquired && !contextTransferred) {
        FwpsApplyModifiedLayerData0(
            classifyHandle,
            request,
            0);
        writableAcquired = FALSE;
    }
    if (context != NULL && !contextTransferred) {
        ExFreePoolWithTag(context, NP_WFP_POOL_TAG);
    }
    if (classifyHandle != 0) {
        FwpsReleaseClassifyHandle0(classifyHandle);
    }
    InterlockedDecrement(&g_NonProxyWfp->ActiveClassifications);
}
