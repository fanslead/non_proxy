#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "用法：$0 <NonProxy.app>" >&2
    exit 64
fi

app_bundle=${1%/}
gateway_binary="${app_bundle}/Contents/Resources/nonproxy-gatewayd"
state_directory=$(mktemp -d)
gateway_pid=

cleanup() {
    local exit_code=$?
    if [[ "${exit_code}" -ne 0 &&
          -f "${state_directory}/gateway.log" ]]; then
        cat "${state_directory}/gateway.log" >&2
    fi
    if [[ -n "${gateway_pid}" ]] &&
       kill -0 "${gateway_pid}" 2>/dev/null; then
        kill -TERM "${gateway_pid}" 2>/dev/null || true
        wait "${gateway_pid}" 2>/dev/null || true
    fi
    rm -rf "${state_directory}"
    exit "${exit_code}"
}
trap cleanup EXIT

if [[ ! -x "${gateway_binary}" || -L "${gateway_binary}" ]]; then
    echo "App Bundle 中缺少有效的 gatewayd 可执行文件" >&2
    exit 65
fi

NONPROXY_STATE_DIR="${state_directory}" \
    "${gateway_binary}" >"${state_directory}/gateway.log" 2>&1 &
gateway_pid=$!

for _attempt in {1..100}; do
    if [[ -S "${state_directory}/gatewayd.sock" &&
          -S "${state_directory}/gatewayd-flow.sock" &&
          -f "${state_directory}/session.capability" &&
          -f "${state_directory}/provider.capability" ]]; then
        break
    fi
    if ! kill -0 "${gateway_pid}" 2>/dev/null; then
        echo "App Bundle 中的 gatewayd 启动后提前退出" >&2
        exit 67
    fi
    sleep 0.05
done

for socket in gatewayd.sock gatewayd-flow.sock; do
    if [[ ! -S "${state_directory}/${socket}" ]]; then
        echo "gatewayd 未在限定时间内创建 ${socket}" >&2
        exit 67
    fi
    if [[ $(stat -f '%Lp' "${state_directory}/${socket}") != 600 ]]; then
        echo "gatewayd 套接字权限不是 0600：${socket}" >&2
        exit 67
    fi
done
for capability in session.capability provider.capability; do
    if [[ ! -f "${state_directory}/${capability}" ||
          -L "${state_directory}/${capability}" ]]; then
        echo "gatewayd 能力文件缺失或类型无效：${capability}" >&2
        exit 67
    fi
    if [[ $(wc -c <"${state_directory}/${capability}" | tr -d ' ') != 32 ||
          $(stat -f '%Lp' "${state_directory}/${capability}") != 600 ]]; then
        echo "gatewayd 能力文件长度或权限无效：${capability}" >&2
        exit 67
    fi
done

kill -TERM "${gateway_pid}"
wait "${gateway_pid}"
gateway_pid=
if [[ -e "${state_directory}/gatewayd.sock" ||
      -e "${state_directory}/gatewayd-flow.sock" ]]; then
    echo "gatewayd 收到 SIGTERM 后未清理 Unix Socket" >&2
    exit 67
fi

echo "App Bundle 内 gatewayd 已通过启动、私有运行时文件和优雅退出验证。"
