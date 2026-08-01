#include "nonproxy_wfp_driver.h"

static UINT32
NonProxyUdpFlowAppIdField(
    _In_ UINT16 LayerId)
{
    return LayerId == FWPS_LAYER_ALE_FLOW_ESTABLISHED_V4
        ? FWPS_FIELD_ALE_FLOW_ESTABLISHED_V4_ALE_APP_ID
        : FWPS_FIELD_ALE_FLOW_ESTABLISHED_V6_ALE_APP_ID;
}

static UINT32
NonProxyUdpFlowPackageSidField(
    _In_ UINT16 LayerId)
{
    return LayerId == FWPS_LAYER_ALE_FLOW_ESTABLISHED_V4
        ? FWPS_FIELD_ALE_FLOW_ESTABLISHED_V4_ALE_PACKAGE_ID
        : FWPS_FIELD_ALE_FLOW_ESTABLISHED_V6_ALE_PACKAGE_ID;
}

static UINT16
NonProxyUdpDatagramLayer(
    _In_ UINT16 FlowLayerId)
{
    return FlowLayerId == FWPS_LAYER_ALE_FLOW_ESTABLISHED_V4
        ? FWPS_LAYER_DATAGRAM_DATA_V4
        : FWPS_LAYER_DATAGRAM_DATA_V6;
}

static UINT32
NonProxyUdpDatagramCalloutId(
    _In_ UINT16 FlowLayerId)
{
    return FlowLayerId == FWPS_LAYER_ALE_FLOW_ESTABLISHED_V4
        ? g_NonProxyWfp->UdpDatagramCalloutV4Id
        : g_NonProxyWfp->UdpDatagramCalloutV6Id;
}

static VOID
NonProxyCopyUdpAddress(
    _In_ const FWPS_INCOMING_VALUES0* Values,
    _In_ BOOLEAN Local,
    _Out_writes_(16) UCHAR Address[16])
{
    if (Values->layerId == FWPS_LAYER_DATAGRAM_DATA_V4) {
        UINT32 field = Local
            ? FWPS_FIELD_DATAGRAM_DATA_V4_IP_LOCAL_ADDRESS
            : FWPS_FIELD_DATAGRAM_DATA_V4_IP_REMOTE_ADDRESS;
        UINT32 networkAddress =
            RtlUlongByteSwap(Values->incomingValue[field].value.uint32);
        RtlCopyMemory(Address, &networkAddress, sizeof(networkAddress));
    } else {
        UINT32 field = Local
            ? FWPS_FIELD_DATAGRAM_DATA_V6_IP_LOCAL_ADDRESS
            : FWPS_FIELD_DATAGRAM_DATA_V6_IP_REMOTE_ADDRESS;
        const FWP_BYTE_ARRAY16* value =
            Values->incomingValue[field].value.byteArray16;
        if (value != NULL) {
            RtlCopyMemory(Address, value->byteArray16, 16);
        }
    }
}

