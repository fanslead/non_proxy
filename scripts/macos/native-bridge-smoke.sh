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

echo "macOS 托管宿主已通过 C ABI 收到并验证原生 UTF-8 响应。"
