#include "nonproxy_wfp_driver.h"

typedef struct _NP_WFP_UDP_INJECTION_CONTEXT {
    NP_WFP_DEVICE_EXTENSION* Extension;
    UCHAR* Buffer;
    PMDL Mdl;
    NET_BUFFER_LIST* NetBufferList;
} NP_WFP_UDP_INJECTION_CONTEXT;

static VOID
NonProxyFinishUdpInjection(
    _Inout_ NP_WFP_UDP_INJECTION_CONTEXT* Context)
{
    NP_WFP_DEVICE_EXTENSION* extension = Context->Extension;

    if (Context->NetBufferList != NULL) {
        FwpsFreeNetBufferList(Context->NetBufferList);
    }
    if (Context->Mdl != NULL) {
        IoFreeMdl(Context->Mdl);
    }
    if (Context->Buffer != NULL) {
        ExFreePoolWithTag(Context->Buffer, NP_WFP_POOL_TAG);
    }
    ExFreePoolWithTag(Context, NP_WFP_POOL_TAG);
    if (InterlockedDecrement(&extension->ActiveUdpInjections) == 0) {
        KeSetEvent(&extension->UdpInjectionIdle, IO_NO_INCREMENT, FALSE);
    }
}

static VOID NTAPI
NonProxyCompleteUdpInjection(
    _In_ VOID* Context,
    _Inout_ NET_BUFFER_LIST* NetBufferList,
    _In_ BOOLEAN DispatchLevel)
{
    NP_WFP_UDP_INJECTION_CONTEXT* injection = Context;

    UNREFERENCED_PARAMETER(DispatchLevel);
    if (NT_SUCCESS(NetBufferList->Status)) {
        InterlockedIncrement64(
            &injection->Extension->InjectedUdpDatagrams);
    }
    NonProxyFinishUdpInjection(injection);
}

static BOOLEAN
NonProxyUdpAddressIsValid(
    _In_reads_(16) const UCHAR Address[16],
    _In_ UINT16 AddressFamily)
{
    ULONG length = AddressFamily == AF_INET ? 4 : 16;
    ULONG index;

    for (index = 0; index < length; index += 1) {
        if (Address[index] != 0) {
            return TRUE;
        }
    }
    return FALSE;
}

static BOOLEAN
NonProxyUdpBytesAreZero(
    _In_reads_(Length) const UCHAR* Bytes,
    _In_ ULONG Length)
{
    ULONG index;

    for (index = 0; index < Length; index += 1) {
        if (Bytes[index] != 0) {
            return FALSE;
        }
    }
    return TRUE;
}

static NTSTATUS
NonProxyValidateUdpInjection(
    _In_ const NP_WFP_UDP_INJECT_V1* Injection,
    _In_ ULONG InputLength)
{
    ULONG headerSize = FIELD_OFFSET(NP_WFP_UDP_INJECT_V1, Payload);

    if (Injection->Magic != NP_WFP_UDP_INJECT_MAGIC ||
        Injection->Version != NP_WFP_UDP_ABI_VERSION ||
        Injection->HeaderSize != headerSize ||
        Injection->TotalSize != InputLength ||
        Injection->Flags != 0 ||
        Injection->Reserved != 0 ||
        (Injection->AddressFamily != AF_INET &&
         Injection->AddressFamily != AF_INET6) ||
        Injection->LocalPortNetworkOrder == 0 ||
        Injection->RemotePortNetworkOrder == 0 ||
        Injection->PayloadLength > NP_WFP_MAX_UDP_PAYLOAD_BYTES ||
        Injection->PayloadLength != InputLength - headerSize ||
        !NonProxyUdpAddressIsValid(
            Injection->LocalAddress,
            Injection->AddressFamily) ||
        !NonProxyUdpAddressIsValid(
            Injection->RemoteAddress,
            Injection->AddressFamily)) {
        return STATUS_INVALID_PARAMETER;
    }
    if (Injection->AddressFamily == AF_INET &&
        (!NonProxyUdpBytesAreZero(Injection->LocalAddress + 4, 12) ||
         !NonProxyUdpBytesAreZero(Injection->RemoteAddress + 4, 12))) {
        return STATUS_INVALID_PARAMETER;
    }
    return STATUS_SUCCESS;
}