static NP_WFP_UDP_PACKET_NODE*
NonProxyBuildUdpRecord(
    _In_ const FWPS_INCOMING_VALUES0* Values,
    _In_ const FWPS_INCOMING_METADATA_VALUES0* Metadata,
    _In_ NET_BUFFER_LIST* NetBufferList,
    _In_ const NP_WFP_UDP_FLOW_CONTEXT* Flow)
{
    NET_BUFFER* netBuffer = NET_BUFFER_LIST_FIRST_NB(NetBufferList);
    ULONG udpSize;
    ULONG payloadSize;
    ULONG recordSize;
    SIZE_T allocationSize;
    NP_WFP_UDP_PACKET_NODE* node = NULL;
    NP_WFP_UDP_DATAGRAM_V2* record;
    UCHAR* contiguous;
    UCHAR* temporary = NULL;
    UINT16 localPortNetworkOrder;
    UINT16 remotePortNetworkOrder;
    UINT16 udpLength;

    if (netBuffer == NULL || NET_BUFFER_NEXT_NB(netBuffer) != NULL) {
        return NULL;
    }
    udpSize = NET_BUFFER_DATA_LENGTH(netBuffer);
    if (udpSize < NP_WFP_UDP_HEADER_BYTES ||
        udpSize > NP_WFP_MAX_UDP_PAYLOAD_BYTES + NP_WFP_UDP_HEADER_BYTES) {
        return NULL;
    }
    payloadSize = udpSize - NP_WFP_UDP_HEADER_BYTES;
    if (Flow->AppIdLength > NP_WFP_MAX_APP_ID_BYTES ||
        Flow->PackageSidLength > NP_WFP_MAX_PACKAGE_SID_BYTES ||
        Flow->AppIdLength > MAXULONG - Flow->PackageSidLength -
            FIELD_OFFSET(NP_WFP_UDP_DATAGRAM_V2, Data) ||
        payloadSize > MAXULONG - Flow->AppIdLength -
            Flow->PackageSidLength -
            FIELD_OFFSET(NP_WFP_UDP_DATAGRAM_V2, Data)) {
        return NULL;
    }
    recordSize =
        FIELD_OFFSET(NP_WFP_UDP_DATAGRAM_V2, Data) +
        Flow->AppIdLength +
        Flow->PackageSidLength +
        payloadSize;
    allocationSize =
        FIELD_OFFSET(NP_WFP_UDP_PACKET_NODE, Record) + recordSize;
    node = ExAllocatePool2(
        POOL_FLAG_NON_PAGED,
        allocationSize,
        NP_WFP_POOL_TAG);
    if (node == NULL) {
        return NULL;
    }
    RtlZeroMemory(node, allocationSize);

    contiguous = NdisGetDataBuffer(netBuffer, udpSize, NULL, 1, 0);
    if (contiguous == NULL) {
        temporary = ExAllocatePool2(
            POOL_FLAG_NON_PAGED,
            udpSize,
            NP_WFP_POOL_TAG);
        if (temporary == NULL) {
            ExFreePoolWithTag(node, NP_WFP_POOL_TAG);
            return NULL;
        }
        contiguous = NdisGetDataBuffer(netBuffer, udpSize, temporary, 1, 0);
    }
    if (contiguous == NULL) {
        if (temporary != NULL) {
            ExFreePoolWithTag(temporary, NP_WFP_POOL_TAG);
        }
        ExFreePoolWithTag(node, NP_WFP_POOL_TAG);
        return NULL;
    }

    RtlCopyMemory(
        &localPortNetworkOrder,
        contiguous,
        sizeof(localPortNetworkOrder));
    RtlCopyMemory(
        &remotePortNetworkOrder,
        contiguous + 2,
        sizeof(remotePortNetworkOrder));
    RtlCopyMemory(&udpLength, contiguous + 4, sizeof(udpLength));
    if (localPortNetworkOrder == 0 || remotePortNetworkOrder == 0 ||
        RtlUshortByteSwap(udpLength) != udpSize) {
        if (temporary != NULL) {
            ExFreePoolWithTag(temporary, NP_WFP_POOL_TAG);
        }
        ExFreePoolWithTag(node, NP_WFP_POOL_TAG);
        return NULL;
    }

    node->RecordSize = recordSize;
    record = (NP_WFP_UDP_DATAGRAM_V2*)node->Record;
    record->Magic = NP_WFP_UDP_DATAGRAM_MAGIC;
    record->Version = NP_WFP_UDP_ABI_VERSION;
    record->HeaderSize = FIELD_OFFSET(NP_WFP_UDP_DATAGRAM_V2, Data);
    record->TotalSize = recordSize;
    record->AddressFamily =
        Values->layerId == FWPS_LAYER_DATAGRAM_DATA_V4 ? AF_INET : AF_INET6;
    record->PacketId =
        (UINT64)InterlockedIncrement64(&g_NonProxyWfp->NextUdpPacketId);
    record->ProcessId = Flow->ProcessId;
    record->CompartmentId = FWPS_IS_METADATA_FIELD_PRESENT(
        Metadata,
        FWPS_METADATA_FIELD_COMPARTMENT_ID)
        ? Metadata->compartmentId
        : UNSPECIFIED_COMPARTMENT_ID;
    record->InterfaceIndex = Values->incomingValue[
        Values->layerId == FWPS_LAYER_DATAGRAM_DATA_V4
            ? FWPS_FIELD_DATAGRAM_DATA_V4_INTERFACE_INDEX
            : FWPS_FIELD_DATAGRAM_DATA_V6_INTERFACE_INDEX].value.uint32;
    record->SubInterfaceIndex = Values->incomingValue[
        Values->layerId == FWPS_LAYER_DATAGRAM_DATA_V4
            ? FWPS_FIELD_DATAGRAM_DATA_V4_SUB_INTERFACE_INDEX
            : FWPS_FIELD_DATAGRAM_DATA_V6_SUB_INTERFACE_INDEX].value.uint32;
    record->LocalPortNetworkOrder = localPortNetworkOrder;
    record->RemotePortNetworkOrder = remotePortNetworkOrder;
    NonProxyCopyUdpAddress(Values, TRUE, record->LocalAddress);
    NonProxyCopyUdpAddress(Values, FALSE, record->RemoteAddress);
    record->AppIdLength = Flow->AppIdLength;
    record->PackageSidLength = Flow->PackageSidLength;
    record->PayloadLength = payloadSize;
    RtlCopyMemory(record->Data, Flow->Data, Flow->AppIdLength);
    RtlCopyMemory(
        record->Data + Flow->AppIdLength,
        Flow->Data + Flow->AppIdLength,
        Flow->PackageSidLength);
    RtlCopyMemory(
        record->Data + Flow->AppIdLength + Flow->PackageSidLength,
        contiguous + NP_WFP_UDP_HEADER_BYTES,
        payloadSize);
    if (temporary != NULL) {
        ExFreePoolWithTag(temporary, NP_WFP_POOL_TAG);
    }
    return node;
}

