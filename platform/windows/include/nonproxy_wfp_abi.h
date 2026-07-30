#pragma once

#include <guiddef.h>
#include <devioctl.h>
#include <ws2def.h>

#define NP_WFP_CONFIG_MAGIC ((UINT32)0x4657504e)
#define NP_WFP_CONFIG_VERSION ((UINT16)1)
#define NP_WFP_CONFIG_FLAG_ENABLED ((UINT32)0x00000001)

#define NP_WFP_STATUS_MAGIC ((UINT32)0x5357504e)
#define NP_WFP_STATUS_VERSION ((UINT16)1)

#define NP_WFP_CONTEXT_MAGIC ((UINT32)0x4357504e)
#define NP_WFP_CONTEXT_VERSION ((UINT16)1)
#define NP_WFP_MAX_APP_ID_BYTES ((UINT32)4096)

#define IOCTL_NP_WFP_APPLY_CONFIG \
    CTL_CODE(FILE_DEVICE_NETWORK, 0x801, METHOD_BUFFERED, FILE_WRITE_DATA)
#define IOCTL_NP_WFP_QUERY_STATUS \
    CTL_CODE(FILE_DEVICE_NETWORK, 0x802, METHOD_BUFFERED, FILE_READ_DATA)

typedef struct _NP_WFP_CONFIG_V1 {
    UINT32 Magic;
    UINT16 Version;
    UINT16 Size;
    UINT64 Generation;
    UINT64 ProxyProcessId;
    UINT16 Ipv4ProxyPortNetworkOrder;
    UINT16 Ipv6ProxyPortNetworkOrder;
    UINT32 Flags;
} NP_WFP_CONFIG_V1;

typedef struct _NP_WFP_STATUS_V1 {
    UINT32 Magic;
    UINT16 Version;
    UINT16 Size;
    UINT64 Generation;
    UINT64 ProxyProcessId;
    UINT32 Flags;
    UINT32 ActiveClassifications;
    UINT64 RedirectedConnections;
    UINT64 FailOpenConnections;
} NP_WFP_STATUS_V1;

typedef struct _NP_WFP_REDIRECT_CONTEXT_V1 {
    UINT32 Magic;
    UINT16 Version;
    UINT16 HeaderSize;
    UINT32 TotalSize;
    UINT32 Flags;
    UINT64 ProcessId;
    SOCKADDR_STORAGE OriginalLocal;
    SOCKADDR_STORAGE OriginalRemote;
    UINT32 AppIdLength;
    UINT32 Reserved;
    UCHAR AppId[ANYSIZE_ARRAY];
} NP_WFP_REDIRECT_CONTEXT_V1;

DEFINE_GUID(
    NP_WFP_PROVIDER_KEY,
    0x40485aa1, 0x1262, 0x4be1, 0x80, 0xf8, 0x57, 0x4a, 0xd4, 0xd2, 0x64, 0xe5);
DEFINE_GUID(
    NP_WFP_SUBLAYER_KEY,
    0xd8566362, 0x525d, 0x40de, 0x94, 0x6f, 0x50, 0xad, 0x72, 0x39, 0xa8, 0x0e);
DEFINE_GUID(
    NP_WFP_CALLOUT_V4_KEY,
    0x32715ea8, 0x87fd, 0x4da0, 0x8f, 0x7f, 0x2d, 0xfb, 0xb1, 0xf8, 0xdb, 0xd2);
DEFINE_GUID(
    NP_WFP_CALLOUT_V6_KEY,
    0xa9fe83c7, 0x813e, 0x4653, 0xa4, 0x4d, 0xb5, 0xa4, 0x56, 0x4f, 0xc6, 0x32);

C_ASSERT(sizeof(NP_WFP_CONFIG_V1) == 32);
C_ASSERT(sizeof(NP_WFP_STATUS_V1) == 48);
C_ASSERT(FIELD_OFFSET(NP_WFP_REDIRECT_CONTEXT_V1, AppId) == 288);