NTSTATUS
NonProxyInjectUdp(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _In_reads_bytes_(InputLength) const VOID* Input,
    _In_ ULONG InputLength)
{
    const NP_WFP_UDP_INJECT_V1* injection = Input;
    NP_WFP_UDP_INJECTION_CONTEXT* context = NULL;
    ULONG udpSize;
    ULONG bufferSize;
    UCHAR* udp;
    UINT16 udpLength;
    HANDLE injectionHandle;
    NTSTATUS status;

    if (Input == NULL ||
        InputLength < FIELD_OFFSET(NP_WFP_UDP_INJECT_V1, Payload)) {
        return STATUS_BUFFER_TOO_SMALL;
    }
    status = NonProxyValidateUdpInjection(injection, InputLength);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    udpSize = NP_WFP_UDP_HEADER_BYTES + injection->PayloadLength;
    bufferSize = NP_WFP_UDP_IP_HEADROOM + udpSize;
    context = ExAllocatePool2(
        POOL_FLAG_NON_PAGED,
        sizeof(*context),
        NP_WFP_POOL_TAG);
    if (context == NULL) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    RtlZeroMemory(context, sizeof(*context));
    context->Extension = Extension;
    context->Buffer = ExAllocatePool2(
        POOL_FLAG_NON_PAGED,
        bufferSize,
        NP_WFP_POOL_TAG);
    if (context->Buffer == NULL) {
        ExFreePoolWithTag(context, NP_WFP_POOL_TAG);
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    RtlZeroMemory(context->Buffer, bufferSize);
    udp = context->Buffer + NP_WFP_UDP_IP_HEADROOM;
    RtlCopyMemory(udp, &injection->RemotePortNetworkOrder, sizeof(UINT16));
    RtlCopyMemory(udp + 2, &injection->LocalPortNetworkOrder, sizeof(UINT16));
    udpLength = RtlUshortByteSwap((UINT16)udpSize);
    RtlCopyMemory(udp + 4, &udpLength, sizeof(UINT16));
    RtlCopyMemory(
        udp + NP_WFP_UDP_HEADER_BYTES,
        injection->Payload,
        injection->PayloadLength);

    context->Mdl = IoAllocateMdl(
        context->Buffer,
        bufferSize,
        FALSE,
        FALSE,
        NULL);
    if (context->Mdl == NULL) {
        ExFreePoolWithTag(context->Buffer, NP_WFP_POOL_TAG);
        ExFreePoolWithTag(context, NP_WFP_POOL_TAG);
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    MmBuildMdlForNonPagedPool(context->Mdl);
    status = FwpsAllocateNetBufferAndNetBufferList0(
        Extension->UdpNblPool,
        0,
        0,
        context->Mdl,
        NP_WFP_UDP_IP_HEADROOM,
        udpSize,
        &context->NetBufferList);
    if (!NT_SUCCESS(status)) {
        IoFreeMdl(context->Mdl);
        ExFreePoolWithTag(context->Buffer, NP_WFP_POOL_TAG);
        ExFreePoolWithTag(context, NP_WFP_POOL_TAG);
        return status;
    }
    status = FwpsConstructIpHeaderForTransportPacket0(
        context->NetBufferList,
        0,
        injection->AddressFamily,
        injection->RemoteAddress,
        injection->LocalAddress,
        IPPROTO_UDP,
        0,
        NULL,
        0,
        NULL,
        0,
        injection->InterfaceIndex,
        injection->SubInterfaceIndex);
    if (!NT_SUCCESS(status)) {
        FwpsFreeNetBufferList(context->NetBufferList);
        IoFreeMdl(context->Mdl);
        ExFreePoolWithTag(context->Buffer, NP_WFP_POOL_TAG);
        ExFreePoolWithTag(context, NP_WFP_POOL_TAG);
        return status;
    }

    injectionHandle = injection->AddressFamily == AF_INET
        ? Extension->UdpInjectionV4
        : Extension->UdpInjectionV6;
    if (InterlockedIncrement(&Extension->ActiveUdpInjections) == 1) {
        KeClearEvent(&Extension->UdpInjectionIdle);
    }
    status = FwpsInjectTransportReceiveAsync0(
        injectionHandle,
        NULL,
        NULL,
        0,
        injection->AddressFamily,
        injection->CompartmentId,
        injection->InterfaceIndex,
        injection->SubInterfaceIndex,
        context->NetBufferList,
        NonProxyCompleteUdpInjection,
        context);
    if (!NT_SUCCESS(status)) {
        NonProxyFinishUdpInjection(context);
    }
    return status;
}

NTSTATUS
NonProxyInitializeUdpDataPlane(
    _In_ PDRIVER_OBJECT DriverObject,
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension)
{
    NET_BUFFER_LIST_POOL_PARAMETERS parameters;
    NTSTATUS status;

    Extension->UdpNdisHandle =
        NdisAllocateGenericObject(DriverObject, NP_WFP_POOL_TAG, 0);
    if (Extension->UdpNdisHandle == NULL) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    RtlZeroMemory(&parameters, sizeof(parameters));
    parameters.Header.Type = NDIS_OBJECT_TYPE_DEFAULT;
    parameters.Header.Revision =
        NET_BUFFER_LIST_POOL_PARAMETERS_REVISION_1;
    parameters.Header.Size =
        NDIS_SIZEOF_NET_BUFFER_LIST_POOL_PARAMETERS_REVISION_1;
    parameters.fAllocateNetBuffer = TRUE;
    parameters.PoolTag = NP_WFP_POOL_TAG;
    Extension->UdpNblPool = NdisAllocateNetBufferListPool(
        Extension->UdpNdisHandle,
        &parameters);
    if (Extension->UdpNblPool == NULL) {
        NdisFreeGenericObject(Extension->UdpNdisHandle);
        Extension->UdpNdisHandle = NULL;
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    status = FwpsInjectionHandleCreate0(
        AF_INET,
        FWPS_INJECTION_TYPE_TRANSPORT,
        &Extension->UdpInjectionV4);
    if (!NT_SUCCESS(status)) {
        NonProxyUninitializeUdpDataPlane(Extension);
        return status;
    }
    status = FwpsInjectionHandleCreate0(
        AF_INET6,
        FWPS_INJECTION_TYPE_TRANSPORT,
        &Extension->UdpInjectionV6);
    if (!NT_SUCCESS(status)) {
        NonProxyUninitializeUdpDataPlane(Extension);
    }
    return status;
}

VOID
NonProxyUninitializeUdpDataPlane(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension)
{
    NonProxyFlushUdpQueue(Extension);
    if (InterlockedCompareExchange(
            &Extension->ActiveUdpInjections,
            0,
            0) != 0) {
        KeWaitForSingleObject(
            &Extension->UdpInjectionIdle,
            Executive,
            KernelMode,
            FALSE,
            NULL);
    }
    if (Extension->UdpInjectionV6 != NULL) {
        FwpsInjectionHandleDestroy0(Extension->UdpInjectionV6);
        Extension->UdpInjectionV6 = NULL;
    }
    if (Extension->UdpInjectionV4 != NULL) {
        FwpsInjectionHandleDestroy0(Extension->UdpInjectionV4);
        Extension->UdpInjectionV4 = NULL;
    }
    if (Extension->UdpNblPool != NULL) {
        NdisFreeNetBufferListPool(Extension->UdpNblPool);
        Extension->UdpNblPool = NULL;
    }
    if (Extension->UdpNdisHandle != NULL) {
        NdisFreeGenericObject(Extension->UdpNdisHandle);
        Extension->UdpNdisHandle = NULL;
    }
}