VOID NTAPI
NonProxyClassifyUdpFlow(
    _In_ const FWPS_INCOMING_VALUES0* IncomingValues,
    _In_ const FWPS_INCOMING_METADATA_VALUES0* Metadata,
    _Inout_opt_ VOID* LayerData,
    _In_opt_ const VOID* ClassifyContext,
    _In_ const FWPS_FILTER1* Filter,
    _In_ UINT64 FlowContext,
    _Inout_ FWPS_CLASSIFY_OUT0* ClassifyOut)
{
    const FWP_BYTE_BLOB* appId;
    const UCHAR* packageSid;
    UINT32 appIdLength;
    UINT32 packageSidLength;
    ULONG headerSize;
    SIZE_T contextSize;
    NP_WFP_UDP_FLOW_CONTEXT* context;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(LayerData);
    UNREFERENCED_PARAMETER(ClassifyContext);
    UNREFERENCED_PARAMETER(Filter);
    ClassifyOut->actionType = FWP_ACTION_CONTINUE;
    if (FlowContext != 0 ||
        !FWPS_IS_METADATA_FIELD_PRESENT(Metadata, FWPS_METADATA_FIELD_FLOW_HANDLE)) {
        return;
    }
    appId = IncomingValues->incomingValue[
        NonProxyUdpFlowAppIdField(IncomingValues->layerId)].value.byteBlob;
    appIdLength = appId == NULL ? 0 : appId->size;
    if (appIdLength > NP_WFP_MAX_APP_ID_BYTES ||
        (appIdLength != 0 && appId->data == NULL) ||
        !NonProxyReadPackageSid(
            &IncomingValues->incomingValue[
                NonProxyUdpFlowPackageSidField(IncomingValues->layerId)].value,
            &packageSid,
            &packageSidLength)) {
        return;
    }
    headerSize = FIELD_OFFSET(NP_WFP_UDP_FLOW_CONTEXT, Data);
    if (appIdLength > MAXULONG - packageSidLength - headerSize) {
        return;
    }
    contextSize = headerSize + appIdLength + packageSidLength;
    context = ExAllocatePool2(
        POOL_FLAG_NON_PAGED,
        contextSize,
        NP_WFP_POOL_TAG);
    if (context == NULL) {
        return;
    }
    RtlZeroMemory(context, contextSize);
    context->ProcessId = FWPS_IS_METADATA_FIELD_PRESENT(
        Metadata,
        FWPS_METADATA_FIELD_PROCESS_ID)
        ? Metadata->processId
        : 0;
    context->AppIdLength = appIdLength;
    context->PackageSidLength = packageSidLength;
    if (appIdLength != 0) {
        RtlCopyMemory(context->Data, appId->data, appIdLength);
    }
    if (packageSidLength != 0) {
        RtlCopyMemory(
            context->Data + appIdLength,
            packageSid,
            packageSidLength);
    }
    status = FwpsFlowAssociateContext0(
        Metadata->flowHandle,
        NonProxyUdpDatagramLayer(IncomingValues->layerId),
        NonProxyUdpDatagramCalloutId(IncomingValues->layerId),
        (UINT64)context);
    if (!NT_SUCCESS(status)) {
        ExFreePoolWithTag(context, NP_WFP_POOL_TAG);
    }
}

