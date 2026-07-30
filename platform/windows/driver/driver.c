#include <initguid.h>
#include "nonproxy_wfp_driver.h"

NP_WFP_DEVICE_EXTENSION* g_NonProxyWfp = NULL;

static PDEVICE_OBJECT g_DeviceObject = NULL;

NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT DriverObject,
    _In_ PUNICODE_STRING RegistryPath)
{
    NTSTATUS status;

    UNREFERENCED_PARAMETER(RegistryPath);
    DriverObject->DriverUnload = NonProxyDriverUnload;

    status = NonProxyCreateControlDevice(DriverObject, &g_DeviceObject);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    g_NonProxyWfp = g_DeviceObject->DeviceExtension;
    NonProxyInitializeState(g_NonProxyWfp);

    status = FwpsRedirectHandleCreate0(
        &NP_WFP_PROVIDER_KEY,
        0,
        &g_NonProxyWfp->RedirectHandle);
    if (!NT_SUCCESS(status)) {
        NonProxyDeleteControlDevice(g_DeviceObject);
        g_DeviceObject = NULL;
        g_NonProxyWfp = NULL;
        return status;
    }

    status = NonProxyRegisterCallouts(g_DeviceObject);
    if (!NT_SUCCESS(status)) {
        FwpsRedirectHandleDestroy0(g_NonProxyWfp->RedirectHandle);
        NonProxyDeleteControlDevice(g_DeviceObject);
        g_DeviceObject = NULL;
        g_NonProxyWfp = NULL;
        return status;
    }
    return STATUS_SUCCESS;
}

VOID
NonProxyDriverUnload(
    _In_ PDRIVER_OBJECT DriverObject)
{
    UNREFERENCED_PARAMETER(DriverObject);

    if (g_NonProxyWfp != NULL) {
        NonProxyDisableRedirect(g_NonProxyWfp);
        NonProxyUnregisterCallouts();
        if (g_NonProxyWfp->RedirectHandle != NULL) {
            FwpsRedirectHandleDestroy0(g_NonProxyWfp->RedirectHandle);
            g_NonProxyWfp->RedirectHandle = NULL;
        }
    }
    NonProxyDeleteControlDevice(g_DeviceObject);
    g_DeviceObject = NULL;
    g_NonProxyWfp = NULL;
}

NTSTATUS
NonProxyRegisterCallouts(
    _In_ PDEVICE_OBJECT DeviceObject)
{
    FWPS_CALLOUT1 callout;
    NTSTATUS status;

    RtlZeroMemory(&callout, sizeof(callout));
    callout.classifyFn = NonProxyClassifyConnect;
    callout.notifyFn = NonProxyCalloutNotify;
    callout.calloutKey = NP_WFP_CALLOUT_V4_KEY;
    status = FwpsCalloutRegister1(
        DeviceObject,
        &callout,
        &g_NonProxyWfp->CalloutV4Id);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    callout.calloutKey = NP_WFP_CALLOUT_V6_KEY;
    status = FwpsCalloutRegister1(
        DeviceObject,
        &callout,
        &g_NonProxyWfp->CalloutV6Id);
    if (!NT_SUCCESS(status)) {
        FwpsCalloutUnregisterById0(g_NonProxyWfp->CalloutV4Id);
        g_NonProxyWfp->CalloutV4Id = 0;
    }
    return status;
}

VOID
NonProxyUnregisterCallouts(VOID)
{
    if (g_NonProxyWfp->CalloutV6Id != 0) {
        FwpsCalloutUnregisterById0(g_NonProxyWfp->CalloutV6Id);
        g_NonProxyWfp->CalloutV6Id = 0;
    }
    if (g_NonProxyWfp->CalloutV4Id != 0) {
        FwpsCalloutUnregisterById0(g_NonProxyWfp->CalloutV4Id);
        g_NonProxyWfp->CalloutV4Id = 0;
    }
}

NTSTATUS NTAPI
NonProxyCalloutNotify(
    _In_ FWPS_CALLOUT_NOTIFY_TYPE NotifyType,
    _In_ const GUID* FilterKey,
    _Inout_ FWPS_FILTER1* Filter)
{
    UNREFERENCED_PARAMETER(NotifyType);
    UNREFERENCED_PARAMETER(FilterKey);
    UNREFERENCED_PARAMETER(Filter);
    return STATUS_SUCCESS;
}
