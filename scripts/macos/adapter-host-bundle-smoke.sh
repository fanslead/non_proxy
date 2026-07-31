#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "用法：$0 <NonProxy.app>" >&2
    exit 64
fi

app_bundle=${1%/}
adapter_binary="${app_bundle}/Contents/Resources/nonproxy-adapter-host"
adapter_agent_plist="${app_bundle}/Contents/Library/LaunchAgents/com.nonproxy.adapter-host.plist"
state_root=$(mktemp -d)
state_directory="${state_root}/adapter-host"
adapter_pid=

cleanup() {
    local exit_code=$?
    if [[ "${exit_code}" -ne 0 && -f "${state_root}/adapter-host.log" ]]; then
        sed -n '1,120p' "${state_root}/adapter-host.log" >&2
    fi
    if [[ -n "${adapter_pid}" ]] && kill -0 "${adapter_pid}" 2>/dev/null; then
        kill -TERM "${adapter_pid}" 2>/dev/null || true
        wait "${adapter_pid}" 2>/dev/null || true
    fi
    rm -rf "${state_root}"
    exit "${exit_code}"
}
trap cleanup EXIT

if [[ ! -x "${adapter_binary}" || -L "${adapter_binary}" ]]; then
    echo "App Bundle 中缺少有效的 adapter-host 可执行文件" >&2
    exit 65
fi
adapter_bundle_fingerprint=$(
    /usr/libexec/PlistBuddy -c \
        "Print :EnvironmentVariables:NONPROXY_ADAPTER_BUNDLE_FINGERPRINT" \
        "${adapter_agent_plist}"
)

env \
    "NONPROXY_ADAPTER_STATE_DIR=${state_directory}" \
    "NONPROXY_ADAPTER_BUNDLE_FINGERPRINT=${adapter_bundle_fingerprint}" \
    "${adapter_binary}" >"${state_root}/adapter-host.log" 2>&1 &
adapter_pid=$!

for _attempt in {1..100}; do
    if [[ -S "${state_directory}/adapter-host.sock" &&
          -f "${state_directory}/adapter.capability" &&
          -f "${state_directory}/adapter.runtime.json" ]]; then
        break
    fi
    if ! kill -0 "${adapter_pid}" 2>/dev/null; then
        echo "App Bundle 中的 adapter-host 启动后提前退出" >&2
        exit 67
    fi
    sleep 0.05
done

adapter_socket="${state_directory}/adapter-host.sock"
if [[ ! -S "${adapter_socket}" ||
      $(stat -f '%Lp' "${adapter_socket}") != 600 ]]; then
    echo "adapter-host 未创建权限为 0600 的 Unix Socket" >&2
    exit 67
fi
capability="${state_directory}/adapter.capability"
if [[ ! -f "${capability}" || -L "${capability}" ||
      $(wc -c <"${capability}" | tr -d ' ') != 32 ||
      $(stat -f '%Lp' "${capability}") != 600 ]]; then
    echo "adapter-host 能力文件类型、长度或权限无效" >&2
    exit 67
fi
runtime_identity="${state_directory}/adapter.runtime.json"
if [[ ! -f "${runtime_identity}" || -L "${runtime_identity}" ||
      $(stat -f '%Lp' "${runtime_identity}") != 600 ]]; then
    echo "adapter-host 运行身份缺失、类型无效或权限不是 0600" >&2
    exit 67
fi
runtime_schema=$(plutil -extract schemaVersion raw -o - "${runtime_identity}")
runtime_fingerprint=$(
    plutil -extract bundleFingerprint raw -o - "${runtime_identity}"
)
runtime_pid=$(plutil -extract processId raw -o - "${runtime_identity}")
if [[ "${runtime_schema}" != 1 ||
      "${runtime_fingerprint}" != "${adapter_bundle_fingerprint}" ||
      "${runtime_pid}" != "${adapter_pid}" ]]; then
    echo "adapter-host 运行身份与 LaunchAgent 包指纹或进程不一致" >&2
    exit 67
fi

kill -TERM "${adapter_pid}"
wait "${adapter_pid}"
adapter_pid=
if [[ -e "${adapter_socket}" || -e "${runtime_identity}" ]]; then
    echo "adapter-host 收到 SIGTERM 后未清理 Unix Socket 或运行身份" >&2
    exit 67
fi

echo "App Bundle 内 adapter-host 已通过签名包身份、私有运行时文件和优雅退出验证。"