VOID NTAPI
NonProxyClassifyUdpDatagram(
    _In_ const FWPS_INCOMING_VALUES0* IncomingValues,
    _In_ const FWPS_INCOMING_METADATA_VALUES0* Metadata,
    _Inout_opt_ VOID* LayerData,
    _In_opt_ const VOID* ClassifyContext,
    _In_ const FWPS_FILTER1* Filter,
    _In_ UINT64 FlowContext,
    _Inout_ FWPS_CLASSIFY_OUT0* ClassifyOut)
{
    NP_WFP_CONFIG_V3 config;
    NP_WFP_UDP_PACKET_NODE* node;

    UNREFERENCED_PARAMETER(ClassifyContext);
    UNREFERENCED_PARAMETER(Filter);
    if ((ClassifyOut->rights & FWPS_RIGHT_ACTION_WRITE) == 0 ||
        g_NonProxyWfp == NULL) {
        return;
    }
    ClassifyOut->actionType = FWP_ACTION_PERMIT;
    if (IncomingValues->incomingValue[
            IncomingValues->layerId == FWPS_LAYER_DATAGRAM_DATA_V4
                ? FWPS_FIELD_DATAGRAM_DATA_V4_DIRECTION
                : FWPS_FIELD_DATAGRAM_DATA_V6_DIRECTION].value.uint32 !=
            FWP_DIRECTION_OUTBOUND ||
        IncomingValues->incomingValue[
            IncomingValues->layerId == FWPS_LAYER_DATAGRAM_DATA_V4
                ? FWPS_FIELD_DATAGRAM_DATA_V4_IP_REMOTE_PORT
                : FWPS_FIELD_DATAGRAM_DATA_V6_IP_REMOTE_PORT].value.uint16 == 53) {
        return;
    }
    config = NonProxyReadConfig(g_NonProxyWfp);
    if ((config.Flags & NP_WFP_CONFIG_FLAG_UDP_DIVERT) == 0) {
        return;
    }
    if (FlowContext != 0 &&
        ((NP_WFP_UDP_FLOW_CONTEXT*)FlowContext)->ProcessId ==
            config.ProxyProcessId) {
        return;
    }
    if (FlowContext == 0 || LayerData == NULL) {
        InterlockedIncrement64(&g_NonProxyWfp->DroppedUdpDatagrams);
        goto Block;
    }
    node = NonProxyBuildUdpRecord(
        IncomingValues,
        Metadata,
        (NET_BUFFER_LIST*)LayerData,
        (NP_WFP_UDP_FLOW_CONTEXT*)FlowContext);
    if (node == NULL) {
        InterlockedIncrement64(&g_NonProxyWfp->DroppedUdpDatagrams);
        goto Block;
    }
    if (!NT_SUCCESS(NonProxyEnqueueUdpRecord(g_NonProxyWfp, node))) {
        ExFreePoolWithTag(node, NP_WFP_POOL_TAG);
    }

Block:
    ClassifyOut->actionType = FWP_ACTION_BLOCK;
    ClassifyOut->flags |= FWPS_CLASSIFY_OUT_FLAG_ABSORB;
    ClassifyOut->rights &= ~FWPS_RIGHT_ACTION_WRITE;
}

VOID NTAPI
NonProxyDeleteUdpFlow(
    _In_ UINT16 LayerId,
    _In_ UINT32 CalloutId,
    _In_ UINT64 FlowContext)
{
    UNREFERENCED_PARAMETER(LayerId);
    UNREFERENCED_PARAMETER(CalloutId);
    if (FlowContext != 0) {
        ExFreePoolWithTag(
            (VOID*)FlowContext,
            NP_WFP_POOL_TAG);
    }
}
