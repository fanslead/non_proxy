#ifndef NONPROXY_MAC_HOST_BRIDGE_H
#define NONPROXY_MAC_HOST_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define NP_MAC_BRIDGE_ABI_VERSION 2u

/**
 * Swift 仅在回调执行期间借出 payload 指针；调用方必须在回调返回前复制字节。
 * payload 是 UTF-8 JSON，不包含结尾空字节，也不能由调用方释放。
 * context 的生命周期由调用方负责，并且必须持续到 completed 事件到达。
 */
typedef void (*np_mac_bridge_callback)(
    uint64_t operation_id,
    int32_t event_kind,
    int32_t status_code,
    const uint8_t *payload,
    size_t payload_length,
    void *context);

enum np_mac_bridge_event_kind {
    NP_MAC_BRIDGE_EVENT_PROGRESS = 1,
    NP_MAC_BRIDGE_EVENT_COMPLETED = 2,
};

enum np_mac_bridge_status_code {
    NP_MAC_BRIDGE_STATUS_SUCCESS = 0,
    NP_MAC_BRIDGE_STATUS_USER_APPROVAL_REQUIRED = 1,
    NP_MAC_BRIDGE_STATUS_FAILED = -1,
};

enum np_mac_bridge_start_result {
    NP_MAC_BRIDGE_START_ACCEPTED = 0,
    NP_MAC_BRIDGE_START_INVALID_ARGUMENT = -1,
    NP_MAC_BRIDGE_START_BUSY = -2,
};

uint32_t np_mac_bridge_abi_version(void);

int32_t np_mac_bridge_probe(
    uint64_t operation_id,
    np_mac_bridge_callback callback,
    void *context);

int32_t np_mac_bridge_query(
    uint64_t operation_id,
    np_mac_bridge_callback callback,
    void *context);

int32_t np_mac_bridge_install_and_enable(
    uint64_t operation_id,
    np_mac_bridge_callback callback,
    void *context);

int32_t np_mac_bridge_disable_and_uninstall(
    uint64_t operation_id,
    np_mac_bridge_callback callback,
    void *context);

#ifdef __cplusplus
}
#endif

#endif
