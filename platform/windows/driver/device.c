#include "nonproxy_wfp_driver.h"

static const UNICODE_STRING g_DeviceName =
    RTL_CONSTANT_STRING(L"\\Device\\NonProxyWfp");
static const UNICODE_STRING g_SymbolicLink =
    RTL_CONSTANT_STRING(L"\\DosDevices\\NonProxyWfp");
static const GUID g_DeviceClass =
    {0xf2e299fc, 0x73ca, 0x43f2, {0xa6, 0x9f, 0xe8, 0x89, 0x6b, 0x5a, 0xde, 0x34}};

static NTSTATUS
NonProxyComplete(
    _Inout_ PIRP Irp,
    _In_ NTSTATUS Status,
    _In_ ULONG_PTR Information)
{
    Irp->IoStatus.Status = Status;
    Irp->IoStatus.Information = Information;
    IoCompleteRequest(Irp, IO_NO_INCREMENT);
    return Status;
}

static NTSTATUS
NonProxyDispatchCreateClose(
    _In_ PDEVICE_OBJECT DeviceObject,
    _Inout_ PIRP Irp)
{
    UNREFERENCED_PARAMETER(DeviceObject);
    return NonProxyComplete(Irp, STATUS_SUCCESS, 0);
}

static NTSTATUS
NonProxyDispatchCleanup(
    _In_ PDEVICE_OBJECT DeviceObject,
    _Inout_ PIRP Irp)
{
    NP_WFP_DEVICE_EXTENSION* extension = DeviceObject->DeviceExtension;
    NonProxyDisableRedirect(extension);
    return NonProxyComplete(Irp, STATUS_SUCCESS, 0);
}

static NTSTATUS
NonProxyDispatchDeviceControl(
    _In_ PDEVICE_OBJECT DeviceObject,
    _Inout_ PIRP Irp)
{
    PIO_STACK_LOCATION stack = IoGetCurrentIrpStackLocation(Irp);
    NP_WFP_DEVICE_EXTENSION* extension = DeviceObject->DeviceExtension;
    ULONG code = stack->Parameters.DeviceIoControl.IoControlCode;
    ULONG inputLength = stack->Parameters.DeviceIoControl.InputBufferLength;
    ULONG outputLength = stack->Parameters.DeviceIoControl.OutputBufferLength;
    NTSTATUS status;
    NP_WFP_STATUS_V1* output;

    if (outputLength < sizeof(NP_WFP_STATUS_V1)) {
        return NonProxyComplete(Irp, STATUS_BUFFER_TOO_SMALL, sizeof(NP_WFP_STATUS_V1));
    }

    output = (NP_WFP_STATUS_V1*)Irp->AssociatedIrp.SystemBuffer;
    if (code == IOCTL_NP_WFP_APPLY_CONFIG) {
        if (inputLength < sizeof(NP_WFP_CONFIG_V2)) {
            return NonProxyComplete(Irp, STATUS_BUFFER_TOO_SMALL, 0);
        }
        status = NonProxyApplyConfig(
            extension,
            (const NP_WFP_CONFIG_V2*)Irp->AssociatedIrp.SystemBuffer);
        if (!NT_SUCCESS(status)) {
            return NonProxyComplete(Irp, status, 0);
        }
    } else if (code != IOCTL_NP_WFP_QUERY_STATUS) {
        return NonProxyComplete(Irp, STATUS_INVALID_DEVICE_REQUEST, 0);
    }

    NonProxyReadStatus(extension, output);
    return NonProxyComplete(Irp, STATUS_SUCCESS, sizeof(*output));
}

NTSTATUS
NonProxyCreateControlDevice(
    _In_ PDRIVER_OBJECT DriverObject,
    _Out_ PDEVICE_OBJECT* DeviceObject)
{
    NTSTATUS status;
    PDEVICE_OBJECT deviceObject = NULL;

    status = IoCreateDeviceSecure(
        DriverObject,
        sizeof(NP_WFP_DEVICE_EXTENSION),
        (PUNICODE_STRING)&g_DeviceName,
        FILE_DEVICE_NETWORK,
        FILE_DEVICE_SECURE_OPEN,
        TRUE,
        &SDDL_DEVOBJ_SYS_ALL_ADM_ALL,
        &g_DeviceClass,
        &deviceObject);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = IoCreateSymbolicLink(
        (PUNICODE_STRING)&g_SymbolicLink,
        (PUNICODE_STRING)&g_DeviceName);
    if (!NT_SUCCESS(status)) {
        IoDeleteDevice(deviceObject);
        return status;
    }

    DriverObject->MajorFunction[IRP_MJ_CREATE] = NonProxyDispatchCreateClose;
    DriverObject->MajorFunction[IRP_MJ_CLOSE] = NonProxyDispatchCreateClose;
    DriverObject->MajorFunction[IRP_MJ_CLEANUP] = NonProxyDispatchCleanup;
    DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL] = NonProxyDispatchDeviceControl;
    deviceObject->Flags |= DO_BUFFERED_IO;
    deviceObject->Flags &= ~DO_DEVICE_INITIALIZING;
    *DeviceObject = deviceObject;
    return STATUS_SUCCESS;
}

VOID
NonProxyDeleteControlDevice(
    _In_opt_ PDEVICE_OBJECT DeviceObject)
{
    if (DeviceObject != NULL) {
        IoDeleteSymbolicLink((PUNICODE_STRING)&g_SymbolicLink);
        IoDeleteDevice(DeviceObject);
    }
}
