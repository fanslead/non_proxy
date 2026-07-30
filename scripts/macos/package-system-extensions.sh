#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "用法：$0 <NonProxy.app> <Debug|Release> [arm64|x86_64|universal]" >&2
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
    usage
    exit 64
fi

app_bundle=${1%/}
configuration=$2
architecture=${3:-$(uname -m)}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "${script_dir}/../.." && pwd)
package_root="${repo_root}/platform/macos"
packaging_root="${package_root}/Packaging"
plist_buddy=/usr/libexec/PlistBuddy

if [[ ! -d "${app_bundle}" || "${app_bundle}" != *.app ]]; then
    echo "macOS App Bundle 不存在或扩展名无效：${app_bundle}" >&2
    exit 65
fi
if [[ ! -f "${app_bundle}/Contents/Info.plist" ]]; then
    echo "macOS App Bundle 缺少 Contents/Info.plist" >&2
    exit 65
fi
case "${configuration}" in
    Debug)
        swift_configuration=debug
        ;;
    Release)
        swift_configuration=release
        ;;
    *)
        echo "不支持的构建配置：${configuration}" >&2
        exit 64
        ;;
esac
case "${architecture}" in
    arm64 | x86_64 | universal)
        ;;
    *)
        echo "不支持的 macOS 架构：${architecture}" >&2
        exit 64
        ;;
esac

build_product() {
    local product=$1
    swift build \
        --package-path "${package_root}" \
        --disable-sandbox \
        --configuration "${swift_configuration}" \
        "${swift_architecture_args[@]}" \
        --product "${product}"
}

bin_path_for_build() {
    swift build \
        --package-path "${package_root}" \
        --configuration "${swift_configuration}" \
        "${swift_architecture_args[@]}" \
        --show-bin-path
}

swift_architecture_args=(--arch "${architecture}")
if [[ "${architecture}" == universal ]]; then
    swift_architecture_args=(--arch arm64 --arch x86_64)
fi
build_product NonProxyTransparentSystemExtension
build_product NonProxyDNSSystemExtension
build_product NonProxyMacHostBridge
bin_path=$(bin_path_for_build)

extensions_root="${app_bundle}/Contents/Library/SystemExtensions"
frameworks_root="${app_bundle}/Contents/Frameworks"
transparent_bundle="${extensions_root}/com.nonproxy.desktop.transparent-proxy.systemextension"
dns_bundle="${extensions_root}/com.nonproxy.desktop.dns-proxy.systemextension"
bridge_library="${frameworks_root}/libNonProxyMacHostBridge.dylib"

assemble_bundle() {
    local bundle=$1
    local executable=$2
    local plist=$3

    install -d -m 0755 "${bundle}/Contents/MacOS"
    install -m 0644 "${plist}" "${bundle}/Contents/Info.plist"
    install -m 0755 "${bin_path}/${executable}" \
        "${bundle}/Contents/MacOS/${executable}"
}

rm -rf "${extensions_root}"
install -d -m 0755 "${extensions_root}"
install -d -m 0755 "${frameworks_root}"
rm -f "${bridge_library}"
install -m 0755 \
    "${bin_path}/libNonProxyMacHostBridge.dylib" \
    "${bridge_library}"
assemble_bundle \
    "${transparent_bundle}" \
    NonProxyTransparentSystemExtension \
    "${packaging_root}/TransparentProxy.Info.plist"
assemble_bundle \
    "${dns_bundle}" \
    NonProxyDNSSystemExtension \
    "${packaging_root}/DNSProxy.Info.plist"

host_version=$("${plist_buddy}" -c "Print :CFBundleShortVersionString" \
    "${app_bundle}/Contents/Info.plist")
host_build=$("${plist_buddy}" -c "Print :CFBundleVersion" \
    "${app_bundle}/Contents/Info.plist")
for bundle in "${transparent_bundle}" "${dns_bundle}"; do
    "${plist_buddy}" -c \
        "Set :CFBundleShortVersionString ${host_version}" \
        "${bundle}/Contents/Info.plist"
    "${plist_buddy}" -c \
        "Set :CFBundleVersion ${host_build}" \
        "${bundle}/Contents/Info.plist"
done

if "${plist_buddy}" -c "Print :NSSystemExtensionUsageDescription" \
    "${app_bundle}/Contents/Info.plist" >/dev/null 2>&1; then
    "${plist_buddy}" -c \
        "Set :NSSystemExtensionUsageDescription NonProxy 需要安装网络系统扩展，以便按应用和网站选择直连或指定代理。" \
        "${app_bundle}/Contents/Info.plist"
else
    "${plist_buddy}" -c \
        "Add :NSSystemExtensionUsageDescription string NonProxy 需要安装网络系统扩展，以便按应用和网站选择直连或指定代理。" \
        "${app_bundle}/Contents/Info.plist"
fi

signing_identity=${NONPROXY_CODESIGN_IDENTITY:--}
restricted_signing=${NONPROXY_RESTRICTED_SIGNING:-0}

embed_profile() {
    local source=$1
    local bundle=$2
    if [[ -z "${source}" || ! -f "${source}" ]]; then
        echo "正式签名缺少 provisioning profile：${source:-未配置}" >&2
        exit 66
    fi
    install -m 0644 "${source}" \
        "${bundle}/Contents/embedded.provisionprofile"
}

if [[ "${restricted_signing}" == 1 ]]; then
    if [[ "${signing_identity}" == - ]]; then
        echo "正式受限权限签名不能使用临时签名身份" >&2
        exit 66
    fi
    embed_profile \
        "${NONPROXY_TRANSPARENT_PROFILE:-}" \
        "${transparent_bundle}"
    embed_profile "${NONPROXY_DNS_PROFILE:-}" "${dns_bundle}"
    embed_profile "${NONPROXY_HOST_PROFILE:-}" "${app_bundle}"
fi

sign_bundle() {
    local bundle=$1
    local entitlements=$2
    local args=(
        --force
        --sign "${signing_identity}"
        --entitlements "${entitlements}"
    )
    if [[ "${signing_identity}" == - ]]; then
        args+=(--timestamp=none)
    else
        args+=(--options runtime --timestamp)
    fi
    codesign "${args[@]}" "${bundle}"
}

sign_bundle \
    "${transparent_bundle}" \
    "${packaging_root}/TransparentProxy.entitlements"
sign_bundle "${dns_bundle}" "${packaging_root}/DNSProxy.entitlements"

bridge_sign_args=(--force --sign "${signing_identity}")
if [[ "${signing_identity}" == - ]]; then
    bridge_sign_args+=(--timestamp=none)
else
    bridge_sign_args+=(--options runtime --timestamp)
fi
codesign "${bridge_sign_args[@]}" "${bridge_library}"

host_sign_args=(--force --sign "${signing_identity}")
if [[ "${restricted_signing}" == 1 ]]; then
    host_sign_args+=(
        --entitlements "${packaging_root}/Host.entitlements"
    )
fi
if [[ "${signing_identity}" == - ]]; then
    host_sign_args+=(--timestamp=none)
else
    host_sign_args+=(--options runtime --timestamp)
fi
codesign "${host_sign_args[@]}" "${app_bundle}"

"${script_dir}/verify-system-extension-bundle.sh" "${app_bundle}"
