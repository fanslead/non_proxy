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

build_gateway() {
    local rust_profile_directory=debug
    local gateway_target_root="${repo_root}/target"
    if [[ "${configuration}" == Release ]]; then
        rust_profile_directory=release
    fi

    build_gateway_target() {
        local target=$1
        if [[ "${configuration}" == Release ]]; then
            CARGO_TARGET_DIR="${gateway_target_root}" cargo build \
                --manifest-path "${repo_root}/Cargo.toml" \
                -p nonproxy-gatewayd \
                --release \
                --target "${target}"
        else
            CARGO_TARGET_DIR="${gateway_target_root}" cargo build \
                --manifest-path "${repo_root}/Cargo.toml" \
                -p nonproxy-gatewayd \
                --target "${target}"
        fi
    }

    if [[ "${architecture}" == universal ]]; then
        build_gateway_target aarch64-apple-darwin
        build_gateway_target x86_64-apple-darwin
        gateway_temporary_directory=$(mktemp -d)
        gateway_source="${gateway_temporary_directory}/nonproxy-gatewayd"
        lipo -create \
            "${gateway_target_root}/aarch64-apple-darwin/${rust_profile_directory}/nonproxy-gatewayd" \
            "${gateway_target_root}/x86_64-apple-darwin/${rust_profile_directory}/nonproxy-gatewayd" \
            -output "${gateway_source}"
        return
    fi

    local rust_target
    case "${architecture}" in
        arm64)
            rust_target=aarch64-apple-darwin
            ;;
        x86_64)
            rust_target=x86_64-apple-darwin
            ;;
    esac
    build_gateway_target "${rust_target}"
    gateway_source="${gateway_target_root}/${rust_target}/${rust_profile_directory}/nonproxy-gatewayd"
}

bin_path_for_build() {
    swift build \
        --package-path "${package_root}" \
        --configuration "${swift_configuration}" \
        "${swift_architecture_args[@]}" \
        --show-bin-path
}

build_browser_extensions() {
    (
        cd "${repo_root}"
        pnpm --filter @nonproxy/browser-extension build
    )
}

swift_architecture_args=(--arch "${architecture}")
if [[ "${architecture}" == universal ]]; then
    swift_architecture_args=(--arch arm64 --arch x86_64)
fi
build_product NonProxyTransparentSystemExtension
build_product NonProxyDNSSystemExtension
build_product NonProxyMacHostBridge
build_product NonProxyNativeMessagingHost
build_browser_extensions
gateway_temporary_directory=
gateway_source=
cleanup_gateway_build() {
    if [[ -n "${gateway_temporary_directory}" &&
          -d "${gateway_temporary_directory}" ]]; then
        rm -rf "${gateway_temporary_directory}"
    fi
}
trap cleanup_gateway_build EXIT
build_gateway
bin_path=$(bin_path_for_build)

extensions_root="${app_bundle}/Contents/Library/SystemExtensions"
frameworks_root="${app_bundle}/Contents/Frameworks"
launch_agents_root="${app_bundle}/Contents/Library/LaunchAgents"
resources_root="${app_bundle}/Contents/Resources"
transparent_bundle="${extensions_root}/com.nonproxy.desktop.transparent-proxy.systemextension"
dns_bundle="${extensions_root}/com.nonproxy.desktop.dns-proxy.systemextension"
bridge_library="${frameworks_root}/libNonProxyMacHostBridge.dylib"
gateway_agent_plist="${launch_agents_root}/com.nonproxy.gatewayd.plist"
gateway_binary="${resources_root}/nonproxy-gatewayd"
native_messaging_host="${resources_root}/nonproxy-native-messaging-host"
browser_extensions_source="${repo_root}/packages/browser-extension/dist"
browser_extensions_root="${resources_root}/BrowserExtensions"

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
rm -rf "${launch_agents_root}"
rm -rf "${browser_extensions_root}"
install -d -m 0755 "${extensions_root}"
install -d -m 0755 "${frameworks_root}"
install -d -m 0755 "${launch_agents_root}" "${resources_root}"
rm -f "${bridge_library}"
install -m 0755 \
    "${bin_path}/libNonProxyMacHostBridge.dylib" \
    "${bridge_library}"
rm -f \
    "${gateway_agent_plist}" \
    "${gateway_binary}" \
    "${native_messaging_host}"
install -m 0644 \
    "${packaging_root}/com.nonproxy.gatewayd.plist" \
    "${gateway_agent_plist}"
install -m 0755 "${gateway_source}" "${gateway_binary}"
install -m 0755 \
    "${bin_path}/NonProxyNativeMessagingHost" \
    "${native_messaging_host}"
install -d -m 0755 "${browser_extensions_root}"
cp -R \
    "${browser_extensions_source}/chromium" \
    "${browser_extensions_source}/safari" \
    "${browser_extensions_root}/"
install -m 0644 \
    "${browser_extensions_source}/BUILD_INFO.json" \
    "${browser_extensions_root}/BUILD_INFO.json"
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
codesign "${bridge_sign_args[@]}" "${gateway_binary}"
codesign "${bridge_sign_args[@]}" "${native_messaging_host}"

gateway_binary_digest=$(shasum -a 256 "${gateway_binary}" | awk '{print $1}')
gateway_plist_digest=$(
    shasum -a 256 "${packaging_root}/com.nonproxy.gatewayd.plist" |
        awk '{print $1}'
)
gateway_bundle_fingerprint=$(
    printf '%s%s' "${gateway_binary_digest}" "${gateway_plist_digest}" |
        shasum -a 256 |
        awk '{print $1}'
)
"${plist_buddy}" -c "Add :EnvironmentVariables dict" \
    "${gateway_agent_plist}"
"${plist_buddy}" -c \
    "Add :EnvironmentVariables:NONPROXY_GATEWAY_BUNDLE_FINGERPRINT string ${gateway_bundle_fingerprint}" \
    "${gateway_agent_plist}"

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
