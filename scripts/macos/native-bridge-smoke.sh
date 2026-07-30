#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "用法：$0 <NonProxy.app>" >&2
    exit 64
fi

app_bundle=${1%/}
plist="${app_bundle}/Contents/Info.plist"
if [[ ! -f "${plist}" ]]; then
    echo "待验证的 macOS App Bundle 无效：${app_bundle}" >&2
    exit 65
fi

executable=$(/usr/libexec/PlistBuddy \
    -c "Print :CFBundleExecutable" \
    "${plist}")
host_binary="${app_bundle}/Contents/MacOS/${executable}"
if [[ ! -x "${host_binary}" ]]; then
    echo "macOS App 缺少可执行宿主：${host_binary}" >&2
    exit 65
fi

output=$("${host_binary}" --native-bridge-smoke)
if ! grep -F '"abiVersion":4' <<<"${output}" >/dev/null; then
    echo "原生桥接冒烟输出缺少预期 ABI 版本：${output}" >&2
    exit 68
fi

set +e
mutation_output=$(
    "${host_binary}" --system-components-install 2>&1
)
mutation_exit_code=$?
set -e
if [[ "${mutation_exit_code}" != 64 ]] ||
   ! grep -F "NP_MAC_SYSTEM_MUTATION_NOT_CONFIRMED" \
       <<<"${mutation_output}" >/dev/null; then
    echo "系统验收命令未拒绝未经确认的网络状态变更：${mutation_output}" >&2
    exit 68
fi

query_output=$("${host_binary}" --system-components-query)
if ! grep -F '"operation":"query"' <<<"${query_output}" >/dev/null ||
   ! grep -F '"success":true' <<<"${query_output}" >/dev/null ||
   ! grep -F '"state":{' <<<"${query_output}" >/dev/null; then
    echo "只读系统组件查询未返回结构化状态：${query_output}" >&2
    exit 68
fi

echo "macOS 托管宿主已通过 C ABI、UTF-8、只读系统查询和变更确认门禁验证。"
