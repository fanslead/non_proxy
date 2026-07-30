#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "用法：$0 <NonProxy.app> <扩展二进制> <Safari Web Extension 资源>" >&2
    exit 64
fi

app_bundle=${1%/}
extension_binary=$2
resource_source=${3%/}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
packaging_root="${script_dir}/../../platform/macos/Packaging"
plist_buddy=/usr/libexec/PlistBuddy

if [[ ! -d "${app_bundle}" ||
      -L "${app_bundle}" ||
      "${app_bundle}" != *.app ||
      ! -f "${app_bundle}/Contents/Info.plist" ]]; then
    echo "Safari 扩展宿主 App Bundle 无效：${app_bundle}" >&2
    exit 65
fi
if [[ ! -x "${extension_binary}" || -L "${extension_binary}" ]]; then
    echo "Safari 扩展二进制无效：${extension_binary}" >&2
    exit 65
fi
if [[ ! -d "${resource_source}" ||
      -L "${resource_source}" ||
      ! -f "${resource_source}/manifest.json" ]]; then
    echo "Safari Web Extension 资源目录无效：${resource_source}" >&2
    exit 65
fi
if find "${resource_source}" -type l -print -quit | grep -q .; then
    echo "Safari Web Extension 资源不得包含符号链接" >&2
    exit 65
fi

plugins_root="${app_bundle}/Contents/PlugIns"
extension_bundle="${plugins_root}/NonProxySafariWebExtension.appex"
extension_contents="${extension_bundle}/Contents"
extension_resources="${extension_contents}/Resources"
extension_macos="${extension_contents}/MacOS"

if [[ -e "${plugins_root}" &&
      ( ! -d "${plugins_root}" || -L "${plugins_root}" ) ]]; then
    echo "Safari 扩展宿主 PlugIns 路径必须是普通目录" >&2
    exit 65
fi
install -d -m 0755 "${plugins_root}"
rm -rf "${extension_bundle}"
install -d -m 0755 "${extension_resources}" "${extension_macos}"
install -m 0644 \
    "${packaging_root}/SafariWebExtension.Info.plist" \
    "${extension_contents}/Info.plist"
install -m 0755 \
    "${extension_binary}" \
    "${extension_macos}/NonProxySafariWebExtension"
cp -R "${resource_source}/." "${extension_resources}/"

host_version=$(
    "${plist_buddy}" -c "Print :CFBundleShortVersionString" \
        "${app_bundle}/Contents/Info.plist"
)
host_build=$(
    "${plist_buddy}" -c "Print :CFBundleVersion" \
        "${app_bundle}/Contents/Info.plist"
)
"${plist_buddy}" -c \
    "Set :CFBundleShortVersionString ${host_version}" \
    "${extension_contents}/Info.plist"
"${plist_buddy}" -c \
    "Set :CFBundleVersion ${host_build}" \
    "${extension_contents}/Info.plist"
