#pragma once

#include <guiddef.h>
#include <devioctl.h>
#include <ws2def.h>

#define NP_WFP_CONFIG_MAGIC ((UINT32)0x4657504e)
#define NP_WFP_CONFIG_VERSION ((UINT16)3)
#define NP_WFP_CONFIG_FLAG_DNS_REDIRECT ((UINT32)0x00000001)
#define NP_WFP_CONFIG_FLAG_TCP_REDIRECT ((UINT32)0x00000002)
#define NP_WFP_CONFIG_FLAG_UDP_DIVERT ((UINT32)0x00000004)

#define NP_WFP_FILTER_CONTEXT_TCP ((UINT64)1)
#define NP_WFP_FILTER_CONTEXT_DNS ((UINT64)2)

#define NP_WFP_STATUS_MAGIC ((UINT32)0x5357504e)
#define NP_WFP_STATUS_VERSION ((UINT16)2)

#define NP_WFP_CONTEXT_MAGIC ((UINT32)0x4357504e)
#define NP_WFP_CONTEXT_VERSION ((UINT16)2)
#define NP_WFP_MAX_APP_ID_BYTES ((UINT32)4096)
#define NP_WFP_MAX_PACKAGE_SID_BYTES ((UINT32)68)

#define IOCTL_NP_WFP_APPLY_CONFIG \
    CTL_CODE(FILE_DEVICE_NETWORK, 0x801, METHOD_BUFFERED, FILE_WRITE_DATA)
#define IOCTL_NP_WFP_QUERY_STATUS \
    CTL_CODE(FILE_DEVICE_NETWORK, 0x802, METHOD_BUFFERED, FILE_READ_DATA)
#define IOCTL_NP_WFP_RECEIVE_UDP \
    CTL_CODE(FILE_DEVICE_NETWORK, 0x803, METHOD_BUFFERED, FILE_READ_DATA)
#define IOCTL_NP_WFP_INJECT_UDP \
    CTL_CODE(FILE_DEVICE_NETWORK, 0x804, METHOD_BUFFERED, FILE_WRITE_DATA)

typedef struct _NP_WFP_CONFIG_V3 {
    UINT32 Magic;
    UINT16 Version;
    UINT16 Size;
    UINT64 Generation;
    UINT64 ProxyProcessId;
    UINT16 Ipv4ProxyPortNetworkOrder;
    UINT16 Ipv6ProxyPortNetworkOrder;
    UINT16 Ipv4DnsPortNetworkOrder;
    UINT16 Ipv6DnsPortNetworkOrder;
    UINT32 Flags;
    UINT32 Reserved;
} NP_WFP_CONFIG_V3;

typedef struct _NP_WFP_STATUS_V2 {
    UINT32 Magic;
    UINT16 Version;
    UINT16 Size;
    UINT64 Generation;
    UINT64 ProxyProcessId;
    UINT32 Flags;
    UINT32 ActiveClassifications;
    UINT64 RedirectedConnections;
    UINT64 FailOpenConnections;
    UINT64 QueuedUdpDatagrams;
    UINT64 DroppedUdpDatagrams;
    UINT64 InjectedUdpDatagrams;
} NP_WFP_STATUS_V2;

typedef struct _NP_WFP_REDIRECT_CONTEXT_V2 {
    UINT32 Magic;
    UINT16 Version;
    UINT16 HeaderSize;
    UINT32 TotalSize;
    UINT32 Flags;
    UINT64 ProcessId;
    SOCKADDR_STORAGE OriginalLocal;
    SOCKADDR_STORAGE OriginalRemote;
    UINT32 AppIdLength;
    UINT32 PackageSidLength;
    UCHAR Data[ANYSIZE_ARRAY];
} NP_WFP_REDIRECT_CONTEXT_V2;

/*
 * 地址始终以网络字节序保存：IPv4 使用 Address[0..4]，其余字节为零；
 * IPv6 使用完整 16 字节。端口同样保持网络字节序。
 */
