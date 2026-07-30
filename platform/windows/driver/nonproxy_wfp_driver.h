#pragma once

#include <ntddk.h>
#include <wdmsec.h>
#include <fwpsk.h>
#include <ws2ipdef.h>

#include "../include/nonproxy_wfp_abi.h"

#define NP_WFP_POOL_TAG 'fwPN'

typedef struct _NP_WFP_DEVICE_EXTENSION {
    KSPIN_LOCK StateLock;
    NP_WFP_CONFIG_V1 Config;
    HANDLE RedirectHandle;
    UINT32 CalloutV4Id;
    UINT32 CalloutV6Id;
    volatile LONG ActiveClassifications;
    volatile LONG64 RedirectedConnections;
    volatile LONG64 FailOpenConnections;
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
    _In_ const NP_WFP_CONFIG_V1* Config);

VOID
NonProxyDisableRedirect(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension);

NP_WFP_CONFIG_V1
NonProxyReadConfig(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension);

VOID
NonProxyReadStatus(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _Out_ NP_WFP_STATUS_V1* Status);

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

NTSTATUS NTAPI
NonProxyCalloutNotify(
    _In_ FWPS_CALLOUT_NOTIFY_TYPE NotifyType,
    _In_ const GUID* FilterKey,
    _Inout_ FWPS_FILTER1* Filter);
