#pragma once

#include <ntddk.h>
#include <wdmsec.h>
#include <ndis.h>
#include <fwpsk.h>
#include <ws2ipdef.h>

#include "../include/nonproxy_wfp_abi.h"

#define NP_WFP_POOL_TAG 'fwPN'
#define NP_WFP_UDP_QUEUE_MAX_COUNT ((ULONG)4096)
#define NP_WFP_UDP_QUEUE_MAX_BYTES ((SIZE_T)(16 * 1024 * 1024))
#define NP_WFP_UDP_IP_HEADROOM ((ULONG)64)
#define NP_WFP_UDP_HEADER_BYTES ((ULONG)8)

typedef struct _NP_WFP_UDP_FLOW_CONTEXT {
    UINT64 ProcessId;
    UINT32 AppIdLength;
    UCHAR AppId[ANYSIZE_ARRAY];
} NP_WFP_UDP_FLOW_CONTEXT;

typedef struct _NP_WFP_UDP_PACKET_NODE {
    LIST_ENTRY Link;
    ULONG RecordSize;
    UCHAR Record[ANYSIZE_ARRAY];
} NP_WFP_UDP_PACKET_NODE;

typedef struct _NP_WFP_DEVICE_EXTENSION {
    KSPIN_LOCK StateLock;
    NP_WFP_CONFIG_V3 Config;
    HANDLE RedirectHandle;
    UINT32 CalloutV4Id;
    UINT32 CalloutV6Id;
    UINT32 UdpFlowCalloutV4Id;
    UINT32 UdpFlowCalloutV6Id;
    UINT32 UdpDatagramCalloutV4Id;
    UINT32 UdpDatagramCalloutV6Id;
    KSPIN_LOCK UdpQueueLock;
    LIST_ENTRY UdpQueue;
    ULONG UdpQueueCount;
    SIZE_T UdpQueueBytes;
    NDIS_HANDLE UdpNdisHandle;
    NDIS_HANDLE UdpNblPool;
    HANDLE UdpInjectionV4;
    HANDLE UdpInjectionV6;
    KEVENT UdpInjectionIdle;
    volatile LONG ActiveUdpInjections;
    volatile LONG64 NextUdpPacketId;
    volatile LONG ActiveClassifications;
    volatile LONG64 RedirectedConnections;
    volatile LONG64 FailOpenConnections;
    volatile LONG64 QueuedUdpDatagrams;
    volatile LONG64 DroppedUdpDatagrams;
    volatile LONG64 InjectedUdpDatagrams;
} NP_WFP_DEVICE_EXTENSION;

extern NP_WFP_DEVICE_EXTENSION* g_NonProxyWfp;

DRIVER_INITIALIZE DriverEntry;
DRIVER_UNLOAD NonProxyDriverUnload;

NTSTATUS
NonProxyCreateControlDevice(
    _In_ PDRIVER_OBJECT DriverObject,
    _Out_ PDEVICE_OBJECT* DeviceObject);

VOID
NonProxyDeleteControlDevice(
    _In_opt_ PDEVICE_OBJECT DeviceObject);

VOID
NonProxyInitializeState(
    _Out_ NP_WFP_DEVICE_EXTENSION* Extension);

NTSTATUS
NonProxyApplyConfig(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _In_ const NP_WFP_CONFIG_V3* Config);

VOID
NonProxyDisableRedirect(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension);

NP_WFP_CONFIG_V3
NonProxyReadConfig(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension);

VOID
NonProxyReadStatus(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _Out_ NP_WFP_STATUS_V2* Status);

NTSTATUS
NonProxyInitializeUdpDataPlane(
    _In_ PDRIVER_OBJECT DriverObject,
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension);

VOID
NonProxyUninitializeUdpDataPlane(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension);

VOID
NonProxyFlushUdpQueue(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension);

NTSTATUS
NonProxyReceiveUdpBatch(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _Out_writes_bytes_(OutputLength) VOID* Output,
    _In_ ULONG OutputLength,
    _Out_ ULONG_PTR* BytesWritten);

NTSTATUS
NonProxyEnqueueUdpRecord(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _Inout_ NP_WFP_UDP_PACKET_NODE* Node);

NTSTATUS
NonProxyInjectUdp(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _In_reads_bytes_(InputLength) const VOID* Input,
    _In_ ULONG InputLength);

NTSTATUS
NonProxyRegisterCallouts(
    _In_ PDEVICE_OBJECT DeviceObject);

VOID
NonProxyUnregisterCallouts(VOID);

VOID NTAPI
NonProxyClassifyConnect(
    _In_ const FWPS_INCOMING_VALUES0* IncomingValues,
    _In_ const FWPS_INCOMING_METADATA_VALUES0* Metadata,
    _Inout_opt_ VOID* LayerData,
    _In_opt_ const VOID* ClassifyContext,
    _In_ const FWPS_FILTER1* Filter,
    _In_ UINT64 FlowContext,
    _Inout_ FWPS_CLASSIFY_OUT0* ClassifyOut);

VOID NTAPI
NonProxyClassifyUdpFlow(
    _In_ const FWPS_INCOMING_VALUES0* IncomingValues,
    _In_ const FWPS_INCOMING_METADATA_VALUES0* Metadata,
    _Inout_opt_ VOID* LayerData,
    _In_opt_ const VOID* ClassifyContext,
    _In_ const FWPS_FILTER1* Filter,
    _In_ UINT64 FlowContext,
    _Inout_ FWPS_CLASSIFY_OUT0* ClassifyOut);

VOID NTAPI
NonProxyClassifyUdpDatagram(
    _In_ const FWPS_INCOMING_VALUES0* IncomingValues,
    _In_ const FWPS_INCOMING_METADATA_VALUES0* Metadata,
    _Inout_opt_ VOID* LayerData,
    _In_opt_ const VOID* ClassifyContext,
    _In_ const FWPS_FILTER1* Filter,
    _In_ UINT64 FlowContext,
    _Inout_ FWPS_CLASSIFY_OUT0* ClassifyOut);

VOID NTAPI
NonProxyDeleteUdpFlow(
    _In_ UINT16 LayerId,
    _In_ UINT32 CalloutId,
    _In_ UINT64 FlowContext);

NTSTATUS NTAPI
NonProxyCalloutNotify(
    _In_ FWPS_CALLOUT_NOTIFY_TYPE NotifyType,
    _In_ const GUID* FilterKey,
    _Inout_ FWPS_FILTER1* Filter);
