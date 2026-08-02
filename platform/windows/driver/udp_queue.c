#include "nonproxy_wfp_driver.h"

NTSTATUS
NonProxyEnqueueUdpRecord(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _Inout_ NP_WFP_UDP_PACKET_NODE* Node)
{
    KIRQL oldIrql;
    BOOLEAN accepted = FALSE;
    const ULONG datagramHeaderSize =
        (ULONG)FIELD_OFFSET(NP_WFP_UDP_DATAGRAM_V2, Data);

    if (Node == NULL ||
        Node->RecordSize < datagramHeaderSize ||
        Node->RecordSize > NP_WFP_MAX_UDP_BATCH_BYTES) {
        return STATUS_INVALID_PARAMETER;
    }

    KeAcquireSpinLock(&Extension->UdpQueueLock, &oldIrql);
    if (Extension->UdpQueueCount < NP_WFP_UDP_QUEUE_MAX_COUNT &&
        Extension->UdpQueueBytes <=
            NP_WFP_UDP_QUEUE_MAX_BYTES - Node->RecordSize) {
        InsertTailList(&Extension->UdpQueue, &Node->Link);
        Extension->UdpQueueCount += 1;
        Extension->UdpQueueBytes += Node->RecordSize;
        accepted = TRUE;
    }
    KeReleaseSpinLock(&Extension->UdpQueueLock, oldIrql);

    if (!accepted) {
        InterlockedIncrement64(&Extension->DroppedUdpDatagrams);
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    InterlockedIncrement64(&Extension->QueuedUdpDatagrams);
    return STATUS_SUCCESS;
}

NTSTATUS
NonProxyReceiveUdpBatch(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension,
    _Out_writes_bytes_(OutputLength) VOID* Output,
    _In_ ULONG OutputLength,
    _Out_ ULONG_PTR* BytesWritten)
{
    NP_WFP_UDP_BATCH_V2* batch = Output;
    LIST_ENTRY detached;
    const ULONG batchHeaderSize =
        (ULONG)FIELD_OFFSET(NP_WFP_UDP_BATCH_V2, Datagrams);
    ULONG selectedBytes = batchHeaderSize;
    ULONG cursor;
    ULONG count = 0;
    KIRQL oldIrql;

    InitializeListHead(&detached);
    *BytesWritten = 0;
    if (Output == NULL || OutputLength < batchHeaderSize ||
        OutputLength > NP_WFP_MAX_UDP_BATCH_BYTES) {
        return STATUS_INVALID_BUFFER_SIZE;
    }

    KeAcquireSpinLock(&Extension->UdpQueueLock, &oldIrql);
    while (!IsListEmpty(&Extension->UdpQueue)) {
        PLIST_ENTRY entry = Extension->UdpQueue.Flink;
        NP_WFP_UDP_PACKET_NODE* node =
            CONTAINING_RECORD(entry, NP_WFP_UDP_PACKET_NODE, Link);

        if (node->RecordSize > OutputLength - selectedBytes) {
            break;
        }
        RemoveEntryList(entry);
        InsertTailList(&detached, entry);
        Extension->UdpQueueCount -= 1;
        Extension->UdpQueueBytes -= node->RecordSize;
        selectedBytes += node->RecordSize;
        count += 1;
    }
    KeReleaseSpinLock(&Extension->UdpQueueLock, oldIrql);

    if (count == 0) {
        return STATUS_NO_MORE_ENTRIES;
    }
    cursor = batchHeaderSize;
    while (!IsListEmpty(&detached)) {
        PLIST_ENTRY entry = RemoveHeadList(&detached);
        NP_WFP_UDP_PACKET_NODE* node =
            CONTAINING_RECORD(entry, NP_WFP_UDP_PACKET_NODE, Link);
        RtlCopyMemory(
            (UCHAR*)Output + cursor,
            node->Record,
            node->RecordSize);
        cursor += node->RecordSize;
        ExFreePoolWithTag(node, NP_WFP_POOL_TAG);
    }
    RtlZeroMemory(batch, batchHeaderSize);
    batch->Magic = NP_WFP_UDP_BATCH_MAGIC;
    batch->Version = NP_WFP_UDP_ABI_VERSION;
    batch->HeaderSize = (UINT16)batchHeaderSize;
    batch->TotalSize = selectedBytes;
    batch->DatagramCount = count;
    *BytesWritten = selectedBytes;
    return STATUS_SUCCESS;
}

VOID
NonProxyFlushUdpQueue(
    _Inout_ NP_WFP_DEVICE_EXTENSION* Extension)
{
    LIST_ENTRY detached;
    KIRQL oldIrql;

    InitializeListHead(&detached);
    KeAcquireSpinLock(&Extension->UdpQueueLock, &oldIrql);
    while (!IsListEmpty(&Extension->UdpQueue)) {
        PLIST_ENTRY entry = RemoveHeadList(&Extension->UdpQueue);
        InsertTailList(&detached, entry);
    }
    Extension->UdpQueueCount = 0;
    Extension->UdpQueueBytes = 0;
    KeReleaseSpinLock(&Extension->UdpQueueLock, oldIrql);

    while (!IsListEmpty(&detached)) {
        PLIST_ENTRY entry = RemoveHeadList(&detached);
        NP_WFP_UDP_PACKET_NODE* node =
            CONTAINING_RECORD(entry, NP_WFP_UDP_PACKET_NODE, Link);
        ExFreePoolWithTag(node, NP_WFP_POOL_TAG);
    }
}