#define NP_WFP_UDP_BATCH_MAGIC ((UINT32)0x4255504e)
#define NP_WFP_UDP_DATAGRAM_MAGIC ((UINT32)0x4455504e)
#define NP_WFP_UDP_INJECT_MAGIC ((UINT32)0x4955504e)
#define NP_WFP_UDP_ABI_VERSION ((UINT16)2)
#define NP_WFP_MAX_UDP_PAYLOAD_BYTES ((UINT32)65000)
#define NP_WFP_MAX_UDP_BATCH_BYTES ((UINT32)(256 * 1024))

typedef struct _NP_WFP_UDP_BATCH_V2 {
    UINT32 Magic;
    UINT16 Version;
    UINT16 HeaderSize;
    UINT32 TotalSize;
    UINT32 DatagramCount;
    UCHAR Datagrams[ANYSIZE_ARRAY];
} NP_WFP_UDP_BATCH_V2;

typedef struct _NP_WFP_UDP_DATAGRAM_V2 {
    UINT32 Magic;
    UINT16 Version;
    UINT16 HeaderSize;
    UINT32 TotalSize;
    UINT16 AddressFamily;
    UINT16 Flags;
    UINT64 PacketId;
    UINT64 ProcessId;
    UINT32 CompartmentId;
    UINT32 InterfaceIndex;
    UINT32 SubInterfaceIndex;
    UINT16 LocalPortNetworkOrder;
    UINT16 RemotePortNetworkOrder;
    UCHAR LocalAddress[16];
    UCHAR RemoteAddress[16];
    UINT32 AppIdLength;
    UINT32 PackageSidLength;
    UINT32 PayloadLength;
    UINT32 Reserved;
    UCHAR Data[ANYSIZE_ARRAY];
} NP_WFP_UDP_DATAGRAM_V2;

typedef struct _NP_WFP_UDP_INJECT_V2 {
    UINT32 Magic;
    UINT16 Version;
    UINT16 HeaderSize;
    UINT32 TotalSize;
    UINT16 AddressFamily;
    UINT16 Flags;
    UINT64 PacketId;
    UINT32 CompartmentId;
    UINT32 InterfaceIndex;
    UINT32 SubInterfaceIndex;
    UINT16 LocalPortNetworkOrder;
    UINT16 RemotePortNetworkOrder;
    UCHAR LocalAddress[16];
    UCHAR RemoteAddress[16];
    UINT32 PayloadLength;
    UINT32 Reserved;
    UCHAR Payload[ANYSIZE_ARRAY];
} NP_WFP_UDP_INJECT_V2;

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
DEFINE_GUID(
    NP_WFP_UDP_FLOW_CALLOUT_V4_KEY,
    0x6a9c6933, 0xd8b0, 0x4cb4, 0xa9, 0xd5, 0xe1, 0x0c, 0x4f, 0xd1, 0x51, 0x70);
DEFINE_GUID(
    NP_WFP_UDP_FLOW_CALLOUT_V6_KEY,
    0x0f985ab5, 0x24c6, 0x48a1, 0x97, 0x8a, 0xd8, 0x51, 0xf9, 0x13, 0xe4, 0x21);
DEFINE_GUID(
    NP_WFP_UDP_DATAGRAM_CALLOUT_V4_KEY,
    0xc89549f7, 0x2e03, 0x4c6d, 0x88, 0xf0, 0x32, 0x17, 0x7b, 0xd7, 0xd4, 0x2b);
DEFINE_GUID(
    NP_WFP_UDP_DATAGRAM_CALLOUT_V6_KEY,
    0xf08e9b71, 0xa1cc, 0x4ab2, 0xb7, 0xe4, 0x52, 0xa8, 0xb7, 0x48, 0x96, 0x05);

C_ASSERT(sizeof(NP_WFP_CONFIG_V3) == 40);
C_ASSERT(sizeof(NP_WFP_STATUS_V2) == 72);
C_ASSERT(FIELD_OFFSET(NP_WFP_REDIRECT_CONTEXT_V2, Data) == 288);
C_ASSERT(FIELD_OFFSET(NP_WFP_UDP_BATCH_V2, Datagrams) == 16);
C_ASSERT(FIELD_OFFSET(NP_WFP_UDP_DATAGRAM_V2, Data) == 96);
C_ASSERT(FIELD_OFFSET(NP_WFP_UDP_INJECT_V2, Payload) == 80);
